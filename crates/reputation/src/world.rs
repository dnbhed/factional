use std::collections::BTreeMap;

use factional_core::{Fixed, Tick, suggest};

use crate::{
    Action, ActionId, Alignment, Change, Character, CharacterId, Command, CommandError, Event,
    JournalEntry, Role, Witnesses,
};

/// World-wide rules and defaults from `balance.toml` (DESIGN.md §12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    /// An axis at or beyond ±this reads as Lawful/Chaotic or Good/Evil (DESIGN.md §5.1).
    pub label_threshold: Fixed,
}

impl Balance {
    /// `alignment.label_threshold`'s default: 33.00.
    pub const DEFAULT_LABEL_THRESHOLD: Fixed = Fixed::from_hundredths(33_00);
}

impl Default for Balance {
    fn default() -> Balance {
        Balance {
            label_threshold: Balance::DEFAULT_LABEL_THRESHOLD,
        }
    }
}

/// Validated content: everything a world starts from. `factional-content` builds it from a
/// directory of TOML files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    pub balance: Balance,
    pub characters: BTreeMap<CharacterId, Character>,
    /// The action catalogue.
    pub actions: BTreeMap<ActionId, Action>,
}

/// The reputation module's world: the content it started from, plus everything that has
/// happened since (DESIGN.md §2).
///
/// State changes only through [`World::execute`]. A command is either refused, changing
/// nothing, or accepted, producing events. Applying events is the only thing that changes
/// state, so replaying the event log rebuilds the state exactly (P-14, P-16).
#[derive(Debug, Clone)]
pub struct World {
    content: Content,
    state: State,
    events: Vec<Event>,
    journal: Vec<JournalEntry>,
}

/// Everything that changes during play. Events are the only thing that changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    now: Tick,
    /// Every character's alignment now.
    alignments: BTreeMap<CharacterId, Alignment>,
}

impl State {
    /// The state before anything has happened.
    fn initial(content: &Content) -> State {
        State {
            now: Tick::default(),
            alignments: content
                .characters
                .values()
                .map(|character| (character.id.clone(), character.alignment))
                .collect(),
        }
    }
}

impl World {
    /// A world at tick 0, with nothing yet happened.
    pub fn new(content: Content) -> World {
        World {
            state: State::initial(&content),
            content,
            events: Vec::new(),
            journal: Vec::new(),
        }
    }

    /// Rebuilds a world from its content and its event log, without running any rules: what
    /// saves are built on (P-16).
    pub fn replay(content: Content, events: &[Event]) -> World {
        let mut world = World::new(content);
        for event in events {
            world.apply(event.clone());
        }
        world
    }

    /// Runs a command. Refused: nothing changes, and the error says why. Accepted: the events
    /// it produced, already applied. Either way the journal records it.
    pub fn execute(&mut self, command: Command) -> Result<Vec<Event>, CommandError> {
        let decided = self.decide(&command);
        self.journal.push(JournalEntry {
            command,
            result: decided.as_ref().map(|_| ()).map_err(Clone::clone),
        });
        let mut emitted = Vec::new();
        for change in decided? {
            let event = Event {
                seq: self.events.len() as u64 + 1,
                tick: self.state.now,
                payload: change,
            };
            self.apply(event.clone());
            emitted.push(event);
        }
        Ok(emitted)
    }

    /// Works out what a command would change, without changing anything: the rules live
    /// here, and only here.
    fn decide(&self, command: &Command) -> Result<Vec<Change>, CommandError> {
        match command {
            &Command::AdvanceTime { ticks } => {
                if ticks == 0 {
                    return Err(CommandError::NoTicks);
                }
                let now = self.state.now;
                let to = now
                    .0
                    .checked_add(ticks)
                    .ok_or(CommandError::TimeOverflow { now, ticks })?;
                Ok(vec![Change::TimeAdvanced {
                    from: now,
                    to: Tick(to),
                }])
            }
            Command::PerformAction {
                actor,
                action,
                target,
                scale,
                witnesses,
            } => {
                let from = self.existing(actor, Role::Actor)?;
                let catalogued = self.content.actions.get(action).ok_or_else(|| {
                    CommandError::UnknownAction {
                        action: action.clone(),
                        suggestion: closest(action.as_str(), self.content.actions.keys()),
                    }
                })?;
                if let Some(target) = target {
                    self.existing(target, Role::Target)?;
                    if target == actor {
                        return Err(CommandError::TargetIsActor);
                    }
                }
                if *scale <= Fixed::ZERO {
                    return Err(CommandError::ScaleNotPositive { scale: *scale });
                }
                if let Witnesses::These(witnesses) = witnesses {
                    for witness in witnesses {
                        self.existing(witness, Role::Witness)?;
                    }
                }
                let mut changes = vec![Change::ActionPerformed {
                    actor: actor.clone(),
                    action: action.clone(),
                    target: target.clone(),
                    scale: *scale,
                    witnesses: witnesses.clone(),
                }];
                let to = from.shifted(catalogued.alignment, *scale);
                if to != from {
                    changes.push(Change::AlignmentChanged {
                        character: actor.clone(),
                        from,
                        to,
                    });
                }
                Ok(changes)
            }
        }
    }

    /// A character's alignment now, or a refusal naming them in their `role`.
    fn existing(&self, id: &CharacterId, role: Role) -> Result<Alignment, CommandError> {
        self.state
            .alignments
            .get(id)
            .copied()
            .ok_or_else(|| CommandError::UnknownCharacter {
                role,
                id: id.clone(),
                suggestion: closest(id.as_str(), self.state.alignments.keys()),
            })
    }

    /// Records an event and makes its change: the only place state changes. No rules run
    /// here, so replaying events needs none (P-15).
    fn apply(&mut self, event: Event) {
        match event.payload {
            Change::TimeAdvanced { to, .. } => self.state.now = to,
            Change::ActionPerformed { .. } => {}
            Change::AlignmentChanged {
                ref character, to, ..
            } => {
                self.state.alignments.insert(character.clone(), to);
            }
        }
        self.events.push(event);
    }

    /// The current tick.
    pub fn now(&self) -> Tick {
        self.state.now
    }

    /// Every event so far, oldest first.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// The events after `seq`, oldest first.
    pub fn events_since(&self, seq: u64) -> &[Event] {
        // Events are numbered from 1 with no gaps, so event `seq` sits at index `seq - 1`.
        let start =
            usize::try_from(seq).map_or(self.events.len(), |seq| seq.min(self.events.len()));
        &self.events[start..]
    }

    /// Every command issued, in order, and whether it was accepted.
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    pub fn balance(&self) -> &Balance {
        &self.content.balance
    }

    /// Every character, in id order.
    pub fn characters(&self) -> impl Iterator<Item = &Character> {
        self.content.characters.values()
    }

    pub fn character(&self, id: &CharacterId) -> Option<&Character> {
        self.content.characters.get(id)
    }

    /// A character's alignment now.
    pub fn alignment(&self, id: &CharacterId) -> Option<Alignment> {
        self.state.alignments.get(id).copied()
    }

    /// The action catalogue, in id order.
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.content.actions.values()
    }
}

/// The id among `ids` closest to a misspelt `word`, if one is close enough to suggest.
fn closest<'a, Id>(word: &str, mut ids: impl Iterator<Item = &'a Id> + Clone) -> Option<Id>
where
    Id: Clone + AsRef<str> + 'a,
{
    let close = suggest(word, ids.clone().map(AsRef::as_ref))?;
    ids.find(|id| id.as_ref() == close).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AXIS_LIMIT, AlignmentDelta, Role, Witnesses};

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn id(text: &str) -> CharacterId {
        CharacterId::new(text).expect("a valid id")
    }

    fn character(name: &str, law: i64, good: i64) -> Character {
        Character {
            id: id(&name.to_lowercase()),
            name: name.to_owned(),
            alignment: Alignment::new(h(law), h(good)).expect("in range"),
        }
    }

    fn action_id(text: &str) -> ActionId {
        ActionId::new(text).expect("a valid id")
    }

    fn action(id: &str, law: i64, good: i64) -> Action {
        Action {
            id: action_id(id),
            alignment: AlignmentDelta {
                law: h(law),
                good: h(good),
            },
        }
    }

    /// A world with these characters and Riverhold's `steal` and `help_stranger`.
    fn world_of(characters: impl IntoIterator<Item = Character>) -> World {
        let actions = [
            action("steal", -5_00, -3_00),
            action("help_stranger", 0, 4_00),
        ];
        World::new(Content {
            balance: Balance::default(),
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
            actions: actions.into_iter().map(|a| (a.id.clone(), a)).collect(),
        })
    }

    fn riverhold() -> World {
        world_of([
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
            character("Player", 0, 0),
        ])
    }

    #[test]
    fn the_default_label_threshold_is_33() {
        assert_eq!(Balance::default().label_threshold, h(33_00));
    }

    #[test]
    fn looks_up_a_characters_alignment() {
        let world = riverhold();
        assert_eq!(
            world.alignment(&id("vex")),
            Some(Alignment::new(h(-55_00), h(-20_00)).expect("in range"))
        );
        assert_eq!(world.alignment(&id("nobody")), None);
        assert_eq!(
            world.character(&id("ava")).map(|c| c.name.as_str()),
            Some("Ava")
        );
    }

    #[test]
    fn lists_characters_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["ava", "player", "vex"]);
    }

    #[test]
    fn keeps_the_balance_it_was_given() {
        let world = World::new(Content {
            balance: Balance {
                label_threshold: h(40_00),
            },
            ..Content::default()
        });
        assert_eq!(world.balance().label_threshold, h(40_00));
    }

    // Commands, events and time

    fn advance(ticks: u64) -> Command {
        Command::AdvanceTime { ticks }
    }

    fn time_advanced(seq: u64, from: u64, to: u64) -> Event {
        Event {
            seq,
            tick: Tick(from),
            payload: Change::TimeAdvanced {
                from: Tick(from),
                to: Tick(to),
            },
        }
    }

    #[test]
    fn a_new_world_is_at_tick_zero_with_nothing_happened() {
        let world = riverhold();
        assert_eq!(world.now(), Tick(0));
        assert!(world.events().is_empty());
        assert!(world.journal().is_empty());
    }

    #[test]
    fn advancing_time_moves_the_clock_and_emits_one_event() {
        let mut world = riverhold();
        assert_eq!(world.execute(advance(5)), Ok(vec![time_advanced(1, 0, 5)]));
        assert_eq!(world.now(), Tick(5));
        assert_eq!(world.events(), [time_advanced(1, 0, 5)]);
    }

    #[test]
    fn events_are_numbered_and_stamped_across_commands() {
        let mut world = riverhold();
        world.execute(advance(5)).expect("accepted");
        assert_eq!(world.execute(advance(3)), Ok(vec![time_advanced(2, 5, 8)]));
        assert_eq!(
            world.events(),
            [time_advanced(1, 0, 5), time_advanced(2, 5, 8)]
        );
    }

    #[test]
    fn advancing_by_zero_is_refused_and_changes_nothing() {
        let mut world = riverhold();
        world.execute(advance(5)).expect("accepted");
        assert_eq!(world.execute(advance(0)), Err(CommandError::NoTicks));
        assert_eq!(world.now(), Tick(5));
        assert_eq!(world.events().len(), 1);
    }

    #[test]
    fn time_cannot_pass_the_last_tick() {
        let mut world = riverhold();
        world.execute(advance(u64::MAX)).expect("accepted");
        assert_eq!(
            world.execute(advance(1)),
            Err(CommandError::TimeOverflow {
                now: Tick(u64::MAX),
                ticks: 1,
            })
        );
        assert_eq!(world.now(), Tick(u64::MAX));
    }

    #[test]
    fn the_journal_records_every_command_and_its_result() {
        let mut world = riverhold();
        for ticks in [5, 0, 2] {
            let _ = world.execute(advance(ticks));
        }
        assert_eq!(
            world.journal(),
            [
                JournalEntry {
                    command: advance(5),
                    result: Ok(()),
                },
                JournalEntry {
                    command: advance(0),
                    result: Err(CommandError::NoTicks),
                },
                JournalEntry {
                    command: advance(2),
                    result: Ok(()),
                },
            ]
        );
    }

    #[test]
    fn events_since_gives_the_events_after_a_sequence_number() {
        let mut world = riverhold();
        for ticks in [1, 2, 3] {
            world.execute(advance(ticks)).expect("accepted");
        }
        assert_eq!(
            world.events_since(1),
            [time_advanced(2, 1, 3), time_advanced(3, 3, 6)]
        );
        assert_eq!(world.events_since(0).len(), 3);
        assert!(world.events_since(3).is_empty());
        assert!(world.events_since(99).is_empty());
    }

    #[test]
    fn replaying_the_event_log_rebuilds_the_world() {
        let mut world = riverhold();
        for ticks in [4, 6] {
            world.execute(advance(ticks)).expect("accepted");
        }
        let replayed = World::replay(world.content.clone(), world.events());
        assert_eq!(replayed.now(), Tick(10));
        assert_eq!(replayed.events(), world.events());
        assert!(replayed.journal().is_empty());
    }

    // Actions (DESIGN.md §5.2)

    fn aligned(law: i64, good: i64) -> Alignment {
        Alignment::new(h(law), h(good)).expect("in range")
    }

    fn act(actor: &str, action: &str, target: Option<&str>, scale: i64) -> Command {
        Command::PerformAction {
            actor: id(actor),
            action: action_id(action),
            target: target.map(id),
            scale: h(scale),
            witnesses: Witnesses::Everyone,
        }
    }

    fn performed(seq: u64, actor: &str, action: &str, target: Option<&str>, scale: i64) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::ActionPerformed {
                actor: id(actor),
                action: action_id(action),
                target: target.map(id),
                scale: h(scale),
                witnesses: Witnesses::Everyone,
            },
        }
    }

    fn alignment_changed(seq: u64, character: &str, from: Alignment, to: Alignment) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::AlignmentChanged {
                character: id(character),
                from,
                to,
            },
        }
    }

    #[test]
    fn stealing_moves_the_thief_toward_chaotic_evil() {
        let mut world = riverhold();
        let steal = act("player", "steal", Some("ava"), 1_00);
        assert_eq!(
            world.execute(steal),
            Ok(vec![
                performed(1, "player", "steal", Some("ava"), 1_00),
                alignment_changed(2, "player", aligned(0, 0), aligned(-5_00, -3_00)),
            ])
        );
        assert_eq!(world.alignment(&id("player")), Some(aligned(-5_00, -3_00)));
        assert_eq!(world.alignment(&id("ava")), Some(aligned(20_00, 10_00)));
    }

    #[test]
    fn a_characters_starting_alignment_stays_as_content_gave_it() {
        let mut world = riverhold();
        world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        let player = world.character(&id("player")).expect("exists");
        assert_eq!(player.alignment, aligned(0, 0));
    }

    #[test]
    fn scale_says_how_big_the_act_was() {
        let mut world = riverhold();
        let events = world
            .execute(act("player", "steal", Some("ava"), 2_00))
            .expect("accepted");
        assert_eq!(
            events[1],
            alignment_changed(2, "player", aligned(0, 0), aligned(-10_00, -6_00))
        );
    }

    #[test]
    fn an_action_needs_no_target() {
        let mut world = riverhold();
        assert_eq!(
            world.execute(act("vex", "help_stranger", None, 1_00)),
            Ok(vec![
                performed(1, "vex", "help_stranger", None, 1_00),
                alignment_changed(2, "vex", aligned(-55_00, -20_00), aligned(-55_00, -16_00)),
            ])
        );
    }

    #[test]
    fn alignment_clamps_at_the_end_of_an_axis() {
        let mut world = world_of([character("Player", -98_00, 0)]);
        let events = world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        assert_eq!(
            events[1],
            alignment_changed(2, "player", aligned(-98_00, 0), aligned(-100_00, -3_00))
        );
    }

    #[test]
    fn an_act_that_cannot_move_alignment_emits_no_alignment_change() {
        let mut world = world_of([character("Player", -100_00, -100_00)]);
        assert_eq!(
            world.execute(act("player", "steal", None, 1_00)),
            Ok(vec![performed(1, "player", "steal", None, 1_00)])
        );
    }

    #[test]
    fn action_events_are_stamped_with_the_current_tick() {
        let mut world = riverhold();
        world.execute(advance(7)).expect("accepted");
        let events = world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        let stamps: Vec<(u64, Tick)> = events.iter().map(|e| (e.seq, e.tick)).collect();
        assert_eq!(stamps, [(2, Tick(7)), (3, Tick(7))]);
    }

    #[test]
    fn the_act_records_who_witnessed_it() {
        let mut world = riverhold();
        let witnesses = Witnesses::These([id("vex")].into());
        let command = Command::PerformAction {
            actor: id("player"),
            action: action_id("steal"),
            target: None,
            scale: h(1_00),
            witnesses: witnesses.clone(),
        };
        let events = world.execute(command).expect("accepted");
        let Change::ActionPerformed {
            witnesses: seen, ..
        } = &events[0].payload
        else {
            panic!("expected ActionPerformed first, got {:?}", events[0]);
        };
        assert_eq!(seen, &witnesses);
    }

    /// Runs a command that must be refused and checks that nothing changed but the journal.
    fn refused(world: &mut World, command: Command) -> CommandError {
        let (state, events) = (world.state.clone(), world.events.clone());
        let error = world.execute(command).expect_err("refused");
        assert_eq!(world.state, state);
        assert_eq!(world.events, events);
        assert_eq!(
            world.journal().last().map(|entry| entry.result.clone()),
            Some(Err(error.clone()))
        );
        error
    }

    #[test]
    fn an_unknown_action_is_refused_with_a_suggestion() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("player", "stael", None, 1_00)),
            CommandError::UnknownAction {
                action: action_id("stael"),
                suggestion: Some(action_id("steal")),
            }
        );
        assert_eq!(
            refused(&mut world, act("player", "dance", None, 1_00)),
            CommandError::UnknownAction {
                action: action_id("dance"),
                suggestion: None,
            }
        );
    }

    #[test]
    fn unknown_characters_are_refused_by_role() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("plyer", "steal", None, 1_00)),
            CommandError::UnknownCharacter {
                role: Role::Actor,
                id: id("plyer"),
                suggestion: Some(id("player")),
            }
        );
        assert_eq!(
            refused(&mut world, act("player", "steal", Some("merchant"), 1_00)),
            CommandError::UnknownCharacter {
                role: Role::Target,
                id: id("merchant"),
                suggestion: None,
            }
        );
        let command = Command::PerformAction {
            actor: id("player"),
            action: action_id("steal"),
            target: None,
            scale: h(1_00),
            witnesses: Witnesses::These([id("vex"), id("vx_ghost"), id("avx")].into()),
        };
        assert_eq!(
            refused(&mut world, command),
            CommandError::UnknownCharacter {
                role: Role::Witness,
                id: id("avx"),
                suggestion: Some(id("ava")),
            },
            "the first unknown witness, in id order"
        );
    }

    #[test]
    fn a_scale_of_zero_or_less_is_refused() {
        let mut world = riverhold();
        for scale in [0, -1_00] {
            assert_eq!(
                refused(&mut world, act("player", "steal", None, scale)),
                CommandError::ScaleNotPositive { scale: h(scale) }
            );
        }
        assert!(world.execute(act("player", "steal", None, 1)).is_ok());
    }

    #[test]
    fn an_actor_cannot_target_themselves() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("player", "steal", Some("player"), 1_00)),
            CommandError::TargetIsActor
        );
    }

    #[test]
    fn lists_the_action_catalogue_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.actions().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["help_stranger", "steal"]);
    }

    #[test]
    fn replaying_rebuilds_alignments() {
        let mut world = riverhold();
        world
            .execute(act("player", "steal", Some("ava"), 3_00))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events());
        assert_eq!(
            replayed.alignment(&id("player")),
            Some(aligned(-15_00, -9_00))
        );
    }

    // Properties (DESIGN.md §14, invariants 1–4)

    use proptest::prelude::*;

    /// Some commands, refused ones included: zero ticks, unknown characters or actions,
    /// self-targets and scales of 0.
    fn commands() -> impl Strategy<Value = Vec<Command>> {
        let who = || prop_oneof![Just("player"), Just("vex"), Just("ava"), Just("ghost")];
        let action = (
            who(),
            prop_oneof![Just("steal"), Just("help_stranger"), Just("dance")],
            proptest::option::of(who()),
            prop_oneof![0_i64..=500, Just(i64::MAX)],
        )
            .prop_map(|(actor, action, target, scale)| act(actor, action, target, scale));
        let command = prop_oneof![(0_u64..=1_000).prop_map(advance), action];
        proptest::collection::vec(command, 0..30)
    }

    fn run(commands: &[Command]) -> World {
        let mut world = riverhold();
        for command in commands {
            let _ = world.execute(command.clone());
        }
        world
    }

    proptest! {
        #[test]
        fn replaying_events_reproduces_the_state(commands in commands()) {
            let world = run(&commands);
            let replayed = World::replay(world.content.clone(), world.events());
            prop_assert_eq!(&replayed.state, &world.state);
            prop_assert_eq!(replayed.events(), world.events());
        }

        #[test]
        fn re_executing_the_journal_reproduces_the_events_exactly(commands in commands()) {
            let world = run(&commands);
            let journal: Vec<Command> =
                world.journal().iter().map(|entry| entry.command.clone()).collect();
            let rerun = run(&journal);
            prop_assert_eq!(rerun.events(), world.events());
            prop_assert_eq!(rerun.journal(), world.journal());
        }

        #[test]
        fn a_refused_command_leaves_the_state_exactly_as_it_was(commands in commands()) {
            let mut world = run(&commands);
            let (state, events) = (world.state.clone(), world.events.clone());
            prop_assert!(world.execute(advance(0)).is_err());
            prop_assert_eq!(&world.state, &state);
            prop_assert_eq!(&world.events, &events);
        }

        #[test]
        fn the_same_commands_always_give_the_same_events(commands in commands()) {
            let (first, second) = (run(&commands), run(&commands));
            prop_assert_eq!(first.events(), second.events());
        }

        #[test]
        fn time_only_moves_forward_and_events_count_up_from_one(commands in commands()) {
            let world = run(&commands);
            for (index, event) in world.events().iter().enumerate() {
                prop_assert_eq!(event.seq, index as u64 + 1);
            }
            let total: u64 = commands
                .iter()
                .filter_map(|command| match command {
                    Command::AdvanceTime { ticks } => Some(*ticks),
                    Command::PerformAction { .. } => None,
                })
                .sum();
            prop_assert_eq!(world.now(), Tick(total));
        }

        #[test]
        fn alignments_always_stay_within_the_axes(commands in commands()) {
            let world = run(&commands);
            for character in world.characters() {
                let alignment = world.alignment(&character.id).expect("every character has one");
                for value in [alignment.law(), alignment.good()] {
                    prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&value));
                }
            }
        }

        #[test]
        fn every_alignment_change_follows_the_act_that_caused_it(commands in commands()) {
            let world = run(&commands);
            for pair in world.events().windows(2) {
                if let Change::AlignmentChanged { character, from, to } = &pair[1].payload {
                    prop_assert_ne!(from, to);
                    let Change::ActionPerformed { actor, .. } = &pair[0].payload else {
                        return Err(TestCaseError::fail("an alignment change without an act"));
                    };
                    prop_assert_eq!(actor, character);
                }
            }
        }
    }
}

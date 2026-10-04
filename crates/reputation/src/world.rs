use std::collections::BTreeMap;

use factional_core::{Fixed, Tick};

use crate::{
    Alignment, Change, Character, CharacterId, Command, CommandError, Event, JournalEntry,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct State {
    now: Tick,
}

impl World {
    /// A world at tick 0, with nothing yet happened.
    pub fn new(content: Content) -> World {
        World {
            content,
            state: State::default(),
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
        match *command {
            Command::AdvanceTime { ticks } => {
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
        }
    }

    /// Records an event and makes its change: the only place state changes. No rules run
    /// here, so replaying events needs none (P-15).
    fn apply(&mut self, event: Event) {
        match event.payload {
            Change::TimeAdvanced { to, .. } => self.state.now = to,
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

    pub fn alignment(&self, id: &CharacterId) -> Option<Alignment> {
        self.character(id).map(|character| character.alignment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn riverhold() -> World {
        let characters = [
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
        ];
        World::new(Content {
            balance: Balance::default(),
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
        })
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
        assert_eq!(ids, ["ava", "vex"]);
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

    // Properties (DESIGN.md §14, invariants 2–4)

    use proptest::prelude::*;

    /// Some commands, refused ones (zero ticks) included.
    fn commands() -> impl Strategy<Value = Vec<Command>> {
        proptest::collection::vec((0_u64..=1_000).prop_map(advance), 0..20)
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
                .map(|Command::AdvanceTime { ticks }| *ticks)
                .sum();
            prop_assert_eq!(world.now(), Tick(total));
        }
    }
}

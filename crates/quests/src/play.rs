//! Playing quests (Q6, DESIGN.md §17): each character's progress through quests and
//! questlines, kept by the quest log. It changes only by its commands, and only by applying
//! the events they produce; a choice's effects go to the reputation module as one of its
//! commands, in the same step, so a refused command changes nothing in either.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use factional_core::{Fixed, suggest};
use factional_reputation::{
    CharacterId, Command, CommandError, Event, FactionId, Observer, Party, RankId, Witnesses, World,
};

use crate::quest::{
    ChoiceEffects, ChoiceId, Leftovers, Next, PartyRef, Progress, Quest, QuestId, QuestlineId,
    Quests, Requirements, StageId,
};

/// A change to a character's quest progress (P-72).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestCommand {
    /// `character` starts `quest`, if its gate and its step allow (DESIGN.md §17.1).
    StartQuest {
        character: CharacterId,
        quest: QuestId,
    },
    /// `character` makes `choice` at the stage of `quest` they're at, if its requirements
    /// hold; its effects are applied with `witnesses` (K3).
    MakeChoice {
        character: CharacterId,
        quest: QuestId,
        choice: ChoiceId,
        witnesses: Witnesses,
    },
}

/// What happened to a character's quest progress.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestEvent {
    QuestStarted {
        character: CharacterId,
        quest: QuestId,
    },
    /// Reached, whether or not its requirements hold yet: the character waits there until
    /// they do.
    StageReached {
        character: CharacterId,
        quest: QuestId,
        stage: StageId,
    },
    ChoiceMade {
        character: CharacterId,
        quest: QuestId,
        stage: StageId,
        choice: ChoiceId,
    },
    QuestFinished {
        character: CharacterId,
        quest: QuestId,
    },
    /// A leftover of `step` (from 0) of `questline`, closed because the character started a
    /// quest of a later step (P-69).
    QuestClosed {
        character: CharacterId,
        quest: QuestId,
        questline: QuestlineId,
        step: usize,
    },
}

/// One thing a quest command did, in order: to the quest log, or, through its effects, to the
/// world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Played {
    Quest(QuestEvent),
    World(Event),
}

/// Where a character is in one quest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestState {
    /// Under way, at `stage` (its place, from 0).
    Active {
        stage: usize,
    },
    Finished,
    /// Closed before it started, when its questline moved on from `step` (from 0).
    Closed {
        questline: QuestlineId,
        step: usize,
    },
}

/// One character's progress: each quest they've started or had closed, and every stage they
/// reached and choice they made, for `done` requirements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub states: BTreeMap<QuestId, QuestState>,
    pub reached: BTreeSet<(QuestId, StageId)>,
    pub chosen: BTreeSet<(QuestId, StageId, ChoiceId)>,
}

/// A requirement that doesn't hold, with what it needs and what the character has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmet {
    Standing {
        party: Party,
        needs: Fixed,
        has: Fixed,
    },
    Member(FactionId),
    NotMember(FactionId),
    Rank {
        faction: FactionId,
        needs: RankId,
        has: Option<RankId>,
    },
    /// Further from `faction`, as it pictures them, than its member tolerance.
    Tolerance {
        faction: FactionId,
        distance: Fixed,
        tolerance: Fixed,
    },
    Done(Progress),
}

/// Why a character can't start a quest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartBlock {
    AlreadyStarted,
    AlreadyFinished,
    Closed {
        questline: QuestlineId,
        step: usize,
    },
    /// The earliest step before the quest's own that isn't complete: `done` of the `need`
    /// quests it needs.
    StepIncomplete {
        questline: QuestlineId,
        step: usize,
        done: usize,
        need: usize,
    },
    Unmet(Unmet),
}

/// Whether a character can start a quest, and every reason they can't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartAssessment {
    pub character: CharacterId,
    pub quest: QuestId,
    pub blocks: Vec<StartBlock>,
}

impl StartAssessment {
    pub fn allowed(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// Why a quest command was refused. A refused command changes nothing, in the quest log or
/// the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestError {
    UnknownCharacter {
        character: CharacterId,
        suggestion: Option<CharacterId>,
    },
    UnknownQuest {
        quest: QuestId,
        suggestion: Option<QuestId>,
    },
    UnknownWitness {
        witness: CharacterId,
        suggestion: Option<CharacterId>,
    },
    CannotStart(StartAssessment),
    NotStarted {
        character: CharacterId,
        quest: QuestId,
    },
    Finished {
        character: CharacterId,
        quest: QuestId,
    },
    UnknownChoice {
        quest: QuestId,
        stage: StageId,
        choice: ChoiceId,
        suggestion: Option<ChoiceId>,
    },
    /// The character is at `stage`, but its requirements don't hold yet.
    StageBlocked {
        character: CharacterId,
        quest: QuestId,
        stage: StageId,
        unmet: Vec<Unmet>,
    },
    /// The world refused the choice's effects.
    Refused(CommandError),
}

/// One quest command in the log's journal, accepted or not, and where it came among the
/// world's commands (Q7, P-73).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestEntry {
    pub command: QuestCommand,
    /// How many commands the world's journal held when it came.
    pub at: usize,
    /// How many commands it sent the world: 1 if its effects went there, else 0.
    pub sent: usize,
    /// How many quest events it produced; `None` if it was refused.
    pub events: Option<usize>,
}

/// Why a saved quest log can't be restored onto its quests and world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestRestoreError {
    /// The journal's accepted commands account for a different number of events.
    EventCount { journal: usize, events: usize },
    /// A command, at `index` in the journal, that doesn't sit within the world's journal
    /// after the one before it.
    Misplaced { index: usize },
    /// An event, at `index`, naming a character, quest, stage or choice that doesn't exist.
    DoesNotFit { index: usize },
}

/// Every character's progress through the world's quests (P-72).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestLog {
    quests: Quests,
    records: BTreeMap<CharacterId, Record>,
    events: Vec<QuestEvent>,
    journal: Vec<QuestEntry>,
}

impl QuestLog {
    /// A log of no progress through `quests`, which must have passed their checks (P-32).
    pub fn new(quests: Quests) -> QuestLog {
        QuestLog {
            quests,
            records: BTreeMap::new(),
            events: Vec::new(),
            journal: Vec::new(),
        }
    }

    /// Rebuilds a log from a save: its journal and its events, replayed without running any
    /// rules (P-16), onto `world` as restored. Every event must name what exists, and every
    /// command sit within the world's journal, in order.
    pub fn restore(
        quests: Quests,
        journal: Vec<QuestEntry>,
        events: Vec<QuestEvent>,
        world: &World,
    ) -> Result<QuestLog, QuestRestoreError> {
        let accounted = journal
            .iter()
            .filter_map(|entry| entry.events)
            .fold(0_usize, usize::saturating_add);
        if accounted != events.len() {
            return Err(QuestRestoreError::EventCount {
                journal: accounted,
                events: events.len(),
            });
        }
        let mut next = 0;
        for (index, entry) in journal.iter().enumerate() {
            let end = entry.at.saturating_add(entry.sent);
            if entry.at < next || end > world.journal().len() {
                return Err(QuestRestoreError::Misplaced { index });
            }
            next = end;
        }
        let mut log = QuestLog::new(quests);
        for (index, event) in events.iter().enumerate() {
            if !log.fits(world, event) {
                return Err(QuestRestoreError::DoesNotFit { index });
            }
            log.apply(event);
        }
        log.journal = journal;
        Ok(log)
    }

    /// Whether everything an event names exists.
    fn fits(&self, world: &World, event: &QuestEvent) -> bool {
        let (character, quest) = match event {
            QuestEvent::QuestStarted { character, quest }
            | QuestEvent::StageReached {
                character, quest, ..
            }
            | QuestEvent::ChoiceMade {
                character, quest, ..
            }
            | QuestEvent::QuestFinished { character, quest }
            | QuestEvent::QuestClosed {
                character, quest, ..
            } => (character, quest),
        };
        let Some(found) = self.quests.quests.get(quest) else {
            return false;
        };
        let stage = |id: &StageId| found.stages.iter().find(|stage| &stage.id == id);
        world.character(character).is_some()
            && match event {
                QuestEvent::StageReached { stage: id, .. } => stage(id).is_some(),
                QuestEvent::ChoiceMade {
                    stage: id, choice, ..
                } => stage(id).is_some_and(|stage| stage.choices.iter().any(|c| &c.id == choice)),
                QuestEvent::QuestClosed {
                    questline, step, ..
                } => self.quests.place_of(quest) == Some((questline, *step)),
                QuestEvent::QuestStarted { .. } | QuestEvent::QuestFinished { .. } => true,
            }
    }

    /// Every quest command so far, accepted or not, in order.
    pub fn journal(&self) -> &[QuestEntry] {
        &self.journal
    }

    /// Every character's progress, in id order: those who've done anything.
    pub fn records(&self) -> &BTreeMap<CharacterId, Record> {
        &self.records
    }

    pub fn quests(&self) -> &Quests {
        &self.quests
    }

    /// Every quest event so far, oldest first.
    pub fn events(&self) -> &[QuestEvent] {
        &self.events
    }

    /// Whether anyone has started or had closed any quest.
    pub fn has_progress(&self) -> bool {
        !self.events.is_empty()
    }

    /// A character's progress; empty if they've done nothing yet.
    pub fn record(&self, character: &CharacterId) -> Record {
        self.records.get(character).cloned().unwrap_or_default()
    }

    /// The furthest step (from 0) of `questline` open to `character`: every step before it
    /// complete. `None` for an unknown questline.
    pub fn open_step(&self, character: &CharacterId, questline: &QuestlineId) -> Option<usize> {
        let line = self.quests.questlines.get(questline)?;
        let record = self.records.get(character);
        let complete = |step: usize| {
            let finished = line.steps[step]
                .quests
                .iter()
                .filter(|quest| finished(record, quest))
                .count();
            finished >= line.steps[step].needed()
        };
        // The first step that isn't complete, or else the last.
        let last = line.steps.len() - 1;
        Some((0..last).find(|step| !complete(*step)).unwrap_or(last))
    }

    /// Whether `character` can start `quest` in `world` now, with every reason they can't:
    /// their progress in it, then the step before its own that isn't complete, then each
    /// requirement of its gate that doesn't hold, its own then its step's.
    pub fn assess_start(
        &self,
        world: &World,
        character: &CharacterId,
        quest: &QuestId,
    ) -> Result<StartAssessment, QuestError> {
        self.existing(world, character)?;
        let found = self.quest(quest)?;
        let record = self.records.get(character);
        let mut blocks = Vec::new();
        match record.and_then(|record| record.states.get(quest)) {
            Some(QuestState::Active { .. }) => blocks.push(StartBlock::AlreadyStarted),
            Some(QuestState::Finished) => blocks.push(StartBlock::AlreadyFinished),
            Some(QuestState::Closed { questline, step }) => blocks.push(StartBlock::Closed {
                questline: questline.clone(),
                step: *step,
            }),
            None => {}
        }
        let mut gate = vec![&found.requires];
        if let Some((line, step)) = self.quests.place_of(quest) {
            let open = self.open_step(character, line).unwrap_or_default();
            if open < step {
                let steps = &self.quests.questlines[line].steps;
                let done = steps[open]
                    .quests
                    .iter()
                    .filter(|quest| finished(record, quest))
                    .count();
                blocks.push(StartBlock::StepIncomplete {
                    questline: line.clone(),
                    step: open,
                    done,
                    need: steps[open].needed(),
                });
            }
            gate.push(&self.quests.questlines[line].steps[step].requires);
        }
        for requires in gate {
            blocks.extend(
                self.unmet(world, character, requires)
                    .into_iter()
                    .map(StartBlock::Unmet),
            );
        }
        Ok(StartAssessment {
            character: character.clone(),
            quest: quest.clone(),
            blocks,
        })
    }

    /// Every requirement in `requires` that doesn't hold for `character` in `world` now, in
    /// the order `Requirements::KEYS` lists them.
    pub fn unmet(
        &self,
        world: &World,
        character: &CharacterId,
        requires: &Requirements,
    ) -> Vec<Unmet> {
        let mut unmet = Vec::new();
        let memberships: BTreeMap<FactionId, RankId> = world
            .memberships(character)
            .into_iter()
            .flatten()
            .map(|(faction, membership)| (faction.clone(), membership.rank.clone()))
            .collect();
        for (party, needs) in &requires.standing {
            let party = party_of(world, party);
            let has = world.standing(character, &party).unwrap_or_default();
            if has < *needs {
                unmet.push(Unmet::Standing {
                    party,
                    needs: *needs,
                    has,
                });
            }
        }
        for faction in &requires.member {
            if !memberships.contains_key(faction) {
                unmet.push(Unmet::Member(faction.clone()));
            }
        }
        for faction in &requires.not_member {
            if memberships.contains_key(faction) {
                unmet.push(Unmet::NotMember(faction.clone()));
            }
        }
        for (faction, needs) in &requires.rank_at_least {
            let has = memberships.get(faction);
            let ladder = world.faction(faction);
            let rung = |rank: &RankId| ladder.and_then(|ladder| ladder.rank_position(rank));
            let high_enough = has.is_some_and(|has| rung(has) >= rung(needs));
            if !high_enough {
                unmet.push(Unmet::Rank {
                    faction: faction.clone(),
                    needs: needs.clone(),
                    has: has.cloned(),
                });
            }
        }
        for faction in &requires.within_tolerance {
            let (Some(found), Some(distance)) = (
                world.faction(faction),
                world.distance(&Observer::Faction(faction.clone()), character),
            ) else {
                continue;
            };
            let tolerance = found.tolerances.member();
            if distance.value > tolerance {
                unmet.push(Unmet::Tolerance {
                    faction: faction.clone(),
                    distance: distance.value,
                    tolerance,
                });
            }
        }
        let record = self.records.get(character);
        for progress in &requires.done {
            if !done(record, progress) {
                unmet.push(Unmet::Done(progress.clone()));
            }
        }
        unmet
    }

    /// Runs one command: checks it, sends a choice's effects to `world`, and applies what
    /// happened. Refused, it changes nothing, here or in the world, but the journals. Either
    /// way the journal records it.
    pub fn execute(
        &mut self,
        world: &mut World,
        command: QuestCommand,
    ) -> Result<Vec<Played>, QuestError> {
        let at = world.journal().len();
        let result = match command.clone() {
            QuestCommand::StartQuest { character, quest } => self.start(world, character, quest),
            QuestCommand::MakeChoice {
                character,
                quest,
                choice,
                witnesses,
            } => self.choose(world, character, quest, choice, witnesses),
        };
        let mut events = None;
        if let Ok(played) = &result {
            let mut count = 0;
            for event in played {
                if let Played::Quest(event) = event {
                    self.apply(event);
                    count += 1;
                }
            }
            events = Some(count);
        }
        self.journal.push(QuestEntry {
            command,
            at,
            sent: world.journal().len() - at,
            events,
        });
        result
    }

    fn start(
        &self,
        world: &World,
        character: CharacterId,
        quest: QuestId,
    ) -> Result<Vec<Played>, QuestError> {
        let assessment = self.assess_start(world, &character, &quest)?;
        if !assessment.allowed() {
            return Err(QuestError::CannotStart(assessment));
        }
        let first = self.quests.quests[&quest].stages[0].id.clone();
        let mut played = vec![
            QuestEvent::QuestStarted {
                character: character.clone(),
                quest: quest.clone(),
            },
            QuestEvent::StageReached {
                character: character.clone(),
                quest: quest.clone(),
                stage: first,
            },
        ];
        // Starting a quest of a later step closes the earlier steps' leftovers not yet
        // started, where they close (P-69).
        if let Some((line, step)) = self.quests.place_of(&quest) {
            let record = self.records.get(&character);
            for (earlier, closing) in self.quests.questlines[line].steps[..step]
                .iter()
                .enumerate()
                .filter(|(_, step)| step.leftovers == Leftovers::Close)
            {
                for leftover in &closing.quests {
                    let untouched =
                        record.is_none_or(|record| !record.states.contains_key(leftover));
                    if untouched {
                        played.push(QuestEvent::QuestClosed {
                            character: character.clone(),
                            quest: leftover.clone(),
                            questline: line.clone(),
                            step: earlier,
                        });
                    }
                }
            }
        }
        Ok(played.into_iter().map(Played::Quest).collect())
    }

    fn choose(
        &self,
        world: &mut World,
        character: CharacterId,
        quest: QuestId,
        choice: ChoiceId,
        witnesses: Witnesses,
    ) -> Result<Vec<Played>, QuestError> {
        self.existing(world, &character)?;
        let found = self.quest(&quest)?;
        let at = match self
            .records
            .get(&character)
            .and_then(|record| record.states.get(&quest))
        {
            Some(QuestState::Active { stage }) => *stage,
            Some(QuestState::Finished) => return Err(QuestError::Finished { character, quest }),
            Some(QuestState::Closed { .. }) | None => {
                return Err(QuestError::NotStarted { character, quest });
            }
        };
        let stage = &found.stages[at];
        let Some(chosen) = stage.choices.iter().find(|c| c.id == choice) else {
            let ids = stage.choices.iter().map(|c| c.id.as_str());
            let suggestion = suggest(choice.as_str(), ids).and_then(|id| ChoiceId::new(id).ok());
            return Err(QuestError::UnknownChoice {
                quest,
                stage: stage.id.clone(),
                choice,
                suggestion,
            });
        };
        let unmet = self.unmet(world, &character, &stage.requires);
        if !unmet.is_empty() {
            return Err(QuestError::StageBlocked {
                character,
                quest,
                stage: stage.id.clone(),
                unmet,
            });
        }
        if let Witnesses::These(witnesses) = &witnesses {
            for witness in witnesses {
                if world.character(witness).is_none() {
                    return Err(QuestError::UnknownWitness {
                        witness: witness.clone(),
                        suggestion: closest_character(world, witness),
                    });
                }
            }
        }
        let effects = match &chosen.effects {
            ChoiceEffects::None => None,
            ChoiceEffects::Outcome(outcome) => Some(Command::ApplyOutcome {
                outcome: outcome.clone(),
                character: character.clone(),
                witnesses,
            }),
            ChoiceEffects::Inline(effects) => Some(Command::ApplyEffects {
                source: format!("quest:{quest}.{}.{choice}", stage.id),
                character: character.clone(),
                effects: effects.clone(),
                witnesses,
            }),
        };
        let mut played = vec![Played::Quest(QuestEvent::ChoiceMade {
            character: character.clone(),
            quest: quest.clone(),
            stage: stage.id.clone(),
            choice: choice.clone(),
        })];
        // The world's command is the last thing checked: once it's accepted, nothing here
        // can refuse.
        if let Some(command) = effects {
            let events = world.execute(command).map_err(QuestError::Refused)?;
            played.extend(events.into_iter().map(Played::World));
        }
        played.push(Played::Quest(match &chosen.next {
            Next::Stage(next) => QuestEvent::StageReached {
                character,
                quest,
                stage: next.clone(),
            },
            Next::End => QuestEvent::QuestFinished { character, quest },
        }));
        Ok(played)
    }

    /// Applying events is the only thing that changes the log.
    fn apply(&mut self, event: &QuestEvent) {
        self.events.push(event.clone());
        match event {
            QuestEvent::QuestStarted { character, quest } => {
                let record = self.records.entry(character.clone()).or_default();
                record
                    .states
                    .insert(quest.clone(), QuestState::Active { stage: 0 });
            }
            QuestEvent::StageReached {
                character,
                quest,
                stage,
            } => {
                let place = self.quests.quests[quest].stage_index(stage);
                let record = self.records.entry(character.clone()).or_default();
                record
                    .states
                    .insert(quest.clone(), QuestState::Active { stage: place });
                record.reached.insert((quest.clone(), stage.clone()));
            }
            QuestEvent::ChoiceMade {
                character,
                quest,
                stage,
                choice,
            } => {
                let record = self.records.entry(character.clone()).or_default();
                record
                    .chosen
                    .insert((quest.clone(), stage.clone(), choice.clone()));
            }
            QuestEvent::QuestFinished { character, quest } => {
                let record = self.records.entry(character.clone()).or_default();
                record.states.insert(quest.clone(), QuestState::Finished);
            }
            QuestEvent::QuestClosed {
                character,
                quest,
                questline,
                step,
            } => {
                let record = self.records.entry(character.clone()).or_default();
                record.states.insert(
                    quest.clone(),
                    QuestState::Closed {
                        questline: questline.clone(),
                        step: *step,
                    },
                );
            }
        }
    }

    fn existing(&self, world: &World, character: &CharacterId) -> Result<(), QuestError> {
        if world.character(character).is_some() {
            return Ok(());
        }
        Err(QuestError::UnknownCharacter {
            character: character.clone(),
            suggestion: closest_character(world, character),
        })
    }

    fn quest(&self, quest: &QuestId) -> Result<&Quest, QuestError> {
        self.quests.quests.get(quest).ok_or_else(|| {
            let ids = self.quests.quests.keys().map(QuestId::as_str);
            QuestError::UnknownQuest {
                quest: quest.clone(),
                suggestion: suggest(quest.as_str(), ids).and_then(|id| QuestId::new(id).ok()),
            }
        })
    }
}

fn finished(record: Option<&Record>, quest: &QuestId) -> bool {
    record.is_some_and(|record| record.states.get(quest) == Some(&QuestState::Finished))
}

/// Whether some progress has happened: a quest finished, a stage reached or a choice made.
fn done(record: Option<&Record>, progress: &Progress) -> bool {
    let Some(record) = record else {
        return false;
    };
    match progress {
        Progress::Quest(quest) => record.states.get(quest) == Some(&QuestState::Finished),
        Progress::Stage(quest, stage) => record.reached.contains(&(quest.clone(), stage.clone())),
        Progress::Choice(quest, stage, choice) => {
            record
                .chosen
                .contains(&(quest.clone(), stage.clone(), choice.clone()))
        }
    }
}

/// The faction or character an id names in `world`: a faction if there's one, as one id
/// namespace allows (P-35).
fn party_of(world: &World, party: &PartyRef) -> Party {
    match FactionId::new(party.as_str()) {
        Ok(faction) if world.faction(&faction).is_some() => Party::Faction(faction),
        _ => Party::Character(CharacterId::new(party.as_str()).expect("a valid id")),
    }
}

fn closest_character(world: &World, character: &CharacterId) -> Option<CharacterId> {
    let ids = world.characters().map(|c| c.id.as_str());
    suggest(character.as_str(), ids).and_then(|id| CharacterId::new(id).ok())
}

/// `" (did you mean 'x'?)"`, or nothing.
fn did_you_mean(f: &mut fmt::Formatter<'_>, suggestion: Option<&dyn fmt::Display>) -> fmt::Result {
    match suggestion {
        Some(close) => write!(f, " (did you mean '{close}'?)"),
        None => Ok(()),
    }
}

impl Unmet {
    /// What's missing, in a designer's words, for `character`.
    pub fn describe(&self, character: &CharacterId) -> String {
        match self {
            Unmet::Standing { party, needs, has } => {
                format!("it needs standing {needs} with {party}, and {character} has {has}")
            }
            Unmet::Member(faction) => {
                format!("it needs membership of {faction}, and {character} isn't in {faction}")
            }
            Unmet::NotMember(faction) => {
                format!("it needs not to be in {faction}, and {character} is")
            }
            Unmet::Rank {
                faction,
                needs,
                has: None,
            } => format!(
                "it needs rank {needs} or higher in {faction}, and {character} isn't in {faction}"
            ),
            Unmet::Rank {
                faction,
                needs,
                has: Some(has),
            } => {
                format!("it needs rank {needs} or higher in {faction}, and {character} is a {has}")
            }
            Unmet::Tolerance {
                faction,
                distance,
                tolerance,
            } => format!(
                "it needs {character} within {faction}'s member tolerance of {tolerance}, and {faction} pictures them {distance} away"
            ),
            Unmet::Done(progress) => format!("it needs {progress} done"),
        }
    }
}

impl StartBlock {
    /// Why `character` can't start the quest, in a designer's words.
    pub fn describe(&self, character: &CharacterId) -> String {
        match self {
            StartBlock::AlreadyStarted => format!("{character} has already started it"),
            StartBlock::AlreadyFinished => format!("{character} has already finished it"),
            StartBlock::Closed { questline, step } => {
                format!("it closed when {questline} moved on from step {}", step + 1)
            }
            StartBlock::StepIncomplete {
                questline,
                step,
                done,
                need,
            } => format!(
                "step {} of {questline} isn't complete: {done} of {need} done",
                step + 1
            ),
            StartBlock::Unmet(unmet) => unmet.describe(character),
        }
    }
}

impl fmt::Display for StartAssessment {
    /// `player can start watch_oath`, or `player can't start …:` with each reason on its own
    /// line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (character, quest) = (&self.character, &self.quest);
        if self.allowed() {
            return write!(f, "{character} can start {quest}");
        }
        write!(f, "{character} can't start {quest}:")?;
        for block in &self.blocks {
            write!(f, "\n- {}", block.describe(character))?;
        }
        Ok(())
    }
}

impl fmt::Display for QuestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestError::UnknownCharacter {
                character,
                suggestion,
            } => {
                write!(f, "unknown character '{character}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestError::UnknownQuest { quest, suggestion } => {
                write!(f, "unknown quest '{quest}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestError::UnknownWitness {
                witness,
                suggestion,
            } => {
                write!(f, "unknown witness '{witness}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestError::CannotStart(assessment) => assessment.fmt(f),
            QuestError::NotStarted { character, quest } => {
                write!(f, "{character} hasn't started {quest}")
            }
            QuestError::Finished { character, quest } => {
                write!(f, "{character} has finished {quest}")
            }
            QuestError::UnknownChoice {
                quest,
                stage,
                choice,
                suggestion,
            } => {
                write!(f, "unknown choice '{choice}' at {quest}.{stage}")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestError::StageBlocked {
                character,
                quest,
                stage,
                unmet,
            } => {
                let unmet: Vec<String> = unmet.iter().map(|u| u.describe(character)).collect();
                write!(
                    f,
                    "{character} can't choose at {quest}.{stage} yet: {}",
                    unmet.join("; ")
                )
            }
            QuestError::Refused(refusal) => refusal.fmt(f),
        }
    }
}

impl fmt::Display for QuestRestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestRestoreError::EventCount { journal, events } => write!(
                f,
                "the quest journal accounts for {journal} events, but the save holds {events}"
            ),
            QuestRestoreError::Misplaced { index } => write!(
                f,
                "quest command {} doesn't sit within the world's journal after the one before",
                index + 1
            ),
            QuestRestoreError::DoesNotFit { index } => write!(
                f,
                "quest event {} names a character, quest, stage or choice that doesn't exist",
                index + 1
            ),
        }
    }
}

impl fmt::Display for QuestCommand {
    /// As the REPL writes it: `start player watch_oath`, or `choose player watch_oath report`
    /// with who saw it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestCommand::StartQuest { character, quest } => {
                write!(f, "start {character} {quest}")
            }
            QuestCommand::MakeChoice {
                character,
                quest,
                choice,
                witnesses,
            } => {
                write!(f, "choose {character} {quest} {choice}")?;
                match witnesses {
                    Witnesses::Everyone => Ok(()),
                    Witnesses::Nobody => f.write_str(" --unseen"),
                    Witnesses::These(seen) => {
                        let seen: Vec<&str> = seen.iter().map(CharacterId::as_str).collect();
                        write!(f, " --seen-by {}", seen.join(","))
                    }
                }
            }
        }
    }
}

impl fmt::Display for QuestEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestEvent::QuestStarted { character, quest } => {
                write!(f, "{character} started {quest}")
            }
            QuestEvent::StageReached {
                character,
                quest,
                stage,
            } => write!(f, "{character} reached {quest}.{stage}"),
            QuestEvent::ChoiceMade {
                character,
                quest,
                stage,
                choice,
            } => write!(f, "{character} chose {quest}.{stage}.{choice}"),
            QuestEvent::QuestFinished { character, quest } => {
                write!(f, "{character} finished {quest}")
            }
            QuestEvent::QuestClosed {
                character,
                quest,
                questline,
                step,
            } => write!(
                f,
                "{quest} closed for {character}: {questline} moved on from step {}",
                step + 1
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use factional_reputation::{
        Alignment, Balance, Character, Content, Effects, Faction, Rank, StandingEffects,
        StartingMembership, Tolerances,
    };

    use super::*;
    use crate::quest::{Choice, Questline, Stage, Step};

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn faction_id(id: &str) -> FactionId {
        FactionId::new(id).expect("valid id")
    }

    fn character_id(id: &str) -> CharacterId {
        CharacterId::new(id).expect("valid id")
    }

    fn quest_id(id: &str) -> QuestId {
        QuestId::new(id).expect("valid id")
    }

    fn stage_id(id: &str) -> StageId {
        StageId::new(id).expect("valid id")
    }

    fn rank(id: &str) -> RankId {
        RankId::new(id).expect("valid id")
    }

    /// The Watch at law 60 (tolerance 40, member tolerance 50; recruit, sergeant) and the
    /// Guild at law -60 (cutpurse). Hero is at law 10, a recruit of the Watch, regarded by it
    /// at 10; Rook is at law -60, in nothing.
    fn world() -> World {
        let faction = |id: &str, law: i64, ranks: &[&str]| Faction {
            id: faction_id(id),
            name: id.to_owned(),
            alignment: Alignment::new(h(law), h(0)).expect("in range"),
            weights: None,
            tolerances: Tolerances::new(h(40_00), Some(h(50_00))).expect("valid"),
            leave_standing_change: h(0),
            ranks: ranks
                .iter()
                .map(|id| Rank {
                    id: rank(id),
                    requires_standing: None,
                    tolerance: None,
                })
                .collect(),
            rule_tables: BTreeMap::new(),
            drift: None,
            expel_standing_change: Faction::DEFAULT_EXPEL_STANDING_CHANGE,
            secret_members: false,
        };
        let character = |id: &str, law: i64| Character {
            id: character_id(id),
            name: id.to_owned(),
            alignment: Alignment::new(h(law), h(0)).expect("in range"),
            weights: None,
            inertia: None,
            memberships: Vec::new(),
            standing: StandingEffects::default(),
            contacts: Vec::new(),
        };
        let mut hero = character("hero", 10_00);
        hero.memberships = vec![StartingMembership {
            faction: faction_id("watch"),
            rank: None,
            secret: false,
        }];
        hero.standing.factions = [(faction_id("watch"), h(10_00))].into();
        let content = Content {
            balance: Balance::default(),
            characters: [
                (hero.id.clone(), hero),
                (character_id("rook"), character("rook", -60_00)),
            ]
            .into(),
            factions: [
                (
                    faction_id("watch"),
                    faction("watch", 60_00, &["recruit", "sergeant"]),
                ),
                (faction_id("guild"), faction("guild", -60_00, &["cutpurse"])),
            ]
            .into(),
            actions: BTreeMap::new(),
            relations: Vec::new(),
            outcomes: BTreeMap::new(),
        };
        World::new(content).expect("a valid world")
    }

    fn choice(id: &str, effects: ChoiceEffects, next: Option<&str>) -> Choice {
        Choice {
            id: ChoiceId::new(id).expect("valid id"),
            effects,
            next: next.map_or(Next::End, |next| Next::Stage(stage_id(next))),
            locks: Vec::new(),
        }
    }

    fn stage(id: &str, requires: Requirements, choices: Vec<Choice>) -> Stage {
        Stage {
            id: stage_id(id),
            requires,
            choices,
        }
    }

    fn quest(id: &str, stages: Vec<Stage>) -> Quest {
        Quest {
            id: quest_id(id),
            name: id.to_owned(),
            giver: None,
            requires: Requirements::default(),
            stages,
        }
    }

    /// A one-stage quest with one choice, `done`, that ends it.
    fn errand(id: &str) -> Quest {
        quest(
            id,
            vec![stage(
                "go",
                Requirements::default(),
                vec![choice("done", ChoiceEffects::None, None)],
            )],
        )
    }

    fn needs_watch(at_least: i64) -> Requirements {
        Requirements {
            standing: [(PartyRef::new("watch").expect("valid id"), h(at_least))].into(),
            ..Requirements::default()
        }
    }

    /// `oath`: at `swear`, `take` gives the Watch 5 and leads to `serve`, which needs 20 with
    /// it; `refuse` ends it. `serve`'s `go` ends it. `career`: `oath`; then one of `a`, `b`
    /// and `c`, the rest closing; then `d`, beside `e` in a step whose leftovers stay open;
    /// then `f`.
    fn log() -> QuestLog {
        let take = Effects {
            standing: StandingEffects {
                factions: [(faction_id("watch"), h(5_00))].into(),
                ..StandingEffects::default()
            },
            ..Effects::default()
        };
        let oath = quest(
            "oath",
            vec![
                stage(
                    "swear",
                    Requirements::default(),
                    vec![
                        choice("take", ChoiceEffects::Inline(take), Some("serve")),
                        choice("refuse", ChoiceEffects::None, None),
                    ],
                ),
                stage(
                    "serve",
                    needs_watch(20_00),
                    vec![choice("go", ChoiceEffects::None, None)],
                ),
            ],
        );
        let step = |quests: &[&str], need: Option<usize>, leftovers: Leftovers| Step {
            quests: quests.iter().map(|id| quest_id(id)).collect(),
            need,
            requires: Requirements::default(),
            leftovers,
        };
        let career = Questline {
            id: QuestlineId::new("career").expect("valid id"),
            name: "career".to_owned(),
            giver: None,
            steps: vec![
                step(&["oath"], None, Leftovers::Open),
                step(&["a", "b", "c"], Some(1), Leftovers::Close),
                step(&["d", "e"], Some(1), Leftovers::Open),
                step(&["f"], None, Leftovers::Open),
            ],
        };
        let mut quests: BTreeMap<QuestId, Quest> = ["a", "b", "c", "d", "e", "f", "alone"]
            .into_iter()
            .map(|id| (quest_id(id), errand(id)))
            .collect();
        quests.insert(oath.id.clone(), oath);
        QuestLog::new(Quests {
            quests,
            questlines: [(career.id.clone(), career)].into(),
        })
    }

    fn start(who: &str, what: &str) -> QuestCommand {
        QuestCommand::StartQuest {
            character: character_id(who),
            quest: quest_id(what),
        }
    }

    fn pick(who: &str, what: &str, choice: &str) -> QuestCommand {
        QuestCommand::MakeChoice {
            character: character_id(who),
            quest: quest_id(what),
            choice: ChoiceId::new(choice).expect("valid id"),
            witnesses: Witnesses::Everyone,
        }
    }

    /// Runs each command, expecting it to be accepted.
    fn run(log: &mut QuestLog, world: &mut World, commands: Vec<QuestCommand>) {
        for command in commands {
            log.execute(world, command.clone())
                .unwrap_or_else(|error| panic!("{command:?}: {error}"));
        }
    }

    fn quest_events(played: &[Played]) -> Vec<String> {
        played
            .iter()
            .filter_map(|each| match each {
                Played::Quest(event) => Some(event.to_string()),
                Played::World(_) => None,
            })
            .collect()
    }

    // Requirements

    #[test]
    fn every_requirement_that_doesnt_hold_is_listed_with_what_the_character_has() {
        let world = world();
        let log = log();
        let requires = Requirements {
            standing: [(PartyRef::new("watch").expect("valid id"), h(10_01))].into(),
            member: vec![faction_id("watch")],
            not_member: vec![faction_id("watch")],
            rank_at_least: [(faction_id("watch"), rank("sergeant"))].into(),
            within_tolerance: vec![faction_id("watch")],
            done: vec![Progress::Quest(quest_id("oath"))],
        };
        // Rook is 120 from the Watch: law -60 against 60.
        assert_eq!(
            log.unmet(&world, &character_id("rook"), &requires),
            [
                Unmet::Standing {
                    party: Party::Faction(faction_id("watch")),
                    needs: h(10_01),
                    has: h(0),
                },
                Unmet::Member(faction_id("watch")),
                Unmet::Rank {
                    faction: faction_id("watch"),
                    needs: rank("sergeant"),
                    has: None,
                },
                Unmet::Tolerance {
                    faction: faction_id("watch"),
                    distance: h(120_00),
                    tolerance: h(50_00),
                },
                Unmet::Done(Progress::Quest(quest_id("oath"))),
            ]
        );
        assert_eq!(
            log.unmet(&world, &character_id("hero"), &requires),
            [
                Unmet::Standing {
                    party: Party::Faction(faction_id("watch")),
                    needs: h(10_01),
                    has: h(10_00),
                },
                Unmet::NotMember(faction_id("watch")),
                Unmet::Rank {
                    faction: faction_id("watch"),
                    needs: rank("sergeant"),
                    has: Some(rank("recruit")),
                },
                Unmet::Done(Progress::Quest(quest_id("oath"))),
            ]
        );
    }

    #[test]
    fn standing_can_be_needed_with_a_character() {
        let requires = Requirements {
            standing: [(PartyRef::new("rook").expect("valid id"), h(5_00))].into(),
            ..Requirements::default()
        };
        assert_eq!(
            log().unmet(&world(), &character_id("hero"), &requires),
            [Unmet::Standing {
                party: Party::Character(character_id("rook")),
                needs: h(5_00),
                has: h(0),
            }]
        );
    }

    #[test]
    fn a_requirement_met_exactly_holds() {
        let world = world();
        let log = log();
        // Hero has 10 with the Watch, is a recruit, and is 50 from it: its member tolerance.
        let requires = Requirements {
            standing: [(PartyRef::new("watch").expect("valid id"), h(10_00))].into(),
            rank_at_least: [(faction_id("watch"), rank("recruit"))].into(),
            within_tolerance: vec![faction_id("watch")],
            not_member: vec![faction_id("guild")],
            ..Requirements::default()
        };
        assert_eq!(log.unmet(&world, &character_id("hero"), &requires), []);
    }

    #[test]
    fn done_means_a_quest_finished_a_stage_reached_or_a_choice_made() {
        let mut world = world();
        let mut log = log();
        let done = |progress: &str| Requirements {
            done: vec![Progress::parse(progress).expect("valid")],
            ..Requirements::default()
        };
        let hero = character_id("hero");
        run(
            &mut log,
            &mut world,
            vec![start("hero", "oath"), pick("hero", "oath", "take")],
        );
        for progress in ["oath.swear", "oath.serve", "oath.swear.take"] {
            assert_eq!(log.unmet(&world, &hero, &done(progress)), [], "{progress}");
        }
        for progress in ["oath", "oath.swear.refuse", "oath.serve.go"] {
            assert_eq!(
                log.unmet(&world, &hero, &done(progress)).len(),
                1,
                "{progress}"
            );
        }
        assert_eq!(
            log.unmet(&world, &character_id("rook"), &done("oath.swear"))
                .len(),
            1,
            "progress is each character's own"
        );
    }

    // Starting

    #[test]
    fn a_quest_waits_for_every_step_before_its_own() {
        let mut world = world();
        let mut log = log();
        let hero = character_id("hero");
        let blocks = |log: &QuestLog, world: &World, quest: &str| {
            log.assess_start(world, &hero, &quest_id(quest))
                .expect("known")
                .blocks
        };
        let incomplete = |step, done, need| {
            vec![StartBlock::StepIncomplete {
                questline: QuestlineId::new("career").expect("valid id"),
                step,
                done,
                need,
            }]
        };
        assert_eq!(blocks(&log, &world, "a"), incomplete(0, 0, 1));
        assert_eq!(blocks(&log, &world, "f"), incomplete(0, 0, 1));
        assert_eq!(blocks(&log, &world, "oath"), []);
        assert_eq!(blocks(&log, &world, "alone"), []);
        run(
            &mut log,
            &mut world,
            vec![start("hero", "oath"), pick("hero", "oath", "refuse")],
        );
        assert_eq!(blocks(&log, &world, "a"), []);
        assert_eq!(blocks(&log, &world, "d"), incomplete(1, 0, 1));
        assert_eq!(
            log.open_step(&hero, &QuestlineId::new("career").expect("valid")),
            Some(1)
        );
        run(&mut log, &mut world, vec![start("hero", "b")]);
        assert_eq!(
            blocks(&log, &world, "d"),
            incomplete(1, 0, 1),
            "started isn't done"
        );
        run(&mut log, &mut world, vec![pick("hero", "b", "done")]);
        assert_eq!(blocks(&log, &world, "d"), []);
        assert_eq!(blocks(&log, &world, "f"), incomplete(2, 0, 1));
        assert_eq!(
            log.open_step(&hero, &QuestlineId::new("nowhere").expect("valid")),
            None
        );
    }

    #[test]
    fn starting_a_later_step_closes_untouched_leftovers_where_they_close() {
        let mut world = world();
        let mut log = log();
        run(
            &mut log,
            &mut world,
            vec![
                start("hero", "oath"),
                pick("hero", "oath", "refuse"),
                start("hero", "a"),
                start("hero", "b"),
                pick("hero", "b", "done"),
            ],
        );
        let played = log
            .execute(&mut world, start("hero", "d"))
            .expect("accepted");
        // `a` was started, so it stays open; only `c` closes.
        assert_eq!(
            quest_events(&played),
            [
                "hero started d",
                "hero reached d.go",
                "c closed for hero: career moved on from step 2",
            ]
        );
        run(&mut log, &mut world, vec![pick("hero", "d", "done")]);
        // `e` sits in a step whose leftovers stay open.
        let played = log
            .execute(&mut world, start("hero", "f"))
            .expect("accepted");
        assert_eq!(
            quest_events(&played),
            ["hero started f", "hero reached f.go"]
        );
        let hero = character_id("hero");
        assert_eq!(
            log.assess_start(&world, &hero, &quest_id("c"))
                .expect("known")
                .blocks,
            [StartBlock::Closed {
                questline: QuestlineId::new("career").expect("valid id"),
                step: 1,
            }]
        );
        assert_eq!(
            log.record(&hero).states[&quest_id("a")],
            QuestState::Active { stage: 0 }
        );
        assert_eq!(
            log.assess_start(&world, &hero, &quest_id("e"))
                .expect("known")
                .blocks,
            []
        );
        // Every step done: the last is as far as it goes.
        run(&mut log, &mut world, vec![pick("hero", "f", "done")]);
        let career = QuestlineId::new("career").expect("valid id");
        assert_eq!(log.open_step(&hero, &career), Some(3));
    }

    #[test]
    fn a_quest_is_started_and_finished_once() {
        let mut world = world();
        let mut log = log();
        let hero = character_id("hero");
        let blocks = |log: &QuestLog, world: &World| {
            log.assess_start(world, &hero, &quest_id("alone"))
                .expect("known")
                .blocks
        };
        run(&mut log, &mut world, vec![start("hero", "alone")]);
        assert_eq!(blocks(&log, &world), [StartBlock::AlreadyStarted]);
        run(&mut log, &mut world, vec![pick("hero", "alone", "done")]);
        assert_eq!(blocks(&log, &world), [StartBlock::AlreadyFinished]);
        assert_eq!(
            log.execute(&mut world, pick("hero", "alone", "done")),
            Err(QuestError::Finished {
                character: hero.clone(),
                quest: quest_id("alone"),
            })
        );
    }

    // Choosing

    #[test]
    fn a_choice_applies_its_effects_and_reaches_the_next_stage_whatever_it_needs() {
        let mut world = world();
        let mut log = log();
        run(&mut log, &mut world, vec![start("hero", "oath")]);
        let played = log
            .execute(&mut world, pick("hero", "oath", "take"))
            .expect("accepted");
        assert_eq!(
            quest_events(&played),
            ["hero chose oath.swear.take", "hero reached oath.serve"]
        );
        assert!(matches!(played[1], Played::World(_)), "{played:?}");
        let hero = character_id("hero");
        let watch = Party::Faction(faction_id("watch"));
        assert_eq!(world.standing(&hero, &watch), Some(h(15_00)));
        // `serve` needs 20, and Hero has 15: they wait there.
        let waiting = log.execute(&mut world, pick("hero", "oath", "go"));
        assert_eq!(
            waiting,
            Err(QuestError::StageBlocked {
                character: hero.clone(),
                quest: quest_id("oath"),
                stage: stage_id("serve"),
                unmet: vec![Unmet::Standing {
                    party: watch,
                    needs: h(20_00),
                    has: h(15_00),
                }],
            })
        );
    }

    #[test]
    fn a_refused_command_changes_nothing() {
        let mut world = world();
        let mut log = log();
        run(&mut log, &mut world, vec![start("hero", "oath")]);
        let progress = |log: &QuestLog| (log.records().clone(), log.events().to_vec());
        let before = (progress(&log), world.events().len());
        let refused = [
            start("hero", "oath"),
            start("hero", "f"),
            start("nobody", "oath"),
            start("hero", "oth"),
            pick("hero", "alone", "done"),
            pick("hero", "oath", "tak"),
            QuestCommand::MakeChoice {
                character: character_id("hero"),
                quest: quest_id("oath"),
                choice: ChoiceId::new("take").expect("valid id"),
                witnesses: Witnesses::These([character_id("hro")].into()),
            },
        ];
        for command in refused {
            assert!(
                log.execute(&mut world, command.clone()).is_err(),
                "{command:?}"
            );
            assert_eq!(
                (progress(&log), world.events().len()),
                before,
                "{command:?}"
            );
        }
    }

    #[test]
    fn refusals_name_what_was_meant() {
        let mut world = world();
        let mut log = log();
        let message = |log: &mut QuestLog, world: &mut World, command| {
            log.execute(world, command)
                .expect_err("refused")
                .to_string()
        };
        assert_eq!(
            message(&mut log, &mut world, start("hro", "oath")),
            "unknown character 'hro' (did you mean 'hero'?)"
        );
        assert_eq!(
            message(&mut log, &mut world, start("hero", "oth")),
            "unknown quest 'oth' (did you mean 'oath'?)"
        );
        assert_eq!(
            message(&mut log, &mut world, pick("hero", "oath", "take")),
            "hero hasn't started oath"
        );
        run(&mut log, &mut world, vec![start("hero", "oath")]);
        assert_eq!(
            message(&mut log, &mut world, pick("hero", "oath", "tak")),
            "unknown choice 'tak' at oath.swear (did you mean 'take'?)"
        );
        let unseen = QuestCommand::MakeChoice {
            character: character_id("hero"),
            quest: quest_id("oath"),
            choice: ChoiceId::new("take").expect("valid id"),
            witnesses: Witnesses::These([character_id("rok")].into()),
        };
        assert_eq!(
            message(&mut log, &mut world, unseen),
            "unknown witness 'rok' (did you mean 'rook'?)"
        );
        assert_eq!(
            message(&mut log, &mut world, start("hero", "f")),
            "hero can't start f:\n- step 1 of career isn't complete: 0 of 1 done"
        );
    }

    #[test]
    fn what_doesnt_hold_is_said_in_a_designers_words() {
        let hero = character_id("hero");
        let watch = faction_id("watch");
        let said: Vec<String> = [
            Unmet::Standing {
                party: Party::Faction(watch.clone()),
                needs: h(10_00),
                has: h(-50),
            },
            Unmet::Member(watch.clone()),
            Unmet::NotMember(watch.clone()),
            Unmet::Rank {
                faction: watch.clone(),
                needs: rank("sergeant"),
                has: None,
            },
            Unmet::Rank {
                faction: watch.clone(),
                needs: rank("sergeant"),
                has: Some(rank("recruit")),
            },
            Unmet::Tolerance {
                faction: watch.clone(),
                distance: h(60_00),
                tolerance: h(50_00),
            },
            Unmet::Done(Progress::parse("oath.swear").expect("valid")),
        ]
        .iter()
        .map(|unmet| unmet.describe(&hero))
        .collect();
        assert_eq!(
            said,
            [
                "it needs standing 10.00 with watch, and hero has -0.50",
                "it needs membership of watch, and hero isn't in watch",
                "it needs not to be in watch, and hero is",
                "it needs rank sergeant or higher in watch, and hero isn't in watch",
                "it needs rank sergeant or higher in watch, and hero is a recruit",
                "it needs hero within watch's member tolerance of 50.00, and watch pictures them 60.00 away",
                "it needs oath.swear done",
            ]
        );
        let career = QuestlineId::new("career").expect("valid id");
        let blocked: Vec<String> = [
            StartBlock::AlreadyStarted,
            StartBlock::AlreadyFinished,
            StartBlock::Closed {
                questline: career.clone(),
                step: 1,
            },
            StartBlock::StepIncomplete {
                questline: career,
                step: 1,
                done: 1,
                need: 2,
            },
        ]
        .iter()
        .map(|block| block.describe(&hero))
        .collect();
        assert_eq!(
            blocked,
            [
                "hero has already started it",
                "hero has already finished it",
                "it closed when career moved on from step 2",
                "step 2 of career isn't complete: 1 of 2 done",
            ]
        );
    }

    /// A log through the oath and a refusal, on its world.
    fn played() -> (World, QuestLog) {
        let mut world = world();
        let mut log = log();
        run(
            &mut log,
            &mut world,
            vec![start("hero", "oath"), pick("hero", "oath", "take")],
        );
        let _ = log.execute(&mut world, start("hero", "oath"));
        (world, log)
    }

    #[test]
    fn the_journal_says_where_each_command_came_and_what_it_did() {
        let (_, log) = played();
        let who: Vec<&CharacterId> = log.records().keys().collect();
        assert_eq!(who, [&character_id("hero")]);
        let entry = |command, at, sent, events| QuestEntry {
            command,
            at,
            sent,
            events,
        };
        assert_eq!(
            log.journal(),
            [
                entry(start("hero", "oath"), 0, 0, Some(2)),
                entry(pick("hero", "oath", "take"), 0, 1, Some(2)),
                entry(start("hero", "oath"), 1, 0, None),
            ]
        );
    }

    #[test]
    fn a_restored_log_is_the_log_it_was() {
        let (world, log) = played();
        let restored = QuestLog::restore(
            log.quests().clone(),
            log.journal().to_vec(),
            log.events().to_vec(),
            &world,
        );
        assert_eq!(restored, Ok(log));
    }

    #[test]
    fn a_log_that_doesnt_fit_isnt_restored() {
        let (world, log) = played();
        let restore = |journal: Vec<QuestEntry>, events: Vec<QuestEvent>| {
            QuestLog::restore(log.quests().clone(), journal, events, &world)
        };
        let (journal, events) = (log.journal().to_vec(), log.events().to_vec());
        assert_eq!(
            restore(journal.clone(), events[..3].to_vec()),
            Err(QuestRestoreError::EventCount {
                journal: 4,
                events: 3,
            })
        );
        let mut early = journal.clone();
        early[2].at = 0;
        assert_eq!(
            restore(early, events.clone()),
            Err(QuestRestoreError::Misplaced { index: 2 })
        );
        let mut late = journal.clone();
        late[1].sent = 2;
        assert_eq!(
            restore(late, events.clone()),
            Err(QuestRestoreError::Misplaced { index: 1 })
        );
        let reached = |stage: &str| QuestEvent::StageReached {
            character: character_id("hero"),
            quest: quest_id("oath"),
            stage: stage_id(stage),
        };
        let unfit = [
            reached("nowhere"),
            QuestEvent::QuestStarted {
                character: character_id("nobody"),
                quest: quest_id("oath"),
            },
            QuestEvent::QuestStarted {
                character: character_id("hero"),
                quest: quest_id("nowhere"),
            },
            QuestEvent::ChoiceMade {
                character: character_id("hero"),
                quest: quest_id("oath"),
                stage: stage_id("swear"),
                choice: ChoiceId::new("nothing").expect("valid id"),
            },
            QuestEvent::ChoiceMade {
                character: character_id("hero"),
                quest: quest_id("oath"),
                stage: stage_id("nowhere"),
                choice: ChoiceId::new("take").expect("valid id"),
            },
            QuestEvent::QuestClosed {
                character: character_id("hero"),
                quest: quest_id("a"),
                questline: QuestlineId::new("career").expect("valid id"),
                step: 0,
            },
        ];
        for event in unfit {
            let mut changed = events.clone();
            changed[3] = event.clone();
            assert_eq!(
                restore(journal.clone(), changed),
                Err(QuestRestoreError::DoesNotFit { index: 3 }),
                "{event:?}"
            );
        }
        let closed = QuestEvent::QuestClosed {
            character: character_id("hero"),
            quest: quest_id("a"),
            questline: QuestlineId::new("career").expect("valid id"),
            step: 1,
        };
        let mut fits = events.clone();
        fits[3] = closed;
        assert!(restore(journal, fits).is_ok());
    }

    #[test]
    fn restore_problems_say_what_doesnt_fit() {
        let said: Vec<String> = [
            QuestRestoreError::EventCount {
                journal: 4,
                events: 3,
            },
            QuestRestoreError::Misplaced { index: 2 },
            QuestRestoreError::DoesNotFit { index: 3 },
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            said,
            [
                "the quest journal accounts for 4 events, but the save holds 3",
                "quest command 3 doesn't sit within the world's journal after the one before",
                "quest event 4 names a character, quest, stage or choice that doesn't exist",
            ]
        );
        let commands: Vec<String> = [
            start("hero", "oath"),
            pick("hero", "oath", "take"),
            QuestCommand::MakeChoice {
                character: character_id("hero"),
                quest: quest_id("oath"),
                choice: ChoiceId::new("take").expect("valid id"),
                witnesses: Witnesses::Nobody,
            },
            QuestCommand::MakeChoice {
                character: character_id("hero"),
                quest: quest_id("oath"),
                choice: ChoiceId::new("take").expect("valid id"),
                witnesses: Witnesses::These([character_id("rook"), character_id("hero")].into()),
            },
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            commands,
            [
                "start hero oath",
                "choose hero oath take",
                "choose hero oath take --unseen",
                "choose hero oath take --seen-by hero,rook",
            ]
        );
    }

    #[test]
    fn the_log_records_its_events_in_order() {
        let mut world = world();
        let mut log = log();
        assert!(!log.has_progress());
        run(
            &mut log,
            &mut world,
            vec![start("hero", "alone"), pick("hero", "alone", "done")],
        );
        assert!(log.has_progress());
        let events: Vec<String> = log.events().iter().map(ToString::to_string).collect();
        assert_eq!(
            events,
            [
                "hero started alone",
                "hero reached alone.go",
                "hero chose alone.go.done",
                "hero finished alone",
            ]
        );
        assert_eq!(log.quests().quests.len(), 8);
    }
}

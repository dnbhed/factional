//! Quests, their stages and choices, and questlines of steps (DESIGN.md §17.1).

use std::collections::BTreeMap;
use std::fmt;

use factional_core::{Fixed, InvalidId, id_type};
use factional_reputation::{Effects, FactionId, OutcomeId, RankId};

id_type!(
    /// A quest's id, such as `watch_oath`.
    QuestId
);

id_type!(
    /// A stage's id, unique within its quest, such as `patrol`.
    StageId
);

id_type!(
    /// A choice's id, unique within its stage, such as `report`.
    ChoiceId
);

id_type!(
    /// A questline's id, such as `watch_career`.
    QuestlineId
);

id_type!(
    /// A faction or a character, by its id, as a giver or a standing requirement names it.
    /// Factions and characters share one set of ids (P-35), so an id alone says which.
    PartyRef
);

/// Every quest and questline in a world's content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quests {
    pub quests: BTreeMap<QuestId, Quest>,
    pub questlines: BTreeMap<QuestlineId, Questline>,
}

impl Quests {
    /// Whether there are no quests and no questlines.
    pub fn is_empty(&self) -> bool {
        self.quests.is_empty() && self.questlines.is_empty()
    }

    /// Whose quest it is: its own giver, or else its questline's (D-28); `None` for the
    /// world's own, or a quest that doesn't exist.
    pub fn giver_of(&self, quest: &QuestId) -> Option<&PartyRef> {
        let own = self.quests.get(quest)?.giver.as_ref();
        own.or_else(|| {
            let (line, _) = self.place_of(quest)?;
            self.questlines[line].giver.as_ref()
        })
    }

    /// The questline and step a quest is in, if it's in one: the first place it's listed.
    pub fn place_of(&self, quest: &QuestId) -> Option<(&QuestlineId, usize)> {
        self.questlines.iter().find_map(|(id, line)| {
            line.steps
                .iter()
                .position(|step| step.quests.contains(quest))
                .map(|step| (id, step))
        })
    }
}

/// A list of stages, each with requirements and choices (D-25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: QuestId,
    pub name: String,
    /// Whose quest it is: a faction or a character; `None` for the world's own (D-28).
    pub giver: Option<PartyRef>,
    /// What must hold to start it.
    pub requires: Requirements,
    pub stages: Vec<Stage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub id: StageId,
    /// What must hold to reach it.
    pub requires: Requirements,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: ChoiceId,
    pub effects: ChoiceEffects,
    pub next: Next,
}

/// What making a choice does to the character who makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceEffects {
    None,
    /// An outcome from `outcomes.toml`.
    Outcome(OutcomeId),
    /// Effects written on the choice, of the kinds an outcome has.
    Inline(Effects),
}

/// Where a choice leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// A later stage of the same quest.
    Stage(StageId),
    /// The end of the quest.
    End,
}

impl Next {
    /// How content writes the end of a quest: `next = "end"`.
    pub const END: &'static str = "end";
}

/// What must hold for the character doing a quest: to start it, to reach a stage, or to
/// start a quest at a step of a questline. All of them must hold (P-64).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Requirements {
    /// Standing at least this with each faction or character.
    pub standing: BTreeMap<PartyRef, Fixed>,
    pub member: Vec<FactionId>,
    pub not_member: Vec<FactionId>,
    /// A rank at least this on each faction's ladder.
    pub rank_at_least: BTreeMap<FactionId, RankId>,
    /// Within each faction's member tolerance, as the faction pictures them.
    pub within_tolerance: Vec<FactionId>,
    /// Quests finished, or stages reached, or choices made.
    pub done: Vec<Progress>,
}

impl Requirements {
    /// The keys a `requires` table may have.
    pub const KEYS: [&'static str; 6] = [
        "standing",
        "member",
        "not_member",
        "rank_at_least",
        "within_tolerance",
        "done",
    ];

    /// Whether nothing is required.
    pub fn is_empty(&self) -> bool {
        *self == Requirements::default()
    }
}

/// Progress through another quest: finished, a stage reached, or a choice made. Written
/// `quest`, `quest.stage` or `quest.stage.choice`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Progress {
    Quest(QuestId),
    Stage(QuestId, StageId),
    Choice(QuestId, StageId, ChoiceId),
}

/// Text that isn't `quest`, `quest.stage` or `quest.stage.choice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressError {
    /// One of its parts isn't a valid id.
    Invalid(InvalidId),
    /// More than three parts.
    TooLong(String),
}

impl Progress {
    /// Reads `quest`, `quest.stage` or `quest.stage.choice`.
    pub fn parse(text: &str) -> Result<Progress, ProgressError> {
        match text.split('.').collect::<Vec<_>>()[..] {
            [quest] => Ok(Progress::Quest(quest_id(quest)?)),
            [quest, stage] => Ok(Progress::Stage(quest_id(quest)?, stage_id(stage)?)),
            [quest, stage, choice] => Ok(Progress::Choice(
                quest_id(quest)?,
                stage_id(stage)?,
                ChoiceId::new(choice).map_err(ProgressError::Invalid)?,
            )),
            _ => Err(ProgressError::TooLong(text.to_owned())),
        }
    }

    /// The quest it's about.
    pub fn quest(&self) -> &QuestId {
        match self {
            Progress::Quest(quest) | Progress::Stage(quest, _) | Progress::Choice(quest, ..) => {
                quest
            }
        }
    }
}

fn quest_id(text: &str) -> Result<QuestId, ProgressError> {
    QuestId::new(text).map_err(ProgressError::Invalid)
}

fn stage_id(text: &str) -> Result<StageId, ProgressError> {
    StageId::new(text).map_err(ProgressError::Invalid)
}

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Progress::Quest(quest) => write!(f, "{quest}"),
            Progress::Stage(quest, stage) => write!(f, "{quest}.{stage}"),
            Progress::Choice(quest, stage, choice) => write!(f, "{quest}.{stage}.{choice}"),
        }
    }
}

impl fmt::Display for ProgressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProgressError::Invalid(invalid) => invalid.fmt(f),
            ProgressError::TooLong(text) => write!(
                f,
                "'{text}' has too many parts: write quest, quest.stage or quest.stage.choice"
            ),
        }
    }
}

/// An ordered list of steps, each a group of quests (D-29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Questline {
    pub id: QuestlineId,
    pub name: String,
    /// Whose questline it is; `None` for the world's own. Its quests are the giver's unless
    /// they name their own (D-28).
    pub giver: Option<PartyRef>,
    pub steps: Vec<Step>,
}

/// Quests open together, done in any order (D-29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub quests: Vec<QuestId>,
    /// How many of them must be done to move on; `None` for all of them.
    pub need: Option<usize>,
    /// Added to the gate of every quest in the step.
    pub requires: Requirements,
    pub leftovers: Leftovers,
}

impl Step {
    /// How many of its quests must be done to move on.
    pub fn needed(&self) -> usize {
        self.need.unwrap_or(self.quests.len())
    }
}

/// What happens to a step's quests left undone once the character moves on (D-29, P-66).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Leftovers {
    /// They stay available.
    #[default]
    Open,
    /// They close when the character starts a quest of the next step.
    Close,
}

impl Leftovers {
    pub const ALL: [Leftovers; 2] = [Leftovers::Open, Leftovers::Close];

    /// How content writes it.
    pub fn key(self) -> &'static str {
        match self {
            Leftovers::Open => "open",
            Leftovers::Close => "close",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest(id: &str, giver: Option<&str>) -> Quest {
        Quest {
            id: QuestId::new(id).expect("valid id"),
            name: id.to_owned(),
            giver: giver.map(|giver| PartyRef::new(giver).expect("valid id")),
            requires: Requirements::default(),
            stages: Vec::new(),
        }
    }

    fn step(quests: &[&str]) -> Step {
        Step {
            quests: quests
                .iter()
                .map(|id| QuestId::new(id).expect("valid id"))
                .collect(),
            need: None,
            requires: Requirements::default(),
            leftovers: Leftovers::Open,
        }
    }

    /// `oath` and `errand` in the Watch's `career`, at steps 0 and 1; `errand` is Hale's own;
    /// `winter` is in no questline and has no giver.
    fn quests() -> Quests {
        let career = Questline {
            id: QuestlineId::new("career").expect("valid id"),
            name: "A Career".to_owned(),
            giver: Some(PartyRef::new("watch").expect("valid id")),
            steps: vec![step(&["oath"]), step(&["errand", "oath"])],
        };
        Quests {
            quests: [
                quest("oath", None),
                quest("errand", Some("hale")),
                quest("winter", None),
            ]
            .into_iter()
            .map(|quest| (quest.id.clone(), quest))
            .collect(),
            questlines: [(career.id.clone(), career)].into(),
        }
    }

    fn id(text: &str) -> QuestId {
        QuestId::new(text).expect("valid id")
    }

    #[test]
    fn a_quest_belongs_to_its_own_giver_or_else_its_questlines() {
        let quests = quests();
        let giver = |quest: &str| quests.giver_of(&id(quest)).map(PartyRef::as_str);
        assert_eq!(giver("oath"), Some("watch"));
        assert_eq!(giver("errand"), Some("hale"));
        assert_eq!(giver("winter"), None);
        assert_eq!(giver("nowhere"), None);
    }

    #[test]
    fn a_quests_place_is_the_first_step_that_lists_it() {
        let quests = quests();
        let place = |quest: &str| {
            quests
                .place_of(&id(quest))
                .map(|(line, step)| (line.as_str(), step))
        };
        assert_eq!(place("oath"), Some(("career", 0)));
        assert_eq!(place("errand"), Some(("career", 1)));
        assert_eq!(place("winter"), None);
    }

    #[test]
    fn quests_are_empty_only_without_quests_and_questlines() {
        assert!(Quests::default().is_empty());
        let mut quests = quests();
        assert!(!quests.is_empty());
        quests.quests.clear();
        assert!(!quests.is_empty());
        let mut only_quests = self::quests();
        only_quests.questlines.clear();
        assert!(!only_quests.is_empty());
    }

    #[test]
    fn a_step_needs_all_its_quests_unless_it_says_how_many() {
        let mut both = step(&["oath", "errand"]);
        assert_eq!(both.needed(), 2);
        both.need = Some(0);
        assert_eq!(both.needed(), 0);
    }

    #[test]
    fn nothing_is_required_by_default() {
        assert!(Requirements::default().is_empty());
        let requires = Requirements {
            member: vec![FactionId::new("watch").expect("valid id")],
            ..Requirements::default()
        };
        assert!(!requires.is_empty());
    }

    #[test]
    fn progress_is_a_quest_a_stage_or_a_choice_written_with_dots() {
        let parsed: Vec<String> = ["oath", "oath.patrol", "oath.patrol.report"]
            .into_iter()
            .map(|text| Progress::parse(text).expect("valid").to_string())
            .collect();
        assert_eq!(parsed, ["oath", "oath.patrol", "oath.patrol.report"]);
        assert!(matches!(
            Progress::parse("oath.patrol"),
            Ok(Progress::Stage(..))
        ));
        assert!(matches!(
            Progress::parse("oath.patrol.report"),
            Ok(Progress::Choice(..))
        ));
        assert_eq!(
            Progress::parse("oath.patrol.report")
                .expect("valid")
                .quest(),
            &id("oath")
        );
        assert_eq!(
            Progress::parse("oath.Patrol").map_err(|error| error.to_string()),
            Err("'Patrol' isn't a valid id: use lowercase letters, digits and _, starting with a letter".to_owned())
        );
        assert_eq!(
            Progress::parse("a.b.c.d").map_err(|error| error.to_string()),
            Err(
                "'a.b.c.d' has too many parts: write quest, quest.stage or quest.stage.choice"
                    .to_owned()
            )
        );
        assert!(Progress::parse("oath..report").is_err());
        assert!(Progress::parse("x.y.Z").is_err());
    }

    #[test]
    fn leftovers_are_written_open_or_close() {
        assert_eq!(Leftovers::ALL.map(Leftovers::key), ["open", "close"]);
        assert_eq!(Leftovers::default(), Leftovers::Open);
    }
}

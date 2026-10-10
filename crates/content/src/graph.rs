//! Quests as graphs (U4, P-80): each questline's steps and each quest's stages and choices,
//! the edges between quests, and every quest problem and warning at the questline, step,
//! quest, stage or choice its key names. It's made from the quests as far as they read, so
//! it shows while the world doesn't load. The editor draws it, and the CLI's `graph` prints
//! it; neither works out an edge or places a diagnostic itself.

use std::fmt;

use std::collections::BTreeSet;

use factional_quests::{ChoiceId, Lock, Progress, Quest, QuestId, Questline, Quests, StageId};

use crate::outline::sources;
use crate::quests::{QUESTLINES_FILE, QUESTS_FILE};
use crate::{ContentTexts, Diagnostic, Outline, read_all};

/// The quests as far as they read, their edges, and what the loader says about them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestGraph {
    pub quests: Quests,
    /// In quest id order: each quest's gate, then its stages' requirements, then its
    /// choices' locks.
    pub edges: Vec<Edge>,
    /// Every problem and warning in `quests.toml` and `questlines.toml`, problems first.
    notes: Vec<Note>,
}

/// One problem or warning, kept with which it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub diagnostic: Diagnostic,
    pub problem: bool,
}

/// An edge from one quest to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edge {
    /// A quest's gate, or one of its stages, needs progress in another quest.
    Needs {
        quest: QuestId,
        stage: Option<StageId>,
        done: Progress,
    },
    /// A choice declares it can lock another quest's gate or stage (D-26).
    Locks {
        quest: QuestId,
        stage: StageId,
        choice: ChoiceId,
        lock: Lock,
    },
}

/// A questline as drawn: its steps, the quests outside it at the other end of its edges,
/// and those edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineView<'g> {
    pub line: &'g Questline,
    /// In id order.
    pub outside: Vec<&'g Quest>,
    pub edges: Vec<&'g Edge>,
}

impl QuestGraph {
    /// The quests in `texts` as far as they read, with the diagnostics `outline` placed in
    /// their files.
    pub fn new(texts: &ContentTexts, outline: &Outline) -> QuestGraph {
        let quests = read_all(sources(texts)).quests;
        let edges = quests.quests.values().flat_map(edges_of).collect();
        let files = outline
            .files
            .iter()
            .filter(|file| [QUESTS_FILE, QUESTLINES_FILE].contains(&file.name));
        let mut problems = Vec::new();
        let mut warnings = Vec::new();
        for file in files {
            problems.extend(file.problems.iter());
            warnings.extend(file.warnings.iter());
            for entry in &file.entries {
                problems.extend(entry.problems.iter());
                warnings.extend(entry.warnings.iter());
            }
        }
        let note = |problem: bool| {
            move |diagnostic: &Diagnostic| Note {
                diagnostic: diagnostic.clone(),
                problem,
            }
        };
        let notes = problems
            .into_iter()
            .map(note(true))
            .chain(warnings.into_iter().map(note(false)))
            .collect();
        QuestGraph {
            quests,
            edges,
            notes,
        }
    }

    /// The questline `id` as drawn; `None` if there's no such questline.
    pub fn line(&self, id: &str) -> Option<LineView<'_>> {
        let line = self
            .quests
            .questlines
            .values()
            .find(|l| l.id.as_str() == id)?;
        let members: BTreeSet<&QuestId> = line.steps.iter().flat_map(|s| &s.quests).collect();
        let edges: Vec<&Edge> = self
            .edges
            .iter()
            .filter(|edge| members.contains(edge.from()) || members.contains(edge.to()))
            .collect();
        let others: BTreeSet<&QuestId> = edges
            .iter()
            .flat_map(|edge| [edge.from(), edge.to()])
            .filter(|quest| !members.contains(quest))
            .collect();
        let outside = others
            .into_iter()
            .filter_map(|quest| self.quests.quests.get(quest))
            .collect();
        Some(LineView {
            line,
            outside,
            edges,
        })
    }

    /// The quests in no questline, in id order.
    pub fn loose(&self) -> Vec<&Quest> {
        self.quests
            .quests
            .values()
            .filter(|quest| self.quests.place_of(&quest.id).is_none())
            .collect()
    }

    /// Everything the loader says about a questline, its steps and the quests in them.
    pub fn in_line(&self, line: &str) -> Vec<&Note> {
        let mut notes: Vec<&Note> = self.in_node(QUESTLINES_FILE, line.to_owned()).collect();
        if let Some(found) = self
            .quests
            .questlines
            .values()
            .find(|l| l.id.as_str() == line)
        {
            let quests: BTreeSet<&QuestId> = found.steps.iter().flat_map(|s| &s.quests).collect();
            for quest in quests {
                notes.extend(self.in_quest(quest.as_str()));
            }
        }
        notes
    }

    /// What the loader says about a questline itself, not one of its steps.
    pub fn at_line(&self, line: &str) -> Vec<&Note> {
        let steps = format!("{line}.steps[");
        self.in_node(QUESTLINES_FILE, line.to_owned())
            .filter(|note| !note.key().starts_with(&steps))
            .collect()
    }

    /// What it says about the `step`th step of a questline, from 0.
    pub fn at_step(&self, line: &str, step: usize) -> Vec<&Note> {
        self.in_node(QUESTLINES_FILE, format!("{line}.steps[{step}]"))
            .collect()
    }

    /// Everything it says about a quest, its stages and its choices.
    pub fn in_quest(&self, quest: &str) -> Vec<&Note> {
        self.in_node(QUESTS_FILE, quest.to_owned()).collect()
    }

    /// What it says about a quest itself, not one of its stages.
    pub fn at_quest(&self, quest: &str) -> Vec<&Note> {
        let stages = format!("{quest}.stages[");
        self.in_node(QUESTS_FILE, quest.to_owned())
            .filter(|note| !note.key().starts_with(&stages))
            .collect()
    }

    /// What it says about the `stage`th stage of a quest, not one of its choices.
    pub fn at_stage(&self, quest: &str, stage: usize) -> Vec<&Note> {
        let node = format!("{quest}.stages[{stage}]");
        let choices = format!("{node}.choices[");
        self.in_node(QUESTS_FILE, node)
            .filter(|note| !note.key().starts_with(&choices))
            .collect()
    }

    /// What it says about the `choice`th choice of a quest's `stage`th stage.
    pub fn at_choice(&self, quest: &str, stage: usize, choice: usize) -> Vec<&Note> {
        let node = format!("{quest}.stages[{stage}].choices[{choice}]");
        self.in_node(QUESTS_FILE, node).collect()
    }

    /// The notes in `file` at `node` or anything in it.
    fn in_node(&self, file: &'static str, node: String) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(move |note| note.diagnostic.file == file && within(note.key(), &node))
    }
}

/// Whether `key` is `node` or something in it: `siege.stages[1].requires` is in
/// `siege.stages[1]` and in `siege`, but not in `siege.stages[10]`.
fn within(key: &str, node: &str) -> bool {
    key.strip_prefix(node)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// A quest's edges: its gate's needs, then its stages', then what its choices lock.
fn edges_of(quest: &Quest) -> Vec<Edge> {
    let needs = |stage: Option<&StageId>, done: &Progress| Edge::Needs {
        quest: quest.id.clone(),
        stage: stage.cloned(),
        done: done.clone(),
    };
    let mut edges: Vec<Edge> = quest.requires.done.iter().map(|d| needs(None, d)).collect();
    for stage in &quest.stages {
        edges.extend(
            stage
                .requires
                .done
                .iter()
                .map(|d| needs(Some(&stage.id), d)),
        );
    }
    for stage in &quest.stages {
        for choice in &stage.choices {
            edges.extend(choice.locks.iter().map(|lock| Edge::Locks {
                quest: quest.id.clone(),
                stage: stage.id.clone(),
                choice: choice.id.clone(),
                lock: lock.clone(),
            }));
        }
    }
    edges
}

impl Edge {
    /// The quest it comes from: the one whose progress is needed, or the one locking.
    pub fn from(&self) -> &QuestId {
        match self {
            Edge::Needs { done, .. } => done.quest(),
            Edge::Locks { quest, .. } => quest,
        }
    }

    /// The quest it goes to: the one that needs, or the one locked.
    pub fn to(&self) -> &QuestId {
        match self {
            Edge::Needs { quest, .. } => quest,
            Edge::Locks { lock, .. } => match lock {
                Lock::Gate(quest) | Lock::Stage(quest, _) => quest,
            },
        }
    }
}

impl Note {
    /// `error: requires.done[0]: …`, with where it is within `node` if it's deeper than the
    /// node itself, such as `siege.stages[1]`.
    pub fn said_at(&self, node: &str) -> String {
        let kind = if self.problem { "error" } else { "warning" };
        let message = &self.diagnostic.message;
        let at = self
            .diagnostic
            .key
            .as_deref()
            .and_then(|key| key.strip_prefix(node))
            .and_then(|rest| rest.strip_prefix('.'));
        match at {
            Some(at) => format!("{kind}: {at}: {message}"),
            None => format!("{kind}: {message}"),
        }
    }

    /// Its key, or nothing for one about a whole file.
    fn key(&self) -> &str {
        self.diagnostic.key.as_deref().unwrap_or_default()
    }
}

/// `smugglers_cove needs watch_oath.patrol.report done`, or
/// `circle_rite.whisper.set_them_at_war locks watch_captain`.
impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Edge::Needs { quest, stage, done } => {
                match stage {
                    Some(stage) => write!(f, "{quest}.{stage}")?,
                    None => write!(f, "{quest}")?,
                }
                write!(f, " needs {done} done")
            }
            Edge::Locks {
                quest,
                stage,
                choice,
                lock,
            } => write!(f, "{quest}.{stage}.{choice} locks {lock}"),
        }
    }
}

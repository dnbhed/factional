//! The quest module (DESIGN.md §17): quests of stages and choices, questlines of steps, and
//! the checks that keep them complete (D-20). It reads the reputation module's content to
//! check itself, and changes a world only through that module's commands (§11).
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Rule arithmetic uses fixed-point numbers, never floats (DECISIONS.md P-1).
#![deny(clippy::float_arithmetic)]

mod check;
mod lockout;
mod quest;
mod reach;

pub use check::{
    Blocker, ChoiceAt, Gate, Needs, Owner, QuestProblem, QuestWarning, RequirementKey,
};
pub use lockout::{LockReason, Needed};
pub use quest::{
    Choice, ChoiceEffects, ChoiceId, Leftovers, Lock, LockError, Next, PartyRef, Progress,
    ProgressError, Quest, QuestId, Questline, QuestlineId, Quests, Requirements, Stage, StageId,
    Step,
};

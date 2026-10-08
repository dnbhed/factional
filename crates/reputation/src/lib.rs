//! The reputation & factions module: alignment, standing, rank, factions and disposition
//! (DESIGN.md). State changes only by executing commands, and every change is reported as an
//! event (DESIGN.md §11).
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Rule arithmetic uses fixed-point numbers, never floats (DECISIONS.md P-1).
#![deny(clippy::float_arithmetic)]

mod action;
mod alignment;
mod character;
mod command;
mod defection;
mod disposition;
mod distance;
mod faction;
mod id;
mod inertia;
mod knowledge;
mod membership;
mod relation;
mod shift;
mod standing;
mod world;

pub use action::{Action, TargetCurve, Witnesses};
pub use alignment::{AXIS_LIMIT, Alignment, AlignmentDelta, Axis, AxisOutOfRange};
pub use character::Character;
pub use command::{
    Change, Command, CommandError, Event, JournalEntry, RestoreError, Role, SavedCommand,
};
pub use defection::{
    Condition, ConditionCheck, Defection, Observed, RankRef, Rule, RuleTried, TableDecision,
    TableKind, TableOwner, TableProblem, TableSource, Verdict,
};
pub use disposition::{
    AppliedModifier, Band, BandProblem, Bands, Component, ComponentKind, Disposition,
    DispositionWeights, ModifierObserver, Part,
};
pub use distance::{Metric, WeightProblem, Weights, measure};
pub use faction::{Faction, Rank};
pub use id::{
    ActionId, CharacterId, FactionId, InvalidId, ModifierId, OutcomeId, ProfileId, RankId,
};
pub use inertia::{Inertia, InertiaProfile, Toward};
pub use knowledge::{KnowledgeModel, Learned, News, NextHop, Perception, Reached, Ripple};
pub use membership::{
    ConflictRule, Consequence, DriftPolicy, JoinAssessment, JoinBlock, LeaveReason, Membership,
    PromotionAssessment, RankCheck, StartingMembership, ToleranceProblem, Tolerances,
};
pub use relation::{Regard, Relation, RelationEnds, RelationShift, RelationSide, ShiftProblem};
pub use shift::{AxisShift, Shift, TargetRelation};
pub use standing::{
    ActionStanding, Effects, Outcome, Party, Spill, StandingEffects, StandingKey, StandingOwner,
};
pub use world::{
    Balance, Content, ContentProblem, ContentWarning, Distance, Observer, ProfileUser, RankKey,
    WeightsFrom, World,
};

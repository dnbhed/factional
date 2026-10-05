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
mod disposition;
mod distance;
mod faction;
mod id;
mod membership;
mod relation;
mod standing;
mod world;

pub use action::{Action, Witnesses};
pub use alignment::{AXIS_LIMIT, Alignment, AlignmentDelta, Axis, AxisOutOfRange};
pub use character::Character;
pub use command::{Change, Command, CommandError, Event, JournalEntry, Role};
pub use disposition::{
    Band, BandProblem, Bands, Component, ComponentKind, Disposition, DispositionWeights, Part,
};
pub use distance::{Metric, WeightProblem, Weights, measure};
pub use faction::Faction;
pub use id::{ActionId, CharacterId, FactionId, InvalidId, OutcomeId};
pub use membership::{
    JoinAssessment, JoinBlock, LeaveReason, Membership, ToleranceProblem, Tolerances,
};
pub use relation::{Regard, Relation, RelationEnds, RelationSide};
pub use standing::{
    ActionStanding, Effects, Outcome, Party, StandingEffects, StandingKey, StandingOwner,
};
pub use world::{
    Balance, Content, ContentProblem, ContentWarning, Distance, Observer, WeightsFrom, World,
};

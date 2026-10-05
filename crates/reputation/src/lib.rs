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
mod world;

pub use action::{Action, Witnesses};
pub use alignment::{AXIS_LIMIT, Alignment, AlignmentDelta, Axis, AxisOutOfRange};
pub use character::Character;
pub use command::{Change, Command, CommandError, Event, JournalEntry, Role};
pub use disposition::{Band, BandProblem, Bands, Disposition};
pub use distance::{Metric, WeightProblem, Weights, measure};
pub use faction::Faction;
pub use id::{ActionId, CharacterId, FactionId, InvalidId};
pub use world::{Balance, Content, ContentProblem, Distance, Observer, WeightsFrom, World};

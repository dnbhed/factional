//! Shared primitives for every Factional module: fixed-point numbers, curves, ids, time and
//! the event envelope (DESIGN.md §15).
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Rule arithmetic uses fixed-point numbers, never floats (DECISIONS.md P-1).
#![deny(clippy::float_arithmetic)]

mod curve;
mod fixed;
mod text;
mod time;

pub use curve::{Curve, CurveError};
pub use fixed::{Fixed, ParseFixedError, div_round};
pub use text::{article, is_valid_id, suggest};
pub use time::{Envelope, Tick};

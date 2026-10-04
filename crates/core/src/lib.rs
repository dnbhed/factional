//! Shared primitives for every Factional module: fixed-point numbers, curves, ids, time and
//! the event envelope (DESIGN.md §15).
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Rule arithmetic uses fixed-point numbers, never floats (DECISIONS.md P-1).
#![deny(clippy::float_arithmetic)]

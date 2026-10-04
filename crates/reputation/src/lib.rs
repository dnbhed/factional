//! The reputation & factions module: alignment, standing, rank, factions and disposition
//! (DESIGN.md). State changes only by executing commands, and every change is reported as an
//! event (DESIGN.md §11).
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Rule arithmetic uses fixed-point numbers, never floats (DECISIONS.md P-1).
#![deny(clippy::float_arithmetic)]

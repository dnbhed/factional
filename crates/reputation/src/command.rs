use std::fmt;

use factional_core::{Envelope, Tick};

/// A request to change the world: the only way in (DESIGN.md §2, §11.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Moves time forward. The host's game loop sends it; the engine never reads a clock.
    AdvanceTime { ticks: u64 },
}

/// What changed, carried in an [`Event`]. Events hold absolute before-and-after values, so
/// they read without context and replay without rules (P-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    TimeAdvanced { from: Tick, to: Tick },
}

/// Something that happened in the world, numbered and stamped with when it happened.
pub type Event = Envelope<Change>;

/// Why a command was refused. A refused command changes nothing and emits nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// `AdvanceTime` by zero ticks.
    NoTicks,
    /// `AdvanceTime` past the last tick there is.
    TimeOverflow { now: Tick, ticks: u64 },
}

/// One command as it was issued, and whether the world accepted it (P-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub command: Command,
    pub result: Result<(), CommandError>,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NoTicks => f.write_str("ticks must be at least 1"),
            CommandError::TimeOverflow { now, ticks } => write!(
                f,
                "time can't advance {ticks} ticks from tick {now}: it would pass the last tick"
            ),
        }
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_refusals_in_plain_words() {
        assert_eq!(
            CommandError::NoTicks.to_string(),
            "ticks must be at least 1"
        );
        assert_eq!(
            CommandError::TimeOverflow {
                now: Tick(10),
                ticks: u64::MAX,
            }
            .to_string(),
            format!(
                "time can't advance {} ticks from tick 10: it would pass the last tick",
                u64::MAX
            )
        );
    }
}

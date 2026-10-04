use std::fmt;

/// A moment in abstract game time. The host advances it and decides how long a tick is; the
/// engine never reads a clock (DESIGN.md §2, P-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Tick(pub u64);

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Something that happened, with its place in the world's history: `seq` counts events from
/// 1, and `tick` is the time when it happened (DESIGN.md §11.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope<P> {
    pub seq: u64,
    pub tick: Tick,
    pub payload: P,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_displays_as_its_number() {
        assert_eq!(Tick(42).to_string(), "42");
    }

    #[test]
    fn ticks_order_by_time() {
        assert!(Tick(1) < Tick(2));
        assert_eq!(Tick::default(), Tick(0));
    }
}

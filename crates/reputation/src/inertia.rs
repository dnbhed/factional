use std::collections::BTreeMap;

use factional_core::{Curve, Fixed};

use crate::{Axis, ProfileId};

/// Which way a shift moves an axis, naming one of a profile's four curves (DESIGN.md §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Toward {
    Lawful,
    Chaotic,
    Good,
    Evil,
}

impl Toward {
    pub const ALL: [Toward; 4] = [Toward::Lawful, Toward::Chaotic, Toward::Good, Toward::Evil];

    pub fn axis(self) -> Axis {
        match self {
            Toward::Lawful | Toward::Chaotic => Axis::Law,
            Toward::Good | Toward::Evil => Axis::Good,
        }
    }

    /// Its key under the axis in content, such as `toward_good` in `good.toward_good`.
    pub fn key(self) -> &'static str {
        match self {
            Toward::Lawful => "toward_lawful",
            Toward::Chaotic => "toward_chaotic",
            Toward::Good => "toward_good",
            Toward::Evil => "toward_evil",
        }
    }

    /// The direction a shift of `base` moves `axis`; `None` if it doesn't move it.
    pub fn of(axis: Axis, base: Fixed) -> Option<Toward> {
        let up = match axis {
            Axis::Law => Toward::Lawful,
            Axis::Good => Toward::Good,
        };
        let down = match axis {
            Axis::Law => Toward::Chaotic,
            Axis::Good => Toward::Evil,
        };
        match base.cmp(&Fixed::ZERO) {
            std::cmp::Ordering::Greater => Some(up),
            std::cmp::Ordering::Less => Some(down),
            std::cmp::Ordering::Equal => None,
        }
    }
}

/// How hard a character's alignment is to move: a multiplier curve per axis and direction,
/// read at their position before the act (DESIGN.md §5.3, P-5). A curve left out is 1.0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InertiaProfile {
    pub curves: BTreeMap<Toward, Curve>,
}

/// `[inertia]` in `balance.toml`: the profiles, and the one a character without their own
/// uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inertia {
    pub default_profile: ProfileId,
    pub profiles: BTreeMap<ProfileId, InertiaProfile>,
}

impl Inertia {
    /// The built-in profile, whose every curve is 1.0: every act counts in full.
    pub const STEADY: &str = "steady";

    pub fn steady() -> ProfileId {
        ProfileId::new(Inertia::STEADY).expect("a valid id")
    }
}

impl Default for Inertia {
    /// Only `steady`, the default.
    fn default() -> Inertia {
        Inertia {
            default_profile: Inertia::steady(),
            profiles: [(Inertia::steady(), InertiaProfile::default())].into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    #[test]
    fn a_shift_names_its_direction() {
        assert_eq!(Toward::of(Axis::Law, h(1)), Some(Toward::Lawful));
        assert_eq!(Toward::of(Axis::Law, h(-1)), Some(Toward::Chaotic));
        assert_eq!(Toward::of(Axis::Good, h(1)), Some(Toward::Good));
        assert_eq!(Toward::of(Axis::Good, h(-1)), Some(Toward::Evil));
        assert_eq!(Toward::of(Axis::Good, Fixed::ZERO), None);
        assert_eq!(
            Toward::ALL.map(|toward| (toward.axis().key(), toward.key())),
            [
                ("law", "toward_lawful"),
                ("law", "toward_chaotic"),
                ("good", "toward_good"),
                ("good", "toward_evil"),
            ]
        );
    }

    #[test]
    fn the_default_is_steady_and_always_there() {
        let inertia = Inertia::default();
        assert_eq!(inertia.default_profile.as_str(), "steady");
        assert_eq!(
            inertia.profiles.get(&Inertia::steady()),
            Some(&InertiaProfile::default())
        );
    }
}

use std::collections::BTreeMap;

use factional_core::{Curve, Fixed, Ratio};

use crate::{AXIS_LIMIT, Alignment, AlignmentDelta, Axis, ProfileId};

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

/// The working behind one axis's move (DESIGN.md §5.2):
/// `shift = round(base × scale × inertia)`, then `to = clamp(from + shift)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisShift {
    pub axis: Axis,
    pub from: Fixed,
    pub base: Fixed,
    pub scale: Fixed,
    /// The profile's curve for this direction and its exact value at `from`; `None` if the
    /// profile leaves that curve out, so it's 1.0.
    pub inertia: Option<(Toward, Ratio)>,
    pub shift: Fixed,
    pub to: Fixed,
}

/// How an act moves a character: each axis it touches, law first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shift {
    /// The inertia profile that applied.
    pub profile: ProfileId,
    pub axes: Vec<AxisShift>,
}

/// Each axis `delta` touches, moved by `delta` × `scale` with `profile`'s inertia: the shift
/// is computed exactly and rounded once, then the axis is clamped to −100.00…100.00. A shift
/// too big to compute lands at the end of the axis.
pub(crate) fn shifts(
    from: Alignment,
    delta: AlignmentDelta,
    scale: Fixed,
    profile: &InertiaProfile,
) -> Vec<AxisShift> {
    [(Axis::Law, delta.law), (Axis::Good, delta.good)]
        .into_iter()
        .filter_map(|(axis, base)| {
            let toward = Toward::of(axis, base)?;
            let position = from.on(axis);
            let inertia = profile
                .curves
                .get(&toward)
                .map(|curve| (toward, curve.exact_at(position)));
            let multiplier = inertia.map_or(Ratio::ONE, |(_, value)| value);
            let shift = Ratio::from_fixed(base)
                .checked_mul(Ratio::from_fixed(scale))
                .and_then(|product| product.checked_mul(multiplier))
                .and_then(Ratio::round)
                // Multipliers are never negative, so a shift goes the way the act does.
                .unwrap_or(match toward {
                    Toward::Lawful | Toward::Good => Fixed::from_hundredths(i64::MAX),
                    Toward::Chaotic | Toward::Evil => Fixed::from_hundredths(i64::MIN),
                });
            // A sum too big to hold means a shift that reaches the end of the axis by itself.
            let to = position
                .checked_add(shift)
                .unwrap_or(shift)
                .clamp(-AXIS_LIMIT, AXIS_LIMIT);
            Some(AxisShift {
                axis,
                from: position,
                base,
                scale,
                inertia,
                shift,
                to,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn curve(points: &[(i64, i64)]) -> Curve {
        Curve::from_points(points.iter().map(|&(x, y)| (h(x), h(y))).collect()).expect("valid")
    }

    /// DESIGN.md §5.3's `hardening`: convictions harden toward the extremes.
    fn hardening() -> InertiaProfile {
        InertiaProfile {
            curves: [
                (
                    Toward::Good,
                    curve(&[(-100_00, 50), (0, 1_00), (100_00, 30)]),
                ),
                (
                    Toward::Evil,
                    curve(&[(-100_00, 30), (0, 1_00), (100_00, 50)]),
                ),
            ]
            .into(),
        }
    }

    fn at(law: i64, good: i64) -> Alignment {
        Alignment::new(h(law), h(good)).expect("in range")
    }

    fn good(value: i64) -> AlignmentDelta {
        AlignmentDelta {
            law: Fixed::ZERO,
            good: h(value),
        }
    }

    #[test]
    fn a_hardening_character_is_moved_less_the_further_they_lean() {
        let help = shifts(at(0, 60_00), good(4_00), Fixed::ONE, &hardening());
        assert_eq!(
            help,
            [AxisShift {
                axis: Axis::Good,
                from: h(60_00),
                base: h(4_00),
                scale: Fixed::ONE,
                inertia: Some((Toward::Good, Ratio::from_fixed(h(58)))),
                shift: h(232),
                to: h(6232),
            }]
        );
        let extort = shifts(at(0, 60_00), good(-6_00), Fixed::ONE, &hardening());
        assert_eq!(
            (extort[0].inertia, extort[0].shift, extort[0].to),
            (
                Some((Toward::Evil, Ratio::from_fixed(h(70)))),
                h(-4_20),
                h(55_80)
            )
        );
    }

    #[test]
    fn the_shift_is_rounded_once_not_the_multiplier_first() {
        // Sister Mira at good 85: toward_good is 0.405, so 4.00 × 0.405 = 1.62, not 1.64.
        let help = shifts(at(35_00, 85_00), good(4_00), Fixed::ONE, &hardening());
        assert_eq!((help[0].shift, help[0].to), (h(1_62), h(86_62)));
        assert_eq!(
            help[0]
                .inertia
                .map(|(_, multiplier)| multiplier.to_string()),
            Some("0.405".to_owned())
        );
        // Scale joins the same product: 4.00 × 0.50 × 0.405 = 0.81.
        let half = shifts(at(35_00, 85_00), good(4_00), h(50), &hardening());
        assert_eq!(half[0].shift, h(81));
    }

    #[test]
    fn a_curve_the_profile_leaves_out_counts_in_full() {
        let steal = AlignmentDelta {
            law: h(-5_00),
            good: h(-3_00),
        };
        let moved = shifts(at(75_00, 30_00), steal, Fixed::ONE, &hardening());
        // Law has no curves in hardening; good's toward_evil at 30 is 0.85: −2.55.
        assert_eq!(
            moved
                .iter()
                .map(|axis| (axis.axis, axis.inertia.is_some(), axis.shift))
                .collect::<Vec<_>>(),
            [(Axis::Law, false, h(-5_00)), (Axis::Good, true, h(-2_55))]
        );
        let steady = shifts(
            at(0, 60_00),
            good(4_00),
            Fixed::ONE,
            &InertiaProfile::default(),
        );
        assert_eq!((steady[0].inertia, steady[0].shift), (None, h(4_00)));
    }

    #[test]
    fn an_axis_the_act_does_not_touch_has_no_working() {
        let moved = shifts(at(0, 0), good(4_00), Fixed::ONE, &hardening());
        assert_eq!(moved.len(), 1);
        assert!(
            shifts(
                at(0, 0),
                AlignmentDelta::default(),
                Fixed::ONE,
                &hardening()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_multiplier_of_zero_stops_the_shift() {
        let frozen = InertiaProfile {
            curves: [(Toward::Good, Curve::constant(Fixed::ZERO))].into(),
        };
        let moved = shifts(at(0, 10_00), good(4_00), Fixed::ONE, &frozen);
        assert_eq!((moved[0].shift, moved[0].to), (Fixed::ZERO, h(10_00)));
    }

    #[test]
    fn a_shift_too_big_to_compute_lands_at_the_end_of_the_axis() {
        let huge = InertiaProfile {
            curves: [
                (Toward::Good, Curve::constant(h(i64::MAX))),
                (Toward::Evil, Curve::constant(h(i64::MAX))),
            ]
            .into(),
        };
        let up = shifts(at(0, 0), good(4_00), h(i64::MAX), &huge);
        assert_eq!((up[0].shift, up[0].to), (h(i64::MAX), AXIS_LIMIT));
        let down = shifts(at(0, 0), good(-4_00), h(i64::MAX), &huge);
        assert_eq!((down[0].shift, down[0].to), (h(i64::MIN), -AXIS_LIMIT));
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

use std::collections::BTreeMap;

use factional_core::{Curve, Fixed, Ratio};

use crate::{
    AXIS_LIMIT, Alignment, AlignmentDelta, Axis, FactionId, InertiaProfile, ProfileId, TargetCurve,
    Toward,
};

/// The working behind one axis's move (DESIGN.md §5.2):
/// `shift = round(base × scale × inertia)`, then `to = clamp(from + shift)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisShift {
    pub axis: Axis,
    pub from: Fixed,
    pub base: Fixed,
    pub scale: Fixed,
    /// The action's `by_target.<axis>` curve at the target's position on this axis: that
    /// position and the curve's exact value there. `None` without a target or the curve.
    pub by_target: Option<(Fixed, Ratio)>,
    /// The action's `by_target.relation` curve at the relation between the two sides, and its
    /// exact value there. `None` without a target or the curve.
    pub by_relation: Option<(TargetRelation, Ratio)>,
    /// The profile's curve for this direction and its exact value at `from`; `None` if the
    /// profile leaves that curve out, so it's 1.0.
    pub inertia: Option<(Toward, Ratio)>,
    pub shift: Fixed,
    pub to: Fixed,
}

/// The relation `by_target.relation` is read at: the most hostile from any of the actor's
/// factions toward any of the target's (DESIGN.md §5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRelation {
    pub value: Fixed,
    /// The actor's faction and the target's it's between; `None` when no pair of different
    /// factions links the two sides, so it's 0.
    pub between: Option<(FactionId, FactionId)>,
}

/// Who an act is done to, for `by_target` (DESIGN.md §5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target<'a> {
    /// The action's `by_target` curves.
    pub curves: &'a BTreeMap<TargetCurve, Curve>,
    /// The target's alignment before the act.
    pub alignment: Alignment,
    pub relation: TargetRelation,
}

/// How an act moves a character: each axis it touches, law first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shift {
    /// The inertia profile that applied.
    pub profile: ProfileId,
    pub axes: Vec<AxisShift>,
}

impl Shift {
    /// `from` with each axis moved to where the working says.
    pub fn applied_to(&self, from: Alignment) -> Alignment {
        self.axes
            .iter()
            .fold(from, |moved, axis| moved.with(axis.axis, axis.to))
    }
}

/// Each axis `delta` touches, moved by `delta` × `scale`, scaled by who it's done to if
/// there's a `target`, and with `profile`'s inertia: the shift is computed exactly and
/// rounded once, then the axis is clamped to −100.00…100.00 (DESIGN.md §5.2). A shift too big
/// to compute lands at the end of the axis.
pub(crate) fn shifts(
    from: Alignment,
    delta: AlignmentDelta,
    scale: Fixed,
    target: Option<&Target<'_>>,
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
            let by_target = target.and_then(|target| {
                let on = target.alignment.on(axis);
                let curve = target.curves.get(&TargetCurve::on(axis))?;
                Some((on, curve.exact_at(on)))
            });
            let by_relation = target.and_then(|target| {
                let curve = target.curves.get(&TargetCurve::Relation)?;
                Some((
                    target.relation.clone(),
                    curve.exact_at(target.relation.value),
                ))
            });
            let multipliers = [
                Some(Ratio::from_fixed(scale)),
                by_target.map(|(_, value)| value),
                by_relation.as_ref().map(|(_, value)| *value),
                inertia.map(|(_, value)| value),
            ];
            let shift = multipliers
                .into_iter()
                .flatten()
                .try_fold(Ratio::from_fixed(base), Ratio::checked_mul)
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
                by_target,
                by_relation,
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
        let help = shifts(at(0, 60_00), good(4_00), Fixed::ONE, None, &hardening());
        assert_eq!(
            help,
            [AxisShift {
                axis: Axis::Good,
                from: h(60_00),
                base: h(4_00),
                scale: Fixed::ONE,
                by_target: None,
                by_relation: None,
                inertia: Some((Toward::Good, Ratio::from_fixed(h(58)))),
                shift: h(232),
                to: h(6232),
            }]
        );
        let extort = shifts(at(0, 60_00), good(-6_00), Fixed::ONE, None, &hardening());
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
        let help = shifts(at(35_00, 85_00), good(4_00), Fixed::ONE, None, &hardening());
        assert_eq!((help[0].shift, help[0].to), (h(1_62), h(86_62)));
        assert_eq!(
            help[0]
                .inertia
                .map(|(_, multiplier)| multiplier.to_string()),
            Some("0.405".to_owned())
        );
        // Scale joins the same product: 4.00 × 0.50 × 0.405 = 0.81.
        let half = shifts(at(35_00, 85_00), good(4_00), h(50), None, &hardening());
        assert_eq!(half[0].shift, h(81));
    }

    #[test]
    fn a_curve_the_profile_leaves_out_counts_in_full() {
        let steal = AlignmentDelta {
            law: h(-5_00),
            good: h(-3_00),
        };
        let moved = shifts(at(75_00, 30_00), steal, Fixed::ONE, None, &hardening());
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
            None,
            &InertiaProfile::default(),
        );
        assert_eq!((steady[0].inertia, steady[0].shift), (None, h(4_00)));
    }

    #[test]
    fn an_axis_the_act_does_not_touch_has_no_working() {
        let moved = shifts(at(0, 0), good(4_00), Fixed::ONE, None, &hardening());
        assert_eq!(moved.len(), 1);
        assert!(
            shifts(
                at(0, 0),
                AlignmentDelta::default(),
                Fixed::ONE,
                None,
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
        let moved = shifts(at(0, 10_00), good(4_00), Fixed::ONE, None, &frozen);
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
        let up = shifts(at(0, 0), good(4_00), h(i64::MAX), None, &huge);
        assert_eq!((up[0].shift, up[0].to), (h(i64::MAX), AXIS_LIMIT));
        let down = shifts(at(0, 0), good(-4_00), h(i64::MAX), None, &huge);
        assert_eq!((down[0].shift, down[0].to), (h(i64::MIN), -AXIS_LIMIT));
    }

    // Target-aware effects (DESIGN.md §5.4)

    /// `murder`: law −10.00, good −15.00.
    fn murder() -> AlignmentDelta {
        AlignmentDelta {
            law: h(-10_00),
            good: h(-15_00),
        }
    }

    /// `murder`'s `by_target` curves from DESIGN.md §5.4.
    fn murder_curves() -> BTreeMap<TargetCurve, Curve> {
        [
            (
                TargetCurve::Good,
                curve(&[(-100_00, 20), (0, 1_00), (100_00, 1_50)]),
            ),
            (
                TargetCurve::Relation,
                curve(&[(-100_00, 50), (-50_00, 80), (0, 1_00)]),
            ),
        ]
        .into()
    }

    fn unrelated() -> TargetRelation {
        TargetRelation {
            value: Fixed::ZERO,
            between: None,
        }
    }

    fn exactly(hundredths: i64) -> Ratio {
        Ratio::from_fixed(h(hundredths))
    }

    /// Each axis's target multipliers and shift.
    type Scaled = (Axis, Option<(Fixed, Ratio)>, Option<Ratio>, Fixed);

    fn scaled(axes: &[AxisShift]) -> Vec<Scaled> {
        axes.iter()
            .map(|axis| {
                (
                    axis.axis,
                    axis.by_target,
                    axis.by_relation.as_ref().map(|(_, value)| *value),
                    axis.shift,
                )
            })
            .collect()
    }

    #[test]
    fn killing_the_wicked_is_less_evil() {
        let curves = murder_curves();
        let ash = Target {
            curves: &curves,
            alignment: at(25_00, -70_00),
            relation: unrelated(),
        };
        let moved = shifts(
            at(0, 0),
            murder(),
            Fixed::ONE,
            Some(&ash),
            &InertiaProfile::default(),
        );
        // by_target.good at −70 is 0.44; no factions, so the relation curve is read at 0: 1.00.
        assert_eq!(
            scaled(&moved),
            [
                (Axis::Law, None, Some(Ratio::ONE), h(-10_00)),
                (
                    Axis::Good,
                    Some((h(-70_00), exactly(44))),
                    Some(Ratio::ONE),
                    h(-6_60)
                ),
            ]
        );
        assert_eq!(moved[1].by_relation, Some((unrelated(), Ratio::ONE)));
    }

    #[test]
    fn killing_a_saint_is_more_evil_rounded_once() {
        let curves = murder_curves();
        let mira = Target {
            curves: &curves,
            alignment: at(35_00, 85_00),
            relation: unrelated(),
        };
        let moved = shifts(
            at(0, 0),
            murder(),
            Fixed::ONE,
            Some(&mira),
            &InertiaProfile::default(),
        );
        // 1.425 at 85: −15.00 × 1.425 = −21.375, rounded once to −21.38.
        let good = &moved[1];
        assert_eq!(
            good.by_target.map(|(_, value)| value.to_string()),
            Some("1.425".into())
        );
        assert_eq!((good.shift, good.to), (h(-21_38), h(-21_38)));
    }

    #[test]
    fn the_target_relation_and_inertia_join_one_product() {
        let curves = murder_curves();
        let watch_on_guild = TargetRelation {
            value: h(-80_00),
            between: Some((
                FactionId::new("city_watch").expect("valid"),
                FactionId::new("lantern_guild").expect("valid"),
            )),
        };
        let vex = Target {
            curves: &curves,
            alignment: at(-55_00, -20_00),
            relation: watch_on_guild.clone(),
        };
        // Captain Hale, hardening, at 75 / 30.
        let moved = shifts(
            at(75_00, 30_00),
            murder(),
            Fixed::ONE,
            Some(&vex),
            &hardening(),
        );
        // Good: −15.00 × 0.84 × 0.62 × 0.85 = −6.6402 → −6.64. Law: −10.00 × 0.62 = −6.20.
        assert_eq!(
            scaled(&moved),
            [
                (Axis::Law, None, Some(exactly(62)), h(-6_20)),
                (
                    Axis::Good,
                    Some((h(-20_00), exactly(84))),
                    Some(exactly(62)),
                    h(-664)
                ),
            ]
        );
        assert_eq!(moved[1].by_relation, Some((watch_on_guild, exactly(62))));
        assert_eq!(moved[1].inertia, Some((Toward::Evil, exactly(85))));
        assert_eq!((moved[0].to, moved[1].to), (h(68_80), h(23_36)));
    }

    #[test]
    fn the_law_curve_reads_the_targets_law() {
        let curves: BTreeMap<TargetCurve, Curve> =
            [(TargetCurve::Law, curve(&[(-100_00, 2_00), (100_00, 0)]))].into();
        let outlaw = Target {
            curves: &curves,
            alignment: at(-50_00, 90_00),
            relation: unrelated(),
        };
        let moved = shifts(
            at(0, 0),
            murder(),
            Fixed::ONE,
            Some(&outlaw),
            &InertiaProfile::default(),
        );
        // 1.50 at law −50: −10.00 × 1.50 = −15.00; good has no curve of its own.
        assert_eq!(
            scaled(&moved),
            [
                (Axis::Law, Some((h(-50_00), exactly(1_50))), None, h(-15_00)),
                (Axis::Good, None, None, h(-15_00)),
            ]
        );
    }

    #[test]
    fn without_a_target_or_its_curves_an_act_is_unchanged() {
        let alone = shifts(
            at(0, 0),
            murder(),
            Fixed::ONE,
            None,
            &InertiaProfile::default(),
        );
        assert_eq!(
            scaled(&alone),
            [
                (Axis::Law, None, None, h(-10_00)),
                (Axis::Good, None, None, h(-15_00))
            ]
        );
        let none = BTreeMap::new();
        let plain = Target {
            curves: &none,
            alignment: at(25_00, -70_00),
            relation: unrelated(),
        };
        assert_eq!(
            shifts(
                at(0, 0),
                murder(),
                Fixed::ONE,
                Some(&plain),
                &InertiaProfile::default()
            ),
            alone
        );
    }

    use proptest::prelude::*;

    /// Random multiplier curves: points over −100…100 with values from 0 to 5, or one huge.
    fn multipliers() -> impl Strategy<Value = Curve> {
        let points = proptest::collection::btree_map(-100_i64..=100, 0_i64..=5_00, 2..5).prop_map(
            |points| {
                Curve::from_points(
                    points
                        .into_iter()
                        .map(|(x, y)| (h(x * 100), h(y)))
                        .collect(),
                )
                .expect("increasing x")
            },
        );
        prop_oneof![points, Just(Curve::constant(h(i64::MAX)))]
    }

    proptest! {
        /// DESIGN.md §14, invariants 1 and 8, with every multiplier at once: no target and no
        /// inertia ever reverses an act or pushes an axis out of range.
        #[test]
        fn no_multiplier_reverses_an_act(
            (law, good) in (-100_00_i64..=100_00, -100_00_i64..=100_00),
            (target_law, target_good) in (-100_00_i64..=100_00, -100_00_i64..=100_00),
            relation in -100_00_i64..=100_00,
            (delta_law, delta_good) in (-200_00_i64..=200_00, -200_00_i64..=200_00),
            scale in prop_oneof![1_i64..=1_000, Just(i64::MAX)],
            by_target in proptest::collection::btree_map(
                proptest::sample::select(TargetCurve::ALL.to_vec()), multipliers(), 0..=3),
            inertia in proptest::collection::btree_map(
                proptest::sample::select(Toward::ALL.to_vec()), multipliers(), 0..=4),
        ) {
            let target = Target {
                curves: &by_target,
                alignment: at(target_law, target_good),
                relation: TargetRelation { value: h(relation), between: None },
            };
            let delta = AlignmentDelta { law: h(delta_law), good: h(delta_good) };
            let profile = InertiaProfile { curves: inertia };
            for axis in shifts(at(law, good), delta, h(scale), Some(&target), &profile) {
                prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&axis.to));
                prop_assert!(axis.shift == Fixed::ZERO || (axis.shift > Fixed::ZERO) == (axis.base > Fixed::ZERO));
            }
        }
    }
}

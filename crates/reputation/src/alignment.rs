use std::fmt;

use factional_core::Fixed;

/// How far each axis runs from its centre: −100.00 to 100.00 (DESIGN.md §5.1, P-2).
pub const AXIS_LIMIT: Fixed = Fixed::from_hundredths(100_00);

/// The two axes of alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    /// Chaotic (−100) to lawful (+100).
    Law,
    /// Evil (−100) to good (+100).
    Good,
}

impl Axis {
    /// The axis's name in content files.
    pub fn key(self) -> &'static str {
        match self {
            Axis::Law => "law",
            Axis::Good => "good",
        }
    }
}

/// Where a character or faction sits morally, on two sliding axes (DESIGN.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment {
    law: Fixed,
    good: Fixed,
}

/// An axis value outside −100.00 to 100.00.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisOutOfRange {
    pub axis: Axis,
    pub value: Fixed,
}

impl Alignment {
    /// An alignment, if both axes are within −100.00 to 100.00. Otherwise every axis that's
    /// out of range, law first: content reports them all rather than clamping (P-2).
    pub fn new(law: Fixed, good: Fixed) -> Result<Alignment, Vec<AxisOutOfRange>> {
        let errors: Vec<AxisOutOfRange> = [(Axis::Law, law), (Axis::Good, good)]
            .into_iter()
            .filter(|(_, value)| !(-AXIS_LIMIT..=AXIS_LIMIT).contains(value))
            .map(|(axis, value)| AxisOutOfRange { axis, value })
            .collect();
        if errors.is_empty() {
            Ok(Alignment { law, good })
        } else {
            Err(errors)
        }
    }

    pub fn law(self) -> Fixed {
        self.law
    }

    pub fn good(self) -> Fixed {
        self.good
    }

    /// The nine-box name, such as "Lawful Good" or "True Neutral". An axis at or beyond
    /// ±`threshold` leans that way. For display only: no rule branches on a label (P-2).
    pub fn label(self, threshold: Fixed) -> &'static str {
        match (lean(self.law, threshold), lean(self.good, threshold)) {
            (Lean::Positive, Lean::Positive) => "Lawful Good",
            (Lean::Neutral, Lean::Positive) => "Neutral Good",
            (Lean::Negative, Lean::Positive) => "Chaotic Good",
            (Lean::Positive, Lean::Neutral) => "Lawful Neutral",
            (Lean::Neutral, Lean::Neutral) => "True Neutral",
            (Lean::Negative, Lean::Neutral) => "Chaotic Neutral",
            (Lean::Positive, Lean::Negative) => "Lawful Evil",
            (Lean::Neutral, Lean::Negative) => "Neutral Evil",
            (Lean::Negative, Lean::Negative) => "Chaotic Evil",
        }
    }
}

/// How far an act moves each axis before anything scales it, such as stealing's law −5.00,
/// good −3.00 (DESIGN.md §5.2). An axis the act doesn't touch is 0.00.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AlignmentDelta {
    pub law: Fixed,
    pub good: Fixed,
}

impl Alignment {
    /// This alignment moved by `delta` × `scale`: each axis's shift is rounded once, then the
    /// axis is clamped to −100.00…100.00 (DESIGN.md §5.2).
    pub fn shifted(self, delta: AlignmentDelta, scale: Fixed) -> Alignment {
        Alignment {
            law: moved(self.law, delta.law, scale),
            good: moved(self.good, delta.good, scale),
        }
    }
}

/// One axis's new position after a shift of `base` × `scale`.
fn moved(position: Fixed, base: Fixed, scale: Fixed) -> Fixed {
    let shift = base.saturating_mul(scale);
    // A sum too big to hold means a shift so large it reaches the end of the axis by itself.
    let unclamped = position.checked_add(shift).unwrap_or(shift);
    unclamped.clamp(-AXIS_LIMIT, AXIS_LIMIT)
}

/// Which way one axis leans, for labels.
enum Lean {
    Positive,
    Neutral,
    Negative,
}

fn lean(value: Fixed, threshold: Fixed) -> Lean {
    if value >= threshold {
        Lean::Positive
    } else if value <= -threshold {
        Lean::Negative
    } else {
        Lean::Neutral
    }
}

impl fmt::Display for AxisOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is outside {}..{}",
            self.value, -AXIS_LIMIT, AXIS_LIMIT
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn aligned(law: i64, good: i64) -> Alignment {
        Alignment::new(h(law), h(good)).expect("in range")
    }

    const THRESHOLD: Fixed = h(33_00);

    #[test]
    fn labels_riverholds_characters() {
        assert_eq!(aligned(0, 0).label(THRESHOLD), "True Neutral"); // player
        assert_eq!(aligned(75_00, 30_00).label(THRESHOLD), "Lawful Neutral"); // captain_hale
        assert_eq!(aligned(35_00, 85_00).label(THRESHOLD), "Lawful Good"); // sister_mira
        assert_eq!(aligned(-55_00, -20_00).label(THRESHOLD), "Chaotic Neutral"); // vex
        assert_eq!(aligned(25_00, -70_00).label(THRESHOLD), "Neutral Evil"); // brother_ash
    }

    #[test]
    fn names_all_nine_boxes() {
        let cases = [
            (50_00, 50_00, "Lawful Good"),
            (0, 50_00, "Neutral Good"),
            (-50_00, 50_00, "Chaotic Good"),
            (50_00, 0, "Lawful Neutral"),
            (0, 0, "True Neutral"),
            (-50_00, 0, "Chaotic Neutral"),
            (50_00, -50_00, "Lawful Evil"),
            (0, -50_00, "Neutral Evil"),
            (-50_00, -50_00, "Chaotic Evil"),
        ];
        for (law, good, name) in cases {
            assert_eq!(
                aligned(law, good).label(THRESHOLD),
                name,
                "law {law}, good {good}"
            );
        }
    }

    #[test]
    fn the_label_threshold_is_inclusive() {
        assert_eq!(aligned(33_00, -33_00).label(THRESHOLD), "Lawful Evil");
        assert_eq!(aligned(-33_00, 33_00).label(THRESHOLD), "Chaotic Good");
        assert_eq!(aligned(32_99, 0).label(THRESHOLD), "True Neutral");
        assert_eq!(aligned(0, -32_99).label(THRESHOLD), "True Neutral");
    }

    #[test]
    fn the_threshold_is_a_setting() {
        assert_eq!(aligned(40_00, 0).label(h(50_00)), "True Neutral");
        assert_eq!(aligned(40_00, 0).label(h(40_00)), "Lawful Neutral");
    }

    #[test]
    fn keeps_both_axes() {
        let vex = aligned(-55_00, -20_00);
        assert_eq!((vex.law(), vex.good()), (h(-55_00), h(-20_00)));
    }

    #[test]
    fn rejects_every_axis_outside_the_range_law_first() {
        assert_eq!(
            Alignment::new(h(120_00), h(-100_01)),
            Err(vec![
                AxisOutOfRange {
                    axis: Axis::Law,
                    value: h(120_00)
                },
                AxisOutOfRange {
                    axis: Axis::Good,
                    value: h(-100_01)
                },
            ])
        );
        assert_eq!(
            Alignment::new(h(0), h(100_01)),
            Err(vec![AxisOutOfRange {
                axis: Axis::Good,
                value: h(100_01)
            }])
        );
    }

    #[test]
    fn the_ends_of_the_range_are_allowed() {
        assert!(Alignment::new(h(100_00), h(-100_00)).is_ok());
    }

    #[test]
    fn describes_an_out_of_range_value() {
        let error = AxisOutOfRange {
            axis: Axis::Law,
            value: h(120_00),
        };
        assert_eq!(error.to_string(), "120.00 is outside -100.00..100.00");
    }

    #[test]
    fn names_the_axes_as_content_files_do() {
        assert_eq!((Axis::Law.key(), Axis::Good.key()), ("law", "good"));
    }

    // Shifting (DESIGN.md §5.2)

    const STEAL: AlignmentDelta = AlignmentDelta {
        law: h(-5_00),
        good: h(-3_00),
    };

    #[test]
    fn stealing_moves_toward_chaotic_evil() {
        assert_eq!(aligned(0, 0).shifted(STEAL, h(1_00)), aligned(-5_00, -3_00));
    }

    #[test]
    fn scale_multiplies_the_shift() {
        assert_eq!(
            aligned(0, 0).shifted(STEAL, h(2_00)),
            aligned(-10_00, -6_00)
        );
        assert_eq!(aligned(0, 0).shifted(STEAL, h(50)), aligned(-2_50, -1_50));
    }

    #[test]
    fn each_shift_is_rounded_once_half_away_from_zero() {
        let help = AlignmentDelta {
            law: h(5),
            good: h(4_00),
        };
        // × 0.43: law 0.05 × 0.43 = 0.0215 → 0.02; good 4.00 × 0.43 = 1.72
        assert_eq!(
            aligned(20_00, 20_00).shifted(help, h(43)),
            aligned(20_02, 21_72)
        );
        // × 0.50: law 0.05 × 0.50 = 0.025 → 0.03; good 4.00 × 0.50 = 2.00
        assert_eq!(
            aligned(10_00, 10_00).shifted(help, h(50)),
            aligned(10_03, 12_00)
        );
        assert_eq!(
            aligned(0, 0).shifted(STEAL, h(1)),
            aligned(-5, -3),
            "−5.00 × 0.01 = −0.05"
        );
    }

    #[test]
    fn shifts_clamp_at_the_ends_of_each_axis() {
        assert_eq!(
            aligned(-98_00, 0).shifted(STEAL, h(1_00)),
            aligned(-100_00, -3_00)
        );
        assert_eq!(
            aligned(-100_00, -100_00).shifted(STEAL, h(1_00)),
            aligned(-100_00, -100_00)
        );
        let redeem = AlignmentDelta {
            law: h(4_00),
            good: h(3_00),
        };
        assert_eq!(
            aligned(99_00, 98_00).shifted(redeem, h(1_00)),
            aligned(100_00, 100_00)
        );
    }

    #[test]
    fn an_axis_the_act_does_not_touch_stays_put() {
        let help = AlignmentDelta {
            law: h(0),
            good: h(4_00),
        };
        assert_eq!(
            aligned(-55_00, -20_00).shifted(help, h(3_00)),
            aligned(-55_00, -8_00)
        );
    }

    #[test]
    fn a_scale_too_big_to_compute_still_lands_at_the_end() {
        let huge = h(i64::MAX);
        assert_eq!(
            aligned(50_00, -50_00).shifted(STEAL, huge),
            aligned(-100_00, -100_00)
        );
        let redeem = AlignmentDelta {
            law: h(1),
            good: h(1),
        };
        assert_eq!(
            aligned(-50_00, 50_00).shifted(redeem, huge),
            aligned(100_00, 100_00)
        );
    }

    proptest! {
        #[test]
        fn shifting_never_leaves_the_range_or_reverses_the_act(
            law in -100_00_i64..=100_00,
            good in -100_00_i64..=100_00,
            delta_law in -200_00_i64..=200_00,
            delta_good in -200_00_i64..=200_00,
            scale in prop_oneof![1_i64..=1_000, Just(i64::MAX)],
        ) {
            let before = aligned(law, good);
            let delta = AlignmentDelta { law: h(delta_law), good: h(delta_good) };
            let after = before.shifted(delta, h(scale));
            for (from, to, base) in [
                (before.law(), after.law(), delta_law),
                (before.good(), after.good(), delta_good),
            ] {
                prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&to));
                let moved = to.hundredths() - from.hundredths();
                prop_assert!(moved == 0 || moved.signum() == base.signum());
            }
        }

        #[test]
        fn accepts_exactly_the_values_within_the_range(
            law in -150_00_i64..=150_00,
            good in -150_00_i64..=150_00,
        ) {
            let in_range = |value: i64| (-100_00..=100_00).contains(&value);
            prop_assert_eq!(
                Alignment::new(h(law), h(good)).is_ok(),
                in_range(law) && in_range(good)
            );
        }
    }
}

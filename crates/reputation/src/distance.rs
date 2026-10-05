use std::fmt;

use factional_core::{Fixed, div_round};

use crate::{Alignment, Axis};

/// How much an observer cares about each axis when judging how far away someone is: 0.00 to
/// 1.00 each, with at least one above 0 (DESIGN.md §6, P-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    law: Fixed,
    good: Fixed,
}

/// Why two numbers can't be weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightProblem {
    OutOfRange { axis: Axis, value: Fixed },
    AllZero,
}

impl Weights {
    /// `alignment.default_weights`'s default: 1.00 / 1.00.
    pub const EVEN: Weights = Weights {
        law: Fixed::ONE,
        good: Fixed::ONE,
    };

    /// Weights, if each is within 0.00…1.00 and at least one is above 0. Otherwise every
    /// problem, law first.
    pub fn new(law: Fixed, good: Fixed) -> Result<Weights, Vec<WeightProblem>> {
        let problems: Vec<WeightProblem> = [(Axis::Law, law), (Axis::Good, good)]
            .into_iter()
            .filter(|(_, value)| !(Fixed::ZERO..=Fixed::ONE).contains(value))
            .map(|(axis, value)| WeightProblem::OutOfRange { axis, value })
            .collect();
        if !problems.is_empty() {
            Err(problems)
        } else if law == Fixed::ZERO && good == Fixed::ZERO {
            Err(vec![WeightProblem::AllZero])
        } else {
            Ok(Weights { law, good })
        }
    }

    /// The weight on one axis.
    pub fn on(self, axis: Axis) -> Fixed {
        match axis {
            Axis::Law => self.law,
            Axis::Good => self.good,
        }
    }
}

impl fmt::Display for WeightProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeightProblem::OutOfRange { value, .. } => write!(
                f,
                "{value} must be between {} and {}",
                Fixed::ZERO,
                Fixed::ONE
            ),
            WeightProblem::AllZero => {
                write!(f, "at least one weight must be above {}", Fixed::ZERO)
            }
        }
    }
}

/// How the weighted gaps on the two axes combine into one distance (DESIGN.md §6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Metric {
    /// The straight line: tolerance regions are ellipses.
    #[default]
    Euclidean,
    /// The sum of the two: diamonds.
    Manhattan,
    /// The larger of the two: rectangles.
    Chebyshev,
}

impl Metric {
    pub const ALL: [Metric; 3] = [Metric::Euclidean, Metric::Manhattan, Metric::Chebyshev];

    /// The metric's name in content files.
    pub fn key(self) -> &'static str {
        match self {
            Metric::Euclidean => "euclidean",
            Metric::Manhattan => "manhattan",
            Metric::Chebyshev => "chebyshev",
        }
    }

    pub fn from_key(key: &str) -> Option<Metric> {
        Metric::ALL.into_iter().find(|metric| metric.key() == key)
    }
}

/// How far `subject` is from `observer`, as an observer with these weights sees it. Computed
/// exactly and rounded once, half away from zero (DESIGN.md §6).
pub fn measure(observer: Alignment, subject: Alignment, weights: Weights, metric: Metric) -> Fixed {
    // Each weighted gap, exactly, in ten-thousandths: hundredths × hundredths.
    let [law, good] = [Axis::Law, Axis::Good].map(|axis| {
        let gap = gap(observer.on(axis), subject.on(axis));
        i128::from(weights.on(axis).hundredths()) * i128::from(gap.hundredths())
    });
    let rounded = match metric {
        // ⌊√T⌋ + 50 over 100 rounds √T ten-thousandths to hundredths, half up: √T's fraction
        // can never carry ⌊√T⌋ + 50 past a multiple of 100.
        Metric::Euclidean => {
            let sum = (law * law + good * good).unsigned_abs();
            let root = i128::try_from(sum.isqrt()).expect("a root is smaller than its square");
            (root + 50) / 100
        }
        Metric::Manhattan => div_round(law + good, 100),
        Metric::Chebyshev => div_round(law.max(good), 100),
    };
    Fixed::from_hundredths(i64::try_from(rounded).expect("a distance within the axes fits"))
}

/// How far apart two values are, whichever is higher.
pub(crate) fn gap(a: Fixed, b: Fixed) -> Fixed {
    let difference = a - b;
    difference.max(-difference)
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

    fn weights(law: i64, good: i64) -> Weights {
        Weights::new(h(law), h(good)).expect("valid weights")
    }

    // Riverhold's factions (DESIGN.md §13)
    const WATCH: (i64, i64) = (70_00, 20_00);
    const GUILD: (i64, i64) = (-60_00, -10_00);
    const TEMPLE: (i64, i64) = (30_00, 80_00);

    fn euclidean(from: (i64, i64), to: (i64, i64), w: (i64, i64)) -> Fixed {
        measure(
            aligned(from.0, from.1),
            aligned(to.0, to.1),
            weights(w.0, w.1),
            Metric::Euclidean,
        )
    }

    #[test]
    fn the_watch_sees_a_neutral_player_70_18_away() {
        // Gaps 70 and 20, weighted 70 and 5: sqrt(4900 + 25) = 70.178…
        assert_eq!(euclidean(WATCH, (0, 0), (1_00, 25)), h(70_18));
    }

    #[test]
    fn the_metric_changes_how_gaps_combine() {
        let at = |metric| {
            measure(
                aligned(70_00, 20_00),
                aligned(0, 0),
                weights(1_00, 25),
                metric,
            )
        };
        assert_eq!(at(Metric::Manhattan), h(75_00), "70 + 5");
        assert_eq!(at(Metric::Chebyshev), h(70_00), "the larger of 70 and 5");
    }

    #[test]
    fn the_guild_sees_a_thief_coming_closer() {
        // Gaps 50 and 4, weighted 50 and 2: sqrt(2504) = 50.039…
        assert_eq!(euclidean(GUILD, (-10_00, -6_00), (1_00, 50)), h(50_04));
        // Gaps 40 and 2, weighted 40 and 1: sqrt(1601) = 40.012…
        assert_eq!(euclidean(GUILD, (-20_00, -12_00), (1_00, 50)), h(40_01));
    }

    #[test]
    fn a_reformed_vex_is_near_the_watch_and_far_from_the_guild() {
        // Watch: gaps 35 and 10, weighted 35 and 2.5: sqrt(1231.25) = 35.089…
        assert_eq!(euclidean(WATCH, (35_00, 10_00), (1_00, 25)), h(35_09));
        // Guild: gaps 95 and 20, weighted 95 and 10: sqrt(9125) = 95.524…
        assert_eq!(euclidean(GUILD, (35_00, 10_00), (1_00, 50)), h(95_52));
    }

    #[test]
    fn the_temple_sees_sister_mira_close_by() {
        // Gaps 5 and 5, weighted 2.5 and 5: sqrt(31.25) = 5.590…
        assert_eq!(euclidean(TEMPLE, (35_00, 85_00), (50, 1_00)), h(5_59));
    }

    #[test]
    fn distance_is_rounded_once_half_away_from_zero() {
        // A gap of 0.01 weighted 0.50 is 0.005 exactly, which rounds up to 0.01.
        assert_eq!(euclidean((0, 0), (1, 0), (50, 1_00)), h(1));
        assert_eq!(
            measure(
                aligned(0, 0),
                aligned(1, 0),
                weights(50, 1_00),
                Metric::Manhattan
            ),
            h(1)
        );
        assert_eq!(
            measure(
                aligned(0, 0),
                aligned(1, 0),
                weights(50, 1_00),
                Metric::Chebyshev
            ),
            h(1)
        );
        // 0.0049 rounds down: a gap of 0.07 weighted 0.07.
        assert_eq!(euclidean((0, 0), (7, 0), (7, 1_00)), h(0));
        // Gaps of 0.08 weighted 0.05 are terms of 0.004 each: sqrt(0.000032) = 0.0057 → 0.01,
        // though each term alone would round to 0.00.
        assert_eq!(euclidean((0, 0), (8, 8), (5, 5)), h(1));
        assert_eq!(euclidean((0, 0), (8, 0), (5, 5)), h(0));
    }

    #[test]
    fn opposite_corners_are_the_furthest_apart() {
        // Gaps 200 and 200: sqrt(80000) = 282.842…
        assert_eq!(
            euclidean((-100_00, -100_00), (100_00, 100_00), (1_00, 1_00)),
            h(282_84)
        );
        let corners = |metric| {
            measure(
                aligned(-100_00, -100_00),
                aligned(100_00, 100_00),
                Weights::EVEN,
                metric,
            )
        };
        assert_eq!(corners(Metric::Manhattan), h(400_00));
        assert_eq!(corners(Metric::Chebyshev), h(200_00));
    }

    #[test]
    fn the_direction_of_a_gap_does_not_matter() {
        assert_eq!(euclidean((0, 0), (70_00, -20_00), (1_00, 25)), h(70_18));
        assert_eq!(
            measure(
                aligned(0, 0),
                aligned(-70_00, 20_00),
                weights(1_00, 25),
                Metric::Manhattan
            ),
            h(75_00)
        );
        assert_eq!(
            measure(
                aligned(0, 0),
                aligned(-5_00, 20_00),
                weights(1_00, 50),
                Metric::Chebyshev
            ),
            h(10_00)
        );
    }

    // Weights

    #[test]
    fn weights_run_from_zero_to_one() {
        assert!(Weights::new(h(0), h(1_00)).is_ok());
        assert!(Weights::new(h(1), h(0)).is_ok());
        assert_eq!(
            Weights::new(h(1_50), h(-1)),
            Err(vec![
                WeightProblem::OutOfRange {
                    axis: Axis::Law,
                    value: h(1_50)
                },
                WeightProblem::OutOfRange {
                    axis: Axis::Good,
                    value: h(-1)
                },
            ])
        );
        assert_eq!(
            Weights::new(h(0), h(1_01)),
            Err(vec![WeightProblem::OutOfRange {
                axis: Axis::Good,
                value: h(1_01)
            }])
        );
    }

    #[test]
    fn at_least_one_weight_must_count() {
        assert_eq!(Weights::new(h(0), h(0)), Err(vec![WeightProblem::AllZero]));
    }

    #[test]
    fn keeps_each_axis_weight() {
        let w = weights(1_00, 25);
        assert_eq!((w.on(Axis::Law), w.on(Axis::Good)), (h(1_00), h(25)));
        assert_eq!(
            (Weights::EVEN.on(Axis::Law), Weights::EVEN.on(Axis::Good)),
            (h(1_00), h(1_00))
        );
    }

    #[test]
    fn describes_weight_problems() {
        let out = WeightProblem::OutOfRange {
            axis: Axis::Law,
            value: h(1_50),
        };
        assert_eq!(out.to_string(), "1.50 must be between 0.00 and 1.00");
        assert_eq!(
            WeightProblem::AllZero.to_string(),
            "at least one weight must be above 0.00"
        );
    }

    #[test]
    fn metrics_have_names_in_content_files() {
        for metric in Metric::ALL {
            assert_eq!(Metric::from_key(metric.key()), Some(metric));
        }
        let keys: Vec<&str> = Metric::ALL.into_iter().map(Metric::key).collect();
        assert_eq!(keys, ["euclidean", "manhattan", "chebyshev"]);
        assert_eq!(Metric::from_key("euclidian"), None);
        assert_eq!(Metric::default(), Metric::Euclidean);
    }

    proptest! {
        #[test]
        fn chebyshev_never_exceeds_euclidean_which_never_exceeds_manhattan(
            from in (-100_00_i64..=100_00, -100_00_i64..=100_00),
            to in (-100_00_i64..=100_00, -100_00_i64..=100_00),
            w in (0_i64..=1_00, 1_i64..=1_00),
        ) {
            let at = |metric| measure(aligned(from.0, from.1), aligned(to.0, to.1), weights(w.0, w.1), metric);
            let (chebyshev, euclidean, manhattan) =
                (at(Metric::Chebyshev), at(Metric::Euclidean), at(Metric::Manhattan));
            prop_assert!(Fixed::ZERO <= chebyshev);
            prop_assert!(chebyshev <= euclidean);
            prop_assert!(euclidean <= manhattan);
        }

        #[test]
        fn a_character_is_no_distance_from_themselves(
            at in (-100_00_i64..=100_00, -100_00_i64..=100_00),
            w in (1_i64..=1_00, 0_i64..=1_00),
        ) {
            for metric in Metric::ALL {
                let here = aligned(at.0, at.1);
                prop_assert_eq!(measure(here, here, weights(w.0, w.1), metric), Fixed::ZERO);
            }
        }
    }
}

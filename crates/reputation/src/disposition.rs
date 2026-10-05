use std::fmt;

use factional_core::{Fixed, is_valid_id};

use crate::{Distance, InvalidId};

/// A named range of disposition scores, such as `neutral` (DESIGN.md §8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub name: String,
    /// The highest score in the band; `None` for the last band, which takes every score
    /// above the one before it.
    pub up_to: Option<Fixed>,
}

/// The bands a score falls into, lowest first. Every score lands in exactly one, and a higher
/// score never lands in a lower band (DESIGN.md §14, invariant 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bands(Vec<Band>);

/// Why a list of bands can't be used. `index` counts from 0, as content paths do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BandProblem {
    NoBands,
    InvalidName {
        index: usize,
        name: String,
    },
    DuplicateName {
        index: usize,
        name: String,
    },
    /// A band other than the last leaves out `up_to`.
    MissingUpTo {
        index: usize,
    },
    /// The last band has an `up_to`, so scores above it would have no band.
    LastHasUpTo {
        index: usize,
    },
    NotIncreasing {
        index: usize,
        up_to: Fixed,
        previous: Fixed,
    },
}

impl Bands {
    /// Bands, if there's at least one, every name is a valid id used once, each `up_to`
    /// is above the one before, and only the last is open-ended. Otherwise every problem,
    /// in band order.
    pub fn new(bands: Vec<Band>) -> Result<Bands, Vec<BandProblem>> {
        if bands.is_empty() {
            return Err(vec![BandProblem::NoBands]);
        }
        let last = bands.len() - 1;
        let mut problems = Vec::new();
        let mut previous: Option<Fixed> = None;
        for (index, band) in bands.iter().enumerate() {
            if !is_valid_id(&band.name) {
                problems.push(BandProblem::InvalidName {
                    index,
                    name: band.name.clone(),
                });
            }
            if bands[..index]
                .iter()
                .any(|earlier| earlier.name == band.name)
            {
                problems.push(BandProblem::DuplicateName {
                    index,
                    name: band.name.clone(),
                });
            }
            match (band.up_to, index == last) {
                (Some(_), true) => problems.push(BandProblem::LastHasUpTo { index }),
                (None, false) => problems.push(BandProblem::MissingUpTo { index }),
                (None, true) => {}
                (Some(up_to), false) => {
                    if let Some(previous) = previous.filter(|&previous| up_to <= previous) {
                        problems.push(BandProblem::NotIncreasing {
                            index,
                            up_to,
                            previous,
                        });
                    }
                    previous = Some(up_to);
                }
            }
        }
        if problems.is_empty() {
            Ok(Bands(bands))
        } else {
            Err(problems)
        }
    }

    /// unfriendly ≤ −25 < neutral ≤ 25 < friendly.
    pub fn standard() -> Bands {
        Bands(vec![
            Band {
                name: "unfriendly".to_owned(),
                up_to: Some(Fixed::from_hundredths(-25_00)),
            },
            Band {
                name: "neutral".to_owned(),
                up_to: Some(Fixed::from_hundredths(25_00)),
            },
            Band {
                name: "friendly".to_owned(),
                up_to: None,
            },
        ])
    }

    /// The band `score` falls in: the first whose `up_to` is at or above it.
    pub fn band_for(&self, score: Fixed) -> &Band {
        self.0
            .iter()
            .find(|band| band.up_to.is_none_or(|up_to| score <= up_to))
            .expect("the last band is open-ended")
    }

    /// Every band, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = &Band> {
        self.0.iter()
    }
}

impl fmt::Display for BandProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BandProblem::NoBands => f.write_str("there must be at least one band"),
            BandProblem::InvalidName { name, .. } => InvalidId(name.clone()).fmt(f),
            BandProblem::DuplicateName { name, .. } => {
                write!(f, "another band is already called '{name}'")
            }
            BandProblem::MissingUpTo { .. } => f.write_str(
                "missing 'up_to': only the last band takes every score above the band before it",
            ),
            BandProblem::LastHasUpTo { .. } => f.write_str(
                "the last band can't have 'up_to': it takes every score above the band before it",
            ),
            BandProblem::NotIncreasing {
                up_to, previous, ..
            } => write!(f, "{up_to} must be above the previous band's {previous}"),
        }
    }
}

/// How an observer regards a subject, with its working (DESIGN.md §8, P-24). Until M4 the
/// score is the affinity alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disposition {
    pub score: Fixed,
    /// The name of the band the score falls in.
    pub band: String,
    /// `disposition.affinity` at `distance`: how close the two alignments make them.
    pub affinity: Fixed,
    pub distance: Distance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn band(name: &str, up_to: Option<i64>) -> Band {
        Band {
            name: name.to_owned(),
            up_to: up_to.map(h),
        }
    }

    fn with_hostile() -> Bands {
        Bands::new(vec![
            band("hostile", Some(-30_00)),
            band("unfriendly", Some(-25_00)),
            band("neutral", Some(25_00)),
            band("friendly", None),
        ])
        .expect("valid bands")
    }

    fn band_name(bands: &Bands, score: i64) -> &str {
        &bands.band_for(h(score)).name
    }

    #[test]
    fn the_standard_bands_split_at_minus_25_and_25_inclusive() {
        let bands = Bands::standard();
        assert_eq!(band_name(&bands, -100_00), "unfriendly");
        assert_eq!(band_name(&bands, -25_00), "unfriendly");
        assert_eq!(band_name(&bands, -24_99), "neutral");
        assert_eq!(band_name(&bands, 25_00), "neutral");
        assert_eq!(band_name(&bands, 25_01), "friendly");
        assert_eq!(band_name(&bands, 100_00), "friendly");
    }

    #[test]
    fn designers_can_add_bands() {
        let bands = with_hostile();
        assert_eq!(band_name(&bands, -32_15), "hostile");
        assert_eq!(band_name(&bands, -30_00), "hostile");
        assert_eq!(band_name(&bands, -29_99), "unfriendly");
        let names: Vec<&str> = bands.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["hostile", "unfriendly", "neutral", "friendly"]);
    }

    #[test]
    fn a_single_open_ended_band_takes_every_score() {
        let bands = Bands::new(vec![band("indifferent", None)]).expect("valid bands");
        assert_eq!(band_name(&bands, -100_00), "indifferent");
        assert_eq!(band_name(&bands, 100_00), "indifferent");
    }

    #[test]
    fn the_standard_bands_are_valid() {
        let standard: Vec<Band> = Bands::standard().iter().cloned().collect();
        assert_eq!(Bands::new(standard), Ok(Bands::standard()));
    }

    #[test]
    fn there_must_be_a_band() {
        assert_eq!(Bands::new(Vec::new()), Err(vec![BandProblem::NoBands]));
    }

    #[test]
    fn reports_every_problem_with_a_band_list() {
        let bands = vec![
            band("Very Bad", Some(-50_00)),
            band("unfriendly", Some(-25_00)),
            band("unfriendly", Some(-30_00)),
            band("neutral", None),
            band("friendly", Some(90_00)),
        ];
        assert_eq!(
            Bands::new(bands),
            Err(vec![
                BandProblem::InvalidName {
                    index: 0,
                    name: "Very Bad".to_owned()
                },
                BandProblem::DuplicateName {
                    index: 2,
                    name: "unfriendly".to_owned()
                },
                BandProblem::NotIncreasing {
                    index: 2,
                    up_to: h(-30_00),
                    previous: h(-25_00)
                },
                BandProblem::MissingUpTo { index: 3 },
                BandProblem::LastHasUpTo { index: 4 },
            ])
        );
    }

    #[test]
    fn an_up_to_equal_to_the_one_before_is_not_increasing() {
        let bands = vec![
            band("cold", Some(0)),
            band("cool", Some(0)),
            band("warm", None),
        ];
        assert_eq!(
            Bands::new(bands),
            Err(vec![BandProblem::NotIncreasing {
                index: 1,
                up_to: h(0),
                previous: h(0)
            }])
        );
    }

    #[test]
    fn describes_band_problems() {
        let cases = [
            (BandProblem::NoBands, "there must be at least one band"),
            (
                BandProblem::InvalidName {
                    index: 0,
                    name: "Very Bad".to_owned(),
                },
                "'Very Bad' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            ),
            (
                BandProblem::DuplicateName {
                    index: 2,
                    name: "neutral".to_owned(),
                },
                "another band is already called 'neutral'",
            ),
            (
                BandProblem::MissingUpTo { index: 1 },
                "missing 'up_to': only the last band takes every score above the band before it",
            ),
            (
                BandProblem::LastHasUpTo { index: 2 },
                "the last band can't have 'up_to': it takes every score above the band before it",
            ),
            (
                BandProblem::NotIncreasing {
                    index: 1,
                    up_to: h(-30_00),
                    previous: h(-25_00),
                },
                "-30.00 must be above the previous band's -25.00",
            ),
        ];
        for (problem, message) in cases {
            assert_eq!(problem.to_string(), message);
        }
    }

    proptest! {
        /// DESIGN.md §14, invariant 9.
        #[test]
        fn band_lookup_is_total_and_monotone(a in -100_00_i64..=100_00, b in -100_00_i64..=100_00) {
            let bands = with_hostile();
            let position = |score| {
                let name = &bands.band_for(h(score)).name;
                bands.iter().position(|band| &band.name == name).expect("a band from the list")
            };
            let (low, high) = (a.min(b), a.max(b));
            prop_assert!(position(low) <= position(high));
        }
    }
}

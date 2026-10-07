use std::fmt;

use factional_core::{Fixed, is_valid_id};

use crate::{CharacterId, Distance, FactionId, InvalidId, ModifierId};

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

    /// enemy ≤ −50 < rival ≤ −15 < neutral ≤ 15 < friendly ≤ 50 < allied: the default
    /// relation bands (DESIGN.md §9.4).
    pub fn relations() -> Bands {
        let band = |name: &str, up_to: Option<i64>| Band {
            name: name.to_owned(),
            up_to: up_to.map(Fixed::from_hundredths),
        };
        Bands(vec![
            band("enemy", Some(-50_00)),
            band("rival", Some(-15_00)),
            band("neutral", Some(15_00)),
            band("friendly", Some(50_00)),
            band("allied", None),
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

    /// The band `score` puts someone in who was in `current`, with a `margin` of hysteresis
    /// (DESIGN.md §8.3): they leave `current` only once the score is past its upper edge by
    /// more than the margin, or at or below its lower edge less the margin, and then land in
    /// the score's own band. With no margin, that's always the score's own band.
    pub fn band_after(&self, current: &str, score: Fixed, margin: Fixed) -> &Band {
        let index = self
            .0
            .iter()
            .position(|band| band.name == current)
            .expect("the current band is one of these bands");
        let lower = index.checked_sub(1).and_then(|below| self.0[below].up_to);
        let upper = self.0[index].up_to;
        // An edge too far to compute with can't be crossed.
        let fell = lower
            .and_then(|edge| edge.checked_sub(margin))
            .is_some_and(|edge| score <= edge);
        let rose = upper
            .and_then(|edge| edge.checked_add(margin))
            .is_some_and(|edge| score > edge);
        if fell || rose {
            self.band_for(score)
        } else {
            &self.0[index]
        }
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

/// How an observer regards a subject, with its working (DESIGN.md §8, P-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disposition {
    /// The weighted components added up, clamped to ±100.
    pub score: Fixed,
    /// The name of the band the score falls in.
    pub band: String,
    /// The distance behind the affinity.
    pub distance: Distance,
    /// All five components, in [`ComponentKind::ALL`] order.
    pub components: Vec<Component>,
}

impl Disposition {
    pub fn component(&self, kind: ComponentKind) -> &Component {
        self.components
            .iter()
            .find(|component| component.kind == kind)
            .expect("every disposition has all five components")
    }
}

/// The parts of a disposition (DESIGN.md §8.1, P-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    /// How close the two alignments are: `disposition.affinity` at their distance.
    Affinity,
    /// How the observer regards the subject from what has passed between them.
    Standing,
    /// How the two sides' factions regard each other.
    Kinship,
    /// A character observer adopting their factions' view of the subject.
    FactionOpinion,
    /// From other modules (M10).
    Modifiers,
}

impl ComponentKind {
    pub const ALL: [ComponentKind; 5] = [
        ComponentKind::Affinity,
        ComponentKind::Standing,
        ComponentKind::Kinship,
        ComponentKind::FactionOpinion,
        ComponentKind::Modifiers,
    ];

    /// The component's name in `disposition.weights`.
    pub fn key(self) -> &'static str {
        match self {
            ComponentKind::Affinity => "affinity",
            ComponentKind::Standing => "standing",
            ComponentKind::Kinship => "kinship",
            ComponentKind::FactionOpinion => "faction_opinion",
            ComponentKind::Modifiers => "modifiers",
        }
    }
}

/// One component of a disposition: its value (clamped to ±100, P-7), its weight, and the
/// weighted value that goes into the score, rounded once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub kind: ComponentKind,
    pub value: Fixed,
    pub weight: Fixed,
    pub weighted: Fixed,
    /// What made up the value, for kinship and faction opinion; empty for the others.
    pub parts: Vec<Part>,
    /// The modifiers that made up the value, for the modifiers component; empty for the
    /// others.
    pub modifiers: Vec<AppliedModifier>,
}

/// Who a disposition modifier is for (DESIGN.md §8.1). Everyone comes first, then factions,
/// then characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierObserver {
    Everyone,
    /// The faction, and every character in it.
    Faction(FactionId),
    Character(CharacterId),
}

impl fmt::Display for ModifierObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModifierObserver::Everyone => f.write_str("everyone"),
            ModifierObserver::Faction(id) => id.fmt(f),
            ModifierObserver::Character(id) => id.fmt(f),
        }
    }
}

/// A modifier another module put on how a subject is seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedModifier {
    pub id: ModifierId,
    pub observer: ModifierObserver,
    pub amount: Fixed,
}

/// One contribution to kinship or faction opinion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Kinship: the observer's faction. Faction opinion: the faction whose view it is.
    pub from: FactionId,
    /// Kinship: the subject's faction (the same as `from` when they share it). Faction
    /// opinion: `None`.
    pub to: Option<FactionId>,
    pub value: Fixed,
}

/// How much each component counts toward the score: `disposition.weights` (DESIGN.md §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispositionWeights {
    pub affinity: Fixed,
    pub standing: Fixed,
    pub kinship: Fixed,
    pub faction_opinion: Fixed,
    pub modifiers: Fixed,
}

impl DispositionWeights {
    pub fn get(self, kind: ComponentKind) -> Fixed {
        match kind {
            ComponentKind::Affinity => self.affinity,
            ComponentKind::Standing => self.standing,
            ComponentKind::Kinship => self.kinship,
            ComponentKind::FactionOpinion => self.faction_opinion,
            ComponentKind::Modifiers => self.modifiers,
        }
    }
}

impl Default for DispositionWeights {
    /// affinity 1.00, standing 1.00, kinship 0.50, faction opinion 0.50, modifiers 1.00.
    fn default() -> DispositionWeights {
        DispositionWeights {
            affinity: Fixed::ONE,
            standing: Fixed::ONE,
            kinship: Fixed::from_hundredths(50),
            faction_opinion: Fixed::from_hundredths(50),
            modifiers: Fixed::ONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn hysteresis_holds_a_band_until_the_score_is_past_its_edge_by_the_margin() {
        let bands = Bands::standard();
        let after =
            |current: &str, score: i64| bands.band_after(current, h(score), h(5_00)).name.clone();
        // Leaving neutral downward: at or below −25.00 − 5.00.
        assert_eq!(after("neutral", -29_10), "neutral");
        assert_eq!(after("neutral", -29_99), "neutral");
        assert_eq!(after("neutral", -30_00), "unfriendly");
        assert_eq!(after("neutral", -30_90), "unfriendly");
        // Leaving neutral upward: above 25.00 + 5.00.
        assert_eq!(after("neutral", 30_00), "neutral");
        assert_eq!(after("neutral", 30_01), "friendly");
        // Leaving unfriendly upward: above −25.00 + 5.00; the last band has no upper edge,
        // the first no lower one.
        assert_eq!(after("unfriendly", -20_00), "unfriendly");
        assert_eq!(after("unfriendly", -19_99), "neutral");
        assert_eq!(after("unfriendly", -100_00), "unfriendly");
        assert_eq!(after("friendly", 100_00), "friendly");
        assert_eq!(after("friendly", 20_00), "neutral");
        // Within its own band, nothing changes.
        assert_eq!(after("neutral", 0), "neutral");
    }

    #[test]
    fn leaving_a_band_lands_in_the_scores_own_band_however_far_it_went() {
        let bands = with_hostile();
        assert_eq!(
            bands.band_after("neutral", h(-40_00), h(5_00)).name,
            "hostile"
        );
        assert_eq!(
            bands.band_after("hostile", h(40_00), h(5_00)).name,
            "friendly"
        );
    }

    #[test]
    fn with_no_margin_the_band_is_always_the_scores_own() {
        let bands = Bands::standard();
        for (current, score) in [
            ("neutral", -25_00),
            ("unfriendly", -24_99),
            ("neutral", 25_01),
        ] {
            assert_eq!(
                bands.band_after(current, h(score), Fixed::ZERO),
                bands.band_for(h(score))
            );
        }
    }

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
    fn the_default_relation_bands_run_from_enemy_to_allied() {
        let bands = Bands::relations();
        assert_eq!(band_name(&bands, -80_00), "enemy");
        assert_eq!(band_name(&bands, -50_00), "enemy");
        assert_eq!(band_name(&bands, -40_00), "rival");
        assert_eq!(band_name(&bands, -10_00), "neutral");
        assert_eq!(band_name(&bands, 20_00), "friendly");
        assert_eq!(band_name(&bands, 60_00), "allied");
        let listed: Vec<Band> = bands.iter().cloned().collect();
        assert_eq!(Bands::new(listed), Ok(Bands::relations()));
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

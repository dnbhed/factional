use factional_core::{Fixed, Suggestion};

use crate::{AXIS_LIMIT, FactionId};

/// How one faction regards another, as `relations.toml` writes it (DESIGN.md §9.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub ends: RelationEnds,
    /// −100…100; any direction not written is 0.
    pub value: Fixed,
}

/// Which directions a relation sets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationEnds {
    /// `between = [a, b]`: both directions, the same value.
    Between(FactionId, FactionId),
    /// `from = a`, `to = b`: one direction only, for a grudge that isn't returned.
    Directed { from: FactionId, to: FactionId },
}

/// Where in a relation entry a faction is named, for pointing at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationSide {
    /// `between[0]` or `between[1]`.
    Between(usize),
    From,
    To,
}

impl RelationEnds {
    /// The directions, each as `(from, to)`, in the order written.
    pub fn directions(&self) -> Vec<(FactionId, FactionId)> {
        match self {
            RelationEnds::Between(a, b) => vec![(a.clone(), b.clone()), (b.clone(), a.clone())],
            RelationEnds::Directed { from, to } => vec![(from.clone(), to.clone())],
        }
    }

    /// Every faction named, with where it's named.
    pub fn named(&self) -> Vec<(RelationSide, &FactionId)> {
        match self {
            RelationEnds::Between(a, b) => {
                vec![(RelationSide::Between(0), a), (RelationSide::Between(1), b)]
            }
            RelationEnds::Directed { from, to } => {
                vec![(RelationSide::From, from), (RelationSide::To, to)]
            }
        }
    }

    /// Whether both ends are the same faction.
    pub fn is_self(&self) -> bool {
        match self {
            RelationEnds::Between(a, b) => a == b,
            RelationEnds::Directed { from, to } => from == to,
        }
    }
}

impl Relation {
    /// The directions this relation sets, each as `(from, to)`, in the order written.
    pub fn directions(&self) -> Vec<(FactionId, FactionId)> {
        self.ends.directions()
    }

    /// Every faction the relation names, with where it's named.
    pub fn named(&self) -> Vec<(RelationSide, &FactionId)> {
        self.ends.named()
    }
}

/// A shift in how factions regard each other, as an outcome or a quest's choice makes it
/// (D-30): each direction moves by `by`, stopping at ±100.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelationShift {
    pub ends: RelationEnds,
    pub by: Fixed,
}

impl RelationShift {
    /// How far a shift may go: the whole width of a relation, from one end to the other.
    pub const LIMIT: Fixed = Fixed::from_hundredths(2 * AXIS_LIMIT.hundredths());
}

/// Something wrong with a list of relation shifts, at `index` in it (from 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShiftProblem {
    UnknownFaction {
        index: usize,
        side: RelationSide,
        faction: FactionId,
        suggestion: Option<FactionId>,
    },
    /// A shift between a faction and itself.
    SelfRelation { index: usize },
    /// `by` outside −200…200.
    OutOfRange { index: usize, by: Fixed },
    /// A direction an earlier shift, at `first`, already moves.
    Repeated {
        index: usize,
        from: FactionId,
        to: FactionId,
        first: usize,
    },
}

impl ShiftProblem {
    /// What it suggests instead of a misspelt faction, as its message says (U6c).
    pub fn suggestion(&self) -> Option<Suggestion> {
        match self {
            ShiftProblem::UnknownFaction {
                faction,
                suggestion: Some(close),
                ..
            } => Some(Suggestion {
                wrong: faction.to_string(),
                right: close.to_string(),
            }),
            _ => None,
        }
    }

    /// The shift's place in its list.
    pub fn index(&self) -> usize {
        match self {
            ShiftProblem::UnknownFaction { index, .. }
            | ShiftProblem::SelfRelation { index }
            | ShiftProblem::OutOfRange { index, .. }
            | ShiftProblem::Repeated { index, .. } => *index,
        }
    }
}

impl std::fmt::Display for ShiftProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShiftProblem::UnknownFaction {
                faction,
                suggestion,
                ..
            } => {
                write!(f, "unknown faction '{faction}'")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ShiftProblem::SelfRelation { .. } => {
                f.write_str("a faction can't have a relation with itself")
            }
            ShiftProblem::OutOfRange { by, .. } => write!(
                f,
                "{by} is outside {}..{}",
                -RelationShift::LIMIT,
                RelationShift::LIMIT
            ),
            ShiftProblem::Repeated {
                from, to, first, ..
            } => write!(f, "{from} → {to} is already shifted by relations[{first}]"),
        }
    }
}

/// How one faction regards another now, with its band (DESIGN.md §9.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regard {
    pub from: FactionId,
    pub to: FactionId,
    pub value: Fixed,
    /// The name of the relation band the value falls in, such as `enemy`.
    pub band: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faction(id: &str) -> FactionId {
        FactionId::new(id).expect("valid id")
    }

    #[test]
    fn between_sets_both_directions_and_from_to_one() {
        let mutual = Relation {
            ends: RelationEnds::Between(faction("city_watch"), faction("lantern_guild")),
            value: Fixed::from_hundredths(-80_00),
        };
        assert_eq!(
            mutual.directions(),
            [
                (faction("city_watch"), faction("lantern_guild")),
                (faction("lantern_guild"), faction("city_watch"))
            ]
        );
        let grudge = Relation {
            ends: RelationEnds::Directed {
                from: faction("city_watch"),
                to: faction("free_company"),
            },
            value: Fixed::from_hundredths(-30_00),
        };
        assert_eq!(
            grudge.directions(),
            [(faction("city_watch"), faction("free_company"))]
        );
        assert_eq!(
            grudge.named(),
            [
                (RelationSide::From, &faction("city_watch")),
                (RelationSide::To, &faction("free_company"))
            ]
        );
        assert_eq!(
            mutual.named(),
            [
                (RelationSide::Between(0), &faction("city_watch")),
                (RelationSide::Between(1), &faction("lantern_guild"))
            ]
        );
    }

    #[test]
    fn a_shift_problem_says_which_shift_it_is_about() {
        let problems = [
            ShiftProblem::UnknownFaction {
                index: 2,
                side: RelationSide::From,
                faction: faction("templ"),
                suggestion: None,
            },
            ShiftProblem::SelfRelation { index: 3 },
            ShiftProblem::OutOfRange {
                index: 4,
                by: Fixed::from_hundredths(300_00),
            },
            ShiftProblem::Repeated {
                index: 5,
                from: faction("temple"),
                to: faction("city_watch"),
                first: 1,
            },
        ];
        assert_eq!(problems.map(|problem| problem.index()), [2, 3, 4, 5]);
    }

    #[test]
    fn a_relation_between_a_faction_and_itself_is_self() {
        assert!(RelationEnds::Between(faction("temple"), faction("temple")).is_self());
        assert!(!RelationEnds::Between(faction("temple"), faction("city_watch")).is_self());
        let directed = |from: &str, to: &str| RelationEnds::Directed {
            from: faction(from),
            to: faction(to),
        };
        assert!(directed("temple", "temple").is_self());
        assert!(!directed("temple", "city_watch").is_self());
        assert_eq!(RelationShift::LIMIT, Fixed::from_hundredths(200_00));
    }
}

use factional_core::Fixed;

use crate::FactionId;

/// How one faction regards another, as `relations.toml` writes it (DESIGN.md §9.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub ends: RelationEnds,
    /// −100…100; any direction not written is 0.
    pub value: Fixed,
}

/// Which directions a relation sets.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl Relation {
    /// The directions this relation sets, each as `(from, to)`, in the order written.
    pub fn directions(&self) -> Vec<(FactionId, FactionId)> {
        match &self.ends {
            RelationEnds::Between(a, b) => vec![(a.clone(), b.clone()), (b.clone(), a.clone())],
            RelationEnds::Directed { from, to } => vec![(from.clone(), to.clone())],
        }
    }

    /// Every faction the relation names, with where it's named.
    pub fn named(&self) -> Vec<(RelationSide, &FactionId)> {
        match &self.ends {
            RelationEnds::Between(a, b) => {
                vec![(RelationSide::Between(0), a), (RelationSide::Between(1), b)]
            }
            RelationEnds::Directed { from, to } => {
                vec![(RelationSide::From, from), (RelationSide::To, to)]
            }
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
}

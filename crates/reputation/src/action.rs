use std::collections::{BTreeMap, BTreeSet};

use factional_core::Curve;

use crate::{ActionId, ActionStanding, AlignmentDelta, Axis, CharacterId};

/// Something a character can do, from the action catalogue in `actions.toml` (DESIGN.md §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    /// How far doing it moves the actor's alignment.
    pub alignment: AlignmentDelta,
    /// How doing it changes the way others regard the actor (DESIGN.md §7.1).
    pub standing: ActionStanding,
    /// How who it's done to scales its alignment effect (DESIGN.md §5.4, P-28).
    pub by_target: BTreeMap<TargetCurve, Curve>,
}

/// One of an action's `by_target` curves (DESIGN.md §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetCurve {
    /// Over the target's law, scaling the action's law effect.
    Law,
    /// Over the target's good, scaling the action's good effect.
    Good,
    /// Over the most hostile relation from the actor's factions toward the target's,
    /// scaling both.
    Relation,
}

impl TargetCurve {
    pub const ALL: [TargetCurve; 3] = [TargetCurve::Law, TargetCurve::Good, TargetCurve::Relation];

    /// Its key under `by_target` in content.
    pub fn key(self) -> &'static str {
        match self {
            TargetCurve::Law => "law",
            TargetCurve::Good => "good",
            TargetCurve::Relation => "relation",
        }
    }

    /// The curve over the target's position on `axis`.
    pub fn on(axis: Axis) -> TargetCurve {
        match axis {
            Axis::Law => TargetCurve::Law,
            Axis::Good => TargetCurve::Good,
        }
    }
}

/// Who saw an act (D-7). Under the omniscient knowledge model everyone learns of every act
/// whoever saw it, so this has no effect until K1; it's carried now so the API doesn't
/// change then.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Witnesses {
    #[default]
    Everyone,
    Nobody,
    These(BTreeSet<CharacterId>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_curves_are_keyed_as_content_writes_them() {
        assert_eq!(
            TargetCurve::ALL.map(TargetCurve::key),
            ["law", "good", "relation"]
        );
        assert_eq!(
            [Axis::Law, Axis::Good].map(TargetCurve::on),
            [TargetCurve::Law, TargetCurve::Good]
        );
    }
}

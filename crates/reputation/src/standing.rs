use std::collections::BTreeMap;
use std::fmt;

use factional_core::{Fixed, Ratio};

use crate::{ActionId, AlignmentDelta, CharacterId, FactionId, OutcomeId};

/// Who holds a standing toward a character: a faction or another character (DESIGN.md §7.1).
/// Factions come before characters, each in id order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Party {
    Faction(FactionId),
    Character(CharacterId),
}

impl fmt::Display for Party {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Party::Faction(id) => id.fmt(f),
            Party::Character(id) => id.fmt(f),
        }
    }
}

/// Part of a standing change that spilled over from a change with another faction
/// (DESIGN.md §7.1, P-13): `amount` is `change × multiplier`, rounded once, where the
/// multiplier is `standing.spillover` at how this faction regards `from`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spill {
    /// The faction whose standing changed directly.
    pub from: FactionId,
    /// That change, as applied.
    pub change: Fixed,
    /// How the faction receiving the spill regards `from`.
    pub relation: Fixed,
    pub multiplier: Ratio,
    pub amount: Fixed,
}

/// Whose `standing` block a content problem is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandingOwner {
    /// A character's starting standing, in `characters.toml`.
    Character(CharacterId),
    /// An action's effect, in `actions.toml`.
    Action(ActionId),
    /// An outcome's effect, in `outcomes.toml`.
    Outcome(OutcomeId),
}

/// Which entry in a `standing` block a content problem is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandingKey {
    Target,
    TargetFactions,
    Party(Party),
}

/// Standing changes toward a character, by named party, as content writes them:
/// `standing = { factions = { city_watch = -20.0 }, characters = { captain_hale = -10.0 } }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StandingEffects {
    pub factions: BTreeMap<FactionId, Fixed>,
    pub characters: BTreeMap<CharacterId, Fixed>,
}

impl StandingEffects {
    /// Every named party with its change, factions first, each in id order.
    pub fn parties(&self) -> Vec<(Party, Fixed)> {
        let factions = self
            .factions
            .iter()
            .map(|(id, value)| (Party::Faction(id.clone()), *value));
        let characters = self
            .characters
            .iter()
            .map(|(id, value)| (Party::Character(id.clone()), *value));
        factions.chain(characters).collect()
    }
}

/// An action's effect on how others regard the actor (DESIGN.md §7.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionStanding {
    /// Toward the target, if the act has one.
    pub target: Option<Fixed>,
    /// Toward every faction the target belongs to, if the act has a target.
    pub target_factions: Option<Fixed>,
    /// Toward named factions and characters, whoever the target is.
    pub named: StandingEffects,
}

/// What an outcome or another module changes about a character: alignment, as an action
/// would move it, and standing (P-26).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Effects {
    pub alignment: AlignmentDelta,
    pub standing: StandingEffects,
}

/// A named bundle of effects from `outcomes.toml`, such as a quest's result (P-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub id: OutcomeId,
    pub effects: Effects,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_parties_come_factions_first_each_in_id_order() {
        let h = Fixed::from_hundredths;
        let effects = StandingEffects {
            factions: [
                (FactionId::new("temple").expect("valid"), h(5_00)),
                (FactionId::new("city_watch").expect("valid"), h(-20_00)),
            ]
            .into(),
            characters: [(CharacterId::new("captain_hale").expect("valid"), h(-10_00))].into(),
        };
        let listed: Vec<(String, Fixed)> = effects
            .parties()
            .into_iter()
            .map(|(party, value)| (party.to_string(), value))
            .collect();
        assert_eq!(
            listed,
            [
                ("city_watch".to_owned(), h(-20_00)),
                ("temple".to_owned(), h(5_00)),
                ("captain_hale".to_owned(), h(-10_00)),
            ]
        );
    }
}

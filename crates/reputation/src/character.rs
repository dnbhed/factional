use crate::{Alignment, CharacterId, FactionId, StandingEffects, StartingMembership, Weights};

impl Character {
    /// The factions they start in, as listed.
    pub fn factions(&self) -> impl Iterator<Item = &FactionId> {
        self.memberships
            .iter()
            .map(|membership| &membership.faction)
    }
}

/// Someone in the world, as content describes them. The player is an ordinary character
/// (P-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    pub id: CharacterId,
    pub name: String,
    /// Where they start. Their alignment now is [`World::alignment`](crate::World::alignment).
    pub alignment: Alignment,
    /// How much they care about each axis when judging others; `None` means the balance
    /// default. Characters never inherit weights from their factions (P-21).
    pub weights: Option<Weights>,
    /// The factions they start in, as content lists them. Their memberships now are
    /// [`World::memberships`](crate::World::memberships).
    pub memberships: Vec<StartingMembership>,
    /// How others regard them at the start; anyone not named starts at 0.
    pub standing: StandingEffects,
}

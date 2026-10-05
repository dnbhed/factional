use crate::{Alignment, CharacterId, Weights};

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
}

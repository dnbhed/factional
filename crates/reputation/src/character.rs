use crate::{Alignment, CharacterId};

/// Someone in the world, as content describes them. The player is an ordinary character
/// (P-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    pub id: CharacterId,
    pub name: String,
    /// Where they start. Their alignment now is [`World::alignment`](crate::World::alignment).
    pub alignment: Alignment,
}

use crate::{Alignment, FactionId, Tolerances, Weights};

/// A faction, as content describes it (DESIGN.md §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Faction {
    pub id: FactionId,
    pub name: String,
    pub alignment: Alignment,
    /// How much it cares about each axis when judging others; `None` means the balance
    /// default (DESIGN.md §6).
    pub weights: Option<Weights>,
    /// How close someone must be to join, and to stay (DESIGN.md §9.1).
    pub tolerances: Tolerances,
}

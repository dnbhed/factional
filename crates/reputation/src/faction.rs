use factional_core::Fixed;

use crate::{Alignment, FactionId, RankId, Tolerances, Weights};

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
    /// The change in a member's standing with the faction when they leave of their own
    /// accord (P-22).
    pub leave_standing_change: Fixed,
    /// The rank ladder, lowest first (DESIGN.md §7.2). New members join on the first rung.
    pub ranks: Vec<Rank>,
}

/// One rung of a faction's ladder (DESIGN.md §7.2, P-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rank {
    pub id: RankId,
    /// The standing with the faction a member needs to be promoted to this rank.
    pub requires_standing: Option<Fixed>,
    /// A stricter alignment tolerance for this rank than the faction's own.
    pub tolerance: Option<Fixed>,
}

impl Faction {
    /// The lowest rung, where new members start.
    pub fn lowest_rank(&self) -> Option<&Rank> {
        self.ranks.first()
    }

    /// A rung's place on the ladder, from 0 at the bottom.
    pub fn rank_position(&self, rank: &RankId) -> Option<usize> {
        self.ranks.iter().position(|rung| &rung.id == rank)
    }
}

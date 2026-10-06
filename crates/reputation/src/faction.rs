use std::collections::BTreeMap;

use factional_core::Fixed;

use crate::{Alignment, DriftPolicy, FactionId, RankId, Rule, TableKind, Tolerances, Weights};

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
    /// Its own `defectors` and `deserters` tables, replacing the world's (DESIGN.md §9.2).
    pub rule_tables: BTreeMap<TableKind, Vec<Rule>>,
    /// What happens to a member who drifts out of tolerance; `None` means the balance
    /// default (DESIGN.md §9.3).
    pub drift: Option<DriftPolicy>,
    /// The change in an expelled member's standing with the faction.
    pub expel_standing_change: Fixed,
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
    /// `expel_standing_change`'s default: −20.00.
    pub const DEFAULT_EXPEL_STANDING_CHANGE: Fixed = Fixed::from_hundredths(-20_00);

    /// How far a member on rung `rung` (from 0) may drift: the stricter of that rank's own
    /// `tolerance` and the faction's `member_tolerance` (DESIGN.md §9.3).
    pub fn member_tolerance(&self, rung: usize) -> Fixed {
        let member = self.tolerances.member();
        self.ranks[rung]
            .tolerance
            .map_or(member, |rank| rank.min(member))
    }

    /// The lowest rung, where new members start.
    pub fn lowest_rank(&self) -> Option<&Rank> {
        self.ranks.first()
    }

    /// A rung's place on the ladder, from 0 at the bottom.
    pub fn rank_position(&self, rank: &RankId) -> Option<usize> {
        self.ranks.iter().position(|rung| &rung.id == rank)
    }
}

use std::collections::BTreeSet;

use crate::{ActionId, AlignmentDelta, CharacterId};

/// Something a character can do, from the action catalogue in `actions.toml` (DESIGN.md §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    /// How far doing it moves the actor's alignment.
    pub alignment: AlignmentDelta,
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

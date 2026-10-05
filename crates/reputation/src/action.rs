use std::collections::BTreeSet;

use crate::{ActionId, ActionStanding, AlignmentDelta, CharacterId};

/// Something a character can do, from the action catalogue in `actions.toml` (DESIGN.md §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    /// How far doing it moves the actor's alignment.
    pub alignment: AlignmentDelta,
    /// How doing it changes the way others regard the actor (DESIGN.md §7.1).
    pub standing: ActionStanding,
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

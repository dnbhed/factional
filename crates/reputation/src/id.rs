use factional_core::id_type;

pub use factional_core::InvalidId;

id_type!(
    /// A character's id, such as `captain_hale`.
    CharacterId
);

id_type!(
    /// An action's id in the catalogue, such as `steal`.
    ActionId
);

id_type!(
    /// A rank's id on its faction's ladder, such as `sergeant`.
    RankId
);

id_type!(
    /// An outcome's id in `outcomes.toml`, such as `fined_by_watch`.
    OutcomeId
);

id_type!(
    /// A faction's id, such as `city_watch`. Factions and characters share one set of ids.
    FactionId
);

id_type!(
    /// An inertia profile's id, such as `hardening` (DESIGN.md §5.3).
    ProfileId
);

id_type!(
    /// A disposition modifier's id, such as `bribed`, unique for its subject (DESIGN.md §8.1).
    ModifierId
);

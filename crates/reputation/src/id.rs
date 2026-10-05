use std::fmt;

use factional_core::is_valid_id;

/// Text that isn't a valid id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidId(pub String);

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            self.0
        )
    }
}

/// Defines an id type: a lowercase letter, then lowercase letters, digits or underscores, so
/// it can be typed in the CLI (P-31).
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: &str) -> Result<$name, InvalidId> {
                if is_valid_id(id) {
                    Ok($name(id.to_owned()))
                } else {
                    Err(InvalidId(id.to_owned()))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_ids_and_shows_them_as_written() {
        let id = CharacterId::new("captain_hale").expect("valid");
        assert_eq!(
            (id.as_str(), id.to_string()),
            ("captain_hale", "captain_hale".into())
        );
        let action = ActionId::new("help_stranger").expect("valid");
        assert_eq!(
            (action.as_str(), action.to_string()),
            ("help_stranger", "help_stranger".into())
        );
    }

    #[test]
    fn rejects_invalid_ids_and_says_why() {
        let error = CharacterId::new("Captain Hale").unwrap_err();
        assert_eq!(
            error.to_string(),
            "'Captain Hale' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
        );
        assert_eq!(ActionId::new("Steal"), Err(InvalidId("Steal".to_owned())));
    }
}

//! Ids: a lowercase letter, then lowercase letters, digits or underscores, so they can be
//! typed in the CLI (P-31). Every module defines its own id types with [`id_type!`].

use std::fmt;

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
/// it can be typed in the CLI (P-31). It reads and writes as its text, for saves (T4).
#[macro_export]
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: &str) -> Result<$name, $crate::InvalidId> {
                if $crate::is_valid_id(id) {
                    Ok($name(id.to_owned()))
                } else {
                    Err($crate::InvalidId(id.to_owned()))
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

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl $crate::serde::Serialize for $name {
            /// As the id's text, for saves (T4).
            fn serialize<S: $crate::serde::Serializer>(
                &self,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            /// Text that must be a valid id.
            fn deserialize<D: $crate::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<$name, D::Error> {
                let text = <String as $crate::serde::Deserialize>::deserialize(deserializer)?;
                $name::new(&text).map_err($crate::serde::de::Error::custom)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    id_type!(
        /// An id for these tests.
        TestId
    );

    #[test]
    fn accepts_valid_ids_and_shows_them_as_written() {
        let id = TestId::new("captain_hale").expect("valid");
        assert_eq!(
            (id.as_str(), id.as_ref(), id.to_string()),
            ("captain_hale", "captain_hale", "captain_hale".into())
        );
    }

    #[test]
    fn rejects_invalid_ids_and_says_why() {
        let error = TestId::new("Captain Hale").unwrap_err();
        assert_eq!(
            error.to_string(),
            "'Captain Hale' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
        );
        assert_eq!(TestId::new("Steal"), Err(InvalidId("Steal".to_owned())));
    }

    #[test]
    fn reads_and_writes_as_its_text() {
        let id = TestId::new("vex").expect("valid");
        assert_eq!(serde_json::to_string(&id).expect("writes"), "\"vex\"");
        assert_eq!(
            serde_json::from_str::<TestId>("\"vex\"").expect("reads"),
            id
        );
        assert!(serde_json::from_str::<TestId>("\"Vex\"").is_err());
    }
}

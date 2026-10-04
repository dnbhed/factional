use std::fmt;

use factional_core::is_valid_id;

use crate::Alignment;

/// A character's id, such as `captain_hale`: a lowercase letter, then lowercase letters,
/// digits or underscores.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CharacterId(String);

/// Text that isn't a valid id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidId(pub String);

impl CharacterId {
    pub fn new(id: &str) -> Result<CharacterId, InvalidId> {
        if is_valid_id(id) {
            Ok(CharacterId(id.to_owned()))
        } else {
            Err(InvalidId(id.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CharacterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            self.0
        )
    }
}

/// Someone in the world. The player is an ordinary character (P-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    pub id: CharacterId,
    pub name: String,
    pub alignment: Alignment,
}

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
    }

    #[test]
    fn rejects_invalid_ids_and_says_why() {
        let error = CharacterId::new("Captain Hale").unwrap_err();
        assert_eq!(
            error.to_string(),
            "'Captain Hale' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
        );
    }
}

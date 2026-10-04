use std::collections::BTreeMap;

use factional_core::Fixed;

use crate::{Alignment, Character, CharacterId};

/// World-wide rules and defaults from `balance.toml` (DESIGN.md §12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    /// An axis at or beyond ±this reads as Lawful/Chaotic or Good/Evil (DESIGN.md §5.1).
    pub label_threshold: Fixed,
}

impl Balance {
    /// `alignment.label_threshold`'s default: 33.00.
    pub const DEFAULT_LABEL_THRESHOLD: Fixed = Fixed::from_hundredths(33_00);
}

impl Default for Balance {
    fn default() -> Balance {
        Balance {
            label_threshold: Balance::DEFAULT_LABEL_THRESHOLD,
        }
    }
}

/// Validated content: everything a world starts from. `factional-content` builds it from a
/// directory of TOML files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    pub balance: Balance,
    pub characters: BTreeMap<CharacterId, Character>,
}

/// The reputation module's state. For now it only holds what content set up; from A2 it
/// changes through commands and reports changes as events (DESIGN.md §2).
#[derive(Debug, Clone)]
pub struct World {
    content: Content,
}

impl World {
    pub fn new(content: Content) -> World {
        World { content }
    }

    pub fn balance(&self) -> &Balance {
        &self.content.balance
    }

    /// Every character, in id order.
    pub fn characters(&self) -> impl Iterator<Item = &Character> {
        self.content.characters.values()
    }

    pub fn character(&self, id: &CharacterId) -> Option<&Character> {
        self.content.characters.get(id)
    }

    pub fn alignment(&self, id: &CharacterId) -> Option<Alignment> {
        self.character(id).map(|character| character.alignment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn id(text: &str) -> CharacterId {
        CharacterId::new(text).expect("a valid id")
    }

    fn character(name: &str, law: i64, good: i64) -> Character {
        Character {
            id: id(&name.to_lowercase()),
            name: name.to_owned(),
            alignment: Alignment::new(h(law), h(good)).expect("in range"),
        }
    }

    fn riverhold() -> World {
        let characters = [
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
        ];
        World::new(Content {
            balance: Balance::default(),
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
        })
    }

    #[test]
    fn the_default_label_threshold_is_33() {
        assert_eq!(Balance::default().label_threshold, h(33_00));
    }

    #[test]
    fn looks_up_a_characters_alignment() {
        let world = riverhold();
        assert_eq!(
            world.alignment(&id("vex")),
            Some(Alignment::new(h(-55_00), h(-20_00)).expect("in range"))
        );
        assert_eq!(world.alignment(&id("nobody")), None);
        assert_eq!(
            world.character(&id("ava")).map(|c| c.name.as_str()),
            Some("Ava")
        );
    }

    #[test]
    fn lists_characters_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["ava", "vex"]);
    }

    #[test]
    fn keeps_the_balance_it_was_given() {
        let world = World::new(Content {
            balance: Balance {
                label_threshold: h(40_00),
            },
            ..Content::default()
        });
        assert_eq!(world.balance().label_threshold, h(40_00));
    }
}

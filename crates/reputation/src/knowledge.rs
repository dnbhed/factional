//! Who learns of an act (DESIGN.md §10): the knowledge model, and how each party an act's
//! standing effects name came to know of it, or didn't.

use factional_core::Fixed;

use crate::id::CharacterId;
use crate::standing::Party;

/// `knowledge.model`: who learns of an act (DESIGN.md §10.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KnowledgeModel {
    /// Everyone, fully and at once, whoever saw it.
    #[default]
    Omniscient,
    /// Only those who learn firsthand: the witnesses, the parties the act names, and the
    /// factions of the characters among them (P-55).
    Witnessed,
}

impl KnowledgeModel {
    pub const ALL: [KnowledgeModel; 2] = [KnowledgeModel::Omniscient, KnowledgeModel::Witnessed];

    /// The model's name in content files.
    pub fn key(self) -> &'static str {
        match self {
            KnowledgeModel::Omniscient => "omniscient",
            KnowledgeModel::Witnessed => "witnessed",
        }
    }

    pub fn from_key(key: &str) -> Option<KnowledgeModel> {
        KnowledgeModel::ALL
            .into_iter()
            .find(|model| model.key() == key)
    }
}

/// How a party learned of an act firsthand (DESIGN.md §10.1, P-55).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Learned {
    /// Everyone knows: the model is omniscient, or everyone saw it.
    Everyone,
    /// The act names them directly, so it's addressed to them.
    Named,
    /// They saw it.
    Witness,
    /// A faction, through this member, who learned firsthand.
    ThroughMember(CharacterId),
}

/// One party an act's standing effects name, with the change due to them and how they
/// learned of it; `learned` is `None` if they didn't, and then nothing changes for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    pub party: Party,
    pub change: Fixed,
    pub learned: Option<Learned>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_are_keyed_as_content_writes_them() {
        assert_eq!(
            KnowledgeModel::ALL.map(KnowledgeModel::key),
            ["omniscient", "witnessed"]
        );
        for model in KnowledgeModel::ALL {
            assert_eq!(KnowledgeModel::from_key(model.key()), Some(model));
        }
        assert_eq!(KnowledgeModel::from_key("ripple"), None);
        assert_eq!(KnowledgeModel::default(), KnowledgeModel::Omniscient);
    }
}

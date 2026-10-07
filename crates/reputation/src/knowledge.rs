//! Who learns of an act (DESIGN.md §10): the knowledge model, and how each party an act's
//! standing effects name came to know of it, or didn't.

use std::collections::{BTreeMap, BTreeSet};

use factional_core::{Fixed, Tick};

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
    /// Those who learn firsthand, then everyone the news reaches through contacts and
    /// factions, weaker at each hop (DESIGN.md §10.2).
    Ripple,
}

impl KnowledgeModel {
    pub const ALL: [KnowledgeModel; 3] = [
        KnowledgeModel::Omniscient,
        KnowledgeModel::Witnessed,
        KnowledgeModel::Ripple,
    ];

    /// The model's name in content files.
    pub fn key(self) -> &'static str {
        match self {
            KnowledgeModel::Omniscient => "omniscient",
            KnowledgeModel::Witnessed => "witnessed",
            KnowledgeModel::Ripple => "ripple",
        }
    }

    pub fn from_key(key: &str) -> Option<KnowledgeModel> {
        KnowledgeModel::ALL
            .into_iter()
            .find(|model| model.key() == key)
    }
}

/// `[knowledge.ripple]`: how news travels under the ripple model (DESIGN.md §10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ripple {
    /// The awareness news arrives at on each hop, first hop first; it goes no further than
    /// the last. Each is within 0.01–1.00, and none is stronger than the one before.
    pub strength: Vec<Fixed>,
    /// How many ticks each hop takes, at least 1.
    pub hop_ticks: u64,
}

impl Ripple {
    /// `strength`'s default: 0.50, 0.25, then 0.10.
    pub fn default_strength() -> Vec<Fixed> {
        [50, 25, 10].map(Fixed::from_hundredths).to_vec()
    }

    /// `hop_ticks`' default.
    pub const DEFAULT_HOP_TICKS: u64 = 1;

    /// How strongly news arrives `hop` hops from those who learned it firsthand: 1.00 at
    /// hop 0; `None` beyond the last hop.
    pub fn awareness(&self, hop: u32) -> Option<Fixed> {
        match hop.checked_sub(1) {
            None => Some(Fixed::ONE),
            Some(index) => self.strength.get(usize::try_from(index).ok()?).copied(),
        }
    }
}

impl Default for Ripple {
    fn default() -> Ripple {
        Ripple {
            strength: Ripple::default_strength(),
            hop_ticks: Ripple::DEFAULT_HOP_TICKS,
        }
    }
}

/// The next hop of a piece of news: who it reaches, when, and how strongly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NextHop {
    pub hop: u32,
    pub at: Tick,
    pub awareness: Fixed,
    /// The characters it reaches; their factions hear through them at the same moment.
    pub parties: BTreeSet<CharacterId>,
}

/// News in flight: an act some have heard of and more will (DESIGN.md §10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct News {
    /// Who did it; they know, but never pass it on.
    pub actor: CharacterId,
    /// Everyone who has heard of it so far, the actor included.
    pub heard: BTreeSet<Party>,
    /// The standing change still due to each party the act names that hasn't heard yet, at
    /// full strength; it's scaled by their awareness when they do.
    pub due: BTreeMap<Party, Fixed>,
    /// Where it goes next.
    pub next: NextHop,
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
            ["omniscient", "witnessed", "ripple"]
        );
        for model in KnowledgeModel::ALL {
            assert_eq!(KnowledgeModel::from_key(model.key()), Some(model));
        }
        assert_eq!(KnowledgeModel::from_key("rumour"), None);
        assert_eq!(KnowledgeModel::default(), KnowledgeModel::Omniscient);
    }
}

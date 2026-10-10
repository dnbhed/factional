//! What's in the files but couldn't be read (T6, P-79). It still counts as there for the
//! world's checks across files, so nothing is reported missing, or judged by a default, only
//! because something else couldn't be read. That something is reported itself, so the world
//! still doesn't load (D-20).

use std::collections::BTreeSet;

use factional_reputation::{
    CharacterId, ContentProblem, FactionId, Party, ProfileId, ShiftProblem,
};

/// Ids of one kind that are there but couldn't be read.
#[derive(Debug)]
pub(crate) enum Ids<Id> {
    /// These ones.
    Listed(BTreeSet<Id>),
    /// Any of them: their file didn't parse, so what it holds is unknown.
    All,
}

impl<Id> Default for Ids<Id> {
    fn default() -> Self {
        Ids::Listed(BTreeSet::new())
    }
}

impl<Id: Ord> Ids<Id> {
    pub(crate) fn add(&mut self, id: Id) {
        if let Ids::Listed(ids) = self {
            ids.insert(id);
        }
    }

    pub(crate) fn contains(&self, id: &Id) -> bool {
        match self {
            Ids::Listed(ids) => ids.contains(id),
            Ids::All => true,
        }
    }
}

/// Everything there that couldn't be read.
#[derive(Debug, Default)]
pub(crate) struct Unread {
    pub(crate) factions: Ids<FactionId>,
    pub(crate) characters: Ids<CharacterId>,
    pub(crate) profiles: Ids<ProfileId>,
    /// `knowledge.model` is there but couldn't be read, so the default stands in for it.
    pub(crate) knowledge_model: bool,
}

impl Unread {
    /// Whether `problem` might only be there because of something that couldn't be read.
    pub(crate) fn explains(&self, problem: &ContentProblem) -> bool {
        match problem {
            ContentProblem::UnknownMembershipFaction { faction, .. }
            | ContentProblem::UnknownRelationFaction { faction, .. }
            | ContentProblem::OutcomeRelation {
                problem: ShiftProblem::UnknownFaction { faction, .. },
                ..
            } => self.factions.contains(faction),
            ContentProblem::UnknownStandingParty { party, .. } => match party {
                Party::Faction(faction) => self.factions.contains(faction),
                Party::Character(character) => self.characters.contains(character),
            },
            ContentProblem::UnknownContact { contact, .. } => self.characters.contains(contact),
            ContentProblem::UnknownProfile { profile, .. } => self.profiles.contains(profile),
            ContentProblem::SecretMembersNeedKnowledge(_) => self.knowledge_model,
            _ => false,
        }
    }
}

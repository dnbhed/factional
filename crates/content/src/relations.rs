//! A faction's relations, both ways (U12b, D-38, P-91): every other faction, how this faction
//! regards it and how it regards this one back, each with its band, whether the pair is in
//! conflict, and whether the two are linked "same both ways". Read from the text, so it shows
//! while the world doesn't load.

use std::collections::BTreeSet;

use factional_core::Fixed;
use factional_reputation::Balance;

use crate::edit::{EditError, Step, ValuePath, entry_fields, set_value};
use crate::form::{Offer, Offered, make_offer, written_at};
use crate::references::{Names, ids_named};
use crate::words::Wording;
use crate::{CONTENT_FILES, ContentTexts, FACTIONS_FILE, Sources, parse_content};

/// The key in a faction its relations are written under.
const RELATIONS: &str = "relations";

/// One other faction on a faction's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationRow {
    /// The other faction's id, as written.
    pub other: String,
    /// Its name; `None` for an id written that names no faction.
    pub name: Option<String>,
    /// How this faction regards it.
    pub regard: Regard,
    /// How it regards this one back; `None` for an id that names no faction.
    pub back: Option<Regard>,
    /// Whether either regards the other at or below `relations.conflict_threshold`.
    pub conflict: bool,
}

/// One direction of a relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regard {
    /// As written; `None` if it isn't, when it's 0.
    pub written: Option<String>,
    /// What it is: as written, or 0; `None` for what isn't a number.
    pub value: Option<Fixed>,
    /// The name of its band in `relations.bands`.
    pub band: Option<String>,
}

impl RelationRow {
    /// Whether the two directions hold the same value, so a change to one writes both.
    pub fn linked(&self) -> bool {
        let back = self.back.as_ref().and_then(|back| back.value);
        back.is_some() && back == self.regard.value
    }
}

/// Where `from`'s regard for `to` is written: `<from>.relations.<to>`.
fn relation_at(from: &str, to: &str) -> ValuePath {
    ValuePath(vec![
        Step::Key(from.to_owned()),
        Step::Key(RELATIONS.to_owned()),
        Step::Key(to.to_owned()),
    ])
}

/// `balance.toml` as read on its own, for its bands and threshold; the defaults if it isn't
/// there or doesn't read.
fn balance_of(texts: &ContentTexts) -> Balance {
    let sources = Sources {
        balance: texts[0].as_deref(),
        ..Sources::default()
    };
    parse_content(sources)
        .map(|content| content.balance)
        .unwrap_or_default()
}

/// Every other faction `faction` regards or could, in id order: each that exists, and each
/// id it writes that names none.
pub fn relation_rows(texts: &ContentTexts, faction: &str) -> Vec<RelationRow> {
    let place = CONTENT_FILES.iter().position(|file| *file == FACTIONS_FILE);
    let (Some(text), Some(wording)) = (
        place.and_then(|place| texts[place].as_deref()),
        Wording::new(texts, FACTIONS_FILE),
    ) else {
        return Vec::new();
    };
    let factions = ids_named(texts, Names::Faction);
    if !factions.iter().any(|id| id == faction) {
        return Vec::new();
    }
    let balance = balance_of(texts);
    let regard_of = |from: &str, to: &str| {
        let at = relation_at(from, to);
        let written = entry_fields(text, from)
            .into_iter()
            .find(|field| field.path == at)
            .map(|field| field.value);
        let value = match &written {
            Some(written) => written.parse::<Fixed>().ok(),
            None => Some(Fixed::ZERO),
        };
        let band = value.map(|value| balance.relation_bands.band_for(value).name.clone());
        Regard {
            written,
            value,
            band,
        }
    };
    let written = wording.keys_at(&ValuePath(vec![
        Step::Key(faction.to_owned()),
        Step::Key(RELATIONS.to_owned()),
    ]));
    let others: BTreeSet<&String> = factions.iter().chain(&written).collect();
    others
        .into_iter()
        .filter(|other| *other != faction)
        .map(|other| {
            let regard = regard_of(faction, other);
            let back = factions.contains(other).then(|| regard_of(other, faction));
            let conflict = [Some(&regard), back.as_ref()]
                .into_iter()
                .flatten()
                .filter_map(|regard| regard.value)
                .any(|value| value <= balance.conflict_threshold);
            RelationRow {
                other: other.clone(),
                name: wording.name_of(Names::Faction, other).map(str::to_owned),
                regard,
                back,
                conflict,
            }
        })
        .collect()
}

/// `text`, `factions.toml`, with `from`'s regard for `to` set to `value`, and with `both`,
/// `to`'s for `from` too, writing each `relations` table that isn't there.
pub fn set_relation(
    text: &str,
    from: &str,
    to: &str,
    value: &str,
    both: bool,
) -> Result<String, EditError> {
    let text = set_regard(text, from, to, value)?;
    match both {
        true => set_regard(&text, to, from, value),
        false => Ok(text),
    }
}

/// `text` with `from`'s regard for `to` set to `value`, its key added first if it isn't there.
fn set_regard(text: &str, from: &str, to: &str, value: &str) -> Result<String, EditError> {
    let at = relation_at(from, to);
    if written_at(text, &at).is_some() {
        return set_value(text, &at, value);
    }
    let offer = Offer {
        label: "Relation".to_owned(),
        at: ValuePath(at.0[..2].to_vec()),
        what: Offered::Id(Vec::new()),
    };
    let (text, at) = make_offer(FACTIONS_FILE, text, &offer, Some(to))?;
    set_value(&text, &at, value)
}

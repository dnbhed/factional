//! What names what (U6a, P-83): every reference in the content as written, found by walking
//! each file with its schema, where `x-names` says what an id-valued key names. It reads the
//! text, so it works while the world doesn't load. The loader still checks every reference;
//! a test holds the two to the same places.

use std::collections::BTreeSet;

use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::edit::{Step, ValuePath, child, form, schema_of};
use crate::{CONTENT_FILES, ContentTexts};

/// What an id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Names {
    Faction,
    Character,
    /// A faction or a character: a giver, or a standing requirement.
    Party,
    /// A quest, or the quest a `done` or a `locks` names a stage or choice of.
    Quest,
    Outcome,
    /// An inertia profile.
    Profile,
}

/// One place in the content that names an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub file: &'static str,
    /// The key path where it's named, as diagnostics write it.
    pub key: String,
    pub names: Names,
    pub id: String,
}

/// Every reference in `texts`, file by file in the outline's order, each as written.
pub fn references(texts: &ContentTexts) -> Vec<Reference> {
    let mut found = Vec::new();
    for (file, text) in CONTENT_FILES.iter().zip(texts) {
        let (Some(text), Some(root)) = (text, schema_of(file)) else {
            continue;
        };
        let Ok(document) = text.parse::<DocumentMut>() else {
            continue;
        };
        let mut walk = Walk {
            file,
            root: &root,
            found: &mut found,
        };
        walk.table(document.as_table(), Some(&root), &ValuePath::ROOT);
    }
    found
}

/// The references to the entry `id` of `file`: a faction or a character is also named as a
/// party.
pub fn referenced_by(texts: &ContentTexts, file: &str, id: &str) -> Vec<Reference> {
    let named: &[Names] = match file {
        "factions.toml" => &[Names::Faction, Names::Party],
        "characters.toml" => &[Names::Character, Names::Party],
        "quests.toml" => &[Names::Quest],
        "outcomes.toml" => &[Names::Outcome],
        "balance.toml" => &[Names::Profile],
        _ => &[],
    };
    references(texts)
        .into_iter()
        .filter(|reference| reference.id == id && named.contains(&reference.names))
        .collect()
}

/// The ids that exist for what `names` names, in id order: the entries of the file defining
/// them, or the inertia profiles, steady always among them.
pub(crate) fn ids_named(texts: &ContentTexts, names: Names) -> Vec<String> {
    let files: &[&str] = match names {
        Names::Faction => &["factions.toml"],
        Names::Character => &["characters.toml"],
        Names::Party => &["factions.toml", "characters.toml"],
        Names::Quest => &["quests.toml"],
        Names::Outcome => &["outcomes.toml"],
        Names::Profile => &[],
    };
    let mut ids: BTreeSet<String> = files
        .iter()
        .filter_map(|file| document(texts, file))
        .flat_map(|document| {
            let ids: Vec<String> = document.iter().map(|(id, _)| id.to_owned()).collect();
            ids
        })
        .collect();
    if names == Names::Profile {
        ids.insert("steady".to_owned());
        ids.extend(profiles(texts));
    }
    ids.into_iter().collect()
}

/// The inertia profiles `balance.toml` writes.
fn profiles(texts: &ContentTexts) -> Vec<String> {
    document(texts, "balance.toml")
        .and_then(|balance| {
            let profiles = balance.get("inertia")?.get("profiles")?.as_table_like()?;
            Some(profiles.iter().map(|(id, _)| id.to_owned()).collect())
        })
        .unwrap_or_default()
}

/// `file`'s text, read as TOML, if it's there and is.
fn document(texts: &ContentTexts, file: &str) -> Option<DocumentMut> {
    let place = CONTENT_FILES.iter().position(|name| *name == file)?;
    texts[place].as_deref()?.parse::<DocumentMut>().ok()
}

/// The file that defines `id`: as an entry of `factions.toml`, `characters.toml`,
/// `quests.toml` or `outcomes.toml`, or as an inertia profile in `balance.toml` (steady is
/// always there).
pub fn defined_in(texts: &ContentTexts, id: &str) -> Option<&'static str> {
    let entries = [
        "factions.toml",
        "characters.toml",
        "quests.toml",
        "outcomes.toml",
    ];
    let entry = entries
        .into_iter()
        .find(|file| document(texts, file).is_some_and(|document| document.contains_key(id)));
    if entry.is_some() {
        return entry;
    }
    let profile = id == "steady" || profiles(texts).iter().any(|profile| profile == id);
    profile.then_some("balance.toml")
}

impl Names {
    /// What `x-names` or `x-names-keys` says, as the schema writes it.
    pub(crate) fn from_key(key: &str) -> Option<Names> {
        match key {
            "faction" => Some(Names::Faction),
            "character" => Some(Names::Character),
            "party" => Some(Names::Party),
            "quest" => Some(Names::Quest),
            "outcome" => Some(Names::Outcome),
            "profile" => Some(Names::Profile),
            _ => None,
        }
    }
}

/// A walk through one file, with its schema, noting each reference.
struct Walk<'w> {
    file: &'static str,
    root: &'w Json,
    found: &'w mut Vec<Reference>,
}

impl Walk<'_> {
    fn note(&mut self, names: Option<&Json>, path: &ValuePath, id: &str) {
        let Some(names) = names.and_then(Json::as_str).and_then(Names::from_key) else {
            return;
        };
        // `done` and `locks` name a quest, then perhaps its stage and choice.
        let id = match names {
            Names::Quest => id.split('.').next().unwrap_or(id),
            _ => id,
        };
        self.found.push(Reference {
            file: self.file,
            key: path.to_string(),
            names,
            id: id.to_owned(),
        });
    }

    /// A table, whose schema is `node`: each key it names, and each value in it.
    fn table(&mut self, table: &Table, node: Option<&Json>, path: &ValuePath) {
        let node = node.map(|node| form(self.root, node, "object"));
        for (key, item) in table.iter() {
            let step = Step::Key(key.to_owned());
            let at = path.then(step.clone());
            self.note(node.and_then(|node| node.get("x-names-keys")), &at, key);
            self.item(item, node.and_then(|node| child(node, &step)), &at);
        }
    }

    fn item(&mut self, item: &Item, node: Option<&Json>, path: &ValuePath) {
        match item {
            Item::Table(table) => self.table(table, node, path),
            Item::ArrayOfTables(tables) => {
                let node = node.map(|node| form(self.root, node, "array"));
                for (index, table) in tables.iter().enumerate() {
                    let step = Step::Index(index);
                    let item = node.and_then(|node| child(node, &step));
                    self.table(table, item, &path.then(step));
                }
            }
            Item::Value(value) => self.value(value, node, path),
            Item::None => {}
        }
    }

    fn value(&mut self, value: &Value, node: Option<&Json>, path: &ValuePath) {
        match value {
            Value::InlineTable(table) => {
                let node = node.map(|node| form(self.root, node, "object"));
                for (key, value) in table.iter() {
                    let step = Step::Key(key.to_owned());
                    let at = path.then(step.clone());
                    self.note(node.and_then(|node| node.get("x-names-keys")), &at, key);
                    self.value(value, node.and_then(|node| child(node, &step)), &at);
                }
            }
            Value::Array(items) => {
                let node = node.map(|node| form(self.root, node, "array"));
                for (index, item) in items.iter().enumerate() {
                    let step = Step::Index(index);
                    let child = node.and_then(|node| child(node, &step));
                    self.value(item, child, &path.then(step));
                }
            }
            Value::String(text) => {
                let node = node.map(|node| form(self.root, node, "string"));
                self.note(
                    node.and_then(|node| node.get("x-names")),
                    path,
                    text.value(),
                );
            }
            _ => {}
        }
    }
}

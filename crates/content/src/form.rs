//! An entry's form (U6b, P-84): its values grouped by what they mean, each with the comment
//! written above it, the defaults of the keys left out, and what each value may be. It reads
//! the text with the schema, so it shows while the world doesn't load.

use std::collections::BTreeMap;

use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Table};

use crate::edit::{
    Addition, EditError, Field, Step, ValuePath, add, additions, entry_fields, entry_places, form,
    key_info, prefix_of, schema_at, schema_of, set_value, split,
};
use crate::references::{Names, ids_named};
use crate::words::{Word, Wording};
use crate::{CONTENT_FILES, ContentTexts};

/// One group of an entry's values, such as a faction's *Membership*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormGroup {
    /// `None` for an entry whose keys the schema doesn't group, such as balance's.
    pub name: Option<String>,
    pub rows: Vec<FormRow>,
    /// What can be added in it, in words (U10d).
    pub offers: Vec<Offer>,
}

/// Something a section offers to add (U10d): a key not yet there, an item of a list, or an id
/// in a table keyed by ids or a list of ids; made in one change, writing what it's within if
/// it isn't there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// What it adds, in words, such as `Membership` or `Faction`.
    pub label: String,
    /// Where it's added, from the file's start: the table a key goes in, the list an item goes
    /// on, or the table or list an id goes in.
    pub at: ValuePath,
    pub what: Offered,
}

/// What an [`Offer`] adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offered {
    Key(String),
    Item,
    /// An id in a table keyed by ids, from those that exist and aren't there yet, or a new one.
    Id(Vec<IdChoice>),
    /// An id in a list of ids, from those that exist and aren't in it yet.
    Pick(Vec<IdChoice>),
}

/// Makes `offer` in `file`'s `text`, with `id` for an id: writes what it's within if it isn't
/// there, then what it adds, at its starting value. The new text, and where what was added is.
pub fn make_offer(
    file: &str,
    text: &str,
    offer: &Offer,
    id: Option<&str>,
) -> Result<(String, ValuePath), EditError> {
    let id = || {
        id.filter(|id| !id.trim().is_empty())
            .ok_or(EditError::Blank)
    };
    // Each id is checked before anything is written, so a refusal changes nothing.
    if matches!(offer.what, Offered::Id(_) | Offered::Pick(_)) {
        id()?;
    }
    let mut text = text.to_owned();
    let mut made = false;
    for steps in 1..=offer.at.0.len() {
        let place = ValuePath(offer.at.0[..steps].to_vec());
        if written_at(&text, &place).is_some() {
            continue;
        }
        let (Some((Step::Key(key), within)), true) = (place.0.split_last(), steps > 1) else {
            return Err(EditError::NotThere(place));
        };
        text = add(file, &text, &ValuePath(within.to_vec()), Some(key))?.0;
        made = true;
    }
    match &offer.what {
        Offered::Key(key) => add(file, &text, &offer.at, Some(key)),
        Offered::Item => {
            // A list just made with the items it must have has its first already.
            let count = written_at(&text, &offer.at)
                .and_then(|list| list.as_array().map(Vec::len))
                .unwrap_or_default();
            match (made, count.checked_sub(1)) {
                (true, Some(last)) => Ok((text, offer.at.then(Step::Index(last)))),
                _ => add(file, &text, &offer.at, None),
            }
        }
        Offered::Id(_) => add(file, &text, &offer.at, Some(id()?)),
        Offered::Pick(_) => {
            let (text, path) = add(file, &text, &offer.at, None)?;
            Ok((set_value(&text, &path, id()?)?, path))
        }
    }
}

/// What's written at `path` in `text`, if anything.
fn written_at(text: &str, path: &ValuePath) -> Option<toml::Value> {
    let mut value = toml::Value::Table(text.parse::<toml::Table>().ok()?);
    for step in &path.0 {
        value = match step {
            Step::Key(key) => value.as_table()?.get(key)?.clone(),
            Step::Index(index) => value.as_array()?.get(*index)?.clone(),
        };
    }
    Some(value)
}

/// One value of an entry: as written, a key left out with its default, or, with neither, an
/// empty table or list (U10b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormRow {
    pub path: ValuePath,
    /// Where it is in the entry, such as `alignment.law`.
    pub key: String,
    /// Where it is in the entry, in words, such as *Alignment*, *Law* (U10a).
    pub words: Vec<Word>,
    /// `None` if it's left out, when `default` says what it is.
    pub written: Option<Field>,
    pub default: Option<String>,
    pub description: Option<String>,
    /// The comment lines directly above it, and any after it on its line, without `#`.
    pub comment: Vec<String>,
    /// What it may be: the ids that exist for what it names, the schema's enumeration, or
    /// `false` and `true`; empty if it may be anything of its kind.
    pub choices: Vec<String>,
}

/// The form of the entry `entry` of `file` in `texts` (a top-level key, or one table of a
/// top-level list, such as `relation[2]`): its groups in the schema's order, each with what's
/// written in the order written, then the keys left out of its tables that have a default,
/// in the schema's order. Nothing if the entry isn't there or the text isn't TOML.
pub fn entry_form(texts: &ContentTexts, file: &str, entry: &str) -> Vec<FormGroup> {
    let text = CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .and_then(|place| texts[place].as_deref());
    let (Some(text), Some(root), Some(at)) = (text, schema_of(file), ValuePath::parse(entry))
    else {
        return Vec::new();
    };
    let Some(table) = text
        .parse::<DocumentMut>()
        .ok()
        .and_then(|document| entry_table(&document, &at))
    else {
        return Vec::new();
    };
    let mut comments = Comments::default();
    comments.table(&table, &at);
    let wording = Wording::new(texts, file);
    let words_of = |path: &ValuePath| wording.as_ref().map_or_else(Vec::new, |w| w.words(path));
    // The entry's own words, which each row's start with, and the steps they say.
    let entry_words = words_of(&at).len();
    let within = |path: &ValuePath| -> Vec<Word> {
        words_of(path)
            .into_iter()
            .skip(entry_words)
            .map(|word| Word {
                through: word.through - at.0.len(),
                ..word
            })
            .collect()
    };
    let mut row = |path: ValuePath, written: Option<Field>, default: Option<String>| FormRow {
        key: ValuePath(path.0[at.0.len()..].to_vec()).to_string(),
        words: within(&path),
        description: key_info(file, &path).and_then(|info| info.description),
        comment: comments.of(&path),
        choices: choices(texts, &root, &path),
        written,
        default,
        path,
    };
    let mut rows: Vec<FormRow> = entry_fields(text, entry)
        .into_iter()
        .map(|field| row(field.path.clone(), Some(field), None))
        .collect();
    for place in entry_places(text, entry) {
        for addition in additions(file, text, &place) {
            let Addition::Key(key) = addition else {
                continue;
            };
            let path = place.then(Step::Key(key));
            if let Some(default) = key_info(file, &path).and_then(|info| info.default) {
                rows.push(row(path, None, Some(default)));
            }
        }
    }
    // A table or list with nothing in it, not even a default, is a row of its own (U10b), so
    // it has somewhere to be.
    let places = entry_places(text, entry);
    for place in places.iter().filter(|place| **place != at) {
        let within = |path: &ValuePath| path != place && path.0.starts_with(&place.0);
        if !rows.iter().any(|row| within(&row.path)) && !places.iter().any(within) {
            rows.push(row(place.clone(), None, None));
        }
    }
    let mut groups = grouped(&root, &at, rows);
    if let Some(wording) = &wording {
        for group in &mut groups {
            group.offers = offers_in(texts, file, &root, wording, &at, group.name.as_deref());
        }
    }
    groups
}

/// An id that may be added to a table keyed by ids, with the name of what it names, if it has
/// one (U10b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdChoice {
    pub id: String,
    pub name: Option<String>,
}

/// The ids that may be added to the table at `path` in `file`, keyed by what it names: each
/// that exists and isn't there yet, in id order. Nothing for a table its schema doesn't key by
/// what it names.
pub fn id_choices(texts: &ContentTexts, file: &str, path: &ValuePath) -> Vec<IdChoice> {
    let (Some(root), Some(wording)) = (schema_of(file), Wording::new(texts, file)) else {
        return Vec::new();
    };
    let names = schema_at(&root, path)
        .map(|node| form(&root, node, "object"))
        .and_then(|table| table.get("x-names-keys")?.as_str())
        .and_then(Names::from_key);
    let Some(names) = names else {
        return Vec::new();
    };
    let there = wording.keys_at(path);
    ids_named(texts, names)
        .into_iter()
        .filter(|id| !there.contains(id))
        .map(|id| IdChoice {
            name: wording.name_of(names, &id).map(str::to_owned),
            id,
        })
        .collect()
}

/// The entry at `at` in `document`, as a table.
fn entry_table(document: &DocumentMut, at: &ValuePath) -> Option<Table> {
    match &at.0[..] {
        [Step::Key(key)] => document.get(key)?.as_table().cloned(),
        [Step::Key(key), Step::Index(index)] => document
            .get(key)?
            .as_array_of_tables()?
            .get(*index)
            .cloned(),
        _ => None,
    }
}

/// `rows` in the groups the entry's definition lists, in its order, each there even with
/// nothing in it (U10d); then any row in no group, as one group without a name, which is all
/// of an entry whose definition lists none.
fn grouped(root: &Json, at: &ValuePath, rows: Vec<FormRow>) -> Vec<FormGroup> {
    let names: Vec<String> = schema_at(root, at)
        .map(|node| form(root, node, "object"))
        .and_then(|node| node.get("x-groups"))
        .and_then(Json::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let group_of = |row: &FormRow| {
        let key = row.path.0.get(at.0.len())?.clone();
        let node = schema_at(root, &at.then(key))?;
        node.get("x-group").and_then(Json::as_str)
    };
    let mut groups: Vec<FormGroup> = names
        .iter()
        .map(|name| FormGroup {
            name: Some(name.clone()),
            rows: Vec::new(),
            offers: Vec::new(),
        })
        .collect();
    let mut ungrouped = Vec::new();
    for row in rows {
        let place = group_of(&row).and_then(|group| names.iter().position(|name| name == group));
        match place {
            Some(place) => groups[place].rows.push(row),
            None => ungrouped.push(row),
        }
    }
    if names.is_empty() || !ungrouped.is_empty() {
        groups.push(FormGroup {
            name: None,
            rows: ungrouped,
            offers: Vec::new(),
        });
    }
    groups
}

/// What the group `group` (`None` for an entry whose keys aren't grouped) of the entry at `at`
/// offers to add (U10d), key by key in the schema's order: for a list, an item, or for a list
/// of ids, one of those not in it yet, if any are left; for a table keyed by ids, or each such table in a table
/// of them (a standing's factions and characters), an id; for anything else not yet there and
/// with no default to set, the key.
fn offers_in(
    texts: &ContentTexts,
    file: &str,
    root: &Json,
    wording: &Wording,
    at: &ValuePath,
    group: Option<&str>,
) -> Vec<Offer> {
    let Some(entry) = schema_at(root, at).map(|node| form(root, node, "object")) else {
        return Vec::new();
    };
    let Some(properties) = entry.get("properties").and_then(Json::as_object) else {
        return Vec::new();
    };
    let written = wording.keys_at(at);
    let mut offers = Vec::new();
    for (key, node) in properties {
        if node.get("x-group").and_then(Json::as_str) != group || !allowed(entry, &written, key) {
            continue;
        }
        let path = at.then(Step::Key(key.clone()));
        let title = wording_title(root, node).unwrap_or(key).to_owned();
        let list = form(root, node, "array");
        let table = form(root, node, "object");
        if list.get("type").and_then(Json::as_str) == Some("array") {
            let written = wording.texts_at(&path);
            let room = item_schema_at(list, written.len());
            let names = list
                .get("items")
                .and_then(|items| names_of(form(root, items, "string"), "x-names"));
            let called = list.get("x-item").and_then(Json::as_str);
            let what = match names {
                Some(names) => Offered::Pick(choices_of(texts, wording, names, &written)),
                None => Offered::Item,
            };
            let left = !matches!(&what, Offered::Pick(choices) if choices.is_empty());
            if room && left {
                let label = called
                    .map(str::to_owned)
                    .or_else(|| names.map(|n| noun(n).to_owned()));
                offers.push(Offer {
                    label: label.unwrap_or(title),
                    at: path,
                    what,
                });
            }
        } else if let Some(names) = names_of(table, "x-names-keys") {
            let there = wording.keys_at(&path);
            offers.push(Offer {
                label: noun(names).to_owned(),
                at: path,
                what: Offered::Id(choices_of(texts, wording, names, &there)),
            });
        } else if let Some(inner) = id_tables(root, table) {
            for (inner, names) in inner {
                let path = path.then(Step::Key(inner));
                let there = wording.keys_at(&path);
                offers.push(Offer {
                    label: noun(names).to_owned(),
                    at: path,
                    what: Offered::Id(choices_of(texts, wording, names, &there)),
                });
            }
        } else if wording.value_at(&path).is_none()
            && key_info(file, &path)
                .and_then(|info| info.default)
                .is_none()
        {
            offers.push(Offer {
                label: title,
                at: at.clone(),
                what: Offered::Key(key.clone()),
            });
        }
    }
    offers
}

/// Whether the entry `entry`, with `written` there, may have `key` too: for one the schema
/// gives forms of (`oneOf`), such as a relation's `between` or `from` and `to`, some form that
/// what's written fits doesn't rule it out.
fn allowed(entry: &Json, written: &[String], key: &str) -> bool {
    let Some(forms) = entry.get("oneOf").and_then(Json::as_array) else {
        return true;
    };
    // The keys a form rules out: its `not`'s `required`, or each of its `anyOf`'s.
    let ruled_out = |form: &Json| -> Vec<String> {
        let not = form.get("not");
        let any = not
            .and_then(|not| not.get("anyOf"))
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        not.cloned()
            .into_iter()
            .chain(any)
            .filter_map(|ruled| ruled.get("required")?.as_array().cloned())
            .flatten()
            .filter_map(|key| key.as_str().map(str::to_owned))
            .collect()
    };
    forms.iter().any(|form| {
        let out = ruled_out(form);
        !written.iter().any(|key| out.contains(key)) && !out.iter().any(|out| out == key)
    })
}

/// The title the schema gives `node`.
fn wording_title<'s>(root: &'s Json, node: &'s Json) -> Option<&'s str> {
    node.get("title")
        .or_else(|| form(root, node, "").get("title"))
        .and_then(Json::as_str)
}

/// Whether a list `node` holding `count` has room for one more.
fn item_schema_at(node: &Json, count: usize) -> bool {
    let most = node.get("maxItems").and_then(Json::as_u64);
    most.is_none_or(|most| u64::try_from(count).is_ok_and(|count| count < most))
}

/// What `node`'s `key` (`x-names` or `x-names-keys`) says it names.
fn names_of(node: &Json, key: &str) -> Option<Names> {
    node.get(key)
        .and_then(Json::as_str)
        .and_then(Names::from_key)
}

/// For a table whose every key is a table keyed by ids, such as a standing's `factions` and
/// `characters`, each key and what its keys name; `None` for any other.
fn id_tables(root: &Json, table: &Json) -> Option<Vec<(String, Names)>> {
    let properties = table.get("properties")?.as_object()?;
    let inner: Option<Vec<(String, Names)>> = properties
        .iter()
        .map(|(key, node)| {
            Some((
                key.clone(),
                names_of(form(root, node, "object"), "x-names-keys")?,
            ))
        })
        .collect();
    inner.filter(|inner| !inner.is_empty())
}

/// The ids named as `names` that aren't among `there`, with their names.
fn choices_of(
    texts: &ContentTexts,
    wording: &Wording,
    names: Names,
    there: &[String],
) -> Vec<IdChoice> {
    ids_named(texts, names)
        .into_iter()
        .filter(|id| !there.contains(id))
        .map(|id| IdChoice {
            name: wording.name_of(names, &id).map(str::to_owned),
            id,
        })
        .collect()
}

/// What one of `names` is called, as an offer says it.
fn noun(names: Names) -> &'static str {
    match names {
        Names::Faction => "Faction",
        Names::Character => "Character",
        Names::Party => "Faction or character",
        Names::Quest => "Quest",
        Names::Outcome => "Outcome",
        Names::Profile => "Profile",
    }
}

/// What the value at `path` may be: the ids that exist for what it names, the schema's
/// enumeration, or `false` and `true`.
fn choices(texts: &ContentTexts, root: &Json, path: &ValuePath) -> Vec<String> {
    let Some(node) = schema_at(root, path) else {
        return Vec::new();
    };
    let text = form(root, node, "string");
    let names = node
        .get("x-names")
        .or_else(|| text.get("x-names"))
        .and_then(Json::as_str)
        .and_then(Names::from_key);
    if let Some(names) = names {
        return ids_named(texts, names);
    }
    if let Some(choices) = text.get("enum").and_then(Json::as_array) {
        return choices
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_owned)
            .collect();
    }
    let flag = form(root, node, "boolean").get("type") == Some(&Json::from("boolean"));
    match flag {
        true => vec!["false".to_owned(), "true".to_owned()],
        false => Vec::new(),
    }
}

/// The comments in an entry, by where they belong: above a key, or above a table's header,
/// belongs to the first value within; after a value on its line, to that value.
#[derive(Default)]
struct Comments {
    above: BTreeMap<ValuePath, Vec<String>>,
    after: BTreeMap<ValuePath, Vec<String>>,
}

impl Comments {
    fn table(&mut self, table: &Table, path: &ValuePath) {
        for (key, item) in table.iter() {
            let at = path.then(Step::Key(key.to_owned()));
            if let Some(key) = table.key(key) {
                self.note_above(&at, prefix_of(key.leaf_decor()));
            }
            match item {
                Item::Value(value) => {
                    let after = value.decor().suffix().and_then(|suffix| suffix.as_str());
                    self.after.insert(at, lines_of(after.unwrap_or_default()));
                }
                Item::Table(inner) => {
                    self.note_above(&at, prefix_of(inner.decor()));
                    self.table(inner, &at);
                }
                Item::ArrayOfTables(tables) => {
                    for (index, inner) in tables.iter().enumerate() {
                        let at = at.then(Step::Index(index));
                        self.note_above(&at, prefix_of(inner.decor()));
                        self.table(inner, &at);
                    }
                }
                Item::None => {}
            }
        }
    }

    /// The comment lines directly above what's at `at`: those a blank line sets apart aren't.
    fn note_above(&mut self, at: &ValuePath, prefix: &str) {
        let lines = lines_of(split(prefix).1);
        self.above.entry(at.clone()).or_default().extend(lines);
    }

    /// The comments that belong to the value at `path`, outermost first, each given once: to
    /// the first value asked about within where it was written.
    fn of(&mut self, path: &ValuePath) -> Vec<String> {
        let mut lines = Vec::new();
        for comments in [&mut self.above, &mut self.after] {
            let within: Vec<ValuePath> = comments
                .keys()
                .filter(|at| path.0.starts_with(&at.0))
                .cloned()
                .collect();
            for at in within {
                lines.extend(comments.remove(&at).unwrap_or_default());
            }
        }
        lines
    }
}

/// The comment lines in `text`, each without its `#` and the space after it.
fn lines_of(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix('#'))
        .map(|comment| comment.strip_prefix(' ').unwrap_or(comment).to_owned())
        .collect()
}

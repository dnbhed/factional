//! An entry's form (U6b, P-84): its values grouped by what they mean, each with the comment
//! written above it, the defaults of the keys left out, and what each value may be. It reads
//! the text with the schema, so it shows while the world doesn't load.

use std::collections::BTreeMap;

use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Table};

use crate::edit::{
    Addition, Field, Step, ValuePath, additions, entry_fields, entry_places, form, key_info,
    prefix_of, schema_at, schema_of, split,
};
use crate::references::{Names, ids_named};
use crate::{CONTENT_FILES, ContentTexts};

/// One group of an entry's values, such as a faction's *Membership*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormGroup {
    /// `None` for an entry whose keys the schema doesn't group, such as balance's.
    pub name: Option<String>,
    pub rows: Vec<FormRow>,
}

/// One value of an entry: as written, or a key left out with its default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormRow {
    pub path: ValuePath,
    /// Where it is in the entry, such as `alignment.law`.
    pub key: String,
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
    let mut row = |path: ValuePath, written: Option<Field>, default: Option<String>| FormRow {
        key: ValuePath(path.0[at.0.len()..].to_vec()).to_string(),
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
    grouped(&root, &at, rows)
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

/// `rows` in the groups the entry's definition lists, in its order, leaving out any with
/// none; then any row in no group, as one group without a name.
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
    let mut by_group: BTreeMap<Option<usize>, Vec<FormRow>> = BTreeMap::new();
    for row in rows {
        let place = group_of(&row).and_then(|group| names.iter().position(|name| name == group));
        by_group.entry(place).or_default().push(row);
    }
    // `None` sorts first, so the rows in no group come out and go last.
    let ungrouped = by_group.remove(&None);
    let mut groups: Vec<FormGroup> = by_group
        .into_iter()
        .filter_map(|(place, rows)| {
            Some(FormGroup {
                name: Some(names.get(place?)?.clone()),
                rows,
            })
        })
        .collect();
    if let Some(rows) = ungrouped {
        groups.push(FormGroup { name: None, rows });
    }
    groups
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

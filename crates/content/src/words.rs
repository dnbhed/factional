//! Where a value is, in words (U10a, P-92): each step of a place in a file said as a
//! designer would say it, from the schema's titles and the names of what's named. It reads
//! the text with the schema, so it works while the world doesn't load.

use std::collections::BTreeMap;

use serde_json::Value as Json;
use toml::Value as Toml;

use crate::edit::{Step, ValuePath, child, form, resolve, schema_of};
use crate::references::Names;
use crate::{CONTENT_FILES, ContentTexts};

/// One step of a place, in words: a key's title, a list item's name or number, or the name of
/// what an id names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// How many steps of the path it says, counted from its start: a named list item says
    /// both the list and the item.
    pub through: usize,
}

/// The words for the place `path` in `file`, from its start: each key by its title, each
/// list item whose list says what one is called by that and its name in place of its list
/// (*Stage "inspect"*), or by that and its number from 1 (*Step 2*), any other item by its own
/// title or its number, and each id by the name of what it names. What the schema doesn't
/// know is said as written.
pub fn place_words(texts: &ContentTexts, file: &str, path: &ValuePath) -> Vec<Word> {
    Wording::new(texts, file).map_or_else(Vec::new, |wording| wording.words(path))
}

/// The words for the place `key` in `file`, joined by " › ", such as a problem's key; `key`
/// itself if it isn't a place in a content file.
pub fn words_at(texts: &ContentTexts, file: &str, key: &str) -> String {
    let words = ValuePath::parse(key)
        .map(|path| place_words(texts, file, &path))
        .unwrap_or_default();
    match words.is_empty() {
        true => key.to_owned(),
        false => joined(&words),
    }
}

/// `words`, joined by " › ".
pub(crate) fn joined(words: &[Word]) -> String {
    let texts: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
    texts.join(" › ")
}

/// What saying places in one file needs: its schema, its text as values, and the names of
/// everything named, by the file that defines it.
pub(crate) struct Wording {
    file: &'static str,
    root: Json,
    document: Option<Toml>,
    names: BTreeMap<&'static str, BTreeMap<String, String>>,
}

impl Wording {
    /// `None` for a file that isn't a content file.
    pub(crate) fn new(texts: &ContentTexts, file: &str) -> Option<Wording> {
        let place = CONTENT_FILES.iter().position(|name| *name == file)?;
        let root = schema_of(file)?;
        let values =
            |text: &Option<String>| -> Option<toml::Table> { text.as_deref()?.parse().ok() };
        let names = CONTENT_FILES
            .iter()
            .zip(texts)
            .filter_map(|(file, text)| {
                let named = values(text)?
                    .into_iter()
                    .filter_map(|(id, entry)| Some((id, entry.get("name")?.as_str()?.to_owned())))
                    .collect();
                Some((*file, named))
            })
            .collect();
        Some(Wording {
            file: CONTENT_FILES[place],
            root,
            document: values(&texts[place]).map(Toml::Table),
            names,
        })
    }

    /// The words for `path`, from the file's start.
    pub(crate) fn words(&self, path: &ValuePath) -> Vec<Word> {
        let mut words: Vec<Word> = Vec::new();
        let mut node = Some(&self.root);
        let mut value = self.document.as_ref();
        for (place, step) in path.0.iter().enumerate() {
            let through = place + 1;
            match step {
                Step::Key(key) => {
                    let table = node.map(|node| form(&self.root, node, "object"));
                    let property = table
                        .and_then(|table| table.get("properties"))
                        .and_then(|properties| properties.get(key.as_str()));
                    let text = match property {
                        Some(property) => self.title(property).unwrap_or(key).to_owned(),
                        // An id: an entry of the file, or a key of a table keyed by ids.
                        None => {
                            let files = match place {
                                0 => std::slice::from_ref(&self.file),
                                _ => files_of(
                                    table.and_then(|table| names_in(table, "x-names-keys")),
                                ),
                            };
                            self.name_in(files, key).unwrap_or(key).to_owned()
                        }
                    };
                    words.push(Word { text, through });
                    node = table.and_then(|table| child(table, step));
                    value = value.and_then(|value| value.get(key.as_str()));
                }
                Step::Index(index) => {
                    let list = node.map(|node| form(&self.root, node, "array"));
                    let item = list.and_then(|list| child(list, step));
                    value = value.and_then(|value| value.get(*index));
                    let said = |key: &str| list.and_then(|list| list.get(key)?.as_str());
                    let number = index + 1;
                    let text = match (said("x-item"), said("x-title")) {
                        (Some(called), Some(key))
                            if let Some(name) = self.item_name(item, value, key) =>
                        {
                            // A named item stands in for its list.
                            words.pop();
                            format!("{called} \"{name}\"")
                        }
                        (Some(called), _) => format!("{called} {number}"),
                        (None, _) => item
                            .and_then(|item| self.title(item))
                            .map_or_else(|| number.to_string(), str::to_owned),
                    };
                    words.push(Word { text, through });
                    node = item;
                }
            }
        }
        words
    }

    /// The keys written in the table at `path`; none if there's no table there.
    pub(crate) fn keys_at(&self, path: &ValuePath) -> Vec<String> {
        let mut value = self.document.as_ref();
        for step in &path.0 {
            value = match step {
                Step::Key(key) => value.and_then(|value| value.get(key.as_str())),
                Step::Index(index) => value.and_then(|value| value.get(*index)),
            };
        }
        value
            .and_then(Toml::as_table)
            .map(|table| table.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// The name of `id`, as one of `names`; `None` if it has none.
    pub(crate) fn name_of(&self, names: Names, id: &str) -> Option<&str> {
        self.name_in(files_of(Some(names)), id)
    }

    /// The title the schema gives `node`, its own or its definition's.
    fn title<'s>(&'s self, node: &'s Json) -> Option<&'s str> {
        let titled = |node: &'s Json| node.get("title").and_then(Json::as_str);
        titled(node)
            .or_else(|| titled(resolve(&self.root, node)))
            .or_else(|| titled(form(&self.root, node, "")))
    }

    /// The name of the entry `id` of one of `files`; `None` if it has none.
    fn name_in(&self, files: &[&str], id: &str) -> Option<&str> {
        files
            .iter()
            .find_map(|file| self.names.get(*file)?.get(id))
            .map(String::as_str)
    }

    /// The name of a list item, from its key `key`: the name of what it names, if it names
    /// something with one, else as written.
    fn item_name<'v>(
        &'v self,
        item: Option<&Json>,
        value: Option<&'v Toml>,
        key: &str,
    ) -> Option<&'v str> {
        let written = value?.get(key)?.as_str()?;
        let names = item
            .map(|item| form(&self.root, item, "object"))
            .and_then(|item| item.get("properties")?.get(key))
            .and_then(|property| names_in(form(&self.root, property, "string"), "x-names"));
        Some(self.name_in(files_of(names), written).unwrap_or(written))
    }
}

/// The files whose entries `names` are, as far as they have names.
fn files_of(names: Option<Names>) -> &'static [&'static str] {
    match names {
        Some(Names::Faction) => &["factions.toml"],
        Some(Names::Character) => &["characters.toml"],
        Some(Names::Party) => &["factions.toml", "characters.toml"],
        Some(Names::Quest) => &["quests.toml"],
        Some(Names::Outcome | Names::Profile) | None => &[],
    }
}

/// What `node`'s `key` (`x-names` or `x-names-keys`) says it names.
fn names_in(node: &Json, key: &str) -> Option<Names> {
    node.get(key)
        .and_then(Json::as_str)
        .and_then(Names::from_key)
}

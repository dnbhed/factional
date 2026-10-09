//! Editing content text (U2, P-74, P-77): one value at a time, keeping every comment, every
//! space and every other value as written. The editor writes TOML only through here.

use std::fmt;

use toml_edit::{DocumentMut, Item, Table, Value};

/// One step of a path into a file: a key, or a place in a list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// Where a value is in a file, such as `vex.alignment.law` or `relation[2].value`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValuePath(pub Vec<Step>);

/// What kind of value is at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Number,
    Flag,
}

/// One value an entry holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub path: ValuePath,
    pub kind: ValueKind,
    /// As written: text without its quotes, a number as it's spelt.
    pub value: String,
}

/// Why a value can't be set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The file isn't valid TOML, so nothing in it can be found.
    NotToml(String),
    /// There's no text, number or flag at the path.
    NotThere(ValuePath),
    /// The input isn't the kind of value already there.
    Expected(ValueKind),
}

impl ValuePath {
    /// Reads `vex.alignment.law` or `relation[2].value`: keys joined by dots, each followed
    /// by any number of places in brackets.
    pub fn parse(text: &str) -> Option<ValuePath> {
        let mut steps = Vec::new();
        for part in text.split('.') {
            let (key, mut rest) = part
                .split_once('[')
                .map_or((part, ""), |(key, rest)| (key, rest));
            if key.is_empty() {
                return None;
            }
            steps.push(Step::Key(key.to_owned()));
            while !rest.is_empty() {
                let (index, after) = rest.split_once(']')?;
                steps.push(Step::Index(index.parse().ok()?));
                rest = match after {
                    "" => "",
                    more => more.strip_prefix('[')?,
                };
            }
        }
        Some(ValuePath(steps))
    }

    /// This path with `step` after it.
    fn then(&self, step: Step) -> ValuePath {
        let mut steps = self.0.clone();
        steps.push(step);
        ValuePath(steps)
    }
}

impl fmt::Display for ValuePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (place, step) in self.0.iter().enumerate() {
            match step {
                Step::Key(key) if place == 0 => f.write_str(key)?,
                Step::Key(key) => write!(f, ".{key}")?,
                Step::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValueKind::Text => "text",
            ValueKind::Number => "a number",
            ValueKind::Flag => "true or false",
        })
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NotToml(problem) => write!(f, "the file isn't valid TOML: {problem}"),
            EditError::NotThere(path) => write!(f, "there's no value at {path}"),
            EditError::Expected(kind) => write!(f, "expected {kind}"),
        }
    }
}

/// `text` with the value at `path` set to `input`, read as the kind already there: text as
/// typed, a number, or `true` or `false`. Nothing else in the text changes.
pub fn set_value(text: &str, path: &ValuePath, input: &str) -> Result<String, EditError> {
    let mut document: DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| EditError::NotToml(error.message().to_owned()))?;
    let value = in_table(document.as_table_mut(), &path.0)
        .filter(|value| kind_of(value).is_some())
        .ok_or_else(|| EditError::NotThere(path.clone()))?;
    let kind = kind_of(value).expect("filtered to values with a kind");
    let mut new = match kind {
        ValueKind::Text => Value::from(input),
        ValueKind::Number => input
            .parse::<Value>()
            .ok()
            .filter(|parsed| parsed.is_integer() || parsed.is_float())
            .ok_or(EditError::Expected(kind))?,
        ValueKind::Flag => match input {
            "true" => Value::from(true),
            "false" => Value::from(false),
            _ => return Err(EditError::Expected(kind)),
        },
    };
    *new.decor_mut() = value.decor().clone();
    *value = new;
    Ok(document.to_string())
}

/// What kind a value is, if it's one that can be edited.
fn kind_of(value: &Value) -> Option<ValueKind> {
    match value {
        Value::String(_) => Some(ValueKind::Text),
        Value::Integer(_) | Value::Float(_) => Some(ValueKind::Number),
        Value::Boolean(_) => Some(ValueKind::Flag),
        Value::Datetime(_) | Value::Array(_) | Value::InlineTable(_) => None,
    }
}

fn in_table<'a>(table: &'a mut Table, steps: &[Step]) -> Option<&'a mut Value> {
    let (Step::Key(key), rest) = steps.split_first()? else {
        return None;
    };
    in_item(table.get_mut(key)?, rest)
}

fn in_item<'a>(item: &'a mut Item, steps: &[Step]) -> Option<&'a mut Value> {
    let Some((step, rest)) = steps.split_first() else {
        return item.as_value_mut();
    };
    match (item, step) {
        (Item::Value(value), _) => in_value(value, steps),
        (Item::Table(table), Step::Key(_)) => in_table(table, steps),
        (Item::ArrayOfTables(tables), Step::Index(index)) => {
            in_table(tables.get_mut(*index)?, rest)
        }
        _ => None,
    }
}

fn in_value<'a>(value: &'a mut Value, steps: &[Step]) -> Option<&'a mut Value> {
    let Some((step, rest)) = steps.split_first() else {
        return Some(value);
    };
    match (value, step) {
        (Value::InlineTable(table), Step::Key(key)) => in_value(table.get_mut(key)?, rest),
        (Value::Array(array), Step::Index(index)) => in_value(array.get_mut(*index)?, rest),
        _ => None,
    }
}

/// Every value the entry `entry` holds (a top-level key, or one table of a top-level list,
/// such as `relation[2]`), at any depth, in the order written. None if the text isn't TOML or
/// the entry isn't there.
pub fn entry_fields(text: &str, entry: &str) -> Vec<Field> {
    let mut fields = Vec::new();
    let (Ok(document), Some(path)) = (text.parse::<DocumentMut>(), ValuePath::parse(entry)) else {
        return fields;
    };
    match &path.0[..] {
        [Step::Key(key)] => {
            if let Some(item) = document.get(key) {
                walk_item(item, path.clone(), &mut fields);
            }
        }
        [Step::Key(key), Step::Index(index)] => {
            let table = document
                .get(key)
                .and_then(Item::as_array_of_tables)
                .and_then(|tables| tables.get(*index));
            if let Some(table) = table {
                walk_table(table, &path, &mut fields);
            }
        }
        _ => {}
    }
    fields
}

fn walk_table(table: &Table, path: &ValuePath, fields: &mut Vec<Field>) {
    for (key, item) in table.iter() {
        walk_item(item, path.then(Step::Key(key.to_owned())), fields);
    }
}

fn walk_item(item: &Item, path: ValuePath, fields: &mut Vec<Field>) {
    match item {
        Item::Value(value) => walk_value(value, path, fields),
        Item::Table(table) => walk_table(table, &path, fields),
        Item::ArrayOfTables(tables) => {
            for (index, table) in tables.iter().enumerate() {
                walk_table(table, &path.then(Step::Index(index)), fields);
            }
        }
        Item::None => {}
    }
}

fn walk_value(value: &Value, path: ValuePath, fields: &mut Vec<Field>) {
    let field = |kind, value: String| Field {
        path: path.clone(),
        kind,
        value,
    };
    match value {
        Value::String(text) => fields.push(field(ValueKind::Text, text.value().clone())),
        Value::Integer(number) => {
            fields.push(field(ValueKind::Number, number.display_repr().into_owned()));
        }
        Value::Float(number) => {
            fields.push(field(ValueKind::Number, number.display_repr().into_owned()));
        }
        Value::Boolean(flag) => fields.push(field(ValueKind::Flag, flag.value().to_string())),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk_value(item, path.then(Step::Index(index)), fields);
            }
        }
        Value::InlineTable(table) => {
            for (key, item) in table.iter() {
                walk_value(item, path.then(Step::Key(key.to_owned())), fields);
            }
        }
        Value::Datetime(_) => {}
    }
}

/// Each top-level entry of `text` as written, comments directly above it included, keyed
/// as the outline keys them; `None` if the text isn't TOML.
pub(crate) fn entries_as_written(text: &str) -> Option<Vec<(String, String)>> {
    let document = text.parse::<DocumentMut>().ok()?;
    let mut entries = Vec::new();
    for (key, item) in document.iter() {
        match item {
            Item::ArrayOfTables(tables) => {
                for (index, table) in tables.iter().enumerate() {
                    let mut alone = toml_edit::ArrayOfTables::new();
                    alone.push(attached(table));
                    entries.push((
                        format!("{key}[{index}]"),
                        alone_as_text(key, Item::ArrayOfTables(alone)),
                    ));
                }
            }
            Item::Table(table) => {
                entries.push((
                    key.to_owned(),
                    alone_as_text(key, Item::Table(attached(table))),
                ));
            }
            other => entries.push((key.to_owned(), alone_as_text(key, other.clone()))),
        }
    }
    Some(entries)
}

/// A table with only the comments directly above its header: a blank line ends what belongs
/// to it, so a file's opening comments stay with the file.
fn attached(table: &Table) -> Table {
    let mut table = table.clone();
    let prefix = table
        .decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .map(|prefix| {
            let lines: Vec<&str> = prefix.lines().collect();
            let kept = lines
                .iter()
                .rposition(|line| line.trim().is_empty())
                .map_or(&lines[..], |blank| &lines[blank + 1..]);
            kept.iter()
                .map(|line| format!("{line}\n"))
                .collect::<String>()
        });
    if let Some(prefix) = prefix {
        table.decor_mut().set_prefix(prefix);
    }
    table
}

/// `key` with `item` as a document of its own.
fn alone_as_text(key: &str, item: Item) -> String {
    let mut document = DocumentMut::new();
    document.insert(key, item);
    document.to_string()
}

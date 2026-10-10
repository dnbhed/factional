//! Editing content text (U2, U3, P-74, P-77, P-78): setting a value, or adding or removing a
//! key, an entry or a list item, keeping every comment, every space and everything else as
//! written. What can be added, and what it starts as, is the schema's. The editor writes TOML
//! only through here.

use std::fmt;
use std::str::FromStr;

use factional_core::Fixed;
use serde_json::Value as Json;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::schema::schema;

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
    /// Something is already at the path a key would take.
    Taken(ValuePath),
    /// The schema has no such key at the place.
    Unknown(String),
    /// A key needs at least one character that isn't a space.
    Blank,
    /// The place is a value, a table offered an item, a list offered a key, or a list as
    /// long as it may be.
    CantAdd(ValuePath),
}

/// Something that can be added at a place in a file (U3, P-78).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Addition {
    /// A key the schema names that isn't there yet.
    Key(String),
    /// A key of the designer's choosing, such as a new character's id.
    Id,
    /// An item at the end of a list.
    Item,
}

impl ValuePath {
    /// The top of a file, where its entries are.
    pub const ROOT: ValuePath = ValuePath(Vec::new());

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
            EditError::Taken(path) => write!(f, "{path} is already there"),
            EditError::Unknown(key) => write!(f, "'{key}' isn't a key here"),
            EditError::Blank => f.write_str("a key can't be blank"),
            EditError::CantAdd(path) => write!(f, "nothing can be added at {path}"),
        }
    }
}

/// `text` with the value at `path` set to `input`, read as the kind already there: text as
/// typed, a number, or `true` or `false`. Nothing else in the text changes.
pub fn set_value(text: &str, path: &ValuePath, input: &str) -> Result<String, EditError> {
    let mut document = parse(text)?;
    let value = match walk(&mut document, None, &path.0) {
        Some((Place::Value(value), _)) => value,
        _ => return Err(EditError::NotThere(path.clone())),
    };
    let kind = kind_of(value).ok_or_else(|| EditError::NotThere(path.clone()))?;
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

fn parse(text: &str) -> Result<DocumentMut, EditError> {
    text.parse()
        .map_err(|error: toml_edit::TomlError| EditError::NotToml(error.message().to_owned()))
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

/// Somewhere in a document: a table, written with a header or inline; a list, of tables
/// with headers or inline; or a value.
enum Place<'d> {
    Table(&'d mut Table),
    Inline(&'d mut InlineTable),
    Tables(&'d mut ArrayOfTables),
    List(&'d mut Array),
    Value(&'d mut Value),
}

impl<'d> Place<'d> {
    fn of_item(item: &'d mut Item) -> Option<Place<'d>> {
        match item {
            Item::Table(table) => Some(Place::Table(table)),
            Item::ArrayOfTables(tables) => Some(Place::Tables(tables)),
            Item::Value(value) => Some(Place::of_value(value)),
            Item::None => None,
        }
    }

    fn of_value(value: &'d mut Value) -> Place<'d> {
        match value {
            Value::InlineTable(table) => Place::Inline(table),
            Value::Array(list) => Place::List(list),
            value => Place::Value(value),
        }
    }

    fn step(self, step: &Step) -> Option<Place<'d>> {
        match (self, step) {
            (Place::Table(table), Step::Key(key)) => Place::of_item(table.get_mut(key)?),
            (Place::Inline(table), Step::Key(key)) => Some(Place::of_value(table.get_mut(key)?)),
            (Place::Tables(tables), Step::Index(index)) => {
                Some(Place::Table(tables.get_mut(*index)?))
            }
            (Place::List(list), Step::Index(index)) => Some(Place::of_value(list.get_mut(*index)?)),
            _ => None,
        }
    }

    /// The JSON Schema type of what's here.
    fn shape(&self) -> &'static str {
        match self {
            Place::Table(_) | Place::Inline(_) => "object",
            Place::Tables(_) | Place::List(_) => "array",
            Place::Value(Value::String(_)) => "string",
            Place::Value(Value::Integer(_)) => "integer",
            Place::Value(Value::Float(_)) => "number",
            Place::Value(Value::Boolean(_)) => "boolean",
            Place::Value(_) => "",
        }
    }

    fn has(&self, key: &str) -> bool {
        match self {
            Place::Table(table) => table.contains_key(key),
            Place::Inline(table) => table.contains_key(key),
            _ => false,
        }
    }
}

/// The place at `steps` in `document`, with the schema for it if `root`, a file's schema,
/// knows it.
fn walk<'d, 's>(
    document: &'d mut DocumentMut,
    root: Option<&'s Json>,
    steps: &[Step],
) -> Option<(Place<'d>, Option<&'s Json>)> {
    let mut place = Place::Table(document.as_table_mut());
    let mut node = root;
    for step in steps {
        node = match (root, node) {
            (Some(root), Some(node)) => child(form(root, node, place.shape()), step),
            _ => None,
        };
        place = place.step(step)?;
    }
    let node = match (root, node) {
        (Some(root), Some(node)) => Some(form(root, node, place.shape())),
        _ => None,
    };
    Some((place, node))
}

/// `file`'s schema, for a content file such as `characters.toml`.
fn schema_of(file: &str) -> Option<Json> {
    schema(file.strip_suffix(".toml")?)
}

/// `node`, with any `$ref` followed.
fn resolve<'s>(root: &'s Json, mut node: &'s Json) -> &'s Json {
    while let Some(target) = node
        .get("$ref")
        .and_then(Json::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
    {
        node = target;
    }
    node
}

/// The form of `node` for what's there: itself if it says its type, else the first of its
/// `oneOf` forms of the type `shape` (or, with `shape` empty, the first of them).
fn form<'s>(root: &'s Json, node: &'s Json, shape: &str) -> &'s Json {
    let node = resolve(root, node);
    if node.get("type").is_some() {
        return node;
    }
    let fits = |form: &&Json| match form.get("type").and_then(Json::as_str) {
        Some("number") => matches!(shape, "number" | "integer" | ""),
        Some(kind) => shape.is_empty() || kind == shape,
        None => false,
    };
    node.get("oneOf")
        .and_then(Json::as_array)
        .and_then(|forms| forms.iter().map(|form| resolve(root, form)).find(fits))
        .unwrap_or(node)
}

/// The schema for `step` within a table or list `node`.
fn child<'s>(node: &'s Json, step: &Step) -> Option<&'s Json> {
    match step {
        Step::Key(key) => key_schema(node, key),
        Step::Index(index) => item_schema(node, *index),
    }
}

fn key_schema<'s>(node: &'s Json, key: &str) -> Option<&'s Json> {
    node.get("properties")
        .and_then(|properties| properties.get(key))
        .or_else(|| {
            node.get("additionalProperties")
                .filter(|any| any.is_object())
        })
}

fn item_schema(node: &Json, index: usize) -> Option<&Json> {
    let most = node.get("maxItems").and_then(Json::as_u64);
    if most.is_some_and(|most| u64::try_from(index).is_ok_and(|index| index >= most)) {
        return None;
    }
    node.get("prefixItems")
        .and_then(|items| items.get(index))
        .or_else(|| node.get("items"))
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
    let attached = split(prefix_of(table.decor())).1.to_owned();
    table.decor_mut().set_prefix(attached);
    table
}

/// `key` with `item` as a document of its own.
fn alone_as_text(key: &str, item: Item) -> String {
    let mut document = DocumentMut::new();
    document.insert(key, item);
    document.to_string()
}

/// The text before something, such as the comments above a table's header.
fn prefix_of(decor: &toml_edit::Decor) -> &str {
    decor
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .unwrap_or_default()
}

/// Whole lines that are empty or only spaces.
fn is_blank(line: &str) -> bool {
    line.ends_with('\n') && line.trim().is_empty()
}

/// The text before something, split after its last blank line: what's set apart from it,
/// and what's directly above it and belongs to it.
fn split(prefix: &str) -> (&str, &str) {
    let mut end = 0;
    let mut at = 0;
    for line in prefix.split_inclusive('\n') {
        at += line.len();
        if is_blank(line) {
            end = at;
        }
    }
    prefix.split_at(end)
}

/// `text` without its leading blank lines.
fn without_leading_blanks(text: &str) -> &str {
    let mut rest = text;
    while let Some(line) = rest
        .split_inclusive('\n')
        .next()
        .filter(|line| is_blank(line))
    {
        rest = &rest[line.len()..];
    }
    rest
}

/// `text` without its trailing blank lines.
fn without_trailing_blanks(text: &str) -> &str {
    let mut end = 0;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        at += line.len();
        if !is_blank(line) {
            end = at;
        }
    }
    &text[..end]
}

/// What the schema allows to be added at `at` in `file`'s `text`, such as `characters.toml`:
/// in a table, each key it names that isn't there yet, in its order, and an id if it takes
/// any; in a list, an item, while it holds fewer than it may. Nothing if the text isn't TOML
/// or the schema doesn't know the place.
pub fn additions(file: &str, text: &str, at: &ValuePath) -> Vec<Addition> {
    let (Ok(mut document), Some(schema)) = (parse(text), schema_of(file)) else {
        return Vec::new();
    };
    let Some((place, Some(node))) = walk(&mut document, Some(&schema), &at.0) else {
        return Vec::new();
    };
    match place {
        Place::Table(_) | Place::Inline(_) => {
            let mut offered: Vec<Addition> = node
                .get("properties")
                .and_then(Json::as_object)
                .into_iter()
                .flat_map(|properties| properties.keys())
                .filter(|key| !place.has(key))
                .map(|key| Addition::Key(key.clone()))
                .collect();
            if node
                .get("additionalProperties")
                .is_some_and(Json::is_object)
            {
                offered.push(Addition::Id);
            }
            offered
        }
        Place::Tables(tables) => room(node, tables.len()),
        Place::List(list) => room(node, list.len()),
        Place::Value(_) => Vec::new(),
    }
}

/// An item, if a list `node` holding `count` has room for one more.
fn room(node: &Json, count: usize) -> Vec<Addition> {
    match item_schema(node, count) {
        Some(_) => vec![Addition::Item],
        None => Vec::new(),
    }
}

/// Where a file's new entries go: the top, and each list of tables there, such as
/// `relation`. Nothing if the text isn't TOML.
pub fn file_places(text: &str) -> Vec<ValuePath> {
    let Ok(document) = parse(text) else {
        return Vec::new();
    };
    let lists = document
        .iter()
        .filter(|(_, item)| item.is_array_of_tables())
        .map(|(key, _)| ValuePath(vec![Step::Key(key.to_owned())]));
    std::iter::once(ValuePath::ROOT).chain(lists).collect()
}

/// The entry `entry` (a top-level key, or one table of a top-level list, such as
/// `relation[2]`) and every table and list in it, at any depth, in the order written. Nothing
/// if the text isn't TOML or the entry isn't there.
pub fn entry_places(text: &str, entry: &str) -> Vec<ValuePath> {
    let mut places = Vec::new();
    let (Ok(document), Some(path)) = (parse(text), ValuePath::parse(entry)) else {
        return places;
    };
    match &path.0[..] {
        [Step::Key(key)] => {
            if let Some(item) = document.get(key) {
                places.push(path.clone());
                places_within(item, &path, &mut places);
            }
        }
        [Step::Key(key), Step::Index(index)] => {
            let table = document
                .get(key)
                .and_then(Item::as_array_of_tables)
                .and_then(|tables| tables.get(*index));
            if let Some(table) = table {
                places.push(path.clone());
                places_within_table(table, &path, &mut places);
            }
        }
        _ => {}
    }
    places
}

fn places_within_table(table: &Table, path: &ValuePath, places: &mut Vec<ValuePath>) {
    for (key, item) in table.iter() {
        let path = path.then(Step::Key(key.to_owned()));
        match item {
            Item::Value(value) => places_in_value(value, path, places),
            _ => {
                places.push(path.clone());
                places_within(item, &path, places);
            }
        }
    }
}

/// Every table and list within `item`, not counting itself.
fn places_within(item: &Item, path: &ValuePath, places: &mut Vec<ValuePath>) {
    match item {
        Item::Table(table) => places_within_table(table, path, places),
        Item::ArrayOfTables(tables) => {
            for (index, table) in tables.iter().enumerate() {
                let path = path.then(Step::Index(index));
                places.push(path.clone());
                places_within_table(table, &path, places);
            }
        }
        Item::Value(value) => places_within_value(value, path, places),
        Item::None => {}
    }
}

/// `value`, if it's a table or a list, and every table and list within it.
fn places_in_value(value: &Value, path: ValuePath, places: &mut Vec<ValuePath>) {
    if matches!(value, Value::InlineTable(_) | Value::Array(_)) {
        places.push(path.clone());
        places_within_value(value, &path, places);
    }
}

fn places_within_value(value: &Value, path: &ValuePath, places: &mut Vec<ValuePath>) {
    match value {
        Value::InlineTable(table) => {
            for (key, value) in table.iter() {
                places_in_value(value, path.then(Step::Key(key.to_owned())), places);
            }
        }
        Value::Array(list) => {
            for (index, value) in list.iter().enumerate() {
                places_in_value(value, path.then(Step::Index(index)), places);
            }
        }
        _ => {}
    }
}

/// `text` with something added at `at` in `file`, such as `characters.toml`: the key `key`
/// in a table, or, with no key, an item at the end of a list. It starts as the schema says
/// (see [`starting_value`]). At the top of a file, or in a table written only as part of
/// other tables' headers, such as `[inertia.profiles]`, a table is written with its own
/// header and a list of tables as `[[list]]` tables, starting with one, so that each is an
/// entry or sits among its own kind; anywhere else, what's new is written inline, and a list
/// of `[[tables]]` gains one more.
/// Returns the text and the path of what was added.
pub fn add(
    file: &str,
    text: &str,
    at: &ValuePath,
    key: Option<&str>,
) -> Result<(String, ValuePath), EditError> {
    let mut document = parse(text)?;
    let schema = schema_of(file);
    let root = schema.as_ref();
    let (place, node) =
        walk(&mut document, root, &at.0).ok_or_else(|| EditError::NotThere(at.clone()))?;
    let new_key = |has: bool, key: &str| {
        if key.trim().is_empty() {
            return Err(EditError::Blank);
        }
        let path = at.then(Step::Key(key.to_owned()));
        if has {
            return Err(EditError::Taken(path));
        }
        match (root, node.and_then(|node| key_schema(node, key))) {
            (Some(root), Some(child)) => Ok((path, root, child)),
            _ => Err(EditError::Unknown(key.to_owned())),
        }
    };
    let added = match (place, key) {
        (Place::Table(table), Some(key)) => {
            let (path, root, child) = new_key(table.contains_key(key), key)?;
            if at.0.is_empty() || table.is_implicit() {
                let (item, first) = with_header(root, child);
                table.insert(key, item);
                match first {
                    Some(index) => path.then(Step::Index(index)),
                    None => path,
                }
            } else {
                table.insert(key, Item::Value(starting_value(root, child)));
                path
            }
        }
        (Place::Inline(table), Some(key)) => {
            let (path, root, child) = new_key(table.contains_key(key), key)?;
            insert_inline(table, key, starting_value(root, child));
            path
        }
        (Place::List(list), None) => {
            let (Some(root), Some(item)) =
                (root, node.and_then(|node| item_schema(node, list.len())))
            else {
                return Err(EditError::CantAdd(at.clone()));
            };
            let index = list.len();
            push_inline(list, starting_value(root, item));
            at.then(Step::Index(index))
        }
        (Place::Tables(tables), None) => {
            let (Some(root), Some(item)) =
                (root, node.and_then(|node| item_schema(node, tables.len())))
            else {
                return Err(EditError::CantAdd(at.clone()));
            };
            let index = tables.len();
            tables.push(as_table(starting_value(root, item)));
            at.then(Step::Index(index))
        }
        _ => return Err(EditError::CantAdd(at.clone())),
    };
    Ok((document.to_string(), added))
}

/// What a new key is written as where tables have their own headers: a table with its own
/// header, a list of tables as `[[list]]` tables starting with one (and the place of that
/// one), or else a value.
fn with_header(root: &Json, node: &Json) -> (Item, Option<usize>) {
    let node = form(root, node, "");
    let is = |node: &Json, kind: &str| {
        form(root, node, "").get("type").and_then(Json::as_str) == Some(kind)
    };
    // Only a list has a schema for its items.
    let first_table = item_schema(node, 0).filter(|item| is(item, "object"));
    if let Some(first) = first_table {
        let mut list = ArrayOfTables::new();
        list.push(as_table(starting_value(root, first)));
        return (Item::ArrayOfTables(list), Some(0));
    }
    match starting_value(root, node) {
        Value::InlineTable(table) => (Item::Table(table.into_table()), None),
        value => (Item::Value(value), None),
    }
}

/// A table with a header made from an inline one.
fn as_table(value: Value) -> Table {
    match value {
        Value::InlineTable(table) => table.into_table(),
        _ => Table::new(),
    }
}

/// What something new starts as: the schema's default; else the first of a choice of
/// values or forms; text empty, a number 0 or the nearest the schema allows, a flag false;
/// a list with as many starting items as it must have; a table with the keys it must have,
/// counting those of its first form, each at its starting value.
fn starting_value(root: &Json, node: &Json) -> Value {
    let node = form(root, node, "");
    if let Some(default) = node.get("default").and_then(from_json) {
        return default;
    }
    match node.get("type").and_then(Json::as_str) {
        Some("number") => starting_number(node, "0.0"),
        Some("integer") => starting_number(node, "0"),
        Some("boolean") => Value::from(false),
        Some("array") => {
            let least = node
                .get("minItems")
                .and_then(Json::as_u64)
                .and_then(|least| usize::try_from(least).ok())
                .unwrap_or(0);
            let fixed = node
                .get("prefixItems")
                .and_then(Json::as_array)
                .map_or(0, Vec::len);
            let mut list = Array::new();
            for index in 0..least.max(fixed) {
                if let Some(item) = item_schema(node, index) {
                    list.push(starting_value(root, item));
                }
            }
            Value::Array(list)
        }
        Some("object") => {
            let mut table = InlineTable::new();
            let first_form = node
                .get("oneOf")
                .and_then(|forms| forms.get(0))
                .and_then(|form| form.get("required"));
            let required = [node.get("required"), first_form]
                .into_iter()
                .flatten()
                .filter_map(Json::as_array)
                .flatten()
                .filter_map(Json::as_str);
            for key in required {
                if let Some(child) = key_schema(node, key).filter(|_| !table.contains_key(key)) {
                    table.insert(key, starting_value(root, child));
                }
            }
            Value::InlineTable(table)
        }
        _ => Value::from(
            node.get("enum")
                .and_then(|choices| choices.get(0))
                .and_then(Json::as_str)
                .unwrap_or_default(),
        ),
    }
}

/// `zero`, or the schema's bound nearest it if it's out of range.
fn starting_number(node: &Json, zero: &str) -> Value {
    let bound = |name: &str| {
        let text = node.get(name)?.as_number()?.to_string();
        let value = Fixed::from_str(&text).ok()?;
        Some((text, value))
    };
    let text = match (bound("minimum"), bound("maximum")) {
        (Some((least, value)), _) if value > Fixed::ZERO => least,
        (_, Some((most, value))) if value < Fixed::ZERO => most,
        _ => zero.to_owned(),
    };
    text.parse().unwrap_or_else(|_| Value::from(0))
}

/// A JSON default as TOML.
fn from_json(json: &Json) -> Option<Value> {
    Some(match json {
        Json::String(text) => Value::from(text.as_str()),
        Json::Bool(flag) => Value::from(*flag),
        Json::Number(number) => number.to_string().parse().ok()?,
        Json::Array(items) => Value::Array(items.iter().filter_map(from_json).collect()),
        Json::Object(keys) => Value::InlineTable(
            keys.iter()
                .filter_map(|(key, value)| Some((key.clone(), from_json(value)?)))
                .collect(),
        ),
        Json::Null => return None,
    })
}

/// Adds `key` at the end of an inline table, the space before its closing brace moving to
/// after the new value.
fn insert_inline(table: &mut InlineTable, key: &str, mut value: Value) {
    let last = table.iter().last().map(|(last, _)| last.to_owned());
    if let Some(last) = last.and_then(|last| table.get_mut(&last)) {
        value
            .decor_mut()
            .set_suffix(last.decor().suffix().cloned().unwrap_or_default());
        last.decor_mut().set_suffix("");
    }
    table.insert(key, value);
}

/// Adds `value` at the end of an inline list, spaced as the items before it are.
fn push_inline(list: &mut Array, mut value: Value) {
    let count = list.len();
    let Some(last) = list.get_mut(count.saturating_sub(1)) else {
        list.push(value);
        return;
    };
    let prefix = last.decor().prefix().cloned().unwrap_or_default();
    let spaced = count > 1 || prefix.as_str().is_some_and(|prefix| prefix.contains('\n'));
    *value.decor_mut() = last.decor().clone();
    if !spaced {
        value.decor_mut().set_prefix(" ");
    }
    last.decor_mut().set_suffix("");
    list.push_formatted(value);
}

/// `text` without what's at `path`: a key, an entry or a list item. A table or a key goes
/// with the comments directly above it; any set apart by a blank line stay, above what's
/// written next. A list of `[[tables]]` left empty goes too, since TOML can't write it. An
/// inline table or list keeps its spacing.
pub fn remove(text: &str, path: &ValuePath) -> Result<String, EditError> {
    let mut document = parse(text)?;
    let not_there = || EditError::NotThere(path.clone());
    let (last, parent) = path.0.split_last().ok_or_else(not_there)?;
    let (place, _) = walk(&mut document, None, parent).ok_or_else(not_there)?;
    // Comments set apart from what went, to keep above whatever header is written after the
    // position given, and whether the blank lines above that give way to theirs.
    let mut leftover = None;
    match (place, last) {
        (Place::Table(table), Step::Key(key)) => {
            leftover = match table.get(key).ok_or_else(not_there)? {
                Item::Value(_) => remove_line(table, key),
                item => {
                    let (position, prefix) = first_header(item).ok_or_else(not_there)?;
                    Some((Some(position), split(&prefix).0.to_owned(), true))
                }
            };
            table.remove(key);
        }
        (Place::Inline(table), Step::Key(key)) => {
            remove_inline(table, key).ok_or_else(not_there)?;
        }
        (Place::List(list), Step::Index(index)) => {
            remove_item(list, *index).ok_or_else(not_there)?;
        }
        (Place::Tables(tables), Step::Index(index)) => {
            let mut headers = Vec::new();
            headers_in_table(tables.get(*index).ok_or_else(not_there)?, &mut headers);
            let (position, prefix) = first(headers).ok_or_else(not_there)?;
            leftover = Some((Some(position), split(&prefix).0.to_owned(), true));
            tables.remove(*index);
            if tables.is_empty()
                && let Some((Step::Key(key), above)) = parent.split_last()
                && let Some((Place::Table(table), _)) = walk(&mut document, None, above)
            {
                table.remove(key);
            }
        }
        _ => return Err(not_there()),
    }
    if let Some((after, apart, give_way)) = leftover {
        keep_above_next(&mut document, after, &apart, give_way);
    }
    Ok(document.to_string())
}

/// Removes the line of the value `key` from `table`, with the comments directly above it.
/// Any set apart by a blank line go above the next value in the table; with none, they're
/// returned, to go above the next header after the table's.
fn remove_line(table: &mut Table, key: &str) -> Option<(Option<isize>, String, bool)> {
    let prefix = table.key(key).map(|key| prefix_of(key.leaf_decor()));
    let apart = split(prefix.unwrap_or_default()).0.to_owned();
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
    let next = keys
        .iter()
        .skip_while(|other| *other != key)
        .skip(1)
        .find(|other| table.get(other).is_some_and(Item::is_value));
    let Some(mut next) = next.and_then(|next| table.key_mut(next)) else {
        return Some((table.position(), apart, false));
    };
    if let Some(joined) = joined(&apart, prefix_of(next.leaf_decor()), false) {
        next.leaf_decor_mut().set_prefix(joined);
    }
    None
}

/// Removes `key` from an inline table, the space before its closing brace staying.
fn remove_inline(table: &mut InlineTable, key: &str) -> Option<()> {
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
    let gone = table.remove(key)?;
    if keys.last().is_some_and(|last| last == key)
        && let Some(before) = keys.iter().rev().nth(1).and_then(|k| table.get_mut(k))
    {
        before
            .decor_mut()
            .set_suffix(gone.decor().suffix().cloned().unwrap_or_default());
    }
    Some(())
}

/// Removes the `index`th item from an inline list, the space after its opening bracket and
/// before its closing one staying.
fn remove_item(list: &mut Array, index: usize) -> Option<()> {
    if index >= list.len() {
        return None;
    }
    let gone = list.remove(index);
    if index == 0 {
        if let Some(first) = list.get_mut(0) {
            first
                .decor_mut()
                .set_prefix(gone.decor().prefix().cloned().unwrap_or_default());
        }
    } else if index == list.len()
        && let Some(last) = list.get_mut(index - 1)
    {
        last.decor_mut()
            .set_suffix(gone.decor().suffix().cloned().unwrap_or_default());
    }
    Some(())
}

/// The position and prefix of the first header written within `item`, itself included.
fn first_header(item: &Item) -> Option<(isize, String)> {
    let mut headers = Vec::new();
    headers_in(item, &mut headers);
    first(headers)
}

fn first(headers: Vec<(isize, String)>) -> Option<(isize, String)> {
    headers.into_iter().min_by_key(|(position, _)| *position)
}

/// Each header written within `item`: its position, and the text above it.
fn headers_in(item: &Item, headers: &mut Vec<(isize, String)>) {
    match item {
        Item::Table(table) => headers_in_table(table, headers),
        Item::ArrayOfTables(tables) => {
            for table in tables.iter() {
                headers_in_table(table, headers);
            }
        }
        _ => {}
    }
}

fn headers_in_table(table: &Table, headers: &mut Vec<(isize, String)>) {
    if let Some(position) = table.position().filter(|_| !table.is_implicit()) {
        headers.push((position, prefix_of(table.decor()).to_owned()));
    }
    for (_, item) in table.iter() {
        headers_in(item, headers);
    }
}

/// `apart` above `below`, the text above what's next, its blank lines giving way to
/// `apart`'s; or `None`, leaving `below` as it is, if `apart` is empty and not `give_way`.
fn joined(apart: &str, below: &str, give_way: bool) -> Option<String> {
    (give_way || !apart.is_empty()).then(|| format!("{apart}{}", without_leading_blanks(below)))
}

/// Puts `apart` above the first header written after `after` (after nothing: the first of
/// all), as [`joined`] says, or, if there's none, at the end of the file.
fn keep_above_next(document: &mut DocumentMut, after: Option<isize>, apart: &str, give_way: bool) {
    let mut headers = Vec::new();
    for (_, item) in document.iter() {
        headers_in(item, &mut headers);
    }
    let next = headers
        .into_iter()
        .map(|(position, _)| position)
        .filter(|position| after.is_none_or(|after| *position > after))
        .min();
    match next.and_then(|next| header_at(document.as_table_mut(), next)) {
        Some(table) => {
            if let Some(joined) = joined(apart, prefix_of(table.decor()), give_way) {
                table.decor_mut().set_prefix(joined);
            }
        }
        None => {
            let trailing = document.trailing().as_str().unwrap_or_default().to_owned();
            let joined = format!("{}{trailing}", without_trailing_blanks(apart));
            document.set_trailing(joined);
        }
    }
}

/// The table whose header is at `position`.
fn header_at(table: &mut Table, position: isize) -> Option<&mut Table> {
    if table.position() == Some(position) && !table.is_implicit() {
        return Some(table);
    }
    for (_, item) in table.iter_mut() {
        let found = match item {
            Item::Table(table) => header_at(table, position),
            Item::ArrayOfTables(tables) => tables
                .iter_mut()
                .find_map(|table| header_at(table, position)),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn starting(node: Json, zero: &str) -> String {
        starting_number(&node, zero).to_string()
    }

    #[test]
    fn a_number_starts_at_zero_or_the_bound_nearest_it() {
        assert_eq!(starting(json!({}), "0.0"), "0.0");
        assert_eq!(
            starting(json!({ "minimum": 0, "maximum": 0 }), "0.0"),
            "0.0"
        );
        assert_eq!(
            starting(json!({ "minimum": -5, "maximum": 5 }), "0.0"),
            "0.0"
        );
        assert_eq!(starting(json!({ "minimum": 2 }), "0"), "2");
        assert_eq!(starting(json!({ "maximum": -2.5 }), "0.0"), "-2.5");
        assert_eq!(starting(json!({ "minimum": 1, "maximum": -1 }), "0"), "1");
    }
}

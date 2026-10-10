//! The loader's fixes (U6c, P-85): the change a "did you mean" asks for, found in the text at
//! the problem's key, and made through the writer.

use std::fmt;

use toml_edit::{DocumentMut, Item};

use crate::{
    CONTENT_FILES, ContentTexts, Diagnostic, EditError, Step, ValuePath, rename_key, set_value,
};

/// A change to one file that a problem's suggestion asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub file: String,
    pub path: ValuePath,
    pub change: Change,
}

/// What a fix does at its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Sets the text there.
    Set(String),
    /// Renames the key there.
    Rename(String),
}

/// The fix for `diagnostic`, if it suggests a word and the text at its key has the misspelt
/// one: a key of that name in the table there is renamed; a value there that is the word, or
/// names it as part of a quest path such as `watch_oath.patrol.reprt`, is set with the
/// suggestion in its place.
pub fn fix_for(texts: &ContentTexts, diagnostic: &Diagnostic) -> Option<Fix> {
    let suggestion = diagnostic.suggestion.as_ref()?;
    let place = CONTENT_FILES
        .iter()
        .position(|file| *file == diagnostic.file)?;
    let document = texts[place].as_deref()?.parse::<DocumentMut>().ok()?;
    let at = match &diagnostic.key {
        Some(key) => ValuePath::parse(key)?,
        None => ValuePath::ROOT,
    };
    let item = item_at(document.as_item(), &at)?;
    let fix = |path, change| {
        Some(Fix {
            file: diagnostic.file.clone(),
            path,
            change,
        })
    };
    let (wrong, right) = (&suggestion.wrong, &suggestion.right);
    if item
        .as_table_like()
        .is_some_and(|table| table.contains_key(wrong))
    {
        return fix(
            at.then(Step::Key(wrong.clone())),
            Change::Rename(right.clone()),
        );
    }
    let set = corrected(item.as_str()?, wrong, right)?;
    fix(at, Change::Set(set))
}

/// `text` with `fix` made, through the writer.
pub fn apply_fix(text: &str, fix: &Fix) -> Result<String, EditError> {
    match &fix.change {
        Change::Set(value) => set_value(text, &fix.path, value),
        Change::Rename(key) => rename_key(text, &fix.path, key),
    }
}

/// What's at `path` in `item`.
fn item_at<'i>(mut item: &'i Item, path: &ValuePath) -> Option<&'i Item> {
    for step in &path.0 {
        item = match step {
            Step::Key(key) => item.get(key.as_str())?,
            Step::Index(index) => item.get(*index)?,
        };
    }
    Some(item)
}

/// `text` with `wrong` made `right`, whether it's the whole of it or one of its parts between
/// dots; `None` if it's neither.
fn corrected(text: &str, wrong: &str, right: &str) -> Option<String> {
    let parts: Vec<&str> = text.split('.').collect();
    parts.contains(&wrong).then(|| {
        parts
            .iter()
            .map(|part| if *part == wrong { right } else { part })
            .collect::<Vec<_>>()
            .join(".")
    })
}

impl fmt::Display for Fix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.change {
            Change::Set(value) => write!(f, "{}: {}: set it to '{value}'", self.file, self.path),
            Change::Rename(key) => write!(f, "{}: {}: rename it '{key}'", self.file, self.path),
        }
    }
}

//! An outline of a content directory (U1, P-74): each content file, its entries, and every
//! problem and warning the loader reports, at the entry it's about. It's what the editor and
//! the CLI's `outline` show; neither places a diagnostic itself.

use std::path::Path;

use toml::{Table, Value};

use crate::quests::{QUESTLINES_FILE, QUESTS_FILE};
use crate::{
    ACTIONS_FILE, BALANCE_FILE, CHARACTERS_FILE, Diagnostic, FACTIONS_FILE, OUTCOMES_FILE,
    RELATIONS_FILE, Texts, parse_quests, quest_warnings, warnings,
};

/// The content files, in the order an outline lists them.
pub const CONTENT_FILES: [&str; 8] = [
    BALANCE_FILE,
    FACTIONS_FILE,
    CHARACTERS_FILE,
    ACTIONS_FILE,
    RELATIONS_FILE,
    OUTCOMES_FILE,
    QUESTS_FILE,
    QUESTLINES_FILE,
];

/// Everything in a content directory, and the loader's verdict on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    /// Each content file, in [`CONTENT_FILES`]' order.
    pub files: Vec<OutlineFile>,
    /// Problems that aren't about any one file, such as a directory that can't be read.
    pub problems: Vec<Diagnostic>,
}

/// Whether a content file is there to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// Not there: it holds nothing of its kind.
    Missing,
    /// There, but not valid TOML, so its entries can't be listed.
    Unreadable,
    Read,
}

/// One content file: its entries, and what the loader says about it as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineFile {
    pub name: &'static str,
    pub state: FileState,
    /// In id order.
    pub entries: Vec<OutlineEntry>,
    /// Problems and warnings about the file that aren't at one of its entries.
    pub problems: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// One entry of a file: a top-level table, such as `vex`, or one table of a top-level list,
/// such as `relation[2]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub key: String,
    /// The entry as TOML.
    pub toml: String,
    pub problems: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl Outline {
    /// Whether the world in the directory loads: no problems anywhere.
    pub fn loads(&self) -> bool {
        self.problems.is_empty()
            && self.files.iter().all(|file| {
                file.problems.is_empty() && file.entries.iter().all(|e| e.problems.is_empty())
            })
    }

    /// How many problems there are, everywhere.
    pub fn problem_count(&self) -> usize {
        self.problems.len()
            + self
                .files
                .iter()
                .map(|file| {
                    file.problems.len()
                        + file.entries.iter().map(|e| e.problems.len()).sum::<usize>()
                })
                .sum::<usize>()
    }

    /// How many warnings there are, everywhere.
    pub fn warning_count(&self) -> usize {
        self.files
            .iter()
            .map(|file| {
                file.warnings.len() + file.entries.iter().map(|e| e.warnings.len()).sum::<usize>()
            })
            .sum()
    }
}

impl OutlineFile {
    /// `characters.toml — 2 entries`, `balance.toml — not there` or `… — can't be read`.
    pub fn label(&self) -> String {
        match self.state {
            FileState::Missing => format!("{} — not there", self.name),
            FileState::Unreadable => format!("{} — can't be read", self.name),
            FileState::Read => match self.entries.len() {
                1 => format!("{} — 1 entry", self.name),
                count => format!("{} — {count} entries", self.name),
            },
        }
    }
}

impl OutlineEntry {
    /// `hale — 2 problems`, `jobs — 1 problem, 2 warnings`, or just `vex` with neither.
    pub fn label(&self) -> String {
        let count = |n: usize, noun: &str| match n {
            0 => None,
            1 => Some(format!("1 {noun}")),
            n => Some(format!("{n} {noun}s")),
        };
        let said: Vec<String> = [
            count(self.problems.len(), "problem"),
            count(self.warnings.len(), "warning"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if said.is_empty() {
            self.key.clone()
        } else {
            format!("{} — {}", self.key, said.join(", "))
        }
    }

    /// Where in the entry a diagnostic is, such as `alignment.law` for `vex.alignment.law`;
    /// `None` for the entry as a whole.
    pub fn at<'d>(&self, diagnostic: &'d Diagnostic) -> Option<&'d str> {
        let key = diagnostic.key.as_deref()?;
        key.strip_prefix(self.key.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
    }
}

/// The outline of the content in `dir`, with every problem or warning the loader gives.
pub fn outline(dir: &Path) -> Outline {
    let (texts, problems, found) = match Texts::read(dir) {
        Err(error) => (None, error.diagnostics, Vec::new()),
        Ok(texts) => match parse_quests(texts.sources()) {
            Ok((content, quests)) => {
                let mut found = warnings(&content);
                found.extend(quest_warnings(&content, &quests));
                (Some(texts), Vec::new(), found)
            }
            Err(error) => (Some(texts), error.diagnostics, Vec::new()),
        },
    };
    let files = CONTENT_FILES
        .iter()
        .map(|name| {
            let mut file = read_file(name, texts.as_ref().and_then(|texts| texts.of(name)));
            for problem in problems.iter().filter(|d| d.file == *name) {
                file.place(problem.clone(), true);
            }
            for warning in found.iter().filter(|d| d.file == *name) {
                file.place(warning.clone(), false);
            }
            file
        })
        .collect();
    let problems = problems
        .into_iter()
        .filter(|d| !CONTENT_FILES.contains(&d.file.as_str()))
        .collect();
    Outline { files, problems }
}

/// A file's entries, from its text; none if it isn't there or isn't TOML.
fn read_file(name: &'static str, text: Option<&str>) -> OutlineFile {
    let mut file = OutlineFile {
        name,
        state: FileState::Missing,
        entries: Vec::new(),
        problems: Vec::new(),
        warnings: Vec::new(),
    };
    let Some(text) = text else {
        return file;
    };
    let Ok(table) = text.parse::<Table>() else {
        file.state = FileState::Unreadable;
        return file;
    };
    file.state = FileState::Read;
    for (key, value) in table {
        let tables = match &value {
            Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_table) => {
                Some(items.clone())
            }
            _ => None,
        };
        match tables {
            Some(items) => {
                for (index, item) in items.into_iter().enumerate() {
                    let toml = as_toml(&key, Value::Array(vec![item]));
                    file.entries.push(entry(format!("{key}[{index}]"), toml));
                }
            }
            None => {
                let toml = as_toml(&key, value);
                file.entries.push(entry(key, toml));
            }
        }
    }
    file
}

fn entry(key: String, toml: String) -> OutlineEntry {
    OutlineEntry {
        key,
        toml,
        problems: Vec::new(),
        warnings: Vec::new(),
    }
}

/// `key = value` as a TOML document.
fn as_toml(key: &str, value: Value) -> String {
    let mut table = Table::new();
    table.insert(key.to_owned(), value);
    toml::to_string(&table).expect("a table of values always writes")
}

impl OutlineFile {
    /// Puts a diagnostic at the entry its key starts with, or else with the file.
    fn place(&mut self, diagnostic: Diagnostic, problem: bool) {
        let entry = diagnostic.key.as_deref().and_then(|key| {
            let head = key.split('.').next().unwrap_or(key);
            self.entries.iter_mut().find(|entry| entry.key == head)
        });
        let list = match (entry, problem) {
            (Some(entry), true) => &mut entry.problems,
            (Some(entry), false) => &mut entry.warnings,
            (None, true) => &mut self.problems,
            (None, false) => &mut self.warnings,
        };
        list.push(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// A fresh directory under the test target, with `files` written into it.
    fn dir_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("factional_outline_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a fresh directory");
        for (file, text) in files {
            fs::write(dir.join(file), text).expect("written");
        }
        dir
    }

    fn diagnostic(file: &str, key: Option<&str>, message: &str) -> Diagnostic {
        Diagnostic {
            file: file.to_owned(),
            key: key.map(str::to_owned),
            message: message.to_owned(),
        }
    }

    #[test]
    fn every_content_file_is_listed_in_order_whether_or_not_its_there() {
        let outline = outline(&dir_with("empty", &[]));
        let names: Vec<&str> = outline.files.iter().map(|file| file.name).collect();
        assert_eq!(names, CONTENT_FILES);
        assert!(
            outline
                .files
                .iter()
                .all(|file| file.state == FileState::Missing)
        );
        assert!(outline.loads());
        assert_eq!((outline.problem_count(), outline.warning_count()), (0, 0));
    }

    #[test]
    fn each_file_is_read_from_its_own_name() {
        let files: Vec<(&str, &str)> = CONTENT_FILES.iter().map(|file| (*file, "[x]\n")).collect();
        let outline = outline(&dir_with("all", &files));
        assert!(
            outline
                .files
                .iter()
                .all(|file| file.state == FileState::Read),
            "{outline:?}"
        );
    }

    #[test]
    fn entries_are_top_level_tables_and_each_table_of_a_top_level_list() {
        let dir = dir_with(
            "entries",
            &[
                (
                    "factions.toml",
                    "[watch]\nname = \"The Watch\"\nalignment = { law = 60.0, good = 0.0 }\ntolerance = 40.0\n\n[[watch.ranks]]\nid = \"recruit\"\n",
                ),
                (
                    "relations.toml",
                    "[[relation]]\nbetween = [\"watch\", \"watch\"]\nvalue = 1.0\n\n[[relation]]\nfrom = \"watch\"\nto = \"nobody\"\nvalue = 2.0\n",
                ),
            ],
        );
        let outline = outline(&dir);
        let keys = |file: usize| -> Vec<&str> {
            outline.files[file]
                .entries
                .iter()
                .map(|e| e.key.as_str())
                .collect()
        };
        assert_eq!(keys(1), ["watch"]);
        assert_eq!(keys(4), ["relation[0]", "relation[1]"]);
        assert!(outline.files[4].entries[1].toml.starts_with("[[relation]]"));
        assert!(
            outline.files[1].entries[0]
                .toml
                .contains("name = \"The Watch\"")
        );
        // A relation with itself, and one with a faction that doesn't exist: each at its own.
        assert_eq!(outline.files[4].entries[0].problems.len(), 1);
        assert_eq!(outline.files[4].entries[1].problems.len(), 1);
        assert!(!outline.loads());
        assert_eq!(outline.problem_count(), 2);
    }

    #[test]
    fn a_list_of_values_is_one_entry() {
        let dir = dir_with("values", &[("balance.toml", "strange = [1, 2]\n")]);
        let entries = &outline(&dir).files[0].entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "strange");
        assert_eq!(entries[0].toml, "strange = [1, 2]\n");
        let dir = dir_with("no_values", &[("balance.toml", "strange = []\n")]);
        assert_eq!(outline(&dir).files[0].entries[0].key, "strange");
    }

    #[test]
    fn a_problem_not_at_an_entry_stays_with_its_file_or_the_outline() {
        let outline = outline(&dir_with("not_toml", &[("characters.toml", "[vex\n")]));
        let characters = &outline.files[2];
        assert_eq!(characters.state, FileState::Unreadable);
        assert!(characters.entries.is_empty());
        assert_eq!(characters.problems.len(), 1);
        assert!(!outline.loads());
        assert_eq!(outline.problem_count(), 1);
        let missing = super::outline(&std::env::temp_dir().join("factional_outline_nowhere"));
        assert_eq!(missing.problems.len(), 1);
        assert!(!missing.loads());
        assert_eq!(missing.problem_count(), 1);
    }

    #[test]
    fn a_diagnostic_is_placed_by_its_keys_first_part() {
        let mut file = read_file(
            CHARACTERS_FILE,
            Some("[vex]\nname = \"Vex\"\n[vexed]\nname = \"V\"\n"),
        );
        file.place(
            diagnostic(CHARACTERS_FILE, Some("vex.alignment.law"), "far"),
            true,
        );
        file.place(diagnostic(CHARACTERS_FILE, Some("vexed"), "odd"), false);
        file.place(diagnostic(CHARACTERS_FILE, Some("line 3"), "broken"), true);
        file.place(diagnostic(CHARACTERS_FILE, None, "whole"), false);
        assert_eq!(file.entries[0].problems.len(), 1);
        assert_eq!(file.entries[1].warnings.len(), 1);
        assert_eq!((file.problems.len(), file.warnings.len()), (1, 1));
        let vex = &file.entries[0];
        assert_eq!(vex.at(&vex.problems[0]), Some("alignment.law"));
        assert_eq!(file.entries[1].at(&file.entries[1].warnings[0]), None);
        assert_eq!(
            vex.at(&diagnostic(CHARACTERS_FILE, Some("vexed.name"), "x")),
            None
        );
        assert_eq!(vex.at(&diagnostic(CHARACTERS_FILE, None, "x")), None);
    }

    #[test]
    fn warnings_are_counted_but_dont_stop_loading() {
        let dir = dir_with(
            "warned",
            &[(
                "factions.toml",
                "[watch]\nname = \"The Watch\"\nalignment = { law = 60.0, good = 0.0 }\ntolerance = 40.0\n\n[[watch.ranks]]\nid = \"recruit\"\n",
            )],
        );
        let outline = outline(&dir);
        assert!(outline.loads());
        assert_eq!(outline.warning_count(), 1);
        assert_eq!(outline.files[1].entries[0].warnings.len(), 1);
        assert_eq!(outline.files[1].entries[0].label(), "watch — 1 warning");
    }

    #[test]
    fn labels_say_what_a_file_and_an_entry_hold() {
        let mut file = read_file(CHARACTERS_FILE, Some("[vex]\nname = \"Vex\"\n"));
        assert_eq!(file.label(), "characters.toml — 1 entry");
        let entry = |problems: usize, warnings: usize| OutlineEntry {
            key: "hale".to_owned(),
            toml: String::new(),
            problems: vec![diagnostic(CHARACTERS_FILE, None, "p"); problems],
            warnings: vec![diagnostic(CHARACTERS_FILE, None, "w"); warnings],
        };
        assert_eq!(entry(0, 0).label(), "hale");
        assert_eq!(entry(1, 0).label(), "hale — 1 problem");
        assert_eq!(entry(2, 1).label(), "hale — 2 problems, 1 warning");
        assert_eq!(entry(0, 3).label(), "hale — 3 warnings");
        file.entries.push(entry(0, 0));
        assert_eq!(file.label(), "characters.toml — 2 entries");
        file.entries.clear();
        assert_eq!(file.label(), "characters.toml — 0 entries");
        assert_eq!(
            read_file(BALANCE_FILE, None).label(),
            "balance.toml — not there"
        );
        assert_eq!(
            read_file(BALANCE_FILE, Some("[x")).label(),
            "balance.toml — can't be read"
        );
    }
}

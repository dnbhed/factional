//! The Factional editor (DESIGN.md §18, D-32, P-74): a view of a content directory and of
//! everything the loader says about it, where each value can be changed. It places no
//! diagnostic, checks nothing and writes no TOML itself: the outline and the writer are
//! `factional-content`'s.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use factional_content::{
    CONTENT_FILES, ContentTexts, Diagnostic, EditError, Field, FileState, Outline, OutlineEntry,
    entry_fields, outline, outline_texts, read_texts, set_value,
};

pub use eframe::egui;

/// The editor's state: the directory; each file's text as edited and as saved; the changes
/// that can be undone; the outline of the text as edited; and the selected entry, with its
/// values as fields.
pub struct Editor {
    dir: PathBuf,
    /// `None` if the directory couldn't be read, so there's nothing to edit.
    texts: Option<ContentTexts>,
    saved: Option<ContentTexts>,
    undo: Vec<ContentTexts>,
    outline: Outline,
    selected: Option<(usize, usize)>,
    fields: Vec<FieldInput>,
    /// Something to say at the top, such as a save that failed.
    note: Option<String>,
}

/// One of the selected entry's values, as its field shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInput {
    pub field: Field,
    /// What's typed in the field.
    pub input: String,
    /// Why the last value entered here was refused.
    pub refused: Option<String>,
}

impl Editor {
    /// Opens `dir`, reading its files.
    pub fn open(dir: impl Into<PathBuf>) -> Editor {
        let mut editor = Editor {
            dir: dir.into(),
            texts: None,
            saved: None,
            undo: Vec::new(),
            outline: Outline {
                files: Vec::new(),
                problems: Vec::new(),
            },
            selected: None,
            fields: Vec::new(),
            note: None,
        };
        editor.reload();
        editor
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn outline(&self) -> &Outline {
        &self.outline
    }

    /// The selected entry's values.
    pub fn fields(&self) -> &[FieldInput] {
        &self.fields
    }

    /// Whether anything has changed since the files were read or saved.
    pub fn has_changes(&self) -> bool {
        self.texts != self.saved
    }

    /// Whether there's a change to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Reads the directory again, discarding unsaved changes and what could be undone, and
    /// keeping the selection if that entry is still there.
    pub fn reload(&mut self) {
        self.texts = read_texts(&self.dir).ok();
        self.saved = self.texts.clone();
        self.undo.clear();
        self.note = None;
        self.refresh();
    }

    /// Sets the `field`th value of the selected entry to `input`, through the writer: the
    /// file's text in memory changes, and the outline is made again. Nothing changes if the
    /// writer refuses.
    pub fn set(&mut self, field: usize, input: &str) -> Result<(), EditError> {
        let (Some((file, _)), Some(texts), Some(field)) =
            (self.selected, self.texts.as_mut(), self.fields.get(field))
        else {
            return Ok(());
        };
        let Some(text) = &texts[file] else {
            return Ok(());
        };
        let changed = set_value(text, &field.field.path, input)?;
        self.undo.push(texts.clone());
        texts[file] = Some(changed);
        self.refresh();
        Ok(())
    }

    /// Steps back one change.
    pub fn undo(&mut self) {
        if let Some(before) = self.undo.pop() {
            self.texts = Some(before);
            self.refresh();
        }
    }

    /// Writes every file that has changed.
    pub fn save(&mut self) -> io::Result<()> {
        let (Some(texts), Some(saved)) = (&self.texts, &mut self.saved) else {
            return Ok(());
        };
        for (place, name) in CONTENT_FILES.iter().enumerate() {
            if texts[place] != saved[place]
                && let Some(text) = &texts[place]
            {
                fs::write(self.dir.join(name), text)?;
                saved[place] = Some(text.clone());
            }
        }
        Ok(())
    }

    /// Makes the outline again from the text as edited, keeping the selection if that entry
    /// is still there, and shows its values afresh.
    fn refresh(&mut self) {
        let was = self
            .selected_entry()
            .map(|(file, entry)| (file.to_owned(), entry.key.clone()));
        self.outline = match &self.texts {
            Some(texts) => outline_texts(texts),
            None => outline(&self.dir),
        };
        self.selected = was.and_then(|(file, key)| {
            let place = self.outline.files.iter().position(|f| f.name == file)?;
            let entry = self.outline.files[place]
                .entries
                .iter()
                .position(|e| e.key == key)?;
            Some((place, entry))
        });
        self.show_fields();
    }

    /// The selected entry's values, as written.
    fn show_fields(&mut self) {
        let Some((file, entry)) = self.selected else {
            self.fields.clear();
            return;
        };
        let text = self
            .texts
            .as_ref()
            .and_then(|texts| texts[file].as_deref())
            .unwrap_or_default();
        let key = &self.outline.files[file].entries[entry].key;
        self.fields = entry_fields(text, key)
            .into_iter()
            .map(|field| FieldInput {
                input: field.value.clone(),
                field,
                refused: None,
            })
            .collect();
    }

    /// Selects the `entry`th entry of the `file`th file; nothing if there's no such entry.
    pub fn select(&mut self, file: usize, entry: usize) {
        let there = self
            .outline
            .files
            .get(file)
            .is_some_and(|f| entry < f.entries.len());
        if there {
            self.selected = Some((file, entry));
            self.show_fields();
        }
    }

    /// The selected entry, with its file's name.
    pub fn selected_entry(&self) -> Option<(&'static str, &OutlineEntry)> {
        let (file, entry) = self.selected?;
        let file = &self.outline.files[file];
        Some((file.name, &file.entries[entry]))
    }

    /// `… loads`, `… loads, with 2 warnings` or `… doesn't load: 3 problems`.
    pub fn summary(&self) -> String {
        let dir = self.dir.display();
        let plural = |n: usize, noun: &str| match n {
            1 => format!("1 {noun}"),
            n => format!("{n} {noun}s"),
        };
        if !self.outline.loads() {
            return format!(
                "{dir} doesn't load: {}",
                plural(self.outline.problem_count(), "problem")
            );
        }
        match self.outline.warning_count() {
            0 => format!("{dir} loads"),
            warnings => format!("{dir} loads, with {}", plural(warnings, "warning")),
        }
    }

    /// Draws the editor into `ui`: buttons and the summary at the top, the files and their
    /// entries on the left, the selected entry's values in the middle.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("summary").show(ui, |ui| {
            // The buttons first, so a long directory never pushes them out of reach.
            ui.horizontal(|ui| {
                if ui.button("Read again").clicked() {
                    self.reload();
                }
                if ui
                    .add_enabled(self.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.has_changes(), egui::Button::new("Save"))
                    .clicked()
                {
                    self.note = self
                        .save()
                        .err()
                        .map(|error| format!("couldn't save: {error}"));
                }
                ui.label(self.summary());
                if let Some(note) = &self.note {
                    ui.label(note);
                }
            });
        });
        let mut clicked = None;
        egui::Panel::left("files")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (place, file) in self.outline.files.iter().enumerate() {
                        if file.state != FileState::Read {
                            ui.label(file.label());
                            diagnostics(ui, &file.problems, &file.warnings, |d| d.key.as_deref());
                            continue;
                        }
                        egui::CollapsingHeader::new(file.label())
                            .id_salt(file.name)
                            .default_open(true)
                            .show(ui, |ui| {
                                diagnostics(ui, &file.problems, &file.warnings, |d| {
                                    d.key.as_deref()
                                });
                                for (index, entry) in file.entries.iter().enumerate() {
                                    let chosen = self.selected == Some((place, index));
                                    if ui.selectable_label(chosen, entry.label()).clicked() {
                                        clicked = Some((place, index));
                                    }
                                }
                            });
                    }
                    for problem in &self.outline.problems {
                        ui.label(format!("error: {problem}"));
                    }
                });
            });
        if let Some((file, entry)) = clicked {
            self.select(file, entry);
        }
        let mut entered = None;
        egui::CentralPanel::default().show(ui, |ui| {
            let Some((file, entry)) = self.selected_entry() else {
                ui.label(
                    "Choose an entry on the left to see it and what the loader says about it.",
                );
                return;
            };
            ui.heading(format!("{file}: {}", entry.key));
            diagnostics(ui, &entry.problems, &entry.warnings, |d| entry.at(d));
            ui.separator();
            let prefix = format!("{}.", entry.key);
            let toml = entry.toml.trim_end().to_owned();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("fields").num_columns(3).show(ui, |ui| {
                    for (place, field) in self.fields.iter_mut().enumerate() {
                        let path = field.field.path.to_string();
                        let name = path.strip_prefix(&prefix).unwrap_or(&path);
                        let label = ui.label(name);
                        let edited = ui
                            .add(egui::TextEdit::singleline(&mut field.input).id_salt(&path))
                            .labelled_by(label.id);
                        if edited.lost_focus() && field.input != field.field.value {
                            entered = Some((place, field.input.clone()));
                        }
                        if let Some(refused) = &field.refused {
                            ui.label(refused);
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                egui::CollapsingHeader::new("As written")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.monospace(toml);
                    });
            });
        });
        if let Some((place, input)) = entered
            && let Err(refused) = self.set(place, &input)
        {
            self.fields[place].refused = Some(refused.to_string());
        }
    }
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Editor::ui(self, ui);
    }
}

/// Each problem, then each warning, with where it is if `at` says.
fn diagnostics<'d>(
    ui: &mut egui::Ui,
    problems: &'d [Diagnostic],
    warnings: &'d [Diagnostic],
    at: impl Fn(&'d Diagnostic) -> Option<&'d str>,
) {
    let problems = problems.iter().map(|d| ("error", d));
    let warnings = warnings.iter().map(|d| ("warning", d));
    for (kind, diagnostic) in problems.chain(warnings) {
        let said = match at(diagnostic) {
            Some(key) => format!("{kind}: {key}: {}", diagnostic.message),
            None => format!("{kind}: {}", diagnostic.message),
        };
        ui.label(said);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

    fn broken() -> Editor {
        Editor::open(Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken"))
    }

    #[test]
    fn nothing_is_selected_until_an_entry_that_exists_is() {
        let mut editor = broken();
        assert!(editor.selected_entry().is_none());
        editor.select(2, 2);
        editor.select(9, 0);
        assert!(editor.selected_entry().is_none());
        editor.select(2, 0);
        let (file, entry) = editor.selected_entry().expect("selected");
        assert_eq!((file, entry.key.as_str()), ("characters.toml", "hale"));
        assert!(editor.dir().ends_with("broken"));
        assert_eq!(editor.outline().files.len(), 8);
    }

    #[test]
    fn the_summary_says_whether_the_world_loads() {
        let editor = broken();
        assert!(
            editor
                .summary()
                .ends_with("broken doesn't load: 3 problems")
        );
        let odd_jobs =
            Editor::open(Path::new(REPO).join("crates/cli/tests/fixtures/worlds/odd_jobs"));
        assert!(
            odd_jobs
                .summary()
                .ends_with("odd_jobs loads, with 2 warnings")
        );
        let sample = Editor::open(Path::new(REPO).join("content/sample"));
        assert!(sample.summary().ends_with("sample loads"));
        let dir = std::env::temp_dir().join("factional_editor_one_problem");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a fresh directory");
        fs::write(dir.join("characters.toml"), "[vex\n").expect("written");
        assert!(
            Editor::open(&dir)
                .summary()
                .ends_with("doesn't load: 1 problem")
        );
        fs::write(dir.join("factions.toml"), "[watch]\nname = \"W\"\nalignment = { law = 0.0, good = 0.0 }\ntolerance = 40.0\n\n[[watch.ranks]]\nid = \"r\"\n").expect("written");
        fs::remove_file(dir.join("characters.toml")).expect("removed");
        assert!(
            Editor::open(&dir)
                .summary()
                .ends_with("loads, with 1 warning")
        );
    }

    #[test]
    fn reading_again_keeps_the_selection_while_its_entry_is_there() {
        let dir = std::env::temp_dir().join("factional_editor_reload");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a fresh directory");
        let both = "[hale]\nname = \"Hale\"\n\n[vex]\nname = \"Vex\"\n";
        fs::write(dir.join("characters.toml"), both).expect("written");
        let mut editor = Editor::open(&dir);
        editor.select(2, 1);
        fs::write(
            dir.join("characters.toml"),
            format!("[ava]\nname = \"Ava\"\n\n{both}"),
        )
        .expect("written");
        editor.reload();
        let (_, entry) = editor.selected_entry().expect("still selected");
        assert_eq!(entry.key, "vex");
        fs::write(dir.join("characters.toml"), "[ava]\nname = \"Ava\"\n").expect("written");
        editor.reload();
        assert!(editor.selected_entry().is_none());
        fs::remove_file(dir.join("characters.toml")).expect("removed");
        editor.reload();
        assert!(editor.selected_entry().is_none());
    }

    /// A directory holding `characters`, as `name`.
    fn with_characters(name: &str, characters: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("factional_editor_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a fresh directory");
        fs::write(dir.join("characters.toml"), characters).expect("written");
        dir
    }

    const VEX: &str = "# Vex.\n[vex]\nname = \"Vex\"\nalignment = { law = 120.0, good = -20.0 }\n";

    #[test]
    fn a_change_goes_through_the_writer_into_the_text_in_memory() {
        let dir = with_characters("set", VEX);
        let mut editor = Editor::open(&dir);
        assert_eq!(
            editor.set(1, "20.0"),
            Ok(()),
            "nothing selected: nothing to set"
        );
        assert!(!editor.has_changes() && !editor.can_undo());
        editor.select(2, 0);
        let law: Vec<&str> = editor.fields().iter().map(|f| f.input.as_str()).collect();
        assert_eq!(law, ["Vex", "120.0", "-20.0"]);
        assert_eq!(
            editor.set(1, "fast"),
            Err(EditError::Expected(factional_content::ValueKind::Number))
        );
        assert!(!editor.has_changes() && !editor.can_undo());
        assert_eq!(editor.set(9, "1"), Ok(()), "no such field: nothing to set");
        assert_eq!(editor.set(1, "20.0"), Ok(()));
        assert!(editor.has_changes() && editor.can_undo());
        assert_eq!(editor.fields()[1].input, "20.0");
        assert!(editor.outline().loads());
        assert_eq!(
            fs::read_to_string(dir.join("characters.toml")).expect("there"),
            VEX
        );
    }

    #[test]
    fn undo_steps_back_through_each_change() {
        let dir = with_characters("undo", VEX);
        let mut editor = Editor::open(&dir);
        editor.select(2, 0);
        editor.set(1, "20.0").expect("set");
        editor.set(2, "-10.0").expect("set");
        editor.undo();
        assert_eq!(editor.fields()[2].input, "-20.0");
        assert_eq!(editor.fields()[1].input, "20.0");
        editor.undo();
        assert_eq!(editor.fields()[1].input, "120.0");
        assert!(!editor.has_changes() && !editor.can_undo());
        editor.undo();
        assert_eq!(editor.fields()[1].input, "120.0");
    }

    #[test]
    fn save_writes_only_what_changed_and_keeps_what_can_be_undone() {
        let dir = with_characters("save", VEX);
        fs::write(dir.join("factions.toml"), "[watch\n").expect("written");
        let mut editor = Editor::open(&dir);
        editor.select(2, 0);
        editor.set(1, "20.0").expect("set");
        // A file not changed isn't written, even if it's broken.
        fs::write(dir.join("factions.toml"), "# changed outside\n").expect("written");
        editor.save().expect("saved");
        assert!(!editor.has_changes() && editor.can_undo());
        assert_eq!(
            fs::read_to_string(dir.join("characters.toml")).expect("there"),
            VEX.replacen("law = 120.0", "law = 20.0", 1)
        );
        assert_eq!(
            fs::read_to_string(dir.join("factions.toml")).expect("there"),
            "# changed outside\n"
        );
        editor.undo();
        assert!(editor.has_changes());
    }

    #[test]
    fn a_save_that_fails_says_so() {
        let dir = with_characters("save_fails", VEX);
        let mut editor = Editor::open(&dir);
        editor.select(2, 0);
        editor.set(1, "20.0").expect("set");
        fs::remove_dir_all(&dir).expect("removed");
        assert!(editor.save().is_err());
        assert!(editor.has_changes());
        let mut nowhere = Editor::open(std::env::temp_dir().join("factional_editor_nowhere"));
        assert!(nowhere.save().is_ok());
        assert!(!nowhere.has_changes());
        assert_eq!(nowhere.outline().problems.len(), 1);
    }
}

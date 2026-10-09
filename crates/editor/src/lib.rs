//! The Factional editor (DESIGN.md §18, D-32, P-74): a view of a content directory and of
//! everything the loader says about it. It places no diagnostic and checks nothing itself:
//! that's the outline's, from `factional-content`.

use std::path::{Path, PathBuf};

use factional_content::{Diagnostic, FileState, Outline, OutlineEntry, outline};

pub use eframe::egui;

/// The editor's state: the directory, its outline, and the entry selected, as a file's and
/// an entry's places.
pub struct Editor {
    dir: PathBuf,
    outline: Outline,
    selected: Option<(usize, usize)>,
}

impl Editor {
    /// Opens `dir`, reading its outline.
    pub fn open(dir: impl Into<PathBuf>) -> Editor {
        let dir = dir.into();
        let outline = outline(&dir);
        Editor {
            dir,
            outline,
            selected: None,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn outline(&self) -> &Outline {
        &self.outline
    }

    /// Reads the directory again, keeping the selection if that entry is still there.
    pub fn reload(&mut self) {
        let was = self
            .selected_entry()
            .map(|(file, entry)| (file.to_owned(), entry.key.clone()));
        self.outline = outline(&self.dir);
        self.selected = was.and_then(|(file, key)| {
            let place = self.outline.files.iter().position(|f| f.name == file)?;
            let entry = self.outline.files[place]
                .entries
                .iter()
                .position(|e| e.key == key)?;
            Some((place, entry))
        });
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

    /// Draws the editor into `ui`: a button to read again and the summary at the top, the
    /// files and their entries on the left, the selected entry in the middle.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("summary").show(ui, |ui| {
            // The button first, so a long directory never pushes it out of reach.
            ui.horizontal(|ui| {
                if ui.button("Read again").clicked() {
                    self.reload();
                }
                ui.label(self.summary());
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
        egui::CentralPanel::default().show(ui, |ui| match self.selected_entry() {
            None => {
                ui.label(
                    "Choose an entry on the left to see it and what the loader says about it.",
                );
            }
            Some((file, entry)) => {
                ui.heading(format!("{file}: {}", entry.key));
                diagnostics(ui, &entry.problems, &entry.warnings, |d| entry.at(d));
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(entry.toml.trim_end());
                });
            }
        });
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
}

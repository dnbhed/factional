//! The Factional editor (DESIGN.md §18, D-32, P-74): a view of a content directory and of
//! everything the loader says about it, where each value can be changed and keys, entries
//! and list items added and removed. It places no diagnostic, checks nothing, offers nothing
//! the schema doesn't and writes no TOML itself: the outline, the schema and the writer are
//! `factional-content`'s.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use factional_content::{
    Addition, CONTENT_FILES, ContentTexts, Diagnostic, EditError, FileState, FormRow, Outline,
    OutlineEntry, QuestGraph, Reference, Step, ValuePath, additions, entry_form, entry_places,
    file_places, load_texts, outline, outline_texts, read_texts, referenced_by, set_value,
};
use factional_core::Curve;
use factional_quests::{QuestId, QuestLog, Quests, StartAssessment};
use factional_reputation::{CharacterId, World};

pub use eframe::egui;

/// The window's size when it opens, as the design canvas draws it (1440 by 900), so the
/// entries, the form and what's beside it all fit.
pub const WINDOW: egui::Vec2 = egui::vec2(1440.0, 900.0);

/// How the editor's window opens: at [`WINDOW`]'s size.
pub fn options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(WINDOW),
        ..eframe::NativeOptions::default()
    }
}

mod form_ui;
mod layout;

use form_ui::Acted;
mod previews_ui;
mod quests_ui;

/// The file whose entries are quests, for "Edit in Content".
const QUESTS_FILE: &str = "quests.toml";

/// The files whose entries other content can name, so the Content tab says what refers to
/// them.
const NAMED_FILES: [&str; 4] = [
    "factions.toml",
    "characters.toml",
    "quests.toml",
    "outcomes.toml",
];

/// The editor's state: the directory; each file's text as edited and as saved; the changes
/// that can be undone; the outline of the text as edited, with where each file's entries go;
/// and the selected entry, with its form's values as fields, its places, and what refers to
/// it.
pub struct Editor {
    dir: PathBuf,
    /// `None` if the directory couldn't be read, so there's nothing to edit.
    texts: Option<ContentTexts>,
    saved: Option<ContentTexts>,
    undo: Vec<ContentTexts>,
    outline: Outline,
    selected: Option<(usize, usize)>,
    fields: Vec<FieldInput>,
    places: Vec<PlaceInput>,
    /// What names the selected entry; `None` if nothing can, as for a relation.
    referenced: Option<Vec<Reference>>,
    /// By file, in [`CONTENT_FILES`]' order.
    file_places: Vec<Vec<PlaceInput>>,
    /// Something to say at the top, such as a save that failed.
    note: Option<String>,
    workspace: Workspace,
    /// The quests as graphs, made again with the outline.
    graph: QuestGraph,
    /// The questline or quest drawn in the Quests tab.
    shown: Option<Shown>,
    /// The quest whose stages and choices show beside the graph.
    chosen: Option<String>,
    /// The world built from the last content that loaded, for the previews (D-35).
    preview: Option<World>,
    /// The faction whose alignment map is shown.
    map_faction: Option<String>,
    /// The quests of the previewed world, for whether a character can start one.
    preview_quests: Quests,
    /// The knob whose curve is shown.
    curve: Option<String>,
    /// The character and quest asked about: can they start it?
    starter: Option<String>,
    start_quest: Option<String>,
}

/// The tabs along the top (D-34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Content,
    Quests,
    Previews,
}

/// What the Quests tab draws: a questline's steps, or a quest's stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    Questline(String),
    Quest(String),
}

/// One of the selected entry's values, as its field shows it: written, or left out with its
/// default (U6b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInput {
    /// The group it's in, such as a faction's *Membership*.
    pub group: Option<String>,
    pub row: FormRow,
    /// What's typed in the field; empty for a value left out.
    pub input: String,
    /// Why the last value entered here was refused.
    pub refused: Option<String>,
}

/// A place in the selected entry, or where a file's entries go, with what the schema allows
/// to be added there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceInput {
    pub path: ValuePath,
    pub additions: Vec<Addition>,
    /// A new id, as typed, for a place that takes one.
    pub id: String,
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
            places: Vec::new(),
            referenced: None,
            file_places: Vec::new(),
            note: None,
            workspace: Workspace::Content,
            graph: QuestGraph::default(),
            shown: None,
            chosen: None,
            preview: None,
            map_faction: None,
            preview_quests: Quests::default(),
            curve: None,
            starter: None,
            start_quest: None,
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

    /// The selected entry's values, group by group: what's written, then the defaults of
    /// what's left out.
    pub fn fields(&self) -> &[FieldInput] {
        &self.fields
    }

    /// What names the selected entry, as written; `None` if it's in a file nothing names.
    pub fn referenced_by(&self) -> Option<&[Reference]> {
        self.referenced.as_deref()
    }

    /// The selected entry's places: the entry, and every table and list in it.
    pub fn places(&self) -> &[PlaceInput] {
        &self.places
    }

    /// Where the `file`th file's entries go: its top, and each list of tables there.
    pub fn file_places(&self, file: usize) -> &[PlaceInput] {
        self.file_places.get(file).map_or(&[], Vec::as_slice)
    }

    /// The tab showing.
    pub fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub fn show_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
    }

    pub fn graph(&self) -> &QuestGraph {
        &self.graph
    }

    /// The questline or quest drawn in the Quests tab.
    pub fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }

    /// Draws a questline or a quest; a quest is chosen too. Nothing changes if there's no
    /// such questline or quest.
    pub fn show(&mut self, shown: Shown) {
        if !self.is_there(&shown) {
            return;
        }
        if let Shown::Quest(quest) = &shown {
            self.chosen = Some(quest.clone());
        }
        self.shown = Some(shown);
    }

    /// Whether the questline or quest is in the graph.
    fn is_there(&self, shown: &Shown) -> bool {
        let quests = &self.graph.quests;
        match shown {
            Shown::Questline(id) => quests.questlines.keys().any(|line| line.as_str() == id),
            Shown::Quest(id) => quests.quests.keys().any(|quest| quest.as_str() == id),
        }
    }

    /// The quest whose stages and choices show beside the graph.
    pub fn chosen(&self) -> Option<&str> {
        self.chosen.as_deref()
    }

    /// Chooses a quest; nothing changes if there's no such quest.
    pub fn choose(&mut self, quest: &str) {
        if self.is_there(&Shown::Quest(quest.to_owned())) {
            self.chosen = Some(quest.to_owned());
        }
    }

    /// The world the previews come from: built from the content in memory when it last
    /// loaded (D-35); `None` if it never has.
    pub fn preview(&self) -> Option<&World> {
        self.preview.as_ref()
    }

    /// Whether the previews are from content that has changed since it last loaded, because
    /// it doesn't load now.
    pub fn preview_is_stale(&self) -> bool {
        self.preview.is_some() && !self.outline.loads()
    }

    /// The faction whose alignment map is shown: the one chosen, else the first.
    pub fn map_faction(&self) -> Option<&str> {
        let mut factions = self.preview.as_ref()?.factions();
        let chosen = self.map_faction.as_deref();
        match chosen {
            Some(chosen) => factions.find(|f| f.id.as_str() == chosen),
            None => factions.next(),
        }
        .map(|faction| faction.id.as_str())
    }

    /// Shows `faction`'s alignment map; nothing changes if the preview has no such faction.
    pub fn show_map(&mut self, faction: &str) {
        let there = self
            .preview
            .as_ref()
            .is_some_and(|world| world.factions().any(|f| f.id.as_str() == faction));
        if there {
            self.map_faction = Some(faction.to_owned());
        }
    }

    /// The knob whose curve is shown, the one chosen or else the first, with its curve
    /// (`None` if it's left out).
    pub fn curve(&self) -> Option<(String, Option<Curve>)> {
        let curves = self.preview.as_ref()?.named_curves();
        let chosen = self.curve.as_deref();
        let found = match chosen {
            Some(chosen) => curves.iter().find(|(name, _)| name == chosen),
            None => curves.first(),
        };
        found.cloned()
    }

    /// Shows a knob's curve; nothing changes if the preview has no such knob.
    pub fn show_curve(&mut self, knob: &str) {
        let there = self
            .preview
            .as_ref()
            .is_some_and(|world| world.named_curves().iter().any(|(name, _)| name == knob));
        if there {
            self.curve = Some(knob.to_owned());
        }
    }

    /// Asks whether `character` can start `quest`: either may be chosen first. Nothing
    /// changes for one the preview doesn't have.
    pub fn ask_start(&mut self, character: Option<&str>, quest: Option<&str>) {
        let Some(world) = &self.preview else {
            return;
        };
        if let Some(character) = character
            && world.characters().any(|c| c.id.as_str() == character)
        {
            self.starter = Some(character.to_owned());
        }
        if let Some(quest) = quest
            && self
                .preview_quests
                .quests
                .keys()
                .any(|q| q.as_str() == quest)
        {
            self.start_quest = Some(quest.to_owned());
        }
    }

    /// The character and quest asked about, as far as they're chosen.
    pub fn asked(&self) -> (Option<&str>, Option<&str>) {
        (self.starter.as_deref(), self.start_quest.as_deref())
    }

    /// Whether the character asked about can start the quest asked about, at the start of
    /// play on the previewed world, with every reason not.
    pub fn start_assessment(&self) -> Option<StartAssessment> {
        let world = self.preview.as_ref()?;
        let character = CharacterId::new(self.starter.as_deref()?).ok()?;
        let quest = QuestId::new(self.start_quest.as_deref()?).ok()?;
        let log = QuestLog::new(self.preview_quests.clone());
        log.assess_start(world, &character, &quest).ok()
    }

    /// Selects the chosen quest's entry in `quests.toml` and shows the Content tab.
    pub fn edit_in_content(&mut self) {
        let Some(quest) = &self.chosen else {
            return;
        };
        let place = self.outline.files.iter().enumerate().find_map(|(file, f)| {
            let entry = f.entries.iter().position(|entry| &entry.key == quest)?;
            (f.name == QUESTS_FILE).then_some((file, entry))
        });
        if let Some((file, entry)) = place {
            self.select(file, entry);
            self.workspace = Workspace::Content;
        }
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
        let (Some((file, _)), Some(field)) = (self.selected, self.fields.get(field)) else {
            return Ok(());
        };
        let path = field.row.path.clone();
        self.change(file, |text| set_value(text, &path, input))
    }

    /// Writes the `field`th value of the selected entry, one left out, at its default: the
    /// writer adds its key. Nothing changes for a value that's written.
    pub fn write_default(&mut self, field: usize) -> Result<(), EditError> {
        let Some((file, _)) = self.selected else {
            return Ok(());
        };
        let Some(row) = self.fields.get(field).map(|field| &field.row) else {
            return Ok(());
        };
        let mut place = row.path.clone();
        let (None, Some(Step::Key(key))) = (&row.written, place.0.pop()) else {
            return Ok(());
        };
        self.add(file, &place, Some(&key))
    }

    /// Adds `key` at `at` in the `file`th file, or with no key an item at the end of the list
    /// there, through the writer, and selects the entry it's in. A file that isn't there is
    /// started. Nothing changes if the writer refuses.
    pub fn add(&mut self, file: usize, at: &ValuePath, key: Option<&str>) -> Result<(), EditError> {
        let Some(name) = CONTENT_FILES.get(file) else {
            return Ok(());
        };
        let mut added = None;
        self.change(file, |text| {
            let (text, path) = factional_content::add(name, text, at, key)?;
            added = Some(path);
            Ok(text)
        })?;
        if let Some(entry) = added.and_then(|added| self.entry_containing(file, &added)) {
            self.select(file, entry);
        }
        Ok(())
    }

    /// The entry of the `file`th file that `path` is in.
    fn entry_containing(&self, file: usize, path: &ValuePath) -> Option<usize> {
        self.outline
            .files
            .get(file)?
            .entries
            .iter()
            .position(|entry| {
                ValuePath::parse(&entry.key).is_some_and(|key| path.0.starts_with(&key.0))
            })
    }

    /// Selects the entry of `file` that the key `key` is in, such as a reference's place;
    /// nothing if there's no such entry.
    pub fn select_at(&mut self, file: &str, key: &str) {
        let Some(path) = ValuePath::parse(key) else {
            return;
        };
        let place = self.outline.files.iter().position(|f| f.name == file);
        if let Some((file, entry)) =
            place.and_then(|place| Some((place, self.entry_containing(place, &path)?)))
        {
            self.select(file, entry);
        }
    }

    /// Removes what's at `path` in the selected entry's file, through the writer; removing
    /// the entry itself leaves nothing selected. Nothing changes if the writer refuses.
    pub fn remove(&mut self, path: &ValuePath) -> Result<(), EditError> {
        let Some((file, _)) = self.selected else {
            return Ok(());
        };
        let whole = self
            .selected_entry()
            .is_some_and(|(_, entry)| ValuePath::parse(&entry.key).as_ref() == Some(path));
        self.change(file, |text| factional_content::remove(text, path))?;
        if whole {
            self.selected = None;
            self.show_fields();
        }
        Ok(())
    }

    /// Changes the `file`th file's text in memory by `edit`, keeping what it was to undo, and
    /// makes the outline again. Nothing changes if `edit` refuses.
    fn change(
        &mut self,
        file: usize,
        edit: impl FnOnce(&str) -> Result<String, EditError>,
    ) -> Result<(), EditError> {
        let Some(texts) = self.texts.as_mut() else {
            return Ok(());
        };
        let Some(text) = texts.get(file) else {
            return Ok(());
        };
        let changed = edit(text.as_deref().unwrap_or_default())?;
        self.undo.push(texts.clone());
        texts[file] = Some(changed);
        self.note = None;
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
        if self.outline.loads()
            && let Some(texts) = &self.texts
            && let Ok((content, quests)) = load_texts(texts)
            && let Ok(world) = World::new(content)
        {
            self.preview = Some(world);
            self.preview_quests = quests;
        }
        self.graph = match &self.texts {
            Some(texts) => QuestGraph::new(texts, &self.outline),
            None => QuestGraph::default(),
        };
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| !self.is_there(shown))
        {
            self.shown = None;
        }
        if let Some(quest) = self.chosen.take() {
            self.choose(&quest);
        }
        self.file_places = match &self.texts {
            Some(texts) => self
                .outline
                .files
                .iter()
                .zip(texts)
                .map(|(file, text)| {
                    let text = text.as_deref().unwrap_or_default();
                    places_in(file.name, text, file_places(text))
                })
                .collect(),
            None => Vec::new(),
        };
        self.show_fields();
    }

    /// The selected entry's form, its places, and what names it.
    fn show_fields(&mut self) {
        let (Some((file, entry)), Some(texts)) = (self.selected, &self.texts) else {
            self.fields.clear();
            self.places.clear();
            self.referenced = None;
            return;
        };
        let text = texts[file].as_deref().unwrap_or_default();
        let name = self.outline.files[file].name;
        let key = &self.outline.files[file].entries[entry].key;
        self.fields = entry_form(texts, name, key)
            .into_iter()
            .flat_map(|group| {
                group.rows.into_iter().map(move |row| FieldInput {
                    group: group.name.clone(),
                    input: row
                        .written
                        .as_ref()
                        .map(|field| field.value.clone())
                        .unwrap_or_default(),
                    row,
                    refused: None,
                })
            })
            .collect();
        self.places = places_in(name, text, entry_places(text, key));
        self.referenced = NAMED_FILES
            .contains(&name)
            .then(|| referenced_by(texts, name, key));
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

    /// Draws the editor into `ui`: buttons, the summary and the tabs at the top (D-34), then
    /// the tab showing.
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
                ui.separator();
                for (workspace, name) in [
                    (Workspace::Content, "Content"),
                    (Workspace::Quests, "Quests"),
                    (Workspace::Previews, "Previews"),
                ] {
                    if ui
                        .selectable_label(self.workspace == workspace, name)
                        .clicked()
                    {
                        self.workspace = workspace;
                    }
                }
            });
        });
        match self.workspace {
            Workspace::Content => self.content_ui(ui),
            Workspace::Quests => self.quests_ui(ui),
            Workspace::Previews => self.previews_ui(ui),
        }
    }

    /// The Content tab: the files and their entries on the left, each with what can be added,
    /// and the selected entry in the middle: its values, then its places, each with what can
    /// be added or removed.
    fn content_ui(&mut self, ui: &mut egui::Ui) {
        let mut clicked = None;
        let mut adding = None;
        egui::Panel::left("files")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (place, file) in self.outline.files.iter().enumerate() {
                        let mut new_entries = |ui: &mut egui::Ui| {
                            let Some(places) = self.file_places.get_mut(place) else {
                                return;
                            };
                            for at in places {
                                let name = match at.path.0.is_empty() {
                                    true => file.name.to_owned(),
                                    false => at.path.to_string(),
                                };
                                let mut added = None;
                                ui.horizontal_wrapped(|ui| adding_ui(ui, at, &name, &mut added));
                                if let Some((at, key)) = added {
                                    adding = Some((place, at, key));
                                }
                            }
                        };
                        if file.state != FileState::Read {
                            ui.label(file.label());
                            diagnostics(ui, &file.problems, &file.warnings, |d| d.key.as_deref());
                            new_entries(ui);
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
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(chosen, entry.label()).clicked() {
                                            clicked = Some((place, index));
                                        }
                                        if let Some(name) = &entry.name {
                                            ui.weak(name);
                                        }
                                    });
                                }
                                new_entries(ui);
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
        let mut going = None;
        if self.selected.is_some() {
            egui::Panel::right("entry")
                .resizable(true)
                .default_size(320.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("entry")
                        .show(ui, |ui| going = self.inspector_ui(ui));
                });
        }
        let mut acted = None;
        let mut removing = None;
        egui::CentralPanel::default().show(ui, |ui| {
            let (Some((place, _)), Some((file, entry))) = (self.selected, self.selected_entry())
            else {
                ui.label(
                    "Choose an entry on the left to see it and what the loader says about it.",
                );
                return;
            };
            ui.label(
                egui::RichText::new(format!("{file} › {}", entry.key))
                    .monospace()
                    .weak(),
            );
            ui.heading(entry.name.as_deref().unwrap_or(&entry.key));
            ui.separator();
            let key = entry.key.clone();
            let prefix = format!("{key}.");
            egui::ScrollArea::vertical().show(ui, |ui| {
                acted = form_ui::form_ui(ui, &mut self.fields);
                ui.separator();
                for at in &mut self.places {
                    let path = at.path.to_string();
                    let name = match path == key {
                        true => key.clone(),
                        false => path.strip_prefix(&prefix).unwrap_or(&path).to_owned(),
                    };
                    let mut added = None;
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(format!("Remove {name}")).clicked() {
                            removing = Some(at.path.clone());
                        }
                        adding_ui(ui, at, &name, &mut added);
                    });
                    if let Some((at, key)) = added {
                        adding = Some((place, at, key));
                    }
                }
            });
        });
        if let Some((file, key)) = going {
            self.select_at(file, &key);
        }
        match acted {
            Some(Acted::Enter(place, input)) => {
                if let Err(refused) = self.set(place, &input) {
                    self.fields[place].refused = Some(refused.to_string());
                }
            }
            Some(Acted::Default(place)) => {
                if let Err(refused) = self.write_default(place) {
                    self.note = Some(format!("couldn't set: {refused}"));
                }
            }
            Some(Acted::Remove(path)) => removing = Some(path),
            None => {}
        }
        if let Some((file, at, key)) = adding
            && let Err(refused) = self.add(file, &at, key.as_deref())
        {
            self.note = Some(format!("couldn't add: {refused}"));
        }
        if let Some(path) = removing
            && let Err(refused) = self.remove(&path)
        {
            self.note = Some(format!("couldn't remove: {refused}"));
        }
    }
}

/// Each of `paths` in `file`'s `text`, with what can be added there.
fn places_in(file: &str, text: &str, paths: Vec<ValuePath>) -> Vec<PlaceInput> {
    paths
        .into_iter()
        .map(|path| PlaceInput {
            additions: additions(file, text, &path),
            path,
            id: String::new(),
        })
        .collect()
}

/// A place's additions as buttons, named after the place as `name`, with a field for a new
/// id where it takes one; what's chosen goes in `adding`.
fn adding_ui(
    ui: &mut egui::Ui,
    place: &mut PlaceInput,
    name: &str,
    adding: &mut Option<(ValuePath, Option<String>)>,
) {
    let PlaceInput {
        path,
        additions,
        id,
    } = place;
    for addition in additions.iter() {
        match addition {
            Addition::Key(key) => {
                if ui.button(format!("Add {key} to {name}")).clicked() {
                    *adding = Some((path.clone(), Some(key.clone())));
                }
            }
            Addition::Item => {
                if ui.button(format!("Add to {name}")).clicked() {
                    *adding = Some((path.clone(), None));
                }
            }
            Addition::Id => {
                let label = ui.label(format!("New id in {name}"));
                ui.add(egui::TextEdit::singleline(id).desired_width(120.0))
                    .labelled_by(label.id);
                if ui.button(format!("Add to {name}")).clicked() {
                    *adding = Some((path.clone(), Some(id.clone())));
                }
            }
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

    fn sample() -> Editor {
        Editor::open(Path::new(REPO).join("content/sample"))
    }

    /// The sample with the City Watch's tolerance above its member tolerance, so it doesn't
    /// load.
    fn sample_broken() -> Editor {
        let mut editor = sample();
        let factions = CONTENT_FILES
            .iter()
            .position(|name| *name == "factions.toml")
            .expect("a content file");
        editor.select(factions, 1);
        let tolerance = editor
            .fields()
            .iter()
            .position(|f| f.row.path.to_string() == "city_watch.tolerance")
            .expect("the Watch's tolerance");
        editor.set(tolerance, "140.0").expect("a number");
        editor
    }

    #[test]
    fn previews_come_from_the_content_as_it_stands_while_it_loads() {
        let editor = sample();
        let world = editor.preview().expect("the sample loads");
        assert_eq!(world.characters().count(), 6);
        assert!(!editor.preview_is_stale());
        assert!(broken().preview().is_none());
        assert!(!broken().preview_is_stale());
    }

    #[test]
    fn previews_keep_the_last_world_that_loaded_until_it_loads_again() {
        let mut editor = sample_broken();
        assert!(!editor.outline().loads());
        assert!(editor.preview().is_some());
        assert!(editor.preview_is_stale());
        editor.undo();
        assert!(!editor.preview_is_stale());
        // An edit that loads is previewed at once: the Watch's tolerance as now set.
        let tolerance = editor
            .fields()
            .iter()
            .position(|f| f.row.path.to_string() == "city_watch.tolerance")
            .expect("the Watch's tolerance");
        editor.set(tolerance, "45.0").expect("a number");
        let watch = factional_reputation::FactionId::new("city_watch").expect("valid id");
        let world = editor.preview().expect("it loads");
        let map = world.alignment_map(&watch).expect("the Watch");
        assert_eq!(map.tolerance.to_string(), "45.00");
    }

    #[test]
    fn the_map_shows_the_faction_chosen_or_else_the_first() {
        let mut editor = sample();
        assert_eq!(editor.map_faction(), Some("ashen_circle"));
        editor.show_map("nowhere");
        assert_eq!(editor.map_faction(), Some("ashen_circle"));
        editor.show_map("city_watch");
        assert_eq!(editor.map_faction(), Some("city_watch"));
        assert_eq!(broken().map_faction(), None);
    }

    #[test]
    fn the_curve_shown_is_the_knob_chosen_or_else_the_first() {
        let mut editor = sample();
        let (name, curve) = editor.curve().expect("the sample has curves");
        assert_eq!(name, "disposition.affinity");
        assert!(curve.is_some());
        editor.show_curve("nowhere");
        assert_eq!(
            editor.curve().map(|(name, _)| name).as_deref(),
            Some("disposition.affinity")
        );
        editor.show_curve("inertia.steady.law.toward_lawful");
        let (name, curve) = editor.curve().expect("chosen");
        assert_eq!(name, "inertia.steady.law.toward_lawful");
        assert!(curve.is_none());
        assert!(broken().curve().is_none());
    }

    #[test]
    fn whether_a_character_can_start_a_quest_is_asked_once_both_are_chosen() {
        let mut editor = sample();
        assert!(editor.start_assessment().is_none());
        editor.ask_start(Some("player"), None);
        assert_eq!(editor.asked(), (Some("player"), None));
        assert!(editor.start_assessment().is_none());
        editor.ask_start(None, Some("watch_captain"));
        assert_eq!(editor.asked(), (Some("player"), Some("watch_captain")));
        let assessment = editor.start_assessment().expect("both chosen");
        assert!(!assessment.allowed());
        assert_eq!(assessment.blocks.len(), 3);
        editor.ask_start(Some("nobody"), Some("nowhere"));
        assert_eq!(editor.asked(), (Some("player"), Some("watch_captain")));
        editor.ask_start(None, Some("watch_oath"));
        assert!(editor.start_assessment().expect("both chosen").allowed());
        let mut broken = broken();
        broken.ask_start(Some("hale"), None);
        assert_eq!(broken.asked(), (None, None));
    }

    #[test]
    fn a_questline_or_quest_is_shown_only_if_its_there() {
        let mut editor = sample();
        assert_eq!(editor.workspace(), Workspace::Content);
        let line = Shown::Questline("watch_career".to_owned());
        editor.show(Shown::Questline("nowhere".to_owned()));
        assert_eq!(editor.shown(), None);
        editor.show(line.clone());
        assert_eq!(editor.shown(), Some(&line));
        assert_eq!(editor.chosen(), None);
        editor.show(Shown::Quest("watch_oath".to_owned()));
        assert_eq!(editor.chosen(), Some("watch_oath"));
        editor.show(Shown::Quest("nowhere".to_owned()));
        assert_eq!(editor.shown(), Some(&Shown::Quest("watch_oath".to_owned())));
        editor.choose("nowhere");
        assert_eq!(editor.chosen(), Some("watch_oath"));
        editor.choose("circle_rite");
        assert_eq!(editor.chosen(), Some("circle_rite"));
        editor.show_workspace(Workspace::Quests);
        assert_eq!(editor.workspace(), Workspace::Quests);
    }

    #[test]
    fn edit_in_content_selects_the_chosen_quests_entry() {
        let mut editor = sample();
        editor.show_workspace(Workspace::Quests);
        editor.edit_in_content();
        assert_eq!(editor.workspace(), Workspace::Quests);
        assert!(editor.selected_entry().is_none());
        editor.choose("watch_oath");
        editor.edit_in_content();
        assert_eq!(editor.workspace(), Workspace::Content);
        let (file, entry) = editor.selected_entry().expect("selected");
        assert_eq!((file, entry.key.as_str()), ("quests.toml", "watch_oath"));
    }

    #[test]
    fn the_graph_follows_the_text_as_edited() {
        let mut editor = sample();
        editor.show(Shown::Quest("circle_rite".to_owned()));
        let quests = CONTENT_FILES
            .iter()
            .position(|name| *name == "quests.toml")
            .expect("a content file");
        editor.select(quests, 0);
        let circle_rite = ValuePath::parse("circle_rite").expect("a path");
        editor.remove(&circle_rite).expect("removed");
        assert!(
            !editor
                .graph()
                .quests
                .quests
                .keys()
                .any(|q| q.as_str() == "circle_rite")
        );
        assert_eq!(editor.shown(), None);
        assert_eq!(editor.chosen(), None);
        editor.undo();
        assert!(
            editor
                .graph()
                .quests
                .quests
                .keys()
                .any(|q| q.as_str() == "circle_rite")
        );
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

    fn path(text: &str) -> ValuePath {
        ValuePath::parse(text).expect("a path")
    }

    fn keys(names: &[&str]) -> Vec<Addition> {
        names
            .iter()
            .map(|name| Addition::Key((*name).to_owned()))
            .collect()
    }

    fn inputs(editor: &Editor) -> Vec<&str> {
        editor.fields().iter().map(|f| f.input.as_str()).collect()
    }

    #[test]
    fn adding_an_entry_selects_it_with_its_starting_values() {
        let dir = with_characters("add_entry", VEX);
        let mut editor = Editor::open(&dir);
        let places: Vec<(&ValuePath, &[Addition])> = editor
            .file_places(2)
            .iter()
            .map(|place| (&place.path, &place.additions[..]))
            .collect();
        assert_eq!(places, [(&ValuePath::ROOT, &[Addition::Id][..])]);
        assert_eq!(editor.add(2, &ValuePath::ROOT, Some("ava")), Ok(()));
        let (file, entry) = editor.selected_entry().expect("selected");
        assert_eq!((file, entry.key.as_str()), ("characters.toml", "ava"));
        assert_eq!(inputs(&editor), ["", "0.0", "0.0"]);
        assert!(editor.has_changes() && editor.can_undo());
        editor.undo();
        assert!(editor.selected_entry().is_none());
        assert!(!editor.has_changes());
    }

    #[test]
    fn adding_to_a_file_not_there_starts_it() {
        let dir = with_characters("add_file", VEX);
        let mut editor = Editor::open(&dir);
        let relations = editor.file_places(4);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].additions, keys(&["relation"]));
        editor
            .add(4, &ValuePath::ROOT, Some("relation"))
            .expect("added");
        let (file, entry) = editor.selected_entry().expect("selected");
        assert_eq!(
            (file, entry.key.as_str()),
            ("relations.toml", "relation[0]")
        );
        let lists: Vec<&ValuePath> = editor.file_places(4).iter().map(|p| &p.path).collect();
        assert_eq!(lists, [&ValuePath::ROOT, &path("relation")]);
        editor.save().expect("saved");
        assert_eq!(
            fs::read_to_string(dir.join("relations.toml")).expect("written"),
            "[[relation]]\nvalue = 0.0\nbetween = [\"\", \"\"]\n"
        );
    }

    #[test]
    fn the_selected_entrys_places_offer_what_the_schema_allows() {
        let dir = with_characters("add_key", VEX);
        let mut editor = Editor::open(&dir);
        assert!(editor.places().is_empty());
        editor.select(2, 0);
        let places: Vec<(String, Vec<Addition>)> = editor
            .places()
            .iter()
            .map(|place| (place.path.to_string(), place.additions.clone()))
            .collect();
        assert_eq!(
            places,
            [
                (
                    "vex".to_owned(),
                    keys(&["contacts", "inertia", "memberships", "standing", "weights"])
                ),
                ("vex.alignment".to_owned(), Vec::new()),
            ]
        );
        editor
            .add(2, &path("vex"), Some("standing"))
            .expect("added");
        let (_, entry) = editor.selected_entry().expect("still selected");
        assert_eq!(entry.key, "vex");
        let standing = editor.places().last().expect("there");
        assert_eq!(standing.path, path("vex.standing"));
        assert_eq!(standing.additions, keys(&["characters", "factions"]));
    }

    #[test]
    fn removing_goes_through_the_writer_and_can_be_undone() {
        let dir = with_characters("remove", VEX);
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.remove(&path("vex")), Ok(()), "nothing selected");
        assert!(!editor.has_changes());
        editor.select(2, 0);
        editor.remove(&path("vex.alignment.good")).expect("removed");
        assert_eq!(inputs(&editor), ["Vex", "120.0"]);
        editor.undo();
        assert_eq!(inputs(&editor), ["Vex", "120.0", "-20.0"]);
        editor.remove(&path("vex")).expect("removed");
        assert!(editor.selected_entry().is_none());
        assert!(editor.fields().is_empty() && editor.places().is_empty());
        assert!(editor.outline().files[2].entries.is_empty());
        editor.undo();
        assert_eq!(editor.outline().files[2].entries.len(), 1);
    }

    #[test]
    fn a_refused_change_changes_nothing() {
        let dir = with_characters("add_refused", VEX);
        let mut editor = Editor::open(&dir);
        assert_eq!(
            editor.add(2, &ValuePath::ROOT, Some("vex")),
            Err(EditError::Taken(path("vex")))
        );
        assert_eq!(
            editor.add(9, &ValuePath::ROOT, Some("ava")),
            Ok(()),
            "no such file"
        );
        editor.select(2, 0);
        assert_eq!(
            editor.remove(&path("vex.speed")),
            Err(EditError::NotThere(path("vex.speed")))
        );
        assert!(!editor.has_changes() && !editor.can_undo());
    }

    #[test]
    fn a_file_that_cant_be_read_offers_nothing_to_add() {
        let dir = with_characters("add_unreadable", "[vex\n");
        let editor = Editor::open(&dir);
        assert!(editor.file_places(2).is_empty());
        assert_eq!(
            editor.file_places(1).len(),
            1,
            "factions.toml can be started"
        );
        let nowhere = Editor::open(std::env::temp_dir().join("factional_editor_nowhere"));
        assert!(nowhere.file_places(2).is_empty());
    }

    /// The sample with `key` of `file` selected.
    fn sample_on(file: usize, key: &str) -> Editor {
        let mut editor = sample();
        let entry = editor.outline.files[file]
            .entries
            .iter()
            .position(|entry| entry.key == key)
            .expect("the entry");
        editor.select(file, entry);
        editor
    }

    fn field(editor: &Editor, key: &str) -> usize {
        editor
            .fields()
            .iter()
            .position(|field| field.row.key == key)
            .expect("the field")
    }

    #[test]
    fn the_window_opens_at_the_canvas_size() {
        assert_eq!(options().viewport.inner_size, Some(WINDOW));
    }

    #[test]
    fn a_default_is_written_in_only_where_a_value_is_left_out() {
        let mut editor = sample_on(1, "city_watch");
        let expel = field(&editor, "expel_standing_change");
        assert_eq!(editor.fields()[expel].group.as_deref(), Some("Membership"));
        assert_eq!(editor.fields()[expel].input, "");
        assert_eq!(editor.write_default(expel), Ok(()));
        let expel = field(&editor, "expel_standing_change");
        assert_eq!(editor.fields()[expel].input, "-20.0");
        assert!(editor.fields()[expel].row.written.is_some());
        // A value written, or a field that isn't there, is left as it is.
        let before = editor.texts.clone();
        assert_eq!(editor.write_default(expel), Ok(()));
        assert_eq!(editor.write_default(999), Ok(()));
        assert_eq!(editor.texts, before);
        let mut nothing = sample();
        assert_eq!(nothing.write_default(0), Ok(()));
        assert!(!nothing.has_changes());
    }

    #[test]
    fn a_key_selects_the_entry_it_is_in() {
        let mut editor = sample_on(1, "city_watch");
        editor.select_at("characters.toml", "captain_hale.memberships[0].faction");
        assert_eq!(
            editor
                .selected_entry()
                .map(|(file, entry)| (file, entry.key.as_str())),
            Some(("characters.toml", "captain_hale"))
        );
        editor.select_at("relations.toml", "relation[2].between[0]");
        assert_eq!(
            editor.selected_entry().map(|(_, entry)| entry.key.as_str()),
            Some("relation[2]")
        );
        // Nothing changes for a file, an entry or a key that isn't there.
        for (file, key) in [
            ("nowhere.toml", "city_watch"),
            ("factions.toml", "nobody.name"),
            ("factions.toml", "["),
        ] {
            editor.select_at(file, key);
            assert_eq!(
                editor.selected_entry().map(|(_, entry)| entry.key.as_str()),
                Some("relation[2]")
            );
        }
    }

    #[test]
    fn what_names_an_entry_is_listed_only_where_it_can_be_named() {
        let watch = sample_on(1, "city_watch");
        let named = watch.referenced_by().expect("factions can be named");
        assert!(
            named
                .iter()
                .any(|r| r.key == "captain_hale.memberships[0].faction")
        );
        assert_eq!(
            sample_on(2, "player").referenced_by().map(<[_]>::len),
            Some(0)
        );
        assert_eq!(sample_on(4, "relation[0]").referenced_by(), None);
        assert_eq!(sample().referenced_by(), None);
    }
}

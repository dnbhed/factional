//! The wizard (W8a, P-153, DESIGN.md §21): a map made from an Azgaar export without leaving
//! the builder. An export opened and summarised as `world source` says it; a scale and seed;
//! the map generated in the background and drawn in place of the one shown; and saved to
//! `world/<name>.map`, said as `world import` says it, and set as `builder.toml`'s map.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::ecs::message::{Message, MessageReader};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy_egui::egui;
use factional_session::{
    Choices, Export, export_summary, generate, map_file_named, read_export, save_map,
};
use factional_worldgen::WorldMap;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{BuilderFile, ContentDir, Ground, MapChoice};

/// The largest scale the wizard offers, in tiles a source pixel each way.
pub const MOST_SCALE: u32 = 64;

/// What's asked of the wizard: an export opened, the map generated, saved, and a file there
/// already overwritten or kept. `Browse` is the app's: it shows the system's file dialog and
/// asks for what's chosen to be opened.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum Wizarding {
    Browse,
    Open(PathBuf),
    Generate,
    Save,
    Overwrite,
    KeepIt,
}

/// A map being made off the main thread, or why it can't be.
type Making = Task<Result<WorldMap, String>>;

/// The wizard: whether it's open; the export opened and what's said of it, or why it didn't
/// open; the scale and seed; the map being generated, the export the map shown was generated
/// from until it's saved, the file waiting to be overwritten, and what was last said.
#[derive(Resource)]
pub struct Wizard {
    pub open: bool,
    pub scale: u32,
    pub seed: u32,
    export: Option<Arc<Export>>,
    summary: Option<Result<String, String>>,
    generating: Option<(Making, Arc<Export>)>,
    made: Option<Arc<Export>>,
    confirming: Option<String>,
    note: Option<String>,
}

impl Default for Wizard {
    /// Closed, with nothing open, at `world import`'s scale and seed.
    fn default() -> Self {
        let choices = Choices::default();
        Wizard {
            open: false,
            scale: choices.scale,
            seed: choices.seed,
            export: None,
            summary: None,
            generating: None,
            made: None,
            confirming: None,
            note: None,
        }
    }
}

impl Wizard {
    /// The export opened, if one is.
    pub fn export(&self) -> Option<&Export> {
        self.export.as_deref()
    }

    /// What `world source` says of the export opened, or why the last one didn't open.
    pub fn summary(&self) -> Option<&Result<String, String>> {
        self.summary.as_ref()
    }

    /// Whether a map is being generated.
    pub fn generating(&self) -> bool {
        self.generating.is_some()
    }

    /// The file a map generated and not yet saved would be saved in, from the content
    /// directory.
    pub fn saves_to(&self) -> Option<String> {
        self.made
            .as_ref()
            .map(|export| map_file_named(&export.source.name))
    }

    /// The file waiting to be overwritten, there already.
    pub fn confirming(&self) -> Option<&str> {
        self.confirming.as_deref()
    }

    /// What was last said: why a map couldn't be made or saved, or what `world import` says of
    /// the one saved.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

/// `builder.toml`'s text, `file` if there is one, with `[map]`'s `file` set to `map`, adding
/// `[map]` if there's none, and everything else kept. A file that can't be read is refused.
pub fn write_map_file(file: Option<&str>, map: &str) -> Result<String, String> {
    let refused = |why: &str| {
        let why = why.trim().trim_end_matches('.');
        format!("builder.toml can't be read, so the map isn't set: {why}")
    };
    let text = file.unwrap_or_default();
    toml::from_str::<BuilderFile>(text).map_err(|error| refused(error.message()))?;
    let mut document: DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| refused(error.message()))?;
    if !document.contains_key("map") {
        let mut table = Table::new();
        if !document.is_empty() {
            table.decor_mut().set_prefix("\n");
        }
        document.insert("map", Item::Table(table));
    }
    let table = document["map"]
        .as_table_like_mut()
        .ok_or_else(|| refused("map isn't a table"))?;
    match table.get_mut("file") {
        Some(there) => {
            // The value changed, its comments kept.
            let decor = there.as_value().map(|old| old.decor().clone());
            *there = value(map);
            if let (Some(decor), Some(new)) = (decor, there.as_value_mut()) {
                *new.decor_mut() = decor;
            }
        }
        None => {
            table.insert("file", value(map));
        }
    }
    Ok(document.to_string())
}

/// What's asked of the wizard, done; and the map being generated, drawn once it's made.
pub(crate) fn wizard(
    mut commands: Commands,
    mut asked: MessageReader<Wizarding>,
    mut wizard: ResMut<Wizard>,
    ground: Option<Res<Ground>>,
    mut choice: ResMut<MapChoice>,
    dir: Res<ContentDir>,
) {
    for asked in asked.read() {
        match asked {
            Wizarding::Browse => {}
            Wizarding::Open(path) => match read_export(Path::new(""), &path.to_string_lossy()) {
                Ok(export) => {
                    wizard.summary = Some(Ok(export_summary(&export)));
                    wizard.export = Some(Arc::new(export));
                }
                Err(why) => {
                    wizard.summary = Some(Err(why));
                    wizard.export = None;
                }
            },
            Wizarding::Generate => {
                let Some(export) = wizard.export.clone() else {
                    continue;
                };
                if wizard.generating() {
                    continue;
                }
                let choices = Choices {
                    scale: wizard.scale,
                    seed: wizard.seed,
                    ..Choices::default()
                };
                let from = export.clone();
                let task =
                    AsyncComputeTaskPool::get().spawn(async move { generate(&from, &choices) });
                // The export kept beside it, to save with once it's made.
                wizard.generating = Some((task, export));
                wizard.made = None;
                wizard.confirming = None;
                wizard.note = None;
            }
            Wizarding::Save => {
                let (Some(made), Some(ground)) = (wizard.made.clone(), ground.as_deref()) else {
                    continue;
                };
                let file = map_file_named(&made.source.name);
                if dir.0.join(&file).exists() {
                    wizard.confirming = Some(file);
                    continue;
                }
                save(&mut wizard, &mut choice, &dir.0, &file, &made, ground);
            }
            Wizarding::Overwrite => {
                let (Some(file), Some(made), Some(ground)) = (
                    wizard.confirming.take(),
                    wizard.made.clone(),
                    ground.as_deref(),
                ) else {
                    continue;
                };
                save(&mut wizard, &mut choice, &dir.0, &file, &made, ground);
            }
            Wizarding::KeepIt => wizard.confirming = None,
        }
    }
    let Some(done) = wizard
        .generating
        .as_mut()
        .and_then(|(task, _)| check_ready(task))
    else {
        return;
    };
    let made = wizard.generating.take().map(|(_, export)| export);
    match done {
        Ok(map) => {
            commands.insert_resource(Ground {
                map,
                tile: choice.tile,
            });
            wizard.made = made;
        }
        Err(why) => wizard.note = Some(why),
    }
}

/// The map shown, made from `made`, saved to `file` in `dir`, its `world/` made if need be;
/// said as `world import` says it, and set as `builder.toml`'s map.
fn save(
    wizard: &mut Wizard,
    choice: &mut MapChoice,
    dir: &Path,
    file: &str,
    made: &Export,
    ground: &Ground,
) {
    if let Some(parent) = Path::new(file).parent() {
        let _ = std::fs::create_dir_all(dir.join(parent));
    }
    let said = match save_map(dir, file, &ground.map, &made.source.name) {
        Ok(said) => said,
        Err(why) => {
            wizard.note = Some(why);
            return;
        }
    };
    wizard.made = None;
    let toml = dir.join("builder.toml");
    let text = std::fs::read_to_string(&toml).ok();
    let set = write_map_file(text.as_deref(), file).and_then(|written| {
        std::fs::write(&toml, written)
            .map_err(|error| format!("couldn't save builder.toml: {error}"))
    });
    match set {
        Ok(()) => {
            choice.file = Some(file.to_owned());
            wizard.note = Some(said);
        }
        Err(why) => wizard.note = Some(format!("{said}\n{why}")),
    }
}

/// The wizard's window's contents: Open…, the summary, the scale and seed, Generate, Save and
/// what's said. What's asked, if anything.
pub fn wizard_ui(ui: &mut egui::Ui, wizard: &mut Wizard) -> Option<Wizarding> {
    let mut asked = None;
    if ui.button("Open…").clicked() {
        asked = Some(Wizarding::Browse);
    }
    match wizard.summary() {
        Some(Ok(said)) => {
            ui.label(said.as_str());
        }
        Some(Err(why)) => {
            ui.colored_label(ui.visuals().error_fg_color, why.as_str());
        }
        None => {}
    }
    if wizard.export().is_some() {
        ui.horizontal(|ui| {
            ui.label("Scale");
            ui.add(egui::DragValue::new(&mut wizard.scale).range(1..=MOST_SCALE))
                .on_hover_text("How many tiles a source pixel makes each way");
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut wizard.seed))
                .on_hover_text("The seed for generation");
        });
        ui.horizontal(|ui| {
            let generating = wizard.generating();
            if ui
                .add_enabled(!generating, egui::Button::new("Generate"))
                .clicked()
            {
                asked = Some(Wizarding::Generate);
            }
            if generating {
                ui.spinner();
                ui.label("Generating…");
            }
        });
    }
    if let Some(file) = wizard.confirming() {
        ui.label(format!("{file} is there already."));
        ui.horizontal(|ui| {
            if ui.button("Overwrite").clicked() {
                asked = Some(Wizarding::Overwrite);
            }
            if ui.button("Keep it").clicked() {
                asked = Some(Wizarding::KeepIt);
            }
        });
    } else if let Some(file) = wizard.saves_to()
        && ui.button(format!("Save as {file}")).clicked()
    {
        asked = Some(Wizarding::Save);
    }
    if let Some(note) = wizard.note() {
        ui.label(note);
    }
    asked
}

#[cfg(test)]
mod tests {
    use bevy::tasks::TaskPool;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/tests/fixtures/azgaar"
    );

    /// The tests' task pool runs a task at once, so a map is never seen being generated from
    /// outside: here one is held that never finishes.
    #[test]
    fn while_a_map_is_being_generated_generate_is_off_and_it_says_so() {
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let export = Arc::new(read_export(Path::new(FIXTURES), "raster.json").expect("reads"));
        let task = pool.spawn(std::future::pending::<Result<WorldMap, String>>());
        let wizard = Wizard {
            export: Some(export.clone()),
            summary: Some(Ok(export_summary(&export))),
            generating: Some((task, export)),
            ..Wizard::default()
        };
        assert!(wizard.generating());
        assert!(wizard.saves_to().is_none());
        let mut harness = Harness::new_ui_state(
            |ui, wizard: &mut Wizard| {
                wizard_ui(ui, wizard);
            },
            wizard,
        );
        // The spinner asks for frames for ever, so a few are run.
        harness.run_steps(4);
        assert!(
            harness
                .get_by_label("Generate")
                .accesskit_node()
                .is_disabled()
        );
        assert!(harness.query_by_label("Generating…").is_some());
    }
}

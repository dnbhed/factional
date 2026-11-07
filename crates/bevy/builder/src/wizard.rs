//! The wizard (W8a, P-153, DESIGN.md §21): a map made from an Azgaar export without leaving
//! the builder. An export opened and summarised as `world source` says it; a scale and seed;
//! the map generated in the background and drawn in place of the one shown; and saved to
//! `world/<name>.map`, said as `world import` says it, and set as `builder.toml`'s map. And a
//! region of it (W8b, P-155): the export previewed by its cells' heights, in the background,
//! and a region dragged out on it or set in numbers, with the size of the map it makes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::ecs::message::{Message, MessageReader};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy_egui::egui;
use factional_session::{
    Choices, Export, export_summary, generate, map_file_named, map_size, read_export, save_map,
};
use factional_worldgen::{Preview, Region, Shade, Size, Terrain, WorldMap, preview};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{BuilderFile, ContentDir, Ground, MapChoice, colour};

/// The largest scale the wizard offers, in tiles a source pixel each way.
pub const MOST_SCALE: u32 = 64;

/// The most pixels the preview has on its longer side.
pub const PREVIEW_SIDE: u32 = 320;

/// How long the preview is shown on its longer side, in points.
const PREVIEW_SHOWN: f32 = 320.0;

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

/// A preview being made off the main thread, or why there can't be one.
type Previewing = Task<Result<Preview, String>>;

/// The wizard: whether it's open; the export opened and what's said of it, or why it didn't
/// open; its preview, or the preview being made, and its picture once shown; the region chosen,
/// the whole source if none, and where a drag on the preview began; the scale and seed; the map
/// being generated, the export the map shown was generated from until it's saved, the file
/// waiting to be overwritten, and what was last said.
#[derive(Resource)]
pub struct Wizard {
    pub open: bool,
    pub region: Option<Region>,
    pub scale: u32,
    pub seed: u32,
    export: Option<Arc<Export>>,
    summary: Option<Result<String, String>>,
    preview: Option<Preview>,
    previewing: Option<Previewing>,
    picture: Option<egui::TextureHandle>,
    dragged_from: Option<[f32; 2]>,
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
            region: choices.region,
            scale: choices.scale,
            seed: choices.seed,
            export: None,
            summary: None,
            preview: None,
            previewing: None,
            picture: None,
            dragged_from: None,
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

    /// The export's preview, once it's made.
    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    /// Whether the export's preview is being made.
    pub fn previewing(&self) -> bool {
        self.previewing.is_some()
    }

    /// The size of the map the choices would make of the export opened, or why they can't
    /// make one, in `world import`'s words; nothing if no export is open.
    pub fn makes(&self) -> Option<Result<Size, String>> {
        let export = self.export.as_ref()?;
        Some(map_size(export, &self.choices()))
    }

    /// The choices made: the region, the scale and the seed, and `world import`'s defaults.
    fn choices(&self) -> Choices {
        Choices {
            region: self.region,
            scale: self.scale,
            seed: self.seed,
            ..Choices::default()
        }
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

/// The region dragged out from `a` to `b`, in source pixels, on a source of `size`: each corner
/// taken outward to a whole pixel, and held to the source, at least a pixel each way.
pub fn region_between(a: [f32; 2], b: [f32; 2], size: Size) -> Region {
    // From the lower edge, down to a pixel, to the higher, up to one, each way.
    let axis = |a: f32, b: f32, side: u32| -> (u32, u32) {
        let side = side.max(1) as f32;
        let from = a.min(b).floor().clamp(0.0, side - 1.0);
        let to = a.max(b).ceil().clamp(from + 1.0, side);
        (from as u32, (to - from) as u32)
    };
    let (x, width) = axis(a[0], b[0], size.width);
    let (y, height) = axis(a[1], b[1], size.height);
    Region {
        x,
        y,
        size: Size { width, height },
    }
}

/// A preview pixel's colour, in sRGB, in the map's own colours: water from deep at 0 to shallow
/// at 19 and above, and land from grass at 20 to rock at 60 and snow at 100, each rounded.
pub fn shade_colour(shade: Shade) -> [u8; 3] {
    // `from` to `to`, `step` of `steps` of the way, each channel rounded half away from zero.
    let between = |from: [u8; 3], to: [u8; 3], step: u8, steps: u8| -> [u8; 3] {
        let (step, steps) = (i32::from(step), i32::from(steps));
        std::array::from_fn(|i| {
            let moved = (i32::from(to[i]) - i32::from(from[i])) * step;
            let rounded = (2 * moved + moved.signum() * steps) / (2 * steps);
            u8::try_from(i32::from(from[i]) + rounded).expect("between two channels")
        })
    };
    match shade {
        Shade::Water(height) => between(
            colour(Terrain::DeepWater),
            colour(Terrain::ShallowWater),
            height.min(19),
            19,
        ),
        Shade::Land(height) => match height.clamp(20, 100) {
            height @ ..60 => between(
                colour(Terrain::Grass),
                colour(Terrain::Rock),
                height - 20,
                40,
            ),
            height => between(
                colour(Terrain::Rock),
                colour(Terrain::Snow),
                height - 60,
                40,
            ),
        },
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
            Wizarding::Open(path) => {
                wizard.region = None;
                wizard.preview = None;
                wizard.previewing = None;
                wizard.picture = None;
                wizard.dragged_from = None;
                match read_export(Path::new(""), &path.to_string_lossy()) {
                    Ok(export) => {
                        let export = Arc::new(export);
                        wizard.summary = Some(Ok(export_summary(&export)));
                        let from = export.clone();
                        wizard.previewing = Some(AsyncComputeTaskPool::get().spawn(async move {
                            preview(&from.source, PREVIEW_SIDE).map_err(|error| error.to_string())
                        }));
                        wizard.export = Some(export);
                    }
                    Err(why) => {
                        wizard.summary = Some(Err(why));
                        wizard.export = None;
                    }
                }
            }
            Wizarding::Generate => {
                let Some(export) = wizard.export.clone() else {
                    continue;
                };
                if wizard.generating() {
                    continue;
                }
                let choices = wizard.choices();
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
    if let Some(done) = wizard.previewing.as_mut().and_then(check_ready) {
        wizard.previewing = None;
        match done {
            Ok(made) => wizard.preview = Some(made),
            Err(why) => wizard.note = Some(why),
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
    if let Some(export) = wizard.export.clone() {
        let source = export.source.size;
        if wizard.preview.is_some() {
            preview_ui(ui, wizard, source);
        } else if wizard.previewing() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Previewing…");
            });
        }
        region_ui(ui, wizard, source);
        ui.horizontal(|ui| {
            ui.label("Scale");
            ui.add(egui::DragValue::new(&mut wizard.scale).range(1..=MOST_SCALE))
                .on_hover_text("How many tiles a source pixel makes each way");
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut wizard.seed))
                .on_hover_text("The seed for generation");
        });
        let makes = wizard.makes();
        match &makes {
            Some(Ok(size)) => {
                ui.label(format!("Makes {} × {} tiles", size.width, size.height));
            }
            Some(Err(why)) => {
                ui.colored_label(ui.visuals().error_fg_color, why.as_str());
            }
            None => {}
        }
        ui.horizontal(|ui| {
            let generating = wizard.generating();
            let can = !generating && matches!(makes, Some(Ok(_)));
            if ui.add_enabled(can, egui::Button::new("Generate")).clicked() {
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

/// The preview, its picture made the first time it's shown, a drag on it choosing the region,
/// and the region chosen drawn over it.
fn preview_ui(ui: &mut egui::Ui, wizard: &mut Wizard, source: Size) {
    let Some(preview) = &wizard.preview else {
        return;
    };
    let picture = wizard.picture.get_or_insert_with(|| {
        let size = [preview.size.width as usize, preview.size.height as usize];
        let rgb: Vec<u8> = preview
            .pixels
            .iter()
            .flat_map(|&s| shade_colour(s))
            .collect();
        ui.ctx().load_texture(
            "wizard-preview",
            egui::ColorImage::from_rgb(size, &rgb),
            egui::TextureOptions::NEAREST,
        )
    });
    // Points a source pixel.
    let per_pixel = PREVIEW_SHOWN / source.width.max(source.height).max(1) as f32;
    let shown = egui::vec2(
        source.width as f32 * per_pixel,
        source.height as f32 * per_pixel,
    );
    let response = ui.add(egui::Image::new((picture.id(), shown)).sense(egui::Sense::drag()));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label("Preview");
    });
    let rect = response.rect;
    let to_source = |pos: egui::Pos2| {
        [
            (pos.x - rect.min.x) / per_pixel,
            (pos.y - rect.min.y) / per_pixel,
        ]
    };
    if response.drag_started() {
        wizard.dragged_from = ui
            .input(|input| input.pointer.press_origin())
            .map(to_source);
    }
    if response.dragged()
        && let (Some(from), Some(to)) = (wizard.dragged_from, response.interact_pointer_pos())
    {
        wizard.region = Some(region_between(from, to_source(to), source));
    }
    if response.drag_stopped() {
        wizard.dragged_from = None;
    }
    if let Some(region) = wizard.region {
        let corner = |x: u32, y: u32| rect.min + egui::vec2(x as f32, y as f32) * per_pixel;
        let chosen = egui::Rect::from_min_max(
            corner(region.x, region.y),
            corner(
                region.x.saturating_add(region.size.width),
                region.y.saturating_add(region.size.height),
            ),
        );
        ui.painter().rect_stroke(
            chosen,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
}

/// The region in four numbers, the whole source if none is chosen, its corner within the
/// source and a change to it held to the source, at least a pixel each way; and "Whole map",
/// which clears it.
fn region_ui(ui: &mut egui::Ui, wizard: &mut Wizard, source: Size) {
    let whole = Region {
        x: 0,
        y: 0,
        size: source,
    };
    let mut shown = wizard.region.unwrap_or(whole);
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut number =
            |ui: &mut egui::Ui, label: &str, named: &str, value: &mut u32, most: u32| {
                ui.label(label);
                let response = ui.add(
                    egui::DragValue::new(value)
                        .range(0..=most)
                        .clamp_existing_to_range(false),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label(named);
                });
                changed |= response.changed();
            };
        number(
            ui,
            "x",
            "Region x",
            &mut shown.x,
            source.width.saturating_sub(1),
        );
        number(
            ui,
            "y",
            "Region y",
            &mut shown.y,
            source.height.saturating_sub(1),
        );
        number(
            ui,
            "Width",
            "Region width",
            &mut shown.size.width,
            source.width,
        );
        number(
            ui,
            "Height",
            "Region height",
            &mut shown.size.height,
            source.height,
        );
    });
    if changed {
        let room = |side: u32, from: u32| side.saturating_sub(from).max(1);
        shown.size.width = shown.size.width.clamp(1, room(source.width, shown.x));
        shown.size.height = shown.size.height.clamp(1, room(source.height, shown.y));
        wizard.region = Some(shown);
    }
    if ui
        .add_enabled(wizard.region.is_some(), egui::Button::new("Whole map"))
        .clicked()
    {
        wizard.region = None;
    }
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

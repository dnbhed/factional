//! The inspector (G1, D-44, D-45, P-118, DESIGN.md §20): a window of the world's characters,
//! factions and event log, drawn with `bevy_egui` over the plugin's `Factional` resource and
//! shown and hidden by a key, so any game using the plugin can turn it on. What it says is
//! what the CLI says, in the session's own words.

use bevy::app::{App, Plugin, Update};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use factional_bevy::Factional;
use factional_reputation::World;
use factional_session::{describe_character, describe_event, describe_faction};

/// The inspector, opened and closed by `toggle`.
pub struct InspectorPlugin {
    pub toggle: KeyCode,
}

impl Default for InspectorPlugin {
    /// The backquote, the key games open their consoles with.
    fn default() -> Self {
        InspectorPlugin {
            toggle: KeyCode::Backquote,
        }
    }
}

/// Whether the inspector is open.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Inspector {
    pub open: bool,
}

/// The key that opens and closes it.
#[derive(Resource)]
struct Toggle(KeyCode);

impl Plugin for InspectorPlugin {
    /// Closed to begin with; `bevy_egui` added if the game hasn't.
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin::default());
        }
        app.insert_resource(Inspector::default())
            .insert_resource(Toggle(self.toggle))
            .add_systems(Update, toggle)
            .add_systems(EguiPrimaryContextPass, draw);
    }
}

/// Opens or closes the inspector as its key is pressed.
fn toggle(keys: Res<ButtonInput<KeyCode>>, key: Res<Toggle>, mut inspector: ResMut<Inspector>) {
    if keys.just_pressed(key.0) {
        inspector.open = !inspector.open;
    }
}

/// The window, while it's open and there's a world to show.
fn draw(
    mut contexts: EguiContexts,
    inspector: Res<Inspector>,
    factional: Option<Res<Factional>>,
) -> bevy::ecs::error::Result {
    let Some(factional) = factional.filter(|_| inspector.open) else {
        return Ok(());
    };
    inspector_window(contexts.ctx_mut()?, &factional.world);
    Ok(())
}

/// The inspector's window, showing `world`.
pub fn inspector_window(ctx: &egui::Context, world: &World) {
    egui::Window::new("Factional")
        .default_width(720.0)
        .show(ctx, |ui| inspector_ui(ui, world));
}

/// What the inspector's window shows of `world`: the tick, then its characters, its factions
/// and its events, each a line as the CLI says it.
pub fn inspector_ui(ui: &mut egui::Ui, world: &World) {
    ui.heading(format!("Tick {}", world.now()));
    let section = |ui: &mut egui::Ui, title: String, lines: Vec<String>| {
        egui::CollapsingHeader::new(title)
            .default_open(true)
            .show(ui, |ui| {
                for line in lines {
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
    };
    let characters: Vec<String> = world
        .characters()
        .map(|character| describe_character(world, character))
        .collect();
    section(ui, format!("Characters ({})", characters.len()), characters);
    let factions: Vec<String> = world
        .factions()
        .map(|faction| describe_faction(world, faction))
        .collect();
    section(ui, format!("Factions ({})", factions.len()), factions);
    let events = world.events();
    egui::CollapsingHeader::new(format!("Events ({})", events.len()))
        .default_open(true)
        .show(ui, |ui| {
            if events.is_empty() {
                ui.weak("No events yet.");
            }
            egui::ScrollArea::vertical()
                .id_salt("events")
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for event in events {
                        ui.label(egui::RichText::new(describe_event(event)).monospace());
                    }
                });
        });
}

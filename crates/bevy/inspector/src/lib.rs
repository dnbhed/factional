//! The inspector (G1, D-44, D-45, P-118, DESIGN.md §20): a window of the world's characters,
//! factions and event log, drawn with `bevy_egui` over the plugin's `Factional` resource and
//! shown and hidden by a key, so any game using the plugin can turn it on. What it says is
//! what the CLI says, in the session's own words.

use bevy::app::{App, Plugin, Update};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use factional_bevy::{Factional, FactionalEvent};
use factional_egui::console::{Asked, Console};
use factional_egui::relations::relations_ui;
use factional_quests::Played;
use factional_session::{
    Outcome, Session, describe_character, describe_event, describe_faction, describe_progress,
};

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

/// The inspector's console (G2b).
#[derive(Resource)]
struct InspectorConsole(Console);

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
            .insert_resource(InspectorConsole(Console::new("Console")))
            .insert_resource(Toggle(self.toggle))
            .add_systems(Update, toggle)
            .add_systems(EguiPrimaryContextPass, draw);
    }
}

/// Opens or closes the inspector as its key is pressed.
/// Opening it puts the cursor on its console's line.
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    key: Res<Toggle>,
    mut inspector: ResMut<Inspector>,
    mut console: ResMut<InspectorConsole>,
) {
    if keys.just_pressed(key.0) {
        inspector.open = !inspector.open;
        if inspector.open {
            console.0.focus();
        }
    }
}

/// The window, while it's open and there's a world to show; a line entered in its console
/// run on the game's world, and what it played sent to the game.
fn draw(
    mut contexts: EguiContexts,
    inspector: Res<Inspector>,
    mut console: ResMut<InspectorConsole>,
    factional: Option<ResMut<Factional>>,
    mut out: MessageWriter<FactionalEvent>,
) -> bevy::ecs::error::Result {
    let Some(mut factional) = factional.filter(|_| inspector.open) else {
        return Ok(());
    };
    let asked = inspector_window(contexts.ctx_mut()?, &factional, &mut console.0);
    if let Some(line) = asked.run {
        // Written, and so changed for the game, only if the line played something.
        let played = run_in_console(factional.bypass_change_detection(), &mut console.0, line);
        if !played.is_empty() {
            factional.set_changed();
        }
        out.write_batch(played.into_iter().map(FactionalEvent::from));
    }
    Ok(())
}

/// The inspector's window over the game's world: what `inspector_ui` shows, and below it the
/// console, Tab completing from the world. What the console asked.
pub fn inspector_window(
    ctx: &egui::Context,
    factional: &Factional,
    console: &mut Console,
) -> Asked {
    let mut asked = Asked::default();
    egui::Window::new("Factional")
        .default_width(720.0)
        .default_height(760.0)
        .show(ctx, |ui| {
            egui::Panel::bottom("inspector console")
                .resizable(true)
                .default_size(220.0)
                .show(ui, |ui| {
                    ui.set_min_height(ui.available_height());
                    asked = console.ui(ui, None, |typed| {
                        Session::on(factional.world.clone(), factional.quests.clone())
                            .complete(typed)
                    });
                });
            egui::ScrollArea::vertical()
                .id_salt("inspector")
                .show(ui, |ui| inspector_ui(ui, factional));
        });
    asked
}

/// Runs `line` in the console on the game's world, as the session runs it on a world it was
/// handed, and says what it said. The world is written back only if the line played
/// something; what it played is given back, in order.
pub fn run_in_console(
    factional: &mut Factional,
    console: &mut Console,
    line: String,
) -> Vec<Played> {
    let mut session = Session::on(factional.world.clone(), factional.quests.clone());
    let said = match session.execute(&line) {
        Ok(Outcome::Quit) => {
            "The console doesn't quit: close the inspector with its key instead".to_owned()
        }
        Ok(outcome) => outcome.render(),
        Err(error) => format!("error: {error}"),
    };
    let marks = session.marks(&said);
    console.say(Some(line), said, marks);
    let played = session.played().to_vec();
    if !played.is_empty()
        && let Some((world, quests)) = session.into_world()
    {
        factional.world = world;
        factional.quests = quests;
    }
    played
}

/// What the inspector's window shows of the game's world: the tick, then its characters, its
/// factions, their relations, each character's quests and its events, each as the CLI or the
/// editor says it.
pub fn inspector_ui(ui: &mut egui::Ui, factional: &Factional) {
    let Factional { world, quests } = factional;
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
    egui::CollapsingHeader::new("Relations")
        .default_open(true)
        .show(ui, |ui| relations_ui(ui, world));
    egui::CollapsingHeader::new("Quests")
        .default_open(true)
        .show(ui, |ui| {
            for character in world.characters() {
                let progress = describe_progress(quests, &character.id);
                // Those who haven't started any say so in a line with their id in it.
                if quests.record(&character.id).states.is_empty() {
                    ui.label(egui::RichText::new(progress).monospace());
                    continue;
                }
                ui.label(egui::RichText::new(character.id.as_str()).strong());
                ui.indent(character.id.as_str(), |ui| {
                    for line in progress.lines() {
                        ui.label(egui::RichText::new(line).monospace());
                    }
                });
            }
        });
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

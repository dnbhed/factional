//! The inspector (G1, D-44, P-118): a window of the world's characters, factions and event
//! log, in the CLI's own words, opened and closed by a key.

use std::path::{Path, PathBuf};

use bevy::MinimalPlugins;
use bevy::app::{App, Last};
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Query;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::window::{ExitCondition, WindowPlugin};
use bevy_egui::egui::Order;
use bevy_egui::{EguiContext, EguiPlugin, EguiRenderOutput, PrimaryEguiContext};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::FactionalPlugin;
use factional_bevy_inspector::{Inspector, InspectorPlugin, inspector_window};
use factional_core::Fixed;
use factional_reputation::{ActionId, CharacterId, Command, Witnesses, World};
use factional_session::{describe_event, describe_faction};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn app(inspector: InspectorPlugin) -> App {
    let mut app = App::new();
    // A game's window messages, without a window.
    let windows = WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..WindowPlugin::default()
    };
    app.add_plugins((MinimalPlugins, InputPlugin, windows))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(inspector)
        .add_systems(Last, take_output);
    app.update();
    app
}

/// What egui drew, taken as a renderer would take it, since there's no renderer here.
fn take_output(mut outputs: Query<&mut EguiRenderOutput>) {
    for mut output in &mut outputs {
        output.textures_delta.clear();
    }
}

/// Presses `key` and lets it go, a frame each.
fn press(app: &mut App, key: KeyCode) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code: key,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
    }
}

fn open(app: &App) -> bool {
    app.world().resource::<Inspector>().open
}

fn riverhold() -> World {
    let (content, _) = factional_content::load_quests(&sample()).expect("the sample loads");
    World::new(content).expect("the sample is complete")
}

fn steal(world: &mut World) {
    world
        .execute(Command::PerformAction {
            actor: CharacterId::new("player").expect("an id"),
            action: ActionId::new("steal").expect("an id"),
            target: Some(CharacterId::new("merchant_ava").expect("an id")),
            scale: Fixed::ONE,
            witnesses: Witnesses::Everyone,
        })
        .expect("the player can steal");
}

fn drawn(world: &World) -> Harness<'_> {
    let mut harness = Harness::builder()
        .with_size(bevy_egui::egui::vec2(760.0, 900.0))
        .wgpu()
        .build_ui(move |ui| inspector_window(ui.ctx(), world));
    harness.run();
    harness
}

#[test]
fn the_inspector_starts_closed_and_the_backquote_opens_and_closes_it() {
    let mut app = app(InspectorPlugin::default());
    assert!(!open(&app));
    press(&mut app, KeyCode::Backquote);
    assert!(open(&app));
    press(&mut app, KeyCode::Backquote);
    assert!(!open(&app));
}

#[test]
fn the_key_that_opens_it_is_the_games_choice() {
    let mut app = app(InspectorPlugin {
        toggle: KeyCode::F1,
    });
    press(&mut app, KeyCode::Backquote);
    assert!(!open(&app));
    press(&mut app, KeyCode::F1);
    assert!(open(&app));
}

/// Whether a window was drawn last frame in the context on `entity`: the inspector's is
/// the only one there.
fn window_shown(app: &mut App, entity: Entity) -> bool {
    let mut context = app
        .world_mut()
        .get_mut::<EguiContext>(entity)
        .expect("an egui context");
    context.get_mut().memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .iter()
            .any(|layer| layer.order == Order::Middle)
    })
}

#[test]
fn the_open_inspector_shows_its_window_and_the_closed_one_doesnt() {
    let mut app = app(InspectorPlugin::default());
    // A game's egui context, as a camera would carry it, without a window.
    let context = app.world_mut().spawn(PrimaryEguiContext).id();
    app.update();
    assert!(!window_shown(&mut app, context));
    press(&mut app, KeyCode::Backquote);
    assert!(window_shown(&mut app, context));
    press(&mut app, KeyCode::Backquote);
    assert!(!window_shown(&mut app, context));
}

#[test]
fn the_inspector_brings_egui_if_the_game_hasnt() {
    let app = app(InspectorPlugin::default());
    assert!(app.is_plugin_added::<EguiPlugin>());
}

#[test]
fn the_window_says_the_tick_and_every_character_and_faction_as_the_cli_does() {
    let world = riverhold();
    let harness = drawn(&world);
    harness.get_by_label("Tick 0");
    // DESIGN.md §13: Vex, chaotic by law -55.00, a fence in the Lantern Guild.
    harness.get_by_label(
        "vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of lantern_guild (fence) since tick 0",
    );
    // Every member says since when: Hale, Mira, Vex and Brother Ash (characters.toml).
    assert_eq!(
        harness.query_all_by_label_contains(" since tick 0").count(),
        4
    );
    for faction in world.factions() {
        harness.get_by_label(&describe_faction(&world, faction));
    }
}

#[test]
fn with_nothing_played_the_log_says_so() {
    let world = riverhold();
    drawn(&world).get_by_label("No events yet.");
}

#[test]
fn the_log_has_each_event_oldest_first_as_the_cli_says_it() {
    let mut world = riverhold();
    steal(&mut world);
    let harness = drawn(&world);
    let first = harness.get_by_label("#1 at tick 0: player did steal, targeting merchant_ava");
    let mut above = first.rect().top();
    for event in &world.events()[1..] {
        let said = describe_event(event);
        let top = harness.get_by_label(&said).rect().top();
        assert!(top > above, "{said} is out of order");
        above = top;
    }
    assert!(harness.query_by_label("No events yet.").is_none());
}

/// Its picture, taken as the editor's are (P-120).
#[test]
fn the_window_after_a_theft() {
    let mut world = riverhold();
    steal(&mut world);
    drawn(&world).snapshot("inspector");
}

//! The inspector (G1, D-44, P-118): a window of the world's characters, factions and event
//! log, in the CLI's own words, opened and closed by a key; and its console (G2b), running the
//! session's commands on the game's world and sending the game what they played.

use std::path::{Path, PathBuf};

use bevy::MinimalPlugins;
use bevy::app::{App, Last};
use bevy::camera::Camera;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::Messages;
use bevy::ecs::system::Query;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::window::{ExitCondition, WindowPlugin};
use bevy_egui::egui::{self, Order};
use bevy_egui::input::EguiInputEvent;
use bevy_egui::{EguiContext, EguiPlugin, EguiRenderOutput, PrimaryEguiContext};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::{Factional, FactionalEvent, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin, inspector_window, run_in_console};
use factional_console::Console;
use factional_core::Fixed;
use factional_quests::{Played, QuestLog};
use factional_reputation::{ActionId, Change, CharacterId, Command, Witnesses, World};
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

fn riverhold() -> Factional {
    let (content, quests) = factional_content::load_quests(&sample()).expect("the sample loads");
    Factional {
        world: World::new(content).expect("the sample is complete"),
        quests: QuestLog::new(quests),
    }
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

/// The window drawn over `factional`, with its console, each line entered run in it.
fn drawn(factional: Factional) -> Harness<'static, (Factional, Console)> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(760.0, 900.0))
        .wgpu()
        .build_ui_state(
            |ui, (factional, console): &mut (Factional, Console)| {
                let asked = inspector_window(ui.ctx(), factional, console);
                if let Some(line) = asked.run {
                    run_in_console(factional, console, line);
                }
            },
            (factional, Console::new("Console")),
        );
    harness.run();
    harness
}

/// Types `line` into the console and enters it.
fn enter(harness: &mut Harness<'static, (Factional, Console)>, line: &str) {
    harness.get_by_label("Console command").focus();
    harness.run();
    harness.get_by_label("Console command").type_text(line);
    harness.key_press(egui::Key::Enter);
    harness.run();
}

/// What the console said last.
fn said_last(console: &Console) -> &str {
    &console.log().last().expect("something said").1
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
    let harness = drawn(riverhold());
    let world = &harness.state().0.world;
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
        harness.get_by_label(&describe_faction(world, faction));
    }
}

#[test]
fn with_nothing_played_the_log_says_so() {
    drawn(riverhold()).get_by_label("No events yet.");
}

#[test]
fn the_log_has_each_event_oldest_first_as_the_cli_says_it() {
    let mut factional = riverhold();
    steal(&mut factional.world);
    let harness = drawn(factional);
    let world = &harness.state().0.world;
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

#[test]
fn a_command_run_in_the_console_plays_on_the_games_world_and_says_so() {
    let mut harness = drawn(riverhold());
    enter(&mut harness, "act player steal --target merchant_ava");
    assert_eq!(harness.state().0.world.events().len(), 3);
    harness.get_by_label("> act player steal --target merchant_ava");
    // In the event log, and in what the console said, its ids links.
    harness.get_by_label("#1 at tick 0: player did steal, targeting merchant_ava");
    assert!(said_last(&harness.state().1).starts_with("#1 at tick 0: player did steal"));
    assert_eq!(harness.query_all_by_label("Go to merchant_ava").count(), 2);
}

#[test]
fn what_a_command_played_is_given_back_in_order() {
    let mut factional = riverhold();
    let mut console = Console::new("Console");
    let played = run_in_console(
        &mut factional,
        &mut console,
        "act player steal --target merchant_ava".to_owned(),
    );
    let events: Vec<&Played> = played.iter().collect();
    assert_eq!(
        events,
        factional
            .world
            .events()
            .iter()
            .map(|event| Played::World(event.clone()))
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        &played[0],
        Played::World(event) if matches!(event.payload, Change::ActionPerformed { .. })
    ));
    assert!(said_last(&console).starts_with("#1 at tick 0: player did steal"));
}

#[test]
fn a_command_that_plays_nothing_leaves_the_world_as_it_was() {
    let mut factional = riverhold();
    let mut console = Console::new("Console");
    let played = run_in_console(&mut factional, &mut console, "characters".to_owned());
    assert!(played.is_empty());
    assert!(said_last(&console).starts_with("brother_ash — Brother Ash"));
    let played = run_in_console(
        &mut factional,
        &mut console,
        "load content/sample".to_owned(),
    );
    assert!(played.is_empty());
    assert_eq!(
        said_last(&console),
        "error: load works on a content directory, and this session was handed its world rather than loading one"
    );
    assert!(factional.world.events().is_empty());
}

#[test]
fn the_console_doesnt_quit() {
    let mut factional = riverhold();
    let mut console = Console::new("Console");
    run_in_console(&mut factional, &mut console, "quit".to_owned());
    assert_eq!(
        said_last(&console),
        "The console doesn't quit: close the inspector with its key instead"
    );
}

/// Types `text` into the inspector's console in the app, and enters it, as egui is told.
fn type_in(app: &mut App, context: Entity, text: &str) {
    for event in [
        egui::Event::Text(text.to_owned()),
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    ] {
        app.world_mut()
            .write_message(EguiInputEvent { context, event });
        app.update();
    }
    app.update();
}

/// What the game was sent last frame.
fn sent(app: &App) -> Vec<FactionalEvent> {
    let messages = app.world().resource::<Messages<FactionalEvent>>();
    messages.iter_current_update_messages().cloned().collect()
}

#[test]
fn what_the_console_plays_is_sent_to_the_game_as_its_events() {
    let mut app = app(InspectorPlugin::default());
    // A game's camera, carrying its egui context, without a window.
    let context = app
        .world_mut()
        .spawn((Camera::default(), PrimaryEguiContext))
        .id();
    app.update();
    // Opening it puts the cursor on the console's line.
    press(&mut app, KeyCode::Backquote);
    let changed = app
        .world()
        .get_resource_change_ticks::<Factional>()
        .expect("the world")
        .changed;
    type_in(&mut app, context, "characters");
    let unchanged = app
        .world()
        .get_resource_change_ticks::<Factional>()
        .expect("the world")
        .changed;
    assert_eq!(
        changed, unchanged,
        "a command that plays nothing writes nothing"
    );
    let mut events = Vec::new();
    app.world_mut().write_message(EguiInputEvent {
        context,
        event: egui::Event::Text("act player steal --target merchant_ava".to_owned()),
    });
    app.update();
    app.world_mut().write_message(EguiInputEvent {
        context,
        event: egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    });
    app.update();
    events.extend(sent(&app));
    let world = &app.world().resource::<Factional>().world;
    assert_eq!(world.events().len(), 3);
    let expected: Vec<FactionalEvent> = world
        .events()
        .iter()
        .cloned()
        .map(FactionalEvent::World)
        .collect();
    assert_eq!(events, expected);
}

/// Its picture, taken as the editor's are (P-120): the console after a theft.
#[test]
fn the_window_after_a_theft_in_its_console() {
    let mut harness = drawn(riverhold());
    enter(&mut harness, "act player steal --target merchant_ava");
    harness.snapshot("inspector");
}

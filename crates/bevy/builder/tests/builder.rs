//! The builder's shell (G4): each character a token on the plane, placed from `builder.toml`
//! or laid out on a ring; problems with the file said and left out; and time, paused,
//! stepped or run.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::message::Messages;
use bevy::math::Vec2;
use bevy::time::{Time, TimeUpdateStrategy, Virtual};
use bevy::transform::components::Transform;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::{Factional, FactionalEvent, FactionalPlugin};
use factional_builder::{BuilderPlugin, BuilderProblems, Clock, RING, Token, controls_ui, place};
use factional_core::Tick;
use factional_reputation::CharacterId;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn content(dir: &str) -> PathBuf {
    Path::new(REPO).join(dir)
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn riverholds_six() -> Vec<CharacterId> {
    [
        "brother_ash",
        "captain_hale",
        "merchant_ava",
        "player",
        "sister_mira",
        "vex",
    ]
    .map(id)
    .to_vec()
}

/// The builder on `dir`, a second of game time a frame.
fn app(dir: &str) -> App {
    app_at(dir, Duration::from_secs(1))
}

/// The builder on `dir`, `frame` of game time a frame.
fn app_at(dir: &str, frame: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .add_plugins(FactionalPlugin {
            content: content(dir),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: content(dir),
            player: "player".to_owned(),
        });
    // Game time takes whole seconds a frame here, not Bevy's quarter-second at most.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(10));
    app.update();
    app
}

fn tokens(app: &mut App) -> Vec<(CharacterId, Vec2)> {
    let mut found: Vec<(CharacterId, Vec2)> = app
        .world_mut()
        .query::<(&Token, &Transform)>()
        .iter(app.world())
        .map(|(token, at)| (token.character.clone(), at.translation.truncate()))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn now(app: &App) -> Tick {
    app.world().resource::<Factional>().world.now()
}

#[test]
fn each_character_is_a_token_where_builder_toml_puts_them() {
    let mut app = app("content/sample");
    // content/sample/builder.toml.
    assert_eq!(
        tokens(&mut app),
        [
            (id("brother_ash"), Vec2::new(40.0, 220.0)),
            (id("captain_hale"), Vec2::new(-180.0, 90.0)),
            (id("merchant_ava"), Vec2::new(140.0, -40.0)),
            (id("player"), Vec2::new(0.0, 0.0)),
            (id("sister_mira"), Vec2::new(200.0, 140.0)),
            (id("vex"), Vec2::new(-140.0, -130.0)),
        ]
    );
    assert!(app.world().resource::<BuilderProblems>().0.is_empty());
}

#[test]
fn with_no_file_everyone_is_laid_out_on_a_ring_in_id_order_the_first_at_the_top() {
    let placed = place(None, &riverholds_six());
    assert!(placed.problems.is_empty());
    let ids: Vec<&CharacterId> = placed.positions.iter().map(|(id, _)| id).collect();
    assert_eq!(ids, riverholds_six().iter().collect::<Vec<_>>());
    let at: Vec<Vec2> = placed.positions.iter().map(|(_, at)| *at).collect();
    // The first at the top, the rest clockwise, evenly spaced on one ring.
    assert!(at[0].x.abs() < 0.001 && at[0].y > 0.0, "{:?}", at[0]);
    let radius = at[0].length();
    assert!((radius - RING).abs() < 0.001, "on the ring: {radius}");
    for (i, point) in at.iter().enumerate() {
        assert!((point.length() - radius).abs() < 0.001, "{i}: {point:?}");
        let next = at[(i + 1) % at.len()];
        let turn = point.angle_to(next);
        assert!(
            (turn + std::f32::consts::TAU / 6.0).abs() < 0.001,
            "{i}: turns {turn} to the next"
        );
    }
}

#[test]
fn a_character_the_file_doesnt_place_is_laid_out() {
    let file = "[positions]\nplayer = { x = 5.0, y = 6.0 }\n";
    let placed = place(Some(file), &[id("player"), id("vex")]);
    assert_eq!(placed.positions[0], (id("player"), Vec2::new(5.0, 6.0)));
    assert_eq!(placed.positions[1].0, id("vex"));
    assert!(
        placed.positions[1].1.length() > 0.0,
        "laid out off the middle"
    );
    assert!(placed.problems.is_empty());
}

#[test]
fn a_position_for_no_character_is_said_and_left_out() {
    let file = "[positions]\nvexx = { x = 1.0, y = 2.0 }\nvex = { x = 3.0, y = 4.0 }\n";
    let placed = place(Some(file), &[id("vex")]);
    assert_eq!(placed.positions, [(id("vex"), Vec2::new(3.0, 4.0))]);
    assert_eq!(
        placed.problems,
        ["builder.toml: positions.vexx names no character (did you mean 'vex'?), so it's left out"]
    );
}

#[test]
fn a_file_that_isnt_toml_is_said_and_everyone_laid_out() {
    let placed = place(Some("[positions\n"), &[id("vex")]);
    assert_eq!(placed.positions.len(), 1);
    assert_eq!(placed.problems.len(), 1);
    assert!(
        placed.problems[0].starts_with("builder.toml: ")
            && placed.problems[0].ends_with(", so everyone is laid out"),
        "{}",
        placed.problems[0]
    );
    let placed = place(Some("[elsewhere]\n"), &[id("vex")]);
    assert_eq!(placed.problems.len(), 1, "unknown keys are said too");
}

#[test]
fn time_starts_paused_and_is_stepped_run_and_paused() {
    let mut app = app("content/sample");
    for _ in 0..3 {
        app.update();
        assert!(sent(&app).is_empty(), "paused, nothing is sent to the game");
    }
    assert_eq!(now(&app), Tick(0), "paused, frames pass and time doesn't");
    app.world_mut().resource_mut::<Clock>().step();
    app.update();
    assert_eq!(now(&app), Tick(1));
    app.update();
    assert_eq!(now(&app), Tick(1), "a step is one tick");
    app.world_mut().resource_mut::<Clock>().run();
    app.update();
    app.update();
    assert_eq!(now(&app), Tick(3), "a tick a second while running");
    app.world_mut().resource_mut::<Clock>().pause();
    app.update();
    assert_eq!(now(&app), Tick(3));
}

#[test]
fn the_controls_say_whether_time_runs_and_run_or_pause_it() {
    let mut harness = Harness::new_ui_state(
        |ui, clock: &mut Clock| controls_ui(ui, clock, Tick(4)),
        Clock::default(),
    );
    harness.run();
    harness.get_by_label("Paused");
    harness.get_by_label("Tick 4");
    harness.get_by_label("Step");
    harness.get_by_label("Run").click();
    harness.run();
    assert!(harness.state().running());
    harness.get_by_label("Running");
    harness.get_by_label("Pause").click();
    harness.run();
    assert!(!harness.state().running());
}

/// What the game was sent last frame.
fn sent(app: &App) -> Vec<FactionalEvent> {
    let messages = app.world().resource::<Messages<FactionalEvent>>();
    messages.iter_current_update_messages().cloned().collect()
}

#[test]
fn game_time_between_ticks_carries_over_to_the_next_frame() {
    let mut app = app_at("content/sample", Duration::from_millis(1500));
    app.world_mut().resource_mut::<Clock>().run();
    app.update();
    assert_eq!(
        now(&app),
        Tick(1),
        "a second and a half is one tick, half left over"
    );
    app.update();
    assert_eq!(
        now(&app),
        Tick(3),
        "the half and a second and a half more are two"
    );
}

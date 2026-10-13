//! Playing a character in the builder (G5): walked by the keys, acting on the nearest, those
//! near enough seeing it. Positions are the sample's `builder.toml`; distances worked by hand.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input::{ButtonState, InputPlugin};
use bevy::math::Vec2;
use bevy::time::{Time, TimeUpdateStrategy, Virtual};
use bevy::transform::components::Transform;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::{Factional, FactionalPlugin};
use factional_builder::{
    BuilderPlugin, BuilderProblems, KeyboardTaken, Nearby, Play, Played, Token, act, act_ui,
    nearby, place,
};
use factional_core::Fixed;
use factional_reputation::{ActionId, Change, CharacterId, Command, Witnesses};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn ids(list: &[&str]) -> BTreeSet<CharacterId> {
    list.iter().map(|text| id(text)).collect()
}

/// The builder on the sample, playing `player`, a second of game time a frame.
fn app(player: &str) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: player.to_owned(),
        });
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(10));
    app.update();
    app
}

fn played(app: &mut App) -> Option<(CharacterId, Vec2)> {
    app.world_mut()
        .query_filtered::<(&Token, &Transform), With<Played>>()
        .iter(app.world())
        .next()
        .map(|(token, at)| (token.character.clone(), at.translation.truncate()))
}

fn key(app: &mut App, key: KeyCode, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code: key,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// Holds `code` down for `frames` frames of a second, then lets it go.
fn hold(app: &mut App, code: KeyCode, frames: usize) {
    key(app, code, ButtonState::Pressed);
    for _ in 0..frames {
        app.update();
    }
    key(app, code, ButtonState::Released);
    app.update();
}

fn sample_positions() -> Vec<(CharacterId, Vec2)> {
    vec![
        (id("brother_ash"), Vec2::new(40.0, 220.0)),
        (id("captain_hale"), Vec2::new(-180.0, 90.0)),
        (id("merchant_ava"), Vec2::new(140.0, -40.0)),
        (id("sister_mira"), Vec2::new(200.0, 140.0)),
        (id("vex"), Vec2::new(-140.0, -130.0)),
    ]
}

#[test]
fn the_player_is_played_or_whoever_as_names() {
    let mut app = app("player");
    assert_eq!(played(&mut app), Some((id("player"), Vec2::ZERO)));
    let mut app = self::app("vex");
    assert_eq!(
        played(&mut app),
        Some((id("vex"), Vec2::new(-140.0, -130.0)))
    );
}

#[test]
fn as_naming_no_character_is_said_and_no_one_is_played() {
    let mut app = app("playr");
    assert_eq!(played(&mut app), None);
    assert!(app.world().resource::<BuilderProblems>().0.contains(
        &"--as playr names no character (did you mean 'player'?), so no one is played".to_owned()
    ));
}

#[test]
fn the_keys_walk_the_played_token_at_its_speed_even_while_time_is_paused() {
    let mut app = app("player");
    hold(&mut app, KeyCode::ArrowRight, 1);
    // 200 points a second, the default, for a second.
    assert_eq!(
        played(&mut app),
        Some((id("player"), Vec2::new(200.0, 0.0)))
    );
    hold(&mut app, KeyCode::KeyW, 1);
    assert_eq!(
        played(&mut app),
        Some((id("player"), Vec2::new(200.0, 200.0)))
    );
    hold(&mut app, KeyCode::KeyA, 1);
    hold(&mut app, KeyCode::ArrowDown, 1);
    assert_eq!(played(&mut app), Some((id("player"), Vec2::ZERO)));
    let world = &app.world().resource::<Factional>().world;
    assert_eq!(world.now().0, 0, "the world's time stayed paused");
}

#[test]
fn while_something_else_has_the_keyboard_the_keys_dont_walk() {
    let mut app = app("player");
    app.insert_resource(KeyboardTaken(true));
    hold(&mut app, KeyCode::ArrowRight, 1);
    assert_eq!(played(&mut app), Some((id("player"), Vec2::ZERO)));
}

#[test]
fn the_nearest_is_the_closest_within_reach_and_those_in_sight_would_see() {
    let play = Play::default();
    // At the middle no one is within 80; Ava is 145.6 away.
    let near = nearby(Vec2::ZERO, &sample_positions(), &play);
    assert_eq!(near.nearest, None);
    // At (120, 0): Ava is 44.7 away; Mira 161.2 and Ash 234.1 are in sight; Vex 290.0 and
    // Hale 313.2 aren't.
    let near = nearby(Vec2::new(120.0, 0.0), &sample_positions(), &play);
    assert_eq!(near.nearest, Some(id("merchant_ava")));
    assert_eq!(
        near.seen_by,
        ids(&["brother_ash", "merchant_ava", "sister_mira"])
    );
}

#[test]
fn of_two_as_near_the_first_by_id_is_nearest() {
    for others in [
        vec![
            (id("vex"), Vec2::new(10.0, 0.0)),
            (id("ash"), Vec2::new(-10.0, 0.0)),
        ],
        vec![
            (id("ash"), Vec2::new(-10.0, 0.0)),
            (id("vex"), Vec2::new(10.0, 0.0)),
        ],
    ] {
        let near = nearby(Vec2::ZERO, &others, &Play::default());
        assert_eq!(near.nearest, Some(id("ash")), "{others:?}");
    }
}

#[test]
fn of_two_in_reach_the_closer_is_nearest_whatever_their_ids() {
    for others in [
        vec![
            (id("ash"), Vec2::new(50.0, 0.0)),
            (id("vex"), Vec2::new(20.0, 0.0)),
        ],
        vec![
            (id("vex"), Vec2::new(20.0, 0.0)),
            (id("ash"), Vec2::new(50.0, 0.0)),
        ],
    ] {
        let near = nearby(Vec2::ZERO, &others, &Play::default());
        assert_eq!(near.nearest, Some(id("vex")), "{others:?}");
    }
}

#[test]
fn reach_sight_and_speed_are_builder_tomls_and_must_be_above_zero() {
    let placed = place(
        Some("[play]\nreach = 50.0\nsight = 90.0\nspeed = 30.0\n"),
        &[],
    );
    assert_eq!(
        placed.play,
        Play {
            reach: 50.0,
            sight: 90.0,
            speed: 30.0
        }
    );
    assert!(placed.problems.is_empty());
    let placed = place(Some("[play]\nreach = 0.0\nspeed = -2.0\n"), &[]);
    assert_eq!(placed.play, Play::default());
    assert_eq!(
        placed.problems,
        [
            "builder.toml: play.reach must be above 0, so it's 80",
            "builder.toml: play.speed must be above 0, so it's 200",
        ]
    );
    assert_eq!(
        Play::default(),
        Play {
            reach: 80.0,
            sight: 240.0,
            speed: 200.0
        }
    );
}

#[test]
fn acting_is_the_played_on_the_nearest_seen_by_those_in_sight() {
    let near = Nearby {
        nearest: Some(id("merchant_ava")),
        seen_by: ids(&["merchant_ava", "sister_mira"]),
    };
    let steal = ActionId::new("steal").expect("an id");
    assert_eq!(
        act(&id("player"), steal.clone(), &near),
        factional_bevy::FactionalCommand::World(Command::PerformAction {
            actor: id("player"),
            action: steal.clone(),
            target: Some(id("merchant_ava")),
            scale: Fixed::ONE,
            witnesses: Witnesses::These(ids(&["merchant_ava", "sister_mira"])),
        })
    );
    assert_eq!(
        act(&id("player"), steal.clone(), &Nearby::default()),
        factional_bevy::FactionalCommand::World(Command::PerformAction {
            actor: id("player"),
            action: steal,
            target: None,
            scale: Fixed::ONE,
            witnesses: Witnesses::Nobody,
        })
    );
}

#[test]
fn a_theft_in_the_builder_is_witnessed_by_those_who_were_near() {
    let mut app = app("player");
    // 120 points right: 0.6 of a second at 200 a second.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        600,
    )));
    hold(&mut app, KeyCode::ArrowRight, 1);
    let (_, at) = played(&mut app).expect("played");
    assert!(at.distance(Vec2::new(120.0, 0.0)) < 0.01, "{at:?}");
    let near = app.world().resource::<Nearby>().clone();
    assert_eq!(near.nearest, Some(id("merchant_ava")));
    let steal = act(&id("player"), ActionId::new("steal").expect("an id"), &near);
    app.world_mut().write_message(steal);
    app.update();
    let world = &app.world().resource::<Factional>().world;
    let Change::ActionPerformed {
        witnesses, target, ..
    } = &world.events()[0].payload
    else {
        panic!("{:?}", world.events()[0]);
    };
    assert_eq!(target, &Some(id("merchant_ava")));
    assert_eq!(
        witnesses,
        &Witnesses::These(ids(&["brother_ash", "merchant_ava", "sister_mira"]))
    );
}

#[test]
fn the_act_window_lists_the_actions_and_says_on_whom_and_seen_by_whom() {
    let actions: Vec<ActionId> = ["extort", "steal"]
        .map(|action| ActionId::new(action).expect("an id"))
        .to_vec();
    let near = Nearby {
        nearest: Some(id("merchant_ava")),
        seen_by: ids(&["captain_hale", "merchant_ava"]),
    };
    let mut harness = Harness::new_ui_state(
        move |ui, chosen: &mut Vec<ActionId>| {
            chosen.extend(act_ui(ui, &actions, &near));
        },
        Vec::new(),
    );
    harness.run();
    harness.get_by_label("On merchant_ava, seen by captain_hale, merchant_ava");
    let extort = harness.get_by_label("extort").rect().top();
    let steal = harness.get_by_label("steal").rect().top();
    assert!(extort < steal, "in id order");
    harness.get_by_label("steal").click();
    harness.run();
    assert_eq!(harness.state(), &[ActionId::new("steal").expect("an id")]);
    let mut harness = Harness::new_ui(|ui| {
        act_ui(ui, &[], &Nearby::default());
    });
    harness.run();
    harness.get_by_label("On no one, seen by no one");
}

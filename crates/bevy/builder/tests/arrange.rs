//! Arranging the tokens (G9): dragged in the Arrange mode, each drag undoable, the layout
//! saved to `builder.toml` with everything else in it kept. Positions are the sample's.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input::{ButtonState, InputPlugin};
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::components::Transform;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_bevy::FactionalPlugin;
use factional_builder::{
    Arranging, BuilderPlugin, Played, Pointer, Token, arrange_ui, pick, place, write_positions,
};
use factional_reputation::CharacterId;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

/// A fresh copy of the sample as `<tmp>/<name>`, with `builder` as its `builder.toml`, or
/// none.
fn copy(name: &str, builder: Option<&str>) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for file in fs::read_dir(sample()).expect("the sample") {
        let file = file.expect("a file").path();
        fs::copy(&file, dir.join(file.file_name().expect("a name"))).expect("copied");
    }
    match builder {
        Some(text) => fs::write(dir.join("builder.toml"), text).expect("written"),
        None => fs::remove_file(dir.join("builder.toml")).expect("removed"),
    }
    dir
}

/// The builder on `content`, playing the player, a second a frame.
fn app(content: PathBuf) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: content.clone(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content,
            player: "player".to_owned(),
        });
    app.update();
    app
}

fn at(app: &mut App, character: &str) -> Vec2 {
    app.world_mut()
        .query::<(&Token, &Transform)>()
        .iter(app.world())
        .find(|(token, _)| token.character == id(character))
        .map(|(_, at)| at.translation.truncate())
        .expect("a token")
}

/// The pointer at `x, y`, pressed or not, for a frame.
fn point(app: &mut App, x: f32, y: f32, down: bool) {
    *app.world_mut().resource_mut::<Pointer>() = Pointer {
        at: Some(Vec2::new(x, y)),
        down,
    };
    app.update();
}

/// Hale, at (-180, 90), gripped 5 points right of his middle and dragged to (105, 10).
fn drag_hale(app: &mut App) {
    point(app, -175.0, 90.0, true);
    point(app, 105.0, 10.0, true);
    point(app, 105.0, 10.0, false);
}

fn arranging(app: &mut App) -> &mut Arranging {
    app.world_mut().resource_mut::<Arranging>().into_inner()
}

#[test]
fn a_press_picks_the_nearest_token_within_its_radius_the_first_by_id_if_as_near() {
    let tokens = [
        (id("merchant_ava"), Vec2::new(-170.0, 90.0)),
        (id("captain_hale"), Vec2::new(-180.0, 90.0)),
    ];
    assert_eq!(
        pick(Vec2::new(-171.0, 90.0), &tokens),
        Some(id("merchant_ava"))
    );
    assert_eq!(
        pick(Vec2::new(-179.0, 90.0), &tokens),
        Some(id("captain_hale"))
    );
    assert_eq!(
        pick(Vec2::new(-175.0, 90.0), &tokens),
        Some(id("captain_hale")),
        "as near: the first by id"
    );
    assert_eq!(
        pick(Vec2::new(-180.0, 104.0), &tokens),
        Some(id("captain_hale")),
        "14 points away is within"
    );
    assert_eq!(pick(Vec2::new(-190.0, 105.0), &tokens), None);
    assert_eq!(pick(Vec2::ZERO, &[]), None);
}

#[test]
fn while_arranging_a_token_is_dragged_from_where_it_was_gripped() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    point(&mut app, -175.0, 90.0, true);
    assert_eq!(arranging(&mut app).held(), Some(&id("captain_hale")));
    point(&mut app, 105.0, 10.0, true);
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(100.0, 10.0));
    point(&mut app, 105.0, 10.0, false);
    assert_eq!(arranging(&mut app).held(), None, "let go, it's dropped");
    point(&mut app, 0.0, 0.0, false);
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(100.0, 10.0));
}

#[test]
fn a_press_on_empty_space_holds_nothing_and_a_drag_onto_a_token_doesnt_pick_it() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    point(&mut app, 0.0, -300.0, true);
    assert_eq!(arranging(&mut app).held(), None);
    point(&mut app, -180.0, 90.0, true);
    assert_eq!(arranging(&mut app).held(), None, "only a press picks");
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(-180.0, 90.0));
}

#[test]
fn not_arranging_the_pointer_moves_nothing() {
    let mut app = app(sample());
    drag_hale(&mut app);
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(-180.0, 90.0));
    assert!(!arranging(&mut app).can_undo());
}

#[test]
fn while_arranging_the_keys_dont_walk() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::ArrowRight,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();
    app.update();
    let player = app
        .world_mut()
        .query_filtered::<&Transform, bevy::ecs::query::With<Played>>()
        .single(app.world())
        .expect("played")
        .translation
        .truncate();
    assert_eq!(player, Vec2::ZERO);
}

#[test]
fn each_drag_is_undone_back_to_where_it_began() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    assert!(!arranging(&mut app).can_undo());
    assert!(!arranging(&mut app).unsaved());
    drag_hale(&mut app);
    // Ava, at (140, -40), gripped at her middle and dragged twice.
    point(&mut app, 140.0, -40.0, true);
    point(&mut app, 150.0, -40.0, false);
    point(&mut app, 150.0, -40.0, true);
    point(&mut app, 160.0, -40.0, false);
    assert!(arranging(&mut app).can_undo());
    assert!(arranging(&mut app).unsaved());
    arranging(&mut app).ask_undo();
    app.update();
    assert_eq!(at(&mut app, "merchant_ava"), Vec2::new(150.0, -40.0));
    arranging(&mut app).ask_undo();
    app.update();
    assert_eq!(at(&mut app, "merchant_ava"), Vec2::new(140.0, -40.0));
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(100.0, 10.0));
    arranging(&mut app).ask_undo();
    app.update();
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(-180.0, 90.0));
    assert!(!arranging(&mut app).can_undo());
    assert!(
        !arranging(&mut app).unsaved(),
        "everyone where they were placed"
    );
}

const WRITTEN: &str = "# Where they stand.

[positions]
player = { x = 0.0, y = 0.0 }
captain_hale = { x = -180.0, y = 90.0 }
# Ava keeps her stall.
merchant_ava = { x = 140.0, y = -40.0 }
ghost = { x = 1.0, y = 2.0 }

# How near is near.
[play]
reach = 80.0
";

#[test]
fn saving_changes_positions_in_place_adds_the_rest_and_keeps_everything_else() {
    let positions = [
        (id("captain_hale"), Vec2::new(100.4, 10.6)),
        (id("merchant_ava"), Vec2::new(140.0, -40.0)),
        (id("newcomer"), Vec2::new(-12.5, 3.49)),
        (id("player"), Vec2::new(0.0, 0.0)),
        (id("another"), Vec2::new(7.0, -7.0)),
    ];
    assert_eq!(
        write_positions(Some(WRITTEN), &positions),
        Ok("# Where they stand.

[positions]
player = { x = 0.0, y = 0.0 }
captain_hale = { x = 100.0, y = 11.0 }
# Ava keeps her stall.
merchant_ava = { x = 140.0, y = -40.0 }
ghost = { x = 1.0, y = 2.0 }
newcomer = { x = -13.0, y = 3.0 }
another = { x = 7.0, y = -7.0 }

# How near is near.
[play]
reach = 80.0
"
        .to_owned())
    );
}

#[test]
fn saving_with_no_file_or_no_positions_starts_them() {
    let positions = [(id("player"), Vec2::new(0.0, -0.4))];
    assert_eq!(
        write_positions(None, &positions),
        Ok("[positions]\nplayer = { x = 0.0, y = 0.0 }\n".to_owned())
    );
    assert_eq!(
        write_positions(Some("[play]\nreach = 50.0\n"), &positions),
        Ok("[play]\nreach = 50.0\n\n[positions]\nplayer = { x = 0.0, y = 0.0 }\n".to_owned())
    );
}

#[test]
fn a_file_that_cant_be_read_isnt_overwritten() {
    for broken in [
        "[positions\n",
        "[positions]\nplayer = { x = 0.0 }\n",
        "[layout]\n",
    ] {
        let said = write_positions(Some(broken), &[]).expect_err("refused");
        assert!(
            said.starts_with("builder.toml can't be read, so the layout isn't saved: "),
            "{said}"
        );
    }
}

#[test]
fn save_layout_writes_builder_toml_and_reading_it_places_them_there() {
    let dir = copy(
        "arrange_saved",
        Some(&fs::read_to_string(sample().join("builder.toml")).expect("the sample's")),
    );
    let mut app = app(dir.clone());
    arranging(&mut app).on = true;
    drag_hale(&mut app);
    arranging(&mut app).ask_save();
    app.update();
    assert!(!arranging(&mut app).unsaved());
    assert_eq!(arranging(&mut app).note(), None);
    let text = fs::read_to_string(dir.join("builder.toml")).expect("saved");
    assert!(
        text.contains("captain_hale = { x = 100.0, y = 10.0 }"),
        "{text}"
    );
    assert!(text.contains("[play]\nreach = 80.0"), "{text}");
    let everyone = [
        "brother_ash",
        "captain_hale",
        "merchant_ava",
        "player",
        "sister_mira",
        "vex",
    ]
    .map(id);
    let placed = place(Some(&text), &everyone);
    assert_eq!(
        placed.positions[1],
        (id("captain_hale"), Vec2::new(100.0, 10.0))
    );
    assert_eq!(placed.positions[3], (id("player"), Vec2::ZERO));
    assert!(placed.problems.is_empty());
}

#[test]
fn with_no_file_saving_starts_one_with_everyone() {
    let dir = copy("arrange_started", None);
    let mut app = app(dir.clone());
    arranging(&mut app).on = true;
    arranging(&mut app).ask_save();
    app.update();
    let text = fs::read_to_string(dir.join("builder.toml")).expect("started");
    let ids: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_once(" = {").map(|(id, _)| id))
        .collect();
    assert_eq!(
        ids,
        [
            "brother_ash",
            "captain_hale",
            "merchant_ava",
            "player",
            "sister_mira",
            "vex",
        ],
        "in the world's order"
    );
}

#[test]
fn a_layout_that_cant_be_saved_is_said_and_the_file_kept() {
    let dir = copy("arrange_refused", Some("[positions\n"));
    let mut app = app(dir.clone());
    arranging(&mut app).on = true;
    // Laid out on the ring, Ash first, at the top: (0, 240).
    point(&mut app, 0.0, 240.0, true);
    point(&mut app, 0.0, 200.0, false);
    arranging(&mut app).ask_save();
    app.update();
    let note = arranging(&mut app).note().map(str::to_owned).expect("said");
    assert!(
        note.starts_with("builder.toml can't be read, so the layout isn't saved: "),
        "{note}"
    );
    assert!(arranging(&mut app).unsaved());
    assert_eq!(
        fs::read_to_string(dir.join("builder.toml")).expect("kept"),
        "[positions\n"
    );
}

#[test]
fn arrange_turns_the_mode_on_with_undo_and_save_disabled_until_theres_something() {
    let mut harness = Harness::new_ui_state(
        |ui, arranging: &mut Arranging| arrange_ui(ui, arranging),
        Arranging::default(),
    );
    harness.run();
    assert!(harness.query_by_label("Undo").is_none(), "not arranging");
    harness.get_by_label("Arrange").click();
    harness.run();
    assert!(harness.state().on);
    assert!(harness.get_by_label("Undo").accesskit_node().is_disabled());
    assert!(
        harness
            .get_by_label("Save layout")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("Arrange").click();
    harness.run();
    assert!(!harness.state().on);
}

/// The pointer over a window for a frame, pressed or not: not on the plane.
fn off_plane(app: &mut App, down: bool) {
    *app.world_mut().resource_mut::<Pointer>() = Pointer { at: None, down };
    app.update();
}

#[test]
fn a_drag_carries_on_over_a_window_and_let_go_there_stays_where_it_was_dragged() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    point(&mut app, -175.0, 90.0, true);
    point(&mut app, 105.0, 10.0, true);
    off_plane(&mut app, true);
    assert_eq!(
        arranging(&mut app).held(),
        Some(&id("captain_hale")),
        "still held"
    );
    off_plane(&mut app, false);
    assert_eq!(arranging(&mut app).held(), None);
    assert_eq!(at(&mut app, "captain_hale"), Vec2::new(100.0, 10.0));
    assert_eq!(at(&mut app, "merchant_ava"), Vec2::new(140.0, -40.0));
    assert!(arranging(&mut app).can_undo());
}

#[test]
fn turning_arrange_off_drops_what_is_held() {
    let mut app = app(sample());
    arranging(&mut app).on = true;
    point(&mut app, -175.0, 90.0, true);
    arranging(&mut app).on = false;
    point(&mut app, -175.0, 90.0, true);
    assert_eq!(arranging(&mut app).held(), None);
    arranging(&mut app).on = true;
    point(&mut app, 105.0, 10.0, true);
    assert_eq!(
        at(&mut app, "captain_hale"),
        Vec2::new(-180.0, 90.0),
        "not picked up again without a press"
    );
}

//! Panning and zooming the plane (W7a): the view starts on the played character, Home brings
//! it back, a drag with the right or middle button pans, and the wheel zooms about the cursor.
//! Expected values worked by hand from PLAN.md's acceptance.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::{ButtonState, InputPlugin};
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use factional_bevy::FactionalPlugin;
use factional_builder::{
    BuilderPlugin, FARTHEST, KeyboardTaken, NEAREST, PIXELS_A_LINE, Pointer, View, ZOOM_STEP,
};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).length() < 1e-3
}

fn near_f(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_wheel_zooms_by_a_tenth_a_line_between_an_eighth_and_64_points_a_pixel() {
    assert_eq!(ZOOM_STEP, 1.1);
    assert_eq!(PIXELS_A_LINE, 20.0);
    assert_eq!((NEAREST, FARTHEST), (0.125, 64.0));
    assert_eq!(
        View::default(),
        View {
            at: Vec2::ZERO,
            scale: 1.0
        }
    );
}

#[test]
fn panning_moves_the_plane_with_the_pointer() {
    // 5 pixels right and 3 down, at 2 points a pixel: the view moves 10 left and 6 up.
    let view = View {
        at: Vec2::new(10.0, 20.0),
        scale: 2.0,
    };
    assert_eq!(
        view.panned(Vec2::new(5.0, 3.0)),
        View {
            at: Vec2::new(0.0, 26.0),
            scale: 2.0
        }
    );
}

#[test]
fn zooming_keeps_the_point_under_the_cursor_where_it_is() {
    let view = View::default();
    let about = Vec2::new(100.0, 50.0);
    // In a line: 1 / 1.1 points a pixel; the view moves toward the cursor by 1 − 1 / 1.1.
    let zoomed = view.zoomed(about, 1.0);
    assert!(near_f(zoomed.scale, 1.0 / 1.1), "{zoomed:?}");
    assert!(near(zoomed.at, about - about / 1.1), "{zoomed:?}");
    // Out two lines: 1.21 points a pixel; the view moves away by 0.21 of the way.
    let zoomed = view.zoomed(about, -2.0);
    assert!(near_f(zoomed.scale, 1.21), "{zoomed:?}");
    assert!(near(zoomed.at, about - about * 1.21), "{zoomed:?}");
    // The cursor's point is where it was on the screen: (point − view) / scale is unchanged.
    let start = View {
        at: Vec2::new(-30.0, 12.0),
        scale: 3.0,
    };
    let zoomed = start.zoomed(about, 1.5);
    assert!(near(
        (about - zoomed.at) / zoomed.scale,
        (about - start.at) / start.scale
    ));
}

#[test]
fn zoom_stops_at_the_nearest_and_the_farthest_keeping_the_cursors_point() {
    let about = Vec2::new(40.0, -8.0);
    let close = View {
        at: Vec2::ZERO,
        scale: 0.13,
    }
    .zoomed(about, 5.0);
    assert_eq!(close.scale, NEAREST);
    assert!(near(close.at, about - about * (0.125 / 0.13)), "{close:?}");
    let far = View {
        at: Vec2::ZERO,
        scale: 60.0,
    }
    .zoomed(about, -5.0);
    assert_eq!(far.scale, FARTHEST);
    assert!(near(far.at, about - about * (64.0 / 60.0)), "{far:?}");
}

/// The builder on the sample, playing `player`, with input, a second a frame.
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
    app.update();
    app
}

fn view(app: &App) -> View {
    *app.world().resource::<View>()
}

fn set_view(app: &mut App, at: Vec2, scale: f32) {
    *app.world_mut().resource_mut::<View>() = View { at, scale };
}

fn point_at(app: &mut App, at: Option<Vec2>) {
    *app.world_mut().resource_mut::<Pointer>() = Pointer { at, down: false };
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

fn press_home(app: &mut App) {
    key(app, KeyCode::Home, ButtonState::Pressed);
    app.update();
    key(app, KeyCode::Home, ButtonState::Released);
    app.update();
}

fn button(app: &mut App, button: MouseButton, state: ButtonState) {
    app.world_mut().write_message(MouseButtonInput {
        button,
        state,
        window: Entity::PLACEHOLDER,
    });
}

fn moved(app: &mut App, delta: Vec2) {
    app.world_mut().write_message(MouseMotion { delta });
}

fn wheel(app: &mut App, unit: MouseScrollUnit, y: f32) {
    app.world_mut().write_message(MouseWheel {
        unit,
        x: 0.0,
        y,
        window: Entity::PLACEHOLDER,
        phase: TouchPhase::Moved,
    });
}

#[test]
fn the_view_starts_on_the_played_character_at_a_point_a_pixel() {
    assert_eq!(view(&app("player")), View::default());
    assert_eq!(
        view(&app("captain_hale")),
        View {
            at: Vec2::new(-180.0, 90.0),
            scale: 1.0
        }
    );
    assert_eq!(view(&app("nobody")), View::default());
}

#[test]
fn home_brings_the_view_back_to_the_played_character_unless_egui_has_the_keyboard() {
    let mut app = app("vex");
    set_view(&mut app, Vec2::new(500.0, 500.0), 4.0);
    app.world_mut().resource_mut::<KeyboardTaken>().0 = true;
    press_home(&mut app);
    assert_eq!(view(&app).at, Vec2::new(500.0, 500.0));
    app.world_mut().resource_mut::<KeyboardTaken>().0 = false;
    press_home(&mut app);
    assert_eq!(
        view(&app),
        View {
            at: Vec2::new(-140.0, -130.0),
            scale: 4.0
        }
    );
}

#[test]
fn a_drag_with_the_right_or_middle_button_pans_the_left_doesnt() {
    for (pressed, pans) in [
        (MouseButton::Right, true),
        (MouseButton::Middle, true),
        (MouseButton::Left, false),
    ] {
        let mut app = app("player");
        set_view(&mut app, Vec2::ZERO, 2.0);
        point_at(&mut app, Some(Vec2::new(10.0, 10.0)));
        button(&mut app, pressed, ButtonState::Pressed);
        app.update();
        moved(&mut app, Vec2::new(5.0, 3.0));
        app.update();
        let expected = if pans {
            Vec2::new(-10.0, 6.0)
        } else {
            Vec2::ZERO
        };
        assert_eq!(view(&app).at, expected, "{pressed:?}");
        button(&mut app, pressed, ButtonState::Released);
        app.update();
        moved(&mut app, Vec2::new(5.0, 3.0));
        app.update();
        assert_eq!(view(&app).at, expected, "{pressed:?}, let go");
    }
}

#[test]
fn a_drag_begun_over_eguis_windows_doesnt_pan() {
    let mut app = app("player");
    point_at(&mut app, None);
    button(&mut app, MouseButton::Right, ButtonState::Pressed);
    app.update();
    point_at(&mut app, Some(Vec2::ZERO));
    moved(&mut app, Vec2::new(5.0, 3.0));
    app.update();
    assert_eq!(view(&app).at, Vec2::ZERO);
}

#[test]
fn a_pan_carries_on_over_eguis_windows_once_begun() {
    let mut app = app("player");
    point_at(&mut app, Some(Vec2::ZERO));
    button(&mut app, MouseButton::Right, ButtonState::Pressed);
    app.update();
    point_at(&mut app, None);
    moved(&mut app, Vec2::new(5.0, 3.0));
    app.update();
    assert_eq!(view(&app).at, Vec2::new(-5.0, 3.0));
}

#[test]
fn the_wheel_zooms_about_the_cursor_a_line_or_20_pixels_at_a_time() {
    let about = Vec2::new(100.0, 50.0);
    for (unit, y) in [(MouseScrollUnit::Line, 1.0), (MouseScrollUnit::Pixel, 20.0)] {
        let mut app = app("player");
        point_at(&mut app, Some(about));
        wheel(&mut app, unit, y);
        app.update();
        let zoomed = view(&app);
        assert!(near_f(zoomed.scale, 1.0 / 1.1), "{unit:?}: {zoomed:?}");
        assert!(near(zoomed.at, about - about / 1.1), "{unit:?}: {zoomed:?}");
    }
}

#[test]
fn the_wheel_over_eguis_windows_doesnt_zoom() {
    let mut app = app("player");
    point_at(&mut app, None);
    wheel(&mut app, MouseScrollUnit::Line, 1.0);
    app.update();
    assert_eq!(view(&app), View::default());
}

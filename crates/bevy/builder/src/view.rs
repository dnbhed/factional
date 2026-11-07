//! Panning and zooming the plane (W7a, P-150): the view starts on the played character and
//! Home brings it back; a drag with the right or middle button pans; the wheel zooms about the
//! cursor. The app points the camera where the view says.

use bevy::ecs::query::{Added, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseButton, MouseScrollUnit,
};
use bevy::math::Vec2;
use bevy::transform::components::Transform;

use crate::{KeyboardTaken, Played, Pointer};

/// How much a line of the wheel zooms.
pub const ZOOM_STEP: f32 = 1.1;

/// How many pixels of a touchpad's scrolling make a line.
pub const PIXELS_A_LINE: f32 = 20.0;

/// The nearest and farthest the view zooms, in points a pixel.
pub const NEAREST: f32 = 0.125;
pub const FARTHEST: f32 = 64.0;

/// Where the view is on the plane, and how many points a pixel.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub at: Vec2,
    pub scale: f32,
}

impl Default for View {
    fn default() -> Self {
        View {
            at: Vec2::ZERO,
            scale: 1.0,
        }
    }
}

impl View {
    /// The view after the pointer moved by `moved` pixels, right and down, while panning: the
    /// plane follows the pointer.
    pub fn panned(self, moved: Vec2) -> View {
        View {
            at: self.at + Vec2::new(-moved.x, moved.y) * self.scale,
            ..self
        }
    }

    /// The view after the wheel turned `lines` lines, up zooming in, with the cursor at
    /// `about` on the plane, which stays put.
    pub fn zoomed(self, about: Vec2, lines: f32) -> View {
        let scale = (self.scale * ZOOM_STEP.powf(-lines)).clamp(NEAREST, FARTHEST);
        View {
            at: about - (about - self.at) * (scale / self.scale),
            scale,
        }
    }
}

/// Whether a pan, begun off egui's windows, is under way.
#[derive(Resource, Debug, Default)]
pub(crate) struct Panning(bool);

/// The view starts on the played character, once they stand on the plane.
pub(crate) fn start_on_played(played: Query<&Transform, Added<Played>>, mut view: ResMut<View>) {
    if let Some(at) = played.iter().next() {
        view.at = at.translation.truncate();
    }
}

/// Home brings the view back to the played character, unless egui has the keyboard.
pub(crate) fn home(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    taken: Res<KeyboardTaken>,
    played: Query<&Transform, With<Played>>,
    mut view: ResMut<View>,
) {
    let pressed = keys.is_some_and(|keys| keys.just_pressed(KeyCode::Home));
    if !pressed || taken.0 {
        return;
    }
    if let Some(at) = played.iter().next() {
        view.at = at.translation.truncate();
    }
}

/// A drag with the right or middle button pans, and the wheel zooms about the cursor; each
/// only begun off egui's windows.
pub(crate) fn pan_and_zoom(
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    pointer: Res<Pointer>,
    mut panning: ResMut<Panning>,
    mut view: ResMut<View>,
) {
    if let Some(buttons) = buttons {
        let panners = [MouseButton::Right, MouseButton::Middle];
        if buttons.any_just_pressed(panners) && pointer.at.is_some() {
            panning.0 = true;
        }
        if !buttons.any_pressed(panners) {
            panning.0 = false;
        }
    }
    if let Some(motion) = motion
        && panning.0
        && motion.delta != Vec2::ZERO
    {
        *view = view.panned(motion.delta);
    }
    if let (Some(scroll), Some(about)) = (scroll, pointer.at) {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_A_LINE,
        };
        if lines != 0.0 {
            *view = view.zoomed(about, lines);
        }
    }
}

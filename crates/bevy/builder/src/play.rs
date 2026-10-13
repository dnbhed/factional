//! Playing a character (G5, P-126): walking them among the tokens, and acting on the nearest,
//! those near enough seeing it. Who's near is the builder's rule, sent as `Witnesses` in each
//! command; the module learns nothing about space (P-118).

use std::collections::BTreeSet;

use bevy::ecs::component::Component;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::math::Vec2;
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy_egui::egui;
use factional_bevy::FactionalCommand;
use factional_core::Fixed;
use factional_reputation::{ActionId, CharacterId, Command, Witnesses};

use crate::Token;

/// How near the nearest must be to be acted on, by default, in points.
pub const REACH: f32 = 80.0;
/// How near a character must be to see an act, by default, in points.
pub const SIGHT: f32 = 240.0;
/// How fast the played character walks, by default, in points a second of game time.
pub const SPEED: f32 = 200.0;

/// How near is near, from `builder.toml`'s `[play]`: in points, and points a second.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Play {
    pub reach: f32,
    pub sight: f32,
    pub speed: f32,
}

impl Default for Play {
    fn default() -> Self {
        Play {
            reach: REACH,
            sight: SIGHT,
            speed: SPEED,
        }
    }
}

/// The played character's token.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played;

/// Whether something else has the keyboard, such as the inspector's console, so the keys
/// don't walk.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardTaken(pub bool);

/// Who's near the played character: the nearest within reach, and those who'd see.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Nearby {
    pub nearest: Option<CharacterId>,
    pub seen_by: BTreeSet<CharacterId>,
}

/// Who's near `at`, of `others`, by `play`'s reach and sight: the closest within reach, the
/// first by id of any as close; and every one within sight.
pub fn nearby(at: Vec2, others: &[(CharacterId, Vec2)], play: &Play) -> Nearby {
    let away = |there: &Vec2| at.distance(*there);
    let nearest = others
        .iter()
        .filter(|(_, there)| away(there) <= play.reach)
        .min_by(|(one, here), (other, there)| {
            away(here)
                .total_cmp(&away(there))
                .then_with(|| one.cmp(other))
        })
        .map(|(character, _)| character.clone());
    let seen_by = others
        .iter()
        .filter(|(_, there)| away(there) <= play.sight)
        .map(|(character, _)| character.clone())
        .collect();
    Nearby { nearest, seen_by }
}

/// `played` doing `action`, on whoever's nearest, at scale 1, seen by exactly those near.
pub fn act(played: &CharacterId, action: ActionId, nearby: &Nearby) -> FactionalCommand {
    let witnesses = match nearby.seen_by.is_empty() {
        true => Witnesses::Nobody,
        false => Witnesses::These(nearby.seen_by.clone()),
    };
    FactionalCommand::World(Command::PerformAction {
        actor: played.clone(),
        action,
        target: nearby.nearest.clone(),
        scale: Fixed::ONE,
        witnesses,
    })
}

/// The content's actions to choose from, in the order given, saying who they'd be done to and
/// who'd see. The one chosen, if any.
pub fn act_ui(ui: &mut egui::Ui, actions: &[ActionId], nearby: &Nearby) -> Option<ActionId> {
    let on = nearby
        .nearest
        .as_ref()
        .map_or("no one", CharacterId::as_str);
    let seen: Vec<&str> = nearby.seen_by.iter().map(CharacterId::as_str).collect();
    let seen = match seen.is_empty() {
        true => "no one".to_owned(),
        false => seen.join(", "),
    };
    ui.label(format!("On {on}, seen by {seen}"));
    let mut chosen = None;
    for action in actions {
        if ui.button(action.as_str()).clicked() {
            chosen = Some(action.clone());
        }
    }
    chosen
}

/// The played token walked by the arrow keys or WASD, at `play`'s speed in game time; not
/// while something else has the keyboard.
pub(crate) fn walk(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    time: Res<Time>,
    play: Res<Play>,
    taken: Res<KeyboardTaken>,
    mut played: Query<&mut Transform, With<Played>>,
) {
    let Some(keys) = keys.filter(|_| !taken.0) else {
        return;
    };
    let ways = [
        ([KeyCode::ArrowRight, KeyCode::KeyD], Vec2::X),
        ([KeyCode::ArrowLeft, KeyCode::KeyA], Vec2::NEG_X),
        ([KeyCode::ArrowUp, KeyCode::KeyW], Vec2::Y),
        ([KeyCode::ArrowDown, KeyCode::KeyS], Vec2::NEG_Y),
    ];
    let way: Vec2 = ways
        .into_iter()
        .filter(|(codes, _)| keys.any_pressed(*codes))
        .map(|(_, way)| way)
        .sum();
    let step = way.normalize_or_zero() * play.speed * time.delta_secs();
    for mut at in &mut played {
        at.translation += step.extend(0.0);
    }
}

/// Who's near the played token now.
pub(crate) fn find_nearby(
    play: Res<Play>,
    tokens: Query<(&Token, &Transform, Option<&Played>)>,
    mut near: ResMut<Nearby>,
) {
    let mut at = None;
    let mut others = Vec::new();
    for (token, there, played) in &tokens {
        let there = there.translation.truncate();
        match played {
            Some(_) => at = Some(there),
            None => others.push((token.character.clone(), there)),
        }
    }
    others.sort_by(|a, b| a.0.cmp(&b.0));
    *near = at.map_or_else(Nearby::default, |at| nearby(at, &others, &play));
}

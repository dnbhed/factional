//! The map's suggestions in the builder (W7b, P-151, DESIGN.md §21): each settlement, route and
//! point of interest not dismissed, a placeholder shape by kind with its name; and the one
//! under the pointer, said as `world suggestions` says it.

use std::collections::BTreeMap;

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::math::Vec2;
use factional_session::suggestion_lines;
use factional_worldgen::{Placed, RouteKind};

use crate::{Ground, Pointer, View};

/// How many pixels around a suggestion the pointer is still over it.
pub const SLACK: f32 = 6.0;

/// What a suggestion is drawn as, in points on the plane.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A settlement: a square `side` points a side at `at`, ringed if it's a capital.
    Square { at: Vec2, side: f32, ringed: bool },
    /// A point of interest: a diamond `side` points across at `at`.
    Diamond { at: Vec2, side: f32 },
    /// A route: a line through `points`, `width` points wide.
    Line {
        points: Vec<Vec2>,
        width: f32,
        kind: RouteKind,
    },
}

/// A suggestion drawn on the map: its id, its label and where the label's middle is, and its
/// shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub id: String,
    pub label: String,
    pub label_at: Vec2,
    pub shape: Shape,
}

/// Where `place` lies on the plane, on `ground`: its tile and its offset in 256ths, from the
/// map's middle, y up.
pub fn place_of(ground: &Ground, place: Placed) -> Vec2 {
    let size = ground.map.size();
    let across = |tile: u32, offset: u8, tiles: u32| {
        tile as f32 + f32::from(offset) / 256.0 - tiles as f32 / 2.0
    };
    Vec2::new(
        across(place.tile.x, place.offset[0], size.width),
        -across(place.tile.y, place.offset[1], size.height),
    ) * ground.tile
}

/// A settlement's square, in tiles a side: ½ under 1000 people, ¾ under 10 000, then 1.
fn settlement_side(people: u64) -> f32 {
    match people {
        ..1_000 => 0.5,
        1_000..10_000 => 0.75,
        _ => 1.0,
    }
}

/// Each settlement, route and point of interest on `ground`'s map not dismissed, in that order,
/// each in the suggestions' order.
pub fn marks(ground: &Ground) -> Vec<Mark> {
    let suggestions = ground.map.suggestions();
    let shown = |id: &str| ground.map.dismissed_by(id).is_none();
    let tile = ground.tile;
    // A marker's label is centred a quarter tile below it.
    let below = |at: Vec2, side: f32| at - Vec2::new(0.0, side / 2.0 + tile / 4.0);
    let mut marks = Vec::new();
    for settlement in suggestions.settlements.iter().filter(|s| shown(&s.id)) {
        let at = place_of(ground, settlement.at);
        let side = settlement_side(settlement.people) * tile;
        marks.push(Mark {
            id: settlement.id.clone(),
            label: settlement.name.clone(),
            label_at: below(at, side),
            shape: Shape::Square {
                at,
                side,
                ringed: settlement.capital,
            },
        });
    }
    for route in suggestions.routes.iter().filter(|r| shown(&r.id)) {
        let points: Vec<Vec2> = route.points.iter().map(|&p| place_of(ground, p)).collect();
        let (kind_name, width) = match route.kind {
            RouteKind::Road => ("road", 0.25),
            RouteKind::Trail => ("trail", 0.125),
            RouteKind::SeaRoute => ("sea route", 0.25),
        };
        // The middle of its middle stretch, from point ⌊(n − 1) / 2⌋.
        let middle = (points.len() - 1) / 2;
        let label_at = (points[middle] + points[middle + 1]) / 2.0;
        marks.push(Mark {
            id: route.id.clone(),
            label: route.name.clone().unwrap_or_else(|| kind_name.to_owned()),
            label_at,
            shape: Shape::Line {
                points,
                width: width * tile,
                kind: route.kind,
            },
        });
    }
    for point in suggestions.points.iter().filter(|p| shown(&p.id)) {
        let at = place_of(ground, point.at);
        let side = 0.75 * tile;
        marks.push(Mark {
            id: point.id.clone(),
            label: point.name.clone().unwrap_or_else(|| point.kind.clone()),
            label_at: below(at, side),
            shape: Shape::Diamond { at, side },
        });
    }
    marks
}

/// How far `at` is from the segment `from` to `to`.
fn off_segment(at: Vec2, from: Vec2, to: Vec2) -> f32 {
    let along = to - from;
    let part = match along.length_squared() {
        0.0 => 0.0,
        length => ((at - from).dot(along) / length).clamp(0.0, 1.0),
    };
    at.distance(from + along * part)
}

/// Whether the pointer at `at` is over `shape` with `slack` points to spare, and if so how near
/// it is: to a marker's middle, or to a route's nearest stretch.
fn reach(shape: &Shape, at: Vec2, slack: f32) -> Option<f32> {
    match shape {
        Shape::Square {
            at: middle, side, ..
        } => {
            let off = (at - *middle).abs();
            (off.max_element() <= side / 2.0 + slack).then(|| at.distance(*middle))
        }
        Shape::Diamond { at: middle, side } => {
            let off = (at - *middle).abs();
            (off.x + off.y <= side / 2.0 + slack).then(|| at.distance(*middle))
        }
        Shape::Line { points, width, .. } => {
            let nearest = points
                .windows(2)
                .map(|stretch| off_segment(at, stretch[0], stretch[1]))
                .fold(f32::INFINITY, f32::min);
            (nearest <= width / 2.0 + slack).then_some(nearest)
        }
    }
}

/// The mark the pointer at `at` is over, at `scale` points a pixel: a settlement or point of
/// interest before a route, then the nearest, then the first.
pub fn hovered(marks: &[Mark], at: Vec2, scale: f32) -> Option<&Mark> {
    let slack = SLACK * scale;
    let mut best: Option<(bool, f32, &Mark)> = None;
    for mark in marks {
        let Some(near) = reach(&mark.shape, at, slack) else {
            continue;
        };
        let route = matches!(mark.shape, Shape::Line { .. });
        let better = match best {
            None => true,
            Some((best_route, best_near, _)) => (route, near) < (best_route, best_near),
        };
        if better {
            best = Some((route, near, mark));
        }
    }
    best.map(|(_, _, mark)| mark)
}

/// The map's suggestions drawn, and what `world suggestions` says of each, by id.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Suggested {
    pub marks: Vec<Mark>,
    pub lines: BTreeMap<String, String>,
}

/// The suggestion the pointer is over, if any, by id, and what's said of it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Hovered(pub Option<(String, String)>);

/// The map's suggestions, once it's read.
pub(crate) fn suggest(mut commands: Commands, ground: Option<Res<Ground>>) {
    let Some(ground) = ground.filter(|ground| ground.is_added()) else {
        return;
    };
    commands.insert_resource(Suggested {
        marks: marks(&ground),
        lines: suggestion_lines(&ground.map).into_iter().collect(),
    });
}

/// Which suggestion the pointer is over.
pub(crate) fn hover(
    suggested: Option<Res<Suggested>>,
    pointer: Res<Pointer>,
    view: Res<View>,
    mut hovered: ResMut<Hovered>,
) {
    let over = suggested.as_ref().and_then(|suggested| {
        let mark = self::hovered(&suggested.marks, pointer.at?, view.scale)?;
        let line = suggested.lines.get(&mark.id)?;
        Some((mark.id.clone(), line.clone()))
    });
    if hovered.0 != over {
        hovered.0 = over;
    }
}

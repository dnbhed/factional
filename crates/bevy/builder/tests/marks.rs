//! The map's suggestions in the builder (W7b): settlements, routes and points of interest in
//! placeholder shapes by kind, named, dismissed ones hidden, and the one under the pointer
//! said. Every place is worked by hand from the raster fixture's source pixels: on the sample,
//! 80 × 80 tiles at 32 points, a place at pixel (p, q) lies at ((2p − 40) × 32, (40 − 2q) × 32),
//! rounded to 256ths of a tile.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use factional_bevy::FactionalPlugin;
use factional_builder::{
    BuilderPlugin, Ground, Hovered, MapChoice, Mark, Pointer, SLACK, Shape, Suggested, View,
    hovered, marks, place_of, read_ground,
};
use factional_worldgen::{Placed, RouteKind, TilePos};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

/// The sample's map, as the builder reads it.
fn ground() -> Ground {
    let choice = MapChoice {
        file: Some("world/riverhold.map".to_owned()),
        tile: 32.0,
    };
    read_ground(&sample(), &choice)
        .expect("read")
        .expect("a map")
}

fn find<'a>(marks: &'a [Mark], id: &str) -> &'a Mark {
    marks
        .iter()
        .find(|mark| mark.id == id)
        .unwrap_or_else(|| panic!("{id} drawn"))
}

#[test]
fn a_place_lies_at_its_tile_and_offset_from_the_maps_middle() {
    let ground = ground();
    let place = |x, y, a, b| {
        place_of(
            &ground,
            Placed {
                tile: TilePos { x, y },
                offset: [a, b],
            },
        )
    };
    assert_eq!(place(40, 40, 0, 0), Vec2::ZERO);
    assert_eq!(place(0, 0, 0, 0), Vec2::new(-1280.0, 1280.0));
    assert_eq!(place(60, 19, 128, 128), Vec2::new(656.0, 656.0));
    assert_eq!(place(66, 68, 154, 205), Vec2::new(851.25, -921.625));
    let half = Ground {
        tile: 16.0,
        ..ground.clone()
    };
    assert_eq!(
        place_of(
            &half,
            Placed {
                tile: TilePos { x: 60, y: 19 },
                offset: [128, 128]
            }
        ),
        Vec2::new(328.0, 328.0)
    );
}

#[test]
fn settlements_are_squares_sized_by_their_people_a_capital_ringed() {
    let marks = marks(&ground());
    let expected = [
        ("settlement_1", "Highmoor", 656.0, 656.0, 24.0, true),
        ("settlement_2", "Lakeside", -352.0, -432.0, 16.0, false),
        ("settlement_4", "Pinewatch", 736.0, -704.0, 32.0, false),
    ];
    for (id, name, x, y, side, ringed) in expected {
        let mark = find(&marks, id);
        assert_eq!(mark.label, name);
        assert_eq!(
            mark.shape,
            Shape::Square {
                at: Vec2::new(x, y),
                side,
                ringed
            },
            "{id}"
        );
        // Centred a quarter tile below the square.
        assert_eq!(mark.label_at, Vec2::new(x, y - side / 2.0 - 8.0), "{id}");
    }
}

#[test]
fn points_of_interest_are_diamonds_three_quarters_of_a_tile_across() {
    let marks = marks(&ground());
    let peak = find(&marks, "poi_0");
    assert_eq!(peak.label, "Ash Peak");
    assert_eq!(
        peak.shape,
        Shape::Diamond {
            at: Vec2::new(851.25, -921.625),
            side: 24.0
        }
    );
    assert_eq!(peak.label_at, Vec2::new(851.25, -921.625 - 12.0 - 8.0));
    let keep = find(&marks, "poi_1");
    assert_eq!(keep.label, "Old Keep");
    assert_eq!(
        keep.shape,
        Shape::Diamond {
            at: Vec2::new(0.0, -64.0),
            side: 24.0
        }
    );
}

#[test]
fn routes_are_lines_through_their_points_labelled_mid_way() {
    let marks = marks(&ground());
    let points = |list: &[(f32, f32)]| list.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let high = find(&marks, "route_0");
    assert_eq!(high.label, "High road");
    assert_eq!(
        high.shape,
        Shape::Line {
            points: points(&[
                (656.0, 656.0),
                (352.0, 288.0),
                (32.0, -32.0),
                (736.0, -704.0)
            ]),
            width: 8.0,
            kind: RouteKind::Road,
        }
    );
    assert_eq!(high.label_at, Vec2::new(192.0, 128.0));
    let trail = find(&marks, "route_1");
    assert_eq!(trail.label, "trail");
    assert_eq!(
        trail.shape,
        Shape::Line {
            points: points(&[(-352.0, -432.0), (32.0, -32.0)]),
            width: 4.0,
            kind: RouteKind::Trail,
        }
    );
    assert_eq!(trail.label_at, Vec2::new(-160.0, -232.0));
    let coast = find(&marks, "route_2");
    assert_eq!(coast.label, "Coast road");
    assert_eq!(coast.label_at, Vec2::new(640.0, 0.0));
}

#[test]
fn every_suggestion_with_a_place_is_drawn_settlements_routes_then_points() {
    let ids: Vec<String> = marks(&ground()).into_iter().map(|mark| mark.id).collect();
    assert_eq!(
        ids,
        [
            "settlement_1",
            "settlement_2",
            "settlement_4",
            "route_0",
            "route_1",
            "route_2",
            "poi_0",
            "poi_1"
        ]
    );
}

#[test]
fn dismissed_suggestions_arent_drawn() {
    let mut ground = ground();
    ground
        .map
        .dismiss(&["settlement_2", "route_0"])
        .expect("dismissed");
    let ids: Vec<String> = marks(&ground).into_iter().map(|mark| mark.id).collect();
    assert_eq!(
        ids,
        [
            "settlement_1",
            "settlement_4",
            "route_1",
            "route_2",
            "poi_0",
            "poi_1"
        ]
    );
}

fn over(marks: &[Mark], x: f32, y: f32, scale: f32) -> Option<&str> {
    hovered(marks, Vec2::new(x, y), scale).map(|mark| mark.id.as_str())
}

#[test]
fn the_pointer_is_over_a_square_within_half_its_side_and_6_pixels() {
    let marks = marks(&ground());
    assert_eq!(SLACK, 6.0);
    // Highmoor: 24 points a side, at (656, 656); 12 + 6 each way at a point a pixel.
    assert_eq!(over(&marks, 673.0, 656.0, 1.0), Some("settlement_1"));
    assert_eq!(over(&marks, 656.0, 673.0, 1.0), Some("settlement_1"));
    assert_eq!(over(&marks, 673.0, 673.0, 1.0), Some("settlement_1"));
    assert_eq!(over(&marks, 675.0, 656.0, 1.0), None);
    // At 2 points a pixel, the slack is 12 points.
    assert_eq!(over(&marks, 679.0, 656.0, 2.0), Some("settlement_1"));
    assert_eq!(over(&marks, 681.0, 656.0, 2.0), None);
}

#[test]
fn the_pointer_is_over_a_diamond_within_half_its_width_in_x_and_y_together() {
    let marks = marks(&ground());
    // Old Keep: 24 across at (0, −64); |dx| + |dy| within 12 + 6. The trail passes just east
    // of it, so these points are to its west, 13 and 14 points off the trail.
    assert_eq!(over(&marks, -10.0, -57.0, 1.0), Some("poi_1"));
    assert_eq!(over(&marks, -10.0, -55.0, 1.0), None);
    // On the trail, inside the diamond: the diamond.
    assert_eq!(over(&marks, 10.0, -57.0, 1.0), Some("poi_1"));
}

#[test]
fn the_pointer_is_over_a_route_within_half_its_width_and_6_pixels() {
    let marks = marks(&ground());
    // The Coast road's first stretch, (−512, 512) to (960, 320), 8 wide: within 4 + 6.
    let (from, to) = (Vec2::new(-512.0, 512.0), Vec2::new(960.0, 320.0));
    let middle = (from + to) / 2.0;
    let across = (to - from).perp().normalize();
    let near = middle + across * 9.0;
    let far = middle + across * 11.0;
    assert_eq!(over(&marks, near.x, near.y, 1.0), Some("route_2"));
    assert_eq!(over(&marks, far.x, far.y, 1.0), None);
}

#[test]
fn a_marker_wins_over_a_route_then_the_nearest_then_the_first() {
    let marks = marks(&ground());
    // Lakeside is the trail's end.
    assert_eq!(over(&marks, -352.0, -432.0, 1.0), Some("settlement_2"));
    // The High road and the trail meet at (32, −32), each no distance off: the first.
    assert_eq!(over(&marks, 32.0, -32.0, 1.0), Some("route_0"));
    // Off both but near the High road's stretch toward Pinewatch only.
    let (from, to) = (Vec2::new(32.0, -32.0), Vec2::new(736.0, -704.0));
    let on = from + (to - from) * 0.5 + (to - from).perp().normalize() * 3.0;
    assert_eq!(over(&marks, on.x, on.y, 1.0), Some("route_0"));
    // Two markers within reach: the nearest.
    let squares = vec![
        Mark {
            id: "a".to_owned(),
            label: String::new(),
            label_at: Vec2::ZERO,
            shape: Shape::Square {
                at: Vec2::new(0.0, 0.0),
                side: 32.0,
                ringed: false,
            },
        },
        Mark {
            id: "b".to_owned(),
            label: String::new(),
            label_at: Vec2::ZERO,
            shape: Shape::Square {
                at: Vec2::new(20.0, 0.0),
                side: 32.0,
                ringed: false,
            },
        },
    ];
    assert_eq!(over(&squares, 12.0, 0.0, 1.0), Some("b"));
    assert_eq!(over(&squares, 8.0, 0.0, 1.0), Some("a"));
    assert_eq!(over(&squares, 10.0, 0.0, 1.0), Some("a"));
}

/// The builder on the sample, a second a frame.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: "player".to_owned(),
        });
    app.update();
    app.update();
    app
}

fn point_at(app: &mut App, at: Option<Vec2>) {
    *app.world_mut().resource_mut::<Pointer>() = Pointer { at, down: false };
    app.update();
}

#[test]
fn the_builder_draws_the_maps_suggestions_with_what_world_suggestions_says() {
    let app = app();
    let suggested = app.world().resource::<Suggested>();
    assert_eq!(suggested.marks, marks(&ground()));
    assert_eq!(
        suggested.lines.get("route_1").map(String::as_str),
        Some("route_1: trail, 2 points, tile (29, 53) to (41, 41)")
    );
}

#[test]
fn hovering_a_suggestion_says_it_as_world_suggestions_does() {
    let mut app = app();
    point_at(&mut app, Some(Vec2::new(656.0, 656.0)));
    assert_eq!(
        app.world().resource::<Hovered>().0,
        Some((
            "settlement_1".to_owned(),
            "settlement_1 Highmoor: generic, capital, 2345 people, at tile (60, 19); citadel, walls and temple"
                .to_owned()
        ))
    );
    // Zoomed out to 4 points a pixel, 30 points off is within the slack of 24.
    *app.world_mut().resource_mut::<View>() = View {
        at: Vec2::ZERO,
        scale: 4.0,
    };
    point_at(&mut app, Some(Vec2::new(686.0, 656.0)));
    assert_eq!(
        app.world()
            .resource::<Hovered>()
            .0
            .as_ref()
            .map(|(id, _)| id.as_str()),
        Some("settlement_1")
    );
    point_at(&mut app, Some(Vec2::new(-1000.0, -1000.0)));
    assert_eq!(app.world().resource::<Hovered>().0, None);
    point_at(&mut app, Some(Vec2::new(656.0, 656.0)));
    point_at(&mut app, None);
    assert_eq!(app.world().resource::<Hovered>().0, None);
}

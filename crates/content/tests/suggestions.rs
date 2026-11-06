//! Suggestions on the map (W6b, P-145): raster.json's settlements, routes and points of
//! interest placed by tile and offset, those outside the region dropped, routes cut exactly at
//! its edge. Every expected value was worked by `suggest_ref.py`, written from the acceptance
//! apart from the code, and the crossings by hand (worldgen's unit tests show the working).
//! Cultures, religions and states (W6e, P-148): each one's tiles worked by `peoples_map_ref.py`,
//! written from the acceptance apart from the code, and its extent by W6d's `peoples_ref.py`.

use factional_content::{fingerprint_of, read_azgaar};
use factional_worldgen::{
    Buildings, Coast, Culture, Detail, Extent, Foliage, Levels, Placed, PointOfInterest, Recipe,
    Region, Religion, ReligionKind, Route, RouteKind, Settlement, SettlementKind, Size, Source,
    Stance, State, StateForm, Suggestions, TilePos, rasterise,
};
use proptest::prelude::*;

const RASTER: &str = include_str!("fixtures/azgaar/raster.json");

fn source() -> Source {
    read_azgaar(RASTER).expect("raster.json reads")
}

fn recipe(x: u32, y: u32, width: u32, height: u32, scale: u32) -> Recipe {
    Recipe {
        source: fingerprint_of(RASTER.as_bytes()),
        region: Region {
            x,
            y,
            size: Size { width, height },
        },
        scale,
        seed: 0,
        detail: Detail::default(),
        coast: Coast::default(),
        levels: Levels::default(),
        foliage: Foliage::default(),
    }
}

/// A place in 256ths each way, as its tile and offset.
fn placed(x: u32, y: u32) -> Placed {
    Placed {
        tile: TilePos {
            x: x / 256,
            y: y / 256,
        },
        offset: [(x % 256) as u8, (y % 256) as u8],
    }
}

fn suggested(x: u32, y: u32, width: u32, height: u32, scale: u32) -> Suggestions {
    rasterise(&source(), recipe(x, y, width, height, scale))
        .expect("the recipe makes a map")
        .suggestions()
        .clone()
}

/// How many tiles lie in each people (W6f), in the order `peoples` gives them; checking the
/// territory holds no others.
fn territory(x: u32, y: u32, width: u32, height: u32, scale: u32) -> [u32; 6] {
    let map = rasterise(&source(), recipe(x, y, width, height, scale)).expect("a map");
    let territory = map.territory();
    let ids = [
        "culture_1",
        "culture_2",
        "religion_1",
        "religion_2",
        "state_1",
        "state_2",
    ];
    assert_eq!(territory.len(), 6, "{territory:?}");
    ids.map(|id| territory[id])
}

fn highmoor() -> Settlement {
    Settlement {
        id: "settlement_1".to_owned(),
        name: "Highmoor".to_owned(),
        at: placed(7744, 2496),
        people: 2345,
        kind: SettlementKind::Generic,
        capital: true,
        port: false,
        buildings: Buildings {
            citadel: true,
            walls: true,
            plaza: false,
            temple: true,
            shanty: false,
        },
    }
}

fn lakeside(at: Placed) -> Settlement {
    Settlement {
        id: "settlement_2".to_owned(),
        name: "Lakeside".to_owned(),
        at,
        people: 857,
        kind: SettlementKind::Lake,
        capital: false,
        port: true,
        buildings: Buildings {
            plaza: true,
            ..Buildings::default()
        },
    }
}

fn route(id: &str, name: Option<&str>, kind: RouteKind, points: &[[u32; 2]]) -> Route {
    Route {
        id: id.to_owned(),
        name: name.map(str::to_owned),
        kind,
        points: points.iter().map(|&[x, y]| placed(x, y)).collect(),
    }
}

fn old_keep(at: Placed) -> PointOfInterest {
    PointOfInterest {
        id: "poi_1".to_owned(),
        kind: "ruins".to_owned(),
        icon: "glyph-1f3f0".to_owned(),
        name: Some("Old Keep".to_owned()),
        note: None,
        at,
    }
}

fn extent(cells: u32, people: u64) -> Extent {
    Extent { cells, people }
}

/// raster.json's two cultures, two religions and two states, in order: Highfolk, Pinefolk, Old Ways, Pine Faith, Highmoor and Pinewatch. Seafolk is on no
/// cell, so is never suggested. Highfolk's cells, 1 and 4, have 10 and 8 points at 1000 a
/// point, and Highmoor's 2345 people and Lakeside's 857: 21,202.
fn peoples() -> (Vec<Culture>, Vec<Religion>, Vec<State>) {
    let culture = |id: &str| Some(id.to_owned());
    (
        vec![
            Culture {
                id: "culture_1".to_owned(),
                name: "Highfolk".to_owned(),
                kind: SettlementKind::Highland,
                extent: extent(2, 21_202),
            },
            Culture {
                id: "culture_2".to_owned(),
                name: "Pinefolk".to_owned(),
                kind: SettlementKind::Hunting,
                extent: extent(1, 17_600),
            },
        ],
        vec![
            Religion {
                id: "religion_1".to_owned(),
                name: "Old Ways".to_owned(),
                kind: ReligionKind::Folk,
                form: "Shamanism".to_owned(),
                deity: Some("Ash, the Burning".to_owned()),
                culture: culture("culture_1"),
                extent: extent(1, 12_345),
            },
            Religion {
                id: "religion_2".to_owned(),
                name: "Pine Faith".to_owned(),
                kind: ReligionKind::Organized,
                form: "Monotheism".to_owned(),
                deity: None,
                culture: culture("culture_2"),
                extent: extent(2, 26_457),
            },
        ],
        vec![
            State {
                id: "state_1".to_owned(),
                name: "Highmoor".to_owned(),
                full_name: "Kingdom of Highmoor".to_owned(),
                form: StateForm::Monarchy,
                culture: culture("culture_1"),
                capital: Some("settlement_1".to_owned()),
                stances: vec![("state_2".to_owned(), Stance::Rival)],
                extent: extent(2, 21_202),
            },
            State {
                id: "state_2".to_owned(),
                name: "Pinewatch".to_owned(),
                full_name: "Pinewatch Republic".to_owned(),
                form: StateForm::Republic,
                culture: culture("culture_2"),
                capital: Some("settlement_4".to_owned()),
                stances: vec![("state_1".to_owned(), Stance::Enemy)],
                extent: extent(1, 17_600),
            },
        ],
    )
}

#[test]
fn the_whole_map_at_a_tile_a_pixel_suggests_every_place_where_it_lies() {
    // Worked by peoples_map_ref.py, apart from the code.
    assert_eq!(territory(0, 0, 40, 40, 1), [535, 325, 354, 506, 535, 325]);
    let (cultures, religions, states) = peoples();
    assert_eq!(
        suggested(0, 0, 40, 40, 1),
        Suggestions {
            settlements: vec![
                highmoor(),
                lakeside(placed(3712, 6848)),
                Settlement {
                    id: "settlement_4".to_owned(),
                    name: "Pinewatch".to_owned(),
                    at: placed(8064, 7936),
                    people: 12_600,
                    kind: SettlementKind::Highland,
                    capital: false,
                    port: false,
                    buildings: Buildings {
                        walls: true,
                        plaza: true,
                        shanty: true,
                        ..Buildings::default()
                    },
                },
            ],
            routes: vec![
                route(
                    "route_0",
                    Some("High road"),
                    RouteKind::Road,
                    &[[7744, 2496], [6528, 3968], [5248, 5248], [8064, 7936]]
                ),
                route(
                    "route_1",
                    None,
                    RouteKind::Trail,
                    &[[3712, 6848], [5248, 5248]]
                ),
                route(
                    "route_2",
                    Some("Coast road"),
                    RouteKind::Road,
                    &[[3072, 3072], [8960, 3840], [6400, 6400]]
                ),
            ],
            points: vec![
                PointOfInterest {
                    id: "poi_0".to_owned(),
                    kind: "volcanoes".to_owned(),
                    icon: "glyph-1f30b".to_owned(),
                    name: Some("Ash Peak".to_owned()),
                    note: Some("Dormant volcano.".to_owned()),
                    at: placed(8525, 8806),
                },
                old_keep(placed(5120, 5376)),
            ],
            cultures,
            religions,
            states,
        }
    );
}

#[test]
fn a_region_drops_the_places_outside_it_and_cuts_routes_at_its_edge() {
    // Pixels 10 to 29 each way at 2 tiles a pixel: Highmoor, Pinewatch and Ash Peak lie
    // outside; the High road runs in and out again in one stretch; the Coast road leaves and
    // comes back, in two. Pinewatch, the state, is on the map though its capital isn't.
    assert_eq!(territory(10, 10, 20, 20, 2), [969, 171, 209, 931, 969, 171]);
    let (cultures, religions, states) = peoples();
    assert_eq!(
        suggested(10, 10, 20, 20, 2),
        Suggestions {
            settlements: vec![lakeside(placed(2304, 8576))],
            routes: vec![
                route(
                    "route_0",
                    Some("High road"),
                    RouteKind::Road,
                    &[[10_239, 28], [7936, 2816], [5376, 5376], [10_239, 10_018]]
                ),
                route(
                    "route_1",
                    None,
                    RouteKind::Trail,
                    &[[2304, 8576], [5376, 5376]]
                ),
                route(
                    "route_2_1",
                    Some("Coast road"),
                    RouteKind::Road,
                    &[[1024, 1024], [10_239, 2226]]
                ),
                route(
                    "route_2_2",
                    Some("Coast road"),
                    RouteKind::Road,
                    &[[10_239, 5121], [7680, 7680]]
                ),
            ],
            points: vec![old_keep(placed(5120, 5632))],
            cultures,
            religions,
            states,
        }
    );
}

#[test]
fn a_region_with_no_places_in_it_suggests_none() {
    // The top-left corner, pixels 0 to 9: no place, and no route passes.
    assert_eq!(suggested(0, 0, 10, 10, 1), Suggestions::default());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Any region at any scale: every place is on the map, a settlement is suggested exactly
    /// when its point lies in the region, and every stretch is at least two points, none
    /// repeated in a row.
    #[test]
    fn every_suggestion_lies_on_the_map(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..40,
        height in 1u32..40,
        scale in 1u32..4,
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let map = rasterise(&source(), recipe(x, y, width, height, scale))
            .expect("the recipe makes a map");
        let size = map.size();
        let on = |at: Placed| at.tile.x < size.width && at.tile.y < size.height;
        let suggestions = map.suggestions();
        for settlement in &suggestions.settlements {
            prop_assert!(on(settlement.at));
        }
        for point in &suggestions.points {
            prop_assert!(on(point.at));
        }
        for route in &suggestions.routes {
            prop_assert!(route.points.len() >= 2);
            prop_assert!(route.points.iter().all(|&at| on(at)));
            prop_assert!(route.points.windows(2).all(|pair| pair[0] != pair[1]));
        }
        // A hundredth of a pixel is 2.56 256ths of a tile or more, so rounding never moves a
        // place across the region's edge: it's suggested exactly when its point is inside.
        for settlement in source().settlements {
            let inside = |at: factional_core::Fixed, from: u32, side: u32| {
                let hundredths = at.hundredths();
                hundredths >= i64::from(from) * 100 && hundredths < i64::from(from + side) * 100
            };
            let expected = inside(settlement.at.x, x, width) && inside(settlement.at.y, y, height);
            let id = format!("settlement_{}", settlement.id);
            prop_assert_eq!(
                suggestions.settlements.iter().any(|s| s.id == id),
                expected,
                "{}",
                id
            );
        }
    }
}

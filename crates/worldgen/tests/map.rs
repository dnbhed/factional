//! The map in memory (W1a): its size from the recipe, its tiles in 64 × 64 chunks, in layers,
//! and overrides kept apart from what was generated, so regenerating never undoes them. Every
//! map here is hand-made.

use factional_core::Fixed;
use factional_worldgen::{
    Area, Biome, Buildings, CHUNK, ChunkPos, Coast, Culture, Detail, Extent, Foliage, Levels,
    Override, Placed, Plant, PlantKind, PointOfInterest, Recipe, Region, Religion, ReligionKind,
    Route, RouteKind, Settlement, SettlementKind, Size, Stance, State, StateForm, Suggestions,
    Terrain, Tile, TilePos, Water, WorldMap,
};
use proptest::prelude::*;

const TERRAINS: [Terrain; 9] = [
    Terrain::DeepWater,
    Terrain::ShallowWater,
    Terrain::Sand,
    Terrain::Grass,
    Terrain::ForestFloor,
    Terrain::Marsh,
    Terrain::Rock,
    Terrain::Snow,
    Terrain::Ice,
];

const BIOMES: [Biome; 13] = [
    Biome::Marine,
    Biome::HotDesert,
    Biome::ColdDesert,
    Biome::Savanna,
    Biome::Grassland,
    Biome::TropicalSeasonalForest,
    Biome::TemperateDeciduousForest,
    Biome::TropicalRainforest,
    Biome::TemperateRainforest,
    Biome::Taiga,
    Biome::Tundra,
    Biome::Glacier,
    Biome::Wetland,
];

fn at(x: u32, y: u32) -> TilePos {
    TilePos { x, y }
}

fn size(width: u32, height: u32) -> Size {
    Size { width, height }
}

/// A region of `width` × `height` source pixels at `scale` tiles a pixel.
fn recipe(width: u32, height: u32, scale: u32) -> Recipe {
    Recipe {
        source: String::new(),
        region: Region {
            x: 0,
            y: 0,
            size: size(width, height),
        },
        scale,
        seed: 7,
        detail: Detail::default(),
        coast: Coast::default(),
        levels: Levels::default(),
        foliage: Foliage::default(),
    }
}

/// A tile that says where it is in every layer, so any tile out of place shows.
fn made(pos: TilePos) -> Tile {
    Tile {
        elevation: Fixed::from_hundredths(i64::from(pos.x) * 1000 + i64::from(pos.y)),
        level: ((pos.x * 5 + pos.y) % 17) as u8,
        terrain: TERRAINS[((pos.x + pos.y) % 9) as usize],
        water: match pos.x % 3 {
            0 => None,
            1 => Some(Water::River { flow: pos.y }),
            _ => Some(Water::Lake),
        },
        biome: BIOMES[((pos.x * 3 + pos.y) % 13) as usize],
        temperature: ((pos.x * 7 + pos.y) % 256) as u8 as i8,
        precipitation: (pos.y * 10 + pos.x) as u16,
        plant: pos.y.is_multiple_of(2).then_some(Plant {
            kind: PlantKind::Shrub,
            offset: [(pos.x % 256) as u8, (pos.y % 256) as u8],
        }),
    }
}

/// The 100 × 70 map of acceptance example 1.
fn hundred_by_seventy() -> WorldMap {
    WorldMap::new(recipe(10, 7, 10), made).expect("a map")
}

#[test]
fn a_maps_size_is_its_region_at_its_scale_in_chunks_of_64() {
    let map = hundred_by_seventy();
    assert_eq!(map.size(), size(100, 70));
    assert_eq!(map.chunk_count(), size(2, 2));
    let chunks: Vec<(ChunkPos, Size)> = map
        .chunks()
        .map(|(pos, chunk)| (pos, chunk.size()))
        .collect();
    assert_eq!(
        chunks,
        [
            (ChunkPos { x: 0, y: 0 }, size(64, 64)),
            (ChunkPos { x: 1, y: 0 }, size(36, 64)),
            (ChunkPos { x: 0, y: 1 }, size(64, 6)),
            (ChunkPos { x: 1, y: 1 }, size(36, 6)),
        ]
    );
    assert_eq!(CHUNK, 64);
}

#[test]
fn every_tile_is_asked_for_once_row_by_row_and_kept_where_it_was_made() {
    let mut asked = Vec::new();
    let map = WorldMap::new(recipe(10, 7, 10), |pos| {
        asked.push(pos);
        made(pos)
    })
    .expect("a map");
    let rows: Vec<TilePos> = (0..70)
        .flat_map(|y| (0..100).map(move |x| at(x, y)))
        .collect();
    assert_eq!(asked, rows);
    assert_eq!(map.tile(at(65, 3)), Some(made(at(65, 3))));
    assert_eq!(map.generated(at(65, 3)), Some(made(at(65, 3))));
    let (_, chunk) = map
        .chunks()
        .find(|(pos, _)| *pos == ChunkPos { x: 1, y: 0 })
        .expect("chunk (1, 0)");
    assert_eq!(chunk.tile(1, 3), Some(made(at(65, 3))));
    assert_eq!(chunk.tile(36, 0), None, "past the chunk's right edge");
    assert_eq!(chunk.tile(0, 64), None);
    assert_eq!(map.tile(at(100, 0)), None);
    assert_eq!(
        map.tile(at(130, 0)),
        None,
        "two chunks along, past the edge"
    );
    assert_eq!(map.tile(at(0, 70)), None);
    assert_eq!(map.generated(at(99, 70)), None);
    assert_eq!(map.tile(at(99, 69)), Some(made(at(99, 69))));
}

#[test]
fn a_recipe_that_cant_make_a_map_is_refused_saying_why() {
    let refused = |recipe: Recipe| {
        WorldMap::new(recipe, made)
            .expect_err("refused")
            .to_string()
    };
    assert_eq!(
        refused(recipe(10, 7, 0)),
        "the scale must be at least 1 tile a source pixel"
    );
    assert_eq!(
        refused(recipe(0, 7, 10)),
        "the region must be at least 1 source pixel each way"
    );
    assert_eq!(
        refused(recipe(10, 0, 10)),
        "the region must be at least 1 source pixel each way"
    );
    assert_eq!(
        refused(recipe(1000, 10, 10)),
        "the map would be 10000 × 100 tiles; at most 8192 a side"
    );
    assert_eq!(
        refused(recipe(10, 1000, 10)),
        "the map would be 100 × 10000 tiles; at most 8192 a side"
    );
    assert_eq!(
        refused(recipe(u32::MAX, 1, 2)),
        format!(
            "the map would be {} × 2 tiles; at most 8192 a side",
            u64::from(u32::MAX) * 2
        ),
        "counted without overflowing"
    );
    assert_eq!(
        WorldMap::new(recipe(8192, 1, 1), made).map(|map| map.size()),
        Ok(size(8192, 1)),
        "8192 a side is allowed"
    );
}

#[test]
fn an_override_shows_on_the_tile_and_leaves_what_was_generated() {
    let mut map = hundred_by_seventy();
    let here = at(65, 3);
    let generated = made(here);
    map.edit(
        here,
        Override {
            terrain: Some(Terrain::Snow),
            ..Override::default()
        },
    )
    .expect("on the map");
    map.edit(
        here,
        Override {
            elevation: Some(Fixed::from_hundredths(-250)),
            water: Some(None),
            plant: Some(Some(Plant {
                kind: PlantKind::Tree,
                offset: [128, 64],
            })),
            biome: Some(Biome::Glacier),
            ..Override::default()
        },
    )
    .expect("on the map");
    assert_eq!(
        map.tile(here),
        Some(Tile {
            elevation: Fixed::from_hundredths(-250),
            level: generated.level,
            terrain: Terrain::Snow,
            water: None,
            biome: Biome::Glacier,
            temperature: generated.temperature,
            precipitation: generated.precipitation,
            plant: Some(Plant {
                kind: PlantKind::Tree,
                offset: [128, 64],
            }),
        }),
        "the second adds to the first"
    );
    assert_eq!(map.generated(here), Some(generated));
    assert_eq!(map.overrides().len(), 1);
    assert_eq!(
        map.tile(at(66, 3)),
        Some(made(at(66, 3))),
        "its neighbour untouched"
    );
}

#[test]
fn an_override_replaces_only_the_layers_it_sets() {
    let mut map = hundred_by_seventy();
    // (1, 0): a river, a shrub.
    let here = at(1, 0);
    map.edit(
        here,
        Override {
            plant: Some(None),
            ..Override::default()
        },
    )
    .expect("on the map");
    let tile = map.tile(here).expect("on the map");
    assert_eq!(tile.plant, None, "cleared");
    assert_eq!(tile.water, made(here).water, "kept");
    assert_eq!(tile.elevation, made(here).elevation);
    assert_eq!(tile.terrain, made(here).terrain);
    assert_eq!(tile.biome, made(here).biome);
    map.edit(
        here,
        Override {
            water: Some(Some(Water::Lake)),
            ..Override::default()
        },
    )
    .expect("on the map");
    let tile = map.tile(here).expect("on the map");
    assert_eq!(tile.water, Some(Water::Lake));
    assert_eq!(tile.plant, None, "the first override kept");
}

#[test]
fn an_override_that_sets_nothing_removes_it_and_one_off_the_map_is_refused() {
    let mut map = hundred_by_seventy();
    map.edit(
        at(2, 2),
        Override {
            terrain: Some(Terrain::Ice),
            ..Override::default()
        },
    )
    .expect("on the map");
    map.edit(at(3, 3), Override::default()).expect("on the map");
    assert_eq!(map.overrides().len(), 1, "nothing to keep at (3, 3)");
    assert!(Override::default().is_empty());
    assert!(
        !Override {
            biome: Some(Biome::Taiga),
            ..Override::default()
        }
        .is_empty()
    );
    let refused = map
        .edit(
            at(100, 3),
            Override {
                terrain: Some(Terrain::Ice),
                ..Override::default()
            },
        )
        .expect_err("off the map");
    assert_eq!(
        refused.to_string(),
        "tile (100, 3) is off the map, which is 100 × 70 tiles"
    );
    assert_eq!(map.overrides().len(), 1);
}

/// Overrides at (10, 10), (19, 19), (20, 15), (15, 20), (20, 20) and (99, 69).
fn overridden() -> WorldMap {
    let mut map = hundred_by_seventy();
    for (x, y) in [(10, 10), (19, 19), (20, 15), (15, 20), (20, 20), (99, 69)] {
        map.edit(
            at(x, y),
            Override {
                terrain: Some(Terrain::Rock),
                ..Override::default()
            },
        )
        .expect("on the map");
    }
    map
}

#[test]
fn reset_clears_the_overrides_inside_an_area_and_only_those() {
    let mut map = overridden();
    let cleared = map.reset(Area {
        from: at(10, 10),
        size: size(10, 10),
    });
    assert_eq!(
        cleared, 2,
        "(10, 10) and (19, 19); (20, 15), (15, 20) and (20, 20) are just outside"
    );
    let left: Vec<TilePos> = map.overrides().keys().copied().collect();
    assert_eq!(
        left,
        [at(20, 15), at(15, 20), at(20, 20), at(99, 69)],
        "row by row"
    );
    assert!(at(99, 0) < at(0, 1), "a row before the next");
    assert!(at(1, 5) > at(0, 5));
    assert_eq!(map.tile(at(10, 10)), Some(made(at(10, 10))));
    let cleared = map.reset(Area {
        from: at(90, 60),
        size: size(500, 500),
    });
    assert_eq!(cleared, 1, "an area running off the map");
    assert_eq!(
        map.overrides().keys().copied().collect::<Vec<_>>(),
        [at(20, 15), at(15, 20), at(20, 20)]
    );
    assert_eq!(
        map.reset(Area {
            from: at(0, 0),
            size: size(0, 100),
        }),
        0,
        "an empty area"
    );
}

#[test]
fn regenerating_rebuilds_every_tile_and_keeps_the_overrides() {
    let mut map = overridden();
    let elsewhere = |pos: TilePos| Tile {
        elevation: Fixed::from_hundredths(-1),
        ..made(pos)
    };
    let mut again = recipe(10, 7, 10);
    again.seed = 8;
    map.regenerate(again.clone(), elsewhere)
        .expect("the same size");
    assert_eq!(map.recipe(), &again);
    assert_eq!(map.generated(at(10, 10)), Some(elsewhere(at(10, 10))));
    assert_eq!(map.tile(at(11, 10)), Some(elsewhere(at(11, 10))));
    let kept = map.tile(at(10, 10)).expect("on the map");
    assert_eq!(kept.terrain, Terrain::Rock, "the override applied again");
    assert_eq!(
        kept.elevation,
        Fixed::from_hundredths(-1),
        "the rest regenerated"
    );
    assert_eq!(map.overrides().len(), 6);
}

#[test]
fn regenerating_to_another_size_is_refused_and_changes_nothing() {
    let mut map = overridden();
    let before = map.clone();
    let refused = map
        .regenerate(recipe(5, 5, 10), made)
        .expect_err("another size");
    assert_eq!(
        refused.to_string(),
        "the recipe makes a map of 50 × 50 tiles, and this one is 100 × 70; regenerating can't move the overrides"
    );
    assert_eq!(map, before);
    let refused = map.regenerate(recipe(10, 7, 0), made).expect_err("no map");
    assert_eq!(
        refused.to_string(),
        "the scale must be at least 1 tile a source pixel"
    );
    assert_eq!(map, before);
}

fn placed(x: u32, y: u32) -> Placed {
    Placed {
        tile: at(x, y),
        offset: [255, 0],
    }
}

/// A settlement, a route and a point of interest, each at the map's edge or corner.
fn suggestions() -> Suggestions {
    Suggestions {
        settlements: vec![Settlement {
            id: "settlement_1".to_owned(),
            name: "Highmoor".to_owned(),
            at: placed(99, 69),
            people: 2345,
            kind: SettlementKind::Generic,
            capital: true,
            port: false,
            buildings: Buildings::default(),
        }],
        routes: vec![Route {
            id: "route_0_2".to_owned(),
            name: None,
            kind: RouteKind::Trail,
            points: vec![placed(0, 69), placed(99, 0)],
        }],
        points: vec![PointOfInterest {
            id: "poi_3".to_owned(),
            kind: "ruins".to_owned(),
            icon: "glyph-1f3f0".to_owned(),
            name: Some("Old Keep".to_owned()),
            note: None,
            at: placed(0, 0),
        }],
        ..Suggestions::default()
    }
}

#[test]
fn a_map_has_no_suggestions_until_its_given_them_and_regenerating_keeps_them() {
    let mut map = hundred_by_seventy();
    assert_eq!(map.suggestions(), &Suggestions::default());
    assert_eq!(map.suggest(suggestions()), Ok(()));
    assert_eq!(map.suggestions(), &suggestions());
    map.regenerate(recipe(10, 7, 10), made)
        .expect("the same size");
    assert_eq!(map.suggestions(), &suggestions());
    // Given again, they're replaced.
    assert_eq!(map.suggest(Suggestions::default()), Ok(()));
    assert_eq!(map.suggestions(), &Suggestions::default());
}

#[test]
fn suggestions_off_the_map_are_refused_naming_which_and_change_nothing() {
    let mut map = hundred_by_seventy();
    map.suggest(suggestions()).expect("on the map");
    let before = map.clone();
    let refusal = |map: &mut WorldMap, wrong: Suggestions| {
        let refused = map.suggest(wrong).expect_err("refused").to_string();
        assert_eq!(map, &before, "nothing changed");
        refused
    };
    let mut wrong = suggestions();
    wrong.settlements[0].at = placed(100, 69);
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion settlement_1 is at tile (100, 69), off the map of 100 × 70 tiles"
    );
    let mut wrong = suggestions();
    wrong.routes[0].points[1] = placed(99, 70);
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion route_0_2 is at tile (99, 70), off the map of 100 × 70 tiles"
    );
    let mut wrong = suggestions();
    wrong.points[0].at = placed(100, 70);
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion poi_3 is at tile (100, 70), off the map of 100 × 70 tiles"
    );
}

#[test]
fn a_suggestion_id_must_be_valid_and_unique_across_every_kind() {
    let mut map = hundred_by_seventy();
    let before = map.clone();
    for (id, why) in [
        (
            "Route_0",
            "the suggestion id \"Route_0\" isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores",
        ),
        (
            "route-0",
            "the suggestion id \"route-0\" isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores",
        ),
        ("settlement_1", "two suggestions are settlement_1"),
        ("poi_3", "two suggestions are poi_3"),
    ] {
        let mut wrong = suggestions();
        wrong.routes[0].id = id.to_owned();
        assert_eq!(map.suggest(wrong).expect_err(id).to_string(), why);
        assert_eq!(map, before);
    }
    // Two settlements, and two points of interest, sharing an id.
    let mut wrong = suggestions();
    wrong.settlements.push(wrong.settlements[0].clone());
    assert_eq!(
        map.suggest(wrong).expect_err("twice").to_string(),
        "two suggestions are settlement_1"
    );
    let mut wrong = suggestions();
    wrong.points.push(wrong.points[0].clone());
    assert_eq!(
        map.suggest(wrong).expect_err("twice").to_string(),
        "two suggestions are poi_3"
    );
    let mut wrong = suggestions();
    wrong.settlements[0].id = String::new();
    assert_eq!(
        map.suggest(wrong).expect_err("empty").to_string(),
        "the suggestion id \"\" isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores"
    );
    let mut wrong = suggestions();
    wrong.points[0].id = "poi 3".to_owned();
    assert!(map.suggest(wrong).is_err());
    assert_eq!(map, before);
}

#[test]
fn a_route_of_fewer_than_two_points_is_refused() {
    let mut map = hundred_by_seventy();
    let mut wrong = suggestions();
    wrong.routes[0].points.truncate(1);
    assert_eq!(
        map.suggest(wrong).expect_err("short").to_string(),
        "the route route_0_2 has 1 point; a route needs at least two"
    );
    let mut wrong = suggestions();
    wrong.routes[0].points.clear();
    assert_eq!(
        map.suggest(wrong).expect_err("empty").to_string(),
        "the route route_0_2 has 0 points; a route needs at least two"
    );
    assert_eq!(map.suggestions(), &Suggestions::default());
}

#[test]
fn the_first_wrong_suggestion_is_named_settlements_then_routes_then_points() {
    let mut map = hundred_by_seventy();
    let mut wrong = suggestions();
    wrong.settlements[0].at = placed(100, 0);
    wrong.routes[0].points.clear();
    wrong.points[0].at = placed(0, 70);
    assert_eq!(
        map.suggest(wrong.clone()).expect_err("wrong").to_string(),
        "the suggestion settlement_1 is at tile (100, 0), off the map of 100 × 70 tiles"
    );
    wrong.settlements.clear();
    assert_eq!(
        map.suggest(wrong.clone()).expect_err("wrong").to_string(),
        "the route route_0_2 has 0 points; a route needs at least two"
    );
    wrong.routes.clear();
    assert_eq!(
        map.suggest(wrong).expect_err("wrong").to_string(),
        "the suggestion poi_3 is at tile (0, 70), off the map of 100 × 70 tiles"
    );
}

/// `suggestions()` with a culture, a religion and a state (W6e), their links on the map or not.
fn with_peoples() -> Suggestions {
    let extent = Extent {
        cells: 3,
        people: 900,
    };
    Suggestions {
        cultures: vec![Culture {
            id: "culture_1".to_owned(),
            name: "Highfolk".to_owned(),
            kind: SettlementKind::Highland,
            extent,
            tiles: 7000,
        }],
        religions: vec![Religion {
            id: "religion_2".to_owned(),
            name: "Old Ways".to_owned(),
            kind: ReligionKind::Folk,
            form: "Shamanism".to_owned(),
            deity: None,
            culture: Some("culture_1".to_owned()),
            extent,
            tiles: 1,
        }],
        states: vec![State {
            id: "state_3".to_owned(),
            name: "Highmoor".to_owned(),
            full_name: "Kingdom of Highmoor".to_owned(),
            form: StateForm::Monarchy,
            culture: Some("culture_9".to_owned()),
            capital: Some("settlement_1".to_owned()),
            stances: vec![
                ("state_1".to_owned(), Stance::Rival),
                ("state_4".to_owned(), Stance::Ally),
            ],
            extent,
            tiles: 40,
        }],
        ..suggestions()
    }
}

#[test]
fn peoples_are_suggested_after_places_their_links_on_the_map_or_not() {
    let mut map = hundred_by_seventy();
    assert_eq!(map.suggest(with_peoples()), Ok(()));
    assert_eq!(map.suggestions(), &with_peoples());
    assert_eq!(
        map.suggestions().ids().collect::<Vec<_>>(),
        [
            "settlement_1",
            "route_0_2",
            "poi_3",
            "culture_1",
            "religion_2",
            "state_3"
        ]
    );
    assert_eq!(map.suggestions().len(), 6);
    assert!(!map.suggestions().is_empty());
    assert!(Suggestions::default().is_empty());
    // Each is dismissed and restored by its id, as a place is.
    assert_eq!(map.dismiss(&["state_3", "culture_1"]), Ok(()));
    assert_eq!(map.dismissed_by("state_3"), Some("state_3"));
    assert_eq!(map.dismissed_by("religion_2"), None);
    assert_eq!(
        map.dismiss(&["culture_2"]).map_err(|e| e.to_string()),
        Err("there's no suggestion culture_2 on the map; did you mean culture_1?".to_owned())
    );
    assert_eq!(map.restore(&["culture_1"]), Ok(()));
    assert_eq!(map.dismissed_by("culture_1"), None);
}

#[test]
fn a_people_covers_at_least_one_tile_and_no_more_than_the_map() {
    let mut map = hundred_by_seventy();
    let before = map.clone();
    let refusal = |map: &mut WorldMap, wrong: Suggestions| {
        let refused = map.suggest(wrong).expect_err("refused").to_string();
        assert_eq!(map, &before, "nothing changed");
        refused
    };
    let mut wrong = with_peoples();
    wrong.cultures[0].tiles = 0;
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion culture_1 covers no tiles"
    );
    let mut wrong = with_peoples();
    wrong.religions[0].tiles = 7001;
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion religion_2 covers 7001 tiles, more than the map's 7000"
    );
    let mut wrong = with_peoples();
    wrong.states[0].tiles = 0;
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion state_3 covers no tiles"
    );
    let mut wrong = with_peoples();
    wrong.states[0].tiles = 7001;
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion state_3 covers 7001 tiles, more than the map's 7000"
    );
    let mut wrong = with_peoples();
    wrong.cultures[0].id = "Culture-1".to_owned();
    assert_eq!(
        refusal(&mut map, wrong),
        "the suggestion id \"Culture-1\" isn't a valid id: a lowercase letter, then lowercase \
         letters, digits or underscores"
    );
    // A people's id is unique among every kind.
    let mut wrong = with_peoples();
    wrong.states[0].id = "settlement_1".to_owned();
    assert_eq!(refusal(&mut map, wrong), "two suggestions are settlement_1");
}

#[test]
fn links_and_stances_name_valid_ids_and_a_stance_is_toward_another_once() {
    let mut map = hundred_by_seventy();
    let refused = |map: &mut WorldMap, wrong: Suggestions| {
        map.suggest(wrong).expect_err("refused").to_string()
    };
    let valid = "which isn't a valid id: a lowercase letter, then lowercase letters, digits or \
                 underscores";
    let mut wrong = with_peoples();
    wrong.religions[0].culture = Some("Culture 1".to_owned());
    assert_eq!(
        refused(&mut map, wrong),
        format!("the suggestion religion_2 names \"Culture 1\", {valid}")
    );
    let mut wrong = with_peoples();
    wrong.states[0].culture = Some(String::new());
    assert_eq!(
        refused(&mut map, wrong),
        format!("the suggestion state_3 names \"\", {valid}")
    );
    let mut wrong = with_peoples();
    wrong.states[0].capital = Some("9".to_owned());
    assert_eq!(
        refused(&mut map, wrong),
        format!("the suggestion state_3 names \"9\", {valid}")
    );
    let mut wrong = with_peoples();
    wrong.states[0].stances[1].0 = "State 4".to_owned();
    assert_eq!(
        refused(&mut map, wrong),
        format!("the suggestion state_3 names \"State 4\", {valid}")
    );
    let mut wrong = with_peoples();
    wrong.states[0].stances[1].0 = "state_3".to_owned();
    assert_eq!(
        refused(&mut map, wrong),
        "the suggestion state_3 holds a stance toward itself"
    );
    let mut wrong = with_peoples();
    wrong.states[0].stances[1].0 = "state_1".to_owned();
    assert_eq!(
        refused(&mut map, wrong),
        "the suggestion state_3 holds two stances toward state_1"
    );
    // The first wrong is named: places, then cultures, religions and states.
    let mut wrong = with_peoples();
    wrong.cultures[0].tiles = 0;
    wrong.religions[0].tiles = 0;
    wrong.states[0].tiles = 0;
    wrong.points[0].at = placed(0, 70);
    assert_eq!(
        refused(&mut map, wrong.clone()),
        "the suggestion poi_3 is at tile (0, 70), off the map of 100 × 70 tiles"
    );
    wrong.points.clear();
    assert_eq!(
        refused(&mut map, wrong.clone()),
        "the suggestion culture_1 covers no tiles"
    );
    wrong.cultures.clear();
    assert_eq!(
        refused(&mut map, wrong.clone()),
        "the suggestion religion_2 covers no tiles"
    );
    wrong.religions.clear();
    assert_eq!(
        refused(&mut map, wrong),
        "the suggestion state_3 covers no tiles"
    );
}

/// Two settlements; route_0 whole, route_2 cut in two, and route_20 cut, its first stretch
/// here; and a point of interest.
fn to_dismiss() -> Suggestions {
    let mut suggestions = suggestions();
    let mut settlement_2 = suggestions.settlements[0].clone();
    settlement_2.id = "settlement_2".to_owned();
    suggestions.settlements.push(settlement_2);
    suggestions.routes = ["route_0", "route_2_1", "route_2_2", "route_20_1"]
        .into_iter()
        .map(|id| Route {
            id: id.to_owned(),
            ..suggestions.routes[0].clone()
        })
        .collect();
    suggestions.points[0].id = "poi_1".to_owned();
    suggestions
}

fn dismissing() -> WorldMap {
    let mut map = hundred_by_seventy();
    map.suggest(to_dismiss()).expect("on the map");
    map
}

fn ids(map: &WorldMap) -> Vec<&str> {
    map.dismissed().iter().map(String::as_str).collect()
}

#[test]
fn dismissing_marks_suggestions_and_restoring_unmarks_them() {
    let mut map = dismissing();
    assert_eq!(ids(&map), Vec::<&str>::new());
    assert_eq!(map.dismiss(&["settlement_1", "poi_1"]), Ok(()));
    assert_eq!(ids(&map), ["poi_1", "settlement_1"]);
    assert_eq!(map.dismissed_by("settlement_1"), Some("settlement_1"));
    assert_eq!(map.dismissed_by("poi_1"), Some("poi_1"));
    assert_eq!(map.dismissed_by("settlement_2"), None);
    assert_eq!(map.dismissed_by("route_0"), None);
    assert_eq!(map.restore(&["poi_1"]), Ok(()));
    assert_eq!(ids(&map), ["settlement_1"]);
    assert_eq!(map.dismissed_by("poi_1"), None);
    // The suggestions themselves are untouched.
    assert_eq!(map.suggestions(), &to_dismiss());
}

#[test]
fn a_routes_own_id_covers_its_stretches_and_a_stretchs_only_itself() {
    let mut map = dismissing();
    assert_eq!(map.dismiss(&["route_2"]), Ok(()));
    assert_eq!(ids(&map), ["route_2"]);
    assert_eq!(map.dismissed_by("route_2_1"), Some("route_2"));
    assert_eq!(map.dismissed_by("route_2_2"), Some("route_2"));
    // route_20's stretch isn't route_2's, and route_0 is another route.
    assert_eq!(map.dismissed_by("route_20_1"), None);
    assert_eq!(map.dismissed_by("route_0"), None);
    assert_eq!(map.not_on_the_map(), Vec::<&str>::new());
    assert_eq!(
        map.dismiss(&["route_2_1"]).map_err(|e| e.to_string()),
        Err("route_2_1 is already dismissed with route_2".to_owned())
    );
    assert_eq!(
        map.restore(&["route_2_2"]).map_err(|e| e.to_string()),
        Err("route_2_2 is dismissed with route_2; restore route_2".to_owned())
    );
    assert_eq!(map.restore(&["route_2"]), Ok(()));
    assert_eq!(map.dismiss(&["route_2_1", "route_20"]), Ok(()));
    assert_eq!(map.dismissed_by("route_2_1"), Some("route_2_1"));
    assert_eq!(map.dismissed_by("route_2_2"), None);
    assert_eq!(map.dismissed_by("route_20_1"), Some("route_20"));
    // A route whose stretch is dismissed can still be dismissed whole.
    assert_eq!(map.dismiss(&["route_2"]), Ok(()));
    assert_eq!(ids(&map), ["route_2", "route_20", "route_2_1"]);
    assert_eq!(map.dismissed_by("route_2_1"), Some("route_2_1"));
    assert_eq!(map.dismissed_by("route_2_2"), Some("route_2"));
    // A whole route is dismissed by its own id.
    assert_eq!(map.dismiss(&["route_0"]), Ok(()));
    assert_eq!(map.dismissed_by("route_0"), Some("route_0"));
}

#[test]
fn dismissing_an_id_not_suggested_twice_or_again_is_refused_and_changes_nothing() {
    let mut map = dismissing();
    map.dismiss(&["poi_1"]).expect("suggested");
    let before = map.clone();
    let refused = |map: &mut WorldMap, ids: &[&str]| {
        let why = map.dismiss(ids).expect_err("refused").to_string();
        assert_eq!(map, &before, "nothing changed");
        why
    };
    assert_eq!(
        refused(&mut map, &["settlement_2", "settlement_7"]),
        "there's no suggestion settlement_7 on the map; did you mean settlement_1?"
    );
    assert_eq!(
        refused(&mut map, &["harbour"]),
        "there's no suggestion harbour on the map"
    );
    // A route with no stretch on the map, and a stretch that isn't.
    assert_eq!(
        refused(&mut map, &["route_9"]),
        "there's no suggestion route_9 on the map; did you mean route_0?"
    );
    assert_eq!(
        refused(&mut map, &["route_2_3"]),
        "there's no suggestion route_2_3 on the map; did you mean route_2_1?"
    );
    assert_eq!(
        refused(&mut map, &["settlement_2", "poi_1"]),
        "poi_1 is already dismissed"
    );
    assert_eq!(
        refused(&mut map, &["settlement_2", "route_0", "settlement_2"]),
        "settlement_2 is named twice"
    );
    assert_eq!(
        refused(&mut map, &["route_2", "route_2_1"]),
        "route_2_1 is already dismissed with route_2"
    );
}

#[test]
fn restoring_an_id_not_dismissed_is_refused_and_changes_nothing() {
    let mut map = dismissing();
    map.dismiss(&["poi_1", "route_2"]).expect("suggested");
    let before = map.clone();
    for (ids, why) in [
        (
            &["poi_1", "settlement_2"][..],
            "settlement_2 isn't dismissed",
        ),
        (&["nowhere"][..], "nowhere isn't dismissed"),
        (&["poi_1", "poi_1"][..], "poi_1 is named twice"),
        (
            &["route_2_1"][..],
            "route_2_1 is dismissed with route_2; restore route_2",
        ),
    ] {
        assert_eq!(map.restore(ids).expect_err("refused").to_string(), why);
        assert_eq!(map, before, "nothing changed");
    }
    assert_eq!(map.restore(&["route_2", "poi_1"]), Ok(()));
    assert_eq!(ids(&map), Vec::<&str>::new());
}

#[test]
fn dismissals_are_kept_through_new_suggestions_and_regenerating() {
    let mut map = dismissing();
    map.dismiss(&["settlement_1", "route_2", "poi_1"])
        .expect("suggested");
    map.regenerate(recipe(10, 7, 10), made)
        .expect("the same size");
    assert_eq!(ids(&map), ["poi_1", "route_2", "settlement_1"]);
    // New suggestions without settlement_1 or route_2: kept, but not on the map.
    let mut fewer = to_dismiss();
    fewer.settlements.remove(0);
    fewer
        .routes
        .retain(|route| !route.id.starts_with("route_2_"));
    map.suggest(fewer).expect("on the map");
    assert_eq!(ids(&map), ["poi_1", "route_2", "settlement_1"]);
    assert_eq!(map.not_on_the_map(), ["route_2", "settlement_1"]);
    assert_eq!(map.dismissed_by("poi_1"), Some("poi_1"));
    // One not on the map can still be restored.
    assert_eq!(map.restore(&["settlement_1"]), Ok(()));
    assert_eq!(map.not_on_the_map(), ["route_2"]);
    // And when it comes back, the dismissal applies again.
    map.suggest(to_dismiss()).expect("on the map");
    assert_eq!(map.not_on_the_map(), Vec::<&str>::new());
    assert_eq!(map.dismissed_by("route_2_2"), Some("route_2"));
    assert_eq!(map.dismissed_by("settlement_1"), None);
}

#[test]
fn only_route_digits_underscore_digits_is_a_stretch_its_route_covers() {
    // Kept dismissals needn't be suggested, so a route named anything can be dismissed; only
    // `route_<digits>_<digits>` is a stretch of it.
    let mut map = dismissing();
    let kept: std::collections::BTreeSet<String> =
        ["route_2", "route_x", "route"].map(str::to_owned).into();
    assert_eq!(map.keep_dismissed(kept), Ok(()));
    assert_eq!(map.dismissed_by("route_2_1"), Some("route_2"));
    assert_eq!(map.dismissed_by("route_2_x"), None);
    assert_eq!(map.dismissed_by("route_x_1"), None);
    assert_eq!(map.dismissed_by("route_2_"), None);
    assert_eq!(map.dismissed_by("route__1"), None);
    assert_eq!(map.dismissed_by("route_1"), None);
    assert_eq!(
        map.restore(&["route_x_1"]).map_err(|e| e.to_string()),
        Err("route_x_1 isn't dismissed".to_owned())
    );
}

#[test]
fn kept_dismissals_need_valid_ids_but_not_suggestions() {
    let mut map = dismissing();
    let kept: std::collections::BTreeSet<String> =
        ["route_7".to_owned(), "settlement_1".to_owned()].into();
    assert_eq!(map.keep_dismissed(kept.clone()), Ok(()));
    assert_eq!(map.dismissed(), &kept);
    assert_eq!(map.not_on_the_map(), ["route_7"]);
    let before = map.clone();
    let wrong: std::collections::BTreeSet<String> =
        ["poi_1".to_owned(), "Route-7".to_owned()].into();
    assert_eq!(
        map.keep_dismissed(wrong)
            .expect_err("not an id")
            .to_string(),
        "the dismissal \"Route-7\" isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores"
    );
    assert_eq!(map, before);
}

proptest! {
    #[test]
    fn every_tile_reads_back_as_generated(
        width in 1u32..40,
        height in 1u32..40,
        scale in 1u32..5,
        salt in 0u32..100_000,
    ) {
        let make = |pos: TilePos| made(TilePos { x: pos.x ^ salt, y: pos.y + salt });
        let map = WorldMap::new(recipe(width, height, scale), make).expect("a map");
        let size = map.size();
        prop_assert_eq!(size, Size { width: width * scale, height: height * scale });
        for y in 0..size.height {
            for x in 0..size.width {
                prop_assert_eq!(map.tile(at(x, y)), Some(make(at(x, y))));
            }
        }
        let tiles: u32 = map
            .chunks()
            .map(|(_, chunk)| chunk.size().width * chunk.size().height)
            .sum();
        prop_assert_eq!(tiles, size.width * size.height);
    }
}

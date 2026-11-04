//! The map in memory (W1a): its size from the recipe, its tiles in 64 × 64 chunks, in layers,
//! and overrides kept apart from what was generated, so regenerating never undoes them. Every
//! map here is hand-made.

use factional_core::Fixed;
use factional_worldgen::{
    Area, Biome, CHUNK, ChunkPos, Override, Plant, PlantKind, Recipe, Region, Size, Terrain, Tile,
    TilePos, Water, WorldMap,
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
    }
}

/// A tile that says where it is in every layer, so any tile out of place shows.
fn made(pos: TilePos) -> Tile {
    Tile {
        elevation: Fixed::from_hundredths(i64::from(pos.x) * 1000 + i64::from(pos.y)),
        terrain: TERRAINS[((pos.x + pos.y) % 9) as usize],
        water: match pos.x % 3 {
            0 => None,
            1 => Some(Water::River { flow: pos.y }),
            _ => Some(Water::Lake),
        },
        biome: BIOMES[((pos.x * 3 + pos.y) % 13) as usize],
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
            terrain: Terrain::Snow,
            water: None,
            biome: Biome::Glacier,
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

//! Levels (W3c, P-138): elevation quantised for drawing in 2D, plateaus and pits cleaned, and
//! ledges and cliffs where a level drops to the next; water at the level of its surface (W3e,
//! P-140). The expected levels and counts were
//! worked by a separate script from the rules in DESIGN.md §21, never by this code.

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Drop, Drops, Feature, Foliage, Levels, MapError, Override, Point, Recipe,
    Region, Size, Source, SourceCell, SourceVertex, Terrain, Tile, TilePos, WorldMap,
    level_heights, rasterise,
};
use proptest::prelude::*;

fn hundredths(value: i64) -> Fixed {
    Fixed::from_hundredths(value)
}

fn cell(x: i64, y: i64, height: u8, feature: u32, neighbours: &[u32]) -> SourceCell {
    SourceCell {
        centre: Point {
            x: hundredths(x),
            y: hundredths(y),
        },
        height,
        feature,
        neighbours: neighbours.to_vec(),
        corners: Vec::new(),
        flux: 0,
        // Azgaar's land cells have a land biome, its sea and lakes Marine.
        biome: match height {
            20.. => Biome::Grassland,
            _ => Biome::Marine,
        },
        temperature: 0,
        precipitation: 0,
        culture: None,
        religion: None,
        state: None,
    }
}

fn vertex(cells: [Option<u32>; 3]) -> SourceVertex {
    SourceVertex {
        at: Point {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
        cells,
    }
}

/// W3a's five cells: A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40, C (10.5, 30.5)
/// at 19 in a lake with its surface at 19.50, D (30.5, 30.5) at 60 and E (20.5, 20.5) at 30,
/// the last three on an island; each with its neighbours, and the triangles ABE, BDE, DCE, CAE.
fn raster() -> Source {
    Source {
        name: "Raster".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size {
            width: 40,
            height: 40,
        },
        height_exponent: hundredths(2_00),
        cells: vec![
            cell(10_50, 10_50, 10, 0, &[1, 2, 4]),
            cell(30_50, 10_50, 40, 2, &[0, 3, 4]),
            cell(10_50, 30_50, 19, 1, &[0, 3, 4]),
            cell(30_50, 30_50, 60, 2, &[1, 2, 4]),
            cell(20_50, 20_50, 30, 2, &[0, 1, 2, 3]),
        ],
        vertices: vec![
            vertex([Some(0), Some(1), Some(4)]),
            vertex([Some(1), Some(3), Some(4)]),
            vertex([Some(3), Some(2), Some(4)]),
            vertex([Some(2), Some(0), Some(4)]),
            vertex([Some(0), None, None]),
        ],
        features: vec![
            Feature::Ocean,
            Feature::Lake {
                surface: hundredths(19_50),
            },
            Feature::Island,
        ],
        rivers: Vec::new(),
        settlements: Vec::new(),
        routes: Vec::new(),
        markers: Vec::new(),
        cultures: Vec::new(),
        religions: Vec::new(),
        states: Vec::new(),
    }
}

fn detail(strength: i64, wavelength: i64, octaves: u32) -> Detail {
    Detail {
        strength: hundredths(strength),
        wavelength: hundredths(wavelength),
        octaves,
    }
}

const FLAT: Detail = Detail {
    strength: Fixed::ZERO,
    wavelength: Fixed::from_hundredths(8_00),
    octaves: 4,
};

fn levels(spacing: i64, specks: u32) -> Levels {
    Levels {
        spacing: hundredths(spacing),
        specks,
    }
}

fn recipe(region: [u32; 4], scale: u32, seed: u32, detail: Detail, levels: Levels) -> Recipe {
    let [x, y, width, height] = region;
    Recipe {
        source: String::new(),
        region: Region {
            x,
            y,
            size: Size { width, height },
        },
        scale,
        seed,
        detail,
        coast: Coast::default(),
        levels,
        foliage: Foliage::default(),
    }
}

const WHOLE: [u32; 4] = [0, 0, 40, 40];

fn map(recipe: Recipe) -> WorldMap {
    rasterise(&raster(), recipe).expect("the recipe makes a map")
}

fn whole(seed: u32, detail: Detail, levels: Levels) -> WorldMap {
    map(recipe(WHOLE, 1, seed, detail, levels))
}

fn tile(map: &WorldMap, x: u32, y: u32) -> Tile {
    map.tile(TilePos { x, y }).expect("on the map")
}

/// A tile's height and level.
fn at(map: &WorldMap, x: u32, y: u32) -> (Fixed, u8) {
    let tile = tile(map, x, y);
    (tile.elevation, tile.level)
}

/// Every tile, row by row.
fn tiles(map: &WorldMap) -> Vec<Tile> {
    let size = map.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| tile(map, x, y))
        .collect()
}

/// How many land tiles are at each level, from 0 to the highest.
fn land_at_each_level(map: &WorldMap) -> Vec<usize> {
    let land: Vec<u8> = tiles(map)
        .into_iter()
        .filter(|t| !t.is_water())
        .map(|t| t.level)
        .collect();
    let top = land.iter().copied().max().unwrap_or(0);
    (0..=top)
        .map(|level| land.iter().filter(|&&l| l == level).count())
        .collect()
}

/// How many ledges and cliffs the map has, each counted once, on its higher tile.
fn drops(map: &WorldMap) -> [usize; 2] {
    let size = map.size();
    let mut counts = [0; 2];
    for y in 0..size.height {
        for x in 0..size.width {
            let Drops {
                north,
                west,
                east,
                south,
            } = map.drops(TilePos { x, y }).expect("on the map");
            for side in [north, west, east, south].into_iter().flatten() {
                counts[match side {
                    Drop::Ledge => 0,
                    Drop::Cliff => 1,
                }] += 1;
            }
        }
    }
    counts
}

fn highest(map: &WorldMap) -> u8 {
    tiles(map).iter().map(|t| t.level).max().expect("tiles")
}

#[test]
fn levels_default_to_every_five_points_and_specks_under_four_tiles() {
    assert_eq!(Levels::default(), levels(5_00, 4));
}

#[test]
fn each_level_starts_at_the_metres_of_its_height_on_azgaars_scale() {
    let two = hundredths(2_00);
    // (20 + 5k − 18)², for k from 1 to 16.
    let squares: Vec<Fixed> = (1..=16)
        .map(|k: i64| hundredths((2 + 5 * k).pow(2) * 100))
        .collect();
    assert_eq!(level_heights(hundredths(5_00), two), squares);
    assert_eq!(level_heights(hundredths(2_50), two).len(), 32);
    assert_eq!(level_heights(hundredths(80_00), two), [hundredths(6724_00)]);
    // 81.99².
    assert_eq!(level_heights(hundredths(79_99), two), [hundredths(6722_36)]);
    // 42^1.5 and 82^1.5.
    assert_eq!(
        level_heights(hundredths(40_00), hundredths(1_50)),
        [hundredths(272_19), hundredths(742_54)]
    );
}

#[test]
fn a_land_tiles_level_is_how_many_levels_start_at_or_below_it() {
    let detailed = whole(7, Detail::default(), Levels::default());
    assert_eq!(at(&detailed, 20, 15), (hundredths(159_63), 2));
    assert_eq!(at(&detailed, 25, 25), (hundredths(882_14), 5));
    assert_eq!(at(&detailed, 30, 30), (hundredths(1844_35), 8));
    assert_eq!(at(&detailed, 15, 15), (hundredths(19_44), 0));
    // Exactly where level 8 starts, 42².
    let flat = whole(7, FLAT, Levels::default());
    assert_eq!(at(&flat, 30, 30), (hundredths(1764_00), 8));
}

#[test]
fn water_stands_at_the_level_of_its_surface() {
    let detailed = whole(7, Detail::default(), Levels::default());
    let water: Vec<Tile> = tiles(&detailed)
        .into_iter()
        .filter(Tile::is_water)
        .collect();
    // The sea's surface at 0 m and the lake's at −1.28 m both lie below level 1's start.
    assert!(water.iter().all(|t| t.level == 0));
    // Deep sea at −28.97, and a lake.
    assert_eq!(at(&detailed, 13, 13), (hundredths(-28_97), 0));
    assert!(
        water
            .iter()
            .any(|t| t.terrain == Terrain::ShallowWater && t.water.is_some())
    );
}

#[test]
fn plateaus_and_pits_under_the_specks_take_the_nearest_level_around_them() {
    let rough = detail(1_00, 3_00, 3);
    let kept = whole(7, rough, levels(2_50, 0));
    let cleaned = whole(7, rough, levels(2_50, 4));
    // A hollow below the sea, away from any water cell, and a pit at 15.67.
    assert_eq!(at(&kept, 20, 15), (hundredths(-119_55), 0));
    assert_eq!(at(&cleaned, 20, 15), (hundredths(-119_55), 2));
    assert_eq!(at(&kept, 15, 15), (hundredths(15_67), 0));
    assert_eq!(at(&cleaned, 15, 15), (hundredths(15_67), 3));
    assert_eq!((drops(&kept), highest(&kept)), ([705, 1356], 19));
    assert_eq!((drops(&cleaned), highest(&cleaned)), ([686, 1115], 18));
    // Heights never change.
    let heights =
        |map: &WorldMap| -> Vec<Fixed> { tiles(map).iter().map(|t| t.elevation).collect() };
    assert_eq!(heights(&kept), heights(&cleaned));
}

#[test]
fn the_whole_map_matches_the_separate_script() {
    let defaults = whole(7, Detail::default(), Levels::default());
    assert_eq!(
        land_at_each_level(&defaults),
        [74, 129, 90, 147, 254, 49, 36, 196, 81]
    );
    // Water drops too, by the level of its surface.
    assert_eq!(drops(&defaults), [454, 52]);
    assert_eq!(
        drops(&whole(7, Detail::default(), levels(5_00, 0))),
        [473, 52]
    );
    let wide = whole(7, Detail::default(), levels(10_00, 4));
    assert_eq!((drops(&wide), highest(&wide)), ([294, 24], 4));
    let seed = whole(0, Detail::default(), Levels::default());
    assert_eq!((drops(&seed), highest(&seed)), ([490, 48], 8));
    let region = map(recipe(
        [10, 10, 20, 20],
        2,
        7,
        Detail::default(),
        Levels::default(),
    ));
    assert_eq!((drops(&region), highest(&region)), ([435, 14], 7));
}

/// A 3 × 3 map of grass, each tile at the level `levels` draws for it, a row a string, and `~`
/// for water.
fn drawn(levels: [&str; 3]) -> WorldMap {
    let recipe = recipe([0, 0, 3, 3], 1, 0, FLAT, Levels::default());
    WorldMap::new(recipe, |at| {
        let c = levels[at.y as usize].as_bytes()[at.x as usize] as char;
        Tile {
            elevation: Fixed::ZERO,
            level: c.to_digit(10).map_or(0, |l| l as u8),
            terrain: match c {
                '~' => Terrain::ShallowWater,
                _ => Terrain::Grass,
            },
            water: None,
            biome: Biome::Grassland,
            temperature: 0,
            precipitation: 0,
            plant: None,
        }
    })
    .expect("a map")
}

fn sides(map: &WorldMap, x: u32, y: u32) -> [Option<Drop>; 4] {
    let Drops {
        north,
        west,
        east,
        south,
    } = map.drops(TilePos { x, y }).expect("on the map");
    [north, west, east, south]
}

#[test]
fn a_tile_drops_by_a_ledge_one_level_down_and_a_cliff_further() {
    let map = drawn(["121", "033", "1~1"]);
    // North 2 a ledge, west 0 a cliff, east level, south water at 0 a cliff.
    assert_eq!(
        sides(&map, 1, 1),
        [
            Some(Drop::Ledge),
            Some(Drop::Cliff),
            None,
            Some(Drop::Cliff)
        ]
    );
    // A higher neighbour is its drop, not this tile's; off the map, nothing.
    assert_eq!(
        sides(&map, 1, 0),
        [None, Some(Drop::Ledge), Some(Drop::Ledge), None]
    );
    assert_eq!(sides(&map, 0, 0), [None, None, None, Some(Drop::Ledge)]);
    assert_eq!(
        sides(&map, 0, 2),
        [Some(Drop::Ledge), None, Some(Drop::Ledge), None]
    );
    assert_eq!(sides(&map, 2, 2), [None, Some(Drop::Ledge), None, None]);
    assert_eq!(map.drops(TilePos { x: 3, y: 0 }), None);
}

#[test]
fn water_drops_and_is_dropped_to_by_its_level_as_land_is() {
    let mut map = drawn(["111", "1~~", "111"]);
    // Land at 1 drops to water at 0.
    assert_eq!(sides(&map, 1, 0), [None, None, None, Some(Drop::Ledge)]);
    let mut level = |level: u8| {
        map.edit(
            TilePos { x: 1, y: 1 },
            Override {
                level: Some(level),
                ..Override::default()
            },
        )
        .expect("on the map");
        sides(&map, 1, 1)
    };
    // Water at 3 drops to the land at 1 around it and to the water at 0 beside it.
    assert_eq!(level(3), [Some(Drop::Cliff); 4]);
    // Water at 1 drops to water only.
    assert_eq!(level(1), [None, None, Some(Drop::Ledge), None]);
}

#[test]
fn drops_follow_the_overrides() {
    let mut map = drawn(["111", "131", "111"]);
    assert_eq!(sides(&map, 1, 1), [Some(Drop::Cliff); 4]);
    map.edit(
        TilePos { x: 1, y: 0 },
        Override {
            level: Some(2),
            ..Override::default()
        },
    )
    .expect("on the map");
    assert_eq!(
        sides(&map, 1, 1),
        [
            Some(Drop::Ledge),
            Some(Drop::Cliff),
            Some(Drop::Cliff),
            Some(Drop::Cliff)
        ]
    );
    // Water by its level, as land.
    map.edit(
        TilePos { x: 0, y: 1 },
        Override {
            terrain: Some(Terrain::DeepWater),
            level: Some(2),
            ..Override::default()
        },
    )
    .expect("on the map");
    assert_eq!(sides(&map, 1, 1)[1], Some(Drop::Ledge));
}

#[test]
fn levels_outside_their_ranges_are_refused() {
    let said = |levels| {
        rasterise(&raster(), recipe(WHOLE, 1, 7, Detail::default(), levels))
            .expect_err("refused")
            .to_string()
    };
    assert_eq!(
        said(levels(99, 4)),
        "the levels' spacing must be 1.00 to 80.00 of Azgaar's heights, not 0.99"
    );
    assert_eq!(
        said(levels(80_01, 4)),
        "the levels' spacing must be 1.00 to 80.00 of Azgaar's heights, not 80.01"
    );
    assert_eq!(
        said(levels(5_00, 4097)),
        "the levels' specks must be 0 to 4096 tiles, not 4097"
    );
    assert!(matches!(
        recipe(WHOLE, 1, 7, Detail::default(), levels(5_00, 4097)).size(),
        Err(MapError::LevelSpecks(4097))
    ));
    for edge in [levels(1_00, 0), levels(80_00, 4096)] {
        assert!(rasterise(&raster(), recipe(WHOLE, 1, 7, Detail::default(), edge)).is_ok());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn a_region_shows_the_whole_maps_levels_away_from_its_edges(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..16,
        height in 1u32..16,
        scale in 1u32..=3,
        seed in any::<u32>(),
        spacing in 1_00i64..=20_00,
        specks in 0u32..6,
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let options = levels(spacing, specks);
        let part = map(recipe([x, y, width, height], scale, seed, Detail::default(), options));
        let whole = map(recipe(WHOLE, scale, seed, Detail::default(), options));
        // A tile's terrain looks at its neighbours (W4b), so a tile further in; and a plant at
        // the candidates as far as the widest spacing reaches, 2 tiles (W5).
        let margin = 2 * Coast::default().specks + specks + 1 + 2;
        let (w, h) = (width * scale, height * scale);
        for ty in 0..h {
            for tx in 0..w {
                let (here, there) = (tile(&part, tx, ty), tile(&whole, x * scale + tx, y * scale + ty));
                prop_assert_eq!(here.elevation, there.elevation);
                let inside = tx >= margin && ty >= margin && tx + margin < w && ty + margin < h;
                if inside {
                    prop_assert_eq!(here, there);
                }
            }
        }
    }
}

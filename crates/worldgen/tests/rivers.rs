//! Rivers (W3d, P-139): Azgaar's rivers drawn on the tiles along a smooth curve, as wide as
//! Azgaar draws them and never narrower than a tile; their beds carved never to rise (W3e,
//! P-140). The expected counts were worked by a separate script from the rules in DESIGN.md
//! §21, never by this code.

use std::collections::BTreeMap;

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Drop, Drops, Feature, Levels, Point, Recipe, Region, Size, Source,
    SourceCell, SourceRiver, SourceVertex, Terrain, Tile, TilePos, Water, WorldMap, rasterise,
};
use proptest::prelude::*;

fn hundredths(value: i64) -> Fixed {
    Fixed::from_hundredths(value)
}

fn cell(x: i64, y: i64, height: u8, feature: u32, neighbours: &[u32], flux: u32) -> SourceCell {
    SourceCell {
        centre: Point {
            x: hundredths(x),
            y: hundredths(y),
        },
        height,
        feature,
        neighbours: neighbours.to_vec(),
        corners: Vec::new(),
        flux,
        biome: match height {
            20.. => Biome::Grassland,
            _ => Biome::Marine,
        },
        temperature: 0,
        precipitation: 0,
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

fn river(cells: &[u32], off_map: bool, width_factor: i64, source_width: i64) -> SourceRiver {
    SourceRiver {
        cells: cells.to_vec(),
        off_map,
        width_factor: hundredths(width_factor),
        source_width: hundredths(source_width),
    }
}

/// `rivers.json`: W3a's five cells, A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40,
/// C (10.5, 30.5) at 19 in a lake with its surface at 19.50, D (30.5, 30.5) at 60 and E
/// (20.5, 20.5) at 30, with fluxes 0, 40, 900, 20 and 2000; and three rivers: Ford from D by
/// E into the lake, Beck from B joining it at E, and Rill from B off the map.
fn rivers() -> Source {
    Source {
        name: "Rivers".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size {
            width: 40,
            height: 40,
        },
        height_exponent: hundredths(2_00),
        cells: vec![
            cell(10_50, 10_50, 10, 0, &[1, 2, 4], 0),
            cell(30_50, 10_50, 40, 2, &[0, 3, 4], 40),
            cell(10_50, 30_50, 19, 1, &[0, 3, 4], 900),
            cell(30_50, 30_50, 60, 2, &[1, 2, 4], 20),
            cell(20_50, 20_50, 30, 2, &[0, 1, 2, 3], 2000),
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
        rivers: vec![
            river(&[3, 4, 2], false, 1_20, 5),
            river(&[1, 4], false, 1_00, 4),
            river(&[1], true, 1_00, 4),
        ],
    }
}

fn recipe(region: [u32; 4], scale: u32, seed: u32, levels: Levels) -> Recipe {
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
        detail: Detail::default(),
        coast: Coast::default(),
        levels,
    }
}

const WHOLE: [u32; 4] = [0, 0, 40, 40];

fn map(source: &Source, recipe: Recipe) -> WorldMap {
    rasterise(source, recipe).expect("the recipe makes a map")
}

fn whole(seed: u32) -> WorldMap {
    map(&rivers(), recipe(WHOLE, 1, seed, Levels::default()))
}

fn tile(map: &WorldMap, x: u32, y: u32) -> Tile {
    map.tile(TilePos { x, y }).expect("on the map")
}

/// Every tile, row by row.
fn tiles(map: &WorldMap) -> Vec<Tile> {
    let size = map.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| tile(map, x, y))
        .collect()
}

/// How many river tiles have each flow.
fn flows(map: &WorldMap) -> BTreeMap<u32, usize> {
    let mut flows = BTreeMap::new();
    for tile in tiles(map) {
        if let Some(Water::River { flow }) = tile.water {
            *flows.entry(flow).or_default() += 1;
        }
    }
    flows
}

fn flow(map: &WorldMap, x: u32, y: u32) -> Option<u32> {
    match tile(map, x, y).water {
        Some(Water::River { flow }) => Some(flow),
        _ => None,
    }
}

/// How many tiles are ocean, lake, river and land.
fn kinds(map: &WorldMap) -> [usize; 4] {
    let mut counts = [0; 4];
    for tile in tiles(map) {
        counts[match (tile.terrain, tile.water) {
            (_, Some(Water::Lake)) => 1,
            (_, Some(Water::River { .. })) => 2,
            (Terrain::DeepWater | Terrain::ShallowWater, None) => 0,
            _ => 3,
        }] += 1;
    }
    counts
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
                counts[usize::from(side == Drop::Cliff)] += 1;
            }
        }
    }
    counts
}

#[test]
fn rivers_run_from_their_source_into_the_water_they_end_in() {
    let map = whole(7);
    assert_eq!(
        flows(&map),
        BTreeMap::from([(20, 19), (40, 39), (2000, 20)])
    );
    // Beck and Rill rise at B, and Rill runs off the map's right edge.
    assert_eq!(flow(&map, 30, 10), Some(40));
    assert_eq!(flow(&map, 39, 10), Some(40));
    // Ford, below E where Beck joins it, carries E's flux into the lake.
    for (x, y) in [(17, 21), (20, 20), (13, 27), (11, 29)] {
        assert_eq!(flow(&map, x, y), Some(2000), "({x}, {y})");
    }
    // Ford's curve from D to E bends away from the straight line between them.
    assert_eq!(flow(&map, 25, 25), None);
}

#[test]
fn rivers_take_land_and_leave_the_sea_and_the_lake_alone() {
    let map = whole(7);
    // W3b's 358 ocean and 186 lake; 78 of its 1056 land are river now.
    assert_eq!(kinds(&map), [358, 186, 78, 978]);
}

#[test]
fn a_river_tile_is_shallow_water_on_its_bed_never_raised() {
    let mut without = rivers();
    without.rivers.clear();
    let (with, without) = (
        whole(7),
        map(&without, recipe(WHOLE, 1, 7, Levels::default())),
    );
    let (mut seen, mut lowered, mut deepest) = (0, 0, Fixed::ZERO);
    for (here, there) in tiles(&with).into_iter().zip(tiles(&without)) {
        match here.water {
            Some(Water::River { .. }) => {
                seen += 1;
                assert_eq!(here.terrain, Terrain::ShallowWater);
                assert_eq!(here.biome, Biome::Marine);
                assert!(here.is_water());
                assert!(!there.is_water());
                assert!(here.elevation <= there.elevation);
                let cut = there.elevation - here.elevation;
                lowered += usize::from(cut > Fixed::ZERO);
                deepest = deepest.max(cut);
            }
            _ => assert_eq!(here.elevation, there.elevation),
        }
    }
    assert_eq!((seen, lowered, deepest), (78, 74, hundredths(282_93)));
}

#[test]
fn a_river_is_carved_never_to_rise_and_stands_at_its_beds_level() {
    let map = whole(7);
    let at = |x, y| {
        let tile = tile(&map, x, y);
        (tile.elevation, tile.level)
    };
    // Beck from B, down to E.
    assert_eq!(at(30, 10), (hundredths(412_08), 3));
    assert_eq!(at(39, 10), (hundredths(426_54), 3));
    // Ford, down from D and through E.
    assert_eq!(at(30, 30), (hundredths(1754_48), 7));
    assert_eq!(at(20, 20), (hundredths(73_52), 1));
    // Near the lake, held at its surface, -1.28 m, at level 0.
    assert_eq!(at(13, 27), (hundredths(-1_28), 0));
    // Land beside it is as it was.
    assert_eq!(at(25, 26), (hundredths(1015_99), 5));
}

#[test]
fn water_stands_at_the_level_of_its_surface() {
    let mut source = rivers();
    source.features[1] = Feature::Lake {
        surface: hundredths(30_00),
    };
    let map = map(&source, recipe(WHOLE, 1, 7, Levels::default()));
    // The lake's surface at 30.00 is 144 m, level 2's start, level 3's being 289 m; the sea's,
    // 0 m, is level 0.
    let mut levels = BTreeMap::new();
    for tile in tiles(&map) {
        let kind = match (tile.terrain, tile.water) {
            (_, Some(Water::Lake)) => "lake",
            (Terrain::DeepWater | Terrain::ShallowWater, None) => "sea",
            _ => continue,
        };
        *levels.entry((kind, tile.level)).or_insert(0) += 1;
    }
    assert_eq!(
        levels,
        BTreeMap::from([(("lake", 2), 341), (("sea", 0), 358)])
    );
    assert_eq!(kinds(&map), [358, 341, 64, 837]);
    assert_eq!(drops(&map), [355, 68]);
}

#[test]
fn levels_are_made_with_the_rivers_as_water() {
    let map = whole(7);
    let top = tiles(&map).iter().map(|t| t.level).max();
    assert_eq!(top, Some(8));
    // Water drops now too, by the level of its surface.
    assert_eq!(drops(&map), [447, 67]);
}

#[test]
fn rivers_widen_with_the_scale() {
    let map = map(&rivers(), recipe([10, 10, 20, 20], 4, 7, Levels::default()));
    assert_eq!(
        flows(&map),
        BTreeMap::from([(20, 135), (40, 125), (2000, 241)])
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn a_region_shows_the_whole_maps_rivers_away_from_its_edges(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..16,
        height in 1u32..16,
        scale in 1u32..=3,
        seed in any::<u32>(),
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let source = rivers();
        let part = map(&source, recipe([x, y, width, height], scale, seed, Levels::default()));
        let whole = map(&source, recipe(WHOLE, scale, seed, Levels::default()));
        // A tile's terrain looks at its neighbours (W4b), so a tile further in.
        let margin = 2 * Coast::default().specks + Levels::default().specks + 1;
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

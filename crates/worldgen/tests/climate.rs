//! Climate and biomes (W4a, P-141): each tile takes the temperature and precipitation of the
//! cell it lies in, and land the biome of the nearest land cell. The expected counts were
//! worked by a separate script from the rules in DESIGN.md §21, never by this code.

use std::collections::BTreeMap;

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Feature, Levels, Point, Recipe, Region, Size, Source, SourceCell,
    SourceRiver, SourceVertex, Tile, TilePos, Water, WorldMap, rasterise,
};
use proptest::prelude::*;

fn hundredths(value: i64) -> Fixed {
    Fixed::from_hundredths(value)
}

/// A cell of `raster.json`, with its biome and its grid cell's climate.
fn cell(
    (x, y): (i64, i64),
    height: u8,
    feature: u32,
    neighbours: &[u32],
    (biome, temperature, precipitation): (Biome, i8, u16),
) -> SourceCell {
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
        biome,
        temperature,
        precipitation,
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

/// `raster.json`: W3a's five cells, A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40,
/// C (10.5, 30.5) at 19 in a lake, D (30.5, 30.5) at 60 and E (20.5, 20.5) at 30; A and C
/// Marine, B Grassland, D Taiga and E Temperate deciduous forest; at 14, 10, 13, −3 and 8 °C,
/// with 2000, 1200, 3000, 800 and 1800 mm.
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
            cell((10_50, 10_50), 10, 0, &[1, 2, 4], (Biome::Marine, 14, 2000)),
            cell(
                (30_50, 10_50),
                40,
                2,
                &[0, 3, 4],
                (Biome::Grassland, 10, 1200),
            ),
            cell((10_50, 30_50), 19, 1, &[0, 3, 4], (Biome::Marine, 13, 3000)),
            cell((30_50, 30_50), 60, 2, &[1, 2, 4], (Biome::Taiga, -3, 800)),
            cell(
                (20_50, 20_50),
                30,
                2,
                &[0, 1, 2, 3],
                (Biome::TemperateDeciduousForest, 8, 1800),
            ),
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
    }
}

/// `rivers.json`: `raster.json` with W3d's fluxes and its three rivers.
fn rivers() -> Source {
    let mut source = raster();
    for (cell, flux) in source.cells.iter_mut().zip([0, 40, 900, 20, 2000]) {
        cell.flux = flux;
    }
    let river = |cells: &[u32], off_map, width_factor, source_width| SourceRiver {
        cells: cells.to_vec(),
        off_map,
        width_factor: hundredths(width_factor),
        source_width: hundredths(source_width),
    };
    source.name = "Rivers".to_owned();
    source.rivers = vec![
        river(&[3, 4, 2], false, 1_20, 5),
        river(&[1, 4], false, 1_00, 4),
        river(&[1], true, 1_00, 4),
    ];
    source
}

fn recipe(region: [u32; 4], scale: u32, seed: u32) -> Recipe {
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
        levels: Levels::default(),
    }
}

const WHOLE: [u32; 4] = [0, 0, 40, 40];

fn map(source: &Source, recipe: Recipe) -> WorldMap {
    rasterise(source, recipe).expect("the recipe makes a map")
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

/// How many tiles have each value of `layer`.
fn counts<T: Ord>(map: &WorldMap, layer: impl Fn(&Tile) -> T) -> BTreeMap<T, usize> {
    let mut counts = BTreeMap::new();
    for tile in tiles(map) {
        *counts.entry(layer(&tile)).or_default() += 1;
    }
    counts
}

fn biomes(map: &WorldMap) -> BTreeMap<Biome, usize> {
    counts(map, |tile| tile.biome)
}

#[test]
fn a_tile_takes_the_climate_of_the_cell_it_lies_in() {
    let map = map(&raster(), recipe(WHOLE, 1, 7));
    assert_eq!(
        counts(&map, |tile| tile.temperature),
        BTreeMap::from([(-3, 325), (8, 181), (10, 354), (13, 354), (14, 386)])
    );
    assert_eq!(
        counts(&map, |tile| tile.precipitation),
        BTreeMap::from([
            (800, 325),
            (1200, 354),
            (1800, 181),
            (2000, 386),
            (3000, 354)
        ])
    );
    let climate = |x, y| {
        let tile = tile(&map, x, y);
        (tile.temperature, tile.precipitation)
    };
    assert_eq!(climate(30, 10), (10, 1200));
    assert_eq!(climate(30, 30), (-3, 800));
    assert_eq!(climate(20, 20), (8, 1800));
    // In the sea, and on land in the lake's cell: the cell's own climate either way.
    assert_eq!(climate(10, 10), (14, 2000));
    assert_eq!(climate(10, 30), (13, 3000));
}

#[test]
fn land_takes_the_biome_of_the_nearest_land_cell_and_water_is_marine() {
    let map = map(&raster(), recipe(WHOLE, 1, 7));
    assert_eq!(
        biomes(&map),
        BTreeMap::from([
            (Biome::Marine, 544),
            (Biome::Grassland, 355),
            (Biome::TemperateDeciduousForest, 364),
            (Biome::Taiga, 337),
        ])
    );
    let biome = |x, y| tile(&map, x, y).biome;
    assert_eq!(biome(30, 10), Biome::Grassland);
    assert_eq!(biome(30, 30), Biome::Taiga);
    assert_eq!(biome(20, 20), Biome::TemperateDeciduousForest);
    // Land in the lake's cell, C, takes E's biome, the nearest land cell's.
    assert!(!tile(&map, 10, 30).is_water());
    assert_eq!(biome(10, 30), Biome::TemperateDeciduousForest);
    for tile in tiles(&map) {
        assert_eq!(tile.is_water(), tile.biome == Biome::Marine, "{tile:?}");
    }
}

#[test]
fn the_biomes_follow_the_seed_and_the_region() {
    assert_eq!(
        biomes(&map(&raster(), recipe(WHOLE, 1, 0))),
        BTreeMap::from([
            (Biome::Marine, 531),
            (Biome::Grassland, 355),
            (Biome::TemperateDeciduousForest, 357),
            (Biome::Taiga, 357),
        ])
    );
    assert_eq!(
        biomes(&map(&raster(), recipe([10, 10, 20, 20], 2, 7))),
        BTreeMap::from([
            (Biome::Marine, 224),
            (Biome::Grassland, 209),
            (Biome::TemperateDeciduousForest, 996),
            (Biome::Taiga, 171),
        ])
    );
}

#[test]
fn rivers_are_marine() {
    let map = map(&rivers(), recipe(WHOLE, 1, 7));
    assert_eq!(
        biomes(&map),
        BTreeMap::from([
            (Biome::Marine, 622),
            (Biome::Grassland, 330),
            (Biome::TemperateDeciduousForest, 322),
            (Biome::Taiga, 326),
        ])
    );
    for tile in tiles(&map) {
        if let Some(Water::River { .. }) = tile.water {
            assert_eq!(tile.biome, Biome::Marine);
        }
    }
}

#[test]
fn with_no_land_cell_land_is_marine() {
    let mut source = raster();
    for cell in &mut source.cells {
        cell.biome = Biome::Marine;
    }
    let map = map(&source, recipe(WHOLE, 1, 7));
    assert_eq!(biomes(&map), BTreeMap::from([(Biome::Marine, 1600)]));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn a_region_shows_the_whole_maps_climate_and_biomes_away_from_its_edges(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..16,
        height in 1u32..16,
        scale in 1u32..=3,
        seed in any::<u32>(),
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let source = rivers();
        let part = map(&source, recipe([x, y, width, height], scale, seed));
        let whole = map(&source, recipe(WHOLE, scale, seed));
        let margin = 2 * Coast::default().specks + Levels::default().specks;
        let (w, h) = (width * scale, height * scale);
        for ty in 0..h {
            for tx in 0..w {
                let (here, there) = (tile(&part, tx, ty), tile(&whole, x * scale + tx, y * scale + ty));
                prop_assert_eq!(
                    (here.temperature, here.precipitation),
                    (there.temperature, there.precipitation)
                );
                let inside = tx >= margin && ty >= margin && tx + margin < w && ty + margin < h;
                if inside {
                    prop_assert_eq!(here, there);
                }
            }
        }
    }
}

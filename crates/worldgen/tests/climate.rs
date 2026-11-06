//! Climate and biomes (W4a, P-141): each tile takes the temperature and precipitation of the
//! cell it lies in, and land the biome of the nearest land cell; and terrain (W4b, P-142), land's
//! from its biome, rock at a cliff and sand by the sea. The expected counts were worked by a
//! separate script from the rules in DESIGN.md §21, never by this code.

use std::collections::BTreeMap;

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Feature, Foliage, Levels, MapError, PerKind, Plant, PlantKind, Point,
    Recipe, Region, Size, Source, SourceCell, SourceRiver, SourceVertex, Terrain, Tile, TilePos,
    Water, WorldMap, rasterise,
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
        settlements: Vec::new(),
        routes: Vec::new(),
        markers: Vec::new(),
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
        foliage: Foliage::default(),
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

fn terrains(map: &WorldMap) -> BTreeMap<Terrain, usize> {
    counts(map, |tile| tile.terrain)
}

#[test]
fn land_takes_its_biomes_terrain_rock_at_a_cliff_and_sand_by_the_sea() {
    let map = map(&raster(), recipe(WHOLE, 1, 7));
    assert_eq!(
        terrains(&map),
        BTreeMap::from([
            (Terrain::DeepWater, 346),
            (Terrain::ShallowWater, 198),
            (Terrain::Sand, 23),
            (Terrain::Grass, 338),
            (Terrain::ForestFloor, 644),
            (Terrain::Rock, 51),
        ])
    );
    // Grassland, Taiga and temperate deciduous forest away from the sea and cliffs.
    let terrain = |x, y| tile(&map, x, y).terrain;
    assert_eq!(terrain(30, 10), Terrain::Grass);
    assert_eq!(terrain(30, 30), Terrain::ForestFloor);
    assert_eq!(terrain(20, 20), Terrain::ForestFloor);
    // The first rock, sand and forest floor, row by row.
    assert_eq!(terrain(21, 0), Terrain::Rock);
    assert_eq!(terrain(11, 10), Terrain::Sand);
    assert_eq!(terrain(14, 11), Terrain::ForestFloor);
}

#[test]
fn the_terrain_follows_the_seed_the_region_and_the_rivers() {
    let terrains_of = |source: &Source, region, scale, seed| {
        let counts = terrains(&map(source, recipe(region, scale, seed)));
        [
            Terrain::DeepWater,
            Terrain::ShallowWater,
            Terrain::ForestFloor,
            Terrain::Grass,
            Terrain::Rock,
            Terrain::Sand,
        ]
        .map(|terrain| counts.get(&terrain).copied().unwrap_or(0))
    };
    assert_eq!(
        terrains_of(&raster(), WHOLE, 1, 0),
        [348, 183, 661, 339, 47, 22]
    );
    assert_eq!(
        terrains_of(&raster(), [10, 10, 20, 20], 2, 7),
        [150, 74, 1125, 204, 13, 34]
    );
    assert_eq!(
        terrains_of(&rivers(), WHOLE, 1, 7),
        [346, 276, 583, 312, 60, 23]
    );
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
        // A tile's terrain looks at its neighbours, so one tile further in; and a plant at the
        // candidates as far as the widest spacing reaches, rounded up (W5).
        let margin = 2 * Coast::default().specks + Levels::default().specks + 1 + 2;
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

/// How many plants of each kind: trees, shrubs and grass.
fn plants(map: &WorldMap) -> [usize; 3] {
    let kinds: Vec<PlantKind> = tiles(map)
        .iter()
        .filter_map(|t| t.plant.map(|p| p.kind))
        .collect();
    PlantKind::ALL.map(|kind| kinds.iter().filter(|&&k| k == kind).count())
}

/// Every plant, row by row: where it stands, in 256ths of a tile from the map's top left, and
/// its kind.
fn standing(map: &WorldMap) -> Vec<([i64; 2], PlantKind)> {
    let size = map.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let plant = tile(map, x, y).plant?;
            let at = |tile: u32, offset: u8| i64::from(tile) * 256 + i64::from(offset);
            Some(([at(x, plant.offset[0]), at(y, plant.offset[1])], plant.kind))
        })
        .collect()
}

#[test]
fn plants_scatter_by_their_biomes_chances_spaced_by_kind() {
    let map = map(&raster(), recipe(WHOLE, 1, 7));
    // From foliage_ref.py: 157 plants of 615 candidates.
    assert_eq!(plants(&map), [47, 15, 95]);
    let offsets: u32 = tiles(&map)
        .iter()
        .filter_map(|t| t.plant)
        .map(|p| u32::from(p.offset[0]) + 2 * u32::from(p.offset[1]))
        .sum();
    assert_eq!(offsets, 58_945);
    // The first of each kind, row by row: on grassland, then forest.
    let plant = |x, y| tile(&map, x, y).plant;
    let of = |kind, offset| Some(Plant { kind, offset });
    assert_eq!(plant(25, 0), of(PlantKind::Grass, [96, 38]));
    assert_eq!(plant(34, 3), of(PlantKind::Shrub, [37, 251]));
    assert_eq!(plant(15, 11), of(PlantKind::Tree, [41, 92]));
    // Grass on (24, 0) and (26, 0) were candidates, too close to (25, 0)'s and outranked.
    assert_eq!((plant(24, 0), plant(26, 0)), (None, None));
}

#[test]
fn the_plants_follow_the_seed_the_region_the_rivers_the_scale_and_the_spacing() {
    let raster = raster();
    let with = |change: fn(&mut Foliage)| {
        let mut recipe = recipe(WHOLE, 1, 7);
        change(&mut recipe.foliage);
        plants(&map(&raster, recipe))
    };
    assert_eq!(plants(&map(&raster, recipe(WHOLE, 1, 0))), [51, 24, 91]);
    assert_eq!(
        plants(&map(&raster, recipe([10, 10, 20, 20], 2, 7))),
        [62, 23, 73]
    );
    assert_eq!(plants(&map(&rivers(), recipe(WHOLE, 1, 7))), [47, 16, 90]);
    assert_eq!(with(|f| f.scale = hundredths(2_00)), [57, 12, 90]);
    assert_eq!(
        with(|f| f.spacing = PerKind {
            tree: Fixed::ONE,
            shrub: Fixed::ONE,
            grass: Fixed::ONE
        }),
        [167, 54, 129]
    );
    assert_eq!(with(|f| f.scale = Fixed::ZERO), [0, 0, 0]);
}

#[test]
fn foliage_outside_its_ranges_is_refused() {
    let said = |change: fn(&mut Foliage)| {
        let mut recipe = recipe(WHOLE, 1, 7);
        change(&mut recipe.foliage);
        rasterise(&raster(), recipe)
            .expect_err("refused")
            .to_string()
    };
    assert_eq!(
        said(|f| f.scale = hundredths(10_01)),
        "the foliage's scale must be 0.00 to 10.00, not 10.01"
    );
    assert_eq!(
        said(|f| f.scale = hundredths(-1)),
        "the foliage's scale must be 0.00 to 10.00, not -0.01"
    );
    assert_eq!(
        said(|f| f.spacing.tree = hundredths(8_01)),
        "the foliage's tree spacing must be 0.00 to 8.00 tiles, not 8.01"
    );
    assert_eq!(
        said(|f| f.spacing.shrub = hundredths(-1)),
        "the foliage's shrub spacing must be 0.00 to 8.00 tiles, not -0.01"
    );
    assert_eq!(
        said(|f| f.spacing.grass = hundredths(8_01)),
        "the foliage's grass spacing must be 0.00 to 8.00 tiles, not 8.01"
    );
    assert_eq!(
        said(|f| f.chances[Biome::HotDesert as usize].tree = hundredths(1_01)),
        "the foliage's chance of a tree in hot desert must be 0.00 to 1.00, not 1.01"
    );
    assert_eq!(
        said(|f| f.chances[Biome::Wetland as usize].shrub = hundredths(-1)),
        "the foliage's chance of a shrub in wetland must be 0.00 to 1.00, not -0.01"
    );
    assert_eq!(
        said(|f| f.chances[Biome::Marine as usize].grass = hundredths(1_01)),
        "the foliage's chance of grass in marine must be 0.00 to 1.00, not 1.01"
    );
    assert_eq!(
        said(|f| f.chances[Biome::Taiga as usize] = PerKind {
            tree: hundredths(50),
            shrub: hundredths(30),
            grass: hundredths(21)
        }),
        "the foliage's chances in taiga must add up to 1.00 at most, not 1.01"
    );
    assert!(matches!(
        {
            let mut recipe = recipe(WHOLE, 1, 7);
            recipe.foliage.spacing.tree = hundredths(8_01);
            recipe.size()
        },
        Err(MapError::PlantSpacing(PlantKind::Tree, _))
    ));
    // The edges of every range are allowed: each chance at 1.00 alone, and summing to 1.00.
    let edges: [fn(&mut Foliage); 4] = [
        |f| {
            f.scale = hundredths(10_00);
            f.spacing = PerKind {
                tree: hundredths(8_00),
                shrub: hundredths(8_00),
                grass: hundredths(8_00),
            };
        },
        |f| {
            f.scale = Fixed::ZERO;
            f.spacing = PerKind {
                tree: Fixed::ZERO,
                shrub: Fixed::ZERO,
                grass: Fixed::ZERO,
            };
        },
        |f| {
            f.chances = [PerKind {
                tree: Fixed::ONE,
                shrub: Fixed::ZERO,
                grass: Fixed::ZERO,
            }; 13]
        },
        |f| {
            f.chances = [PerKind {
                tree: hundredths(20),
                shrub: hundredths(30),
                grass: hundredths(50),
            }; 13]
        },
    ];
    for edge in edges {
        let mut recipe = recipe(WHOLE, 1, 7);
        edge(&mut recipe.foliage);
        assert!(rasterise(&raster(), recipe).is_ok());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// With every biome given a chance of each kind, no plant stands on water, rock or ice,
    /// and no two stand nearer than the larger of their spacings, checked pair by pair.
    #[test]
    fn plants_stand_apart_and_never_on_water_rock_or_ice(
        seed in any::<u32>(),
        rivers_too in any::<bool>(),
        scale in 1u32..=2,
        spacing in prop::array::uniform3(0i64..=300),
    ) {
        let mut recipe = recipe(WHOLE, scale, seed);
        let all = PerKind { tree: hundredths(30), shrub: hundredths(30), grass: hundredths(30) };
        recipe.foliage.chances = [all; 13];
        recipe.foliage.spacing = PerKind {
            tree: hundredths(spacing[0]),
            shrub: hundredths(spacing[1]),
            grass: hundredths(spacing[2]),
        };
        let source = if rivers_too { rivers() } else { raster() };
        let map = map(&source, recipe.clone());
        for t in tiles(&map) {
            let bare = t.water.is_some()
                || matches!(t.terrain, Terrain::DeepWater | Terrain::ShallowWater | Terrain::Rock | Terrain::Ice);
            prop_assert!(!(bare && t.plant.is_some()), "{t:?}");
        }
        let plants = standing(&map);
        // Enough plants that the spacing is tested, even at the widest.
        prop_assert!(plants.len() > 30, "{}", plants.len());
        let reach = |kind: PlantKind| recipe.foliage.spacing.of_kind(kind).hundredths();
        for (i, &(a, first)) in plants.iter().enumerate() {
            for &(b, second) in &plants[i + 1..] {
                let squared = (a[0] - b[0]).pow(2) + (a[1] - b[1]).pow(2);
                let spacing = reach(first).max(reach(second));
                prop_assert!(squared * 10_000 >= (spacing * 256).pow(2), "{a:?} {b:?}");
            }
        }
    }
}

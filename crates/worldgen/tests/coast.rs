//! Coasts (W3b, P-137): land and water by each tile's height against the water near it, the
//! sea's shallows by depth, and specks removed. The expected counts and heights were worked by
//! a separate script from the rules in DESIGN.md §21, never by this code.

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Feature, Levels, MapError, Point, Recipe, Region, Size, Source,
    SourceCell, SourceVertex, Terrain, Tile, TilePos, Water, WorldMap, height_metres, rasterise,
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

fn coast(shallows: i64, specks: u32) -> Coast {
    Coast {
        shallows: hundredths(shallows),
        specks,
    }
}

fn recipe(region: [u32; 4], scale: u32, seed: u32, detail: Detail, coast: Coast) -> Recipe {
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
        coast,
        levels: Levels::default(),
    }
}

const WHOLE: [u32; 4] = [0, 0, 40, 40];

fn map(recipe: Recipe) -> WorldMap {
    rasterise(&raster(), recipe).expect("the recipe makes a map")
}

fn whole(seed: u32, detail: Detail, coast: Coast) -> WorldMap {
    map(recipe(WHOLE, 1, seed, detail, coast))
}

fn tile(map: &WorldMap, x: u32, y: u32) -> Tile {
    map.tile(TilePos { x, y }).expect("on the map")
}

/// What a tile is: deep or shallow sea, lake or land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Deep,
    Shallow,
    Lake,
    Land,
}

/// What `tile` is, and its height; or a panic if its layers don't agree on what it is.
fn kind(tile: Tile) -> (Kind, Fixed) {
    let kind = match (tile.terrain, tile.water, tile.biome) {
        (Terrain::DeepWater, None, Biome::Marine) => Kind::Deep,
        (Terrain::ShallowWater, None, Biome::Marine) => Kind::Shallow,
        (Terrain::ShallowWater, Some(Water::Lake), Biome::Marine) => Kind::Lake,
        // Land's terrain is its biome's, or sand by the sea or rock at a cliff (W4b).
        (Terrain::Grass | Terrain::Sand | Terrain::Rock, None, Biome::Grassland) => Kind::Land,
        layers => panic!("no kind of tile has the layers {layers:?}"),
    };
    assert_eq!(tile.plant, None);
    (kind, tile.elevation)
}

fn at(map: &WorldMap, x: u32, y: u32) -> (Kind, Fixed) {
    kind(tile(map, x, y))
}

/// Every tile, row by row.
fn tiles(map: &WorldMap) -> Vec<Tile> {
    let size = map.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| tile(map, x, y))
        .collect()
}

/// How many tiles are deep, shallow, lake and land.
fn counts(map: &WorldMap) -> [usize; 4] {
    let kinds: Vec<Kind> = tiles(map).into_iter().map(|t| kind(t).0).collect();
    [Kind::Deep, Kind::Shallow, Kind::Lake, Kind::Land]
        .map(|of| kinds.iter().filter(|&&k| k == of).count())
}

#[test]
fn a_lakes_surface_is_in_metres_by_azgaars_formula() {
    let two = hundredths(2_00);
    for (height, metres) in [
        (19_90, -25),
        (20_90, 8_41),
        (21_90, 15_21),
        (23_90, 34_81),
        (19_50, -1_28),
    ] {
        assert_eq!(height_metres(hundredths(height), two), hundredths(metres));
    }
    assert_eq!(
        height_metres(hundredths(21_90), hundredths(1_50)),
        hundredths(7_70)
    );
    // A whole height gives what a cell at it gives: 50 × (5 − 20) / 5, 22², and −990 at 0;
    // and a height is held to Azgaar's scale, 82² at its top.
    for (height, metres) in [
        (5_00, -150_00),
        (40_00, 484_00),
        (0, -990_00),
        (-1, -990_00),
        (150_00, 6724_00),
    ] {
        assert_eq!(height_metres(hundredths(height), two), hundredths(metres));
    }
}

#[test]
fn the_coast_defaults_to_shallows_of_ten_metres_and_specks_under_four_tiles() {
    assert_eq!(Coast::default(), coast(10_00, 4));
}

#[test]
fn land_and_water_follow_the_ground_not_the_nearest_cell() {
    let flat = whole(7, FLAT, Coast::default());
    // Nearest A, the ocean, but above the sea.
    assert_eq!(at(&flat, 13, 13), (Kind::Land, hundredths(8_20)));
    // Nearest C, below the lake's surface of −1.28.
    assert_eq!(at(&flat, 10, 30), (Kind::Lake, hundredths(-2_63)));
    let detailed = whole(7, Detail::default(), Coast::default());
    // Nearest C, but above its surface.
    assert_eq!(at(&detailed, 10, 30), (Kind::Land, hundredths(82_93)));
    assert_eq!(at(&detailed, 13, 13), (Kind::Deep, hundredths(-28_97)));
}

#[test]
fn a_tile_at_its_waters_surface_is_land() {
    // C's surface at 19.00 is −2.63 m, C's own height: the tiles that take it, nearest C and
    // outside every triangle, are land.
    let mut source = raster();
    source.features[1] = Feature::Lake {
        surface: hundredths(19_00),
    };
    let level = rasterise(&source, recipe(WHOLE, 1, 7, FLAT, Coast::default())).expect("a map");
    assert_eq!(at(&level, 10, 30), (Kind::Land, hundredths(-2_63)));
    assert_eq!(counts(&level), [342, 6, 11, 1241]);
}

#[test]
fn a_tiles_water_is_the_nearest_among_its_nearest_cell_and_its_neighbours() {
    // Nearest E, an island; C is nearer than A, so the lake.
    let detailed = whole(7, Detail::default(), Coast::default());
    assert_eq!(at(&detailed, 12, 21), (Kind::Lake, hundredths(-3_99)));
    assert_eq!(at(&detailed, 12, 20), (Kind::Deep, hundredths(-16_01)));
    // Nearest E; A and C are as near, and A comes first, so the sea.
    let flat = whole(7, FLAT, Coast::default());
    assert_eq!(at(&flat, 11, 20), (Kind::Shallow, hundredths(-9_28)));
}

#[test]
fn sea_less_deep_than_the_shallows_is_shallow() {
    let (x, y) = (12, 12);
    assert_eq!(
        at(&whole(7, FLAT, Coast::default()), x, y),
        (Kind::Deep, hundredths(-11_20))
    );
    assert_eq!(at(&whole(7, FLAT, coast(11_20, 4)), x, y).0, Kind::Deep);
    assert_eq!(at(&whole(7, FLAT, coast(11_21, 4)), x, y).0, Kind::Shallow);
    assert_eq!(counts(&whole(7, FLAT, coast(12_00, 4))), [340, 8, 302, 950]);
}

#[test]
fn the_whole_map_matches_the_separate_script() {
    assert_eq!(
        counts(&whole(7, Detail::default(), Coast::default())),
        [346, 12, 186, 1056]
    );
    assert_eq!(
        counts(&whole(0, Detail::default(), Coast::default())),
        [348, 9, 174, 1069]
    );
    assert_eq!(
        counts(&whole(7, FLAT, Coast::default())),
        [342, 6, 302, 950]
    );
    let region = map(recipe(
        [10, 10, 20, 20],
        2,
        7,
        Detail::default(),
        Coast::default(),
    ));
    assert_eq!(counts(&region), [150, 16, 58, 1376]);
    assert_eq!(
        at(&whole(7, Detail::default(), Coast::default()), 0, 0),
        (Kind::Deep, hundredths(-49_90))
    );
}

#[test]
fn specks_under_the_size_become_what_surrounds_them() {
    let rough = detail(1_00, 3_00, 3);
    let kept = whole(7, rough, coast(10_00, 0));
    let removed = whole(7, rough, coast(10_00, 4));
    assert_eq!(counts(&kept), [262, 12, 163, 1163]);
    assert_eq!(counts(&removed), [251, 23, 161, 1165]);
    // Heights never change.
    let heights =
        |map: &WorldMap| -> Vec<Fixed> { tiles(map).iter().map(|t| t.elevation).collect() };
    assert_eq!(heights(&kept), heights(&removed));
}

#[test]
fn a_coast_outside_its_ranges_is_refused() {
    let said = |coast| {
        rasterise(&raster(), recipe(WHOLE, 1, 7, Detail::default(), coast))
            .expect_err("refused")
            .to_string()
    };
    assert_eq!(
        said(coast(1000_01, 4)),
        "the coast's shallows must be 0.00 to 1000.00 metres, not 1000.01"
    );
    assert_eq!(
        said(coast(-1, 4)),
        "the coast's shallows must be 0.00 to 1000.00 metres, not -0.01"
    );
    assert_eq!(
        said(coast(10_00, 4097)),
        "the coast's specks must be 0 to 4096 tiles, not 4097"
    );
    assert!(matches!(
        recipe(WHOLE, 1, 7, Detail::default(), coast(10_00, 4097)).size(),
        Err(MapError::CoastSpecks(4097))
    ));
    for edge in [coast(0, 0), coast(1000_00, 4096)] {
        assert!(rasterise(&raster(), recipe(WHOLE, 1, 7, Detail::default(), edge)).is_ok());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn a_region_shows_the_whole_maps_tiles_away_from_its_edges(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..16,
        height in 1u32..16,
        scale in 1u32..=3,
        seed in any::<u32>(),
        specks in 0u32..6,
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let options = (Detail::default(), coast(10_00, specks));
        // No level specks, so that each tile's level is its own height's (tested in levels.rs).
        let flat = |mut recipe: Recipe| {
            recipe.levels.specks = 0;
            map(recipe)
        };
        let part = flat(recipe([x, y, width, height], scale, seed, options.0, options.1));
        let whole = flat(recipe(WHOLE, scale, seed, options.0, options.1));
        // A tile's terrain looks at its neighbours (W4b), so a tile further in.
        let margin = 2 * specks + 1;
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

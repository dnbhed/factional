//! Detail (W3a, P-136): seeded integer gradient noise in octaves, its strength a share of the
//! local relief, added to each tile's interpolated height and rounded once. The expected heights
//! were worked by a separate script from the formulas in DESIGN.md §21, never by this code.

use factional_core::Fixed;
use factional_worldgen::{
    Coast, Detail, Feature, Levels, MapError, Point, Recipe, Region, Size, Source, SourceCell,
    SourceVertex, TilePos, WorldMap, cell_reliefs, rasterise,
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

/// W2b's five cells, A to E, each with its neighbours, and the triangles ABE, BDE, DCE, CAE.
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

fn recipe(region: [u32; 4], scale: u32, seed: u32, detail: Detail) -> Recipe {
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
        levels: Levels::default(),
    }
}

const WHOLE: [u32; 4] = [0, 0, 40, 40];

fn map(recipe: Recipe) -> WorldMap {
    rasterise(&raster(), recipe).expect("the recipe makes a map")
}

fn elevation(map: &WorldMap, x: u32, y: u32) -> Fixed {
    map.tile(TilePos { x, y }).expect("on the map").elevation
}

/// Every tile's elevation, row by row.
fn elevations(map: &WorldMap) -> Vec<Fixed> {
    let size = map.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| elevation(map, x, y))
        .collect()
}

#[test]
fn the_detail_defaults_to_a_quarter_of_the_relief_over_eight_pixels_in_four_octaves() {
    assert_eq!(Detail::default(), detail(25, 8_00, 4));
}

#[test]
fn relief_is_each_cells_steepest_difference_to_a_neighbour() {
    assert_eq!(
        cell_reliefs(&raster()),
        [534_00, 1280_00, 1766_63, 1766_63, 1620_00].map(hundredths)
    );
    let mut alone = raster();
    alone.cells[4].neighbours.clear();
    assert_eq!(cell_reliefs(&alone)[4], Fixed::ZERO);
}

#[test]
fn detail_adds_strength_times_relief_times_noise_rounded_once() {
    let detailed = map(recipe(WHOLE, 1, 7, Detail::default()));
    assert_eq!(elevation(&detailed, 20, 15), hundredths(159_63));
    assert_eq!(elevation(&detailed, 20, 20), hundredths(104_05));
    assert_eq!(elevation(&detailed, 0, 0), hundredths(-49_90));
}

#[test]
fn the_seed_picks_the_noise() {
    let detailed = map(recipe(WHOLE, 1, 0, Detail::default()));
    assert_eq!(elevation(&detailed, 20, 15), hundredths(265_49));
}

#[test]
fn the_options_set_the_strength_wavelength_and_octaves() {
    let detailed = map(recipe([10, 10, 20, 20], 2, 7, detail(1_00, 3_00, 2)));
    assert_eq!(elevation(&detailed, 20, 18), hundredths(-420_30));
}

#[test]
fn the_noise_is_zero_on_its_lattice() {
    // (1.5, 1.5) is on the lattice of a wavelength of 1.50 pixels and of every octave's after.
    let detailed = map(recipe(WHOLE, 1, 7, detail(1_00, 1_50, 4)));
    assert_eq!(elevation(&detailed, 1, 1), hundredths(-50_00));
}

#[test]
fn the_whole_map_matches_the_separate_script() {
    let all = elevations(&map(recipe(WHOLE, 1, 7, Detail::default())));
    let sum: i64 = all.iter().map(|e| e.hundredths()).sum();
    assert_eq!(sum, 766_393_73);
    assert_eq!(all.iter().min(), Some(&hundredths(-176_93)));
    assert_eq!(all.iter().max(), Some(&hundredths(1889_31)));
}

#[test]
fn no_strength_gives_the_undetailed_map() {
    let flat = map(recipe(WHOLE, 1, 7, detail(0, 8_00, 4)));
    assert_eq!(elevation(&flat, 20, 15), hundredths(180_50));
    assert_eq!(elevation(&flat, 0, 0), hundredths(-50_00));
}

#[test]
fn detail_outside_its_ranges_is_refused() {
    let refused = |detail| rasterise(&raster(), recipe(WHOLE, 1, 7, detail)).expect_err("refused");
    let said = |detail| refused(detail).to_string();
    assert_eq!(
        said(detail(1_50, 8_00, 4)),
        "the detail's strength must be 0.00 to 1.00, not 1.50"
    );
    assert_eq!(
        said(detail(-1, 8_00, 4)),
        "the detail's strength must be 0.00 to 1.00, not -0.01"
    );
    assert_eq!(
        said(detail(25, 99, 4)),
        "the detail's wavelength must be at least 1.00 source pixel, not 0.99"
    );
    assert_eq!(
        said(detail(25, 8_00, 9)),
        "the detail needs 1 to 8 octaves, not 9"
    );
    assert_eq!(
        said(detail(25, 8_00, 0)),
        "the detail needs 1 to 8 octaves, not 0"
    );
    assert!(matches!(
        recipe(WHOLE, 1, 7, detail(25, 8_00, 0)).size(),
        Err(MapError::DetailOctaves(0))
    ));
    for edge in [detail(0, 1_00, 1), detail(1_00, 1_00, 8)] {
        assert!(rasterise(&raster(), recipe(WHOLE, 1, 7, edge)).is_ok());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn detail_stays_within_strength_times_relief(
        seed in any::<u32>(),
        strength in 0i64..=1_00,
        wavelength in 1_00i64..20_00,
        octaves in 1u32..=8,
    ) {
        let options = detail(strength, wavelength, octaves);
        let detailed = elevations(&map(recipe(WHOLE, 1, seed, options)));
        let flat = elevations(&map(recipe(WHOLE, 1, seed, detail(0, 8_00, 1))));
        let most = cell_reliefs(&raster()).into_iter().max().expect("cells");
        let bound = (hundredths(strength) * most).hundredths() + 1;
        for (detailed, flat) in detailed.iter().zip(&flat) {
            prop_assert!((*detailed - *flat).hundredths().abs() <= bound);
        }
    }

    #[test]
    fn a_region_shows_the_whole_maps_tiles_at_its_scale(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..12,
        height in 1u32..12,
        scale in 1u32..=3,
        seed in any::<u32>(),
    ) {
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let part = map(recipe([x, y, width, height], scale, seed, Detail::default()));
        let whole = map(recipe(WHOLE, scale, seed, Detail::default()));
        for ty in 0..height * scale {
            for tx in 0..width * scale {
                prop_assert_eq!(
                    elevation(&part, tx, ty),
                    elevation(&whole, x * scale + tx, y * scale + ty)
                );
            }
        }
        prop_assert_eq!(map(recipe([x, y, width, height], scale, seed, Detail::default())), part);
    }
}

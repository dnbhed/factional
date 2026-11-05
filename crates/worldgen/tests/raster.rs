//! Rasterising a source (W2b, P-135): a region at a scale made into tiles, each tile's height
//! interpolated across the triangle of cell centres it falls in, in metres by Azgaar's own
//! formula; and the sea, a lake or land under it (W3b's rules, tested in `coast.rs`). Every
//! expected value here is worked by hand from the formulas in DESIGN.md §21.

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Feature, HEIGHT_EXPONENTS, Point, RasterError, Recipe, Region, Size,
    Source, SourceCell, SourceVertex, Terrain, TilePos, Water, cell_metres, rasterise,
};
use proptest::prelude::*;

fn hundredths(value: i64) -> Fixed {
    Fixed::from_hundredths(value)
}

fn point(x: i64, y: i64) -> Point {
    Point {
        x: hundredths(x),
        y: hundredths(y),
    }
}

fn cell(centre: Point, height: u8, feature: u32) -> SourceCell {
    SourceCell {
        centre,
        height,
        feature,
        neighbours: Vec::new(),
        corners: Vec::new(),
    }
}

fn vertex(cells: [Option<u32>; 3]) -> SourceVertex {
    SourceVertex {
        at: point(0, 0),
        cells,
    }
}

/// 40 × 40 pixels: A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40, C (10.5, 30.5)
/// at 19 in a lake, D (30.5, 30.5) at 60 and E (20.5, 20.5) at 30, the last three on an
/// island; the triangles ABE, BDE and DCE, and ACE turning the other way; a vertex where A, E
/// and D meet in a line, which makes no triangle; and two vertices at the map's edge.
fn raster(exponent: i64) -> Source {
    Source {
        name: "Raster".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size {
            width: 40,
            height: 40,
        },
        height_exponent: hundredths(exponent),
        cells: vec![
            cell(point(10_50, 10_50), 10, 0),
            cell(point(30_50, 10_50), 40, 2),
            cell(point(10_50, 30_50), 19, 1),
            cell(point(30_50, 30_50), 60, 2),
            cell(point(20_50, 20_50), 30, 2),
        ],
        vertices: vec![
            vertex([Some(0), Some(1), Some(4)]),
            vertex([Some(1), Some(3), Some(4)]),
            vertex([Some(3), Some(2), Some(4)]),
            vertex([Some(0), Some(2), Some(4)]),
            vertex([Some(0), Some(4), Some(3)]),
            vertex([Some(0), None, None]),
            vertex([Some(1), Some(3), None]),
        ],
        features: vec![
            Feature::Ocean,
            Feature::Lake {
                surface: hundredths(19_50),
            },
            Feature::Island,
        ],
    }
}

fn recipe(x: u32, y: u32, width: u32, height: u32, scale: u32) -> Recipe {
    Recipe {
        source: "fnv1a64:0000000000000000".to_owned(),
        region: Region {
            x,
            y,
            size: Size { width, height },
        },
        scale,
        seed: 0,
        // No detail, so heights are W2b's.
        detail: Detail {
            strength: Fixed::ZERO,
            ..Detail::default()
        },
        coast: Coast::default(),
    }
}

fn whole() -> Recipe {
    recipe(0, 0, 40, 40, 1)
}

fn elevation(source: &Source, recipe: Recipe, x: u32, y: u32) -> Fixed {
    rasterise(source, recipe)
        .expect("the recipe makes a map")
        .tile(TilePos { x, y })
        .expect("on the map")
        .elevation
}

#[test]
fn metres_follow_azgaars_formula_with_a_whole_exponent() {
    let two = hundredths(2_00);
    assert_eq!(cell_metres(10, two), hundredths(-50_00));
    assert_eq!(cell_metres(40, two), hundredths(484_00));
    assert_eq!(cell_metres(19, two), hundredths(-2_63));
    assert_eq!(cell_metres(60, two), hundredths(1764_00));
    assert_eq!(cell_metres(30, two), hundredths(144_00));
    assert_eq!(cell_metres(20, two), hundredths(4_00));
    assert_eq!(cell_metres(1, two), hundredths(-950_00));
    assert_eq!(cell_metres(0, two), hundredths(-990_00));
}

#[test]
fn a_fractional_exponent_is_computed_within_a_centimetre() {
    let one_and_a_half = hundredths(1_50);
    assert_eq!(cell_metres(40, one_and_a_half), hundredths(103_19));
    assert_eq!(cell_metres(60, one_and_a_half), hundredths(272_19));
    assert_eq!(cell_metres(30, one_and_a_half), hundredths(41_57));
    assert_eq!(cell_metres(20, one_and_a_half), hundredths(2_83));
    assert_eq!(cell_metres(40, hundredths(2_20)), hundredths(898_11));
    assert_eq!(cell_metres(100, hundredths(2_20)), hundredths(16232_69));
    // Under water, the exponent plays no part.
    assert_eq!(cell_metres(10, hundredths(2_20)), hundredths(-50_00));
}

#[test]
fn an_exponent_outside_azgaars_range_is_held_to_it() {
    assert_eq!(cell_metres(40, hundredths(3_00)), hundredths(898_11));
    assert_eq!(cell_metres(40, hundredths(1_00)), hundredths(103_19));
    assert_eq!(HEIGHT_EXPONENTS, [hundredths(1_50), hundredths(2_20)]);
}

#[test]
fn a_tile_at_a_cells_centre_has_its_height() {
    let source = raster(2_00);
    assert_eq!(elevation(&source, whole(), 20, 20), hundredths(144_00));
    assert_eq!(elevation(&source, whole(), 10, 30), hundredths(-2_63));
    assert_eq!(elevation(&source, whole(), 30, 30), hundredths(1764_00));
}

#[test]
fn a_tile_inside_a_triangle_is_weighted_by_its_barycentric_coordinates() {
    // (20.5, 15.5) in ABE: a quarter of A and of B, half of E.
    assert_eq!(
        elevation(&raster(2_00), whole(), 20, 15),
        hundredths(180_50)
    );
}

#[test]
fn a_tile_on_an_edge_is_weighted_along_it() {
    // (15.5, 15.5), halfway along AE, which ABE and ACE share: half of A and half of E,
    // where the nearest cell, A, would give -50.00.
    assert_eq!(elevation(&raster(2_00), whole(), 15, 15), hundredths(47_00));
}

#[test]
fn a_triangle_turning_either_way_is_weighted_alike() {
    // (15.5, 20.5) in ACE: a quarter of A and of C, half of E.
    assert_eq!(elevation(&raster(2_00), whole(), 15, 20), hundredths(58_84));
}

#[test]
fn a_tile_outside_every_triangle_takes_the_nearest_cells_height() {
    let source = raster(2_00);
    assert_eq!(elevation(&source, whole(), 0, 0), hundredths(-50_00));
    assert_eq!(elevation(&source, whole(), 39, 39), hundredths(1764_00));
}

#[test]
fn a_region_at_a_scale_is_rasterised_at_its_tiles_centres() {
    let map = rasterise(&raster(2_00), recipe(10, 10, 20, 20, 2)).expect("a map");
    assert_eq!(
        map.size(),
        Size {
            width: 40,
            height: 40
        }
    );
    // (20.25, 19.25) in ABE: 0.075 of A, 0.05 of B and 0.875 of E.
    let tile = map.tile(TilePos { x: 20, y: 18 }).expect("on the map");
    assert_eq!(tile.elevation, hundredths(146_45));
}

#[test]
fn sea_lake_and_land_have_their_layers() {
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    let at = |x, y| map.tile(TilePos { x, y }).expect("on the map");
    let sea = at(0, 0);
    assert_eq!(
        (sea.terrain, sea.water, sea.biome, sea.plant),
        (Terrain::DeepWater, None, Biome::Marine, None)
    );
    let lake = at(10, 30);
    assert_eq!(
        (lake.terrain, lake.water, lake.biome, lake.plant),
        (
            Terrain::ShallowWater,
            Some(Water::Lake),
            Biome::Marine,
            None
        )
    );
    let land = at(20, 15);
    assert_eq!(
        (land.terrain, land.water, land.biome, land.plant),
        (Terrain::Grass, None, Biome::Grassland, None)
    );
}

#[test]
fn a_tie_for_nearest_goes_to_the_first_cell() {
    // (5.5, 20.5) is outside every triangle, and as far from A as from C: A's height and its
    // ocean win, deep sea at −50.00 m, where C's would be lake at −2.63.
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    let tile = map.tile(TilePos { x: 5, y: 20 }).expect("on the map");
    assert_eq!(
        (tile.terrain, tile.water, tile.elevation),
        (Terrain::DeepWater, None, hundredths(-50_00))
    );
}

#[test]
fn a_region_off_the_source_is_refused() {
    let refused = rasterise(&raster(2_00), recipe(30, 30, 20, 20, 1)).expect_err("refused");
    assert_eq!(
        refused.to_string(),
        "the region runs off the source map: pixels (30, 30) to (49, 49), and the map is 40 × 40 pixels"
    );
    for (x, y, width, height) in [(1, 0, 40, 40), (0, 1, 40, 40)] {
        assert!(matches!(
            rasterise(&raster(2_00), recipe(x, y, width, height, 1)),
            Err(RasterError::OffSource { .. })
        ));
    }
    assert!(matches!(
        rasterise(&raster(2_00), recipe(0, 0, 40, 40, 0)),
        Err(RasterError::Map(_))
    ));
}

#[test]
fn a_source_without_cells_is_refused() {
    let mut source = raster(2_00);
    source.cells.clear();
    source.vertices.clear();
    let refused = rasterise(&source, whole()).expect_err("refused");
    assert_eq!(refused, RasterError::NoCells);
    assert_eq!(refused.to_string(), "the source map has no cells");
}

#[test]
fn the_recipe_is_kept() {
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    assert_eq!(*map.recipe(), whole());
}

proptest! {
    #[test]
    fn every_tile_lies_between_the_lowest_and_highest_cells(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..20,
        height in 1u32..20,
        scale in 1u32..4,
        exponent in 1_50i64..=2_20,
    ) {
        let source = raster(exponent);
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let metres: Vec<Fixed> = source
            .cells
            .iter()
            .map(|cell| cell_metres(cell.height, source.height_exponent))
            .collect();
        let (lowest, highest) = (
            *metres.iter().min().expect("cells"),
            *metres.iter().max().expect("cells"),
        );
        let map = rasterise(&source, recipe(x, y, width, height, scale)).expect("a map");
        for (_, chunk) in map.chunks() {
            let size = chunk.size();
            for (tx, ty) in (0..size.height).flat_map(|ty| (0..size.width).map(move |tx| (tx, ty))) {
                let tile = chunk.tile(tx, ty).expect("in the chunk");
                prop_assert!(lowest <= tile.elevation && tile.elevation <= highest);
            }
        }
        prop_assert_eq!(rasterise(&source, recipe(x, y, width, height, scale)), Ok(map));
    }
}

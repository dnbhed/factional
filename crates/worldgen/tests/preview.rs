//! The wizard's preview of a source (W8b, P-155): each pixel the cell nearest its centre, water
//! or land at its height, shrunk to a side; and the size of the map a recipe would make, or why
//! it can't, before it's made. Every expected value is worked by hand from the raster source:
//! 40 × 40 pixels, A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40, C (10.5, 30.5) at
//! 19 in a lake, D (30.5, 30.5) at 60 and E (20.5, 20.5) at 30, the last three on an island.

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Feature, Foliage, Levels, Point, RasterError, Recipe, Region, Shade,
    Size, Source, SourceCell, preview, rasterise, rasterised_size,
};

fn point(x: i64, y: i64) -> Point {
    Point {
        x: Fixed::from_hundredths(x),
        y: Fixed::from_hundredths(y),
    }
}

fn cell(centre: Point, height: u8, feature: u32) -> SourceCell {
    SourceCell {
        centre,
        height,
        feature,
        neighbours: Vec::new(),
        corners: Vec::new(),
        flux: 0,
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

/// The raster source, `width` × `height` pixels, its five cells where they are at 40 × 40.
fn raster(width: u32, height: u32) -> Source {
    Source {
        name: "Raster".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size { width, height },
        height_exponent: Fixed::from_hundredths(2_00),
        cells: vec![
            cell(point(10_50, 10_50), 10, 0),
            cell(point(30_50, 10_50), 40, 2),
            cell(point(10_50, 30_50), 19, 1),
            cell(point(30_50, 30_50), 60, 2),
            cell(point(20_50, 20_50), 30, 2),
        ],
        vertices: Vec::new(),
        features: vec![
            Feature::Ocean,
            Feature::Lake {
                surface: Fixed::from_hundredths(20_00),
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

fn recipe(region: [u32; 4], scale: u32) -> Recipe {
    let [x, y, width, height] = region;
    Recipe {
        source: String::new(),
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

#[test]
fn a_source_no_longer_than_the_side_is_previewed_a_pixel_a_pixel_by_its_nearest_cells() {
    let preview = preview(&raster(40, 40), 512).expect("a preview");
    assert_eq!(
        preview.size,
        Size {
            width: 40,
            height: 40
        }
    );
    assert_eq!(preview.pixels.len(), 40 * 40);
    assert_eq!(preview.at(0, 0), Shade::Water(10));
    assert_eq!(preview.at(39, 0), Shade::Land(40));
    // C lies in a lake: water, however high.
    assert_eq!(preview.at(0, 39), Shade::Water(19));
    assert_eq!(preview.at(39, 39), Shade::Land(60));
    assert_eq!(preview.at(20, 20), Shade::Land(30));
    // Row by row, from the top left.
    assert_eq!(preview.pixels[39], Shade::Land(40));
    assert_eq!(preview.pixels[39 * 40], Shade::Water(19));
}

#[test]
fn a_longer_source_is_shrunk_to_the_side_each_pixel_its_centres_cell() {
    // At 4 a side, the centres are at 5, 15, 25 and 35 pixels each way.
    let four = preview(&raster(40, 40), 4).expect("a preview");
    assert_eq!(
        four.size,
        Size {
            width: 4,
            height: 4
        }
    );
    // (15, 15) is 4.5 each way from A and 5.5 from E.
    assert_eq!(four.at(1, 1), Shade::Water(10));
    // (25, 25) is 4.5 each way from E and 5.5 from D.
    assert_eq!(four.at(2, 2), Shade::Land(30));
    assert_eq!(four.at(3, 0), Shade::Land(40));
    assert_eq!(four.at(0, 3), Shade::Water(19));
    // At 3 a side, the centres are at 6.67, 20 and 33.33.
    let three = preview(&raster(40, 40), 3).expect("a preview");
    assert_eq!(
        three.size,
        Size {
            width: 3,
            height: 3
        }
    );
    assert_eq!(three.at(1, 1), Shade::Land(30));
    assert_eq!(three.at(2, 0), Shade::Land(40));
    assert_eq!(three.at(0, 2), Shade::Water(19));
}

#[test]
fn a_shrunk_preview_keeps_the_sources_shape() {
    let wide = preview(&raster(80, 40), 8).expect("a preview");
    assert_eq!(
        wide.size,
        Size {
            width: 8,
            height: 4
        }
    );
    let tall = preview(&raster(40, 80), 8).expect("a preview");
    assert_eq!(
        tall.size,
        Size {
            width: 4,
            height: 8
        }
    );
    // 1000 × 3 at 8 a side is 8 × 0.024, rounded to none but held to 1.
    let thin = preview(&raster(1000, 3), 8).expect("a preview");
    assert_eq!(
        thin.size,
        Size {
            width: 8,
            height: 1
        }
    );
    // 30 × 20 at 8 a side is 8 × 5.33, rounded to 5; 30 × 25 is 8 × 6.67, rounded to 7.
    assert_eq!(
        preview(&raster(30, 20), 8).expect("a preview").size.height,
        5
    );
    assert_eq!(
        preview(&raster(30, 25), 8).expect("a preview").size.height,
        7
    );
}

#[test]
fn a_source_without_cells_has_no_preview() {
    let mut source = raster(40, 40);
    source.cells.clear();
    assert_eq!(preview(&source, 512), Err(RasterError::NoCells));
}

#[test]
fn the_size_a_recipe_makes_is_worked_out_before_its_made() {
    let source = raster(40, 40);
    assert_eq!(
        rasterised_size(&source, &recipe([0, 0, 40, 40], 2)),
        Ok(Size {
            width: 80,
            height: 80
        })
    );
    assert_eq!(
        rasterised_size(&source, &recipe([10, 10, 20, 10], 2)),
        Ok(Size {
            width: 40,
            height: 20
        })
    );
}

#[test]
fn a_recipe_that_cant_be_made_is_refused_before_its_made_as_rasterise_refuses_it() {
    let source = raster(40, 40);
    let mut empty = raster(40, 40);
    empty.cells.clear();
    let cases = [
        (
            &source,
            recipe([30, 0, 20, 10], 1),
            "the region runs off the source map: pixels (30, 0) to (49, 9), and the map is 40 × 40 pixels",
        ),
        (
            &source,
            recipe([0, 0, 40, 40], 300),
            "the map would be 12000 × 12000 tiles; at most 8192 a side",
        ),
        (
            &source,
            recipe([0, 0, 0, 40], 1),
            "the region must be at least 1 source pixel each way",
        ),
        (
            &empty,
            recipe([0, 0, 40, 40], 1),
            "the source map has no cells",
        ),
    ];
    for (source, recipe, said) in cases {
        let refused = rasterised_size(source, &recipe).expect_err("refused");
        assert_eq!(refused.to_string(), said);
        assert_eq!(rasterise(source, recipe).expect_err("refused"), refused);
    }
}

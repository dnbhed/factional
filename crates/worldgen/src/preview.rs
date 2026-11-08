//! The wizard's preview of a source (W8b, P-155): each pixel the cell nearest its centre, water
//! or land at its height, the source shrunk to a side if it's longer.

use crate::map::{Coast, Detail, Foliage, Levels, Recipe, Region, Size};
use crate::raster::{Raster, RasterError};
use crate::source::{Feature, Source};

/// A pixel of a preview: water or land, at the height of the cell it takes, on Azgaar's scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shade {
    Water(u8),
    Land(u8),
}

/// A source drawn small: its size in pixels, and its pixels row by row from the top left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub size: Size,
    pub pixels: Vec<Shade>,
}

impl Preview {
    /// The pixel `x` across and `y` down.
    pub fn at(&self, x: u32, y: u32) -> Shade {
        self.pixels[y as usize * self.size.width as usize + x as usize]
    }
}

/// `source` drawn small: its size, or, if it's longer than `side` a side, shrunk to `side` on its
/// longer side, keeping its shape, each side rounded and at least 1; each pixel the cell whose
/// centre is nearest the pixel's centre, the first if several are as near, water if that's in
/// the ocean or a lake. A source without cells has none.
pub fn preview(source: &Source, side: u32) -> Result<Preview, RasterError> {
    if source.cells.is_empty() {
        return Err(RasterError::NoCells);
    }
    let Size { width, height } = source.size;
    let longer = u64::from(width.max(height));
    let shrunk = |pixels: u32| -> u32 {
        match longer <= u64::from(side) {
            true => pixels,
            false => {
                let rounded = (u64::from(pixels) * u64::from(side) + longer / 2) / longer;
                // Never more than the side, so a slip here can't make a vast preview.
                let held = rounded.clamp(1, u64::from(side.max(1)));
                u32::try_from(held).expect("no longer than the side")
            }
        }
    };
    let size = Size {
        width: shrunk(width),
        height: shrunk(height),
    };
    // The whole source at a tile a pixel, whose units are 200 a source pixel.
    let whole = Recipe {
        source: String::new(),
        region: Region {
            x: 0,
            y: 0,
            size: source.size,
        },
        scale: 1,
        seed: 0,
        detail: Detail::default(),
        coast: Coast::default(),
        levels: Levels::default(),
        foliage: Foliage::default(),
    };
    let raster = Raster::new(source, &whole);
    // The centre of pixel `i` of `count` across `pixels` of the source, in units.
    let centre = |i: u32, count: u32, pixels: u32| -> i128 {
        (2 * i128::from(i) + 1) * 100 * i128::from(pixels) / i128::from(count)
    };
    let pixels = (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let at = [centre(x, size.width, width), centre(y, size.height, height)];
            let (nearest, _) = raster.nearest(at);
            let cell = &source.cells[nearest];
            match source.features[cell.feature as usize] {
                Feature::Ocean | Feature::Lake { .. } => Shade::Water(cell.height),
                Feature::Island => Shade::Land(cell.height),
            }
        })
        .collect();
    Ok(Preview { size, pixels })
}

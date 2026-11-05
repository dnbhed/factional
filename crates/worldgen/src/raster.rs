//! Rasterising a source (W2b, P-135): a region of it at a scale made into tiles. Each tile's
//! height is interpolated at its centre across the triangle of cell centres it falls in, in
//! metres by Azgaar's own formula, rounded once; outside every triangle it takes the nearest
//! cell's. Land, sea and lake come from the nearest cell's feature.

use std::fmt;

use factional_core::{Fixed, div_round};

use crate::map::{MapError, Recipe, Region, Size, TilePos, WorldMap};
use crate::source::{Feature, Source};
use crate::tile::{Biome, Terrain, Tile, Water};

/// The lowest and highest height exponents Azgaar allows (`settings.heightExponent`).
pub const HEIGHT_EXPONENTS: [Fixed; 2] =
    [Fixed::from_hundredths(1_50), Fixed::from_hundredths(2_20)];

/// Sea level on Azgaar's height scale of 0 to 100.
const SEA_LEVEL: u8 = 20;

/// How many binary places of the exponent's fraction are taken; the rest moves a height by
/// far less than a centimetre.
const FRACTION_BITS: u32 = 40;

/// Binary places in the fixed point the power is worked in.
const POINT: u32 = 48;

/// The height in metres of a cell at `height` on Azgaar's scale, as Azgaar has it:
/// (h − 18)^exponent at sea level and above, ((h − 20) / h) × 50 below it, and −990 at 0.
/// The exponent is held to the range Azgaar allows. A fractional power is worked in integers,
/// the same on every machine, and rounded once.
pub fn cell_metres(height: u8, exponent: Fixed) -> Fixed {
    let h = i128::from(height);
    let hundredths = match height {
        0 => -990_00,
        1..SEA_LEVEL => div_round(50_00 * (h - i128::from(SEA_LEVEL)), h),
        _ => {
            let [lowest, highest] = HEIGHT_EXPONENTS.map(Fixed::hundredths);
            power(
                u128::from(height - 18),
                exponent.hundredths().clamp(lowest, highest),
            )
        }
    };
    Fixed::from_hundredths(i64::try_from(hundredths).expect("Azgaar's heights fit"))
}

/// `base` to the power `exponent` hundredths, in hundredths, rounded once. The whole part is
/// multiplied out; the fraction, taken bit by bit, multiplies in `base`'s square root, its
/// fourth root and so on, each worked by integer square roots in fixed point.
fn power(base: u128, exponent: i64) -> i128 {
    let one = 1u128 << POINT;
    let whole = u32::try_from(exponent / 100).expect("a held exponent");
    let mut fraction = exponent % 100;
    let mut root = base << POINT;
    let mut value = base.pow(whole) << POINT;
    for _ in 0..FRACTION_BITS {
        root = (root << POINT).isqrt();
        fraction *= 2;
        if fraction >= 100 {
            fraction -= 100;
            value = (value * root) >> POINT;
        }
    }
    let hundredths = i128::try_from(value * 100).expect("Azgaar's heights fit");
    div_round(hundredths, i128::try_from(one).expect("one fits"))
}

/// Why a source can't be rasterised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    /// The recipe can't make a map.
    Map(MapError),
    /// The recipe's region runs past the source's edge.
    OffSource { region: Region, size: Size },
    /// The source has no cells to take heights from.
    NoCells,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RasterError::Map(error) => error.fmt(f),
            RasterError::OffSource { region, size } => write!(
                f,
                "the region runs off the source map: pixels ({}, {}) to ({}, {}), and the map is {} × {} pixels",
                region.x,
                region.y,
                u64::from(region.x) + u64::from(region.size.width) - 1,
                u64::from(region.y) + u64::from(region.size.height) - 1,
                size.width,
                size.height
            ),
            RasterError::NoCells => write!(f, "the source map has no cells"),
        }
    }
}

impl std::error::Error for RasterError {}

/// The map `recipe` makes of `source`: its region at its scale, each tile from the cells
/// around its centre. `source` is as `factional-content` reads it, every reference resolving.
pub fn rasterise(source: &Source, recipe: Recipe) -> Result<WorldMap, RasterError> {
    recipe.size().map_err(RasterError::Map)?;
    let region = recipe.region;
    let past =
        |from: u32, pixels: u32, side: u32| u64::from(from) + u64::from(pixels) > u64::from(side);
    if past(region.x, region.size.width, source.size.width)
        || past(region.y, region.size.height, source.size.height)
    {
        return Err(RasterError::OffSource {
            region,
            size: source.size,
        });
    }
    if source.cells.is_empty() {
        return Err(RasterError::NoCells);
    }
    let raster = Raster::new(source, recipe.scale);
    let scale = i128::from(recipe.scale);
    let origin = [region.x, region.y].map(|from| i128::from(from) * 200 * scale);
    WorldMap::new(recipe, |at: TilePos| {
        let centre = |from: i128, tile: u32| from + (2 * i128::from(tile) + 1) * 100;
        raster.tile([centre(origin[0], at.x), centre(origin[1], at.y)])
    })
    .map_err(RasterError::Map)
}

/// A point in units of 1 / (200 × scale) of a source pixel, so that every tile's centre and
/// every cell's centre, to the hundredth, is a whole number of them.
type At = [i128; 2];

/// The source made ready to rasterise: its cells' centres and metres, its triangles, and a
/// grid of buckets to find them in.
struct Raster<'a> {
    source: &'a Source,
    centres: Vec<At>,
    metres: Vec<i128>,
    triangles: Vec<[usize; 3]>,
    grid: Grid,
    cells_in: Vec<Vec<usize>>,
    triangles_in: Vec<Vec<usize>>,
}

impl Raster<'_> {
    fn new(source: &Source, scale: u32) -> Raster<'_> {
        let scale = i128::from(scale);
        let centres: Vec<At> = source
            .cells
            .iter()
            .map(|cell| {
                [cell.centre.x, cell.centre.y].map(|c| i128::from(c.hundredths()) * 2 * scale)
            })
            .collect();
        let metres = source
            .cells
            .iter()
            .map(|cell| i128::from(cell_metres(cell.height, source.height_exponent).hundredths()))
            .collect();
        // A triangle for each vertex with three cells, unless they lie on a line, each turned
        // the same way, so that a point inside has no negative weight.
        let triangles: Vec<[usize; 3]> = source
            .vertices
            .iter()
            .filter_map(|vertex| match vertex.cells {
                [Some(a), Some(b), Some(c)] => {
                    let [a, b, c] = [a, b, c].map(|cell| cell as usize);
                    match cross(centres[a], centres[b], centres[c]).signum() {
                        1 => Some([a, b, c]),
                        -1 => Some([a, c, b]),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect();
        let corner =
            [source.size.width, source.size.height].map(|side| i128::from(side) * 200 * scale);
        let grid = Grid::new(&centres, corner);
        let mut cells_in = vec![Vec::new(); grid.len()];
        for (i, &centre) in centres.iter().enumerate() {
            cells_in[grid.index(grid.bucket(centre))].push(i);
        }
        let mut triangles_in = vec![Vec::new(); grid.len()];
        for (i, triangle) in triangles.iter().enumerate() {
            let points = triangle.map(|cell| centres[cell]);
            let lowest = grid
                .bucket([0, 1].map(|axis| points.iter().map(|p| p[axis]).min().expect("three")));
            let highest = grid
                .bucket([0, 1].map(|axis| points.iter().map(|p| p[axis]).max().expect("three")));
            for y in lowest[1]..=highest[1] {
                for x in lowest[0]..=highest[0] {
                    triangles_in[grid.index([x, y])].push(i);
                }
            }
        }
        Raster {
            source,
            centres,
            metres,
            triangles,
            grid,
            cells_in,
            triangles_in,
        }
    }

    /// The tile whose centre is at `at`.
    fn tile(&self, at: At) -> Tile {
        let (nearest, _) = self.nearest(at);
        let elevation = self.interpolated(at).unwrap_or(self.metres[nearest]);
        let cell = &self.source.cells[nearest];
        let (terrain, water, biome) = match self.source.features[cell.feature as usize] {
            Feature::Ocean => (Terrain::DeepWater, None, Biome::Marine),
            Feature::Lake { .. } => (Terrain::ShallowWater, Some(Water::Lake), Biome::Marine),
            Feature::Island => (Terrain::Grass, None, Biome::Grassland),
        };
        Tile {
            elevation: Fixed::from_hundredths(
                i64::try_from(elevation).expect("between the cells' heights"),
            ),
            terrain,
            water,
            biome,
            plant: None,
        }
    }

    /// The height at `at`, weighted across the first triangle it falls in by its barycentric
    /// coordinates and rounded once; none outside every triangle.
    fn interpolated(&self, at: At) -> Option<i128> {
        self.triangles_in[self.grid.index(self.grid.bucket(at))]
            .iter()
            .find_map(|&i| {
                let [a, b, c] = self.triangles[i];
                let [pa, pb, pc] = [a, b, c].map(|cell| self.centres[cell]);
                let weights = [cross(at, pb, pc), cross(at, pc, pa), cross(at, pa, pb)];
                if weights.iter().any(|&w| w < 0) {
                    return None;
                }
                let weighted: i128 = weights
                    .iter()
                    .zip([a, b, c])
                    .map(|(w, cell)| w * self.metres[cell])
                    .sum();
                Some(div_round(weighted, weights.iter().sum()))
            })
    }

    /// The cell whose centre is nearest `at`, the first if several are as near, and how many
    /// rings of buckets were searched for it: ring by ring outward, until none further out could
    /// hold a nearer one or the whole grid has been searched.
    fn nearest(&self, at: At) -> (usize, u32) {
        let centre = self.grid.bucket(at);
        let mut best: Option<(i128, usize)> = None;
        let last = self.grid.cover(centre);
        for ring in 0..=last {
            for bucket in self.grid.ring(centre, ring) {
                for &i in &self.cells_in[self.grid.index(bucket)] {
                    let [dx, dy] = [0, 1].map(|axis| self.centres[i][axis] - at[axis]);
                    let candidate = (dx * dx + dy * dy, i);
                    best = Some(best.map_or(candidate, |best| best.min(candidate)));
                }
            }
            // A cell further out is further than this, since `at` lies inside its bucket.
            let reach = i128::from(ring) * self.grid.side;
            if let Some((distance, i)) = best
                && distance < reach * reach
            {
                return (i, ring + 1);
            }
        }
        let (_, i) = best.expect("the grid holds a cell");
        (i, last + 1)
    }
}

/// Twice the signed area of the triangle `a`, `b`, `c`: positive when they turn one way,
/// negative the other, zero on a line.
fn cross(a: At, b: At, c: At) -> i128 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Square buckets over every cell centre and the whole source map, about as many as there
/// are cells.
struct Grid {
    origin: At,
    side: i128,
    across: usize,
    down: usize,
}

impl Grid {
    fn new(centres: &[At], corner: At) -> Grid {
        let bound = |axis: usize, pick: fn(i128, i128) -> i128, edge: i128| {
            centres.iter().map(|c| c[axis]).fold(edge, pick)
        };
        let origin = [0, 1].map(|axis| bound(axis, i128::min, 0));
        let end = [0, 1].map(|axis| bound(axis, i128::max, corner[axis]));
        let [width, height] = [0, 1].map(|axis| end[axis] - origin[axis] + 1);
        let cells = i128::try_from(centres.len()).expect("a count fits");
        let area = u128::try_from(width * height / cells).expect("a positive area");
        let side = i128::try_from(area.isqrt()).expect("a side fits").max(1);
        let count =
            |length: i128| usize::try_from((length + side - 1) / side).expect("a count fits");
        Grid {
            origin,
            side,
            across: count(width),
            down: count(height),
        }
    }

    fn len(&self) -> usize {
        self.across * self.down
    }

    /// The bucket `at`, a cell centre or a point on the map, falls in, column then row.
    fn bucket(&self, at: At) -> [usize; 2] {
        [0, 1].map(|axis| {
            usize::try_from((at[axis] - self.origin[axis]) / self.side).expect("on the grid")
        })
    }

    fn index(&self, [x, y]: [usize; 2]) -> usize {
        y * self.across + x
    }

    /// How many rings out from `centre` reach the grid's furthest bucket.
    fn cover(&self, [cx, cy]: [usize; 2]) -> u32 {
        let furthest = [cx, self.across - 1 - cx, cy, self.down - 1 - cy];
        u32::try_from(furthest.into_iter().max().expect("four")).expect("a ring fits")
    }

    /// The buckets `ring` steps out from `centre`, on the grid.
    fn ring(&self, [cx, cy]: [usize; 2], ring: u32) -> impl Iterator<Item = [usize; 2]> {
        let r = ring as usize;
        let ys = cy.saturating_sub(r)..=(cy + r).min(self.down - 1);
        let xs = move || cx.saturating_sub(r)..=(cx + r).min(self.across - 1);
        ys.flat_map(move |y| xs().map(move |x| [x, y]))
            .filter(move |&[x, y]| x.abs_diff(cx).max(y.abs_diff(cy)) == r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_has_about_a_bucket_a_cell_over_the_cells_and_the_map() {
        let centres = [
            [2100, 2100],
            [6100, 2100],
            [2100, 6100],
            [6100, 6100],
            [4100, 4100],
        ];
        let grid = Grid::new(&centres, [8000, 8000]);
        // 8001 × 8001 units over 5 cells is 12,803,200 each, a side of 3578.
        assert_eq!(
            (grid.origin, grid.side, grid.across, grid.down),
            ([0, 0], 3578, 3, 3)
        );
        assert_eq!(grid.bucket([8000, 8000]), [2, 2]);
        assert_eq!(grid.bucket([3578, 3577]), [1, 0]);
        // A cell past the map's edge stretches the grid to hold it: 10,001 × 12,001 over 2.
        let grid = Grid::new(&[[-1000, 500], [9000, 12000]], [8000, 8000]);
        assert_eq!(
            (grid.origin, grid.side, grid.across, grid.down),
            ([-1000, 0], 7746, 2, 2)
        );
        assert_eq!(grid.bucket([-1000, 12000]), [0, 1]);
        assert_eq!(grid.bucket([8000, 0]), [1, 0]);
    }

    /// A source of 40 × 40 pixels with cells at `centres`, in units of 1/200 of a pixel.
    fn source(centres: &[At]) -> Source {
        use crate::source::{Point, SourceCell};
        let point = |units: i128| Fixed::from_hundredths(i64::try_from(units / 2).expect("fits"));
        Source {
            name: "Search".to_owned(),
            version: "1.154.0".to_owned(),
            size: Size {
                width: 40,
                height: 40,
            },
            height_exponent: Fixed::from_hundredths(2_00),
            cells: centres
                .iter()
                .map(|&[x, y]| SourceCell {
                    centre: Point {
                        x: point(x),
                        y: point(y),
                    },
                    height: 30,
                    feature: 0,
                    neighbours: Vec::new(),
                    corners: Vec::new(),
                })
                .collect(),
            vertices: Vec::new(),
            features: vec![Feature::Island],
        }
    }

    #[test]
    fn the_search_stops_once_no_further_ring_could_hold_a_nearer_cell() {
        // 16 cells over 8001 units each way: buckets of 2000, 5 × 5. From (100, 100), cell 0
        // is in ring 2, exactly 4000 away, so ring 3 is searched too, and then no more.
        let mut centres = vec![[4100, 100]];
        centres.extend([[8000, 8000]; 15]);
        let source = source(&centres);
        let raster = Raster::new(&source, 1);
        assert_eq!((raster.grid.side, raster.grid.across), (2000, 5));
        assert_eq!(raster.nearest([100, 100]), (0, 4));
        // At a cell's centre, the next ring settles it.
        assert_eq!(raster.nearest([8000, 8000]), (1, 2));
    }

    #[test]
    fn the_search_ends_when_it_has_covered_the_grid() {
        // 9 cells at the top left, buckets of 2667, 3 × 3: from the far corner, all three rings.
        let source = source(&[[0, 0]; 9]);
        let raster = Raster::new(&source, 1);
        assert_eq!(raster.grid.cover([2, 2]), 2);
        assert_eq!(raster.grid.cover([1, 0]), 2);
        assert_eq!(raster.grid.cover([1, 1]), 1);
        assert_eq!(raster.nearest([8000, 8000]), (0, 3));
    }

    #[test]
    fn a_ring_is_the_buckets_that_many_steps_out_on_the_grid() {
        let grid = Grid::new(&[[2100, 2100]; 9], [8000, 8000]);
        assert_eq!((grid.across, grid.down), (3, 3));
        let ring = |centre, ring| grid.ring(centre, ring).collect::<Vec<_>>();
        assert_eq!(ring([1, 1], 0), [[1, 1]]);
        assert_eq!(
            ring([1, 1], 1),
            [
                [0, 0],
                [1, 0],
                [2, 0],
                [0, 1],
                [2, 1],
                [0, 2],
                [1, 2],
                [2, 2]
            ]
        );
        assert_eq!(ring([0, 0], 1), [[1, 0], [0, 1], [1, 1]]);
        assert_eq!(ring([0, 0], 2), [[2, 0], [2, 1], [0, 2], [1, 2], [2, 2]]);
        assert_eq!(ring([0, 0], 3), Vec::<[usize; 2]>::new());
        assert_eq!(grid.index([2, 1]), 5);
        assert_eq!(grid.len(), 9);
    }
}

//! Rasterising a source (W2b, P-135): a region of it at a scale made into tiles. Each tile's
//! height is interpolated at its centre across the triangle of cell centres it falls in, in
//! metres by Azgaar's own formula, rounded once; outside every triangle it takes the nearest
//! cell's. Whether it lies under the sea, a lake or neither is W3b's, in `coast.rs`.

use std::fmt;

use factional_core::{Fixed, div_round};

use crate::coast::{self, Body};
use crate::foliage;
use crate::levels;
use crate::map::{Detail, MapError, Recipe, Region, Size, TilePos, WorldMap};
use crate::noise;
use crate::rivers::{self, At, Piece};
use crate::source::{Feature, Source};
use crate::suggestions;
use crate::terrain;
use crate::tile::{Biome, Terrain, Tile, Water};

/// The lowest and highest height exponents Azgaar allows (`settings.heightExponent`).
pub const HEIGHT_EXPONENTS: [Fixed; 2] =
    [Fixed::from_hundredths(1_50), Fixed::from_hundredths(2_20)];

/// Sea level on Azgaar's height scale of 0 to 100, in hundredths.
const SEA_LEVEL: i128 = 20_00;

/// The highest height on Azgaar's scale, in hundredths.
const HIGHEST: i128 = 100_00;

/// The most levels there can be: one a hundredth above sea level, the closest they're spaced.
const MOST_LEVELS: i128 = 80_00;

/// How many binary places of the exponent's fraction are taken; the rest moves a height by
/// far less than a centimetre.
const FRACTION_BITS: u32 = 40;

/// Binary places in the fixed point the power is worked in.
const POINT: u32 = 48;

/// The height in metres of a cell at `height` on Azgaar's scale, as `height_metres` has it.
pub fn cell_metres(height: u8, exponent: Fixed) -> Fixed {
    height_metres(Fixed::from_hundredths(i64::from(height) * 100), exponent)
}

/// The height in metres of `height` on Azgaar's scale of 0 to 100, to the hundredth (a lake's
/// surface, say), as Azgaar has it: (h − 18)^exponent at sea level and above, ((h − 20) / h) ×
/// 50 below it, and −990 at 0. The height is held to that scale, and the exponent to the range
/// Azgaar allows. A fractional power is worked in integers, the same on every machine, and
/// rounded once.
pub fn height_metres(height: Fixed, exponent: Fixed) -> Fixed {
    let h = i128::from(height.hundredths()).min(HIGHEST);
    let hundredths = match h {
        ..=0 => -990_00,
        _ if h < SEA_LEVEL => div_round(50_00 * (h - SEA_LEVEL), h),
        _ => {
            let [lowest, highest] = HEIGHT_EXPONENTS.map(Fixed::hundredths);
            power(
                u128::try_from(h - 18_00).expect("above sea level"),
                exponent.hundredths().clamp(lowest, highest),
            )
        }
    };
    Fixed::from_hundredths(i64::try_from(hundredths).expect("Azgaar's heights fit"))
}

/// The height in metres at which each level above 0 starts (W3c, P-138): level k at 20 + k ×
/// `spacing` on Azgaar's scale, for each k that reaches no higher than its top, by
/// `height_metres`. A spacing is held to at least a hundredth.
pub fn level_heights(spacing: Fixed, exponent: Fixed) -> Vec<Fixed> {
    let step = i128::from(spacing.hundredths()).max(1);
    (1..=MOST_LEVELS)
        .map(|k| SEA_LEVEL + k * step)
        .take_while(|&height| height <= HIGHEST)
        .map(|height| {
            let height = Fixed::from_hundredths(i64::try_from(height).expect("on Azgaar's scale"));
            height_metres(height, exponent)
        })
        .collect()
}

/// `base` hundredths to the power `exponent` hundredths, in hundredths, rounded once. The
/// whole part is multiplied out; the fraction, taken bit by bit, multiplies in `base`'s square
/// root, its fourth root and so on, each worked by integer square roots in fixed point.
pub(crate) fn power(base: u128, exponent: i64) -> i128 {
    let one = 1u128 << POINT;
    let mut fraction = exponent % 100;
    let mut root = (base << POINT) / 100;
    let mut value = one;
    for _ in 0..exponent / 100 {
        value = (value * root) >> POINT;
    }
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

/// Each cell's relief: its steepest difference in metres to a neighbour, none without one.
pub fn cell_reliefs(source: &Source) -> Vec<Fixed> {
    reliefs(source, &metres(source))
        .into_iter()
        .map(|relief| Fixed::from_hundredths(i64::try_from(relief).expect("Azgaar's heights fit")))
        .collect()
}

/// Each cell's metres, in hundredths.
fn metres(source: &Source) -> Vec<i128> {
    source
        .cells
        .iter()
        .map(|cell| i128::from(cell_metres(cell.height, source.height_exponent).hundredths()))
        .collect()
}

/// Each cell's relief, in hundredths, from its `metres`.
fn reliefs(source: &Source, metres: &[i128]) -> Vec<i128> {
    source
        .cells
        .iter()
        .zip(metres)
        .map(|(cell, here)| {
            cell.neighbours
                .iter()
                .map(|&n| (here - metres[n as usize]).abs())
                .max()
                .unwrap_or(0)
        })
        .collect()
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
    let size = recipe.size().map_err(RasterError::Map)?;
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
    let raster = Raster::new(source, &recipe);
    let scale = i128::from(recipe.scale);
    let origin = [region.x, region.y].map(|from| i128::from(from) * 200 * scale);
    let centre = |from: i128, tile: u32| from + (2 * i128::from(tile) + 1) * 100;
    // Every tile's ground, row by row: its height, the water cell it lies under and the cell it
    // lies in; then the specks.
    let point = |x: u32, y: u32| [centre(origin[0], x), centre(origin[1], y)];
    let grounds: Vec<Ground> = (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .map(|(x, y)| raster.ground(point(x, y)))
        .collect();
    let elevations: Vec<Fixed> = grounds.iter().map(|ground| ground.elevation).collect();
    let mut waters: Vec<Option<u32>> = grounds.iter().map(|ground| ground.water).collect();
    let width = size.width as usize;
    coast::remove_specks(width, &mut waters, recipe.coast.specks as usize);
    // The rivers, on the land, each tile lowered to its bed; then each land tile's level, then
    // the plateaus and pits; water stands at the level of its surface.
    let land: Vec<bool> = waters.iter().map(Option::is_none).collect();
    let courses = rivers::paint(width, origin, &land, &raster.rivers(scale));
    let elevations: Vec<Fixed> = elevations
        .iter()
        .zip(&courses)
        .map(|(&elevation, course)| course.map_or(elevation, |c| elevation.min(c.bed)))
        .collect();
    let starts = level_heights(recipe.levels.spacing, source.height_exponent);
    let level = |height: Fixed| {
        u8::try_from(starts.partition_point(|&start| start <= height)).expect("at most 80 levels")
    };
    let mut levels: Vec<Option<u8>> = elevations
        .iter()
        .zip(&land)
        .zip(&courses)
        .map(|((&elevation, &land), course)| (land && course.is_none()).then(|| level(elevation)))
        .collect();
    levels::clean(width, &mut levels, recipe.levels.specks as usize);
    // Each tile's water body; each land tile's biome, that of the nearest land cell; each
    // tile's level, water's that of its surface; then each land tile's terrain (P-142).
    let bodies: Vec<Option<Body>> = waters
        .iter()
        .map(|water| water.map(|cell| raster.bodies[cell as usize].expect("a water cell")))
        .collect();
    let biomes: Vec<Option<Biome>> = (0..grounds.len())
        .map(|i| {
            (land[i] && courses[i].is_none()).then(|| {
                let at = point((i % width) as u32, (i / width) as u32);
                raster
                    .nearest_land(at)
                    .map_or(Biome::Marine, |(land, _)| source.cells[land].biome)
            })
        })
        .collect();
    let levels: Vec<u8> = (0..grounds.len())
        .map(|i| {
            let surface = bodies[i].map_or(elevations[i], |body| body.surface);
            levels[i].unwrap_or_else(|| level(surface))
        })
        .collect();
    let kinds: Vec<terrain::Kind> = (0..grounds.len())
        .map(|i| match (biomes[i], bodies[i]) {
            (Some(biome), _) => terrain::Kind::Land(biome),
            (None, Some(Body { lake: false, .. })) => terrain::Kind::Sea,
            (None, _) => terrain::Kind::Fresh,
        })
        .collect();
    let terrains = terrain::terrains(width, &kinds, &levels);
    // Each land tile's candidate plant, but on rock or ice, at its place on the whole map; then
    // those that stand (P-143).
    let foliage = recipe.foliage;
    let corner = [region.x, region.y].map(|from| u64::from(from) * u64::from(recipe.scale));
    let candidates: Vec<Option<foliage::Candidate>> = (0..grounds.len())
        .map(|i| {
            let biome = biomes[i]?;
            if matches!(terrains[i], Some(Terrain::Rock | Terrain::Ice)) {
                return None;
            }
            let at = [
                corner[0] + (i % width) as u64,
                corner[1] + (i / width) as u64,
            ];
            let hash = foliage::hash(recipe.seed, at);
            foliage::candidate(hash, foliage.chances_in(biome), foliage.scale)
        })
        .collect();
    let plants = foliage::plants(width, &candidates, foliage.spacing);
    let shallows = recipe.coast.shallows;
    let cells: Vec<usize> = grounds.iter().map(|ground| ground.cell).collect();
    let suggestions = suggestions::suggest(source, &recipe, size, &cells);
    let mut map = WorldMap::new(recipe, |at: TilePos| {
        let i = at.y as usize * width + at.x as usize;
        let tile = coast::tile(elevations[i], bodies[i], shallows);
        let tile = match courses[i] {
            Some(course) => Tile {
                terrain: Terrain::ShallowWater,
                water: Some(Water::River { flow: course.flow }),
                ..tile
            },
            None => tile,
        };
        // The climate of the cell it lies in.
        let cell = &source.cells[grounds[i].cell];
        Tile {
            level: levels[i],
            terrain: terrains[i].unwrap_or(tile.terrain),
            biome: biomes[i].unwrap_or(Biome::Marine),
            temperature: cell.temperature,
            precipitation: cell.precipitation,
            plant: plants[i],
            ..tile
        }
    })
    .map_err(RasterError::Map)?;
    // The source's places, on the map (W6b, P-145).
    map.suggest(suggestions).map_err(RasterError::Map)?;
    Ok(map)
}

/// The ground at a point: its height, the water cell it lies under, if any, and the cell it
/// lies in.
struct Ground {
    elevation: Fixed,
    water: Option<u32>,
    cell: usize,
}

/// The source made ready to rasterise: its cells' centres, metres, reliefs and water, its
/// triangles, a grid of buckets to find them in, and the detail to add, its wavelength in
/// units.
struct Raster<'a> {
    source: &'a Source,
    centres: Vec<At>,
    metres: Vec<i128>,
    reliefs: Vec<i128>,
    bodies: Vec<Option<Body>>,
    triangles: Vec<[usize; 3]>,
    detail: Detail,
    seed: u32,
    wavelength: i128,
    grid: Grid,
    cells_in: Vec<Vec<usize>>,
    land_in: Vec<Vec<usize>>,
    triangles_in: Vec<Vec<usize>>,
}

impl Raster<'_> {
    fn new<'a>(source: &'a Source, recipe: &Recipe) -> Raster<'a> {
        let scale = i128::from(recipe.scale);
        let centres: Vec<At> = source
            .cells
            .iter()
            .map(|cell| {
                [cell.centre.x, cell.centre.y].map(|c| i128::from(c.hundredths()) * 2 * scale)
            })
            .collect();
        let metres = metres(source);
        let reliefs = reliefs(source, &metres);
        let bodies = source
            .cells
            .iter()
            .map(|cell| match source.features[cell.feature as usize] {
                Feature::Ocean => Some(Body {
                    surface: Fixed::ZERO,
                    lake: false,
                }),
                Feature::Lake { surface } => Some(Body {
                    surface: height_metres(surface, source.height_exponent),
                    lake: true,
                }),
                Feature::Island => None,
            })
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
        let mut land_in = vec![Vec::new(); grid.len()];
        for (i, &centre) in centres.iter().enumerate() {
            let bucket = grid.index(grid.bucket(centre));
            cells_in[bucket].push(i);
            if source.cells[i].biome != Biome::Marine {
                land_in[bucket].push(i);
            }
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
        let detail = recipe.detail;
        Raster {
            source,
            centres,
            metres,
            reliefs,
            bodies,
            triangles,
            detail,
            seed: recipe.seed,
            wavelength: i128::from(detail.wavelength.hundredths()) * 2 * scale,
            grid,
            cells_in,
            land_in,
            triangles_in,
        }
    }

    /// The pieces of every river's line, at `scale` tiles a pixel, each with its bed: the
    /// lowest ground along the line so far, never below the surface of the water it ends in.
    fn rivers(&self, scale: i128) -> Vec<Piece> {
        let size = self.source.size;
        let corner = [size.width, size.height].map(|side| i128::from(side) * 200 * scale);
        self.source
            .rivers
            .iter()
            .flat_map(|river| {
                let running = rivers::running_fluxes(river, &self.source.cells);
                let half_widths = rivers::half_widths(river, &running);
                let anchors = rivers::anchors(river, &self.centres, corner);
                // The surface of the water its last cell is in, unless it runs off the map.
                let floor = match river.off_map {
                    true => None,
                    false => river
                        .cells
                        .last()
                        .and_then(|&cell| self.bodies[cell as usize].map(|body| body.surface)),
                };
                rivers::pieces(
                    &anchors,
                    &half_widths,
                    &running,
                    scale,
                    |at| self.ground(at).elevation,
                    floor,
                )
            })
            .collect()
    }

    /// The ground whose centre is at `at`: its height interpolated, or the nearest cell's, with
    /// strength × relief × noise added, rounded once; the water cell it lies under, if any; and
    /// the cell it lies in, the nearest. The water cell is the one nearest `at` among the
    /// nearest cell and its neighbours, the first if several are as near, if `at` lies below its
    /// surface.
    fn ground(&self, at: At) -> Ground {
        let (nearest, _) = self.nearest(at);
        let weights = self
            .weights(at)
            .unwrap_or([(1, nearest), (0, nearest), (0, nearest)]);
        let sum = |of: &[i128]| -> i128 { weights.iter().map(|&(w, cell)| w * of[cell]).sum() };
        let area: i128 = weights.iter().map(|&(w, _)| w).sum();
        // The octaves' total weight, and the noise's fixed point, and hundredths to a share.
        let total = (1i128 << self.detail.octaves) - 1;
        let unit = 100 * (1i128 << noise::POINT) * total;
        let noise = noise::octaves(self.seed, self.detail.octaves, at, self.wavelength);
        let strength = i128::from(self.detail.strength.hundredths());
        // About 4 × 10³⁶ at most, for cells 1000 pixels apart at a scale of 8192, where
        // Azgaar's are tens apart; past i128, a crash rather than a wrapped height (P-30).
        let fits = "a height and its detail fit in i128";
        let base = sum(&self.metres).checked_mul(unit).expect(fits);
        let detail = (strength * sum(&self.reliefs))
            .checked_mul(noise)
            .expect(fits);
        let elevation = div_round(base.checked_add(detail).expect(fits), area * unit);
        let elevation =
            Fixed::from_hundredths(i64::try_from(elevation).expect("between the cells' heights"));
        let around = &self.source.cells[nearest].neighbours;
        let water = std::iter::once(nearest)
            .chain(around.iter().map(|&cell| cell as usize))
            .filter_map(|cell| {
                self.bodies[cell].map(|body| ((self.distance(cell, at), cell), body))
            })
            .min_by_key(|&(key, _)| key)
            .filter(|(_, body)| elevation < body.surface)
            .map(|((_, cell), _)| u32::try_from(cell).expect("a source's cells fit"));
        Ground {
            elevation,
            water,
            cell: nearest,
        }
    }

    /// The square of the distance from cell `i`'s centre to `at`.
    fn distance(&self, i: usize, at: At) -> i128 {
        let [dx, dy] = [0, 1].map(|axis| self.centres[i][axis] - at[axis]);
        dx * dx + dy * dy
    }

    /// The cells of the first triangle `at` falls in, each with its barycentric weight, twice
    /// the area of the triangle `at` makes with the other two; none outside every triangle.
    fn weights(&self, at: At) -> Option<[(i128, usize); 3]> {
        self.triangles_in[self.grid.index(self.grid.bucket(at))]
            .iter()
            .find_map(|&i| {
                let [a, b, c] = self.triangles[i];
                let [pa, pb, pc] = [a, b, c].map(|cell| self.centres[cell]);
                let weights = [cross(at, pb, pc), cross(at, pc, pa), cross(at, pa, pb)];
                match weights.iter().any(|&w| w < 0) {
                    true => None,
                    false => Some([(weights[0], a), (weights[1], b), (weights[2], c)]),
                }
            })
    }

    /// The cell whose centre is nearest `at`, the first if several are as near, and how many
    /// rings of buckets were searched for it.
    fn nearest(&self, at: At) -> (usize, u32) {
        self.nearest_in(at, &self.cells_in)
            .expect("the grid holds a cell")
    }

    /// The land cell, one whose biome isn't Marine, whose centre is nearest `at`, the first
    /// if several are as near, and how many rings of buckets were searched for it; none if
    /// the source has no land cell.
    fn nearest_land(&self, at: At) -> Option<(usize, u32)> {
        self.nearest_in(at, &self.land_in)
    }

    /// Of the cells in `buckets`, the one whose centre is nearest `at`, the first if several
    /// are as near, and how many rings of buckets were searched for it: ring by ring outward,
    /// until none further out could hold a nearer one or the whole grid has been searched.
    fn nearest_in(&self, at: At, buckets: &[Vec<usize>]) -> Option<(usize, u32)> {
        let centre = self.grid.bucket(at);
        let mut best: Option<(i128, usize)> = None;
        let last = self.grid.cover(centre);
        for ring in 0..=last {
            for bucket in self.grid.ring(centre, ring) {
                for &i in &buckets[self.grid.index(bucket)] {
                    let candidate = (self.distance(i, at), i);
                    best = Some(best.map_or(candidate, |best| best.min(candidate)));
                }
            }
            // A cell further out is further than this, since `at` lies inside its bucket.
            let reach = i128::from(ring) * self.grid.side;
            if let Some((distance, i)) = best
                && distance < reach * reach
            {
                return Some((i, ring + 1));
            }
        }
        best.map(|(_, i)| (i, last + 1))
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
                    flux: 0,
                    biome: Biome::Grassland,
                    temperature: 0,
                    precipitation: 0,
                    culture: None,
                    religion: None,
                    state: None,
                })
                .collect(),
            vertices: Vec::new(),
            features: vec![Feature::Island],
            rivers: Vec::new(),
            settlements: Vec::new(),
            routes: Vec::new(),
            markers: Vec::new(),
            cultures: Vec::new(),
            religions: Vec::new(),
            states: Vec::new(),
        }
    }

    /// The whole of a 40 × 40 source at a tile a pixel.
    fn whole() -> Recipe {
        Recipe {
            source: String::new(),
            region: Region {
                x: 0,
                y: 0,
                size: Size {
                    width: 40,
                    height: 40,
                },
            },
            scale: 1,
            seed: 0,
            detail: Detail::default(),
            coast: crate::map::Coast::default(),
            levels: crate::map::Levels::default(),
            foliage: crate::map::Foliage::default(),
        }
    }

    #[test]
    fn the_search_stops_once_no_further_ring_could_hold_a_nearer_cell() {
        // 16 cells over 8001 units each way: buckets of 2000, 5 × 5. From (100, 100), cell 0
        // is in ring 2, exactly 4000 away, so ring 3 is searched too, and then no more.
        let mut centres = vec![[4100, 100]];
        centres.extend([[8000, 8000]; 15]);
        let source = source(&centres);
        let raster = Raster::new(&source, &whole());
        assert_eq!((raster.grid.side, raster.grid.across), (2000, 5));
        assert_eq!(raster.nearest([100, 100]), (0, 4));
        // At a cell's centre, the next ring settles it.
        assert_eq!(raster.nearest([8000, 8000]), (1, 2));
    }

    #[test]
    fn the_land_search_passes_over_the_sea_and_finds_none_without_land() {
        // As above, but cell 0 is Marine: from (100, 100), cell 1 at (8000, 8000) is the
        // nearest land, √2 × 7900 away, beyond ring 4's reach of 8000, so every ring is searched.
        let mut centres = vec![[4100, 100]];
        centres.extend([[8000, 8000]; 15]);
        let mut source = source(&centres);
        source.cells[0].biome = Biome::Marine;
        let raster = Raster::new(&source, &whole());
        assert_eq!(raster.nearest([100, 100]), (0, 4));
        assert_eq!(raster.nearest_land([100, 100]), Some((1, 5)));
        assert_eq!(raster.nearest_land([8000, 8000]), Some((1, 2)));
        for cell in &mut source.cells {
            cell.biome = Biome::Marine;
        }
        let raster = Raster::new(&source, &whole());
        assert_eq!(raster.nearest_land([100, 100]), None);
    }

    #[test]
    fn the_search_ends_when_it_has_covered_the_grid() {
        // 9 cells at the top left, buckets of 2667, 3 × 3: from the far corner, all three rings.
        let source = source(&[[0, 0]; 9]);
        let raster = Raster::new(&source, &whole());
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

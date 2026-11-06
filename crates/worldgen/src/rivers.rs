//! Rivers (W3d, P-139): each of the source's rivers drawn on the tiles along a smooth curve
//! through its cells, as wide as Azgaar draws it and never narrower than a tile; its bed
//! carved never to rise from its source to its mouth (W3e, P-140).

use std::ops::Range;

use factional_core::{Fixed, div_round};

use crate::raster::power;
use crate::source::{SourceCell, SourceRiver};

/// A point in units of 1 / (200 × scale) of a source pixel, so that every tile's centre and
/// every cell's centre, to the hundredth, is a whole number of them.
pub(crate) type At = [i128; 2];

/// A tile's side in the raster's units.
const TILE: i128 = 200;

/// Azgaar's width: its flux factor, the flux^0.7 / 500 a river's width grows by, at most 1
/// pixel, its flux's exponent in hundredths, and its length factor, its width growing by a
/// 200th of a pixel a point along it and by its length progression's 200ths besides.
const FLUX_FACTOR: i128 = 500;
const FLUX_EXPONENT: i64 = 70;
const LENGTH_FACTOR: i128 = 200;
const LENGTH_PROGRESSION: [i128; 9] = [1, 1, 2, 3, 5, 8, 13, 21, 34];

/// A half-width's parts of a pixel: hundredths of the width factor, times the flux factor's
/// share in hundredths, times hundredths of a pixel.
const PIXEL: i128 = 100 * FLUX_FACTOR * 100;

/// A straight piece of a river's line, from one point of its curve to the next: its
/// half-width in the raster's units, the flow over it and the height of its bed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Piece {
    pub from: At,
    pub to: At,
    pub half_width: i128,
    pub flow: u32,
    pub bed: Fixed,
}

/// What a river tile takes from the pieces over it: the largest flow and the lowest bed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Course {
    pub flow: u32,
    pub bed: Fixed,
}

/// The largest flux so far at each of `river`'s anchors, its cells' and, off the map, the
/// last cell's again.
pub(crate) fn running_fluxes(river: &SourceRiver, cells: &[SourceCell]) -> Vec<u32> {
    let mut most = 0;
    let mut running: Vec<u32> = river
        .cells
        .iter()
        .map(|&cell| {
            most = most.max(cells[cell as usize].flux);
            most
        })
        .collect();
    if river.off_map {
        running.push(most);
    }
    running
}

/// Azgaar's half-width at each anchor, in 1 / `PIXEL` of a source pixel, from the largest
/// flux so far at each, `running`: its source width at the first; then its width factor ×
/// ((j + its length progression at j) / 200 + min(flux^0.7 / 500, 1)) + its source width.
pub(crate) fn half_widths(river: &SourceRiver, running: &[u32]) -> Vec<i128> {
    let source = i128::from(river.source_width.hundredths()) * 100 * FLUX_FACTOR;
    let factor = i128::from(river.width_factor.hundredths());
    let last = LENGTH_PROGRESSION[LENGTH_PROGRESSION.len() - 1];
    running
        .iter()
        .enumerate()
        .map(|(j, &flux)| {
            if j == 0 {
                return source;
            }
            let length = j as i128 + LENGTH_PROGRESSION.get(j).copied().unwrap_or(last);
            let flux = power(u128::from(flux) * 100, FLUX_EXPONENT).min(100 * FLUX_FACTOR);
            factor * (length * 100 * FLUX_FACTOR / LENGTH_FACTOR + flux) + source
        })
        .collect()
}

/// `river`'s anchors: its cells' `centres`, and off the map the point on the edge nearest
/// the last, the map's far corner being `corner`: the top, the bottom, the left and the right
/// in that order on a tie, as Azgaar has it.
pub(crate) fn anchors(river: &SourceRiver, centres: &[At], corner: At) -> Vec<At> {
    let mut anchors: Vec<At> = river
        .cells
        .iter()
        .map(|&cell| centres[cell as usize])
        .collect();
    if let (true, Some(&[x, y])) = (river.off_map, anchors.last()) {
        let [width, height] = corner;
        let nearest = [y, height - y, x, width - x]
            .into_iter()
            .min()
            .expect("four sides");
        anchors.push(match nearest {
            _ if nearest == y => [x, 0],
            _ if nearest == height - y => [x, height],
            _ if nearest == x => [0, y],
            _ => [width, y],
        });
    }
    anchors
}

/// The points of the uniform Catmull-Rom curve from `p[1]` to `p[2]`, `p[0]` before and `p[3]`
/// after, at `n` even steps of its parameter, each rounded once: `n + 1` points, the first
/// `p[1]` and the last `p[2]`.
pub(crate) fn stretch(p: [At; 4], n: i128) -> Vec<At> {
    (0..=n)
        .map(|k| {
            [0, 1].map(|axis| {
                let [p0, p1, p2, p3] = p.map(|point| point[axis]);
                // 2n³ × the curve at k / n.
                let twice = 2 * p1 * n * n * n
                    + (p2 - p0) * k * n * n
                    + (2 * p0 - 5 * p1 + 4 * p2 - p3) * k * k * n
                    + (3 * p1 - p0 - 3 * p2 + p3) * k * k * k;
                div_round(twice, 2 * n * n * n)
            })
        })
        .collect()
}

/// The bed of each piece of a river whose line, from its source to its mouth, runs over
/// `grounds`, a piece's bed being that at its far end: the lowest ground so far, never below
/// `floor`, the surface of the water it ends in, if any.
pub(crate) fn beds(grounds: &[Fixed], floor: Option<Fixed>) -> Vec<Fixed> {
    let mut lowest = grounds.first().copied();
    grounds
        .iter()
        .skip(1)
        .map(|&ground| {
            let bed = lowest.map_or(ground, |lowest| lowest.min(ground));
            lowest = Some(bed);
            floor.map_or(bed, |floor| bed.max(floor))
        })
        .collect()
}

/// The pieces of a river through `anchors`, with Azgaar's `half_widths` there and the
/// `running` flux, at `scale` tiles a pixel: each stretch cut into as many pieces as whole
/// tiles its chord spans along its longer axis, at least one, its half-width interpolated
/// between its anchors and rounded once, each piece taking its wider end and the flux where
/// its stretch starts; and its bed, from the `ground` at the points of its line, held at
/// `floor`.
pub(crate) fn pieces(
    anchors: &[At],
    half_widths: &[i128],
    running: &[u32],
    scale: i128,
    ground: impl Fn(At) -> Fixed,
    floor: Option<Fixed>,
) -> Vec<Piece> {
    let last = anchors.len().saturating_sub(1);
    let mut lines = Vec::new();
    for j in 0..last {
        let p = [
            anchors[j.saturating_sub(1)],
            anchors[j],
            anchors[j + 1],
            anchors[(j + 2).min(last)],
        ];
        let chord = (p[2][0] - p[1][0]).abs().max((p[2][1] - p[1][1]).abs());
        let n = ((chord + TILE - 1) / TILE).max(1);
        let points = stretch(p, n);
        let widths: Vec<i128> = (0..=n)
            .map(|k| {
                let between = half_widths[j] * (n - k) + half_widths[j + 1] * k;
                div_round(between * TILE * scale, PIXEL * n)
            })
            .collect();
        lines.extend((0..points.len() - 1).map(|k| {
            (
                points[k],
                points[k + 1],
                widths[k].max(widths[k + 1]),
                running[j],
            )
        }));
    }
    let grounds: Vec<Fixed> = lines
        .first()
        .map(|&(from, ..)| from)
        .into_iter()
        .chain(lines.iter().map(|&(_, to, ..)| to))
        .map(ground)
        .collect();
    lines
        .into_iter()
        .zip(beds(&grounds, floor))
        .map(|((from, to, half_width, flow), bed)| Piece {
            from,
            to,
            half_width,
            flow,
            bed,
        })
        .collect()
}

/// The course on each tile of a map `width` tiles a row, its first tile's corner at `origin`,
/// that is `land` and lies under one of `pieces`: within its half-width of the centre, or
/// touched by it; the largest flow of any, and the lowest bed.
pub(crate) fn paint(
    width: usize,
    origin: At,
    land: &[bool],
    pieces: &[Piece],
) -> Vec<Option<Course>> {
    let height = land.len() / width.max(1);
    let mut courses: Vec<Option<Course>> = vec![None; land.len()];
    for piece in pieces {
        let [xs, ys] = near(piece, origin, [width, height]);
        for y in ys {
            for x in xs.clone() {
                let i = y * width + x;
                let centre = [x, y].map(|t| t as i128 * TILE + TILE / 2);
                let centre = [origin[0] + centre[0], origin[1] + centre[1]];
                if land[i] && (within(centre, piece) || touches(centre, piece)) {
                    let (flow, bed) = (piece.flow, piece.bed);
                    courses[i] = Some(courses[i].map_or(Course { flow, bed }, |course| Course {
                        flow: flow.max(course.flow),
                        bed: bed.min(course.bed),
                    }));
                }
            }
        }
    }
    courses
}

/// The columns and rows of a map `size` tiles, its first tile's corner at `origin`, whose
/// tiles might lie under `piece`: their centres within its bounds widened by its half-width,
/// or by half a tile if that's more.
fn near(piece: &Piece, origin: At, size: [usize; 2]) -> [Range<usize>; 2] {
    let reach = piece.half_width.max(TILE / 2);
    [0, 1].map(|axis| {
        let [a, b] = [piece.from[axis], piece.to[axis]];
        span(a.min(b) - reach, a.max(b) + reach, origin[axis], size[axis])
    })
}

/// The tiles of a row or column of `count`, its first tile's edge at `origin`, whose centres
/// lie from `low` to `high`.
fn span(low: i128, high: i128, origin: i128, count: usize) -> Range<usize> {
    let first = -(origin + TILE / 2 - low).div_euclid(TILE);
    let last = (high - origin - TILE / 2).div_euclid(TILE);
    let clamp = |tile: i128| tile.clamp(0, count as i128) as usize;
    clamp(first)..clamp(last + 1)
}

/// Whether `centre` lies within `piece`'s half-width of it.
fn within(centre: At, piece: &Piece) -> bool {
    let [a, b] = [piece.from, piece.to];
    let d = [b[0] - a[0], b[1] - a[1]];
    let w = [centre[0] - a[0], centre[1] - a[1]];
    let (along, length) = (w[0] * d[0] + w[1] * d[1], d[0] * d[0] + d[1] * d[1]);
    let reach = piece.half_width * piece.half_width;
    let squared = |p: At| p[0] * p[0] + p[1] * p[1];
    match along {
        _ if along <= 0 => squared(w) <= reach,
        _ if along >= length => squared([centre[0] - b[0], centre[1] - b[1]]) <= reach,
        _ => {
            let across = w[0] * d[1] - w[1] * d[0];
            across * across <= reach * length
        }
    }
}

/// Whether `piece` touches the square of the tile centred at `centre`, its edges included.
fn touches(centre: At, piece: &Piece) -> bool {
    let [a, b] = [piece.from, piece.to];
    let [low, high] = [-TILE / 2, TILE / 2].map(|off| [centre[0] + off, centre[1] + off]);
    let apart =
        (0..2).any(|axis| a[axis].max(b[axis]) < low[axis] || a[axis].min(b[axis]) > high[axis]);
    let d = [b[0] - a[0], b[1] - a[1]];
    let sides = [low[0], high[0]]
        .into_iter()
        .flat_map(|x| [low[1], high[1]].map(|y| d[0] * (y - a[1]) - d[1] * (x - a[0])));
    let (mut above, mut below) = (false, false);
    for side in sides {
        above |= side >= 0;
        below |= side <= 0;
    }
    !apart && above && below
}

#[cfg(test)]
mod tests {
    use factional_core::Fixed;

    use super::*;
    use crate::source::Point;

    fn cell(flux: u32) -> SourceCell {
        SourceCell {
            centre: Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            height: 30,
            feature: 0,
            neighbours: Vec::new(),
            corners: Vec::new(),
            flux,
            biome: crate::Biome::Grassland,
            temperature: 0,
            precipitation: 0,
            culture: None,
            religion: None,
            state: None,
        }
    }

    fn river(cells: &[u32], off_map: bool, width_factor: i64, source_width: i64) -> SourceRiver {
        SourceRiver {
            cells: cells.to_vec(),
            off_map,
            width_factor: Fixed::from_hundredths(width_factor),
            source_width: Fixed::from_hundredths(source_width),
        }
    }

    /// The fixture's fluxes, `rivers.json`'s: the sea, Beck and Rill's source, the lake,
    /// Ford's source and the cell where they meet.
    fn fixture() -> Vec<SourceCell> {
        [0, 40, 900, 20, 2000].map(cell).to_vec()
    }

    #[test]
    fn the_flux_so_far_is_the_largest_of_the_rivers_cells_and_off_the_map_the_last() {
        let cells = fixture();
        assert_eq!(
            running_fluxes(&river(&[3, 4, 2], false, 1_20, 5), &cells),
            [20, 2000, 2000]
        );
        assert_eq!(
            running_fluxes(&river(&[1, 4], false, 1_00, 4), &cells),
            [40, 2000]
        );
        assert_eq!(
            running_fluxes(&river(&[1], true, 1_00, 4), &cells),
            [40, 40]
        );
    }

    #[test]
    fn the_half_width_is_azgaars_offset() {
        // Worked by hand: F^0.7 in hundredths is 1323 at 40 and 20451 at 2000. Ford's middle
        // is 1.20 × ((1 + 1) / 200 + 204.51 / 500) + 0.05 = 0.552824 pixels.
        assert_eq!(
            half_widths(&river(&[3, 4, 2], false, 1_20, 5), &[20, 2000, 2000]),
            [250_000, 2_764_120, 2_824_120]
        );
        assert_eq!(
            half_widths(&river(&[1, 4], false, 1_00, 4), &[40, 2000]),
            [200_000, 2_295_100]
        );
        assert_eq!(
            half_widths(&river(&[1], true, 1_00, 4), &[40, 40]),
            [200_000, 382_300]
        );
    }

    #[test]
    fn the_flux_term_is_at_most_one_pixel_and_the_length_term_grows_by_fibonacci() {
        // 500^(1/0.7) is about 7200: past it the flux adds 1 pixel, times the factor.
        let long = river(&[0; 11], false, 1_00, 0);
        let widths = half_widths(&long, &[100_000; 11]);
        // (j + P[j]) / 200 + 1, P being 1, 1, 2, 3, 5, 8, 13, 21, 34 and then 34.
        let expected = [0, 2, 4, 6, 9, 13, 19, 28, 42, 43, 44].map(|length| {
            if length == 0 {
                0
            } else {
                length * 25_000 + 5_000_000
            }
        });
        assert_eq!(widths, expected);
    }

    #[test]
    fn off_the_map_a_river_ends_on_the_nearest_edge_top_bottom_left_right() {
        let corner = [8000, 6000];
        let off = |at: At| anchors(&river(&[0], true, 1_00, 4), &[at], corner)[1];
        assert_eq!(off([6100, 2100]), [8000, 2100]);
        assert_eq!(off([1000, 2100]), [0, 2100]);
        assert_eq!(off([3000, 900]), [3000, 0]);
        assert_eq!(off([3000, 5500]), [3000, 6000]);
        // Ties: top before bottom before left before right.
        assert_eq!(off([3000, 3000]), [3000, 0]);
        assert_eq!(off([500, 5500]), [500, 6000]);
        assert_eq!(off([500, 3000]), [0, 3000]);
        assert_eq!(
            anchors(
                &river(&[1, 0], false, 1_00, 4),
                &[[10, 20], [30, 40]],
                corner
            ),
            [[30, 40], [10, 20]]
        );
    }

    #[test]
    fn the_curve_passes_through_its_anchors() {
        let p = [[0, 0], [1000, 400], [3000, 0], [3500, 2500]];
        for n in [1, 3, 8] {
            let points = stretch(p, n);
            assert_eq!(points.len() as i128, n + 1);
            assert_eq!(points.first(), Some(&[1000, 400]));
            assert_eq!(points.last(), Some(&[3000, 0]));
        }
    }

    #[test]
    fn the_curve_through_anchors_on_a_line_lies_on_it() {
        // Evenly spaced, so the points are evenly spaced too.
        assert_eq!(
            stretch([[0, 0], [400, 200], [800, 400], [1200, 600]], 4),
            [[400, 200], [500, 250], [600, 300], [700, 350], [800, 400]]
        );
        // Doubled at the ends, as a river's first and last stretch are.
        let points = stretch([[0, 0], [0, 0], [800, 0], [800, 0]], 4);
        assert!(points.iter().all(|&[_, y]| y == 0), "{points:?}");
        assert!(
            points.windows(2).all(|pair| pair[0][0] < pair[1][0]),
            "{points:?}"
        );
    }

    #[test]
    fn the_curve_bends_by_catmull_rom() {
        // Worked by hand at t = 1/2, (−P0 + 9 P1 + 9 P2 − P3) / 16: x is 32500 / 16 = 2031.25
        // and y is 1100 / 16 = 68.75.
        let points = stretch([[0, 0], [1000, 400], [3000, 0], [3500, 2500]], 2);
        assert_eq!(points[1], [2031, 69]);
    }

    #[test]
    fn a_stretch_takes_a_piece_for_each_whole_tile_its_chord_spans() {
        // At scale 1 a tile is 200 units: 1000 units across is 5 pieces, 1001 is 6, and a
        // stretch shorter than a tile still has 1.
        let count = |to: At| pieces(&[[0, 0], to], &[0, 0], &[1, 1], 1, flat, None).len();
        assert_eq!(count([1000, 300]), 5);
        assert_eq!(count([-300, 1001]), 6);
        assert_eq!(count([50, 50]), 1);
    }

    /// Level ground, at 0 m.
    fn flat(_: At) -> Fixed {
        Fixed::ZERO
    }

    /// `metres` m.
    fn m(metres: i64) -> Fixed {
        Fixed::from_hundredths(metres * 100)
    }

    fn metres(values: &[i64]) -> Vec<Fixed> {
        values.iter().map(|&v| m(v)).collect()
    }

    #[test]
    fn a_piece_takes_the_wider_end_and_the_flow_where_its_stretch_starts() {
        // Half-widths of 0.5 and 1.0 pixels at scale 2: 200 and 400 units, 300 halfway; two
        // tiles long, so two pieces.
        let made = pieces(
            &[[0, 0], [400, 0]],
            &[2_500_000, 5_000_000],
            &[7, 9],
            2,
            flat,
            None,
        );
        assert_eq!(made.len(), 2);
        assert_eq!(
            made[0],
            Piece {
                from: [0, 0],
                to: [200, 0],
                half_width: 300,
                flow: 7,
                bed: Fixed::ZERO,
            }
        );
        assert_eq!(made[1].half_width, 400);
        assert_eq!(made[1].flow, 7);
    }

    #[test]
    fn a_bed_is_the_lowest_ground_so_far_from_the_source() {
        assert_eq!(beds(&metres(&[5, 3, 4, 1, 2]), None), metres(&[3, 3, 1, 1]));
        assert_eq!(beds(&metres(&[1, 2, 3]), None), metres(&[1, 1]));
        assert_eq!(beds(&metres(&[4]), None), metres(&[]));
    }

    #[test]
    fn a_bed_is_never_below_the_surface_of_the_water_it_ends_in() {
        let floor = Some(m(2));
        assert_eq!(
            beds(&metres(&[5, 3, 4, 1, 2]), floor),
            metres(&[3, 3, 2, 2])
        );
        // Ground below the floor at the source is held there too.
        assert_eq!(beds(&metres(&[1, 5]), floor), metres(&[2]));
    }

    #[test]
    fn a_piece_takes_the_bed_at_its_far_end() {
        // Two tiles long, so two pieces, ending at x 200 and 400; the ground there 300, 100
        // and 200 m along it.
        let ground = |at: At| {
            m(match at[0] {
                0 => 300,
                200 => 100,
                _ => 200,
            })
        };
        let beds = |floor: Option<Fixed>| -> Vec<(At, Fixed)> {
            pieces(&[[0, 0], [400, 0]], &[0, 0], &[1, 1], 1, ground, floor)
                .iter()
                .map(|piece| (piece.to, piece.bed))
                .collect()
        };
        assert_eq!(beds(None), [([200, 0], m(100)), ([400, 0], m(100))]);
        assert_eq!(beds(Some(m(150))), [([200, 0], m(150)), ([400, 0], m(150))]);
    }

    /// `pieces` painted on a `rows`-drawn map, `.` land and `~` water, at scale 1, drawn
    /// again with each river tile its flow's digit.
    fn painted(rows: &[&str], pieces: &[Piece]) -> Vec<String> {
        let width = rows[0].len();
        let land: Vec<bool> = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c == '.'))
            .collect();
        let courses = paint(width, [0, 0], &land, pieces);
        courses
            .chunks(width)
            .zip(land.chunks(width))
            .map(|(courses, land)| {
                courses
                    .iter()
                    .zip(land)
                    .map(|(course, &land)| match (course, land) {
                        (Some(course), _) => char::from_digit(course.flow, 10).expect("a digit"),
                        (None, true) => '.',
                        (None, false) => '~',
                    })
                    .collect()
            })
            .collect()
    }

    fn piece(from: At, to: At, half_width: i128, flow: u32) -> Piece {
        Piece {
            from,
            to,
            half_width,
            flow,
            bed: Fixed::ZERO,
        }
    }

    const BLANK: [&str; 5] = ["....."; 5];

    #[test]
    fn a_span_is_the_tiles_whose_centres_lie_within_it_ends_included() {
        // Tile t's centre is at 200t + 100, after the origin.
        assert_eq!(span(100, 700, 0, 5), 0..4);
        assert_eq!(span(101, 699, 0, 5), 1..3);
        assert_eq!(span(300, 300, 0, 5), 1..2);
        assert_eq!(span(301, 499, 0, 5), 2..2);
        assert_eq!(span(-500, 3000, 0, 5), 0..5);
        assert_eq!(span(1100, 1300, 1000, 5), 0..2);
        assert_eq!(span(-900, -100, 0, 5), 0..0);
    }

    #[test]
    fn a_piece_is_looked_for_within_its_half_width_or_half_a_tile() {
        let row = piece([100, 500], [700, 500], 0, 1);
        assert_eq!(near(&row, [0, 0], [5, 5]), [0..4, 2..3]);
        assert_eq!(
            near(&piece([100, 500], [700, 500], 250, 1), [0, 0], [5, 5]),
            [0..5, 1..4]
        );
        assert_eq!(
            near(&piece([700, 500], [100, 500], 0, 1), [0, 0], [5, 5]),
            [0..4, 2..3]
        );
        assert_eq!(
            near(&piece([1100, 500], [1700, 500], 0, 1), [1000, 0], [5, 5]),
            [0..4, 2..3]
        );
    }

    #[test]
    fn a_thin_line_takes_the_tiles_it_touches() {
        // Along the centres of row 2, from the first tile's centre to the fourth's.
        assert_eq!(
            painted(&BLANK, &[piece([100, 500], [700, 500], 0, 1)]),
            [".....", ".....", "1111.", ".....", "....."]
        );
        // Along the border of rows 1 and 2, so it touches both.
        assert_eq!(
            painted(&BLANK, &[piece([100, 400], [700, 400], 0, 1)]),
            [".....", "1111.", "1111.", ".....", "....."]
        );
    }

    #[test]
    fn a_thin_diagonal_is_joined_edge_to_edge() {
        // y = 150 + x / 2, which passes no tile's corner: column i spans y 150 + 100i to
        // 250 + 100i.
        assert_eq!(
            painted(&BLANK, &[piece([0, 150], [1000, 650], 0, 1)]),
            ["1....", "111..", "..111", "....1", "....."]
        );
    }

    #[test]
    fn a_wide_line_takes_every_centre_within_its_half_width() {
        // 1.25 tiles either side of row 2's centres from x 100 to 700: rows 1 to 3, and the
        // fifth tile of row 2, 1 tile from the end; its neighbours above and below are √2 away.
        assert_eq!(
            painted(&BLANK, &[piece([100, 500], [700, 500], 250, 1)]),
            [".....", "1111.", "11111", "1111.", "....."]
        );
        // Exactly at the half-width is in.
        assert_eq!(
            painted(&BLANK, &[piece([100, 500], [700, 500], 200, 1)]),
            [".....", "1111.", "11111", "1111.", "....."]
        );
    }

    #[test]
    fn a_wide_line_takes_no_tile_past_its_end_that_its_line_runs_on_to() {
        // (2, 2)'s centre is 283 from the end, past the half-width of 250, and its square lies
        // outside the piece's bounds, though the line would run on through it.
        assert_eq!(
            painted(&BLANK, &[piece([100, 100], [300, 300], 250, 1)]),
            ["11...", "111..", ".1...", ".....", "....."]
        );
    }

    #[test]
    fn rivers_are_painted_on_land_only_and_the_larger_flow_wins() {
        assert_eq!(
            painted(
                &["..~..", "..~..", ".....", ".....", "....."],
                &[
                    piece([500, 100], [500, 900], 0, 9),
                    piece([100, 500], [900, 500], 0, 5),
                ]
            ),
            ["..~..", "..~..", "55955", "..9..", "..9.."]
        );
    }

    #[test]
    fn a_tile_under_two_pieces_takes_the_lower_bed() {
        let beds = |first: i64, second: i64| {
            let pieces = [
                Piece {
                    bed: m(first),
                    ..piece([500, 100], [500, 900], 0, 9)
                },
                Piece {
                    bed: m(second),
                    ..piece([100, 500], [900, 500], 0, 5)
                },
            ];
            let courses = paint(5, [0, 0], &[true; 25], &pieces);
            [12, 2, 10].map(|i| courses[i].map(|course| (course.flow, course.bed)))
        };
        // Where they cross, the larger flow and the lower bed, whichever is painted first.
        assert_eq!(
            beds(30, 20),
            [Some((9, m(20))), Some((9, m(30))), Some((5, m(20)))]
        );
        assert_eq!(beds(20, 30)[0], Some((9, m(20))));
    }
}

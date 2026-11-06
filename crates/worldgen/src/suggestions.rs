//! Suggestions (W6b, P-145): the source's settlements, routes and points of interest placed on
//! the map, by tile and offset in 256ths of a tile, as a plant is; those outside the region
//! dropped, and each route cut exactly at the map's edge into its stretches inside. Each keeps
//! what the source says of it, and a stable id from its kind and Azgaar's id. Nothing is built
//! from them (D-49). Cultures, religions and states (W6e, P-148) are suggested when a tile of
//! the map lies in one of their cells, with their tiles counted.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use factional_core::{Fixed, div_round};

use crate::{
    Buildings, Extent, Point, Recipe, ReligionKind, RouteKind, SettlementKind, Size, Source,
    Stance, StateForm, TilePos,
};

/// A place on the map: its tile, and where in the tile, in 256ths each way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub tile: TilePos,
    pub offset: [u8; 2],
}

/// A settlement suggested: its id, name and place, its people, type, whether it's a capital
/// and a port, and its buildings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub id: String,
    pub name: String,
    pub at: Placed,
    pub people: u64,
    pub kind: SettlementKind,
    pub capital: bool,
    pub port: bool,
    pub buildings: Buildings,
}

/// A route suggested, or one stretch of it: its id, its name if it has one, its kind, and its
/// points, at least two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub id: String,
    pub name: Option<String>,
    pub kind: RouteKind,
    pub points: Vec<Placed>,
}

/// A point of interest suggested: its id, its type and icon as Azgaar names them, its name and
/// note if it has them, and its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointOfInterest {
    pub id: String,
    pub kind: String,
    pub icon: String,
    pub name: Option<String>,
    pub note: Option<String>,
    pub at: Placed,
}

/// A culture suggested (W6e): its id, name and type, its extent in the whole source, and how
/// many of the map's tiles lie in its cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Culture {
    pub id: String,
    pub name: String,
    pub kind: SettlementKind,
    pub extent: Extent,
    pub tiles: u32,
}

/// A religion suggested (W6e): its id, name, type, form and deity if any, its culture's id if
/// it has one, on the map or not, its extent in the whole source, and its tiles on the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Religion {
    pub id: String,
    pub name: String,
    pub kind: ReligionKind,
    pub form: String,
    pub deity: Option<String>,
    pub culture: Option<String>,
    pub extent: Extent,
    pub tiles: u32,
}

/// A state suggested (W6e): its id, name, full name and form, its culture's and capital's ids
/// if any and its stance toward each other state by id, each on the map or not, its extent in
/// the whole source, and its tiles on the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub id: String,
    pub name: String,
    pub full_name: String,
    pub form: StateForm,
    pub culture: Option<String>,
    pub capital: Option<String>,
    pub stances: Vec<(String, Stance)>,
    pub extent: Extent,
    pub tiles: u32,
}

/// A map's suggestions: its settlements, routes and points of interest, and its cultures,
/// religions and states, each in the source's order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub settlements: Vec<Settlement>,
    pub routes: Vec<Route>,
    pub points: Vec<PointOfInterest>,
    pub cultures: Vec<Culture>,
    pub religions: Vec<Religion>,
    pub states: Vec<State>,
}

impl Suggestions {
    /// Every suggestion's id, in order: settlements, routes, points of interest, cultures,
    /// religions, then states.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        (self.settlements.iter().map(|s| s.id.as_str()))
            .chain(self.routes.iter().map(|r| r.id.as_str()))
            .chain(self.points.iter().map(|p| p.id.as_str()))
            .chain(self.cultures.iter().map(|c| c.id.as_str()))
            .chain(self.religions.iter().map(|r| r.id.as_str()))
            .chain(self.states.iter().map(|s| s.id.as_str()))
    }

    /// How many suggestions there are, of every kind.
    pub fn len(&self) -> usize {
        self.ids().count()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A coordinate on the map in 256ths of a tile: (`at` − `corner`) × `scale` × 256, `at` and
/// `corner` in source pixels, rounded once, half away from zero.
pub(crate) fn place(at: Fixed, corner: u32, scale: u32) -> i64 {
    let from = i128::from(at.hundredths()) - i128::from(corner) * 100;
    i64::try_from(div_round(from * i128::from(scale) * 256, 100)).expect("a source's pixels")
}

/// The piece from `a` to `b` that lies within 0 to `last` each way, in 256ths, if any: where it
/// starts and ends, each crossing worked exactly and its other coordinate rounded once, and
/// whether it starts at `a` and ends at `b`.
pub(crate) fn clip(a: [i64; 2], b: [i64; 2], last: [i64; 2]) -> Option<Clipped> {
    // Liang and Barsky's: the piece is a + t(b − a), t from 0 to 1, narrowed at each edge it
    // crosses; each t a fraction, its denominator positive, compared exactly.
    let later = |(n, d): (i128, i128), (m, e): (i128, i128)| n * e > m * d;
    let (mut enter, mut exit) = ((0, 1), (1, 1));
    for k in 0..2 {
        let d = i128::from(b[k] - a[k]);
        // Past 0, then past the last.
        for (p, q) in [(-d, i128::from(a[k])), (d, i128::from(last[k] - a[k]))] {
            match p.cmp(&0) {
                Ordering::Equal if q < 0 => return None,
                Ordering::Equal => {}
                Ordering::Less if later((-q, -p), enter) => enter = (-q, -p),
                Ordering::Greater if later(exit, (q, p)) => exit = (q, p),
                Ordering::Less | Ordering::Greater => {}
            }
        }
    }
    if later(enter, exit) {
        return None;
    }
    let at = |(n, d): (i128, i128)| {
        [0, 1].map(|k| {
            let along = div_round(n * i128::from(b[k] - a[k]), d);
            a[k] + i64::try_from(along).expect("within the piece")
        })
    };
    Some(Clipped {
        from: at(enter),
        to: at(exit),
        whole_start: enter.0 == 0,
        whole_end: exit.0 == exit.1,
    })
}

/// A piece of a route within the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Clipped {
    pub from: [i64; 2],
    pub to: [i64; 2],
    pub whole_start: bool,
    pub whole_end: bool,
}

/// The stretches of the route through `points` within 0 to `last` each way: each run of it
/// inside, a point repeated in a row kept once, of at least two points.
pub(crate) fn stretches(points: &[[i64; 2]], last: [i64; 2]) -> Vec<Vec<[i64; 2]>> {
    let mut runs: Vec<Vec<[i64; 2]>> = Vec::new();
    // Whether the last run reached the end of its piece, still inside.
    let mut open = false;
    for pair in points.windows(2) {
        match clip(pair[0], pair[1], last) {
            None => open = false,
            Some(piece) => {
                if !(open && piece.whole_start) {
                    runs.push(vec![piece.from]);
                }
                runs.last_mut().expect("a run begun").push(piece.to);
                open = piece.whole_end;
            }
        }
    }
    runs.into_iter()
        .map(|mut run| {
            run.dedup();
            run
        })
        .filter(|run| run.len() >= 2)
        .collect()
}

/// What `source` suggests on the map of `size` tiles that `recipe` makes of it: its places on a map of `size` made by `recipe`; and its cultures, religions and
/// states, each counted by the tiles lying in its cells, `cells` holding each tile's cell, row
/// by row.
pub(crate) fn suggest(
    source: &Source,
    recipe: &Recipe,
    size: Size,
    cells: &[usize],
) -> Suggestions {
    let (region, scale) = (recipe.region, recipe.scale);
    let tiles = Tiles::count(source, cells);
    let at = |point: Point| {
        [
            place(point.x, region.x, scale),
            place(point.y, region.y, scale),
        ]
    };
    let last = [size.width, size.height].map(|side| i64::from(side) * 256 - 1);
    let on = |at: [i64; 2]| (0..=last[0]).contains(&at[0]) && (0..=last[1]).contains(&at[1]);
    let placed = |at: [i64; 2]| Placed {
        tile: TilePos {
            x: (at[0] / 256) as u32,
            y: (at[1] / 256) as u32,
        },
        offset: at.map(|at| u8::try_from(at % 256).expect("a 256th")),
    };
    Suggestions {
        settlements: source
            .settlements
            .iter()
            .filter(|settlement| on(at(settlement.at)))
            .map(|settlement| Settlement {
                id: format!("settlement_{}", settlement.id),
                name: settlement.name.clone(),
                at: placed(at(settlement.at)),
                people: settlement.people,
                kind: settlement.kind,
                capital: settlement.capital,
                port: settlement.port,
                buildings: settlement.buildings,
            })
            .collect(),
        routes: source
            .routes
            .iter()
            .flat_map(|route| {
                let points: Vec<[i64; 2]> = route.points.iter().map(|&point| at(point)).collect();
                let runs = stretches(&points, last);
                let cut = runs.len() > 1;
                runs.into_iter().zip(1..).map(move |(run, k)| Route {
                    id: match cut {
                        true => format!("route_{}_{k}", route.id),
                        false => format!("route_{}", route.id),
                    },
                    name: route.name.clone(),
                    kind: route.kind,
                    points: run.into_iter().map(placed).collect(),
                })
            })
            .collect(),
        points: source
            .markers
            .iter()
            .filter(|marker| on(at(marker.at)))
            .map(|marker| PointOfInterest {
                id: format!("poi_{}", marker.id),
                kind: marker.kind.clone(),
                icon: marker.icon.clone(),
                name: marker.name.clone(),
                note: marker.note.clone(),
                at: placed(at(marker.at)),
            })
            .collect(),
        cultures: source
            .cultures
            .iter()
            .filter_map(|culture| {
                Some(Culture {
                    id: format!("culture_{}", culture.id),
                    name: culture.name.clone(),
                    kind: culture.kind,
                    extent: culture.extent,
                    tiles: *tiles.cultures.get(&culture.id)?,
                })
            })
            .collect(),
        religions: source
            .religions
            .iter()
            .filter_map(|religion| {
                Some(Religion {
                    id: format!("religion_{}", religion.id),
                    name: religion.name.clone(),
                    kind: religion.kind,
                    form: religion.form.clone(),
                    deity: religion.deity.clone(),
                    culture: religion.culture.map(|id| format!("culture_{id}")),
                    extent: religion.extent,
                    tiles: *tiles.religions.get(&religion.id)?,
                })
            })
            .collect(),
        states: source
            .states
            .iter()
            .filter_map(|state| {
                Some(State {
                    id: format!("state_{}", state.id),
                    name: state.name.clone(),
                    full_name: state.full_name.clone(),
                    form: state.form,
                    culture: state.culture.map(|id| format!("culture_{id}")),
                    capital: state.capital.map(|id| format!("settlement_{id}")),
                    stances: (state.stances.iter())
                        .map(|&(toward, stance)| (format!("state_{toward}"), stance))
                        .collect(),
                    extent: state.extent,
                    tiles: *tiles.states.get(&state.id)?,
                })
            })
            .collect(),
    }
}

/// How many tiles lie in each culture's, religion's and state's cells, by Azgaar's id.
#[derive(Default)]
struct Tiles {
    cultures: BTreeMap<u32, u32>,
    religions: BTreeMap<u32, u32>,
    states: BTreeMap<u32, u32>,
}

impl Tiles {
    /// The tiles in each, `cells` holding each tile's cell in `source`.
    fn count(source: &Source, cells: &[usize]) -> Tiles {
        let mut tiles = Tiles::default();
        for &cell in cells {
            let cell = &source.cells[cell];
            for (counts, id) in [
                (&mut tiles.cultures, cell.culture),
                (&mut tiles.religions, cell.religion),
                (&mut tiles.states, cell.state),
            ] {
                if let Some(id) = id {
                    *counts.entry(id).or_default() += 1;
                }
            }
        }
        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    const NONE: Vec<Vec<[i64; 2]>> = Vec::new();

    /// The last 256th of a 40 × 40 tile map, each way.
    const FORTY: [i64; 2] = [10_239, 10_239];

    #[test]
    fn a_place_is_its_pixels_from_the_corner_times_the_scale_in_256ths() {
        // Highmoor's 30.25 and 9.75 at 1 tile a pixel: 7744 and 2496, exactly.
        assert_eq!(place(px(30_25), 0, 1), 7744);
        assert_eq!(place(px(9_75), 0, 1), 2496);
        // Lakeside's 14.5, from a corner at 10, at 2 tiles a pixel: 4.5 × 2 × 256.
        assert_eq!(place(px(14_50), 10, 2), 2304);
        // A pixel before the corner is negative.
        assert_eq!(place(px(9_75), 10, 1), -64);
    }

    #[test]
    fn a_place_is_rounded_once_to_the_nearest_256th() {
        // Ash Peak's 33.3 and 34.4: 8524.8 up to 8525, and 8806.4 down to 8806.
        assert_eq!(place(px(33_30), 0, 1), 8525);
        assert_eq!(place(px(34_40), 0, 1), 8806);
        // 0.01 pixel is 2.56 256ths either side of the corner: 3 away from it, each way.
        assert_eq!(place(px(10_01), 10, 1), 3);
        assert_eq!(place(px(9_99), 10, 1), -3);
        // At 3 tiles a pixel: 0.01 × 3 × 256 = 7.68, so 8.
        assert_eq!(place(px(1), 0, 3), 8);
    }

    #[test]
    fn a_piece_inside_the_map_is_kept_whole() {
        let whole = |from, to| {
            Some(Clipped {
                from,
                to,
                whole_start: true,
                whole_end: true,
            })
        };
        assert_eq!(
            clip([5376, 5376], [7936, 2816], FORTY),
            whole([5376, 5376], [7936, 2816])
        );
        // The last 256th each way is still on the map, and so is 0.
        assert_eq!(
            clip([10_239, 0], [10_239, 10_239], FORTY),
            whole([10_239, 0], [10_239, 10_239])
        );
        assert_eq!(clip([0, 0], [0, 0], FORTY), whole([0, 0], [0, 0]));
    }

    #[test]
    fn a_piece_crossing_the_edge_is_cut_exactly_where_it_crosses() {
        // The High road's first piece, from (10368, −128) to (7936, 2816), crosses the right
        // edge at t = 129/2432, before the top at 128/2944: y = −128 + 129 × 2944 / 2432 =
        // −128 + 156.16, so 28.
        assert_eq!(
            clip([10_368, -128], [7936, 2816], FORTY),
            Some(Clipped {
                from: [10_239, 28],
                to: [7936, 2816],
                whole_start: false,
                whole_end: true,
            })
        );
        // Its last, from (5376, 5376) to (11008, 10752), leaves by the right edge at
        // t = 4863/5632, before the bottom: y = 5376 + 4863 × 5376 / 5632 = 5376 + 4641.97, so
        // 10018.
        assert_eq!(
            clip([5376, 5376], [11_008, 10_752], FORTY),
            Some(Clipped {
                from: [5376, 5376],
                to: [10_239, 10_018],
                whole_start: true,
                whole_end: false,
            })
        );
        // Right through, left to right.
        assert_eq!(
            clip([-100, 50], [200, 50], [100, 100]),
            Some(Clipped {
                from: [0, 50],
                to: [100, 50],
                whole_start: false,
                whole_end: false,
            })
        );
        // And bottom to top.
        assert_eq!(
            clip([30, 150], [30, -50], [100, 100]),
            Some(Clipped {
                from: [30, 100],
                to: [30, 0],
                whole_start: false,
                whole_end: false,
            })
        );
    }

    #[test]
    fn a_piece_wholly_off_the_map_is_none() {
        // Beside the map, each side.
        assert_eq!(clip([-5, 0], [-1, 100], [100, 100]), None);
        assert_eq!(clip([101, 0], [150, 100], [100, 100]), None);
        assert_eq!(clip([0, -5], [100, -1], [100, 100]), None);
        assert_eq!(clip([0, 101], [100, 150], [100, 100]), None);
        // Across a corner, missing it: x + y = −5.
        assert_eq!(clip([-10, 5], [5, -10], [100, 100]), None);
        // A point off the map.
        assert_eq!(clip([-1, 5], [-1, 5], [100, 100]), None);
    }

    #[test]
    fn a_piece_touching_a_corner_is_that_point() {
        // x + y = 0 meets the map at (0, 0) alone, at t = 1/2.
        assert_eq!(
            clip([-10, 10], [10, -10], [100, 100]),
            Some(Clipped {
                from: [0, 0],
                to: [0, 0],
                whole_start: false,
                whole_end: false,
            })
        );
    }

    #[test]
    fn a_route_inside_is_one_stretch_and_one_crossing_the_edge_is_cut_there() {
        // The High road in raster.json's region (10, 10, 20, 20) at 2 tiles a pixel.
        assert_eq!(
            stretches(
                &[[10_368, -128], [7936, 2816], [5376, 5376], [11_008, 10_752]],
                FORTY
            ),
            vec![vec![
                [10_239, 28],
                [7936, 2816],
                [5376, 5376],
                [10_239, 10_018]
            ]]
        );
        assert_eq!(
            stretches(&[[3712, 6848], [5248, 5248]], FORTY),
            vec![vec![[3712, 6848], [5248, 5248]]]
        );
    }

    #[test]
    fn a_route_out_of_the_map_and_back_is_two_stretches() {
        // The Coast road: out by the right edge at t = 9215/11776, y = 1024 + 1201.95; back in
        // at t = 2561/5120, y = 2560 + 2561.
        assert_eq!(
            stretches(&[[1024, 1024], [12_800, 2560], [7680, 7680]], FORTY),
            vec![
                vec![[1024, 1024], [10_239, 2226]],
                vec![[10_239, 5121], [7680, 7680]]
            ]
        );
    }

    #[test]
    fn a_stretch_needs_two_different_points() {
        assert_eq!(stretches(&[[-5, 0], [-1, 100]], [100, 100]), NONE);
        assert_eq!(stretches(&[[-10, 10], [10, -10]], [100, 100]), NONE);
        assert_eq!(stretches(&[[5, 5], [5, 5]], [100, 100]), NONE);
        assert_eq!(stretches(&[[5, 5]], [100, 100]), NONE);
        assert_eq!(
            stretches(&[[5, 5], [5, 5], [9, 9], [9, 9]], [100, 100]),
            vec![vec![[5, 5], [9, 9]]]
        );
        // To the edge, out, and back in to the edge only: the touch back is a single point.
        assert_eq!(
            stretches(&[[50, 50], [100, 50], [150, 50], [100, 60]], [100, 100]),
            vec![vec![[50, 50], [100, 50]]]
        );
    }

    #[test]
    fn a_stretch_running_off_and_on_through_a_corner_is_two() {
        // To the top edge at (20, 0) and out from there, round the corner off the map, and
        // back in at the left edge at (0, 20), then on: leaving adds only (20, 0) again.
        assert_eq!(
            stretches(
                &[[40, 20], [20, 0], [0, -20], [-20, 0], [0, 20], [30, 30]],
                [100, 100]
            ),
            vec![vec![[40, 20], [20, 0]], vec![[0, 20], [30, 30]]]
        );
    }
}

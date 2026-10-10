//! Where the Quests tab puts things (U4, P-80), worked out apart from drawing so it's tested
//! like the rest of the model. Positions are in points, from the `origin` the graph is drawn
//! at, so the egui layer only paints and places what it's given. Floats here are egui's
//! layout, never content (DESIGN.md §18).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Range;

use factional_content::{LineView, Step, ValuePath};
use factional_quests::{Next, Quest};
use factional_reputation::{MAP_CELLS, MapCell};

use crate::egui::{Pos2, Rect, Vec2};

/// A node's width, and the space between columns.
pub const COLUMN: f32 = 210.0;
pub const GAP_X: f32 = 56.0;
/// A step's heading, above its quests.
pub const HEADING: f32 = 72.0;
/// A quest's node, and the space between nodes in a column.
pub const NODE: f32 = 56.0;
pub const GAP_Y: f32 = 14.0;
/// A stage's heading, each of its choices beneath it, and the space below them.
pub const STAGE_HEADING: f32 = 23.0;
pub const CHOICE: f32 = 18.0;
pub const STAGE_FOOT: f32 = 12.0;

/// A cell of the alignment map: its side, with a point between cells.
pub const MAP_CELL: f32 = 16.0;

/// The whole alignment map, 21 cells each way.
pub fn map_size() -> Vec2 {
    Vec2::splat(MAP_CELLS as f32 * MAP_CELL)
}

/// Where a cell of the alignment map goes, drawn from `origin`.
pub fn map_cell(origin: Pos2, cell: MapCell) -> Rect {
    let corner = origin + Vec2::new(cell.column as f32, cell.row as f32) * MAP_CELL;
    Rect::from_min_size(corner, Vec2::splat(MAP_CELL - 1.0))
}

/// A curve's plot: its points, in `(x, y)` order, placed in `area` with the lowest x at the
/// left, the highest at the right, and y rising up the area. A curve of one value runs flat
/// across the middle.
pub fn curve_plot(points: &[(f32, f32)], area: Rect) -> Vec<Pos2> {
    let range = |values: Vec<f32>| {
        let low = values.iter().copied().fold(f32::INFINITY, f32::min);
        let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        (low, high)
    };
    let (left, right) = range(points.iter().map(|(x, _)| *x).collect());
    let (bottom, top) = range(points.iter().map(|(_, y)| *y).collect());
    let across = |value: f32, low: f32, high: f32| match high > low {
        true => (value - low) / (high - low),
        false => 0.5,
    };
    let placed: Vec<Pos2> = points
        .iter()
        .map(|(x, y)| {
            Pos2::new(
                area.left() + across(*x, left, right) * area.width(),
                area.bottom() - across(*y, bottom, top) * area.height(),
            )
        })
        .collect();
    match placed.as_slice() {
        [only] => vec![
            Pos2::new(area.left(), only.y),
            Pos2::new(area.right(), only.y),
        ],
        _ => placed,
    }
}

/// A route a line takes: its points from start to tip, each turn a right angle.
pub type Route = Vec<Pos2>;

/// A questline laid out: its steps' headings, its quests and the quests outside it, and a
/// route for each of its edges between two of them.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    pub size: Vec2,
    /// One per step, in order.
    pub headings: Vec<Rect>,
    /// Each quest drawn, with where, and whether it's outside the questline.
    pub nodes: Vec<(String, Rect, bool)>,
    /// Above the quests outside, if there are any, at the head of their column.
    pub outside: Option<Rect>,
    /// One per edge of the view, in its order; `None` for an edge between a quest and itself.
    pub routes: Vec<Option<Route>>,
}

/// A quest laid out: its stages in order, the end after them, and a route from each choice
/// to where it leads.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestLayout {
    pub size: Vec2,
    /// One per stage, in order.
    pub stages: Vec<Rect>,
    pub end: Rect,
    /// One per choice, stage by stage; `None` for a `next` that names no stage of the quest.
    pub routes: Vec<Option<Route>>,
}

/// The left edge of the `column`th column.
fn left(column: usize) -> f32 {
    column as f32 * (COLUMN + GAP_X)
}

/// A questline's steps in columns, each with its quests beneath its heading, and the quests
/// outside it in a column of their own after the last step.
pub fn line_layout(view: &LineView<'_>, quests: &[&Quest], origin: Pos2) -> LineLayout {
    let steps = &view.line.steps;
    let heading = |column: usize| {
        Rect::from_min_size(
            Pos2::new(left(column), 0.0),
            Vec2::new(COLUMN, HEADING - GAP_Y),
        )
    };
    let node = |column: usize, row: usize| {
        let top = HEADING + row as f32 * (NODE + GAP_Y);
        Rect::from_min_size(Pos2::new(left(column), top), Vec2::new(COLUMN, NODE))
    };
    let headings: Vec<Rect> = (0..steps.len()).map(heading).collect();
    let mut nodes = Vec::new();
    let mut columns = Vec::new();
    for (column, step) in steps.iter().enumerate() {
        for (row, quest) in step.quests.iter().enumerate() {
            if quests.iter().any(|known| &known.id == quest) {
                nodes.push((quest.to_string(), node(column, row), false));
                columns.push(column);
            }
        }
    }
    let outside = (!view.outside.is_empty()).then(|| heading(steps.len()));
    for (row, quest) in view.outside.iter().enumerate() {
        nodes.push((quest.id.to_string(), node(steps.len(), row), true));
        columns.push(steps.len());
    }
    let place = |quest: &str| nodes.iter().position(|(id, ..)| id == quest);
    let wires: Vec<Option<Wire>> = view
        .edges
        .iter()
        .map(|edge| {
            Some(Wire {
                from: place(edge.from().as_str())?,
                to: place(edge.to().as_str())?,
                leaves_at: None,
            })
        })
        .collect();
    let boxes: Vec<(usize, Rect)> = columns
        .iter()
        .zip(&nodes)
        .map(|(column, (_, rect, _))| (*column, *rect))
        .collect();
    let (routes, below) = route(&boxes, &wires);
    let count = steps.len() + usize::from(outside.is_some());
    let by = origin.to_vec2();
    LineLayout {
        size: Vec2::new(left(count), below),
        headings: headings
            .into_iter()
            .map(|r: Rect| r.translate(by))
            .collect(),
        nodes: nodes
            .into_iter()
            .map(|(id, rect, outside)| (id, rect.translate(by), outside))
            .collect(),
        outside: outside.map(|rect| rect.translate(by)),
        routes: routes.into_iter().map(|route| moved(route, by)).collect(),
    }
}

/// A route, moved `by`.
fn moved(route: Option<Route>, by: Vec2) -> Option<Route> {
    route.map(|points| points.into_iter().map(|point| point + by).collect())
}

/// A quest's stages in order, each as tall as its choices, then the end, as tall as the
/// tallest.
pub fn quest_layout(quest: &Quest, origin: Pos2) -> QuestLayout {
    let tall = |choices: usize| STAGE_HEADING + choices as f32 * CHOICE + STAGE_FOOT;
    let node = |column: usize, choices: usize| {
        Rect::from_min_size(
            Pos2::new(left(column), 0.0),
            Vec2::new(COLUMN, tall(choices)),
        )
    };
    let stages: Vec<Rect> = quest
        .stages
        .iter()
        .enumerate()
        .map(|(column, stage)| node(column, stage.choices.len()))
        .collect();
    // The end is as tall as the tallest stage, so the lines into it have room.
    let tallest = quest
        .stages
        .iter()
        .map(|stage| stage.choices.len())
        .max()
        .unwrap_or(0);
    let end = node(quest.stages.len(), tallest);
    let boxes: Vec<(usize, Rect)> = stages
        .iter()
        .chain([&end])
        .enumerate()
        .map(|(column, rect)| (column, *rect))
        .collect();
    let mut wires = Vec::new();
    for (column, (stage, rect)) in quest.stages.iter().zip(&stages).enumerate() {
        for (place, choice) in stage.choices.iter().enumerate() {
            let to = match &choice.next {
                Next::End => Some(quest.stages.len()),
                Next::Stage(next) => quest.stages.iter().position(|s| &s.id == next),
            };
            wires.push(to.map(|to| Wire {
                from: column,
                to,
                leaves_at: Some(rect.min.y + STAGE_HEADING + (place as f32 + 0.5) * CHOICE),
            }));
        }
    }
    let (routes, below) = route(&boxes, &wires);
    let by = origin.to_vec2();
    QuestLayout {
        size: Vec2::new(left(quest.stages.len() + 1), below),
        stages: stages.into_iter().map(|rect| rect.translate(by)).collect(),
        end: end.translate(by),
        routes: routes.into_iter().map(|route| moved(route, by)).collect(),
    }
}

/// A line to route between two boxes, by their places; from a given height on its first
/// box's side, such as a choice's row, or else from the next free point there.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Wire {
    from: usize,
    to: usize,
    leaves_at: Option<f32>,
}

/// A side of a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
}

/// How one line runs: the sides it leaves and enters by, the gaps it turns in (the `n`th gap
/// is the one after the `n`th column), and whether it needs the corridor.
#[derive(Debug, Clone, Copy)]
struct Plan {
    leaves: Side,
    enters: Side,
    near: usize,
    far: usize,
    long: bool,
}

/// One end of a line on a box's side: the height of its other end, the wire, and whether
/// the line leaves there.
type End = (f32, usize, bool);

/// The space between lines in the corridor below the boxes.
pub const TRACK: f32 = 10.0;

/// A route for each wire between `boxes`, each a column and where it is, so that no line
/// passes through a box (P-90), and how far down the graph goes, its corridor included.
fn route(boxes: &[(usize, Rect)], wires: &[Option<Wire>]) -> (Vec<Option<Route>>, f32) {
    let plans: Vec<Option<Plan>> = wires
        .iter()
        .map(|wire| {
            let wire = wire.filter(|wire| wire.from != wire.to)?;
            let (from, to) = (boxes[wire.from].0, boxes[wire.to].0);
            Some(match to.cmp(&from) {
                Ordering::Greater => Plan {
                    leaves: Side::Right,
                    enters: Side::Left,
                    near: from,
                    far: to - 1,
                    long: to > from + 1,
                },
                Ordering::Less => Plan {
                    leaves: Side::Left,
                    enters: Side::Right,
                    near: from - 1,
                    far: to,
                    long: to + 1 < from,
                },
                Ordering::Equal => Plan {
                    leaves: Side::Right,
                    enters: Side::Right,
                    near: from,
                    far: from,
                    long: false,
                },
            })
        })
        .collect();
    let centre = |place: usize| boxes[place].1.center().y;
    // Each end on a box's side, in the order of the other end's height, then of the wires.
    let mut ends: BTreeMap<(usize, Side), Vec<End>> = BTreeMap::new();
    for (index, (wire, plan)) in wires.iter().zip(&plans).enumerate() {
        let (Some(wire), Some(plan)) = (wire, plan) else {
            continue;
        };
        if wire.leaves_at.is_none() {
            let other = centre(wire.to);
            ends.entry((wire.from, plan.leaves))
                .or_default()
                .push((other, index, true));
        }
        let other = wire.leaves_at.unwrap_or_else(|| centre(wire.from));
        ends.entry((wire.to, plan.enters))
            .or_default()
            .push((other, index, false));
    }
    let mut heights: BTreeMap<(usize, bool), f32> = BTreeMap::new();
    for ((place, _), mut on_side) in ends {
        on_side.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let rect = boxes[place].1;
        let count = on_side.len();
        for (at, (_, index, leaving)) in on_side.into_iter().enumerate() {
            let height = rect.top() + rect.height() * (at + 1) as f32 / (count + 1) as f32;
            heights.insert((index, leaving), height);
        }
    }
    // Each line's lane in the gaps it turns in, in the order of the wires.
    let mut lanes: BTreeMap<usize, Vec<(usize, bool)>> = BTreeMap::new();
    for (index, plan) in plans.iter().enumerate() {
        let Some(plan) = plan else {
            continue;
        };
        lanes.entry(plan.near).or_default().push((index, true));
        if plan.long {
            lanes.entry(plan.far).or_default().push((index, false));
        }
    }
    let mut across: BTreeMap<(usize, bool), f32> = BTreeMap::new();
    for (gap, in_gap) in lanes {
        let count = in_gap.len();
        for (at, key) in in_gap.into_iter().enumerate() {
            let x = left(gap) + COLUMN + GAP_X * (at + 1) as f32 / (count + 1) as f32;
            across.insert(key, x);
        }
    }
    let lowest = boxes
        .iter()
        .map(|(_, rect)| rect.bottom())
        .fold(0.0, f32::max);
    let mut tracks = 0;
    let routes = wires
        .iter()
        .zip(&plans)
        .enumerate()
        .map(|(index, (wire, plan))| {
            let (wire, plan) = ((*wire)?, (*plan)?);
            let side_x = |place: usize, side: Side| match side {
                Side::Left => boxes[place].1.left(),
                Side::Right => boxes[place].1.right(),
            };
            let start_y = wire
                .leaves_at
                .or_else(|| heights.get(&(index, true)).copied())?;
            let end_y = *heights.get(&(index, false))?;
            let near = *across.get(&(index, true))?;
            let start = Pos2::new(side_x(wire.from, plan.leaves), start_y);
            let tip = Pos2::new(side_x(wire.to, plan.enters), end_y);
            let mut points = vec![start, Pos2::new(near, start_y)];
            if plan.long {
                let corridor = lowest + GAP_Y + tracks as f32 * TRACK;
                tracks += 1;
                let far = *across.get(&(index, false))?;
                points.extend([
                    Pos2::new(near, corridor),
                    Pos2::new(far, corridor),
                    Pos2::new(far, end_y),
                ]);
            } else {
                points.push(Pos2::new(near, end_y));
            }
            points.push(tip);
            Some(straightened(points))
        })
        .collect();
    let below = match tracks {
        0 => lowest,
        tracks => lowest + GAP_Y + (tracks - 1) as f32 * TRACK + GAP_Y,
    };
    (routes, below)
}

/// `points` without those that don't turn: any point in line with the ones either side of it.
fn straightened(points: Vec<Pos2>) -> Route {
    let mut kept: Route = Vec::new();
    for point in points {
        if kept.last() == Some(&point) {
            continue;
        }
        if let [.., before, last] = kept.as_slice()
            && ((before.x == last.x && last.x == point.x)
                || (before.y == last.y && last.y == point.y))
        {
            kept.pop();
        }
        kept.push(point);
    }
    kept
}

/// `route` with each turn rounded, to `radius` or as much as its neighbouring legs allow:
/// a turn becomes a few points along a quarter circle, so the line reads as one stroke.
pub fn rounded(route: &[Pos2], radius: f32) -> Route {
    let mut points = Vec::new();
    for (at, point) in route.iter().enumerate() {
        let (Some(before), Some(after)) = (at.checked_sub(1).map(|b| route[b]), route.get(at + 1))
        else {
            points.push(*point);
            continue;
        };
        let into = *point - before;
        let out = *after - *point;
        let r = radius.min(into.length() / 2.0).min(out.length() / 2.0);
        let from = *point - into.normalized() * r;
        let to = *point + out.normalized() * r;
        // A quadratic curve from `from` to `to`, bending at the corner.
        for step in 0..=4 {
            let t = step as f32 / 4.0;
            let a = from.lerp(*point, t);
            let b = point.lerp(to, t);
            points.push(a.lerp(b, t));
        }
    }
    points
}

/// One part of a group of an entry's form (U6f): a value on its row, or a list whose items
/// are tables, drawn as a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// The field at this place in the form.
    Row(usize),
    Table(ListTable),
}

/// A list of tables as a table: a row an item, a column a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTable {
    /// The list's key in the entry, such as `ranks`.
    pub list: String,
    /// Each key any item has, such as `requires.standing`, in the order first met.
    pub columns: Vec<String>,
    /// Each item's field in each column, if it has one.
    pub rows: Vec<Vec<Option<usize>>>,
}

/// Where each group of a form begins and ends, from each field's group in order: a run of
/// fields in the same group.
pub fn groups(names: &[Option<&str>]) -> Vec<Range<usize>> {
    let mut groups: Vec<Range<usize>> = Vec::new();
    for (place, name) in names.iter().enumerate() {
        match groups.last_mut() {
            Some(group) if names[group.start] == *name => group.end = place + 1,
            _ => groups.push(place..place + 1),
        }
    }
    groups
}

/// The blocks of a group, from each field's place in the form and its key within the entry,
/// such as `ranks[1].requires.standing`: a list whose every key goes on to a key of an item,
/// with no list within it, is a table where its first field is; every other field is a row.
pub fn blocks(keys: &[(usize, ValuePath)]) -> Vec<Block> {
    // An item's key: the list, the item, and the key within it.
    let in_item = |path: &ValuePath| match &path.0[..] {
        [Step::Key(list), Step::Index(item), within @ ..] => {
            Some((list.clone(), *item, ValuePath(within.to_vec())))
        }
        _ => None,
    };
    let mut flat: BTreeMap<String, bool> = BTreeMap::new();
    for (_, path) in keys {
        if let [Step::Key(list), Step::Index(_), within @ ..] = &path.0[..] {
            let keyed =
                !within.is_empty() && within.iter().all(|step| matches!(step, Step::Key(_)));
            *flat.entry(list.clone()).or_insert(true) &= keyed;
        }
    }
    let mut blocks = Vec::new();
    let mut placed: BTreeMap<String, usize> = BTreeMap::new();
    for (field, path) in keys {
        let Some((list, item, within)) =
            in_item(path).filter(|(list, ..)| flat.get(list) == Some(&true))
        else {
            blocks.push(Block::Row(*field));
            continue;
        };
        let at = *placed.entry(list.clone()).or_insert_with(|| {
            blocks.push(Block::Table(ListTable {
                list,
                columns: Vec::new(),
                rows: Vec::new(),
            }));
            blocks.len() - 1
        });
        let Some(Block::Table(table)) = blocks.get_mut(at) else {
            continue;
        };
        let column = within.to_string();
        let column = match table.columns.iter().position(|known| *known == column) {
            Some(known) => known,
            None => {
                table.columns.push(column);
                for row in &mut table.rows {
                    row.push(None);
                }
                table.columns.len() - 1
            }
        };
        while table.rows.len() <= item {
            table.rows.push(vec![None; table.columns.len()]);
        }
        table.rows[item][column] = Some(*field);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use factional_content::{QuestGraph, outline_texts, read_texts};

    use super::*;

    fn graph(dir: &str) -> QuestGraph {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(dir);
        let texts = read_texts(&dir).expect("there");
        QuestGraph::new(&texts, &outline_texts(&texts))
    }

    fn keys(keys: &[&str]) -> Vec<(usize, ValuePath)> {
        keys.iter()
            .enumerate()
            .map(|(field, key)| (field + 10, ValuePath::parse(key).expect("a key")))
            .collect()
    }

    #[test]
    fn a_list_of_tables_is_a_table_with_a_row_an_item_and_a_column_a_key() {
        let ranks = keys(&[
            "ranks[0].id",
            "ranks[1].id",
            "ranks[1].requires.standing",
            "ranks[2].id",
            "ranks[2].requires.standing",
            "ranks[2].tolerance",
        ]);
        assert_eq!(
            blocks(&ranks),
            [Block::Table(ListTable {
                list: "ranks".to_owned(),
                columns: vec![
                    "id".to_owned(),
                    "requires.standing".to_owned(),
                    "tolerance".to_owned(),
                ],
                rows: vec![
                    vec![Some(10), None, None],
                    vec![Some(11), Some(12), None],
                    vec![Some(13), Some(14), Some(15)],
                ],
            })]
        );
    }

    #[test]
    fn a_table_sits_where_its_first_field_is_and_takes_its_later_fields() {
        // A membership's default comes after what's written, but it's still in its row.
        let hale = keys(&[
            "memberships[0].faction",
            "memberships[0].rank",
            "standing.factions.city_watch",
            "memberships[0].secret",
        ]);
        assert_eq!(
            blocks(&hale),
            [
                Block::Table(ListTable {
                    list: "memberships".to_owned(),
                    columns: vec!["faction".to_owned(), "rank".to_owned(), "secret".to_owned()],
                    rows: vec![vec![Some(10), Some(11), Some(13)]],
                }),
                Block::Row(12),
            ]
        );
    }

    #[test]
    fn a_group_is_a_run_of_fields_in_it() {
        let names = [Some("A"), Some("A"), Some("B"), None, None, Some("A")];
        assert_eq!(groups(&names), [0..2, 2..3, 3..5, 5..6]);
        assert_eq!(groups(&[]), []);
    }

    #[test]
    fn plain_values_and_lists_with_lists_in_them_stay_rows() {
        let mira = keys(&["name", "contacts[0]", "contacts[1]"]);
        assert_eq!(
            blocks(&mira),
            [Block::Row(10), Block::Row(11), Block::Row(12)]
        );
        let quest = keys(&["stages[0].id", "stages[0].choices[0].id", "stages[1].id"]);
        assert_eq!(
            blocks(&quest),
            [Block::Row(10), Block::Row(11), Block::Row(12)]
        );
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    fn pos(x: f32, y: f32) -> Pos2 {
        Pos2::new(x, y)
    }

    #[test]
    fn a_questlines_steps_are_columns_with_the_quests_outside_in_one_after_them() {
        let graph = graph("content/sample");
        let view = graph.line("watch_career").expect("the sample's questline");
        let quests: Vec<&Quest> = graph.quests.quests.values().collect();
        let layout = line_layout(&view, &quests, Pos2::ZERO);
        // Four steps and the column outside, of 210 with 56 between (and after); the deepest
        // step has three quests.
        assert_eq!(
            layout.size,
            Vec2::new(5.0 * 266.0, 72.0 + 2.0 * 70.0 + 56.0)
        );
        assert_eq!(layout.headings.len(), 4);
        assert_eq!(layout.headings[2], rect(532.0, 0.0, 210.0, 58.0));
        let at = |id: &str| {
            layout
                .nodes
                .iter()
                .find(|(node, ..)| node == id)
                .map(|(_, rect, outside)| (*rect, *outside))
        };
        assert_eq!(
            at("watch_oath"),
            Some((rect(0.0, 72.0, 210.0, 56.0), false))
        );
        assert_eq!(
            at("smugglers_cove"),
            Some((rect(266.0, 212.0, 210.0, 56.0), false))
        );
        assert_eq!(
            at("watch_captain"),
            Some((rect(798.0, 72.0, 210.0, 56.0), false))
        );
        assert_eq!(
            at("circle_rite"),
            Some((rect(1064.0, 72.0, 210.0, 56.0), true))
        );
        assert_eq!(layout.nodes.len(), 8);
        assert_eq!(layout.outside, Some(rect(1064.0, 0.0, 210.0, 58.0)));
        // circle_rite → watch_captain, twice: straight across the gap between them, each at
        // its own height; watch_oath → smugglers_cove: out of its side, down the first gap,
        // into the cove's side.
        let first = 72.0 + 56.0 * 1.0 / 3.0;
        let second = 72.0 + 56.0 * 2.0 / 3.0;
        assert_eq!(
            layout.routes,
            [
                Some(vec![pos(1064.0, first), pos(1008.0, first)]),
                Some(vec![pos(1064.0, second), pos(1008.0, second)]),
                Some(vec![
                    pos(210.0, 100.0),
                    pos(238.0, 100.0),
                    pos(238.0, 240.0),
                    pos(266.0, 240.0),
                ]),
            ]
        );
    }

    #[test]
    fn a_questline_with_nothing_outside_ends_below_its_deepest_step() {
        let graph = graph("crates/cli/tests/fixtures/worlds/dead_ends");
        let view = graph.line("jobs").expect("the fixture's questline");
        let quests: Vec<&Quest> = graph.quests.quests.values().collect();
        let layout = line_layout(&view, &quests, Pos2::ZERO);
        assert_eq!(layout.outside, None);
        assert_eq!(
            layout.size,
            Vec2::new(2.0 * 266.0, 72.0 + 2.0 * 70.0 - 14.0)
        );
        // patrol needs promotion: the line runs from promotion, in step 2, back to patrol.
        assert_eq!(
            layout.routes,
            [Some(vec![pos(266.0, 100.0), pos(210.0, 100.0)])]
        );
        let unknown: Vec<&Quest> = Vec::new();
        assert!(line_layout(&view, &unknown, Pos2::ZERO).nodes.is_empty());
        // Only the quests given are drawn, and a line to one that isn't has no route.
        let some: Vec<&Quest> = quests
            .iter()
            .copied()
            .filter(|quest| quest.id.as_str() != "inspect")
            .collect();
        let some = line_layout(&view, &some, Pos2::ZERO);
        let drawn: Vec<&str> = some.nodes.iter().map(|(id, ..)| id.as_str()).collect();
        assert_eq!(drawn, ["patrol", "promotion"]);
    }

    #[test]
    fn a_line_that_skips_a_stage_runs_along_the_corridor_below() {
        let graph = graph("crates/cli/tests/fixtures/worlds/dead_ends");
        let siege = graph
            .quests
            .quests
            .values()
            .find(|quest| quest.id.as_str() == "siege")
            .expect("the siege");
        let layout = quest_layout(siege, Pos2::ZERO);
        assert_eq!(
            layout.stages,
            [rect(0.0, 0.0, 210.0, 71.0), rect(266.0, 0.0, 210.0, 53.0)]
        );
        assert_eq!(layout.end, rect(532.0, 0.0, 210.0, 71.0));
        // The corridor's one track is 14 below the tallest stage, with 14 below it.
        assert_eq!(layout.size, Vec2::new(3.0 * 266.0, 71.0 + 14.0 + 14.0));
        // wait → assault and storm → the end cross one gap each; leave → the end, from its
        // row at 50, drops through the first gap, runs along the corridor at 85, and rises
        // through the second into the end, below storm's line, whose row is higher.
        let first_gap = |at: f32| 210.0 + 56.0 * at / 3.0;
        let second_gap = |at: f32| 476.0 + 56.0 * at / 3.0;
        let end = |at: f32| 71.0 * at / 3.0;
        assert_eq!(
            layout.routes,
            [
                Some(vec![
                    pos(210.0, 32.0),
                    pos(first_gap(1.0), 32.0),
                    pos(first_gap(1.0), 26.5),
                    pos(266.0, 26.5),
                ]),
                Some(vec![
                    pos(210.0, 50.0),
                    pos(first_gap(2.0), 50.0),
                    pos(first_gap(2.0), 85.0),
                    pos(second_gap(1.0), 85.0),
                    pos(second_gap(1.0), end(2.0)),
                    pos(532.0, end(2.0)),
                ]),
                Some(vec![
                    pos(476.0, 32.0),
                    pos(second_gap(2.0), 32.0),
                    pos(second_gap(2.0), end(1.0)),
                    pos(532.0, end(1.0)),
                ]),
            ]
        );
    }

    #[test]
    fn everything_is_placed_from_the_origin() {
        let graph = graph("content/sample");
        let view = graph.line("watch_career").expect("the sample's questline");
        let quests: Vec<&Quest> = graph.quests.quests.values().collect();
        let origin = Pos2::new(10.0, 20.0);
        let layout = line_layout(&view, &quests, origin);
        assert_eq!(layout.size, line_layout(&view, &quests, Pos2::ZERO).size);
        assert_eq!(layout.headings[0], rect(10.0, 20.0, 210.0, 58.0));
        assert_eq!(layout.nodes[0].1, rect(10.0, 92.0, 210.0, 56.0));
        assert_eq!(layout.outside, Some(rect(1074.0, 20.0, 210.0, 58.0)));
        assert_eq!(
            layout.routes[2],
            Some(vec![
                pos(220.0, 120.0),
                pos(248.0, 120.0),
                pos(248.0, 260.0),
                pos(276.0, 260.0),
            ])
        );
        let rite = graph
            .quests
            .quests
            .values()
            .find(|quest| quest.id.as_str() == "circle_rite")
            .expect("the rite");
        let layout = quest_layout(rite, origin);
        assert_eq!(layout.stages[0], rect(10.0, 20.0, 210.0, 71.0));
        assert_eq!(layout.end, rect(276.0, 20.0, 210.0, 71.0));
        let lane = 10.0 + (210.0 + 56.0 * 1.0 / 3.0);
        let enters = 20.0 + 71.0 * 1.0 / 3.0;
        assert_eq!(
            layout.routes[0],
            Some(vec![
                pos(220.0, 52.0),
                pos(lane, 52.0),
                pos(lane, enters),
                pos(276.0, enters),
            ])
        );
    }

    #[test]
    fn lines_further_than_a_column_each_way_take_their_own_tracks_in_the_corridor() {
        let mut texts = factional_content::ContentTexts::default();
        let quest = |id: &str, needs: &str| {
            format!(
                "[{id}]\nname = \"{id}\"\nrequires = {{ done = [\"{needs}\"] }}\n\n\
                 [[{id}.stages]]\nid = \"go\"\nchoices = [{{ id = \"on\", next = \"end\" }}]\n\n"
            )
        };
        texts[6] = Some(
            quest("early", "late")
                + &quest("late", "early")
                + "[mid]\nname = \"Mid\"\n\n[[mid.stages]]\nid = \"go\"\nchoices = [{ id = \"on\", next = \"end\" }]\n",
        );
        texts[7] = Some(
            "[line]\nname = \"Line\"\n\n[[line.steps]]\nquests = [\"early\"]\n\n\
             [[line.steps]]\nquests = [\"mid\"]\n\n[[line.steps]]\nquests = [\"late\"]\n"
                .to_owned(),
        );
        let graph = QuestGraph::new(&texts, &outline_texts(&texts));
        let view = graph.line("line").expect("the questline");
        let quests: Vec<&Quest> = graph.quests.quests.values().collect();
        let layout = line_layout(&view, &quests, Pos2::ZERO);
        let routes: Vec<&Route> = layout.routes.iter().flatten().collect();
        assert_eq!(routes.len(), 2);
        // Below the boxes, which end at 128, the corridor has a track for each, 10 apart.
        let mut tracks: Vec<f32> = routes.iter().map(|route| route[2].y).collect();
        tracks.sort_by(f32::total_cmp);
        assert_eq!(tracks, [142.0, 152.0]);
        assert_eq!(
            layout.size,
            Vec2::new(3.0 * 266.0, 128.0 + 14.0 + 10.0 + 14.0)
        );
        // early → late leaves early's right side and turns in the first gap; late → early
        // leaves late's left side and turns in the second, into early's right side.
        for route in routes {
            assert_eq!(route.len(), 6);
            let (start, tip) = (route[0], route[5]);
            let turn = route[1].x;
            match start.x {
                210.0 => {
                    assert_eq!(tip.x, 532.0);
                    assert!((210.0..266.0).contains(&turn), "{turn}");
                }
                532.0 => {
                    assert_eq!(tip.x, 210.0);
                    assert!((476.0..532.0).contains(&turn), "{turn}");
                }
                other => panic!("a line from {other}"),
            }
        }
    }

    /// Whether the line from `a` to `b`, level or upright, passes through the inside of
    /// `rect`.
    fn crosses(a: Pos2, b: Pos2, rect: Rect) -> bool {
        let inside = rect.shrink(0.5);
        let (low, high) = (a.min(b), a.max(b));
        low.x < inside.max.x
            && high.x > inside.min.x
            && low.y < inside.max.y
            && high.y > inside.min.y
            || inside.contains(a)
            || inside.contains(b)
    }

    #[test]
    fn no_line_passes_through_a_box_in_any_graph() {
        let mut checked = 0;
        for dir in [
            "content/sample",
            "crates/cli/tests/fixtures/worlds/dead_ends",
            "crates/cli/tests/fixtures/worlds/lockouts",
            "crates/cli/tests/fixtures/worlds/stale_lock",
            "crates/cli/tests/fixtures/worlds/odd_jobs",
            "crates/cli/tests/fixtures/worlds/diamonds",
        ] {
            let graph = graph(dir);
            let quests: Vec<&Quest> = graph.quests.quests.values().collect();
            let mut drawn: Vec<(Vec<Rect>, Vec<Route>)> = Vec::new();
            for line in graph.quests.questlines.keys() {
                let view = graph.line(line.as_str()).expect("a questline");
                let layout = line_layout(&view, &quests, Pos2::ZERO);
                let boxes = layout.nodes.iter().map(|(_, rect, _)| *rect).collect();
                drawn.push((boxes, layout.routes.into_iter().flatten().collect()));
            }
            for quest in &quests {
                let layout = quest_layout(quest, Pos2::ZERO);
                let mut boxes = layout.stages.clone();
                boxes.push(layout.end);
                drawn.push((boxes, layout.routes.into_iter().flatten().collect()));
            }
            for (boxes, routes) in drawn {
                for route in routes {
                    for leg in route.windows(2) {
                        let (a, b) = (leg[0], leg[1]);
                        assert!(
                            a.x == b.x || a.y == b.y,
                            "{dir}: a slanting leg {a:?} {b:?}"
                        );
                        for rect in &boxes {
                            assert!(
                                !crosses(a, b, *rect),
                                "{dir}: {a:?} → {b:?} crosses {rect:?}"
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 20, "{checked}");
    }

    #[test]
    fn a_turn_is_rounded_without_moving_the_ends() {
        let route = [pos(0.0, 0.0), pos(40.0, 0.0), pos(40.0, 40.0)];
        let round = rounded(&route, 8.0);
        assert_eq!(round.first(), Some(&pos(0.0, 0.0)));
        assert_eq!(round.last(), Some(&pos(40.0, 40.0)));
        // The corner is cut 8 before it and 8 after, through 5 points of a quadratic curve.
        assert_eq!(round[1], pos(32.0, 0.0));
        assert_eq!(round[5], pos(40.0, 8.0));
        assert_eq!(round[3], pos(38.0, 2.0));
        assert_eq!(round.len(), 7);
        // A short leg rounds its turn less.
        let short = rounded(&[pos(0.0, 0.0), pos(4.0, 0.0), pos(4.0, 40.0)], 8.0);
        assert_eq!(short[1], pos(2.0, 0.0));
        let short = rounded(&[pos(0.0, 0.0), pos(40.0, 0.0), pos(40.0, 4.0)], 8.0);
        assert_eq!(short[5], pos(40.0, 2.0));
        assert_eq!(rounded(&[pos(1.0, 1.0)], 8.0), [pos(1.0, 1.0)]);
    }

    #[test]
    fn a_route_keeps_only_its_turns() {
        let points = vec![
            pos(0.0, 0.0),
            pos(5.0, 0.0),
            pos(5.0, 0.0),
            pos(9.0, 0.0),
            pos(9.0, 3.0),
            pos(9.0, 7.0),
            pos(2.0, 7.0),
        ];
        assert_eq!(
            straightened(points),
            [pos(0.0, 0.0), pos(9.0, 0.0), pos(9.0, 7.0), pos(2.0, 7.0)]
        );
    }

    #[test]
    fn the_map_is_twenty_one_cells_each_way_from_its_origin() {
        assert_eq!(map_size(), Vec2::new(336.0, 336.0));
        let origin = Pos2::new(10.0, 20.0);
        assert_eq!(
            map_cell(origin, MapCell { row: 0, column: 0 }),
            rect(10.0, 20.0, 15.0, 15.0)
        );
        assert_eq!(
            map_cell(origin, MapCell { row: 8, column: 17 }),
            rect(10.0 + 17.0 * 16.0, 20.0 + 8.0 * 16.0, 15.0, 15.0)
        );
    }

    #[test]
    fn a_curve_is_plotted_across_its_area_rising_upward() {
        let area = rect(10.0, 20.0, 280.0, 100.0);
        // The sample's affinity: 50 at 0, 0 at 60, -50 at 200.
        assert_eq!(
            curve_plot(&[(0.0, 50.0), (60.0, 0.0), (200.0, -50.0)], area),
            [
                Pos2::new(10.0, 20.0),
                Pos2::new(10.0 + 280.0 * 60.0 / 200.0, 70.0),
                Pos2::new(290.0, 120.0),
            ]
        );
        // One value runs flat across the middle; a flat curve of two points too.
        assert_eq!(
            curve_plot(&[(0.0, 1.0)], area),
            [Pos2::new(10.0, 70.0), Pos2::new(290.0, 70.0)]
        );
        assert_eq!(
            curve_plot(&[(0.0, 1.0), (100.0, 1.0)], area),
            [Pos2::new(10.0, 70.0), Pos2::new(290.0, 70.0)]
        );
    }
}

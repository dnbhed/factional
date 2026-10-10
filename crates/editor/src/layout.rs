//! Where the Quests tab puts things (U4, P-80), worked out apart from drawing so it's tested
//! like the rest of the model. Positions are in points, from the `origin` the graph is drawn
//! at, so the egui layer only paints and places what it's given. Floats here are egui's
//! layout, never content (DESIGN.md §18).

use factional_content::LineView;
use factional_quests::{Next, Quest};

use crate::egui::{Pos2, Rect, Vec2};

/// A node's width, and the space between columns.
pub const COLUMN: f32 = 210.0;
pub const GAP_X: f32 = 56.0;
/// A step's heading, above its quests.
pub const HEADING: f32 = 72.0;
/// A quest's node, and the space between nodes in a column.
pub const NODE: f32 = 56.0;
pub const GAP_Y: f32 = 14.0;
/// The words "Outside this questline", above the quests outside it.
pub const OUTSIDE_LABEL: f32 = 28.0;
/// A stage's heading, each of its choices beneath it, and the space below them.
pub const STAGE_HEADING: f32 = 23.0;
pub const CHOICE: f32 = 18.0;
pub const STAGE_FOOT: f32 = 12.0;

/// A questline laid out: its steps' headings, its quests and the quests outside it, and an
/// arrow for each of its edges between two of them.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    pub size: Vec2,
    /// One per step, in order.
    pub headings: Vec<Rect>,
    /// Each quest drawn, with where, and whether it's outside the questline.
    pub nodes: Vec<(String, Rect, bool)>,
    /// Above the quests outside, if there are any.
    pub outside: Option<Rect>,
    /// One per edge of the view, in its order, from start to tip; `None` for an edge between
    /// a quest and itself.
    pub arrows: Vec<Option<(Pos2, Pos2)>>,
}

/// A quest laid out: its stages in order, the end after them, and an arrow from each choice
/// to where it leads.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestLayout {
    pub size: Vec2,
    /// One per stage, in order.
    pub stages: Vec<Rect>,
    pub end: Rect,
    /// One per choice, stage by stage, from start to tip; `None` for a `next` that names no
    /// stage of the quest.
    pub arrows: Vec<Option<(Pos2, Pos2)>>,
}

/// The left edge of the `column`th column.
fn left(column: usize) -> f32 {
    column as f32 * (COLUMN + GAP_X)
}

/// A questline's steps in columns, each with its quests beneath its heading, and the quests
/// outside it in a row below the deepest step.
pub fn line_layout(view: &LineView<'_>, quests: &[&Quest], origin: Pos2) -> LineLayout {
    let steps = &view.line.steps;
    let rows = steps
        .iter()
        .map(|step| step.quests.len())
        .max()
        .unwrap_or(0);
    let columns = steps.len().max(view.outside.len());
    let outside_top = HEADING + rows as f32 * (NODE + GAP_Y);
    let node = |column: usize, top: f32| {
        Rect::from_min_size(Pos2::new(left(column), top), Vec2::new(COLUMN, NODE))
    };
    let headings: Vec<Rect> = (0..steps.len())
        .map(|column| {
            Rect::from_min_size(
                Pos2::new(left(column), 0.0),
                Vec2::new(COLUMN, HEADING - GAP_Y),
            )
        })
        .collect();
    let mut nodes = Vec::new();
    for (column, step) in steps.iter().enumerate() {
        for (row, quest) in step.quests.iter().enumerate() {
            if quests.iter().any(|known| &known.id == quest) {
                let top = HEADING + row as f32 * (NODE + GAP_Y);
                nodes.push((quest.to_string(), node(column, top), false));
            }
        }
    }
    let outside = (!view.outside.is_empty()).then(|| {
        Rect::from_min_size(
            Pos2::new(0.0, outside_top),
            Vec2::new(COLUMN * 2.0, OUTSIDE_LABEL - 4.0),
        )
    });
    for (column, quest) in view.outside.iter().enumerate() {
        let top = outside_top + OUTSIDE_LABEL;
        nodes.push((quest.id.to_string(), node(column, top), true));
    }
    let height = match outside {
        Some(_) => outside_top + OUTSIDE_LABEL + NODE,
        None => outside_top - GAP_Y,
    };
    let place = |quest: &str| {
        nodes
            .iter()
            .find(|(id, ..)| id == quest)
            .map(|(_, rect, _)| *rect)
    };
    let arrows: Vec<Option<(Pos2, Pos2)>> = view
        .edges
        .iter()
        .map(|edge| arrow_ends(place(edge.from().as_str())?, place(edge.to().as_str())?))
        .collect();
    let by = origin.to_vec2();
    LineLayout {
        size: Vec2::new(left(columns), height),
        headings: headings
            .into_iter()
            .map(|r: Rect| r.translate(by))
            .collect(),
        nodes: nodes
            .into_iter()
            .map(|(id, rect, outside)| (id, rect.translate(by), outside))
            .collect(),
        outside: outside.map(|rect| rect.translate(by)),
        arrows: arrows.into_iter().map(|ends| moved(ends, by)).collect(),
    }
}

/// An arrow's ends, moved `by`.
fn moved(ends: Option<(Pos2, Pos2)>, by: Vec2) -> Option<(Pos2, Pos2)> {
    ends.map(|(start, tip)| (start + by, tip + by))
}

/// A quest's stages in order, each as tall as its choices, then the end.
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
    let end = node(quest.stages.len(), 0);
    let mut arrows = Vec::new();
    for (stage, rect) in quest.stages.iter().zip(&stages) {
        for (place, choice) in stage.choices.iter().enumerate() {
            let middle = rect.min.y + STAGE_HEADING + (place as f32 + 0.5) * CHOICE;
            let row =
                Rect::from_min_max(Pos2::new(rect.min.x, middle), Pos2::new(rect.max.x, middle));
            let to = match &choice.next {
                Next::End => Some(end),
                Next::Stage(next) => quest
                    .stages
                    .iter()
                    .position(|s| &s.id == next)
                    .map(|at| stages[at]),
            };
            arrows.push(to.and_then(|to| arrow_ends(row, to)));
        }
    }
    let tallest = quest
        .stages
        .iter()
        .map(|s| s.choices.len())
        .max()
        .unwrap_or(0);
    let by = origin.to_vec2();
    QuestLayout {
        size: Vec2::new(left(quest.stages.len() + 1), tall(tallest)),
        stages: stages.into_iter().map(|rect| rect.translate(by)).collect(),
        end: end.translate(by),
        arrows: arrows.into_iter().map(|ends| moved(ends, by)).collect(),
    }
}

/// Where an arrow from `from` to `to` starts and ends: leaving the side of `from` that faces
/// `to`, and arriving at the facing side of `to`; `None` between a box and itself.
pub fn arrow_ends(from: Rect, to: Rect) -> Option<(Pos2, Pos2)> {
    if from == to {
        None
    } else if to.min.x > from.max.x {
        Some((from.right_center(), to.left_center()))
    } else if to.max.x < from.min.x {
        Some((from.left_center(), to.right_center()))
    } else if to.min.y > from.max.y {
        Some((from.center_bottom(), to.center_top()))
    } else {
        Some((from.center_top(), to.center_bottom()))
    }
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

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn a_questlines_steps_are_columns_with_its_quests_beneath_and_outsiders_below() {
        let graph = graph("content/sample");
        let view = graph.line("watch_career").expect("the sample's questline");
        let quests: Vec<&Quest> = graph.quests.quests.values().collect();
        let layout = line_layout(&view, &quests, Pos2::ZERO);
        // Four columns of 210 with 56 between (and after); the deepest step has three quests.
        assert_eq!(
            layout.size,
            Vec2::new(4.0 * 266.0, 72.0 + 3.0 * 70.0 + 28.0 + 56.0)
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
            Some((rect(0.0, 310.0, 210.0, 56.0), true))
        );
        assert_eq!(layout.nodes.len(), 8);
        assert_eq!(layout.outside, Some(rect(0.0, 282.0, 420.0, 24.0)));
        // circle_rite → watch_captain (twice), watch_oath → smugglers_cove.
        assert_eq!(
            layout.arrows,
            [
                Some((Pos2::new(210.0, 338.0), Pos2::new(798.0, 100.0))),
                Some((Pos2::new(210.0, 338.0), Pos2::new(798.0, 100.0))),
                Some((Pos2::new(210.0, 100.0), Pos2::new(266.0, 240.0))),
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
        // patrol needs promotion: the arrow runs from promotion, in step 2, back to patrol.
        assert_eq!(
            layout.arrows,
            [Some((Pos2::new(266.0, 100.0), Pos2::new(210.0, 100.0)))]
        );
        let unknown: Vec<&Quest> = Vec::new();
        assert!(line_layout(&view, &unknown, Pos2::ZERO).nodes.is_empty());
        // Only the quests given are drawn.
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
    fn a_quests_stages_are_as_tall_as_their_choices_with_an_arrow_from_each() {
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
        assert_eq!(layout.end, rect(532.0, 0.0, 210.0, 35.0));
        assert_eq!(layout.size, Vec2::new(3.0 * 266.0, 71.0));
        // wait → assault, leave → the end, storm → the end; each from its own row.
        assert_eq!(
            layout.arrows,
            [
                Some((Pos2::new(210.0, 32.0), Pos2::new(266.0, 26.5))),
                Some((Pos2::new(210.0, 50.0), Pos2::new(532.0, 17.5))),
                Some((Pos2::new(476.0, 32.0), Pos2::new(532.0, 17.5))),
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
        assert_eq!(layout.outside, Some(rect(10.0, 302.0, 420.0, 24.0)));
        assert_eq!(
            layout.arrows[2],
            Some((Pos2::new(220.0, 120.0), Pos2::new(276.0, 260.0)))
        );
        let rite = graph
            .quests
            .quests
            .values()
            .find(|quest| quest.id.as_str() == "circle_rite")
            .expect("the rite");
        let layout = quest_layout(rite, origin);
        assert_eq!(layout.stages[0], rect(10.0, 20.0, 210.0, 71.0));
        assert_eq!(layout.end, rect(276.0, 20.0, 210.0, 35.0));
        assert_eq!(
            layout.arrows[0],
            Some((Pos2::new(220.0, 52.0), Pos2::new(276.0, 37.5)))
        );
    }

    #[test]
    fn an_arrow_leaves_the_side_facing_where_it_goes() {
        let middle = rect(100.0, 100.0, 50.0, 50.0);
        let right = rect(200.0, 100.0, 50.0, 50.0);
        let left = rect(0.0, 100.0, 50.0, 50.0);
        let below = rect(100.0, 200.0, 50.0, 50.0);
        let above = rect(100.0, 0.0, 50.0, 50.0);
        assert_eq!(
            arrow_ends(middle, right),
            Some((Pos2::new(150.0, 125.0), Pos2::new(200.0, 125.0)))
        );
        assert_eq!(
            arrow_ends(middle, left),
            Some((Pos2::new(100.0, 125.0), Pos2::new(50.0, 125.0)))
        );
        assert_eq!(
            arrow_ends(middle, below),
            Some((Pos2::new(125.0, 150.0), Pos2::new(125.0, 200.0)))
        );
        assert_eq!(
            arrow_ends(middle, above),
            Some((Pos2::new(125.0, 100.0), Pos2::new(125.0, 50.0)))
        );
        assert_eq!(arrow_ends(middle, middle), None);
        // A box touching on the left or just below overlaps, so the arrow goes up through it.
        let touching_left = rect(50.0, 100.0, 50.0, 50.0);
        assert_eq!(
            arrow_ends(middle, touching_left),
            Some((Pos2::new(125.0, 100.0), Pos2::new(75.0, 150.0)))
        );
        let touching_below = rect(100.0, 150.0, 50.0, 50.0);
        assert_eq!(
            arrow_ends(middle, touching_below),
            Some((Pos2::new(125.0, 100.0), Pos2::new(125.0, 200.0)))
        );
        // Touching edges count as overlapping, not beside.
        let touching = rect(150.0, 200.0, 50.0, 50.0);
        assert_eq!(
            arrow_ends(middle, touching),
            Some((Pos2::new(125.0, 150.0), Pos2::new(175.0, 200.0)))
        );
    }
}

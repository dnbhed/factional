//! The Quests tab (U4, P-80): questlines and quests as graphs, drawn from
//! `factional-content`'s `QuestGraph`. It draws the graph and turns clicks into model calls;
//! the edges, the wording and where each problem sits are the content crate's.

use factional_content::{
    Edge, Note, QuestGraph, describe_choice, describe_step, needs, quest_heading,
};
use factional_quests::{Next, Quest};

use crate::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use crate::layout::{GAP_Y, line_layout, quest_layout};
use crate::{Editor, Shown};

/// What a locking edge is drawn in: the design's orange for locks.
const LOCKS: Color32 = Color32::from_rgb(0xf0, 0xa3, 0x5e);

impl Editor {
    /// The Quests tab: what can be shown on the left, the chosen quest on the right, and the
    /// graph in the middle.
    pub(crate) fn quests_ui(&mut self, ui: &mut egui::Ui) {
        let mut show = None;
        egui::Panel::left("quest list")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| {
                show = list_ui(ui, &self.graph, self.shown.as_ref())
            });
        if let Some(shown) = show {
            self.show(shown);
        }
        let mut edit = false;
        if let Some(quest) = self.chosen_quest() {
            egui::Panel::right("chosen quest")
                .resizable(true)
                .default_size(340.0)
                .show(ui, |ui| edit = chosen_ui(ui, &self.graph, quest));
        }
        if edit {
            self.edit_in_content();
        }
        let mut choose = None;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| match &self.shown {
                None => {
                    ui.label("Choose a questline or a quest on the left to see it as a graph.");
                }
                Some(Shown::Questline(line)) => {
                    choose = line_ui(ui, &self.graph, line, self.chosen.as_deref());
                }
                Some(Shown::Quest(quest)) => quest_ui(ui, &self.graph, quest),
            });
        });
        if let Some(quest) = choose {
            self.choose(&quest);
        }
    }

    fn chosen_quest(&self) -> Option<&Quest> {
        let chosen = self.chosen.as_deref()?;
        self.graph
            .quests
            .quests
            .values()
            .find(|quest| quest.id.as_str() == chosen)
    }
}

/// Questlines, then quests in no questline, each with how many problems and warnings it has.
fn list_ui(ui: &mut egui::Ui, graph: &QuestGraph, shown: Option<&Shown>) -> Option<Shown> {
    let mut show = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Questlines");
        for line in graph.quests.questlines.values() {
            let id = line.id.as_str();
            let label = format!("{id} — {}{}", line.name, counts(&graph.in_line(id)));
            let on = shown == Some(&Shown::Questline(id.to_owned()));
            if ui.selectable_label(on, label).clicked() {
                show = Some(Shown::Questline(id.to_owned()));
            }
        }
        ui.heading("Quests in no questline");
        for quest in graph.loose() {
            let id = quest.id.as_str();
            let label = format!("{id} — {}{}", quest.name, counts(&graph.in_quest(id)));
            let on = shown == Some(&Shown::Quest(id.to_owned()));
            if ui.selectable_label(on, label).clicked() {
                show = Some(Shown::Quest(id.to_owned()));
            }
        }
    });
    show
}

/// The chosen quest: its heading and gate, each stage with its choices, and what the loader
/// says at each. Whether "Edit in Content" was clicked.
fn chosen_ui(ui: &mut egui::Ui, graph: &QuestGraph, quest: &Quest) -> bool {
    let id = quest.id.as_str();
    let edit = ui.button("Edit in Content").clicked();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let heading = quest_heading(&graph.quests, quest, true) + &needs(&quest.requires);
        ui.add(egui::Label::new(egui::RichText::new(heading).strong()).wrap());
        notes_ui(ui, &graph.at_quest(id), id);
        for (index, stage) in quest.stages.iter().enumerate() {
            ui.separator();
            ui.label(format!(
                "{}. {}{}",
                index + 1,
                stage.id,
                needs(&stage.requires)
            ));
            let node = format!("{id}.stages[{index}]");
            notes_ui(ui, &graph.at_stage(id, index), &node);
            for (place, choice) in stage.choices.iter().enumerate() {
                ui.indent(&node, |ui| {
                    ui.add(egui::Label::new(describe_choice(choice)).wrap());
                    let node = format!("{node}.choices[{place}]");
                    notes_ui(ui, &graph.at_choice(id, index, place), &node);
                });
            }
        }
    });
    edit
}

/// A questline: its steps in columns, each headed by what it needs, with its quests beneath;
/// the quests outside it at the far end of its edges in a row below; the edges drawn, then
/// listed. The quest clicked, if one was.
fn line_ui(
    ui: &mut egui::Ui,
    graph: &QuestGraph,
    line: &str,
    chosen: Option<&str>,
) -> Option<String> {
    let view = graph.line(line)?;
    let quests: Vec<&Quest> = graph.quests.quests.values().collect();
    let size = line_layout(&view, &quests, Pos2::ZERO).size;
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let layout = line_layout(&view, &quests, area.min);
    // The graph is drawn in its own area, so placing its nodes moves nothing after it.
    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let canvas = &mut canvas;

    for (_, rect, outside) in &layout.nodes {
        node_box(canvas, *rect, *outside);
    }
    for (edge, ends) in view.edges.iter().zip(&layout.arrows) {
        if let Some((start, tip)) = ends {
            arrow(canvas, *start, *tip, edge_colour(canvas, edge));
        }
    }
    for (column, (step, heading)) in view.line.steps.iter().zip(&layout.headings).enumerate() {
        let text = format!(
            "step {} — {}{}",
            column + 1,
            describe_step(step),
            needs(&step.requires)
        );
        canvas.scope_builder(egui::UiBuilder::new().max_rect(*heading), |ui| {
            ui.add(egui::Label::new(text).wrap());
            let node = format!("{line}.steps[{column}]");
            notes_ui(ui, &graph.at_step(line, column), &node);
        });
    }
    if let Some(label) = layout.outside {
        canvas.put(label, egui::Label::new("Outside this questline"));
    }
    let mut clicked = None;
    for (id, rect, _) in &layout.nodes {
        let Some(quest) = quests.iter().find(|quest| quest.id.as_str() == id) else {
            continue;
        };
        let notes = graph.in_quest(id);
        let label = format!(
            "{id} — {} — {}{}",
            quest.name,
            stages(quest.stages.len()),
            counts(&notes)
        );
        let button = egui::Button::selectable(chosen == Some(id.as_str()), label).wrap();
        if canvas.put(*rect, button).clicked() {
            clicked = Some(id.clone());
        }
        outline_node(canvas, *rect, &notes);
    }

    ui.add_space(GAP_Y);
    let lines = graph.at_line(line);
    notes_ui(ui, &lines, line);
    edges_ui(ui, view.edges.iter().copied());
    clicked
}

/// A quest: its stages in columns, each listing its choices, an edge from each choice to its
/// next stage or the end, then the edges between it and other quests, listed.
fn quest_ui(ui: &mut egui::Ui, graph: &QuestGraph, quest: &str) {
    let Some(found) = graph
        .quests
        .quests
        .values()
        .find(|q| q.id.as_str() == quest)
    else {
        return;
    };
    let size = quest_layout(found, Pos2::ZERO).size;
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let layout = quest_layout(found, area.min);
    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let canvas = &mut canvas;

    let stroke = canvas.visuals().widgets.noninteractive.fg_stroke.color;
    for (start, tip) in layout.arrows.iter().flatten() {
        arrow(canvas, *start, *tip, stroke);
    }
    for (column, (stage, rect)) in found.stages.iter().zip(&layout.stages).enumerate() {
        let rect = *rect;
        let mut notes = graph.at_stage(quest, column);
        for place in 0..stage.choices.len() {
            notes.extend(graph.at_choice(quest, column, place));
        }
        node_box(canvas, rect, false);
        outline_node(canvas, rect, &notes);
        canvas.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
            let heading = format!("{}{}", stage.id, counts(&notes));
            ui.label(egui::RichText::new(heading).strong());
            for (place, choice) in stage.choices.iter().enumerate() {
                let next = match &choice.next {
                    Next::End => "the end".to_owned(),
                    Next::Stage(next) => next.to_string(),
                };
                let notes = graph.at_choice(quest, column, place);
                ui.label(format!("{}, then {next}{}", choice.id, counts(&notes)));
            }
        });
    }
    node_box(canvas, layout.end, false);
    canvas.put(layout.end, egui::Label::new("the end"));

    ui.add_space(GAP_Y);
    let touching = graph
        .edges
        .iter()
        .filter(|edge| edge.from() == &found.id || edge.to() == &found.id);
    edges_ui(ui, touching);
}

/// `Edges`, then each edge in words, or nothing if there are none.
fn edges_ui<'g>(ui: &mut egui::Ui, edges: impl Iterator<Item = &'g Edge>) {
    let edges: Vec<&Edge> = edges.collect();
    if edges.is_empty() {
        return;
    }
    ui.label(egui::RichText::new("Edges").strong());
    for edge in edges {
        ui.colored_label(edge_colour(ui, edge), edge.to_string());
    }
}

/// Each note in words, coloured as an error or a warning, with where it is within `node`.
fn notes_ui(ui: &mut egui::Ui, notes: &[&Note], node: &str) {
    for note in notes {
        let colour = match note.problem {
            true => ui.visuals().error_fg_color,
            false => ui.visuals().warn_fg_color,
        };
        ui.add(egui::Label::new(egui::RichText::new(note.said_at(node)).color(colour)).wrap());
    }
}

/// A node's box: filled, or for a quest outside the questline, only a dashed outline.
fn node_box(ui: &egui::Ui, rect: Rect, outside: bool) {
    let visuals = ui.visuals();
    if outside {
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ];
        let stroke = Stroke::new(1.0, visuals.weak_text_color());
        ui.painter()
            .extend(Shape::dashed_line(&corners, stroke, 5.0, 4.0));
    } else {
        ui.painter()
            .rect_filled(rect, 6.0, visuals.widgets.inactive.weak_bg_fill);
        ui.painter().rect_stroke(
            rect,
            6.0,
            visuals.widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
}

/// How a node is marked by what the loader says at it: a problem outweighs a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Problem,
    Warning,
}

/// The mark for `notes`, if they have any.
fn mark(notes: &[&Note]) -> Option<Mark> {
    if notes.iter().any(|note| note.problem) {
        Some(Mark::Problem)
    } else if notes.is_empty() {
        None
    } else {
        Some(Mark::Warning)
    }
}

/// A border in the colour of the worst of `notes`, if there are any.
fn outline_node(ui: &egui::Ui, rect: Rect, notes: &[&Note]) {
    let colour = match mark(notes) {
        None => return,
        Some(Mark::Problem) => ui.visuals().error_fg_color,
        Some(Mark::Warning) => ui.visuals().warn_fg_color,
    };
    ui.painter().rect_stroke(
        rect,
        6.0,
        Stroke::new(2.0, colour),
        egui::StrokeKind::Outside,
    );
}

/// What an edge is drawn in: needs in the link colour, locks in orange.
fn edge_colour(ui: &egui::Ui, edge: &Edge) -> Color32 {
    colour_of(ui.visuals(), edge)
}

fn colour_of(visuals: &egui::Visuals, edge: &Edge) -> Color32 {
    match edge {
        Edge::Needs { .. } => visuals.hyperlink_color,
        Edge::Locks { .. } => LOCKS,
    }
}

/// A dashed arrow from `start` to `tip`.
fn arrow(ui: &egui::Ui, start: Pos2, tip: Pos2, colour: Color32) {
    let stroke = Stroke::new(1.5, colour);
    ui.painter()
        .extend(Shape::dashed_line(&[start, tip], stroke, 6.0, 4.0));
    ui.painter().add(Shape::convex_polygon(
        arrowhead(start, tip).to_vec(),
        colour,
        Stroke::NONE,
    ));
}

/// An arrowhead at `tip`, pointing away from `start`: 9 points long and 9 wide.
fn arrowhead(start: Pos2, tip: Pos2) -> [Pos2; 3] {
    let back = (start - tip).normalized() * 9.0;
    let side = Vec2::new(-back.y, back.x) * 0.5;
    [tip, tip + back + side, tip + back - side]
}

/// `1 stage` or `2 stages`.
fn stages(count: usize) -> String {
    match count {
        1 => "1 stage".to_owned(),
        count => format!("{count} stages"),
    }
}

/// ` — 1 problem`, ` — 2 problems, 1 warning`, or nothing.
fn counts(notes: &[&Note]) -> String {
    let problems = notes.iter().filter(|note| note.problem).count();
    let warnings = notes.len() - problems;
    let count = |n: usize, noun: &str| match n {
        0 => None,
        1 => Some(format!("1 {noun}")),
        n => Some(format!("{n} {noun}s")),
    };
    let said: Vec<String> = [count(problems, "problem"), count(warnings, "warning")]
        .into_iter()
        .flatten()
        .collect();
    if said.is_empty() {
        String::new()
    } else {
        format!(" — {}", said.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use factional_content::{Diagnostic, Edge, Note};
    use factional_quests::{ChoiceId, Lock, Progress, QuestId, StageId};

    use super::*;

    fn note(problem: bool) -> Note {
        Note {
            diagnostic: Diagnostic {
                file: "quests.toml".to_owned(),
                key: Some("siege".to_owned()),
                message: "something".to_owned(),
                suggestion: None,
            },
            problem,
        }
    }

    #[test]
    fn a_node_is_marked_by_the_worst_of_what_the_loader_says_at_it() {
        let (problem, warning) = (note(true), note(false));
        assert_eq!(mark(&[]), None);
        assert_eq!(mark(&[&warning]), Some(Mark::Warning));
        assert_eq!(mark(&[&warning, &problem]), Some(Mark::Problem));
        assert_eq!(mark(&[&problem]), Some(Mark::Problem));
    }

    #[test]
    fn needs_are_drawn_in_the_link_colour_and_locks_in_orange() {
        let visuals = egui::Visuals::dark();
        let quest = |id: &str| QuestId::new(id).expect("valid id");
        let needs = Edge::Needs {
            quest: quest("patrol"),
            stage: None,
            done: Progress::parse("promotion").expect("progress"),
        };
        let locks = Edge::Locks {
            quest: quest("riot"),
            stage: StageId::new("street").expect("valid id"),
            choice: ChoiceId::new("incite").expect("valid id"),
            lock: Lock::parse("muster").expect("a lock"),
        };
        assert_eq!(colour_of(&visuals, &needs), visuals.hyperlink_color);
        assert_eq!(colour_of(&visuals, &locks), LOCKS);
        assert_ne!(LOCKS, visuals.hyperlink_color);
    }

    #[test]
    fn an_arrowhead_points_along_its_line() {
        // A line running right ends in a head 9 back from the tip and 9 across.
        assert_eq!(
            arrowhead(Pos2::new(0.0, 0.0), Pos2::new(20.0, 0.0)),
            [
                Pos2::new(20.0, 0.0),
                Pos2::new(11.0, -4.5),
                Pos2::new(11.0, 4.5)
            ]
        );
        // Running down, it turns with the line.
        assert_eq!(
            arrowhead(Pos2::new(5.0, 0.0), Pos2::new(5.0, 30.0)),
            [
                Pos2::new(5.0, 30.0),
                Pos2::new(9.5, 21.0),
                Pos2::new(0.5, 21.0)
            ]
        );
    }
}

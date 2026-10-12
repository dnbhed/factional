//! The Quests tab (U4, P-80): questlines and quests as graphs, drawn from
//! `factional-content`'s `QuestGraph`. It draws the graph and turns clicks into model calls;
//! the edges, the wording and where each problem sits are the content crate's.

use factional_content::{
    Edge, Note, QuestGraph, describe_choice, describe_step, needs, quest_heading,
};
use std::collections::BTreeMap;

use factional_quests::{Leftovers, Next, Quest, Questline};

use crate::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2, WidgetInfo, WidgetType};
use crate::layout::{GAP_Y, line_layout, quest_layout, rounded};
use crate::theme;
use crate::{Editor, Shown};

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
        let mut back = false;
        let mut paged = None;
        // The chosen quest beside the graph, or the questline's own page (U13a).
        let line = match &self.shown {
            Some(Shown::Questline(line)) => self
                .graph
                .quests
                .questlines
                .values()
                .find(|found| found.id.as_str() == line),
            _ => None,
        };
        if let Some(quest) = self.chosen_quest() {
            egui::Panel::right("chosen quest")
                .resizable(true)
                .default_size(340.0)
                .show(ui, |ui| {
                    if let Some(line) = line {
                        let said = format!("Back to {}", line.name);
                        let button = ui.button(format!("‹ {}", line.name));
                        button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
                        back = button.clicked();
                    }
                    edit = chosen_ui(ui, &self.graph, quest);
                });
        } else if let Some(line) = line {
            egui::Panel::right("questline page")
                .resizable(true)
                .default_size(340.0)
                .show(ui, |ui| {
                    let typed = (&mut self.new_quests, &mut self.step_needs);
                    paged = page_ui(ui, &self.graph, line, typed);
                });
        }
        if edit {
            self.edit_in_content();
        }
        if back {
            self.unchoose();
        }
        match paged {
            Some(Paged::Open(quest)) => self.choose(&quest),
            Some(Paged::Add(line, step, quest)) => {
                if let Err(refused) = self.add_to_step(&line, step, &quest) {
                    self.note = Some(format!("couldn't add: {refused}"));
                }
            }
            Some(Paged::New(line, step, quest)) => {
                if let Err(refused) = self.new_quest_in_step(&line, step, &quest) {
                    self.note = Some(format!("couldn't add: {refused}"));
                }
            }
            Some(Paged::MoveQuest(line, from, quest, to)) => {
                if let Err(refused) = self.move_quest(&line, from, &quest, to) {
                    self.note = Some(format!("couldn't move: {refused}"));
                }
            }
            Some(Paged::MoveStep(line, step, down)) => {
                if let Err(refused) = self.move_step(&line, step, down) {
                    self.note = Some(format!("couldn't move: {refused}"));
                }
            }
            Some(Paged::Set(line, step, key, input)) => {
                if let Err(refused) = self.set_step(&line, step, key, &input) {
                    self.note = Some(format!("couldn't set: {refused}"));
                }
            }
            None => {}
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

/// What was done on a questline's page (U13a).
enum Paged {
    /// A quest's own page opened.
    Open(String),
    /// A quest added to the questline's `n`th step.
    Add(String, usize, String),
    /// A new quest made in the questline's `n`th step, by its id (U13b).
    New(String, usize, String),
    /// A quest moved from one step to another (U13c).
    MoveQuest(String, usize, String, usize),
    /// A step moved up, or with `true` down (U13c).
    MoveStep(String, usize, bool),
    /// A step's `need` or `leftovers` set (U13c).
    Set(String, usize, &'static str, String),
}

/// A questline's page (U13a, P-94): its name, then its steps in order, each a card saying what
/// it needs, with its quests by name, each opening its own, and "+ Existing quest…", offering
/// the quests in no questline, as a quest is in one at most; and "+ New quest", its id typed
/// in `new_quests` by step (U13b). Each step moves up and down, each quest to another step,
/// and a step's need, typed in `needs` by step, and leftovers are set on its card (U13c).
fn page_ui(
    ui: &mut egui::Ui,
    graph: &QuestGraph,
    line: &Questline,
    (new_quests, needs_typed): (&mut BTreeMap<usize, String>, &mut BTreeMap<usize, String>),
) -> Option<Paged> {
    let mut paged = None;
    let named = |id: &str| {
        graph
            .quests
            .quests
            .values()
            .find(|quest| quest.id.as_str() == id)
            .filter(|quest| !quest.name.is_empty())
            .map_or_else(|| id.to_owned(), |quest| format!("{} · {id}", quest.name))
    };
    let loose = graph.loose();
    let id = line.id.as_str();
    ui.heading(&line.name);
    egui::ScrollArea::vertical().show(ui, |ui| {
        let last = line.steps.len().saturating_sub(1);
        for (index, step) in line.steps.iter().enumerate() {
            let number = index + 1;
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(format!("Step {number}"));
                    for (down, shown, there) in [(false, "↑", index > 0), (true, "↓", index < last)]
                    {
                        if !there {
                            continue;
                        }
                        let way = if down { "down" } else { "up" };
                        let said = format!("Move step {number} {way}");
                        let button = ui.small_button(shown);
                        button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
                        if button.clicked() {
                            paged = Some(Paged::MoveStep(id.to_owned(), index, down));
                        }
                    }
                });
                ui.add(egui::Label::new(describe_step(step) + &needs(&step.requires)).wrap());
                for quest in &step.quests {
                    let quest = quest.as_str();
                    ui.horizontal(|ui| {
                        if ui.selectable_label(false, named(quest)).clicked() {
                            paged = Some(Paged::Open(quest.to_owned()));
                        }
                        if last == 0 {
                            return;
                        }
                        let menu = ui.menu_button("⋯", |ui| {
                            for to in (0..=last).filter(|to| *to != index) {
                                let said = format!("Move {quest} to step {}", to + 1);
                                let button = ui.button(format!("To step {}", to + 1));
                                button.widget_info(|| {
                                    WidgetInfo::labeled(WidgetType::Button, true, &said)
                                });
                                if button.clicked() {
                                    paged = Some(Paged::MoveQuest(
                                        id.to_owned(),
                                        index,
                                        quest.to_owned(),
                                        to,
                                    ));
                                }
                            }
                        });
                        let said = format!("Move {quest}…");
                        menu.response
                            .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
                    });
                }
                ui.horizontal(|ui| {
                    // Left out, all its quests are needed.
                    let written = step.need.map(|need| need.to_string()).unwrap_or_default();
                    let typed = needs_typed.entry(index).or_insert_with(|| written.clone());
                    let label = ui.label("Need");
                    let field = ui
                        .add(egui::TextEdit::singleline(typed).desired_width(40.0))
                        .labelled_by(label.id);
                    field.widget_info(|| {
                        let mut info = WidgetInfo::text_edit(true, &written, typed.as_str(), "");
                        info.label = Some(format!("Need of step {number}"));
                        info
                    });
                    if field.lost_focus() && *typed != written {
                        paged = Some(Paged::Set(id.to_owned(), index, "need", typed.clone()));
                    }
                    ui.label("Leftovers");
                    for leftovers in Leftovers::ALL {
                        let key = leftovers.key();
                        let on = step.leftovers == leftovers;
                        let said = format!("Leftovers of step {number}: {key}");
                        let button = ui.selectable_label(on, key);
                        button.widget_info(|| {
                            WidgetInfo::selected(WidgetType::SelectableLabel, true, on, &said)
                        });
                        if button.clicked() && !on {
                            paged = Some(Paged::Set(
                                id.to_owned(),
                                index,
                                "leftovers",
                                key.to_owned(),
                            ));
                        }
                    }
                });
                if !loose.is_empty() {
                    let menu = ui.menu_button("+ Existing quest…", |ui| {
                        for quest in &loose {
                            let quest = quest.id.as_str();
                            let said = format!("Add {quest} to step {number}");
                            let button = ui.button(named(quest));
                            button.widget_info(|| {
                                WidgetInfo::labeled(WidgetType::Button, true, &said)
                            });
                            if button.clicked() {
                                paged = Some(Paged::Add(id.to_owned(), index, quest.to_owned()));
                            }
                        }
                    });
                    let said = format!("Add an existing quest to step {number}…");
                    menu.response
                        .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
                }
                ui.horizontal(|ui| {
                    let label = ui.label("+ New quest");
                    let typed = new_quests.entry(index).or_default();
                    ui.add(egui::TextEdit::singleline(typed).desired_width(140.0))
                        .labelled_by(label.id)
                        .widget_info(|| {
                            let mut info = WidgetInfo::text_edit(true, "", typed.as_str(), "");
                            info.label = Some(format!("New quest id in step {number}"));
                            info
                        });
                    let said = format!("Add a new quest to step {number}");
                    let button = ui.button("Add");
                    button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
                    if button.clicked() {
                        paged = Some(Paged::New(id.to_owned(), index, typed.clone()));
                    }
                });
            });
        }
    });
    paged
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

    for (id, rect, outside) in &layout.nodes {
        let marked = mark(&graph.in_quest(id));
        node_box(canvas, *rect, *outside, marked);
    }
    for (edge, route) in view.edges.iter().zip(&layout.routes) {
        if let Some(route) = route {
            arrow(canvas, route, edge_colour(canvas, edge));
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
    for route in layout.routes.iter().flatten() {
        arrow(canvas, route, stroke);
    }
    for (column, (stage, rect)) in found.stages.iter().zip(&layout.stages).enumerate() {
        let rect = *rect;
        let mut notes = graph.at_stage(quest, column);
        for place in 0..stage.choices.len() {
            notes.extend(graph.at_choice(quest, column, place));
        }
        node_box(canvas, rect, false, mark(&notes));
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
    node_box(canvas, layout.end, false, None);
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

/// A node's fill: red behind a problem (board 6), else the widgets' own.
fn fill_of(visuals: &egui::Visuals, marked: Option<Mark>) -> Color32 {
    match marked {
        Some(Mark::Problem) => theme::PROBLEM_FILL,
        _ => visuals.widgets.inactive.weak_bg_fill,
    }
}

/// A node's box: filled as `marked` says, or for a quest outside the questline, only a dashed
/// outline.
fn node_box(ui: &egui::Ui, rect: Rect, outside: bool, marked: Option<Mark>) {
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
            .rect_filled(rect, 6.0, fill_of(visuals, marked));
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
        Edge::Locks { .. } => theme::LOCKS,
    }
}

/// A dashed arrow along `route`, its turns rounded, with its head along its last leg.
fn arrow(ui: &egui::Ui, route: &[Pos2], colour: Color32) {
    let [.., before, tip] = route else {
        return;
    };
    let stroke = Stroke::new(1.5, colour);
    ui.painter()
        .extend(Shape::dashed_line(&rounded(route, 8.0), stroke, 6.0, 4.0));
    ui.painter().add(Shape::convex_polygon(
        arrowhead(*before, *tip).to_vec(),
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
        assert_eq!(colour_of(&visuals, &locks), theme::LOCKS);
        // A node with a problem is filled red; one with a warning, or none, as widgets are.
        assert_eq!(fill_of(&visuals, Some(Mark::Problem)), theme::PROBLEM_FILL);
        let plain = visuals.widgets.inactive.weak_bg_fill;
        assert_eq!(fill_of(&visuals, Some(Mark::Warning)), plain);
        assert_eq!(fill_of(&visuals, None), plain);
        assert_ne!(theme::LOCKS, visuals.hyperlink_color);
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

//! `graph <dir> <questline|quest>` (U4): a questline's steps, quests and edges, or a quest's
//! stages and choices, with every problem and warning at the node it's about, as the
//! editor's Quests tab draws them. It reads the quests as far as they go, so it shows while
//! the world doesn't load.

use std::path::Path;

use factional_content::{
    ContentTexts, Note, QuestGraph, describe_choice, describe_step, needs, outline_texts,
    quest_heading, read_texts,
};
use factional_quests::{Quest, QuestId};

use crate::check::count;
use crate::session::{Outcome, hint, lines};

/// `graph <dir> <questline|quest>`, with `dir` read from `base`.
pub(crate) fn graph(base: &Path, args: &str) -> Outcome {
    let [dir, id] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("graph needs the form: graph <dir> <questline|quest>".to_owned());
    };
    // A directory that can't be read holds no quests: the id is then simply unknown.
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    let graph = QuestGraph::new(&texts, &outline_texts(&texts));
    if let Some(shown) = questline(&graph, id) {
        return Outcome::Output(lines(shown.into_iter()));
    }
    match graph
        .quests
        .quests
        .values()
        .find(|quest| quest.id.as_str() == id)
    {
        Some(quest) => Outcome::Output(lines(show_quest(&graph, quest).into_iter())),
        None => {
            let ids = graph
                .quests
                .questlines
                .keys()
                .map(|line| line.as_str())
                .chain(graph.quests.quests.keys().map(QuestId::as_str))
                .collect();
            Outcome::Error(format!(
                "unknown questline or quest '{id}'{}",
                hint(id, ids)
            ))
        }
    }
}

/// A questline: its heading, each step with its quests, the quests outside it at the far end
/// of its edges, and those edges; `None` if there's no such questline.
fn questline(graph: &QuestGraph, id: &str) -> Option<Vec<String>> {
    let view = graph.line(id)?;
    let line = view.line;
    let giver = line.giver.as_ref().map_or_else(
        || "the world's own".to_owned(),
        |giver| format!("from {giver}"),
    );
    let mut shown = vec![format!(
        "{} — {} — {giver} — {}",
        line.id,
        line.name,
        count(line.steps.len(), "step")
    )];
    shown.extend(notes(graph.at_line(id), line.id.as_str(), "  "));
    for (index, step) in line.steps.iter().enumerate() {
        shown.push(format!(
            "step {} — {}{}",
            index + 1,
            describe_step(step),
            needs(&step.requires)
        ));
        let node = format!("{id}.steps[{index}]");
        shown.extend(notes(graph.at_step(id, index), &node, "  "));
        for quest in &step.quests {
            if let Some(quest) = graph.quests.quests.get(quest) {
                shown.extend(quest_node(graph, quest));
            }
        }
    }
    if !view.outside.is_empty() {
        shown.push("outside".to_owned());
        for quest in &view.outside {
            shown.extend(quest_node(graph, quest));
        }
    }
    shown.extend(edges(view.edges.iter().map(ToString::to_string)));
    Some(shown)
}

/// A quest as a node of a questline: `watch_oath — The Watch's Oath — 2 stages`, then
/// everything the loader says about it.
fn quest_node(graph: &QuestGraph, quest: &Quest) -> Vec<String> {
    let mut shown = vec![format!(
        "  {} — {} — {}",
        quest.id,
        quest.name,
        count(quest.stages.len(), "stage")
    )];
    shown.extend(notes(
        graph.in_quest(quest.id.as_str()),
        quest.id.as_str(),
        "    ",
    ));
    shown
}

/// A quest: its heading, each stage with its choices, and its edges, with what the loader
/// says at each.
fn show_quest(graph: &QuestGraph, quest: &Quest) -> Vec<String> {
    let id = quest.id.as_str();
    let mut shown = vec![quest_heading(&graph.quests, quest, true) + &needs(&quest.requires)];
    shown.extend(notes(graph.at_quest(id), id, "  "));
    for (index, stage) in quest.stages.iter().enumerate() {
        shown.push(format!(
            "{}. {}{}",
            index + 1,
            stage.id,
            needs(&stage.requires)
        ));
        let node = format!("{id}.stages[{index}]");
        shown.extend(notes(graph.at_stage(id, index), &node, "   "));
        for (place, choice) in stage.choices.iter().enumerate() {
            shown.push(format!("   {}", describe_choice(choice)));
            let node = format!("{node}.choices[{place}]");
            shown.extend(notes(graph.at_choice(id, index, place), &node, "     "));
        }
    }
    let touching = graph
        .edges
        .iter()
        .filter(|edge| edge.from() == &quest.id || edge.to() == &quest.id);
    shown.extend(edges(touching.map(ToString::to_string)));
    shown
}

/// `edges`, then each edge, or nothing if there are none.
fn edges(edges: impl Iterator<Item = String>) -> Vec<String> {
    let edges: Vec<String> = edges.map(|edge| format!("  {edge}")).collect();
    if edges.is_empty() {
        return edges;
    }
    std::iter::once("edges".to_owned()).chain(edges).collect()
}

/// Each note, indented, with where it is within `node`.
fn notes(notes: Vec<&Note>, node: &str, indent: &str) -> Vec<String> {
    notes
        .into_iter()
        .map(|note| format!("{indent}{}", note.said_at(node)))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::session::{Outcome, ScriptError, Session};

    fn run(line: &str) -> Result<Outcome, ScriptError> {
        Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).execute(line)
    }

    fn output(text: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(text.into()))
    }

    fn command_error(message: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Error(message.into()))
    }

    const WORLDS: &str = "crates/cli/tests/fixtures/worlds";

    #[test]
    fn a_questline_shows_its_steps_quests_outsiders_and_edges() {
        assert_eq!(
            run("graph content/sample watch_career"),
            output(
                "watch_career — A Life in the Watch — from city_watch — 4 steps\n\
                 step 1 — watch_oath\n\
                 \x20 watch_oath — The Watch's Oath — 2 stages\n\
                 step 2 — 2 of night_patrol, dock_inspection, smugglers_cove, in any order; the rest close\n\
                 \x20 night_patrol — Night Patrol — 1 stage\n\
                 \x20 dock_inspection — Inspecting the Docks — 1 stage\n\
                 \x20 smugglers_cove — The Smugglers' Cove — 2 stages\n\
                 step 3 — any of harbour_errands, lost_dog, or none\n\
                 \x20 harbour_errands — Harbour Errands — 1 stage\n\
                 \x20 lost_dog — The Captain's Dog — 1 stage\n\
                 step 4 — watch_captain — needs standing 40.00 with city_watch, and rank sergeant or higher in city_watch\n\
                 \x20 watch_captain — Captain of the Watch — 1 stage\n\
                 outside\n\
                 \x20 circle_rite — The Ashen Rite — 1 stage\n\
                 edges\n\
                 \x20 circle_rite.whisper.set_them_at_war locks watch_captain\n\
                 \x20 circle_rite.whisper.set_them_at_war locks watch_captain.command\n\
                 \x20 smugglers_cove needs watch_oath.patrol.report done"
            )
        );
    }

    #[test]
    fn a_quest_shows_its_stages_choices_and_edges() {
        assert_eq!(
            run("graph content/sample watch_oath"),
            output(
                "watch_oath — The Watch's Oath — from city_watch, in watch_career at step 1 — needs not to be in lantern_guild\n\
                 1. patrol\n\
                 \x20  report — outcome turned_in_vex — then oath\n\
                 \x20  look_away — outcome took_a_bribe — then the end\n\
                 2. oath — needs standing 10.00 with city_watch\n\
                 \x20  swear — alignment: law 5.00, good 0.00 — then the end\n\
                 edges\n\
                 \x20 smugglers_cove needs watch_oath.patrol.report done"
            )
        );
    }

    #[test]
    fn a_quest_that_can_never_start_says_so_in_its_questline() {
        assert_eq!(
            run(&format!("graph {WORLDS}/dead_ends jobs")),
            output(
                "jobs — Odd Jobs — from watch — 2 steps\n\
                 step 1 — 1 of patrol, inspect, in any order; the rest close\n\
                 \x20 patrol — Patrol — 1 stage\n\
                 \x20   error: it can never start: what it needs only comes after jobs moves on from steps[0], which closes it\n\
                 \x20 inspect — Inspect — 1 stage\n\
                 step 2 — promotion\n\
                 \x20 promotion — Promotion — 1 stage\n\
                 edges\n\
                 \x20 patrol needs promotion done"
            )
        );
    }

    #[test]
    fn a_stage_that_can_never_be_reached_says_so() {
        assert_eq!(
            run(&format!("graph {WORLDS}/dead_ends siege")),
            output(
                "siege — The Siege — the world's own\n\
                 1. camp\n\
                 \x20  wait — then assault\n\
                 \x20  leave — then the end\n\
                 2. assault — needs aftermath done\n\
                 \x20  error: requires.done[0]: aftermath can only happen once siege is over, so this stage can never be reached\n\
                 \x20  storm — then the end\n\
                 edges\n\
                 \x20 aftermath needs siege done\n\
                 \x20 siege.assault needs aftermath done"
            )
        );
    }

    #[test]
    fn an_undeclared_lockout_sits_at_its_choice() {
        assert_eq!(
            run(&format!("graph {WORLDS}/lockouts confession")),
            output(
                "confession — Confession — the world's own\n\
                 1. chapel\n\
                 \x20  repent — alignment: law 5.00, good 0.00 — then the end\n\
                 \x20    error: may lock out heist: it moves alignment toward lawful, which no action moves back toward chaotic, so it can take the character outside guild's member tolerance, which its gate needs; declare it in locks\n\
                 \x20    error: may lock out muster.drill: it moves alignment toward lawful, and watch expels members who drift out of its tolerance, which can end the membership of watch that the stage needs; declare it in locks"
            )
        );
    }

    #[test]
    fn a_stale_lock_warns_at_its_choice() {
        assert_eq!(
            run(&format!("graph {WORLDS}/stale_lock chores")),
            output(
                "chores — Chores — the world's own\n\
                 1. yard\n\
                 \x20  skip — standing: watch -5.00 — locks muster.drill — then the end\n\
                 \x20    warning: locks[0]: it can't lock out muster.drill: nothing it does can make that stage's requirements false for good; remove it\n\
                 edges\n\
                 \x20 chores.yard.skip locks muster.drill"
            )
        );
    }

    #[test]
    fn graph_names_an_unknown_questline_or_quest_with_a_suggestion() {
        assert_eq!(
            run("graph content/sample watch_oth"),
            command_error("unknown questline or quest 'watch_oth' (did you mean 'watch_oath'?)")
        );
        assert_eq!(
            run("graph content/sample nowhere"),
            command_error("unknown questline or quest 'nowhere'")
        );
    }

    #[test]
    fn graph_needs_a_directory_and_one_questline_or_quest() {
        let form = "graph needs the form: graph <dir> <questline|quest>";
        assert_eq!(run("graph content/sample"), command_error(form));
        assert_eq!(run("graph content/sample a b"), command_error(form));
        assert_eq!(
            run("graph nowhere/at/all watch_career"),
            command_error("unknown questline or quest 'watch_career'")
        );
    }
}

//! Quests as graphs (U4): the sample's questline, its quests' edges, and the loader's quest
//! problems at the node each is about.

use std::path::Path;

use factional_content::{CONTENT_FILES, ContentTexts, QuestGraph, outline_texts, read_texts};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

/// The sample with `change` applied to the file named `file`.
fn sample_with(file: &str, change: impl Fn(&str) -> String) -> ContentTexts {
    let mut texts = sample();
    let place = CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file");
    let text = texts[place].take().expect("the sample has it");
    texts[place] = Some(change(&text));
    texts
}

fn replace(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "the sample has {from:?}");
    text.replacen(from, to, 1)
}

fn graph_of(texts: &ContentTexts) -> QuestGraph {
    QuestGraph::new(texts, &outline_texts(texts))
}

#[test]
fn edges_are_what_gates_and_stages_need_and_what_choices_lock() {
    let edges: Vec<String> = graph_of(&sample())
        .edges
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        edges,
        [
            "circle_rite.whisper.set_them_at_war locks watch_captain",
            "circle_rite.whisper.set_them_at_war locks watch_captain.command",
            "smugglers_cove needs watch_oath.patrol.report done",
            "the_long_winter.stores.share locks circle_rite",
        ]
    );
}

#[test]
fn a_questline_brings_the_quests_at_the_far_end_of_its_edges() {
    let graph = graph_of(&sample());
    let line = graph.line("watch_career").expect("the sample's questline");
    assert_eq!(line.line.steps.len(), 4);
    let outside: Vec<&str> = line.outside.iter().map(|quest| quest.id.as_str()).collect();
    assert_eq!(outside, ["circle_rite"]);
    let edges: Vec<String> = line.edges.iter().map(ToString::to_string).collect();
    assert_eq!(
        edges,
        [
            "circle_rite.whisper.set_them_at_war locks watch_captain",
            "circle_rite.whisper.set_them_at_war locks watch_captain.command",
            "smugglers_cove needs watch_oath.patrol.report done",
        ]
    );
    assert!(graph.line("watch_oath").is_none());
}

#[test]
fn quests_in_no_questline_are_listed_apart() {
    let graph = graph_of(&sample());
    let loose: Vec<&str> = graph
        .loose()
        .iter()
        .map(|quest| quest.id.as_str())
        .collect();
    assert_eq!(loose, ["circle_rite", "lost_ring", "the_long_winter"]);
}

#[test]
fn the_graph_shows_while_the_world_doesnt_load() {
    let texts = sample_with("factions.toml", |text| {
        replace(text, "\ntolerance = 40.0\n", "\ntolerance = 140.0\n")
    });
    assert!(!outline_texts(&texts).loads());
    let graph = graph_of(&texts);
    let line = graph
        .line("watch_career")
        .expect("the questline still reads");
    assert_eq!(line.line.steps.len(), 4);
    assert_eq!(graph.edges.len(), 4);
}

#[test]
fn a_problem_sits_at_the_choice_stage_step_or_questline_its_key_names() {
    let texts = sample_with("quests.toml", |text| {
        let text = replace(
            text,
            "{ id = \"follow_the_lights\", next = \"raid\" }",
            "{ id = \"follow_the_lights\", next = \"rade\" }",
        );
        replace(
            &text,
            "id = \"raid\"\n",
            "id = \"raid\"\nrequires = { member = [\"city_wach\"] }\n",
        )
    });
    let graph = graph_of(&texts);
    let choice = graph.at_choice("smugglers_cove", 0, 0);
    assert_eq!(choice.len(), 1);
    assert!(choice[0].problem);
    let said = choice[0].said_at("smugglers_cove.stages[0].choices[0]");
    assert!(said.starts_with("error: next: "), "{said}");
    assert!(graph.at_stage("smugglers_cove", 0).is_empty());
    let stage = graph.at_stage("smugglers_cove", 1);
    assert_eq!(stage.len(), 1);
    let said = stage[0].said_at("smugglers_cove.stages[1]");
    assert!(said.starts_with("error: requires.member[0]: "), "{said}");
    assert!(graph.at_quest("smugglers_cove").is_empty());
    assert_eq!(graph.in_quest("smugglers_cove").len(), 2);
    assert!(graph.in_quest("watch_oath").is_empty());

    let texts = sample_with("quests.toml", |text| {
        replace(text, "giver = \"city_watch\"", "giver = \"city_wach\"")
    });
    let graph = graph_of(&texts);
    let quest = graph.at_quest("watch_oath");
    assert_eq!(quest.len(), 1);
    assert!(quest[0].said_at("watch_oath").starts_with("error: giver: "));
    assert!(graph.at_stage("watch_oath", 0).is_empty());

    let texts = sample_with("questlines.toml", |text| {
        let text = replace(text, "need = 2", "need = 4");
        replace(&text, "giver = \"city_watch\"", "giver = \"city_wach\"")
    });
    let graph = graph_of(&texts);
    let step = graph.at_step("watch_career", 1);
    assert_eq!(step.len(), 1);
    assert!(
        step[0]
            .said_at("watch_career.steps[1]")
            .starts_with("error: need: ")
    );
    assert!(graph.at_step("watch_career", 0).is_empty());
    let line = graph.at_line("watch_career");
    assert_eq!(line.len(), 1);
    assert!(
        line[0]
            .said_at("watch_career")
            .starts_with("error: giver: ")
    );
}

#[test]
fn a_questlines_notes_include_its_quests() {
    let texts = sample_with("quests.toml", |text| {
        replace(
            text,
            "{ id = \"follow_the_lights\", next = \"raid\" }",
            "{ id = \"follow_the_lights\", next = \"rade\" }",
        )
    });
    let graph = graph_of(&texts);
    assert_eq!(graph.in_line("watch_career").len(), 1);
    assert!(graph.at_line("watch_career").is_empty());
    assert!(graph.in_line("nowhere").is_empty());
    let texts = sample_with("questlines.toml", |text| {
        replace(text, "need = 2", "need = 4")
    });
    assert_eq!(graph_of(&texts).in_line("watch_career").len(), 1);
}

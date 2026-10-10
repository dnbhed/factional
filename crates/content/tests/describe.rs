//! Quests in words (U4): the sample's quests, steps and choices as the CLI's `quests` and
//! `graph` and the editor's Quests tab all describe them.

use std::path::Path;

use factional_content::{
    CONTENT_FILES, describe_choice, describe_mark, load_quests, load_texts, map_heading,
    named_effects, named_shifts, needs, quest_heading, read_texts,
};
use factional_quests::{Quest, Quests};
use factional_reputation::{Alignment, Content, FactionId, OutcomeId, World};

fn sample() -> (Content, Quests) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    load_quests(&dir).expect("the sample loads")
}

fn quest<'q>(quests: &'q Quests, id: &str) -> &'q Quest {
    quests
        .quests
        .values()
        .find(|quest| quest.id.as_str() == id)
        .expect("a sample quest")
}

/// The `choice`th choice of a quest's `stage`th stage, in words.
fn choice(quests: &Quests, id: &str, stage: usize, choice: usize) -> String {
    describe_choice(&quest(quests, id).stages[stage].choices[choice])
}

#[test]
fn a_quest_heading_says_whose_it_is_and_where_it_sits() {
    let (_, quests) = sample();
    let heading = |id: &str, at_step: bool| quest_heading(&quests, quest(&quests, id), at_step);
    assert_eq!(
        heading("watch_oath", true),
        "watch_oath — The Watch's Oath — from city_watch, in watch_career at step 1"
    );
    assert_eq!(
        heading("watch_oath", false),
        "watch_oath — The Watch's Oath — from city_watch, in watch_career"
    );
    assert_eq!(
        heading("lost_dog", true),
        "lost_dog — The Captain's Dog — from captain_hale, in watch_career at step 3"
    );
    assert_eq!(
        heading("the_long_winter", true),
        "the_long_winter — The Long Winter — the world's own"
    );
}

#[test]
fn needs_lists_every_requirement_joined_by_and() {
    let (_, quests) = sample();
    assert_eq!(
        needs(&quest(&quests, "watch_oath").requires),
        " — needs not to be in lantern_guild"
    );
    assert_eq!(
        needs(&quest(&quests, "watch_oath").stages[1].requires),
        " — needs standing 10.00 with city_watch"
    );
    assert_eq!(
        needs(&quest(&quests, "watch_captain").stages[0].requires),
        " — needs to be in city_watch, and to be within city_watch's member tolerance"
    );
    assert_eq!(
        needs(&quest(&quests, "smugglers_cove").requires),
        " — needs watch_oath.patrol.report done"
    );
    let career = quests
        .questlines
        .values()
        .next()
        .expect("the sample's questline");
    assert_eq!(
        needs(&career.steps[3].requires),
        " — needs standing 40.00 with city_watch, and rank sergeant or higher in city_watch"
    );
    assert_eq!(needs(&quest(&quests, "night_patrol").requires), "");
}

#[test]
fn a_choice_says_what_it_does_what_it_locks_and_where_it_leads() {
    let (_, quests) = sample();
    assert_eq!(
        choice(&quests, "watch_oath", 0, 0),
        "report — outcome turned_in_vex — then oath"
    );
    assert_eq!(
        choice(&quests, "watch_oath", 1, 0),
        "swear — alignment: law 5.00, good 0.00 — then the end"
    );
    assert_eq!(
        choice(&quests, "night_patrol", 0, 0),
        "by_the_book — standing: city_watch 5.00 — then the end"
    );
    assert_eq!(
        choice(&quests, "dock_inspection", 0, 0),
        "seize — alignment: law 2.00, good 0.00 — standing: city_watch 5.00, lantern_guild -5.00 — then the end"
    );
    assert_eq!(
        choice(&quests, "lost_dog", 0, 0),
        "found_it — standing: city_watch 2.00, captain_hale 10.00 — then the end"
    );
    assert_eq!(
        choice(&quests, "the_long_winter", 0, 0),
        "share — alignment: law 0.00, good 6.00 — standing: temple 10.00 — relations: temple ↔ city_watch 5.00 — locks circle_rite — then the end"
    );
    assert_eq!(
        choice(&quests, "circle_rite", 0, 0),
        "refuse — then the end"
    );
    assert_eq!(
        choice(&quests, "circle_rite", 0, 1),
        "set_them_at_war — relations: temple ↔ city_watch -120.00 — locks watch_captain, watch_captain.command — then the end"
    );
}

#[test]
fn relation_shifts_name_both_ends_and_which_way_they_go() {
    let (content, _) = sample();
    let outcome = |id: &str| &content.outcomes[&OutcomeId::new(id).expect("valid id")];
    assert_eq!(
        named_shifts(&outcome("sowed_discord").effects.relations),
        [
            "city_watch ↔ temple -40.00",
            "city_watch → ashen_circle -20.00"
        ]
    );
    assert!(named_effects(&outcome("sowed_discord").effects.standing).is_empty());
}

#[test]
fn a_map_says_whose_it_is_and_where_everyone_stands_in_words() {
    let (content, _) = sample();
    let world = World::new(content).expect("the sample makes a world");
    let watch = FactionId::new("city_watch").expect("valid id");
    let map = world.alignment_map(&watch).expect("the Watch");
    assert_eq!(
        map_heading("The City Watch", &map),
        "The City Watch (city_watch): law 70.00, good 20.00, tolerance 40.00"
    );
    let hale = map
        .characters
        .iter()
        .find(|mark| mark.character.as_str() == "captain_hale")
        .expect("on the map");
    assert_eq!(describe_mark(hale), "B captain_hale: 5.59 away, within");
    // Pictured somewhere they aren't, it says so.
    let mut misjudged = hale.clone();
    misjudged.within = false;
    misjudged.distance.subject = Alignment::new(
        "60".parse().expect("a number"),
        "10".parse().expect("a number"),
    )
    .expect("on the plane");
    assert_eq!(
        describe_mark(&misjudged),
        "B captain_hale: 5.59 away, outside; pictured at law 60.00, good 10.00, truly law 75.00, good 30.00"
    );
}

#[test]
fn content_held_in_memory_loads_as_a_directory_does() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    let mut texts = read_texts(&dir).expect("the sample is there");
    let (content, quests) = load_texts(&texts).expect("it loads");
    assert_eq!(content.characters.len(), 6);
    assert_eq!(quests.quests.len(), 10);
    let factions = CONTENT_FILES
        .iter()
        .position(|name| *name == "factions.toml")
        .expect("a content file");
    texts[factions] = Some("[city_watch\n".to_owned());
    let problems = load_texts(&texts).expect_err("it doesn't load").diagnostics;
    assert_eq!(problems.len(), 1);
}

//! A questline's page (U13a, P-94): beside its read-only graph, its steps in order, each a card
//! of its quests, which open their own, and "+ Existing quest…", offering the quests in no
//! questline, added in one change; and "+ New quest", made in `quests.toml` and added to its
//! step as one change to both files (U13b); quests moved between steps, steps reordered, and
//! a step's need and leftovers set on its card (U13c); and pages nested under a breadcrumb, a
//! quest's its stages and a stage's its choices (U13d). Only a copy of the sample is ever saved.

use std::fs;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::egui::{Key, Modifiers};
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

fn on_watch_career() -> Harness<'static, Editor> {
    on_watch_career_in(Path::new(SAMPLE))
}

fn on_watch_career_in(dir: &Path) -> Harness<'static, Editor> {
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.run();
    click(&mut harness, "Quests");
    click(&mut harness, "watch_career — A Life in the Watch");
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn height(harness: &Harness<'static, Editor>, label: &str) -> f32 {
    harness.get_by_label(label).rect().center().y
}

#[test]
fn a_questlines_page_has_its_steps_in_order_each_with_its_quests() {
    let harness = on_watch_career();
    harness.get_by_label("A Life in the Watch");
    let steps: Vec<f32> = (1..=4)
        .map(|step| height(&harness, &format!("Step {step}")))
        .collect();
    assert!(steps.windows(2).all(|pair| pair[0] < pair[1]), "{steps:?}");
    // Each step says what it needs, and lists its quests by name.
    harness.get_by_label(
        "2 of night_patrol, dock_inspection, smugglers_cove, in any order; the rest close",
    );
    let patrol = height(&harness, "Night Patrol · night_patrol");
    assert!(steps[1] < patrol && patrol < steps[2]);
    harness.get_by_label("The Watch's Oath · watch_oath");
}

#[test]
fn an_existing_quest_is_added_to_a_step_in_one_change() {
    let mut harness = on_watch_career();
    click(&mut harness, "Add an existing quest to step 3…");
    // Only quests in no questline: a quest is in one at most.
    harness.get_by_label("Add lost_ring to step 3");
    harness.get_by_label("Add circle_rite to step 3");
    assert!(
        harness
            .query_by_label("Add night_patrol to step 3")
            .is_none()
    );
    click(&mut harness, "Add lost_ring to step 3");
    let ring = height(&harness, "Ava's Lost Ring · lost_ring");
    assert!(height(&harness, "Step 3") < ring && ring < height(&harness, "Step 4"));
    click(&mut harness, "Undo");
    assert!(
        harness
            .query_by_label("Ava's Lost Ring · lost_ring")
            .is_none()
    );
}

#[test]
fn a_quest_on_the_page_opens_its_own_and_goes_back() {
    let mut harness = on_watch_career();
    click(&mut harness, "Night Patrol · night_patrol");
    harness.get_by_label("Edit in Content");
    assert!(harness.query_by_label("Step 1").is_none());
    click(&mut harness, "Back to A Life in the Watch");
    harness.get_by_label("Step 1");
}

/// A fresh copy of the sample, at `<tmp>/<name>`.
fn sample_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for entry in fs::read_dir(SAMPLE).expect("the sample") {
        let path = entry.expect("an entry").path();
        fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
    }
    dir
}

/// Types `id` as the new quest of the `step`th step, and makes it.
fn new_quest(harness: &mut Harness<'static, Editor>, step: usize, id: &str) {
    let field = format!("New quest id in step {step}");
    harness.get_by_label(&field).scroll_to_me();
    harness.run();
    harness.get_by_label(&field).focus();
    harness.run();
    harness.get_by_label(&field).type_text(id);
    harness.run();
    click(harness, &format!("Add a new quest to step {step}"));
}

#[test]
fn a_new_quest_is_made_and_added_to_its_step_in_one_change() {
    let dir = sample_copy("new_quest");
    let mut harness = on_watch_career_in(&dir);
    new_quest(&mut harness, 3, "harbour_watch");
    // It opens, ready to be written.
    harness.get_by_label("Back to A Life in the Watch");
    click(&mut harness, "Save");
    let quests = fs::read_to_string(dir.join("quests.toml")).expect("written");
    let quests: toml::Table = quests.parse().expect("toml");
    // With the stage and choice a quest must have, so it reads: one choice, ending it.
    let stage = &quests["harbour_watch"]["stages"][0];
    assert_eq!(stage["id"].as_str(), Some("start"));
    assert_eq!(stage["choices"][0]["id"].as_str(), Some("done"));
    assert_eq!(stage["choices"][0]["next"].as_str(), Some("end"));
    let lines: toml::Table = fs::read_to_string(dir.join("questlines.toml"))
        .expect("written")
        .parse()
        .expect("toml");
    let step = &lines["watch_career"]["steps"][2]["quests"];
    assert_eq!(
        step.as_array().and_then(|quests| quests.last()),
        Some(&toml::Value::String("harbour_watch".to_owned()))
    );
    // One Undo takes back both.
    click(&mut harness, "Back to A Life in the Watch");
    harness.get_by_label("harbour_watch");
    click(&mut harness, "Undo");
    assert!(harness.query_by_label("harbour_watch").is_none());
    assert!(
        harness.query_by_label_contains("harbour_watch —").is_none(),
        "nor is it among the quests"
    );
}

#[test]
fn a_new_quests_id_must_be_new() {
    let mut harness = on_watch_career();
    new_quest(&mut harness, 3, "lost_ring");
    harness.get_by_label("couldn't add: lost_ring is already there");
    assert!(
        harness
            .query_by_label("Back to A Life in the Watch")
            .is_none()
    );
}

/// Types `input` over what's in the field labelled `label`, and enters it.
fn enter(harness: &mut Harness<'static, Editor>, label: &str, input: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    harness.get_by_label(label).type_text(input);
    harness.key_press(Key::Enter);
    harness.run();
}

/// Whether what's labelled `label` is in the card of the `step`th step, the last step's card
/// running to the end.
fn in_step(harness: &Harness<'static, Editor>, label: &str, step: usize) -> bool {
    let at = height(harness, label);
    let after = harness
        .query_by_label(&format!("Step {}", step + 1))
        .map_or(f32::MAX, |next| next.rect().center().y);
    height(harness, &format!("Step {step}")) < at && at < after
}

#[test]
fn a_quest_moves_to_another_step_in_one_change() {
    let mut harness = on_watch_career();
    let docks = "Inspecting the Docks · dock_inspection";
    assert!(in_step(&harness, docks, 2));
    click(&mut harness, "Move dock_inspection…");
    assert!(
        harness
            .query_by_label("Move dock_inspection to step 2")
            .is_none()
    );
    click(&mut harness, "Move dock_inspection to step 3");
    assert!(in_step(&harness, docks, 3));
    click(&mut harness, "Undo");
    assert!(in_step(&harness, docks, 2));
}

#[test]
fn a_step_moves_up_or_down() {
    let mut harness = on_watch_career();
    assert!(harness.query_by_label("Move step 1 up").is_none());
    assert!(harness.query_by_label("Move step 4 down").is_none());
    click(&mut harness, "Move step 3 up");
    assert!(in_step(&harness, "Harbour Errands · harbour_errands", 2));
    assert!(in_step(&harness, "Night Patrol · night_patrol", 3));
    click(&mut harness, "Move step 2 down");
    assert!(in_step(&harness, "Night Patrol · night_patrol", 2));
}

#[test]
fn a_steps_need_and_leftovers_are_set_on_its_card() {
    let mut harness = on_watch_career();
    assert_eq!(
        harness.get_by_label("Need of step 2").value().as_deref(),
        Some("2")
    );
    // Left out, a step needs all its quests.
    assert_eq!(
        harness.get_by_label("Need of step 1").value().as_deref(),
        Some("")
    );
    enter(&mut harness, "Need of step 2", "3");
    harness.get_by_label("all of night_patrol, dock_inspection, smugglers_cove, in any order");
    enter(&mut harness, "Need of step 2", "");
    harness.get_by_label("all of night_patrol, dock_inspection, smugglers_cove, in any order");
    assert_eq!(
        harness.get_by_label("Need of step 2").value().as_deref(),
        Some("")
    );
    // A step with no need written has one added.
    enter(&mut harness, "Need of step 1", "0");
    harness.get_by_label("any of watch_oath, or none");
    for _ in 0..3 {
        click(&mut harness, "Undo");
    }
    click(&mut harness, "Leftovers of step 2: open");
    harness.get_by_label("2 of night_patrol, dock_inspection, smugglers_cove, in any order");
}

#[test]
fn pages_nest_under_a_breadcrumb_each_part_going_back_there() {
    let mut harness = on_watch_career();
    click(&mut harness, "Night Patrol · night_patrol");
    // A Life in the Watch › Step 2 › Night Patrol: the quest's page, its stages.
    harness.get_by_label("Back to A Life in the Watch");
    harness.get_by_label("Back to Step 2");
    assert!(harness.query_by_label_contains("by_the_book").is_none());
    click(&mut harness, "1. walk");
    // … › Night Patrol › Stage "walk": the stage's page, its choices.
    harness.get_by_label("Stage \"walk\"");
    assert_eq!(
        harness.query_all_by_label("›").count(),
        3,
        "between the four parts"
    );
    harness.get_by_label_contains("by_the_book");
    click(&mut harness, "Back to Night Patrol");
    harness.get_by_label("1. walk");
    assert!(harness.query_by_label_contains("by_the_book").is_none());
    click(&mut harness, "Back to Step 2");
    harness.get_by_label("Step 1");
}

#[test]
fn a_stage_says_how_many_problems_its_choices_have() {
    let stale = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/cli/tests/fixtures/worlds/stale_lock");
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(stale));
    harness.run();
    click(&mut harness, "Quests");
    click(&mut harness, "chores — Chores — 1 warning");
    // A quest in no questline is the first part of its breadcrumb.
    assert!(harness.query_by_label_contains("Back to").is_none());
    // The warning is at the stage's choice, on the stage's page; the stage counts it.
    assert!(
        harness
            .query_by_label_contains("warning: locks[0]")
            .is_none()
    );
    click(&mut harness, "1. yard — 1 warning");
    harness.get_by_label_contains("warning: locks[0]: it can't lock out muster.drill");
}

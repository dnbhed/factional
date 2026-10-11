//! Each place's adding and removing where it is (U10b): on its heading, beside its group's
//! name, at its table's end or beside the entry's name, never in a list after the form.
//! Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

/// The editor on the sample, with `entry` chosen.
fn on(entry: &str) -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(SAMPLE)),
    );
    harness.run();
    click(&mut harness, entry);
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

/// How far down the window the middle of what's labelled `label` is.
fn height(harness: &Harness<'static, Editor>, label: &str) -> f32 {
    harness.get_by_label(label).rect().center().y
}

/// Whether what's labelled `a` and `b` are on one line.
fn on_one_line(harness: &Harness<'static, Editor>, a: &str, b: &str) -> bool {
    (height(harness, a) - height(harness, b)).abs() < 6.0
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

#[test]
fn a_rank_is_added_by_the_plus_at_the_end_of_the_ladder() {
    let mut harness = on("city_watch");
    // "+ Rank", under the captain.
    harness.get_by_label("Add to ranks");
    assert!(height(&harness, "Add to ranks") > height(&harness, "ranks[2].id"));
    assert!(harness.query_by_label("Add to ranks…").is_none());
    click(&mut harness, "Add to ranks");
    assert_eq!(value(&harness, "ranks[3].id").as_deref(), Some(""));
}

#[test]
fn a_tables_adding_and_removing_are_on_its_heading() {
    let harness = on("city_watch");
    assert!(on_one_line(&harness, "Remove alignment", "Alignment"));
    assert!(on_one_line(&harness, "Remove weights", "Weights"));
    // The alignment has both its axes, so there's nothing more to add.
    assert!(harness.query_by_label("Add to alignment…").is_none());
    // The drift's heading is its group's name, so its removing is beside that.
    assert!(on_one_line(&harness, "Remove drift", "Drift"));
    // Murder's target curves can take one for the law.
    let harness = on("murder");
    assert!(on_one_line(&harness, "Add to by_target…", "By target"));
}

#[test]
fn the_entrys_removing_is_beside_its_name() {
    let harness = on("city_watch");
    let header = height(&harness, "factions.toml › city_watch");
    let entry = height(&harness, "Remove city_watch");
    assert!(header < entry && entry < height(&harness, "name"));
}

#[test]
fn a_new_stage_is_added_in_the_stages_and_a_new_choice_beside_its_stage() {
    let mut harness = on("dock_inspection");
    // A new stage is the stages' section's to offer, at its end (U10d).
    assert!(height(&harness, "Add to stages") > height(&harness, "Stage \"inspect\""));
    assert!(on_one_line(
        &harness,
        "Add to stages[0].choices",
        "Stage \"inspect\""
    ));
    assert!(on_one_line(
        &harness,
        "Add to stages[0]…",
        "Stage \"inspect\""
    ));
    click(&mut harness, "Add to stages[0].choices");
    assert_eq!(
        value(&harness, "stages[0].choices[2].id").as_deref(),
        Some("")
    );
}

#[test]
fn a_table_keyed_by_ids_offers_those_that_exist_by_name() {
    let mut harness = on("vex");
    // At the end of the standing's section (U10d), after its lines.
    assert!(
        height(&harness, "Add to standing.factions…")
            > height(&harness, "standing.factions.lantern_guild")
    );
    click(&mut harness, "Add to standing.factions…");
    // The Guild's is there already.
    assert!(
        harness
            .query_by_label("Add lantern_guild to standing.factions")
            .is_none()
    );
    click(&mut harness, "Add city_watch to standing.factions");
    assert_eq!(
        value(&harness, "standing.factions.city_watch").as_deref(),
        Some("0.0")
    );
}

#[test]
fn an_empty_table_is_shown_with_its_removing_beside_it() {
    let mut harness = on("vex");
    click(&mut harness, "Remove standing.factions.lantern_guild");
    harness.get_by_label("empty");
    assert!(on_one_line(
        &harness,
        "Remove standing.factions",
        "Factions"
    ));
}

#[test]
fn a_cells_remove_takes_what_it_is_within_its_item() {
    let mut harness = on("city_watch");
    click(&mut harness, "Remove ranks[1].requires");
    assert!(
        harness
            .query_by_label("ranks[1].requires.standing")
            .is_none()
    );
    assert_eq!(value(&harness, "ranks[1].id").as_deref(), Some("sergeant"));
}

#[test]
fn what_names_an_entry_is_said_in_words() {
    let harness = on("city_watch");
    harness.get_by_label("Captain Hale › Membership \"The City Watch\" › Faction");
}

#[test]
fn a_pair_is_its_two_ids_on_one_line_either_side_of_an_arrow() {
    let harness = on("sowed_discord");
    let first = "relations[0].between[0]";
    let second = "relations[0].between[1]";
    assert_eq!(value(&harness, first).as_deref(), Some("city_watch"));
    assert_eq!(value(&harness, second).as_deref(), Some("temple"));
    assert!(on_one_line(&harness, first, second));
    assert!(on_one_line(&harness, first, "↔"));
    assert!(on_one_line(&harness, first, "Between"));
    // Its × is the pair's, on its line; each id is still chosen from the factions.
    assert!(on_one_line(&harness, first, "Remove relations[0].between"));
    harness.get_by_label(&format!("Choose {second}"));
    // Neither is numbered on a row of its own.
    assert!(harness.query_by_label("1").is_none());
}

#[test]
fn a_list_of_any_number_of_ids_isnt_a_pair() {
    let harness = on("merchant_ava");
    assert!(!on_one_line(&harness, "contacts[0]", "contacts[1]"));
    assert!(harness.query_by_label("↔").is_none());
}

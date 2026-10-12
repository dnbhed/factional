//! The Content tab's form (U6b): an entry's values grouped by what they mean under its name,
//! with their comments, the defaults of what's left out, choices where there are any, and
//! what refers to the entry.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

/// The editor on the sample, with `entry` chosen. Nothing here is saved.
fn on(entry: &str) -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(SAMPLE)),
    );
    harness.run();
    // An entry or a field may be further down than the window shows.
    harness.get_by_label(entry).scroll_to_me();
    harness.run();
    harness.get_by_label(entry).click();
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

#[test]
fn an_entry_shows_its_name_and_its_values_grouped_with_their_comments() {
    let harness = on("city_watch");
    harness.get_by_label("factions.toml › city_watch");
    // Its name heads the form, as it does its place among the entries.
    assert_eq!(harness.query_all_by_label("The City Watch").count(), 2);
    for group in [
        "Identity",
        "Where it stands",
        "Membership",
        "Ranks",
        "Drift",
    ] {
        harness.get_by_label(group);
    }
    // Every section is there, even with nothing in it yet (U10d).
    harness.get_by_label("Its own rules");
    harness.get_by_label("# Cares about order far more than kindness.");
    harness.get_by_label(
        "# Captains are held to a stricter standard than the faction's member tolerance.",
    );
    assert_eq!(value(&harness, "tolerance").as_deref(), Some("40.0"));
    assert_eq!(value(&harness, "drift.grace_ticks").as_deref(), Some("100"));
    harness.get_by_label("-20.00 — not set, the default");
    harness.get_by_label("false — not set, the default");
}

#[test]
fn set_writes_a_default_in_and_undo_takes_it_out() {
    let mut harness = on("city_watch");
    assert!(
        harness
            .query_by_label("Standing change on expulsion")
            .is_some()
    );
    click(&mut harness, "Set expel_standing_change");
    assert_eq!(
        value(&harness, "expel_standing_change").as_deref(),
        Some("-20.0")
    );
    assert!(
        harness
            .query_by_label("-20.00 — not set, the default")
            .is_none()
    );
    click(&mut harness, "Undo");
    harness.get_by_label("-20.00 — not set, the default");
}

#[test]
fn a_value_with_choices_is_chosen_from_them() {
    let mut harness = on("city_watch");
    click(&mut harness, "Choose drift.then");
    click(&mut harness, "demote");
    assert_eq!(value(&harness, "drift.then").as_deref(), Some("demote"));
    harness.get_by_label_contains(
        "drift = { policy = \"probation\", grace_ticks = 100, then = \"demote\" }",
    );
    // A value that may be anything has nothing to choose from.
    assert!(harness.query_by_label("Choose tolerance").is_none());
}

#[test]
fn what_refers_to_an_entry_is_listed_and_each_goes_to_its_entry() {
    let mut harness = on("city_watch");
    harness.get_by_label("Referenced by");
    // Grouped by the file each is in.
    harness.get_by_label("factions.toml");
    // Each in words (U10b): the Temple's regard for the Watch is in the Temple (U12a).
    harness.get_by_label("Temple of the Dawn › Relations › The City Watch");
    assert!(harness.query_by_label("Nothing names city_watch").is_none());
    click(
        &mut harness,
        "Captain Hale › Membership \"The City Watch\" › Faction",
    );
    harness.get_by_label("characters.toml › captain_hale");
    assert_eq!(value(&harness, "alignment.law").as_deref(), Some("75.0"));
    // Nothing names the player.
    click(&mut harness, "player");
    harness.get_by_label("Nothing names player");
}

#[test]
fn what_the_loader_says_and_the_entry_as_written_are_beside_it() {
    let harness = on("city_watch");
    harness.get_by_label("Nothing at city_watch");
    harness.get_by_label_contains("name = \"The City Watch\"");
}

#[test]
fn a_value_is_said_in_words_under_headings_for_where_it_is() {
    let harness = on("city_watch");
    // Each axis under the table it's in, said once.
    assert_eq!(harness.query_all_by_label("Alignment").count(), 1);
    assert_eq!(harness.query_all_by_label("Weights").count(), 1);
    assert_eq!(harness.query_all_by_label("Law").count(), 2);
    harness.get_by_label("Member tolerance");
    // The drift's own heading is its group's, so its values are just their words.
    harness.get_by_label("Grace (ticks)");
    assert!(harness.query_by_label("drift.grace_ticks").is_some());
    // A rank ladder's columns are in words too.
    harness.get_by_label("Requirements › Standing");
}

#[test]
fn a_quests_stages_and_choices_are_headings_by_their_ids() {
    let harness = on("dock_inspection");
    harness.get_by_label("Stage \"inspect\"");
    harness.get_by_label("Choice \"seize\"");
    harness.get_by_label("Choice \"wave_through\"");
    assert_eq!(harness.query_all_by_label("Effects").count(), 2);
    // Each field is still found by its key.
    assert_eq!(
        value(&harness, "stages[0].choices[0].effects.alignment.law").as_deref(),
        Some("2.0")
    );
}

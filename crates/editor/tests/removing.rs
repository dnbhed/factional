//! Adding and removing, to board 2 (U6d): keys to add with what each is, removing an entry
//! that's named only once the designer has seen what names it, and what the last change
//! broke, with Undo beside it. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

fn on_watch() -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(SAMPLE)),
    );
    harness.run();
    click(&mut harness, "city_watch");
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn the_keys_to_add_say_what_each_is() {
    let mut harness = on_watch();
    assert!(
        harness
            .query_by_label("Add expel_standing_change to city_watch")
            .is_none()
    );
    click(&mut harness, "Add to city_watch…");
    harness.get_by_label("a number from -100.00 to 100.00, -20.00 by default");
    harness.get_by_label("The change in standing with the faction when it expels someone.");
    harness.get_by_label("true or false, false by default");
    // A place with nothing more to add is named beside its ×.
    harness.get_by_label("weights");
    click(&mut harness, "Add secret_members to city_watch");
    assert_eq!(
        harness.get_by_label("secret_members").value().as_deref(),
        Some("false")
    );
}

#[test]
fn what_a_change_broke_is_said_with_undo_beside_it() {
    let mut harness = on_watch();
    click(&mut harness, "Remove tolerance");
    harness.get_by_label("Removed tolerance: 1 new problem");
    harness.get_by_label_contains("doesn't load: 1 problem");
    click(&mut harness, "Show it");
    harness.get_by_label("factions.toml › city_watch");
    click(&mut harness, "Undo it");
    assert!(
        harness
            .query_by_label("Removed tolerance: 1 new problem")
            .is_none()
    );
    harness.get_by_label("Problems 0");
    assert_eq!(
        harness.get_by_label("tolerance").value().as_deref(),
        Some("40.0")
    );
}

#[test]
fn removing_an_entry_that_is_named_asks_first() {
    let mut harness = on_watch();
    click(&mut harness, "Remove city_watch");
    harness.get_by_label_contains("city_watch is named in");
    click(&mut harness, "Keep it");
    assert!(
        harness
            .query_by_label_contains("city_watch is named in")
            .is_none()
    );
    harness.get_by_label("factions.toml › city_watch");
    click(&mut harness, "Remove city_watch");
    click(&mut harness, "Remove anyway");
    assert!(harness.query_by_label("city_watch").is_none());
    harness.get_by_label_contains("Removed city_watch:");
}

#[test]
fn removing_an_entry_nothing_names_asks_nothing() {
    let mut harness = on_watch();
    click(&mut harness, "player");
    click(&mut harness, "Remove player");
    assert!(harness.query_by_label("player").is_none());
    assert!(harness.query_by_label("Remove anyway").is_none());
}

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
    // What's below the form may be further down than the window shows.
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn the_keys_to_add_say_what_each_is() {
    let mut harness = on_watch();
    // The recruit's rung can still have a requirement and a tolerance of its own.
    assert!(
        harness
            .query_by_label("Add tolerance to ranks[0]")
            .is_none()
    );
    click(&mut harness, "Add to ranks[0]…");
    harness.get_by_label("a table");
    harness.get_by_label("What a promotion to this rank needs.");
    harness.get_by_label("a number at least 0.00");
    // A place with nothing more to add has only its ×, on its heading (U10b).
    harness.get_by_label("Remove weights");
    assert!(harness.query_by_label("Add to weights…").is_none());
    click(&mut harness, "Add tolerance to ranks[0]");
    assert_eq!(
        harness
            .get_by_label("ranks[0].tolerance")
            .value()
            .as_deref(),
        Some("0.0")
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

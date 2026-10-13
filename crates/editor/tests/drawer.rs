//! The drawer (U15a, D-40, P-96): along the bottom of every tab, with the problems in it,
//! each going to its entry from wherever it's shown; it closes and opens again. Nothing here
//! is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::{Editor, WINDOW, Workspace};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

const TYPOS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../cli/tests/fixtures/worlds/typos"
);

fn open() -> Harness<'static, Editor> {
    open_on(TYPOS)
}

fn open_on(dir: &str) -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(dir)),
    );
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn the_drawer_is_along_the_bottom_of_every_tab() {
    let mut harness = open();
    for tab in ["Content", "Quests", "Previews"] {
        click(&mut harness, tab);
        harness.get_by_label("Problems 3");
        let drawer = harness.get_by_label("Problems 3").rect().center().y;
        assert!(drawer > 450.0, "{tab}: along the bottom, at {drawer}");
    }
}

#[test]
fn a_problem_goes_to_its_entry_from_any_tab() {
    let mut harness = open();
    click(&mut harness, "Quests");
    click(&mut harness, "Vex › Membership \"lantern_gild\" › Faction");
    assert_eq!(harness.state().workspace(), Workspace::Content);
    harness.get_by_label("characters.toml › vex");
}

#[test]
fn the_drawer_closes_and_opens_again() {
    let mut harness = open();
    click(&mut harness, "Close the drawer");
    assert!(harness.query_by_label("Problems 3").is_none());
    // Closed, it counts the problems, not the warnings.
    click(&mut harness, "Open the drawer, with 3 problems");
    harness.get_by_label("Problems 3");
}

#[test]
fn the_drawer_opens_at_its_height_however_little_it_shows() {
    // The sample has no problems, the shortest list there is.
    let mut harness = open_on(SAMPLE);
    for tab in ["Problems", "Play"] {
        click(&mut harness, tab);
        let top = harness.get_by_label("Close the drawer").rect().top();
        // It opens 160 points tall; its edge and margins take a few.
        assert!(WINDOW.y - top >= 150.0, "{tab}: the drawer's top at {top}");
    }
}

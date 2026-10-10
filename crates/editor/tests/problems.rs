//! Problems at their keys (U6c, board 3): a panel of every problem and warning, each with
//! its key as a way to its entry and the loader's "did you mean" as a fix, and each also
//! under its field. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
use factional_editor::{Editor, WINDOW};

const TYPOS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../cli/tests/fixtures/worlds/typos"
);

fn open() -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(TYPOS)),
    );
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn every_problem_is_listed_with_the_loaders_fix_which_can_be_undone() {
    let mut harness = open();
    harness.get_by_label("Problems 3");
    harness.get_by_label("Warnings 0");
    harness.get_by_label("unknown drift policy 'demot' (did you mean 'demote'?)");
    harness.get_by_label("Use 'demote'");
    harness.get_by_label("Use 'weights'");
    click(&mut harness, "Use 'lantern_guild'");
    harness.get_by_label("Problems 2");
    assert!(harness.query_by_label("Use 'lantern_guild'").is_none());
    click(&mut harness, "Undo");
    harness.get_by_label("Problems 3");
    // With every fix made, the world loads.
    for fix in ["Use 'demote'", "Use 'weights'", "Use 'lantern_guild'"] {
        click(&mut harness, fix);
    }
    harness.get_by_label("Problems 0");
    harness.get_by_label_contains("typos loads");
}

#[test]
fn a_problem_shows_under_its_field_with_its_fix() {
    let mut harness = open();
    click(&mut harness, "vex — 1 problem");
    let said = "unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)";
    // In the panel and under the field.
    assert_eq!(harness.query_all_by_label(said).count(), 2);
    assert_eq!(harness.query_all_by_label("Use 'lantern_guild'").count(), 2);
    assert!(harness.query_by_label("Nothing at vex").is_none());
    harness.get_by_label("Some at their fields");
    // Captain Hale's misspelt key is the entry's, so it's said beside the form.
    click(&mut harness, "captain_hale — 1 problem");
    harness.get_by_label("error: unknown key 'weigths' (did you mean 'weights'?)");
    assert!(harness.query_by_label("Some at their fields").is_none());
    assert_eq!(harness.query_all_by_label("Use 'weights'").count(), 2);
}

#[test]
fn a_problems_key_goes_to_its_entry() {
    let mut harness = open();
    click(&mut harness, "captain_hale");
    harness.get_by_label("characters.toml › captain_hale");
    click(&mut harness, "temple.drift.policy");
    harness.get_by_label("factions.toml › temple");
}

#[test]
fn the_entries_can_be_narrowed_to_those_with_problems() {
    let mut harness = open();
    harness.get_by_label("city_watch");
    click(&mut harness, "Only entries with problems");
    assert!(harness.query_by_label("city_watch").is_none());
    harness.get_by_label("vex — 1 problem");
    harness.get_by_label("temple — 1 problem");
    click(&mut harness, "Only entries with problems");
    harness.get_by_label("city_watch");
}

#[test]
fn warnings_are_listed_on_their_own() {
    let mut harness = open();
    let chosen = |harness: &Harness<'static, Editor>, label: &str| {
        harness.get_by_label(label).accesskit_node().toggled()
    };
    assert_eq!(chosen(&harness, "Problems 3"), Some(Toggled::True));
    assert_eq!(chosen(&harness, "Warnings 0"), Some(Toggled::False));
    click(&mut harness, "Warnings 0");
    assert_eq!(chosen(&harness, "Problems 3"), Some(Toggled::False));
    assert_eq!(chosen(&harness, "Warnings 0"), Some(Toggled::True));
    harness.get_by_label("No warnings");
    assert!(
        harness
            .query_by_label("unknown drift policy 'demot' (did you mean 'demote'?)")
            .is_none()
    );
    click(&mut harness, "Problems 3");
    harness.get_by_label("unknown drift policy 'demot' (did you mean 'demote'?)");
}

//! Lists of tables as tables (U6f): a rank ladder as a rung a row and a key a column, each
//! cell its value's field, its default, or nothing. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::egui::{Key, Modifiers};
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

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
    harness.get_by_label(label).click();
    harness.run();
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

#[test]
fn a_rank_ladder_is_a_table_of_its_rungs() {
    let harness = on("city_watch");
    for column in ["id", "requires.standing", "tolerance"] {
        harness.get_by_label(&format!("ranks · {column}"));
    }
    assert_eq!(value(&harness, "ranks[0].id").as_deref(), Some("recruit"));
    assert_eq!(
        value(&harness, "ranks[1].requires.standing").as_deref(),
        Some("30.0")
    );
    assert_eq!(
        value(&harness, "ranks[2].tolerance").as_deref(),
        Some("25.0")
    );
    // The rungs are numbered from 1.
    harness.get_by_label("3");
    assert!(harness.query_by_label("0").is_none());
    // The recruit has neither a requirement nor a tolerance of its own.
    assert!(
        harness
            .query_by_label("ranks[0].requires.standing")
            .is_none()
    );
    assert!(harness.query_by_label("ranks[0].tolerance").is_none());
    // A cell's comment is under its row, after its column.
    harness.get_by_label("ranks[2] · tolerance");
    harness.get_by_label(
        "# Captains are held to a stricter standard than the faction's member tolerance.",
    );
}

#[test]
fn a_rung_is_removed_and_a_cell_set_from_the_table() {
    let mut harness = on("city_watch");
    harness.get_by_label("ranks[1].requires.standing").focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness
        .get_by_label("ranks[1].requires.standing")
        .type_text("35.0");
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(
        value(&harness, "ranks[1].requires.standing").as_deref(),
        Some("35.0")
    );
    click(&mut harness, "Remove ranks[1]");
    assert_eq!(value(&harness, "ranks[1].id").as_deref(), Some("captain"));
    assert!(harness.query_by_label("ranks[2].id").is_none());
    // Each rung has what can be added to it beside it.
    click(&mut harness, "Add to ranks[0]…");
    harness.get_by_label("Add tolerance to ranks[0]");
}

#[test]
fn a_membership_is_a_table_row_with_its_choices_and_defaults() {
    let harness = on("captain_hale");
    assert_eq!(
        value(&harness, "memberships[0].faction").as_deref(),
        Some("city_watch")
    );
    harness.get_by_label("Choose memberships[0].faction");
    harness.get_by_label("Set memberships[0].secret");
}

#[test]
fn a_list_of_plain_values_stays_rows() {
    let harness = on("sister_mira");
    assert_eq!(
        value(&harness, "contacts[0]").as_deref(),
        Some("brother_ash")
    );
}

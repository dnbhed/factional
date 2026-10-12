//! Adding requirements and effects by kind (U11b): "+ Requirement…" and "+ Effect…", each a
//! menu of the kinds its place takes, with each id's choices, made in one change. Nothing here
//! is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

#[test]
fn a_requirement_is_added_by_its_kind_from_the_requirements_section() {
    let mut harness = on("dock_inspection");
    // The quest has no requirements yet; its section offers each kind.
    click(&mut harness, "Add to requires…");
    click(&mut harness, "Member of");
    click(&mut harness, "Add temple to requires.member");
    assert_eq!(
        value(&harness, "requires.member[0]").as_deref(),
        Some("temple")
    );
    // In one change.
    click(&mut harness, "Undo");
    assert!(harness.query_by_label("requires.member[0]").is_none());
}

#[test]
fn a_stages_requirements_offer_their_kinds_in_place() {
    let mut harness = on("watch_oath");
    click(&mut harness, "Add to stages[1].requires…");
    click(&mut harness, "Standing");
    click(
        &mut harness,
        "Add lantern_guild to stages[1].requires.standing",
    );
    harness.get_by_label("stages[1].requires.standing.lantern_guild");
    // The standing's own "+" is the kind's, in the menu, not beside it too.
    assert!(
        harness
            .query_by_label("Add to stages[1].requires.standing…")
            .is_none()
    );
}

#[test]
fn an_effect_is_added_by_its_kind_beside_the_effects() {
    let mut harness = on("dock_inspection");
    let effects = "stages[0].choices[0].effects";
    assert!(
        harness
            .query_by_label(&format!("Add to {effects}.standing.factions…"))
            .is_none()
    );
    click(&mut harness, &format!("Add to {effects}…"));
    click(&mut harness, &format!("Add good to {effects}.alignment"));
    harness.get_by_label(&format!("{effects}.alignment.good"));
}

#[test]
fn a_relation_shift_is_added_as_an_effect() {
    let mut harness = on("dock_inspection");
    let effects = "stages[0].choices[0].effects";
    click(&mut harness, &format!("Add to {effects}…"));
    click(&mut harness, &format!("Add to {effects}.relations"));
    assert_eq!(
        value(&harness, &format!("{effects}.relations[0].by")).as_deref(),
        Some("0.0")
    );
}

//! Every section, with its adding in it (U10d): each section an entry may have is shown, even
//! with nothing in it, and ends with what can be added there. Nothing here is saved.

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

fn height(harness: &Harness<'static, Editor>, label: &str) -> f32 {
    harness.get_by_label(label).rect().center().y
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

#[test]
fn a_characters_sections_are_there_with_nothing_in_them_and_add_in_place() {
    let mut harness = on("merchant_ava");
    // Ava has no memberships and no standing, but the sections are there to add them.
    let memberships = height(&harness, "Memberships");
    let standing = height(&harness, "Standing");
    let adding = height(&harness, "Add to memberships");
    assert!(memberships < adding && adding < standing);
    click(&mut harness, "Add to memberships");
    assert_eq!(
        value(&harness, "memberships[0].faction").as_deref(),
        Some("")
    );
}

#[test]
fn a_standing_line_is_added_in_one_step_and_undone_in_one() {
    let mut harness = on("merchant_ava");
    click(&mut harness, "Add to standing.factions…");
    click(&mut harness, "Add city_watch to standing.factions");
    assert_eq!(
        value(&harness, "standing.factions.city_watch").as_deref(),
        Some("0.0")
    );
    click(&mut harness, "Undo");
    assert!(
        harness
            .query_by_label("standing.factions.city_watch")
            .is_none()
    );
    // The section is still there, to add to again.
    harness.get_by_label("Add to standing.factions…");
}

#[test]
fn a_contact_is_picked_from_the_characters() {
    let mut harness = on("vex");
    click(&mut harness, "Add to contacts…");
    click(&mut harness, "Add captain_hale to contacts");
    assert_eq!(
        value(&harness, "contacts[0]").as_deref(),
        Some("captain_hale")
    );
}

#[test]
fn a_key_is_added_from_its_section_and_the_entry_has_only_its_remove_beside_its_name() {
    let mut harness = on("city_watch");
    assert!(harness.query_by_label("Add to city_watch…").is_none());
    harness.get_by_label("Remove city_watch");
    // A faction's own rules are a section, even before it has any.
    assert!(height(&harness, "Its own rules") < height(&harness, "Add defectors to city_watch"));
    click(&mut harness, "Add defectors to city_watch");
    harness.get_by_label("Defectors");
    assert!(
        harness
            .query_by_label("Add defectors to city_watch")
            .is_none()
    );
}

#[test]
fn a_rank_is_offered_once_at_the_end_of_its_section() {
    let harness = on("city_watch");
    assert_eq!(harness.query_all_by_label("Add to ranks").count(), 1);
    assert!(height(&harness, "Add to ranks") > height(&harness, "ranks[2].id"));
}

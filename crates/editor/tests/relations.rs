//! Each faction's relations, both ways (U12b, D-38): every other faction a row, how this one
//! regards it and how it regards this one back, each with its band, a mark where the pair is
//! in conflict, and "same both ways", linked while the two are equal. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
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
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn value(harness: &Harness<'static, Editor>, label: &str) -> Option<String> {
    harness.get_by_label(label).value()
}

/// Types `input` over what's in the field labelled `label`, and enters it.
fn enter(harness: &mut Harness<'static, Editor>, label: &str, input: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.get_by_label(label).type_text(input);
    harness.key_press(Key::Enter);
    harness.run();
}

fn linked(harness: &Harness<'static, Editor>, other: &str) -> bool {
    let label = format!("Same both ways with {other}");
    harness.get_by_label(&label).accesskit_node().toggled() == Some(Toggled::True)
}

#[test]
fn a_factions_page_has_every_other_faction_both_ways() {
    let harness = on("temple");
    // The Temple and the Watch are allies, the same both ways.
    assert_eq!(
        value(&harness, "relations.city_watch").as_deref(),
        Some("60.0")
    );
    assert_eq!(
        value(&harness, "city_watch.relations.temple").as_deref(),
        Some("60.0")
    );
    assert!(linked(&harness, "city_watch"));
    assert_eq!(harness.query_all_by_label("allied").count(), 2);
    // The Temple and the Circle are enemies, so in conflict.
    harness.get_by_label("In conflict with The Ashen Circle");
    assert!(
        harness
            .query_by_label("In conflict with The City Watch")
            .is_none()
    );
    // The Free Company is regarded at 0 either way, with nothing written.
    harness.get_by_label("Set relations.free_company");
    harness.get_by_label("Set free_company.relations.temple");
    // Every other faction is a row already, so the section offers none to add.
    assert!(harness.query_by_label("Add to relations…").is_none());
}

#[test]
fn a_change_to_one_side_of_a_linked_pair_writes_both_and_undoes_in_one() {
    let mut harness = on("temple");
    enter(&mut harness, "relations.city_watch", "50.0");
    assert_eq!(
        value(&harness, "relations.city_watch").as_deref(),
        Some("50.0")
    );
    assert_eq!(
        value(&harness, "city_watch.relations.temple").as_deref(),
        Some("50.0")
    );
    assert!(linked(&harness, "city_watch"));
    click(&mut harness, "Undo");
    assert_eq!(
        value(&harness, "city_watch.relations.temple").as_deref(),
        Some("60.0")
    );
    assert_eq!(
        value(&harness, "relations.city_watch").as_deref(),
        Some("60.0")
    );
}

#[test]
fn an_unlinked_pair_changes_one_way_for_a_grudge() {
    let mut harness = on("temple");
    click(&mut harness, "Same both ways with city_watch");
    assert!(!linked(&harness, "city_watch"));
    enter(&mut harness, "city_watch.relations.temple", "20.0");
    assert_eq!(
        value(&harness, "city_watch.relations.temple").as_deref(),
        Some("20.0")
    );
    assert_eq!(
        value(&harness, "relations.city_watch").as_deref(),
        Some("60.0")
    );
    assert!(!linked(&harness, "city_watch"));
    harness.get_by_label("friendly");
}

#[test]
fn set_writes_a_pair_with_nothing_written_both_ways() {
    let mut harness = on("temple");
    click(&mut harness, "Set relations.free_company");
    assert_eq!(
        value(&harness, "relations.free_company").as_deref(),
        Some("0.0")
    );
    assert_eq!(
        value(&harness, "free_company.relations.temple").as_deref(),
        Some("0.0")
    );
}

#[test]
fn an_unlinked_pair_is_linked_again_once_the_faction_is_left() {
    let mut harness = on("temple");
    click(&mut harness, "Same both ways with city_watch");
    assert!(!linked(&harness, "city_watch"));
    // Unticking is for the visit: the two are still the same.
    click(&mut harness, "city_watch");
    click(&mut harness, "temple");
    assert!(linked(&harness, "city_watch"));
}

//! Editing values in the editor (U2): each change written to the file's text in memory, the
//! problems updated at once, undo, save, and reading again to discard.

use std::fs;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::Editor;
use factional_editor::egui::{Key, Modifiers};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// A fresh copy of the broken fixture, as `<tmp>/<name>`.
fn broken_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    fs::copy(
        Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken/characters.toml"),
        dir.join("characters.toml"),
    )
    .expect("copied");
    dir
}

/// The editor on `dir`, with `vex` selected.
fn on_vex(dir: &Path) -> Harness<'static, Editor> {
    let mut harness =
        Harness::new_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.run();
    harness.get_by_label("vex — 1 problem").click();
    harness.run();
    harness
}

/// Replaces what's in the field labelled `label` with `text`, then presses Enter.
fn enter(harness: &mut Harness<'static, Editor>, label: &str, text: &str) {
    harness.get_by_label(label).focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.get_by_label(label).type_text(text);
    harness.key_press(Key::Enter);
    harness.run();
}

fn on_disk(dir: &Path) -> String {
    fs::read_to_string(dir.join("characters.toml")).expect("readable")
}

#[test]
fn an_entrys_values_are_shown_in_fields() {
    let dir = broken_copy("editing_fields");
    let harness = on_vex(&dir);
    assert_eq!(harness.get_by_label("name").value().as_deref(), Some("Vex"));
    assert_eq!(
        harness.get_by_label("alignment.law").value().as_deref(),
        Some("120.0")
    );
    assert_eq!(
        harness.get_by_label("alignment.good").value().as_deref(),
        Some("-20.0")
    );
}

#[test]
fn a_change_updates_the_problems_at_once_but_not_the_file() {
    let dir = broken_copy("editing_change");
    let before = on_disk(&dir);
    let mut harness = on_vex(&dir);
    enter(&mut harness, "alignment.law", "20.0");
    harness.get_by_label_contains("doesn't load: 2 problems");
    harness.get_by_label("vex");
    assert_eq!(on_disk(&dir), before);
}

#[test]
fn undo_steps_back_one_change() {
    let dir = broken_copy("editing_undo");
    let mut harness = on_vex(&dir);
    enter(&mut harness, "alignment.law", "20.0");
    harness.get_by_label("Undo").click();
    harness.run();
    harness.get_by_label_contains("doesn't load: 3 problems");
    assert_eq!(
        harness.get_by_label("alignment.law").value().as_deref(),
        Some("120.0")
    );
}

#[test]
fn save_writes_the_change_keeping_everything_else() {
    let dir = broken_copy("editing_save");
    let before = on_disk(&dir);
    let mut harness = on_vex(&dir);
    enter(&mut harness, "alignment.law", "20.0");
    harness.get_by_label("Save").click();
    harness.run();
    assert_eq!(
        on_disk(&dir),
        before.replacen("law = 120.0", "law = 20.0", 1)
    );
}

#[test]
fn a_refused_value_says_why_and_changes_nothing() {
    let dir = broken_copy("editing_refused");
    let mut harness = on_vex(&dir);
    enter(&mut harness, "alignment.law", "fast");
    harness.get_by_label("expected a number");
    harness.get_by_label_contains("doesn't load: 3 problems");
    harness.get_by_label("Save").click();
    harness.run();
    assert!(on_disk(&dir).contains("law = 120.0"));
}

#[test]
fn reading_again_discards_unsaved_changes() {
    let dir = broken_copy("editing_discard");
    let mut harness = on_vex(&dir);
    enter(&mut harness, "alignment.law", "20.0");
    harness.get_by_label_contains("doesn't load: 2 problems");
    harness.get_by_label("Read again").click();
    harness.run();
    harness.get_by_label_contains("doesn't load: 3 problems");
}

//! The editor's look (U6e, board 6): the canvas's colours, and a value changed since the last
//! save marked, with its file.

use std::fs;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::egui::{Key, Modifiers};
use factional_editor::{Editor, WINDOW, theme};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

/// A fresh copy of the sample, as `<tmp>/<name>`.
fn sample_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for file in fs::read_dir(SAMPLE).expect("the sample") {
        let file = file.expect("a file").path();
        fs::copy(&file, dir.join(file.file_name().expect("named"))).expect("copied");
    }
    dir
}

fn open(dir: &Path) -> Harness<'static, Editor> {
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

fn set_tolerance(harness: &mut Harness<'static, Editor>) {
    click(harness, "city_watch");
    harness.get_by_label("tolerance").focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.get_by_label("tolerance").type_text("45.0");
    harness.key_press(Key::Enter);
    harness.run();
}

#[test]
fn the_editor_draws_with_the_canvas_colours() {
    let harness = open(Path::new(SAMPLE));
    assert_eq!(harness.ctx.global_style().visuals, theme::visuals());
    assert_eq!(harness.ctx.global_style().text_styles, theme::text_styles());
}

#[test]
fn a_value_changed_since_the_last_save_is_marked_with_its_file_until_undone() {
    let dir = sample_copy("look_undo");
    let mut harness = open(&dir);
    assert!(harness.query_by_label_contains("edited").is_none());
    set_tolerance(&mut harness);
    harness.get_by_label("tolerance, edited");
    harness.get_by_label("factions.toml — 5 entries, edited");
    assert!(harness.query_by_label("member_tolerance, edited").is_none());
    click(&mut harness, "Undo");
    assert!(harness.query_by_label_contains("edited").is_none());
}

#[test]
fn saving_clears_what_was_edited() {
    let dir = sample_copy("look_save");
    let mut harness = open(&dir);
    set_tolerance(&mut harness);
    click(&mut harness, "Save");
    assert!(harness.query_by_label_contains("edited").is_none());
}

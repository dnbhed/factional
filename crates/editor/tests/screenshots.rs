//! Pictures of the editor's key pages (U19), rendered headless with wgpu and compared with
//! those checked in under `tests/snapshots/`. AccessKit proves what's on a page; these show
//! how it looks. A missing or changed picture fails, leaving `<name>.new.png` and a diff
//! beside it; `UPDATE_SNAPSHOTS=1 cargo test -p factional-editor --test screenshots` accepts
//! them, after each is looked at.
//!
//! The sample is opened by a relative path, as the status line and Play print it, so the
//! pictures are the same on every machine.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::egui::Key;
use factional_editor::{Editor, WINDOW};

/// The sample, from the crate's directory, where integration tests run.
const SAMPLE: &str = "../../content/sample";

fn open() -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).wgpu().build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(SAMPLE),
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

/// Types `line` into Play and runs it.
fn play(harness: &mut Harness<'static, Editor>, line: &str) {
    harness.get_by_label("Play command").focus();
    harness.run();
    harness.get_by_label("Play command").type_text(line);
    harness.key_press(Key::Enter);
    harness.run();
}

/// The form, its Relations among it: allied, enemy, in conflict, the same both ways.
#[test]
fn a_factions_form_and_relations() {
    let mut harness = open();
    click(&mut harness, "temple");
    harness.snapshot("faction_form");
}

#[test]
fn a_questlines_page() {
    let mut harness = open();
    click(&mut harness, "Quests");
    click(&mut harness, "watch_career — A Life in the Watch");
    harness.snapshot("questline_page");
}

#[test]
fn play_after_a_few_commands() {
    let mut harness = open();
    click(&mut harness, "Play");
    play(&mut harness, "characters");
    play(&mut harness, "act player steal --target merchant_ava");
    harness.snapshot("play");
}

#[test]
fn the_previews_tab() {
    let mut harness = open();
    click(&mut harness, "Previews");
    click(&mut harness, "The City Watch");
    harness.snapshot("previews");
}

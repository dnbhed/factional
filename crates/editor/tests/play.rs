//! Play (U15b, D-40, P-96): the REPL in the drawer, on the content as it stands, unsaved edits
//! included, reloaded with `reload`'s rules each time it loads. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
use factional_editor::egui::{Key, Modifiers};
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

fn open() -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(SAMPLE)),
    );
    harness.run();
    click(&mut harness, "Play");
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

/// Types `input` over what's in the field labelled `label`, and enters it.
fn enter(harness: &mut Harness<'static, Editor>, label: &str, input: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    harness.get_by_label(label).type_text(input);
    harness.key_press(Key::Enter);
    harness.run();
}

#[test]
fn play_runs_commands_on_the_content_as_it_stands() {
    let mut harness = open();
    harness.get_by_label_contains("loaded 6 characters from");
    let chosen = |name: &str| harness.get_by_label(name).accesskit_node().toggled();
    assert_eq!(chosen("Play"), Some(Toggled::True));
    assert_eq!(chosen("Problems"), Some(Toggled::False));
    play(&mut harness, "characters");
    harness.get_by_label("> characters");
    harness.get_by_label_contains("merchant_ava — Merchant Ava");
    // What was typed comes back with the up arrow.
    harness.get_by_label("Play command").focus();
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(
        harness.get_by_label("Play command").value().as_deref(),
        Some("characters")
    );
}

#[test]
fn play_reloads_as_the_content_loads_replaying_what_was_played() {
    let mut harness = open();
    play(&mut harness, "act player steal --target merchant_ava");
    click(&mut harness, "vex");
    enter(&mut harness, "name", "Vexa");
    harness.get_by_label_contains("replayed 1 command");
    play(&mut harness, "show character vex");
    harness.get_by_label_contains("vex — Vexa — ");
}

#[test]
fn while_the_content_doesnt_load_play_keeps_the_last_world_that_did() {
    let mut harness = open();
    click(&mut harness, "vex");
    enter(&mut harness, "alignment.law", "250.0");
    harness.get_by_label("The content doesn't load: Play has the last world that did.");
    play(&mut harness, "show character vex");
    harness.get_by_label_contains("vex — Vex — law -55.00");
}

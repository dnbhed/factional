//! Play (U15b, D-40, P-96): the REPL in the drawer, on the content as it stands, unsaved edits
//! included, reloaded with `reload`'s rules each time it loads; with Tab completing what's
//! typed, and each id it says a link to its entry (U15c). Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
use factional_editor::egui::{Key, Modifiers};
use factional_editor::{Editor, WINDOW, Workspace};

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
    harness.get_by_label_contains("— Merchant Ava —");
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
    harness.get_by_label_contains("— Vexa —");
}

#[test]
fn while_the_content_doesnt_load_play_keeps_the_last_world_that_did() {
    let mut harness = open();
    click(&mut harness, "vex");
    enter(&mut harness, "alignment.law", "250.0");
    harness.get_by_label("The content doesn't load: Play has the last world that did.");
    play(&mut harness, "show character vex");
    harness.get_by_label_contains("— Vex — law -55.00");
}

#[test]
fn tab_completes_what_is_typed() {
    let mut harness = open();
    harness.get_by_label("Play command").focus();
    harness.run();
    harness
        .get_by_label("Play command")
        .type_text("show character merch");
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(
        harness.get_by_label("Play command").value().as_deref(),
        Some("show character merchant_ava ")
    );
    // With more than one, what they share, and each listed.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    harness
        .get_by_label("Play command")
        .type_text("relations city_watch tu");
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(
        harness.get_by_label("Play command").value().as_deref(),
        Some("relations city_watch turned_in_vex ")
    );
    harness.get_by_label("Play command").type_text("t");
    harness.key_press(Key::Tab);
    harness.run();
    harness.get_by_label("temple, the_long_winter, took_a_bribe, turned_in_vex");
    assert_eq!(
        harness.get_by_label("Play command").value().as_deref(),
        Some("relations city_watch turned_in_vex t")
    );
}

#[test]
fn an_id_play_says_goes_to_its_entry() {
    let mut harness = open();
    play(&mut harness, "characters");
    assert!(
        harness
            .query_all_by_label("Go to merchant_ava")
            .next()
            .is_some(),
        "merchant_ava is a link"
    );
    click_first(&mut harness, "Go to merchant_ava");
    assert_eq!(harness.state().workspace(), Workspace::Content);
    harness.get_by_label("characters.toml › merchant_ava");
    play(&mut harness, "outcomes");
    click_first(&mut harness, "Go to turned_in_vex");
    harness.get_by_label("outcomes.toml › turned_in_vex");
}

fn click_first(harness: &mut Harness<'static, Editor>, label: &str) {
    let first = |harness: &Harness<'static, Editor>| harness.query_all_by_label(label).count();
    assert!(first(harness) > 0, "{label}");
    harness
        .query_all_by_label(label)
        .next()
        .expect("there")
        .scroll_to_me();
    harness.run();
    harness
        .query_all_by_label(label)
        .next()
        .expect("there")
        .click();
    harness.run();
}

#[test]
fn what_play_says_stays_above_the_line_to_type_on() {
    let mut harness = open();
    play(&mut harness, "characters");
    play(&mut harness, "act player steal --target merchant_ava");
    let line = harness.get_by_label("Play command").rect();
    // The last thing said, at the bottom of the log.
    let last = harness
        .get_by_label_contains("moved from 0.00 to -20.00")
        .rect();
    assert!(last.bottom() <= line.top(), "{last:?} covers {line:?}");
}

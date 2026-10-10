//! Adding and removing in the editor (U3): new entries by id, the schema's keys and list
//! items where they fit, and removing values, tables and entries, all undoable.

use std::fs;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::Editor;

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

fn open(dir: &Path) -> Harness<'static, Editor> {
    let mut harness =
        Harness::new_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.run();
    harness
}

/// The editor on `dir`, with `vex` selected.
fn on_vex(dir: &Path) -> Harness<'static, Editor> {
    let mut harness = open(dir);
    harness.get_by_label("vex — 1 problem").click();
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

/// Types `id` into the field labelled `New id in <place>`, then adds it.
fn add_id(harness: &mut Harness<'static, Editor>, place: &str, id: &str) {
    let field = format!("New id in {place}");
    harness.get_by_label(&field).focus();
    harness.run();
    harness.get_by_label(&field).type_text(id);
    harness.run();
    click(harness, &format!("Add to {place}"));
}

#[test]
fn a_new_entry_is_added_by_its_id_and_selected() {
    let dir = broken_copy("adding_entry");
    let mut harness = open(&dir);
    add_id(&mut harness, "characters.toml", "ava");
    harness.get_by_label("characters.toml: ava");
    assert_eq!(harness.get_by_label("name").value().as_deref(), Some(""));
    assert_eq!(
        harness.get_by_label("alignment.law").value().as_deref(),
        Some("0.0")
    );
}

#[test]
fn the_schemas_keys_are_offered_where_they_fit() {
    let dir = broken_copy("adding_keys");
    let mut harness = on_vex(&dir);
    assert!(
        harness.query_by_label("Add name to vex").is_none(),
        "already there"
    );
    click(&mut harness, "Add standing to vex");
    click(&mut harness, "Add factions to standing");
    add_id(&mut harness, "standing.factions", "city_watch");
    assert_eq!(
        harness
            .get_by_label("standing.factions.city_watch")
            .value()
            .as_deref(),
        Some("0.0")
    );
    click(&mut harness, "Save");
    let saved = fs::read_to_string(dir.join("characters.toml")).expect("saved");
    assert!(
        saved.contains("standing = { factions = { city_watch = 0.0 } }\n"),
        "{saved}"
    );
}

#[test]
fn values_tables_and_entries_can_be_removed_and_put_back() {
    let dir = broken_copy("adding_removing");
    let mut harness = on_vex(&dir);
    click(&mut harness, "Remove alignment.good");
    assert!(harness.query_by_label("alignment.good").is_none());
    harness.get_by_label("alignment.law");
    click(&mut harness, "Remove alignment");
    assert!(harness.query_by_label("alignment.law").is_none());
    click(&mut harness, "Remove vex");
    harness.get_by_label_contains("Choose an entry on the left");
    harness.get_by_label_contains("doesn't load: 2 problems");
    for _ in 0..3 {
        click(&mut harness, "Undo");
    }
    harness.get_by_label_contains("doesn't load: 3 problems");
    harness.get_by_label("vex — 1 problem");
}

#[test]
fn an_addition_that_cant_be_made_says_why() {
    let dir = broken_copy("adding_refused");
    let mut harness = open(&dir);
    add_id(&mut harness, "characters.toml", "vex");
    harness.get_by_label("couldn't add: vex is already there");
    assert!(harness.get_by_label("Undo").accesskit_node().is_disabled());
}

#[test]
fn a_file_not_there_is_started_by_adding_to_it() {
    let dir = broken_copy("adding_new_file");
    let mut harness = open(&dir);
    click(&mut harness, "Add relation to relations.toml");
    harness.get_by_label("relations.toml: relation[0]");
    click(&mut harness, "Save");
    assert_eq!(
        fs::read_to_string(dir.join("relations.toml")).expect("written"),
        "[[relation]]\nvalue = 0.0\nbetween = [\"\", \"\"]\n"
    );
}

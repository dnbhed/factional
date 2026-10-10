//! The editor's view of a content directory (U1), driven headless through AccessKit.

use std::fs;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
use factional_editor::{Editor, WINDOW};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn broken() -> PathBuf {
    Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken")
}

fn harness(dir: PathBuf) -> Harness<'static, Editor> {
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.run();
    harness
}

#[test]
fn opening_a_directory_shows_each_entry_with_its_problems() {
    let harness = harness(broken());
    harness.get_by_label("hale — 2 problems");
    harness.get_by_label("vex — 1 problem");
    harness.get_by_label("balance.toml — not there");
    harness.get_by_label_contains("doesn't load: 3 problems");
}

#[test]
fn selecting_an_entry_shows_its_toml_and_its_problems() {
    let mut harness = harness(broken());
    assert!(
        harness
            .query_by_label("error: missing 'alignment'")
            .is_none()
    );
    harness.get_by_label("hale — 2 problems").click();
    harness.run();
    harness.get_by_label("error: missing 'alignment'");
    assert!(harness.query_by_label("Nothing at hale").is_none());
    harness.get_by_label("error: unknown key 'alignmnet' (did you mean 'alignment'?)");
    harness.get_by_label_contains("name = \"Captain Hale\"");
    let toggled = |label: &str| harness.get_by_label(label).accesskit_node().toggled();
    assert_eq!(toggled("hale — 2 problems"), Some(Toggled::True));
    assert_eq!(toggled("vex — 1 problem"), Some(Toggled::False));
}

#[test]
fn the_editor_runs_as_an_eframe_app() {
    let mut harness = Harness::new_eframe(|_| Editor::open(broken()));
    harness.run();
    harness.get_by_label("hale — 2 problems");
    harness.get_by_label_contains("doesn't load: 3 problems");
}

#[test]
fn reading_again_shows_the_content_as_it_is_now() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor_reread");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    fs::copy(
        broken().join("characters.toml"),
        dir.join("characters.toml"),
    )
    .expect("copied");
    let mut harness = harness(dir.clone());
    harness.get_by_label_contains("doesn't load: 3 problems");
    fs::write(
        dir.join("characters.toml"),
        "[vex]\nname = \"Vex\"\nalignment = { law = 20.0, good = -20.0 }\n",
    )
    .expect("fixed");
    harness.get_by_label("Read again").click();
    harness.run();
    harness.get_by_label_contains("editor_reread loads");
    harness.get_by_label("vex");
    assert!(harness.query_by_label("hale — 2 problems").is_none());
}

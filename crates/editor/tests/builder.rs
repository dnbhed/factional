//! The editor and the builder open each other (G8): "Play in the builder" plays the selected
//! character, only once the changes are saved and the content loads; and `--at` opens the
//! editor at an entry, as the builder's "Edit in the editor" asks.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::{Editor, Opening, USAGE, WINDOW, opening};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn broken() -> PathBuf {
    Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken")
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_owned()).collect()
}

fn os(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

/// The sample with `key` in `characters.toml` selected, its name changed in memory.
fn edited(key: &str) -> Editor {
    let mut editor = Editor::open(sample());
    editor.select_at("characters.toml", key);
    let name = editor
        .fields()
        .iter()
        .position(|field| field.row.key == "name")
        .expect("a name");
    editor.set(name, "Someone else").expect("a name");
    editor
}

#[test]
fn playing_a_selected_character_plays_them() {
    let dir = sample();
    let mut editor = Editor::open(&dir);
    let shown = dir.display().to_string();
    assert_eq!(
        editor.play_args(),
        os(&[&shown]),
        "nothing selected: the player"
    );
    editor.select_at("characters.toml", "captain_hale");
    assert_eq!(
        editor.play_args(),
        os(&[&shown, "--as", "captain_hale"]),
        "Hale selected: Hale"
    );
    editor.select_at("factions.toml", "city_watch");
    assert_eq!(
        editor.play_args(),
        os(&[&shown]),
        "a faction selected: the player"
    );
}

#[test]
fn the_builder_plays_only_saved_content_that_loads() {
    assert_eq!(Editor::open(sample()).play_blocked(), None);
    assert_eq!(
        edited("vex").play_blocked(),
        Some("Save first: the builder plays what's saved")
    );
    assert_eq!(
        Editor::open(broken()).play_blocked(),
        Some("The builder needs content that loads")
    );
}

#[test]
fn play_in_the_builder_is_in_the_top_bar_and_disabled_while_blocked() {
    let harness = |editor: Editor| {
        let mut harness = Harness::builder()
            .with_size(WINDOW)
            .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), editor);
        harness.run();
        harness
    };
    let open = harness(Editor::open(sample()));
    assert!(
        !open
            .get_by_label("Play in the builder")
            .accesskit_node()
            .is_disabled()
    );
    let unsaved = harness(edited("vex"));
    assert!(
        unsaved
            .get_by_label("Play in the builder")
            .accesskit_node()
            .is_disabled()
    );
    unsaved.get_by_label("Save first: the builder plays what's saved");
}

#[test]
fn the_editor_opens_on_a_directory_at_an_entry() {
    assert_eq!(
        opening(&args(&["content/sample"])),
        Ok(Opening {
            dir: PathBuf::from("content/sample"),
            at: None,
        })
    );
    assert_eq!(
        opening(&args(&[
            "content/sample",
            "--at",
            "characters.toml:captain_hale"
        ])),
        Ok(Opening {
            dir: PathBuf::from("content/sample"),
            at: Some(("characters.toml".to_owned(), "captain_hale".to_owned())),
        })
    );
    for wrong in [
        &[][..],
        &["content/sample", "--at"],
        &["content/sample", "--at", "captain_hale"],
        &["content/sample", "--at", ":captain_hale"],
        &["content/sample", "--at", "characters.toml:"],
        &["content/sample", "--as", "characters.toml:captain_hale"],
        &["content/sample", "extra"],
    ] {
        assert_eq!(opening(&args(wrong)), Err(USAGE.to_owned()), "{wrong:?}");
    }
}

//! The builder opens the editor (G8): "Edit in the editor", on the same directory, at the
//! character in view.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bevy::MinimalPlugins;
use bevy::app::App;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::FactionalPlugin;
use factional_builder::{BuilderPlugin, ContentDir, Nearby, edit_args, edit_ui};
use factional_reputation::CharacterId;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn os(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

#[test]
fn the_editor_opens_at_the_nearest_else_the_one_played() {
    let dir = Path::new("content/sample");
    let near_hale = Nearby {
        nearest: Some(id("captain_hale")),
        seen_by: Default::default(),
    };
    assert_eq!(
        edit_args(dir, &near_hale, Some(&id("player"))),
        os(&["content/sample", "--at", "characters.toml:captain_hale"])
    );
    assert_eq!(
        edit_args(dir, &Nearby::default(), Some(&id("player"))),
        os(&["content/sample", "--at", "characters.toml:player"])
    );
    assert_eq!(
        edit_args(dir, &Nearby::default(), None),
        os(&["content/sample"])
    );
}

#[test]
fn the_builder_keeps_the_directory_it_plays() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: "player".to_owned(),
        });
    app.update();
    assert_eq!(app.world().resource::<ContentDir>().0, sample());
}

#[test]
fn edit_in_the_editor_is_a_button_that_says_why_it_failed() {
    let mut harness = Harness::new_ui_state(
        |ui, asked: &mut usize| {
            if edit_ui(
                ui,
                Some("factional-editor isn't beside this app or on PATH"),
            ) {
                *asked += 1;
            }
        },
        0,
    );
    harness.run();
    harness.get_by_label("factional-editor isn't beside this app or on PATH");
    harness.get_by_label("Edit in the editor").click();
    harness.run();
    assert_eq!(*harness.state(), 1);
    let mut harness = Harness::new_ui(|ui| {
        edit_ui(ui, None);
    });
    harness.run();
    harness.get_by_label("Edit in the editor");
}

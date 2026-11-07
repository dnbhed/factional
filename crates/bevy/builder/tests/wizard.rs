//! The wizard (W8a): an Azgaar export opened and summarised, a map generated from it in the
//! background and drawn, and saved as the builder's map, each in the CLI's words. The export
//! is the raster fixture, whose `world import --scale 2` is the sample's map (W7a).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_bevy::FactionalPlugin;
use factional_builder::{
    BuilderPlugin, Ground, MOST_SCALE, MapChoice, Shape, Suggested, Wizard, Wizarding, marks,
    wizard_ui, write_map_file,
};
use factional_content::write_map;
use factional_session::{Choices, Import, generate, read_export, world_import, world_source};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn raster() -> PathBuf {
    Path::new(REPO).join("crates/content/tests/fixtures/azgaar/raster.json")
}

/// The sample's map file's bytes.
fn riverhold() -> Vec<u8> {
    fs::read(sample().join("world/riverhold.map")).expect("the sample's map")
}

/// A fresh copy of the sample as `<tmp>/<name>`, its maps too.
fn copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&sample(), &dir);
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("a directory");
    for entry in fs::read_dir(from).expect("a directory to copy") {
        let path = entry.expect("an entry").path();
        let into = to.join(path.file_name().expect("a name"));
        match path.is_dir() {
            true => copy_dir(&path, &into),
            false => {
                fs::copy(&path, &into).expect("copied");
            }
        }
    }
}

/// The builder on `content`, a second a frame.
fn app(content: PathBuf) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: content.clone(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content,
            player: "player".to_owned(),
        });
    app.update();
    app.update();
    app
}

/// `asked` of the wizard, then frames until it's no longer generating, at most 100.
fn ask(app: &mut App, asked: Wizarding) {
    app.world_mut().write_message(asked);
    app.update();
    for _ in 0..100 {
        if !app.world().resource::<Wizard>().generating() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
        app.update();
    }
    assert!(
        !app.world().resource::<Wizard>().generating(),
        "still generating"
    );
}

/// The raster export opened and generated at `scale`.
fn generated(app: &mut App, scale: u32) {
    ask(app, Wizarding::Open(raster()));
    app.world_mut().resource_mut::<Wizard>().scale = scale;
    ask(app, Wizarding::Generate);
}

fn wizard(app: &App) -> &Wizard {
    app.world().resource::<Wizard>()
}

#[test]
fn the_wizard_begins_closed_at_world_imports_scale_and_seed() {
    let app = app(copy("w8a-begins"));
    let wizard = wizard(&app);
    assert!(!wizard.open);
    assert_eq!((wizard.scale, wizard.seed), (1, 0));
    assert_eq!(MOST_SCALE, 64);
    assert!(wizard.export().is_none());
    assert!(wizard.summary().is_none());
    assert!(wizard.saves_to().is_none());
}

#[test]
fn an_export_opened_is_said_as_world_source_says_it() {
    let mut app = app(copy("w8a-opened"));
    ask(&mut app, Wizarding::Open(raster()));
    let wizard = wizard(&app);
    let base = raster().parent().expect("a directory").to_owned();
    assert_eq!(wizard.summary(), Some(&world_source(&base, "raster.json")));
    assert!(
        wizard
            .summary()
            .and_then(|said| said.as_ref().ok())
            .is_some_and(|said| said
                .starts_with("raster.json: Raster, from Azgaar 1.154.0, 40 × 40 pixels\n")),
        "{:?}",
        wizard.summary()
    );
    assert_eq!(
        wizard.export().map(|export| export.source.name.as_str()),
        Some("Raster")
    );
}

#[test]
fn a_file_that_isnt_an_export_is_said_why_and_nothing_is_kept() {
    let mut app = app(copy("w8a-not-an-export"));
    ask(&mut app, Wizarding::Open(raster()));
    let characters = sample().join("characters.toml");
    ask(&mut app, Wizarding::Open(characters.clone()));
    let refused = world_source(Path::new(""), &characters.to_string_lossy());
    assert!(refused.is_err());
    assert_eq!(wizard(&app).summary(), Some(&refused));
    assert!(wizard(&app).export().is_none());
    // With nothing open, Generate does nothing: the sample's map is still drawn.
    ask(&mut app, Wizarding::Generate);
    let ground = app.world().resource::<Ground>();
    assert_eq!(write_map(&ground.map), riverhold());
    assert!(wizard(&app).saves_to().is_none());
}

#[test]
fn a_map_generated_is_drawn_in_place_of_the_one_shown_with_its_suggestions() {
    let mut app = app(copy("w8a-generated"));
    generated(&mut app, 1);
    let ground = app.world().resource::<Ground>();
    assert_eq!(
        (ground.map.size().width, ground.map.size().height),
        (40, 40)
    );
    assert_eq!(ground.tile, 32.0);
    // Highmoor, at the source's pixel (30.25, 9.75), at a tile a pixel on a 40 × 40 map:
    // ((30.25 − 20) × 32, (20 − 9.75) × 32).
    let suggested = app.world().resource::<Suggested>();
    assert_eq!(suggested.marks, marks(ground));
    let highmoor = suggested
        .marks
        .iter()
        .find(|mark| mark.id == "settlement_1")
        .expect("Highmoor");
    assert!(
        matches!(highmoor.shape, Shape::Square { at, .. } if at == Vec2::new(328.0, 328.0)),
        "{:?}",
        highmoor.shape
    );
    assert_eq!(wizard(&app).saves_to().as_deref(), Some("world/raster.map"));
    assert_eq!(wizard(&app).note(), None);
}

#[test]
fn a_map_generated_at_a_scale_is_what_world_import_makes_at_it() {
    let mut app = app(copy("w8a-scale"));
    // First a map of a tile a pixel, so the sample's map isn't still the one shown.
    generated(&mut app, 1);
    assert_eq!(app.world().resource::<Ground>().map.size().width, 40);
    app.world_mut().resource_mut::<Wizard>().scale = 2;
    ask(&mut app, Wizarding::Generate);
    let ground = app.world().resource::<Ground>();
    assert_eq!(write_map(&ground.map), riverhold());
}

#[test]
fn a_map_generated_with_a_seed_is_what_the_seed_makes() {
    let mut app = app(copy("w8a-seed"));
    ask(&mut app, Wizarding::Open(raster()));
    {
        let mut wizard = app.world_mut().resource_mut::<Wizard>();
        wizard.scale = 2;
        wizard.seed = 7;
    }
    ask(&mut app, Wizarding::Generate);
    let base = raster().parent().expect("a directory").to_owned();
    let export = read_export(&base, "raster.json").expect("reads");
    let seeded = generate(
        &export,
        &Choices {
            scale: 2,
            seed: 7,
            ..Choices::default()
        },
    )
    .expect("a map");
    let ground = app.world().resource::<Ground>();
    assert_eq!(write_map(&ground.map), write_map(&seeded));
    // The seed moves the detail, so it isn't the sample's map, made at seed 0.
    assert_ne!(write_map(&ground.map), riverhold());
}

#[test]
fn a_map_that_cant_be_made_is_said_and_the_one_shown_kept() {
    let mut app = app(copy("w8a-too-large"));
    // 40 pixels × 256 is 10 240 tiles a side, over the 8192 a map may be.
    generated(&mut app, 256);
    assert!(wizard(&app).note().is_some_and(|note| !note.is_empty()));
    assert_eq!(
        write_map(&app.world().resource::<Ground>().map),
        riverhold()
    );
    assert!(wizard(&app).saves_to().is_none());
}

#[test]
fn saving_writes_the_map_says_what_world_import_says_and_sets_it_as_the_builders() {
    let dir = copy("w8a-saved");
    fs::remove_dir_all(dir.join("world")).expect("no world/ to begin with");
    let mut app = app(dir.clone());
    generated(&mut app, 2);
    ask(&mut app, Wizarding::Save);
    assert_eq!(
        fs::read(dir.join("world/raster.map")).expect("saved"),
        riverhold()
    );
    // What world import says of the same map.
    let imported = Path::new(env!("CARGO_TARGET_TMPDIR")).join("w8a-saved-imported");
    let _ = fs::remove_dir_all(&imported);
    fs::create_dir_all(imported.join("world")).expect("a directory");
    fs::copy(raster(), imported.join("raster.json")).expect("copied");
    let said = world_import(
        &imported,
        &Import {
            export: "raster.json",
            map: "world/raster.map",
            region: None,
            scale: 2,
            seed: 0,
            detail: Default::default(),
            coast: Default::default(),
            levels: Default::default(),
            foliage: Default::default(),
        },
    )
    .expect("imported");
    assert!(
        said.starts_with("imported Raster into raster.map: 80 × 80 tiles"),
        "{said}"
    );
    assert_eq!(wizard(&app).note(), Some(said.as_str()));
    // builder.toml's map is the one saved, its tile and positions kept.
    let text = fs::read_to_string(dir.join("builder.toml")).expect("builder.toml");
    assert!(
        text.contains("[map]\nfile = \"world/raster.map\"\ntile = 32.0"),
        "{text}"
    );
    assert!(text.contains("player = { x = 0.0, y = 0.0 }"), "{text}");
    assert_eq!(
        app.world().resource::<MapChoice>().file.as_deref(),
        Some("world/raster.map")
    );
    // Saved, there's nothing more to save.
    assert!(wizard(&app).saves_to().is_none());
}

#[test]
fn saving_over_a_file_asks_first() {
    let dir = copy("w8a-overwrite");
    fs::write(dir.join("world/raster.map"), b"old").expect("written");
    let mut app = app(dir.clone());
    generated(&mut app, 2);
    ask(&mut app, Wizarding::Save);
    assert_eq!(wizard(&app).confirming(), Some("world/raster.map"));
    assert_eq!(
        fs::read(dir.join("world/raster.map")).expect("there"),
        b"old"
    );
    ask(&mut app, Wizarding::KeepIt);
    assert_eq!(wizard(&app).confirming(), None);
    assert_eq!(
        fs::read(dir.join("world/raster.map")).expect("there"),
        b"old"
    );
    assert_eq!(wizard(&app).saves_to().as_deref(), Some("world/raster.map"));
    ask(&mut app, Wizarding::Save);
    ask(&mut app, Wizarding::Overwrite);
    assert_eq!(wizard(&app).confirming(), None);
    assert_eq!(
        fs::read(dir.join("world/raster.map")).expect("there"),
        riverhold()
    );
}

#[test]
fn the_map_file_is_set_in_builder_toml_with_everything_else_kept() {
    let text = "# Where they stand.\n[positions]\nplayer = { x = 0.0, y = 0.0 }\n\n\
                # The map.\n[map]\nfile = \"world/riverhold.map\" # made by world import\ntile = 16.0\n";
    assert_eq!(
        write_map_file(Some(text), "world/raster.map"),
        Ok(
            "# Where they stand.\n[positions]\nplayer = { x = 0.0, y = 0.0 }\n\n\
            # The map.\n[map]\nfile = \"world/raster.map\" # made by world import\ntile = 16.0\n"
                .to_owned()
        )
    );
    assert_eq!(
        write_map_file(Some("[positions]\n"), "world/raster.map"),
        Ok("[positions]\n\n[map]\nfile = \"world/raster.map\"\n".to_owned())
    );
    assert_eq!(
        write_map_file(None, "world/raster.map"),
        Ok("[map]\nfile = \"world/raster.map\"\n".to_owned())
    );
    let refused = write_map_file(Some("[map]\nfile = 3\n"), "world/raster.map")
        .expect_err("not a builder.toml");
    assert!(
        refused.starts_with("builder.toml can't be read, so the map isn't set: "),
        "{refused}"
    );
}

#[test]
fn the_wizards_window_asks_to_browse_generate_and_save() {
    let mut app = app(copy("w8a-window"));
    let mut harness = Harness::new_ui_state(
        |ui, (wizard, asked): &mut (Wizard, Option<Wizarding>)| {
            if let Some(now) = wizard_ui(ui, wizard) {
                *asked = Some(now);
            }
        },
        (Wizard::default(), None),
    );
    harness.run();
    assert!(harness.query_by_label("Generate").is_none(), "nothing open");
    harness.get_by_label("Open…").click();
    harness.run();
    assert_eq!(harness.state().1, Some(Wizarding::Browse));
    // Opened and generated in the app, the wizard is shown as it is.
    generated(&mut app, 2);
    let shown = std::mem::take(&mut *app.world_mut().resource_mut::<Wizard>());
    let mut harness = Harness::new_ui_state(
        |ui, (wizard, asked): &mut (Wizard, Option<Wizarding>)| {
            if let Some(now) = wizard_ui(ui, wizard) {
                *asked = Some(now);
            }
        },
        (shown, None),
    );
    harness.run();
    assert!(
        harness
            .query_by_label_contains("raster.json: Raster, from Azgaar 1.154.0")
            .is_some()
    );
    harness.get_by_label("Generate").click();
    harness.run();
    assert_eq!(harness.state().1, Some(Wizarding::Generate));
    harness.get_by_label("Save as world/raster.map").click();
    harness.run();
    assert_eq!(harness.state().1, Some(Wizarding::Save));
}

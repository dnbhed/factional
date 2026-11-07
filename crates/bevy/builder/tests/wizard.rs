//! The wizard (W8a): an Azgaar export opened and summarised, a map generated from it in the
//! background and drawn, and saved as the builder's map, each in the CLI's words. The export
//! is the raster fixture, whose `world import --scale 2` is the sample's map (W7a). And a region
//! of it chosen on a preview (W8b): dragged out, or set in numbers, and what it makes said.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use bevy_egui::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_bevy::FactionalPlugin;
use factional_builder::{
    BuilderPlugin, Ground, MOST_SCALE, MapChoice, PREVIEW_SIDE, Shape, Suggested, Wizard,
    Wizarding, colour, marks, region_between, shade_colour, wizard_ui, write_map_file,
};
use factional_content::write_map;
use factional_session::{
    Choices, Import, generate, map_size, read_export, world_import, world_source,
};
use factional_worldgen::{Region, Shade, Size, Terrain, preview};

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

/// `asked` of the wizard, then frames until it's no longer generating or previewing, at most 100.
fn ask(app: &mut App, asked: Wizarding) {
    app.world_mut().write_message(asked);
    app.update();
    let busy = |app: &App| {
        let wizard = app.world().resource::<Wizard>();
        wizard.generating() || wizard.previewing()
    };
    for _ in 0..100 {
        if !busy(app) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
        app.update();
    }
    assert!(!busy(app), "still generating or previewing");
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

fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
    Region {
        x,
        y,
        size: Size { width, height },
    }
}

fn forty() -> Size {
    Size {
        width: 40,
        height: 40,
    }
}

/// The wizard's window, on the wizard taken from `app`, and what it last asked.
fn window(app: &mut App) -> Harness<'static, (Wizard, Option<Wizarding>)> {
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
    harness
}

#[test]
fn an_export_opened_is_previewed_in_the_background_with_no_region_chosen() {
    let mut app = app(copy("w8b-previewed"));
    assert!(wizard(&app).preview().is_none());
    ask(&mut app, Wizarding::Open(raster()));
    let base = raster().parent().expect("a directory").to_owned();
    let export = read_export(&base, "raster.json").expect("reads");
    assert_eq!(PREVIEW_SIDE, 320);
    let expected = preview(&export.source, PREVIEW_SIDE).expect("a preview");
    assert_eq!(expected.size, forty());
    assert_eq!(wizard(&app).preview(), Some(&expected));
    assert_eq!(wizard(&app).region, None);
}

#[test]
fn opening_another_export_clears_the_region_and_a_file_thats_not_one_the_preview() {
    let mut app = app(copy("w8b-cleared"));
    ask(&mut app, Wizarding::Open(raster()));
    app.world_mut().resource_mut::<Wizard>().region = Some(region(10, 10, 20, 10));
    ask(&mut app, Wizarding::Open(raster()));
    assert_eq!(wizard(&app).region, None);
    assert!(wizard(&app).preview().is_some());
    ask(&mut app, Wizarding::Open(sample().join("builder.toml")));
    assert!(wizard(&app).preview().is_none());
}

#[test]
fn the_wizard_says_what_its_choices_make() {
    let mut app = app(copy("w8b-makes"));
    assert_eq!(wizard(&app).makes(), None);
    ask(&mut app, Wizarding::Open(raster()));
    assert_eq!(wizard(&app).makes(), Some(Ok(forty())));
    {
        let mut wizard = app.world_mut().resource_mut::<Wizard>();
        wizard.region = Some(region(10, 10, 20, 10));
        wizard.scale = 2;
        wizard.seed = 7;
    }
    let base = raster().parent().expect("a directory").to_owned();
    let export = read_export(&base, "raster.json").expect("reads");
    let choices = Choices {
        region: Some(region(10, 10, 20, 10)),
        scale: 2,
        seed: 7,
        ..Choices::default()
    };
    assert_eq!(wizard(&app).makes(), Some(map_size(&export, &choices)));
    assert_eq!(
        wizard(&app).makes(),
        Some(Ok(Size {
            width: 40,
            height: 20
        }))
    );
    let harness = window(&mut app);
    assert!(harness.query_by_label("Makes 40 × 20 tiles").is_some());
    assert!(
        !harness
            .get_by_label("Generate")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_region_that_cant_be_made_says_why_and_generate_is_off() {
    let mut app = app(copy("w8b-off"));
    ask(&mut app, Wizarding::Open(raster()));
    app.world_mut().resource_mut::<Wizard>().region = Some(region(30, 0, 20, 10));
    let said = "the region runs off the source map: pixels (30, 0) to (49, 9), and the map is \
                40 × 40 pixels";
    assert_eq!(wizard(&app).makes(), Some(Err(said.to_owned())));
    let harness = window(&mut app);
    assert!(harness.query_by_label(said).is_some());
    assert!(harness.query_by_label_contains("Makes").is_none());
    assert!(
        harness
            .get_by_label("Generate")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn generating_makes_the_region_chosen() {
    let mut app = app(copy("w8b-region"));
    ask(&mut app, Wizarding::Open(raster()));
    app.world_mut().resource_mut::<Wizard>().region = Some(region(10, 10, 20, 10));
    ask(&mut app, Wizarding::Generate);
    let base = raster().parent().expect("a directory").to_owned();
    let export = read_export(&base, "raster.json").expect("reads");
    let made = generate(
        &export,
        &Choices {
            region: Some(region(10, 10, 20, 10)),
            ..Choices::default()
        },
    )
    .expect("a map");
    let ground = app.world().resource::<Ground>();
    assert_eq!(
        ground.map.size(),
        Size {
            width: 20,
            height: 10
        }
    );
    assert_eq!(write_map(&ground.map), write_map(&made));
}

#[test]
fn a_region_dragged_takes_each_corner_outward_to_a_whole_pixel_held_to_the_source() {
    assert_eq!(
        region_between([10.2, 5.7], [20.1, 15.0], forty()),
        region(10, 5, 11, 10)
    );
    assert_eq!(
        region_between([20.1, 15.0], [10.2, 5.7], forty()),
        region(10, 5, 11, 10)
    );
    assert_eq!(
        region_between([20.1, 5.7], [10.2, 15.0], forty()),
        region(10, 5, 11, 10)
    );
    assert_eq!(
        region_between([-5.0, -5.0], [50.0, 50.0], forty()),
        region(0, 0, 40, 40)
    );
    assert_eq!(
        region_between([10.5, 10.5], [10.6, 10.7], forty()),
        region(10, 10, 1, 1)
    );
    // Wholly past an edge, it's the last pixel there.
    assert_eq!(
        region_between([45.0, 50.0], [48.0, 60.0], forty()),
        region(39, 39, 1, 1)
    );
    assert_eq!(
        region_between([-9.0, -3.0], [-1.0, -2.0], forty()),
        region(0, 0, 1, 1)
    );
}

#[test]
fn dragging_on_the_preview_chooses_the_region_and_whole_map_clears_it() {
    let mut app = app(copy("w8b-dragged"));
    ask(&mut app, Wizarding::Open(raster()));
    let mut harness = window(&mut app);
    assert!(
        harness
            .get_by_label("Whole map")
            .accesskit_node()
            .is_disabled()
    );
    // The preview is shown 320 points on its longer side, 8 a pixel of the fixture.
    let shown = harness.get_by_label("Preview").rect();
    assert_eq!(shown.size(), egui::vec2(320.0, 320.0));
    let at = |x: f32, y: f32| shown.min + egui::vec2(x * 8.0, y * 8.0);
    harness.hover_at(at(10.5, 5.5));
    harness.run();
    harness.drag_at(at(10.5, 5.5));
    harness.run();
    harness.hover_at(at(15.0, 10.0));
    harness.run();
    harness.hover_at(at(20.5, 15.5));
    harness.run();
    harness.drop_at(at(20.5, 15.5));
    harness.run();
    assert_eq!(harness.state().0.region, Some(region(10, 5, 11, 11)));
    assert!(harness.query_by_label("Makes 11 × 11 tiles").is_some());
    harness.get_by_label("Whole map").click();
    harness.run();
    assert_eq!(harness.state().0.region, None);
    assert!(harness.query_by_label("Makes 40 × 40 tiles").is_some());
}

#[test]
fn the_region_is_shown_in_numbers_the_whole_map_with_none_chosen() {
    let mut app = app(copy("w8b-numbers"));
    ask(&mut app, Wizarding::Open(raster()));
    app.world_mut().resource_mut::<Wizard>().region = Some(region(10, 5, 11, 10));
    let harness = window(&mut app);
    let value = |label: &str| harness.get_by_label(label).accesskit_node().numeric_value();
    assert_eq!(value("Region x"), Some(10.0));
    assert_eq!(value("Region y"), Some(5.0));
    assert_eq!(value("Region width"), Some(11.0));
    assert_eq!(value("Region height"), Some(10.0));
    drop(harness);
    let mut app = self::app(copy("w8b-numbers-whole"));
    ask(&mut app, Wizarding::Open(raster()));
    let harness = window(&mut app);
    let value = |label: &str| harness.get_by_label(label).accesskit_node().numeric_value();
    assert_eq!(value("Region x"), Some(0.0));
    assert_eq!(value("Region width"), Some(40.0));
    assert_eq!(value("Region height"), Some(40.0));
}

#[test]
fn the_preview_shades_water_by_depth_and_land_by_height_in_the_maps_colours() {
    assert_eq!(shade_colour(Shade::Water(0)), colour(Terrain::DeepWater));
    assert_eq!(
        shade_colour(Shade::Water(19)),
        colour(Terrain::ShallowWater)
    );
    // A lake above sea level is as shallow as water gets.
    assert_eq!(
        shade_colour(Shade::Water(25)),
        colour(Terrain::ShallowWater)
    );
    // 9 of the 19 steps from deep, (32, 64, 128), to shallow, (64, 128, 192), each rounded.
    assert_eq!(shade_colour(Shade::Water(9)), [47, 94, 158]);
    // 10 of the 19: 16.84, 33.68 and 33.68 along, rounded up.
    assert_eq!(shade_colour(Shade::Water(10)), [49, 98, 162]);
    // 3 of the 40 from grass, (112, 168, 80), to rock, (128, 120, 112): 1.2, −3.6 and 2.4
    // along, the fall rounded away from zero.
    assert_eq!(shade_colour(Shade::Land(23)), [113, 164, 82]);
    assert_eq!(shade_colour(Shade::Land(20)), colour(Terrain::Grass));
    assert_eq!(shade_colour(Shade::Land(40)), [120, 144, 96]);
    assert_eq!(shade_colour(Shade::Land(60)), colour(Terrain::Rock));
    assert_eq!(shade_colour(Shade::Land(80)), [182, 180, 178]);
    assert_eq!(shade_colour(Shade::Land(100)), colour(Terrain::Snow));
}

/// `text` typed into the number labelled `label`, in place of what's there.
fn type_into(harness: &mut Harness<'static, (Wizard, Option<Wizarding>)>, label: &str, text: &str) {
    harness.get_by_label(label).click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label(label).type_text(text);
    harness.key_press(egui::Key::Enter);
    harness.run();
}

#[test]
fn a_number_typed_sets_the_region_held_to_the_source() {
    let mut app = app(copy("w8b-typed"));
    ask(&mut app, Wizarding::Open(raster()));
    let mut harness = window(&mut app);
    // From the whole map, x moved to 30 leaves only 10 pixels of width.
    type_into(&mut harness, "Region x", "30");
    assert_eq!(harness.state().0.region, Some(region(30, 0, 10, 40)));
    // At most what's left, and at least a pixel.
    type_into(&mut harness, "Region height", "99");
    assert_eq!(harness.state().0.region, Some(region(30, 0, 10, 40)));
    type_into(&mut harness, "Region width", "0");
    assert_eq!(harness.state().0.region, Some(region(30, 0, 1, 40)));
    type_into(&mut harness, "Region y", "39");
    assert_eq!(harness.state().0.region, Some(region(30, 39, 1, 1)));
}

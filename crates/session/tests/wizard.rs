//! What the builder's wizard shares with `world source` and `world import` (W8a): an export
//! read and summarised, a map generated from it, and the map saved, each in the CLI's words.

use std::path::{Path, PathBuf};

use factional_content::write_map;
use factional_session::{
    Choices, Import, export_summary, generate, map_file_named, read_export, save_map, world_import,
    world_source,
};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn fixtures() -> PathBuf {
    Path::new(REPO).join("crates/content/tests/fixtures/azgaar")
}

/// A scratch directory of its own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("factional-w8a-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

#[test]
fn an_exports_summary_is_what_world_source_says_of_it() {
    let export = read_export(&fixtures(), "raster.json").expect("raster.json reads");
    let summary = export_summary(&export);
    assert!(
        summary.starts_with("raster.json: Raster, from Azgaar 1.154.0, 40 × 40 pixels\n"),
        "{summary}"
    );
    assert!(
        summary.ends_with("; fingerprint fnv1a64:dbaa33cac888a6e3"),
        "{summary}"
    );
    assert_eq!(Ok(summary), world_source(&fixtures(), "raster.json"));
}

#[test]
fn a_file_that_isnt_an_export_is_refused_as_world_source_refuses_it() {
    let base = Path::new(REPO).join("content/sample");
    let refused = read_export(&base, "characters.toml").expect_err("not an export");
    assert!(!refused.is_empty());
    assert_eq!(Err(refused), world_source(&base, "characters.toml"));
    let missing = read_export(&base, "nope.json").expect_err("not there");
    assert!(missing.starts_with("cannot read nope.json: "), "{missing}");
}

#[test]
fn choices_begin_as_world_imports_defaults() {
    // The rest of world import's defaults are checked by generating: the sample's map was
    // imported with no options but the scale.
    let choices = Choices::default();
    assert_eq!((choices.region, choices.scale, choices.seed), (None, 1, 0));
}

#[test]
fn generating_at_a_scale_makes_what_world_import_makes_at_it() {
    let export = read_export(&fixtures(), "raster.json").expect("raster.json reads");
    let whole = generate(&export, &Choices::default()).expect("a map");
    assert_eq!((whole.size().width, whole.size().height), (40, 40));
    let twice = generate(
        &export,
        &Choices {
            scale: 2,
            ..Choices::default()
        },
    )
    .expect("a map");
    // The sample's map is `world import --scale 2` of raster.json (W7a).
    let sample = std::fs::read(Path::new(REPO).join("content/sample/world/riverhold.map"))
        .expect("the sample's map");
    assert_eq!(write_map(&twice), sample);
}

#[test]
fn generating_too_large_a_map_says_why() {
    let export = read_export(&fixtures(), "raster.json").expect("raster.json reads");
    // 40 pixels × 256 is 10 240 tiles a side, over the 8192 a map may be.
    let refused = generate(
        &export,
        &Choices {
            scale: 256,
            ..Choices::default()
        },
    )
    .expect_err("too large");
    assert!(!refused.is_empty());
}

#[test]
fn saving_writes_and_says_what_world_import_writes_and_says() {
    let export = read_export(&fixtures(), "raster.json").expect("raster.json reads");
    let map = generate(
        &export,
        &Choices {
            scale: 2,
            ..Choices::default()
        },
    )
    .expect("a map");
    let saved = scratch("saved");
    let said = save_map(&saved, "raster.map", &map, &export.source.name).expect("saved");
    let imported = scratch("imported");
    std::fs::copy(fixtures().join("raster.json"), imported.join("raster.json")).expect("copied");
    let import = Import {
        export: "raster.json",
        map: "raster.map",
        region: None,
        scale: 2,
        seed: 0,
        detail: Default::default(),
        coast: Default::default(),
        levels: Default::default(),
        foliage: Default::default(),
    };
    assert_eq!(Ok(said.clone()), world_import(&imported, &import));
    assert!(
        said.starts_with("imported Raster into raster.map: 80 × 80 tiles in 2 × 2 chunks"),
        "{said}"
    );
    assert_eq!(
        std::fs::read(saved.join("raster.map")).expect("written"),
        std::fs::read(imported.join("raster.map")).expect("written")
    );
}

#[test]
fn a_map_that_cant_be_written_says_why() {
    let export = read_export(&fixtures(), "raster.json").expect("raster.json reads");
    let map = generate(&export, &Choices::default()).expect("a map");
    let base = scratch("unwritable");
    let refused = save_map(&base, "no/such/dir/raster.map", &map, "Raster").expect_err("no dir");
    assert!(
        refused.starts_with("cannot write no/such/dir/raster.map: "),
        "{refused}"
    );
}

#[test]
fn a_maps_file_is_named_after_its_exports_map() {
    assert_eq!(map_file_named("Raster"), "world/raster.map");
    assert_eq!(map_file_named("Menayland Full"), "world/menayland-full.map");
    assert_eq!(map_file_named("  Ash & Ember! "), "world/ash-ember.map");
    assert_eq!(
        map_file_named("Isle_of 2 Moons"),
        "world/isle-of-2-moons.map"
    );
    assert_eq!(map_file_named("—!?"), "world/map.map");
    assert_eq!(map_file_named(""), "world/map.map");
}

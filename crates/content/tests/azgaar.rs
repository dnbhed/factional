//! Reading Azgaar's map (W2a, P-134): a Full JSON export read into a `Source`, its version
//! checked, every problem named by key path. The fixtures are hand-made in Azgaar's shape, at
//! the user's choice; the user's own export is checked only locally.

use factional_content::{AzgaarError, fingerprint_of, read_azgaar};
use factional_core::Fixed;
use factional_worldgen::{
    Feature, Point, Recipe, Region, Size, Source, SourceCell, SourceVertex, Terrain, Water,
    rasterise,
};
use serde_json::{Value, json};

const TINY: &str = include_str!("fixtures/azgaar/tiny.json");

fn point(x: i64, y: i64) -> Point {
    Point {
        x: Fixed::from_hundredths(x),
        y: Fixed::from_hundredths(y),
    }
}

fn cell(
    centre: Point,
    height: u8,
    feature: u32,
    neighbours: &[u32],
    corners: &[u32],
) -> SourceCell {
    SourceCell {
        centre,
        height,
        feature,
        neighbours: neighbours.to_vec(),
        corners: corners.to_vec(),
    }
}

fn vertex(at: Point, cells: [Option<u32>; 3]) -> SourceVertex {
    SourceVertex { at, cells }
}

/// The tiny export with `change` made to it.
fn tiny_with(change: impl FnOnce(&mut Value)) -> String {
    let mut json: Value = serde_json::from_str(TINY).expect("the fixture is JSON");
    change(&mut json);
    json.to_string()
}

/// What reading `text` reports, each problem on a line.
fn problems(text: &str) -> Vec<String> {
    match read_azgaar(text) {
        Err(AzgaarError::Problems(problems)) => problems.iter().map(ToString::to_string).collect(),
        other => panic!("expected problems, got {other:?}"),
    }
}

fn refusal(text: &str) -> String {
    read_azgaar(text).expect_err("refused").to_string()
}

#[test]
fn a_full_export_reads_as_its_cells_vertices_and_features() {
    let expected = Source {
        name: "Tiny".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size {
            width: 40,
            height: 30,
        },
        height_exponent: Fixed::from_hundredths(2_00),
        cells: vec![
            cell(point(10_00, 7_50), 10, 0, &[1, 2], &[0, 1, 3]),
            cell(point(30_00, 7_50), 40, 2, &[0, 3], &[0, 2, 3]),
            cell(point(10_00, 22_50), 19, 1, &[0, 3], &[0, 1, 4]),
            cell(point(111_67, 22_50), 60, 2, &[1, 2], &[0, 2, 4, 5]),
        ],
        vertices: vec![
            vertex(point(20_00, 15_00), [Some(0), Some(1), Some(3)]),
            vertex(point(0, 15_00), [Some(0), Some(2), None]),
            vertex(point(40_00, 15_00), [Some(1), Some(3), None]),
            vertex(point(20_00, 0), [Some(0), Some(1), None]),
            vertex(point(20_00, 30_00), [Some(2), Some(3), None]),
            vertex(point(-9_60, 13), [Some(3), None, None]),
        ],
        features: vec![
            Feature::Ocean,
            Feature::Lake {
                surface: Fixed::from_hundredths(19_90),
            },
            Feature::Island,
        ],
    };
    assert_eq!(read_azgaar(TINY), Ok(expected));
}

#[test]
fn decimals_round_once_to_hundredths_half_away_from_zero() {
    let text = tiny_with(|json| {
        json["pack"]["cells"][0]["p"] = json!([0.125, -0.125]);
        json["pack"]["cells"][1]["p"] = json!([2.004999, -2.005]);
        json["pack"]["cells"][2]["p"] = json!([1e-7, 3]);
    });
    let source = read_azgaar(&text).expect("it reads");
    assert_eq!(source.cells[0].centre, point(13, -13));
    assert_eq!(source.cells[1].centre, point(2_00, -2_01));
    assert_eq!(source.cells[2].centre, point(0, 3_00));
}

#[test]
fn every_problem_is_reported_at_once_by_key_path() {
    let text = tiny_with(|json| {
        let cells = &mut json["pack"]["cells"];
        cells[0]["h"] = json!(101);
        cells[1]["v"] = json!([0, 40, 3]);
        cells[2]["c"] = json!([0, 9]);
        cells[3]["f"] = json!(7);
    });
    assert_eq!(
        problems(&text),
        [
            "pack.cells[0].h: 101 is outside 0 to 100",
            "pack.cells[1].v: corner 40 doesn't exist; there are 6 vertices",
            "pack.cells[2].c: neighbour 9 doesn't exist; there are 4 cells",
            "pack.cells[3].f: feature 7 doesn't exist; there are 3 features",
        ]
    );
}

#[test]
fn missing_and_misshapen_values_are_named() {
    let text = tiny_with(|json| {
        let cells = &mut json["pack"]["cells"];
        cells[0].as_object_mut().expect("a cell").remove("h");
        cells[1]["p"] = json!([1]);
        cells[2]["f"] = json!("two");
        cells[3]["c"] = json!([1, -2]);
        json["pack"]["vertices"][4]["c"] = json!([2, 3]);
        json["pack"]["vertices"][5]["p"] = json!("here");
    });
    assert_eq!(
        problems(&text),
        [
            "pack.cells[0]: missing h",
            "pack.cells[1].p: expected two numbers, an x and a y",
            "pack.cells[2].f: expected a whole number, not \"two\"",
            "pack.cells[3].c: expected whole numbers, not -2",
            "pack.vertices[4].c: expected three cells",
            "pack.vertices[5].p: expected two numbers, an x and a y",
        ]
    );
}

#[test]
fn a_feature_factional_doesnt_read_or_a_lake_without_its_surface_is_named() {
    let text = tiny_with(|json| {
        json["pack"]["features"][3]["type"] = json!("glacier");
        json["pack"]["features"][2]
            .as_object_mut()
            .expect("a feature")
            .remove("height");
    });
    assert_eq!(
        problems(&text),
        [
            "pack.features[2]: missing height, a lake's surface",
            "pack.features[3].type: \"glacier\" isn't a feature Factional reads: ocean, lake or island",
        ]
    );
}

#[test]
fn the_height_exponent_is_read_to_the_hundredth() {
    let text = tiny_with(|json| json["settings"]["heightExponent"] = json!(1.785));
    let source = read_azgaar(&text).expect("it reads");
    assert_eq!(source.height_exponent, Fixed::from_hundredths(1_79));
}

#[test]
fn a_height_exponent_azgaar_doesnt_allow_is_a_problem() {
    for (exponent, said) in [(json!(2.5), "2.50"), (json!(1.49), "1.49")] {
        let text = tiny_with(|json| json["settings"]["heightExponent"] = exponent);
        assert_eq!(
            problems(&text),
            [format!(
                "settings.heightExponent: {said} is outside 1.50 to 2.20, as Azgaar allows"
            )]
        );
    }
    for edge in [json!(1.5), json!(2.2)] {
        let text = tiny_with(|json| json["settings"]["heightExponent"] = edge);
        assert!(read_azgaar(&text).is_ok());
    }
}

#[test]
fn a_missing_or_misshapen_height_exponent_is_named() {
    let text = tiny_with(|json| json["settings"]["heightExponent"] = json!("steep"));
    assert_eq!(
        problems(&text),
        ["settings.heightExponent: expected a number, not \"steep\""]
    );
    let text = tiny_with(|json| {
        json.as_object_mut().expect("an export").remove("settings");
    });
    assert_eq!(problems(&text), ["settings: missing heightExponent"]);
}

#[test]
fn text_that_isnt_json_is_refused() {
    let refused = refusal("{ \"info\": ");
    assert!(refused.starts_with("this isn't JSON: "), "{refused}");
}

#[test]
fn json_without_a_version_isnt_an_azgaar_map() {
    assert_eq!(
        refusal("{\"name\": \"The Watch\"}"),
        "this isn't an Azgaar map: it has no info.version"
    );
    assert_eq!(
        refusal("[1, 2]"),
        "this isn't an Azgaar map: it has no info.version"
    );
    assert_eq!(read_azgaar("{}"), Err(AzgaarError::NotAzgaar));
}

#[test]
fn another_version_is_refused_naming_both() {
    let text = tiny_with(|json| json["info"]["version"] = json!("1.100.0"));
    assert_eq!(
        refusal(&text),
        "info.version: this map is from Azgaar 1.100.0, and Factional reads 1.154"
    );
    let text = tiny_with(|json| json["info"]["version"] = json!("1.155.2"));
    assert_eq!(
        refusal(&text),
        "info.version: this map is from Azgaar 1.155.2, and Factional reads 1.154"
    );
    let text = tiny_with(|json| json["info"]["version"] = json!("1.1545.0"));
    assert_eq!(
        refusal(&text),
        "info.version: this map is from Azgaar 1.1545.0, and Factional reads 1.154"
    );
    let later = tiny_with(|json| json["info"]["version"] = json!("1.154.12"));
    assert!(read_azgaar(&later).is_ok());
}

#[test]
fn an_export_that_isnt_full_is_refused_naming_what_it_lacks() {
    for lacking in ["cells", "vertices", "features"] {
        let text = tiny_with(|json| {
            json["pack"].as_object_mut().expect("pack").remove(lacking);
        });
        assert_eq!(
            refusal(&text),
            format!(
                "this isn't a Full export: it has no pack.{lacking}; export the Full JSON from Azgaar"
            )
        );
    }
}

#[test]
fn the_map_needs_its_name_and_size() {
    let text = tiny_with(|json| {
        json["info"]
            .as_object_mut()
            .expect("info")
            .remove("mapName");
        json["info"]["width"] = json!(0);
    });
    assert_eq!(
        problems(&text),
        [
            "info: missing mapName",
            "info.width: expected at least 1 pixel, not 0",
        ]
    );
}

/// The user's own export, read when `FACTIONAL_AZGAAR` names it; never committed. Its counts
/// were taken with a separate script.
#[test]
#[ignore = "reads the user's own export, named by FACTIONAL_AZGAAR"]
fn the_users_menayland_export_reads() {
    let path = std::env::var("FACTIONAL_AZGAAR").expect("FACTIONAL_AZGAAR names the export");
    let text = std::fs::read_to_string(path).expect("the export is readable");
    let source = read_azgaar(&text).expect("Menayland reads");
    assert_eq!(source.name, "Menayland");
    assert_eq!(
        source.size,
        Size {
            width: 1470,
            height: 801
        }
    );
    assert_eq!(source.cells.len(), 5717);
    assert_eq!(source.vertices.len(), 11642);
    let count = |kind: fn(&Feature) -> bool| source.features.iter().filter(|f| kind(f)).count();
    assert_eq!(count(|f| *f == Feature::Ocean), 2);
    assert_eq!(count(|f| matches!(f, Feature::Lake { .. })), 9);
    assert_eq!(count(|f| *f == Feature::Island), 15);
}

/// The user's own export rasterised whole at a tile a pixel, when `FACTIONAL_AZGAAR` names it;
/// its tile counts were taken with a separate script.
#[test]
#[ignore = "reads the user's own export, named by FACTIONAL_AZGAAR"]
fn the_users_menayland_export_rasterises_whole() {
    let path = std::env::var("FACTIONAL_AZGAAR").expect("FACTIONAL_AZGAAR names the export");
    let text = std::fs::read_to_string(path).expect("the export is readable");
    let source = read_azgaar(&text).expect("Menayland reads");
    let recipe = Recipe {
        source: fingerprint_of(text.as_bytes()),
        region: Region {
            x: 0,
            y: 0,
            size: source.size,
        },
        scale: 1,
        seed: 0,
    };
    let map = rasterise(&source, recipe).expect("Menayland rasterises");
    let mut counts = [0; 3];
    for (_, chunk) in map.chunks() {
        let size = chunk.size();
        for (x, y) in (0..size.height).flat_map(|y| (0..size.width).map(move |x| (x, y))) {
            let tile = chunk.tile(x, y).expect("in the chunk");
            counts[match (tile.terrain, tile.water) {
                (Terrain::DeepWater, _) => 0,
                (_, Some(Water::Lake)) => 1,
                _ => 2,
            }] += 1;
        }
    }
    assert_eq!(counts, [772_521, 8_931, 396_018]);
}

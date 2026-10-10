//! The Previews tab (U5), driven headless through AccessKit: the disposition matrix and a
//! faction's alignment map, from the last content that loaded (D-35).

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::egui::accesskit::Toggled;
use factional_editor::egui::{Color32, Shape, vec2};
use factional_editor::{Editor, WINDOW};
use factional_reputation::FactionId;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn previews_tab(dir: PathBuf) -> Harness<'static, Editor> {
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    // Tall enough to hold every preview, since egui ignores clicks outside what's shown.
    harness.set_size(factional_editor::egui::vec2(1440.0, 3000.0));
    harness.run();
    harness.get_by_label("Previews").click();
    harness.run();
    harness
}

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

#[test]
fn the_matrix_shows_every_observer_toward_every_character() {
    let harness = previews_tab(sample());
    harness.get_by_label("observer");
    // Hale toward the player, worked by hand in DESIGN.md §8's terms.
    harness.get_by_label("captain_hale → player: -5.49, neutral");
    harness.get_by_label("city_watch → captain_hale: 100.00, friendly");
    assert!(
        harness
            .query_by_label_contains("captain_hale → captain_hale")
            .is_none()
    );
}

#[test]
fn choosing_a_faction_shows_its_map_with_its_key() {
    let mut harness = previews_tab(sample());
    harness.get_by_label("The City Watch").click();
    harness.run();
    harness.get_by_label("The City Watch (city_watch): law 70.00, good 20.00, tolerance 40.00");
    harness.get_by_label("B captain_hale: 5.59 away, within");
    harness.get_by_label("F vex: 125.40 away, outside");
    let on = |label: &str| harness.get_by_label(label).accesskit_node().toggled();
    assert_eq!(on("The City Watch"), Some(Toggled::True));
    assert_eq!(on("The Ashen Circle"), Some(Toggled::False));
    // No two marks share a cell in the sample.
    assert!(harness.query_by_label_contains("* at law").is_none());
}

#[test]
fn previews_stay_while_the_content_doesnt_load_marked_as_the_last_that_did() {
    let mut harness = previews_tab(sample());
    {
        let editor = harness.state_mut();
        let factions = 1;
        editor.select(factions, 1);
        let tolerance = editor
            .fields()
            .iter()
            .position(|f| f.row.path.to_string() == "city_watch.tolerance")
            .expect("the Watch's tolerance");
        editor.set(tolerance, "140.0").expect("a number");
    }
    harness.run();
    harness.get_by_label_contains("From the last version that loaded: it has 1 problem now");
    harness.get_by_label("captain_hale → player: -5.49, neutral");
}

#[test]
fn previews_need_a_world_that_has_loaded() {
    let harness = previews_tab(Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken"));
    harness.get_by_label_contains("Previews need a world that loads: it has 3 problems");
    assert!(harness.query_by_label("observer").is_none());
}

/// Everything painted in the last frame, with groups opened up.
fn painted(harness: &Harness<'static, Editor>) -> Vec<Shape> {
    fn open(shape: &Shape, into: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| open(shape, into)),
            shape => into.push(shape.clone()),
        }
    }
    let mut shapes = Vec::new();
    for clipped in &harness.output().shapes {
        open(&clipped.shape, &mut shapes);
    }
    shapes
}

#[test]
fn the_map_is_painted_cell_by_cell_with_its_marks() {
    let mut harness = previews_tab(sample());
    harness.get_by_label("The City Watch").click();
    harness.run();
    let visuals = harness.ctx.global_style().visuals.clone();
    let shapes = painted(&harness);
    let cells = |fill: Color32| {
        shapes
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect(rect)
                    if rect.fill == fill && rect.rect.size() == vec2(15.0, 15.0))
            })
            .count()
    };
    // The cells the engine says are within the Watch's tolerance, filled; the rest faint.
    let watch = FactionId::new("city_watch").expect("valid id");
    let within = harness
        .state()
        .preview()
        .expect("the sample loads")
        .alignment_map(&watch)
        .expect("the Watch")
        .within
        .iter()
        .flatten()
        .filter(|within| **within)
        .count();
    assert!(within > 0);
    assert_eq!(cells(visuals.selection.bg_fill), within);
    assert_eq!(
        cells(visuals.widgets.inactive.weak_bg_fill),
        21 * 21 - within
    );
    // The Watch and each of the six characters, each in a cell of its own.
    let marks: Vec<String> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) if text.galley.text().chars().count() == 1 => {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .filter(|text| "@ABCDEF".contains(text.as_str()))
        .collect();
    assert_eq!(marks.len(), 7);
    // The matrix's scores are tinted: Hale toward Vex is unfriendly, the Watch toward Hale
    // friendly.
    let tinted = |rgb: (u8, u8, u8)| {
        let tint = Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, 160);
        shapes.iter().any(|shape| match shape {
            Shape::Text(text) => text
                .galley
                .job
                .sections
                .iter()
                .any(|section| section.format.background == tint),
            _ => false,
        })
    };
    assert!(tinted((0xf0, 0xa3, 0x5e)));
    assert!(tinted((0x5a, 0xa9, 0xe6)));
}

#[test]
fn marks_sharing_a_cell_are_listed_together() {
    let dir = std::env::temp_dir().join("factional_editor_shared_cell");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a fresh directory");
    std::fs::write(
        dir.join("factions.toml"),
        "[watch]\nname = \"The Watch\"\nalignment = { law = 70.0, good = 20.0 }\ntolerance = 40.0\n\n[[watch.ranks]]\nid = \"recruit\"\n",
    )
    .expect("written");
    std::fs::write(
        dir.join("characters.toml"),
        "[twin]\nname = \"Twin\"\nalignment = { law = -56.0, good = -21.0 }\n\n[vex]\nname = \"Vex\"\nalignment = { law = -55.0, good = -20.0 }\n",
    )
    .expect("written");
    let harness = previews_tab(dir);
    harness.get_by_label("* at law -60, good -20: A, B");
    assert_eq!(harness.query_all_by_label_contains("* at law").count(), 1);
}

#[test]
fn a_knob_is_plotted_with_its_points_in_words() {
    let mut harness = previews_tab(sample());
    harness.get_by_label("at 0.00: 50.00");
    harness.get_by_label("at 60.00: 0.00");
    harness.get_by_label("at 200.00: -50.00");
    let on = |harness: &Harness<'static, Editor>, label: &str| {
        harness.get_by_label(label).accesskit_node().toggled()
    };
    assert_eq!(on(&harness, "disposition.affinity"), Some(Toggled::True));
    assert_eq!(on(&harness, "standing.spillover"), Some(Toggled::False));
    // Plotted as one line through its three points, on a panel of its own.
    let visuals = harness.ctx.global_style().visuals.clone();
    let shapes = painted(&harness);
    let lines = shapes
        .iter()
        .filter(
            |shape| matches!(shape, Shape::Path(path) if !path.closed && path.points.len() == 3),
        )
        .count();
    assert_eq!(lines, 1);
    let panels = shapes
        .iter()
        .filter(|shape| {
            matches!(shape, Shape::Rect(rect)
                if rect.fill == visuals.extreme_bg_color && rect.rect.size() == vec2(360.0, 160.0))
        })
        .count();
    assert_eq!(panels, 1);
    harness
        .get_by_label("inertia.steady.law.toward_lawful")
        .click();
    harness.run();
    harness.get_by_label("left out, so 1.00 everywhere");
    assert!(harness.query_by_label("at 0.00: 50.00").is_none());
}

#[test]
fn choosing_a_character_and_a_quest_says_whether_they_can_start_it() {
    let mut harness = previews_tab(sample());
    harness.get_by_label("Can start: player").click();
    harness.run();
    harness.get_by_label("Quest: watch_captain").click();
    harness.run();
    harness.get_by_label("player can't start watch_captain:");
    harness.get_by_label_contains("step 1 of watch_career isn't complete: 0 of 1 done");
    harness.get_by_label("Quest: watch_oath").click();
    harness.run();
    harness.get_by_label("player can start watch_oath");
    let on = |label: &str| harness.get_by_label(label).accesskit_node().toggled();
    assert_eq!(on("Can start: player"), Some(Toggled::True));
    assert_eq!(on("Can start: vex"), Some(Toggled::False));
    assert_eq!(on("Quest: watch_oath"), Some(Toggled::True));
    assert_eq!(on("Quest: watch_captain"), Some(Toggled::False));
}

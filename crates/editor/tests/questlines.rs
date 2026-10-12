//! A questline's page (U13a, P-94): beside its read-only graph, its steps in order, each a card
//! of its quests, which open their own, and "+ Existing quest…", offering the quests in no
//! questline, added in one change. Nothing here is saved.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_editor::{Editor, WINDOW};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

fn on_watch_career() -> Harness<'static, Editor> {
    let mut harness = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, editor: &mut Editor| editor.ui(ui),
        Editor::open(Path::new(SAMPLE)),
    );
    harness.run();
    click(&mut harness, "Quests");
    click(&mut harness, "watch_career — A Life in the Watch");
    harness
}

fn click(harness: &mut Harness<'static, Editor>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn height(harness: &Harness<'static, Editor>, label: &str) -> f32 {
    harness.get_by_label(label).rect().center().y
}

#[test]
fn a_questlines_page_has_its_steps_in_order_each_with_its_quests() {
    let harness = on_watch_career();
    harness.get_by_label("A Life in the Watch");
    let steps: Vec<f32> = (1..=4)
        .map(|step| height(&harness, &format!("Step {step}")))
        .collect();
    assert!(steps.windows(2).all(|pair| pair[0] < pair[1]), "{steps:?}");
    // Each step says what it needs, and lists its quests by name.
    harness.get_by_label(
        "2 of night_patrol, dock_inspection, smugglers_cove, in any order; the rest close",
    );
    let patrol = height(&harness, "Night Patrol · night_patrol");
    assert!(steps[1] < patrol && patrol < steps[2]);
    harness.get_by_label("The Watch's Oath · watch_oath");
}

#[test]
fn an_existing_quest_is_added_to_a_step_in_one_change() {
    let mut harness = on_watch_career();
    click(&mut harness, "Add an existing quest to step 3…");
    // Only quests in no questline: a quest is in one at most.
    harness.get_by_label("Add lost_ring to step 3");
    harness.get_by_label("Add circle_rite to step 3");
    assert!(
        harness
            .query_by_label("Add night_patrol to step 3")
            .is_none()
    );
    click(&mut harness, "Add lost_ring to step 3");
    let ring = height(&harness, "Ava's Lost Ring · lost_ring");
    assert!(height(&harness, "Step 3") < ring && ring < height(&harness, "Step 4"));
    click(&mut harness, "Undo");
    assert!(
        harness
            .query_by_label("Ava's Lost Ring · lost_ring")
            .is_none()
    );
}

#[test]
fn a_quest_on_the_page_opens_its_own_and_goes_back() {
    let mut harness = on_watch_career();
    click(&mut harness, "Night Patrol · night_patrol");
    harness.get_by_label("Edit in Content");
    assert!(harness.query_by_label("Step 1").is_none());
    click(&mut harness, "Back to A Life in the Watch");
    harness.get_by_label("Step 1");
}

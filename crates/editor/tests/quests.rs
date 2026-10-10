//! The Quests tab (U4), driven headless through AccessKit: questlines and quests as graphs,
//! with the loader's problems at their nodes.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_editor::Editor;
use factional_editor::egui::accesskit::Toggled;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn harness(dir: PathBuf) -> Harness<'static, Editor> {
    let mut harness =
        Harness::new_ui_state(|ui, editor: &mut Editor| editor.ui(ui), Editor::open(dir));
    harness.set_size(factional_editor::egui::vec2(1440.0, 900.0));
    harness.run();
    harness
}

fn quests_tab(dir: PathBuf) -> Harness<'static, Editor> {
    let mut harness = harness(dir);
    harness.get_by_label("Quests").click();
    harness.run();
    harness
}

/// Whether the control labelled `label` is on.
fn toggled(harness: &Harness<'static, Editor>, label: &str) -> Option<Toggled> {
    harness.get_by_label(label).accesskit_node().toggled()
}

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn world(name: &str) -> PathBuf {
    Path::new(REPO)
        .join("crates/cli/tests/fixtures/worlds")
        .join(name)
}

#[test]
fn the_quests_tab_lists_questlines_then_quests_in_none() {
    let harness = quests_tab(sample());
    harness.get_by_label("watch_career — A Life in the Watch");
    harness.get_by_label("circle_rite — The Ashen Rite");
    harness.get_by_label("lost_ring — Ava's Lost Ring");
    harness.get_by_label("the_long_winter — The Long Winter");
    assert!(
        harness
            .query_by_label("watch_oath — The Watch's Oath")
            .is_none()
    );
}

#[test]
fn a_questline_shows_its_steps_quests_and_edges() {
    let mut harness = quests_tab(sample());
    assert_eq!(toggled(&harness, "Quests"), Some(Toggled::True));
    assert_eq!(toggled(&harness, "Content"), Some(Toggled::False));
    let line = "watch_career — A Life in the Watch";
    harness.get_by_label(line).click();
    harness.run();
    assert_eq!(toggled(&harness, line), Some(Toggled::True));
    let loose = "circle_rite — The Ashen Rite";
    assert_eq!(toggled(&harness, loose), Some(Toggled::False));
    harness.get_by_label_contains("step 2 — 2 of night_patrol, dock_inspection, smugglers_cove");
    harness.get_by_label("watch_oath — The Watch's Oath — 2 stages");
    harness.get_by_label("watch_captain — Captain of the Watch — 1 stage");
    harness.get_by_label("circle_rite — The Ashen Rite — 1 stage");
    harness.get_by_label("circle_rite.whisper.set_them_at_war locks watch_captain");
    harness.get_by_label("smugglers_cove needs watch_oath.patrol.report done");
    assert!(
        harness
            .query_by_label("the_long_winter.stores.share locks circle_rite")
            .is_none()
    );
}

#[test]
fn choosing_a_quest_shows_its_stages_and_edit_in_content_selects_it() {
    let mut harness = quests_tab(sample());
    harness
        .get_by_label("watch_career — A Life in the Watch")
        .click();
    harness.run();
    harness
        .get_by_label("watch_oath — The Watch's Oath — 2 stages")
        .click();
    harness.run();
    let oath = "watch_oath — The Watch's Oath — 2 stages";
    assert_eq!(toggled(&harness, oath), Some(Toggled::True));
    let captain = "watch_captain — Captain of the Watch — 1 stage";
    assert_eq!(toggled(&harness, captain), Some(Toggled::False));
    harness.get_by_label("report — outcome turned_in_vex — then oath");
    harness.get_by_label("2. oath — needs standing 10.00 with city_watch");
    harness.get_by_label("Edit in Content").click();
    harness.run();
    harness.get_by_label("quests.toml: watch_oath");
}

#[test]
fn a_quest_shows_its_stages_as_a_graph_with_their_problems() {
    let mut harness = quests_tab(world("dead_ends"));
    let siege = "siege — The Siege — 1 problem";
    harness.get_by_label(siege).click();
    harness.run();
    assert_eq!(toggled(&harness, siege), Some(Toggled::True));
    let jobs = "jobs — Odd Jobs — 1 problem";
    assert_eq!(toggled(&harness, jobs), Some(Toggled::False));
    harness.get_by_label("camp");
    harness.get_by_label("assault — 1 problem");
    harness.get_by_label("wait, then assault");
    harness.get_by_label("siege.assault needs aftermath done");
    harness.get_by_label("aftermath needs siege done");
    assert!(harness.query_by_label("rumour needs gossip done").is_none());
    harness.get_by_label(
        "error: requires.done[0]: aftermath can only happen once siege is over, so this stage can never be reached",
    );
}

#[test]
fn a_quest_that_can_never_start_says_so_at_its_node() {
    let mut harness = quests_tab(world("dead_ends"));
    harness.get_by_label("jobs — Odd Jobs — 1 problem").click();
    harness.run();
    harness
        .get_by_label("patrol — Patrol — 1 stage — 1 problem")
        .click();
    harness.run();
    harness.get_by_label(
        "error: it can never start: what it needs only comes after jobs moves on from steps[0], which closes it",
    );
}

#[test]
fn a_warning_shows_at_its_choice() {
    let mut harness = quests_tab(world("stale_lock"));
    harness.get_by_label("chores — Chores — 1 warning").click();
    harness.run();
    harness.get_by_label("yard — 1 warning");
    harness.get_by_label_contains("warning: locks[0]: it can't lock out muster.drill");
}

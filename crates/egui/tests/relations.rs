//! The relations matrix (U12c, G3): each faction's regard for each other, tinted by band,
//! marked where in conflict, labelled in full; drawn for the editor's Previews and the
//! inspector alike.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_egui::relations::relations_ui;
use factional_reputation::World;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

fn riverhold() -> World {
    let (content, _) = factional_content::load_quests(Path::new(SAMPLE)).expect("loads");
    World::new(content).expect("complete")
}

fn drawn(world: &World) -> Harness<'_> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 180.0))
        .wgpu()
        .build_ui(move |ui| relations_ui(ui, world));
    harness.run();
    harness
}

#[test]
fn every_faction_toward_every_other_is_labelled_in_full() {
    let world = riverhold();
    let harness = drawn(&world);
    // DESIGN.md §13: the Watch and the Guild are enemies, in conflict.
    harness.get_by_label("city_watch → lantern_guild: -80.00, enemy, in conflict");
    harness.get_by_label("free_company → city_watch: -10.00, neutral");
    harness.get_by_label("temple → free_company: 0.00, neutral");
    assert_eq!(harness.query_all_by_label_contains(" → ").count(), 20);
    assert!(harness.query_by_label_contains("temple → temple").is_none());
}

/// Its picture: each cell tinted by its band, hostile orange and friendly blue (P-120).
#[test]
fn the_matrix_tinted_by_band() {
    let world = riverhold();
    drawn(&world).snapshot("relations");
}

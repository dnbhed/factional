//! Each suggestion's line, as `world suggestions` says it, for the builder to show on hover
//! (W7b).

use std::path::Path;

use factional_content::read_map;
use factional_session::{suggestion_lines, world_suggestions};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[test]
fn each_suggestions_line_is_what_world_suggestions_says_of_it_in_order() {
    let base = Path::new(REPO).join("content/sample");
    let bytes = std::fs::read(base.join("world/riverhold.map")).expect("the sample's map");
    let map = read_map(&bytes).expect("a map");
    let lines = suggestion_lines(&map);
    let ids: Vec<&str> = lines.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "settlement_1",
            "settlement_2",
            "settlement_4",
            "route_0",
            "route_1",
            "route_2",
            "poi_0",
            "poi_1",
            "culture_1",
            "culture_2",
            "religion_1",
            "religion_2",
            "state_1",
            "state_2",
        ]
    );
    assert_eq!(
        lines[0].1,
        "settlement_1 Highmoor: generic, capital, 2345 people, at tile (60, 19); citadel, walls and temple"
    );
    let said = world_suggestions(&base, "world/riverhold.map").expect("the listing");
    let listed: Vec<&str> = said.lines().skip(1).collect();
    let shared: Vec<&str> = lines.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(shared, listed);
}

#[test]
fn a_dismissed_suggestions_line_is_marked_as_the_listing_marks_it() {
    let base = Path::new(REPO).join("content/sample");
    let bytes = std::fs::read(base.join("world/riverhold.map")).expect("the sample's map");
    let mut map = read_map(&bytes).expect("a map");
    map.dismiss(&["route_1"]).expect("dismissed");
    let lines = suggestion_lines(&map);
    assert_eq!(
        lines[4].1,
        "route_1: trail, 2 points, tile (29, 53) to (41, 41) (dismissed)"
    );
}

//! What a key is, in words (U6d): from the schema, for the keys a designer can add.

use factional_content::{ValuePath, key_info};

fn kind(file: &str, path: &str) -> String {
    let path = ValuePath::parse(path).expect("a path");
    key_info(file, &path).expect("the schema knows it").kind
}

#[test]
fn a_number_says_its_range_and_a_whole_number_says_it_is_whole() {
    assert_eq!(
        kind("factions.toml", "city_watch.expel_standing_change"),
        "a number from -100.00 to 100.00"
    );
    assert_eq!(
        kind("factions.toml", "city_watch.tolerance"),
        "a number at least 0.00"
    );
    assert_eq!(
        kind("factions.toml", "city_watch.drift.grace_ticks"),
        "a whole number at least 1"
    );
}

#[test]
fn a_choice_says_what_it_may_be() {
    assert_eq!(
        kind("factions.toml", "city_watch.drift.policy"),
        "one of ignore, flag, demote, expel or probation"
    );
    assert_eq!(
        kind("factions.toml", "city_watch.secret_members"),
        "true or false"
    );
    assert_eq!(
        kind("characters.toml", "captain_hale.inertia"),
        "the id of an inertia profile"
    );
    assert_eq!(
        kind("characters.toml", "captain_hale.memberships[0].faction"),
        "the id of a faction"
    );
}

#[test]
fn text_tables_and_lists_say_so() {
    assert_eq!(kind("factions.toml", "city_watch.name"), "text");
    assert_eq!(kind("factions.toml", "city_watch.ranks"), "a list");
    assert_eq!(kind("factions.toml", "city_watch.drift"), "a table");
    assert_eq!(kind("factions.toml", "city_watch.defectors"), "a table");
    // A curve may be a number or a list of points.
    assert_eq!(
        kind("balance.toml", "disposition.affinity"),
        "a number from -100.00 to 100.00 or a list"
    );
}

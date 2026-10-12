//! Where a value is, in words (U10a, P-92): each key by its title, each named list item by
//! what it's called and its name, each numbered one from 1, and each id by the name of what it
//! names.

use std::path::Path;

use factional_content::{
    ContentTexts, FormGroup, SCHEMA_FILES, ValuePath, Word, entry_form, key_info, place_words,
    read_texts, schema, words_at,
};
use serde_json::Value;

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn path(key: &str) -> ValuePath {
    ValuePath::parse(key).expect("a key")
}

fn texts(words: &[Word]) -> Vec<&str> {
    words.iter().map(|word| word.text.as_str()).collect()
}

#[test]
fn a_key_is_said_by_its_title_and_an_entry_by_its_name() {
    let texts = sample();
    assert_eq!(
        words_at(&texts, "factions.toml", "city_watch.member_tolerance"),
        "The City Watch › Member tolerance"
    );
    assert_eq!(
        words_at(&texts, "factions.toml", "city_watch.leave_standing_change"),
        "The City Watch › Standing change on leaving"
    );
    // Balance's entries are its keys, so they're said by their titles too.
    assert_eq!(
        words_at(&texts, "balance.toml", "knowledge.ripple.hop_ticks"),
        "Knowledge › Ripple › Ticks a hop"
    );
}

#[test]
fn a_named_list_item_is_said_by_what_its_called_and_its_name_in_place_of_its_list() {
    let texts = sample();
    assert_eq!(
        words_at(
            &texts,
            "quests.toml",
            "dock_inspection.stages[0].choices[0].effects.standing.factions.city_watch"
        ),
        "Inspecting the Docks › Stage \"inspect\" › Choice \"seize\" › Effects › Standing › \
         Factions › The City Watch"
    );
    assert_eq!(
        words_at(
            &texts,
            "factions.toml",
            "city_watch.ranks[1].requires.standing"
        ),
        "The City Watch › Rank \"sergeant\" › Requirements › Standing"
    );
    // An item named by an id is said by the name of what the id names.
    assert_eq!(
        words_at(
            &texts,
            "characters.toml",
            "captain_hale.memberships[0].rank"
        ),
        "Captain Hale › Membership \"The City Watch\" › Rank"
    );
    // So is a key that's an id, as a faction's relations are keyed (U12a).
    assert_eq!(
        words_at(&texts, "factions.toml", "city_watch.relations.temple"),
        "The City Watch › Relations › Temple of the Dawn"
    );
    assert_eq!(
        words_at(&texts, "balance.toml", "disposition.bands[1].up_to"),
        "Disposition › Band \"neutral\" › Up to"
    );
}

#[test]
fn an_item_without_a_name_is_numbered_from_one_under_its_list() {
    let texts = sample();
    assert_eq!(
        words_at(&texts, "questlines.toml", "watch_career.steps[3].quests[0]"),
        "A Life in the Watch › Steps › Step 4 › Quests › 1"
    );
    assert_eq!(
        words_at(
            &texts,
            "balance.toml",
            "membership.defectors.rules[1].when.closer_to_target"
        ),
        "Membership › Defectors › Rules › Rule 2 › When › Closer to target"
    );
    // A curve's point, and its x and y.
    assert_eq!(
        words_at(&texts, "balance.toml", "disposition.affinity[1][0]"),
        "Disposition › Affinity › Point 2 › x"
    );
}

#[test]
fn what_the_schema_doesnt_know_is_said_as_written() {
    let texts = sample();
    // A misspelt key keeps its spelling, so the designer sees what they wrote.
    assert_eq!(
        words_at(&texts, "factions.toml", "city_watch.tolerence"),
        "The City Watch › tolerence"
    );
    // An entry that isn't there is its id; a key that isn't a place is itself.
    assert_eq!(
        words_at(&texts, "factions.toml", "nobody.name"),
        "nobody › Name"
    );
    assert_eq!(words_at(&texts, "factions.toml", "a..b"), "a..b");
    // A profile has no name, so it's its id.
    assert_eq!(
        words_at(
            &texts,
            "balance.toml",
            "inertia.profiles.hardening.good.toward_good[0][1]"
        ),
        "Inertia › Profiles › hardening › Good › Toward good › Point 1 › y"
    );
}

#[test]
fn each_word_says_how_far_along_the_path_it_reaches() {
    let words = place_words(
        &sample(),
        "factions.toml",
        &path("city_watch.ranks[1].requires.standing"),
    );
    let through: Vec<(&str, usize)> = words
        .iter()
        .map(|word| (word.text.as_str(), word.through))
        .collect();
    // The rank says both the list and the item, so it reaches the third step.
    assert_eq!(
        through,
        [
            ("The City Watch", 1),
            ("Rank \"sergeant\"", 3),
            ("Requirements", 4),
            ("Standing", 5),
        ]
    );
}

#[test]
fn a_forms_rows_are_said_in_words_from_the_entry() {
    fn words(group: &FormGroup) -> Vec<Vec<&str>> {
        group.rows.iter().map(|row| texts(&row.words)).collect()
    }
    let form = entry_form(&sample(), "factions.toml", "city_watch");
    assert_eq!(
        words(&form[1]),
        [
            vec!["Alignment", "Law"],
            vec!["Alignment", "Good"],
            vec!["Weights", "Law"],
            vec!["Weights", "Good"],
        ]
    );
    // A default is said like what's written.
    assert_eq!(
        words(&form[3])[2..],
        [
            vec!["Standing change on expulsion"],
            vec!["Standing change on leaving"],
            vec!["Secret members"],
        ]
    );
    let ranks = &form[5].rows[2];
    assert_eq!(ranks.key, "ranks[1].requires.standing");
    let through: Vec<usize> = ranks.words.iter().map(|word| word.through).collect();
    assert_eq!(
        texts(&ranks.words),
        ["Rank \"sergeant\"", "Requirements", "Standing"]
    );
    assert_eq!(through, [2, 3, 4]);
}

#[test]
fn a_keys_title_comes_with_what_the_schema_says_of_it() {
    let info = key_info("factions.toml", &path("city_watch.member_tolerance")).expect("known");
    assert_eq!(info.title.as_deref(), Some("Member tolerance"));
    let info = key_info("characters.toml", &path("vex.memberships")).expect("known");
    assert_eq!(info.title.as_deref(), Some("Memberships"));
}

#[test]
fn what_the_schema_says_reaches_into_a_curves_points() {
    // A curve is a number or a list of points, and a point's place is in the list (a defect
    // found in U10a: the walk took the number, so a point said nothing).
    let point = key_info("balance.toml", &path("disposition.affinity[1][1]")).expect("known");
    assert_eq!(point.title.as_deref(), Some("y"));
    let point = key_info("balance.toml", &path("disposition.affinity[1]")).expect("known");
    assert_eq!(point.kind, "a list");
}

#[test]
fn every_key_in_every_schema_has_a_title() {
    fn untitled(node: &Value, at: &str, missing: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                if let Some(Value::Object(properties)) = map.get("properties") {
                    for (key, property) in properties {
                        if property.get("title").and_then(Value::as_str).is_none() {
                            missing.push(format!("{at}.{key}"));
                        }
                    }
                }
                // An `if` says when a rule applies; its properties aren't keys.
                for (key, inner) in map.iter().filter(|(key, _)| *key != "if") {
                    untitled(inner, &format!("{at}/{key}"), missing);
                }
            }
            Value::Array(items) => {
                for item in items {
                    untitled(item, at, missing);
                }
            }
            _ => {}
        }
    }
    let mut missing = Vec::new();
    for file in SCHEMA_FILES {
        untitled(&schema(file).expect("a schema"), file, &mut missing);
    }
    assert!(missing.is_empty(), "keys without a title: {missing:?}");
}

//! What the form needs to put each place's adding where it is (U10b): a row for an empty table
//! or list, what a list's items are called, and the ids a table keyed by them may take.

use std::path::Path;

use factional_content::{
    ContentTexts, FormRow, IdChoice, ValuePath, entry_form, id_choices, key_info, read_texts,
};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn path(key: &str) -> ValuePath {
    ValuePath::parse(key).expect("a key")
}

/// The sample, with `added` written after Merchant Ava's alignment.
fn with_ava(added: &str) -> ContentTexts {
    let mut texts = sample();
    let characters = texts[2].as_mut().expect("characters.toml");
    let line = "alignment = { law = 20.0, good = 10.0 }\n";
    let at = characters.find(line).expect("Ava's alignment") + line.len();
    characters.insert_str(at, added);
    texts
}

fn rows(texts: &ContentTexts, entry: &str) -> Vec<FormRow> {
    entry_form(texts, "characters.toml", entry)
        .into_iter()
        .flat_map(|group| group.rows)
        .collect()
}

#[test]
fn an_empty_table_or_list_is_a_row_with_nothing_written_and_no_default() {
    let texts = with_ava("standing = {}\nmemberships = []\n");
    let rows = rows(&texts, "merchant_ava");
    let empty: Vec<(&str, Vec<&str>)> = rows
        .iter()
        .filter(|row| row.written.is_none() && row.default.is_none())
        .map(|row| {
            let words = row.words.iter().map(|word| word.text.as_str()).collect();
            (row.key.as_str(), words)
        })
        .collect();
    assert_eq!(
        empty,
        [
            ("memberships", vec!["Memberships"]),
            ("standing", vec!["Standing"]),
        ]
    );
}

#[test]
fn a_table_with_an_empty_table_in_it_isnt_empty_itself() {
    let texts = with_ava("standing = { factions = {} }\n");
    let rows = rows(&texts, "merchant_ava");
    let empty: Vec<&str> = rows
        .iter()
        .filter(|row| row.written.is_none() && row.default.is_none())
        .map(|row| row.key.as_str())
        .collect();
    assert_eq!(empty, ["standing.factions"]);
}

#[test]
fn a_list_says_what_its_items_are_called() {
    let item = |file: &str, key: &str| key_info(file, &path(key)).and_then(|info| info.item);
    assert_eq!(
        item("factions.toml", "city_watch.ranks").as_deref(),
        Some("Rank")
    );
    assert_eq!(
        item("quests.toml", "dock_inspection.stages[0].choices").as_deref(),
        Some("Choice")
    );
    // A list of values has none, nor does anything that isn't a list.
    assert_eq!(
        item("questlines.toml", "watch_career.steps[0].quests"),
        None
    );
    assert_eq!(item("factions.toml", "city_watch.alignment"), None);
}

fn choice(id: &str, name: Option<&str>) -> IdChoice {
    IdChoice {
        id: id.to_owned(),
        name: name.map(str::to_owned),
    }
}

#[test]
fn a_table_keyed_by_ids_may_take_each_that_exists_and_isnt_there_yet() {
    let texts = sample();
    // Vex's standing is with the Guild already.
    assert_eq!(
        id_choices(&texts, "characters.toml", &path("vex.standing.factions")),
        [
            choice("ashen_circle", Some("The Ashen Circle")),
            choice("city_watch", Some("The City Watch")),
            choice("free_company", Some("The Free Company")),
            choice("temple", Some("Temple of the Dawn")),
        ]
    );
    // A requirement's standing may be with a faction or a character.
    let choices = id_choices(
        &texts,
        "questlines.toml",
        &path("watch_career.steps[3].requires.standing"),
    );
    let ids: Vec<&str> = choices.iter().map(|choice| choice.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "ashen_circle",
            "brother_ash",
            "captain_hale",
            "free_company",
            "lantern_guild",
            "merchant_ava",
            "player",
            "sister_mira",
            "temple",
            "vex",
        ]
    );
    // A table not keyed by what it names takes none.
    assert!(id_choices(&texts, "characters.toml", &path("vex.alignment")).is_empty());
}

#[test]
fn a_list_of_exactly_two_ids_is_a_pair() {
    let pair = |file: &str, key: &str| key_info(file, &path(key)).is_some_and(|info| info.pair);
    assert!(pair("relations.toml", "relation[0].between"));
    assert!(pair("outcomes.toml", "sowed_discord.relations[0].between"));
    // A curve's point is two numbers, not two ids; a list of ids may be any length.
    assert!(!pair("balance.toml", "disposition.affinity[0]"));
    assert!(!pair("characters.toml", "merchant_ava.contacts"));
    assert!(!pair("relations.toml", "relation[0].value"));
}

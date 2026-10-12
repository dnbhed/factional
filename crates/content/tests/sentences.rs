//! Requirements and effects as sentences (U11a): each with its value as a field within it, the
//! words before and after it, and how many of the value's own words it says.

use std::path::Path;

use factional_content::{ContentTexts, FormRow, Sentence, entry_form, read_texts};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn row(file: &str, entry: &str, key: &str) -> FormRow {
    entry_form(&sample(), file, entry)
        .into_iter()
        .flat_map(|group| group.rows)
        .find(|row| row.key == key)
        .unwrap_or_else(|| panic!("{key} in {entry}'s form"))
}

fn sentence(before: &str, after: &str, says: usize) -> Option<Sentence> {
    Some(Sentence {
        before: before.to_owned(),
        after: after.to_owned(),
        says,
    })
}

#[test]
fn a_requirement_is_a_sentence_with_its_value_in_it() {
    let quest = |entry: &str, key: &str| row("quests.toml", entry, key).sentence;
    assert_eq!(
        quest("watch_oath", "stages[1].requires.standing.city_watch"),
        sentence("Needs standing", "with The City Watch", 2)
    );
    assert_eq!(
        quest("watch_oath", "requires.not_member[0]"),
        sentence("Needs not to be in", "", 2)
    );
    assert_eq!(
        quest("watch_captain", "stages[0].requires.member[0]"),
        sentence("Needs to be in", "", 2)
    );
    assert_eq!(
        quest("watch_captain", "stages[0].requires.within_tolerance[0]"),
        sentence("Needs to be within", "'s member tolerance", 2)
    );
    assert_eq!(
        quest("smugglers_cove", "requires.done[0]"),
        sentence("Needs", "done", 2)
    );
    assert_eq!(
        row(
            "questlines.toml",
            "watch_career",
            "steps[3].requires.rank_at_least.city_watch"
        )
        .sentence,
        sentence("Needs rank", "or higher in The City Watch", 2)
    );
}

#[test]
fn an_effect_is_a_sentence_too() {
    let docks = |key: &str| row("quests.toml", "dock_inspection", key).sentence;
    assert_eq!(
        docks("stages[0].choices[0].effects.alignment.law"),
        sentence("Moves law by", "", 2)
    );
    assert_eq!(
        docks("stages[0].choices[0].effects.standing.factions.city_watch"),
        sentence("Standing with The City Watch changes by", "", 3)
    );
    let murder = |key: &str| row("actions.toml", "murder", key).sentence;
    assert_eq!(murder("alignment.good"), sentence("Moves good by", "", 2));
    assert_eq!(
        murder("standing.target"),
        sentence("The target's regard changes by", "", 2)
    );
    assert_eq!(
        murder("standing.target_factions"),
        sentence("The target's factions' regard changes by", "", 2)
    );
}

#[test]
fn a_characters_standing_is_where_it_starts_and_other_values_arent_sentences() {
    let hale = |key: &str| row("characters.toml", "captain_hale", key).sentence;
    assert_eq!(
        hale("standing.factions.city_watch"),
        sentence("Standing", "with The City Watch", 3)
    );
    assert_eq!(hale("name"), None);
    assert_eq!(hale("alignment.law"), None);
}

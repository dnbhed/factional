//! What names what (U6a): references found from the schema, held to the loader's own
//! checks, plus what the schema says of a key and an entry's name.

use std::collections::BTreeSet;
use std::path::Path;

use factional_content::{
    CONTENT_FILES, ContentTexts, ValuePath, defined_in, key_info, load_texts, outline_texts,
    read_texts, referenced_by, references,
};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn place(file: &str) -> usize {
    CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file")
}

/// `texts` with `id`'s definition in `file` renamed, so whatever names it names nothing.
fn renamed(texts: &ContentTexts, file: &str, id: &str) -> ContentTexts {
    let mut texts = texts.clone();
    let text = texts[place(file)].take().expect("the file is there");
    let new = format!("zz_{id}");
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let header = line.trim_start_matches('[');
            let depth = line.len() - header.len();
            let names_it = depth > 0
                && (header.starts_with(&format!("{id}]")) || header.starts_with(&format!("{id}.")));
            let profile = format!("inertia.profiles.{id}");
            if names_it {
                format!("{}{new}{}", &line[..depth], &header[id.len()..])
            } else if file == "balance.toml" && header.starts_with(&profile) {
                line.replacen(&profile, &format!("inertia.profiles.{new}"), 1)
            } else {
                line.to_owned()
            }
        })
        .collect();
    texts[place(file)] = Some(lines.join("\n") + "\n");
    texts
}

/// Where the loader says `id` is unknown, as `file: key`: nowhere if nothing names it, so
/// the content still loads.
fn unknown_at(texts: &ContentTexts, id: &str) -> BTreeSet<String> {
    let quoted = format!("'{id}'");
    let Err(error) = load_texts(texts) else {
        return BTreeSet::new();
    };
    error
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("unknown") && d.message.contains(&quoted))
        .map(|d| format!("{}: {}", d.file, d.key.as_deref().unwrap_or_default()))
        .collect()
}

fn listed(texts: &ContentTexts, file: &str, id: &str) -> BTreeSet<String> {
    referenced_by(texts, file, id)
        .iter()
        .map(|r| format!("{}: {}", r.file, r.key))
        .collect()
}

/// Every content directory in the repository that loads: the sample, the complete example
/// and the CLI's fixture worlds.
fn worlds() -> Vec<ContentTexts> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = repo.join("crates/cli/tests/fixtures/worlds");
    let mut dirs = vec![
        repo.join("content/sample"),
        repo.join("docs/examples/riverhold"),
    ];
    let mut found: Vec<_> = std::fs::read_dir(&fixtures)
        .expect("the fixture worlds")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    found.sort();
    dirs.extend(found);
    dirs.iter()
        .filter_map(|dir| read_texts(dir).ok())
        .filter(|texts| load_texts(texts).is_ok())
        .collect()
}

#[test]
fn every_reference_the_loader_checks_is_found_and_nothing_else() {
    let worlds = worlds();
    assert!(worlds.len() > 5, "{} worlds load", worlds.len());
    let mut checked = 0;
    for texts in &worlds {
        let outline = outline_texts(texts);
        for file in [
            "factions.toml",
            "characters.toml",
            "quests.toml",
            "outcomes.toml",
        ] {
            for entry in &outline.files[place(file)].entries {
                let id = entry.key.as_str();
                let expected = unknown_at(&renamed(texts, file, id), id);
                assert_eq!(listed(texts, file, id), expected, "{file}: {id}");
                checked += expected.len();
            }
        }
        for profile in profiles(texts) {
            let expected = unknown_at(&renamed(texts, "balance.toml", &profile), &profile);
            assert_eq!(
                listed(texts, "balance.toml", &profile),
                expected,
                "{profile}"
            );
            checked += expected.len();
        }
    }
    // Plenty is named: factions, characters, quests, outcomes and profiles all over.
    assert!(checked > 100, "{checked}");
}

/// The inertia profiles a world defines, other than steady, which is always there.
fn profiles(texts: &ContentTexts) -> Vec<String> {
    let Some(balance) = &texts[place("balance.toml")] else {
        return Vec::new();
    };
    balance
        .lines()
        .filter_map(|line| line.strip_prefix("[inertia.profiles."))
        .filter_map(|rest| rest.strip_suffix(']'))
        .filter(|profile| *profile != "steady")
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_character_is_named_by_id_and_as_a_party() {
    let texts = sample();
    let hale = listed(&texts, "characters.toml", "captain_hale");
    for key in [
        "outcomes.toml: fined_by_watch.standing.characters.captain_hale",
        "quests.toml: lost_dog.giver",
        "quests.toml: lost_dog.stages[0].choices[0].effects.standing.characters.captain_hale",
    ] {
        assert!(hale.contains(key), "{key} in {hale:?}");
    }
    assert!(references(&texts).len() >= hale.len());
    assert!(listed(&texts, "characters.toml", "nobody").is_empty());
}

#[test]
fn a_keys_description_and_default_come_from_the_schema() {
    let path = |text: &str| ValuePath::parse(text).expect("a path");
    let expel = key_info("factions.toml", &path("city_watch.expel_standing_change"))
        .expect("the schema knows it");
    assert_eq!(expel.default.as_deref(), Some("-20.00"));
    assert!(expel.description.is_some_and(|d| d.contains("expels")));
    let tolerance =
        key_info("factions.toml", &path("city_watch.tolerance")).expect("the schema knows it");
    assert_eq!(tolerance.default, None);
    assert!(tolerance.description.is_some());
    let secret = key_info("factions.toml", &path("city_watch.secret_members")).expect("known");
    assert_eq!(secret.default.as_deref(), Some("false"));
    // A whole number stays whole.
    let hop = key_info("balance.toml", &path("knowledge.ripple.hop_ticks")).expect("known");
    assert_eq!(hop.default.as_deref(), Some("1"));
    assert!(key_info("factions.toml", &path("city_watch.nonsense")).is_none());
    assert!(key_info("nowhere.toml", &path("city_watch")).is_none());
}

#[test]
fn an_entry_is_named_by_its_name() {
    let outline = outline_texts(&sample());
    let named = |file: &str, key: &str| {
        outline.files[place(file)]
            .entries
            .iter()
            .find(|entry| entry.key == key)
            .expect("an entry")
            .name
            .clone()
    };
    assert_eq!(
        named("factions.toml", "city_watch").as_deref(),
        Some("The City Watch")
    );
    assert_eq!(
        named("quests.toml", "watch_oath").as_deref(),
        Some("The Watch's Oath")
    );
    assert_eq!(named("relations.toml", "relation[0]"), None);
    assert_eq!(named("actions.toml", "steal"), None);
}

#[test]
fn an_id_is_defined_by_the_file_whose_entry_or_profile_it_is() {
    let texts = sample();
    assert_eq!(defined_in(&texts, "city_watch"), Some("factions.toml"));
    assert_eq!(defined_in(&texts, "captain_hale"), Some("characters.toml"));
    assert_eq!(defined_in(&texts, "watch_oath"), Some("quests.toml"));
    assert_eq!(defined_in(&texts, "turned_in_vex"), Some("outcomes.toml"));
    assert_eq!(defined_in(&texts, "hardening"), Some("balance.toml"));
    // Actions are named only by commands, so they're nobody's reference.
    assert_eq!(defined_in(&texts, "steal"), None);
    assert_eq!(defined_in(&texts, "nobody"), None);
    // Steady is always there, written or not.
    let nothing = ContentTexts::default();
    assert_eq!(defined_in(&nothing, "steady"), Some("balance.toml"));
    assert_eq!(defined_in(&nothing, "hardening"), None);
}

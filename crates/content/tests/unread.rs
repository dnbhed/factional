//! What couldn't be read isn't reported again as missing (T6): the sample, `content/sample`,
//! with one thing broken, reports that one thing and nothing that only follows from it.

use std::fs;
use std::path::Path;

use factional_content::{Sources, parse_content};

/// Each of the sample's files, with `change` applied to the one named `file`.
fn sample_with(file: &str, change: impl Fn(&str) -> String) -> [String; 8] {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    [
        "balance.toml",
        "factions.toml",
        "characters.toml",
        "actions.toml",
        "relations.toml",
        "outcomes.toml",
        "quests.toml",
        "questlines.toml",
    ]
    .map(|name| {
        let text = fs::read_to_string(dir.join(name)).expect("the sample is there");
        if name == file { change(&text) } else { text }
    })
}

/// Every problem `texts` has, as `validate` prints them.
fn problems(texts: &[String; 8]) -> Vec<String> {
    let [
        balance,
        factions,
        characters,
        actions,
        relations,
        outcomes,
        quests,
        questlines,
    ] = texts;
    let sources = Sources {
        balance: Some(balance),
        factions: Some(factions),
        characters: Some(characters),
        actions: Some(actions),
        relations: Some(relations),
        outcomes: Some(outcomes),
        quests: Some(quests),
        questlines: Some(questlines),
    };
    match parse_content(sources) {
        Ok(_) => Vec::new(),
        Err(error) => error.diagnostics.iter().map(ToString::to_string).collect(),
    }
}

/// `text` with `from` replaced once, insisting it's there.
fn replace(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "the sample has {from:?}");
    text.replacen(from, to, 1)
}

/// The City Watch with a tolerance its member tolerance is below.
fn wide_watch(text: &str) -> String {
    replace(text, "\ntolerance = 40.0\n", "\ntolerance = 140.0\n")
}

#[test]
fn a_faction_that_cant_be_read_isnt_reported_wherever_its_named() {
    assert_eq!(
        problems(&sample_with("factions.toml", wide_watch)),
        [
            "factions.toml: city_watch.member_tolerance: 50.00 must be at least the faction's tolerance, 140.00"
        ]
    );
}

#[test]
fn a_character_that_cant_be_read_isnt_reported_wherever_theyre_named() {
    let texts = sample_with("characters.toml", |text| {
        replace(
            text,
            "alignment = { law = 20.0, good = 10.0 }",
            "alignment = { law = 20.0, good = \"x\" }",
        )
    });
    assert_eq!(
        problems(&texts),
        ["characters.toml: merchant_ava.alignment.good: expected a number, like 25.0"]
    );
}

#[test]
fn a_file_that_doesnt_parse_hides_no_reference_to_what_it_holds() {
    let texts = sample_with("factions.toml", |text| replace(text, "[temple]", "[temple"));
    assert_eq!(
        problems(&texts),
        ["factions.toml: line 65: unclosed table, expected `]`"]
    );
}

#[test]
fn a_balance_file_that_doesnt_parse_isnt_judged_by_its_defaults() {
    let texts = sample_with("balance.toml", |text| {
        replace(text, "[disposition]", "[disposition")
    });
    assert_eq!(
        problems(&texts),
        ["balance.toml: line 36: unclosed table, expected `]`"]
    );
}

#[test]
fn a_knowledge_model_that_cant_be_read_doesnt_rule_out_secret_members() {
    let texts = sample_with("balance.toml", |text| {
        replace(text, "model = \"ripple\"", "model = \"rippel\"")
    });
    assert_eq!(
        problems(&texts),
        [
            "balance.toml: knowledge.model: unknown knowledge model 'rippel' (did you mean 'ripple'?)"
        ]
    );
}

#[test]
fn a_profile_that_cant_be_read_isnt_reported_wherever_its_named() {
    let texts = sample_with("balance.toml", |text| {
        let text = replace(
            text,
            "[inertia.profiles.steady]",
            "[inertia.profiles]\nhardening = 1\n\n[inertia.profiles.steady]",
        );
        replace(
            &text,
            "[inertia.profiles.hardening]",
            "[inertia.profiles.unused]",
        )
    });
    assert_eq!(
        problems(&texts),
        [
            "balance.toml: inertia.profiles.hardening: expected a table, like [inertia.profiles.hardening]"
        ]
    );
}

#[test]
fn a_reference_to_something_not_there_at_all_is_still_reported() {
    let mut texts = sample_with("factions.toml", wide_watch);
    texts[2] = replace(
        &texts[2],
        "faction = \"lantern_guild\", rank = \"fence\"",
        "faction = \"lantern_gild\", rank = \"fence\"",
    );
    assert_eq!(
        problems(&texts),
        [
            "factions.toml: city_watch.member_tolerance: 50.00 must be at least the faction's tolerance, 140.00",
            "characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
        ]
    );
}

/// `text` without the part from `start` up to (not including) `end`.
fn cut(text: &str, start: &str, end: &str) -> String {
    let from = text.find(start).expect("the sample has the start");
    let to = text[from..].find(end).map_or(text.len(), |to| from + to);
    format!("{}{}", &text[..from], &text[to..])
}

#[test]
fn a_contact_that_cant_be_read_isnt_reported() {
    let texts = sample_with("characters.toml", |text| {
        replace(
            text,
            "alignment = { law = 25.0, good = -70.0 }",
            "alignment = { law = 25.0, good = \"x\" }",
        )
    });
    assert_eq!(
        problems(&texts),
        ["characters.toml: brother_ash.alignment.good: expected a number, like 25.0"]
    );
}

#[test]
fn a_characters_file_that_doesnt_parse_hides_no_reference_to_its_characters() {
    let texts = sample_with("characters.toml", |text| replace(text, "[vex]", "[vex"));
    assert_eq!(
        problems(&texts),
        ["characters.toml: line 43: unclosed table, expected `]`"]
    );
}

#[test]
fn inertia_that_isnt_a_table_hides_no_reference_to_a_profile() {
    let texts = sample_with("balance.toml", |text| {
        format!("inertia = 1\n{}", cut(text, "[inertia]", "[disposition]"))
    });
    assert_eq!(
        problems(&texts),
        ["balance.toml: inertia: expected a table, like [inertia]"]
    );
}

#[test]
fn profiles_that_arent_a_table_hide_no_reference_to_a_profile() {
    let texts = sample_with("balance.toml", |text| {
        let text = cut(text, "[inertia.profiles.steady]", "[disposition]");
        replace(
            &text,
            "default_profile = \"steady\"\n",
            "default_profile = \"steady\"\nprofiles = 1\n\n",
        )
    });
    assert_eq!(
        problems(&texts),
        ["balance.toml: inertia.profiles: expected a table, like [inertia.profiles.steady]"]
    );
}

#[test]
fn knowledge_that_isnt_a_table_doesnt_rule_out_secret_members() {
    let texts = sample_with("balance.toml", |text| {
        format!(
            "knowledge = 1\n{}",
            cut(text, "[knowledge]", "[knowledge.never]")
        )
    });
    assert_eq!(
        problems(&texts),
        ["balance.toml: knowledge: expected a table, like [knowledge]"]
    );
}

#[test]
fn a_knowledge_model_that_isnt_text_doesnt_rule_out_secret_members() {
    let texts = sample_with("balance.toml", |text| {
        replace(text, "model = \"ripple\"", "model = 3")
    });
    assert_eq!(
        problems(&texts),
        ["balance.toml: knowledge.model: expected text in quotes"]
    );
}

#[test]
fn a_knowledge_model_left_out_is_the_default_and_judged_by() {
    let texts = sample_with("balance.toml", |text| {
        replace(text, "model = \"ripple\"\n", "")
    });
    let secret = "secret members need knowledge.model witnessed or ripple: under omniscient, everyone knows everything";
    assert_eq!(
        problems(&texts),
        [
            format!("factions.toml: ashen_circle.secret_members: {secret}"),
            format!("factions.toml: lantern_guild.secret_members: {secret}"),
        ]
    );
}

#[test]
fn a_profile_named_with_no_inertia_at_all_is_still_reported() {
    let texts = sample_with("balance.toml", |text| {
        cut(text, "[inertia]", "[disposition]")
    });
    let unknown = "inertia: unknown inertia profile 'hardening'";
    assert_eq!(
        problems(&texts),
        [
            format!("characters.toml: captain_hale.{unknown}"),
            format!("characters.toml: sister_mira.{unknown}"),
        ]
    );
}

#[test]
fn an_entry_that_isnt_a_table_isnt_reported_wherever_its_named() {
    let texts = sample_with("factions.toml", |text| {
        format!("temple = 1\n{}", cut(text, "[temple]", "[lantern_guild]"))
    });
    assert_eq!(
        problems(&texts),
        ["factions.toml: temple: expected a table of faction fields, like [temple]"]
    );
}

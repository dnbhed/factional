//! An entry's form (U6b): its values grouped by what they mean, each with the comment above
//! it, the defaults of what's left out, and what each may be, from the text and the schema.

use std::path::Path;

use factional_content::{ContentTexts, FormGroup, FormRow, entry_form, read_texts, schema};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn keys(group: &FormGroup) -> Vec<&str> {
    group.rows.iter().map(|row| row.key.as_str()).collect()
}

fn row<'f>(form: &'f [FormGroup], key: &str) -> &'f FormRow {
    form.iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.key == key)
        .unwrap_or_else(|| panic!("{key} in the form"))
}

fn written(row: &FormRow) -> Option<&str> {
    row.written.as_ref().map(|field| field.value.as_str())
}

#[test]
fn a_factions_values_are_grouped_by_what_they_mean_in_the_schemas_order() {
    let form = entry_form(&sample(), "factions.toml", "city_watch");
    let names: Vec<Option<&str>> = form.iter().map(|group| group.name.as_deref()).collect();
    assert_eq!(
        names,
        [
            Some("Identity"),
            Some("Where it stands"),
            Some("Membership"),
            Some("Drift"),
            Some("Ranks"),
        ]
    );
    assert_eq!(keys(&form[0]), ["name"]);
    assert_eq!(
        keys(&form[1]),
        [
            "alignment.law",
            "alignment.good",
            "weights.law",
            "weights.good"
        ]
    );
    assert_eq!(
        keys(&form[4]),
        [
            "ranks[0].id",
            "ranks[1].id",
            "ranks[1].requires.standing",
            "ranks[2].id",
            "ranks[2].requires.standing",
            "ranks[2].tolerance",
        ]
    );
    assert_eq!(
        keys(&form[3]),
        ["drift.policy", "drift.grace_ticks", "drift.then"]
    );
    assert_eq!(written(row(&form, "name")), Some("The City Watch"));
    assert_eq!(written(row(&form, "drift.grace_ticks")), Some("100"));
}

#[test]
fn a_key_left_out_shows_its_default_after_what_is_written() {
    let form = entry_form(&sample(), "factions.toml", "city_watch");
    assert_eq!(
        keys(&form[2]),
        [
            "tolerance",
            "member_tolerance",
            "expel_standing_change",
            "leave_standing_change",
            "secret_members",
        ]
    );
    let defaults: Vec<(Option<&str>, Option<&str>)> = form[2]
        .rows
        .iter()
        .map(|row| (written(row), row.default.as_deref()))
        .collect();
    assert_eq!(
        defaults,
        [
            (Some("40.0"), None),
            (Some("50.0"), None),
            (None, Some("-20.00")),
            (None, Some("0.00")),
            (None, Some("false")),
        ]
    );
    let expel = row(&form, "expel_standing_change");
    assert_eq!(expel.path.to_string(), "city_watch.expel_standing_change");
    assert!(
        expel
            .description
            .as_deref()
            .is_some_and(|said| said.contains("expels"))
    );
    // A key with no default isn't shown left out: the Temple's drift has no grace_ticks.
    let temple = entry_form(&sample(), "factions.toml", "temple");
    let drift = temple
        .iter()
        .find(|group| group.name.as_deref() == Some("Drift"))
        .expect("the Temple's drift");
    assert_eq!(keys(drift), ["drift.policy"]);
}

#[test]
fn a_value_has_the_comment_directly_above_it_or_above_its_tables_header() {
    let texts = sample();
    let watch = entry_form(&texts, "factions.toml", "city_watch");
    assert_eq!(
        row(&watch, "weights.law").comment,
        ["Cares about order far more than kindness."]
    );
    assert!(row(&watch, "weights.good").comment.is_empty());
    assert_eq!(
        row(&watch, "drift.policy").comment,
        ["The Watch gives a straying officer a hundred ticks to mend their ways."]
    );
    assert_eq!(
        row(&watch, "ranks[2].tolerance").comment,
        ["Captains are held to a stricter standard than the faction's member tolerance."]
    );
    // The file's opening notes are set apart by a blank line, so they're the file's.
    assert!(row(&watch, "name").comment.is_empty());
    let mira = entry_form(&texts, "characters.toml", "sister_mira");
    assert_eq!(
        row(&mira, "contacts[0]").comment,
        ["She ministers to Brother Ash, hoping to turn him."]
    );
    // One after a value on its line is its too.
    let relation = entry_form(&texts, "relations.toml", "relation[0]");
    assert_eq!(row(&relation, "value").comment, ["enemies, so in conflict"]);
    // A comment above a table's header belongs to its first value.
    let text = "[a]\nname = \"A\"\nalignment = { law = 0.0, good = 0.0 }\ntolerance = 10.0\n\n\
                # The first rung.\n# Everyone starts here.\n[[a.ranks]]\nid = \"one\"\n";
    let mut texts = ContentTexts::default();
    texts[1] = Some(text.to_owned());
    let form = entry_form(&texts, "factions.toml", "a");
    assert_eq!(
        row(&form, "ranks[0].id").comment,
        ["The first rung.", "Everyone starts here."]
    );
}

#[test]
fn a_value_may_be_one_of_the_ids_that_exist_or_of_the_schemas_choices() {
    let texts = sample();
    let watch = entry_form(&texts, "factions.toml", "city_watch");
    assert_eq!(
        row(&watch, "drift.policy").choices,
        ["ignore", "flag", "demote", "expel", "probation"]
    );
    assert_eq!(row(&watch, "drift.then").choices, ["demote", "expel"]);
    assert_eq!(row(&watch, "secret_members").choices, ["false", "true"]);
    assert!(row(&watch, "tolerance").choices.is_empty());
    assert!(row(&watch, "name").choices.is_empty());

    let hale = entry_form(&texts, "characters.toml", "captain_hale");
    assert_eq!(
        row(&hale, "memberships[0].faction").choices,
        [
            "ashen_circle",
            "city_watch",
            "free_company",
            "lantern_guild",
            "temple",
        ]
    );
    assert_eq!(row(&hale, "inertia").choices, ["hardening", "steady"]);
    let secret = row(&hale, "memberships[0].secret");
    assert_eq!(
        (written(secret), secret.default.as_deref()),
        (None, Some("false"))
    );
    // A rank is named within its faction, so it isn't chosen from a list.
    assert!(row(&hale, "memberships[0].rank").choices.is_empty());

    let mira = entry_form(&texts, "characters.toml", "sister_mira");
    assert_eq!(
        row(&mira, "contacts[0]").choices,
        [
            "brother_ash",
            "captain_hale",
            "merchant_ava",
            "player",
            "sister_mira",
            "vex",
        ]
    );
}

#[test]
fn the_groups_of_a_character_and_of_a_list_entry_come_from_their_schemas() {
    let texts = sample();
    let hale = entry_form(&texts, "characters.toml", "captain_hale");
    let names: Vec<Option<&str>> = hale.iter().map(|group| group.name.as_deref()).collect();
    assert_eq!(
        names,
        [
            Some("Identity"),
            Some("Where they stand"),
            Some("Memberships"),
            Some("Standing"),
        ]
    );
    assert_eq!(
        keys(&hale[1]),
        [
            "alignment.law",
            "alignment.good",
            "weights.law",
            "weights.good",
            "inertia",
        ]
    );
    let relation = entry_form(&texts, "relations.toml", "relation[0]");
    assert_eq!(relation[0].name.as_deref(), Some("Factions"));
    assert!(
        relation[0]
            .rows
            .iter()
            .all(|row| row.key.starts_with("between"))
    );
    assert_eq!(relation[0].rows[0].choices.len(), 5);
    assert_eq!(relation[1].name.as_deref(), Some("Regard"));
    // Balance's entries are groups of their own, so their values have none.
    let knowledge = entry_form(&texts, "balance.toml", "knowledge");
    assert_eq!(knowledge.len(), 1);
    assert_eq!(knowledge[0].name, None);
}

#[test]
fn nothing_is_in_the_form_of_an_entry_that_isnt_there() {
    let texts = sample();
    assert!(entry_form(&texts, "factions.toml", "nobody").is_empty());
    assert!(entry_form(&texts, "nowhere.toml", "city_watch").is_empty());
    let mut broken = ContentTexts::default();
    broken[1] = Some("[a\n".to_owned());
    assert!(entry_form(&broken, "factions.toml", "a").is_empty());
}

#[test]
fn every_key_an_entry_may_have_is_in_one_of_its_groups() {
    for (file, definition) in [
        ("factions", "faction"),
        ("characters", "character"),
        ("actions", "action"),
        ("outcomes", "outcome"),
        ("relations", "relation"),
        ("quests", "quest"),
        ("questlines", "questline"),
    ] {
        let schema = schema(file).expect("a schema");
        let definition = &schema["$defs"][definition];
        let groups = definition["x-groups"]
            .as_array()
            .expect("its groups, in order");
        let properties = definition["properties"].as_object().expect("properties");
        for (key, property) in properties {
            assert!(
                groups.contains(&property["x-group"]),
                "{file}: {key} isn't in one of {groups:?}"
            );
        }
    }
}

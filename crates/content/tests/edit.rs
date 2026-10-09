//! Editing content one value at a time, keeping everything else as written (U2, P-74).

use std::fs;
use std::path::Path;

use factional_content::{EditError, ValueKind, ValuePath, entry_fields, outline, set_value};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn broken() -> String {
    fs::read_to_string(
        Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken/characters.toml"),
    )
    .expect("the fixture")
}

fn path(text: &str) -> ValuePath {
    ValuePath::parse(text).expect("a path")
}

#[test]
fn setting_a_value_changes_only_that_value() {
    let before = broken();
    let after = set_value(&before, &path("vex.alignment.law"), "20.0").expect("set");
    assert_eq!(
        after,
        before.replacen("law = 120.0", "law = 20.0", 1),
        "only the number changes"
    );
    assert!(after.starts_with("# Three mistakes, all reported at once."));
}

#[test]
fn text_is_written_as_typed_and_quoted() {
    let after = set_value(&broken(), &path("hale.name"), "Hale").expect("set");
    assert!(after.contains("name = \"Hale\"\n"), "{after}");
    assert!(!after.contains("Captain Hale"));
    let quoted = set_value(&broken(), &path("hale.name"), "The \"Captain\"").expect("set");
    assert!(
        quoted.contains(r#"name = 'The "Captain"'"#)
            || quoted.contains(r#"name = "The \"Captain\"""#),
        "{quoted}"
    );
}

#[test]
fn a_value_is_read_as_the_kind_already_there() {
    assert_eq!(
        set_value(&broken(), &path("vex.alignment.law"), "fast"),
        Err(EditError::Expected(ValueKind::Number))
    );
    assert_eq!(
        set_value(
            "[watch]\nsecret_members = true\n",
            &path("watch.secret_members"),
            "yes"
        ),
        Err(EditError::Expected(ValueKind::Flag))
    );
    assert_eq!(
        set_value(
            "[watch]\nsecret_members = true\n",
            &path("watch.secret_members"),
            "false"
        ),
        Ok("[watch]\nsecret_members = false\n".to_owned())
    );
    assert_eq!(
        set_value(
            "[watch]\nsecret_members = false\n",
            &path("watch.secret_members"),
            "true"
        ),
        Ok("[watch]\nsecret_members = true\n".to_owned())
    );
    assert_eq!(
        set_value(&broken(), &path("vex.alignment.law"), "-5"),
        Ok(broken().replacen("law = 120.0", "law = -5", 1))
    );
}

#[test]
fn errors_say_what_was_expected_and_where() {
    let said = |error: EditError| error.to_string();
    assert_eq!(
        said(EditError::Expected(ValueKind::Number)),
        "expected a number"
    );
    assert_eq!(
        said(EditError::Expected(ValueKind::Flag)),
        "expected true or false"
    );
    assert_eq!(said(EditError::Expected(ValueKind::Text)), "expected text");
    assert_eq!(
        set_value(&broken(), &path("vex.alignment.chaos"), "1").map_err(said),
        Err("there's no value at vex.alignment.chaos".to_owned())
    );
    assert_eq!(
        set_value(&broken(), &path("vex.alignment"), "1").map_err(said),
        Err("there's no value at vex.alignment".to_owned())
    );
    let refused = set_value("[vex", &path("vex.name"), "V").map_err(said);
    assert!(refused.is_err_and(|message| message.starts_with("the file isn't valid TOML")));
}

#[test]
fn paths_name_keys_and_places_in_lists() {
    for text in [
        "vex.alignment.law",
        "relation[2].value",
        "watch.ranks[0].id",
        "x",
    ] {
        assert_eq!(path(text).to_string(), text);
    }
    for text in ["", "vex..law", "relation[x]", "relation[2", ".vex"] {
        assert_eq!(ValuePath::parse(text), None, "{text}");
    }
    let relations = "[[relation]]\nbetween = [\"a\", \"b\"]\nvalue = 1.0\n\n[[relation]]\nfrom = \"a\"\nto = \"b\"\nvalue = 2.0 # friendly\n";
    assert_eq!(
        set_value(relations, &path("relation[1].value"), "3.5"),
        Ok(relations.replacen("value = 2.0 # friendly", "value = 3.5 # friendly", 1))
    );
    assert_eq!(
        set_value(relations, &path("relation[0].between[1]"), "c"),
        Ok(relations.replacen("[\"a\", \"b\"]", "[\"a\", \"c\"]", 1))
    );
    assert_eq!(
        set_value(relations, &path("relation[2].value"), "3.5"),
        Err(EditError::NotThere(path("relation[2].value")))
    );
}

#[test]
fn an_entrys_fields_are_every_value_it_holds_in_the_order_written() {
    let fields = entry_fields(&broken(), "vex");
    let listed: Vec<(String, ValueKind, &str)> = fields
        .iter()
        .map(|field| (field.path.to_string(), field.kind, field.value.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("vex.name".to_owned(), ValueKind::Text, "Vex"),
            ("vex.alignment.law".to_owned(), ValueKind::Number, "120.0"),
            ("vex.alignment.good".to_owned(), ValueKind::Number, "-20.0"),
        ]
    );
    let factions = "[watch]\nname = \"W\"\nsecret_members = true\n\n[[watch.ranks]]\nid = \"recruit\"\n\n[[watch.ranks]]\nid = \"sergeant\"\nrequires = { standing = 30.0 }\n";
    let paths: Vec<String> = entry_fields(factions, "watch")
        .iter()
        .map(|field| field.path.to_string())
        .collect();
    assert_eq!(
        paths,
        [
            "watch.name",
            "watch.secret_members",
            "watch.ranks[0].id",
            "watch.ranks[1].id",
            "watch.ranks[1].requires.standing",
        ]
    );
    let relations = "[[relation]]\nbetween = [\"a\", \"b\"]\nvalue = 1.0\n";
    let paths: Vec<String> = entry_fields(relations, "relation[0]")
        .iter()
        .map(|field| field.path.to_string())
        .collect();
    assert_eq!(
        paths,
        [
            "relation[0].between[0]",
            "relation[0].between[1]",
            "relation[0].value"
        ]
    );
    assert!(entry_fields(relations, "relation[1]").is_empty());
    assert!(entry_fields("[x", "x").is_empty());
}

#[test]
fn the_outline_shows_each_entry_as_written() {
    let outline = outline(&Path::new(REPO).join("content/sample"));
    let outcomes = outline
        .files
        .iter()
        .find(|f| f.name == "outcomes.toml")
        .expect("there");
    let vex = outcomes
        .entries
        .iter()
        .find(|e| e.key == "turned_in_vex")
        .expect("there");
    assert!(
        vex.toml
            .contains("# The Watch's Oath (quests.toml): turning in a thief"),
        "{}",
        vex.toml
    );
}

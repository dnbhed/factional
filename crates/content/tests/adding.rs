//! Adding and removing keys, entries and list items, with the schema's keys to choose from,
//! keeping everything else as written (U3, P-78).

use std::fs;
use std::path::Path;

use factional_content::{
    Addition, EditError, ValuePath, add, additions, entry_places, file_places, outline, remove,
};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The broken fixture's characters: Vex out of range and Hale's alignment misspelt.
fn broken() -> String {
    fs::read_to_string(
        Path::new(REPO).join("crates/cli/tests/fixtures/worlds/broken/characters.toml"),
    )
    .expect("the fixture")
}

fn path(text: &str) -> ValuePath {
    ValuePath::parse(text).expect("a path")
}

fn keys(names: &[&str]) -> Vec<Addition> {
    names
        .iter()
        .map(|name| Addition::Key((*name).to_owned()))
        .collect()
}

const CHARACTERS: &str = "characters.toml";
const RELATIONS: &str = "relations.toml";

const AVA: &str = "[ava]\nname = \"Ava\"\nalignment = { law = 0.0, good = 0.0 }\nmemberships = [{ faction = \"watch\" }]\nstanding = { factions = { watch = 5.0 } }\n";

const TWO_RELATIONS: &str = "# Relations.\n\n[[relation]]\nbetween = [\"a\", \"b\"]\nvalue = 1.0\n\n[[relation]]\nfrom = \"a\"\nto = \"b\"\nvalue = 2.0 # friendly\n";

#[test]
fn a_file_of_tables_by_id_takes_a_new_id() {
    assert_eq!(
        additions(CHARACTERS, &broken(), &ValuePath::ROOT),
        [Addition::Id]
    );
    assert_eq!(additions(CHARACTERS, "", &ValuePath::ROOT), [Addition::Id]);
}

#[test]
fn a_table_offers_the_schemas_keys_not_yet_there_in_its_order() {
    assert_eq!(
        additions(CHARACTERS, &broken(), &path("vex")),
        keys(&["contacts", "inertia", "memberships", "standing", "weights"])
    );
    assert_eq!(
        additions(CHARACTERS, &broken(), &path("hale")),
        keys(&[
            "alignment",
            "contacts",
            "inertia",
            "memberships",
            "standing",
            "weights"
        ]),
        "the misspelt key isn't the schema's, so alignment is still missing"
    );
    assert_eq!(additions(CHARACTERS, &broken(), &path("vex.alignment")), []);
    assert_eq!(
        additions(CHARACTERS, &broken(), &path("hale.alignmnet")),
        [],
        "the schema knows nothing under a key it doesn't have"
    );
    assert_eq!(
        additions("balance.toml", "", &ValuePath::ROOT),
        keys(&[
            "alignment",
            "disposition",
            "inertia",
            "knowledge",
            "membership",
            "relations",
            "standing"
        ])
    );
}

#[test]
fn inline_tables_lists_and_tables_by_id_each_offer_what_fits() {
    assert_eq!(
        additions(CHARACTERS, AVA, &path("ava.memberships")),
        [Addition::Item]
    );
    assert_eq!(
        additions(CHARACTERS, AVA, &path("ava.memberships[0]")),
        keys(&["rank", "secret"])
    );
    assert_eq!(
        additions(CHARACTERS, AVA, &path("ava.standing")),
        keys(&["characters"])
    );
    assert_eq!(
        additions(CHARACTERS, AVA, &path("ava.standing.factions")),
        [Addition::Id]
    );
    assert_eq!(additions(CHARACTERS, AVA, &path("ava.name")), []);
    assert_eq!(additions(CHARACTERS, AVA, &path("ava.nothing")), []);
}

#[test]
fn a_list_offers_an_item_until_it_holds_as_many_as_it_may() {
    assert_eq!(
        additions(RELATIONS, TWO_RELATIONS, &path("relation")),
        [Addition::Item]
    );
    assert_eq!(
        additions(RELATIONS, TWO_RELATIONS, &path("relation[0].between")),
        [],
        "between holds two factions at most"
    );
    assert_eq!(
        additions(
            RELATIONS,
            "[[relation]]\nbetween = [\"a\"]\n",
            &path("relation[0].between")
        ),
        [Addition::Item]
    );
    assert_eq!(
        additions(RELATIONS, TWO_RELATIONS, &ValuePath::ROOT),
        [],
        "the only key is there"
    );
    assert_eq!(
        additions(RELATIONS, "", &ValuePath::ROOT),
        keys(&["relation"])
    );
}

#[test]
fn nothing_can_be_added_to_what_cant_be_read() {
    assert_eq!(additions(CHARACTERS, "[vex", &ValuePath::ROOT), []);
    assert_eq!(additions("notes.toml", "", &ValuePath::ROOT), []);
}

#[test]
fn new_entries_go_at_the_top_and_at_the_top_level_lists_of_tables() {
    assert_eq!(file_places(&broken()), [ValuePath::ROOT]);
    assert_eq!(file_places(""), [ValuePath::ROOT]);
    assert_eq!(
        file_places(TWO_RELATIONS),
        [ValuePath::ROOT, path("relation")]
    );
    assert_eq!(file_places("[vex"), []);
}

#[test]
fn an_entrys_places_are_it_and_every_table_and_list_in_it_in_the_order_written() {
    assert_eq!(
        entry_places(&broken(), "vex"),
        [path("vex"), path("vex.alignment")]
    );
    let factions = "[watch]\nname = \"W\"\n\n[[watch.ranks]]\nid = \"recruit\"\n\n[[watch.ranks]]\nid = \"sergeant\"\nrequires = { standing = 30.0 }\n";
    assert_eq!(
        entry_places(factions, "watch"),
        [
            path("watch"),
            path("watch.ranks"),
            path("watch.ranks[0]"),
            path("watch.ranks[1]"),
            path("watch.ranks[1].requires"),
        ]
    );
    assert_eq!(
        entry_places(AVA, "ava"),
        [
            path("ava"),
            path("ava.alignment"),
            path("ava.memberships"),
            path("ava.memberships[0]"),
            path("ava.standing"),
            path("ava.standing.factions"),
        ]
    );
    assert_eq!(
        entry_places(TWO_RELATIONS, "relation[0]"),
        [path("relation[0]"), path("relation[0].between")]
    );
    assert_eq!(
        entry_places("label_threshold = 33.0\n", "label_threshold"),
        [path("label_threshold")]
    );
    assert_eq!(entry_places(TWO_RELATIONS, "relation[2]"), []);
    assert_eq!(entry_places(&broken(), "nobody"), []);
    assert_eq!(entry_places("[x", "x"), []);
}

#[test]
fn a_new_entry_holds_what_the_schema_requires_at_its_starting_values() {
    let before = broken();
    assert_eq!(
        add(CHARACTERS, &before, &ValuePath::ROOT, Some("ava")),
        Ok((
            format!("{before}\n[ava]\nname = \"\"\nalignment = {{ law = 0.0, good = 0.0 }}\n"),
            path("ava")
        ))
    );
    assert_eq!(
        add(CHARACTERS, "", &ValuePath::ROOT, Some("ava")),
        Ok((
            "[ava]\nname = \"\"\nalignment = { law = 0.0, good = 0.0 }\n".to_owned(),
            path("ava")
        ))
    );
    assert_eq!(
        add("factions.toml", "", &ValuePath::ROOT, Some("guild")),
        Ok((
            "[guild]\nname = \"\"\nalignment = { law = 0.0, good = 0.0 }\ntolerance = 0.0\nranks = [{ id = \"\" }]\n"
                .to_owned(),
            path("guild")
        )),
        "inside an entry, everything new is inline; a ladder needs a rung"
    );
}

#[test]
fn a_new_relation_is_a_relation_table_with_the_first_form() {
    assert_eq!(
        add(RELATIONS, TWO_RELATIONS, &path("relation"), None),
        Ok((
            format!("{TWO_RELATIONS}\n[[relation]]\nvalue = 0.0\nbetween = [\"\", \"\"]\n"),
            path("relation[2]")
        ))
    );
    assert_eq!(
        add(RELATIONS, "", &ValuePath::ROOT, Some("relation")),
        Ok((
            "[[relation]]\nvalue = 0.0\nbetween = [\"\", \"\"]\n".to_owned(),
            path("relation[0]")
        )),
        "a list of tables at the top starts with its first table"
    );
}

#[test]
fn a_new_key_is_written_inline_at_its_starting_value() {
    let before = broken();
    let (with_standing, at) =
        add(CHARACTERS, &before, &path("vex"), Some("standing")).expect("added");
    assert_eq!(at, path("vex.standing"));
    assert_eq!(
        with_standing,
        before.replacen("good = -20.0 }\n", "good = -20.0 }\nstanding = {}\n", 1)
    );
    let (with_factions, _) = add(
        CHARACTERS,
        &with_standing,
        &path("vex.standing"),
        Some("factions"),
    )
    .expect("added");
    assert_eq!(
        with_factions,
        with_standing.replacen("standing = {}", "standing = { factions = {} }", 1)
    );
    let (with_watch, at) = add(
        CHARACTERS,
        &with_factions,
        &path("vex.standing.factions"),
        Some("city_watch"),
    )
    .expect("added");
    assert_eq!(at, path("vex.standing.factions.city_watch"));
    assert_eq!(
        with_watch,
        with_standing.replacen(
            "standing = {}",
            "standing = { factions = { city_watch = 0.0 } }",
            1
        )
    );
}

#[test]
fn a_new_table_among_tables_with_their_own_headers_gets_one_too() {
    let inertia = "[inertia]\ndefault_profile = \"steady\"\n\n[inertia.profiles.steady]\n";
    assert_eq!(
        add(
            "balance.toml",
            inertia,
            &path("inertia.profiles"),
            Some("calm")
        ),
        Ok((
            format!("{inertia}\n[inertia.profiles.calm]\n"),
            path("inertia.profiles.calm")
        ))
    );
}

#[test]
fn a_starting_value_is_the_default_else_the_least_the_schema_allows() {
    let added = |file: &str, text: &str, at: &str, key: &str| {
        add(file, text, &path(at), Some(key)).map(|(text, _)| text)
    };
    let watch = "[watch]\nname = \"W\"\n";
    assert_eq!(
        added("factions.toml", watch, "watch", "secret_members"),
        Ok(format!("{watch}secret_members = false\n"))
    );
    assert_eq!(
        added("factions.toml", watch, "watch", "expel_standing_change"),
        Ok(format!("{watch}expel_standing_change = -20.0\n"))
    );
    assert_eq!(
        added("factions.toml", watch, "watch", "member_tolerance"),
        Ok(format!("{watch}member_tolerance = 0.0\n"))
    );
    assert_eq!(
        added(CHARACTERS, watch, "watch", "inertia"),
        Ok(format!("{watch}inertia = \"\"\n"))
    );
    assert_eq!(
        added(CHARACTERS, watch, "watch", "contacts"),
        Ok(format!("{watch}contacts = []\n"))
    );
    assert_eq!(
        added(
            CHARACTERS,
            "[vex]\nalignment = { law = 120.0 }\n",
            "vex.alignment",
            "good"
        ),
        Ok("[vex]\nalignment = { law = 120.0, good = 0.0 }\n".to_owned())
    );
}

#[test]
fn a_choice_of_forms_is_read_as_what_is_written() {
    let curve = "[disposition]\naffinity = [[0.0, 50.0]]\n";
    assert_eq!(
        additions("balance.toml", curve, &path("disposition.affinity")),
        [Addition::Item],
        "a curve written as points takes another"
    );
    assert_eq!(
        additions("balance.toml", curve, &path("disposition.affinity[0]")),
        [],
        "a point is x and y"
    );
    assert_eq!(
        add("balance.toml", curve, &path("disposition.affinity"), None),
        Ok((
            "[disposition]\naffinity = [[0.0, 50.0], [0.0, 0.0]]\n".to_owned(),
            path("disposition.affinity[1]")
        ))
    );
    let rules = "[membership.defectors]\nrules = [{ when = {}, then = \"accept\" }]\n";
    let when = path("membership.defectors.rules[0].when");
    assert_eq!(
        add("balance.toml", rules, &when, Some("rank_at_least")).map(|(text, _)| text),
        Ok(rules.replacen("when = {}", "when = { rank_at_least = 1 }", 1)),
        "a rung is a number from 1 or a rank's id; the first form, from its least"
    );
    assert_eq!(
        add("balance.toml", rules, &when, Some("closer_to_target")).map(|(text, _)| text),
        Ok(rules.replacen("when = {}", "when = { closer_to_target = false }", 1))
    );
}

#[test]
fn a_new_list_item_follows_the_items_before_it() {
    assert_eq!(
        add(CHARACTERS, AVA, &path("ava.memberships"), None),
        Ok((
            AVA.replacen(
                "[{ faction = \"watch\" }]",
                "[{ faction = \"watch\" }, { faction = \"\" }]",
                1
            ),
            path("ava.memberships[1]")
        ))
    );
    let contacts = "[vex]\ncontacts = [ \"a\", \"b\" ]\n";
    assert_eq!(
        add(CHARACTERS, contacts, &path("vex.contacts"), None).map(|(text, _)| text),
        Ok("[vex]\ncontacts = [ \"a\", \"b\", \"\" ]\n".to_owned())
    );
    let lines = "[vex]\ncontacts = [\n  \"a\",\n]\n";
    assert_eq!(
        add(CHARACTERS, lines, &path("vex.contacts"), None).map(|(text, _)| text),
        Ok("[vex]\ncontacts = [\n  \"a\",\n  \"\",\n]\n".to_owned())
    );
    let tight = "[vex]\ncontacts = [\"a\",\"b\"]\n";
    assert_eq!(
        add(CHARACTERS, tight, &path("vex.contacts"), None).map(|(text, _)| text),
        Ok("[vex]\ncontacts = [\"a\",\"b\",\"\"]\n".to_owned())
    );
    let empty = "[vex]\ncontacts = []\n";
    assert_eq!(
        add(CHARACTERS, empty, &path("vex.contacts"), None).map(|(text, _)| text),
        Ok("[vex]\ncontacts = [\"\"]\n".to_owned())
    );
}

#[test]
fn adding_refuses_what_isnt_there_already_there_or_not_the_schemas() {
    let broken = broken();
    let refused = |at: &str, key: Option<&str>| {
        let at = if at.is_empty() {
            ValuePath::ROOT
        } else {
            path(at)
        };
        add(CHARACTERS, &broken, &at, key).map_err(|error| error.to_string())
    };
    assert_eq!(
        refused("vex", Some("name")),
        Err("vex.name is already there".to_owned())
    );
    assert_eq!(
        refused("", Some("vex")),
        Err("vex is already there".to_owned())
    );
    assert_eq!(
        refused("vex", Some("speed")),
        Err("'speed' isn't a key here".to_owned())
    );
    assert_eq!(
        refused("", Some("")),
        Err("a key can't be blank".to_owned())
    );
    assert_eq!(
        refused("", Some("  ")),
        Err("a key can't be blank".to_owned())
    );
    assert_eq!(
        refused("vex.name", Some("x")),
        Err("nothing can be added at vex.name".to_owned())
    );
    assert_eq!(
        refused("vex", None),
        Err("nothing can be added at vex".to_owned())
    );
    assert_eq!(
        refused("nobody", Some("name")),
        Err("there's no value at nobody".to_owned())
    );
    assert_eq!(
        add(RELATIONS, TWO_RELATIONS, &path("relation[0].between"), None),
        Err(EditError::CantAdd(path("relation[0].between")))
    );
    assert_eq!(
        add(RELATIONS, TWO_RELATIONS, &path("relation"), Some("x")),
        Err(EditError::CantAdd(path("relation")))
    );
    assert!(matches!(
        add(CHARACTERS, "[vex", &ValuePath::ROOT, Some("ava")),
        Err(EditError::NotToml(_))
    ));
    assert_eq!(
        add("notes.toml", "", &ValuePath::ROOT, Some("x")),
        Err(EditError::Unknown("x".to_owned()))
    );
}

#[test]
fn removing_an_entry_takes_the_comments_directly_above_it() {
    let tables = "[a]\nx = 1\n\n# About b.\n[b]\ny = 2\n\n[c]\nz = 3\n";
    assert_eq!(
        remove(tables, &path("b")),
        Ok("[a]\nx = 1\n\n[c]\nz = 3\n".to_owned())
    );
    assert_eq!(
        remove(tables, &path("c")),
        Ok("[a]\nx = 1\n\n# About b.\n[b]\ny = 2\n".to_owned())
    );
    assert_eq!(
        remove(tables, &path("a")),
        Ok("# About b.\n[b]\ny = 2\n\n[c]\nz = 3\n".to_owned())
    );
}

#[test]
fn a_files_opening_comments_stay_with_the_file() {
    let broken = broken();
    assert_eq!(
        remove(&broken, &path("vex")),
        Ok("# Three mistakes, all reported at once.\n\n[hale]\nname = \"Captain Hale\"\nalignmnet = { law = 75.0, good = 30.0 }\n".to_owned())
    );
    assert_eq!(
        remove(&broken, &path("hale")),
        Ok("# Three mistakes, all reported at once.\n\n[vex]\nname = \"Vex\"\nalignment = { law = 120.0, good = -20.0 }\n".to_owned())
    );
    assert_eq!(
        remove(TWO_RELATIONS, &path("relation[0]")),
        Ok(
            "# Relations.\n\n[[relation]]\nfrom = \"a\"\nto = \"b\"\nvalue = 2.0 # friendly\n"
                .to_owned()
        )
    );
    let one = remove(TWO_RELATIONS, &path("relation[0]")).expect("removed");
    assert_eq!(
        remove(&one, &path("relation[0]")),
        Ok("# Relations.\n".to_owned()),
        "a list of tables left empty goes, since TOML can't write it"
    );
}

#[test]
fn removing_a_key_takes_its_line_and_the_comments_directly_above_it() {
    let hale = "[hale]\nname = \"H\"\n# Hardened.\ninertia = \"hardening\"\nweights = { law = 1.0, good = 0.25 }\n";
    assert_eq!(
        remove(hale, &path("hale.inertia")),
        Ok("[hale]\nname = \"H\"\nweights = { law = 1.0, good = 0.25 }\n".to_owned())
    );
    let kept = "[hale]\nname = \"H\"\n\n# The Watch's.\nweights = { law = 1.0, good = 0.25 }\n\n[vex]\nname = \"V\"\n";
    assert_eq!(
        remove(kept, &path("hale.weights")),
        Ok("[hale]\nname = \"H\"\n\n[vex]\nname = \"V\"\n".to_owned())
    );
    let notes = "[hale]\nname = \"H\"\n\n# Notes on Hale.\n\ninertia = \"x\"\nweights = { law = 1.0, good = 0.25 }\n\n[vex]\nname = \"V\"\n";
    assert_eq!(
        remove(notes, &path("hale.inertia")),
        Ok("[hale]\nname = \"H\"\n\n# Notes on Hale.\n\nweights = { law = 1.0, good = 0.25 }\n\n[vex]\nname = \"V\"\n".to_owned()),
        "comments set apart stay, above the next key"
    );
    let last = remove(notes, &path("hale.inertia")).expect("removed");
    assert_eq!(
        remove(&last, &path("hale.weights")),
        Ok("[hale]\nname = \"H\"\n\n# Notes on Hale.\n\n[vex]\nname = \"V\"\n".to_owned()),
        "or, after the last key, above the next header"
    );
    let watch = "[watch]\nname = \"W\"\n\n[[watch.ranks]]\nid = \"r\"\n";
    assert_eq!(
        remove(watch, &path("watch.ranks[0]")),
        Ok("[watch]\nname = \"W\"\n".to_owned())
    );
}

#[test]
fn removing_from_an_inline_table_or_list_keeps_its_spacing() {
    let broken = broken();
    assert_eq!(
        remove(&broken, &path("vex.alignment.good")),
        Ok(broken.replacen("{ law = 120.0, good = -20.0 }", "{ law = 120.0 }", 1))
    );
    assert_eq!(
        remove(&broken, &path("vex.alignment.law")),
        Ok(broken.replacen("{ law = 120.0, good = -20.0 }", "{ good = -20.0 }", 1))
    );
    let contacts = "[vex]\ncontacts = [\"a\", \"b\", \"c\"]\n";
    assert_eq!(
        remove(contacts, &path("vex.contacts[0]")),
        Ok("[vex]\ncontacts = [\"b\", \"c\"]\n".to_owned())
    );
    assert_eq!(
        remove(contacts, &path("vex.contacts[2]")),
        Ok("[vex]\ncontacts = [\"a\", \"b\"]\n".to_owned())
    );
    let spaced = "[vex]\ncontacts = [ \"a\", \"b\" ]\n";
    assert_eq!(
        remove(spaced, &path("vex.contacts[1]")),
        Ok("[vex]\ncontacts = [ \"a\" ]\n".to_owned())
    );
    let lines = "[q]\nchoices = [\n  { id = \"a\" },\n  { id = \"b\" },\n]\n";
    assert_eq!(
        remove(lines, &path("q.choices[0]")),
        Ok("[q]\nchoices = [\n  { id = \"b\" },\n]\n".to_owned())
    );
}

#[test]
fn removing_refuses_what_isnt_there() {
    let broken = broken();
    for missing in [
        "nobody",
        "vex.speed",
        "vex.alignment.chaos",
        "vex.name.x",
        "vex[0]",
    ] {
        assert_eq!(
            remove(&broken, &path(missing)),
            Err(EditError::NotThere(path(missing))),
            "{missing}"
        );
    }
    assert_eq!(
        remove(TWO_RELATIONS, &path("relation[2]")),
        Err(EditError::NotThere(path("relation[2]")))
    );
    assert_eq!(
        remove("[vex]\ncontacts = [\"a\"]\n", &path("vex.contacts[1]")),
        Err(EditError::NotThere(path("vex.contacts[1]")))
    );
    assert_eq!(
        remove(&broken, &ValuePath::ROOT),
        Err(EditError::NotThere(ValuePath::ROOT))
    );
    assert!(matches!(
        remove("[vex", &path("vex")),
        Err(EditError::NotToml(_))
    ));
}

#[test]
fn every_addition_offered_in_the_sample_comes_out_again_leaving_the_file_as_it_was() {
    let sample = Path::new(REPO).join("content/sample");
    let mut made = 0;
    for file in outline(&sample).files {
        let text = fs::read_to_string(sample.join(file.name)).expect("the sample's");
        let entries = file
            .entries
            .iter()
            .map(|entry| entry_places(&text, &entry.key));
        for place in file_places(&text).into_iter().chain(entries.flatten()) {
            for addition in additions(file.name, &text, &place) {
                let key = match &addition {
                    Addition::Key(key) => Some(key.as_str()),
                    Addition::Id => Some("new_id"),
                    Addition::Item => None,
                };
                let what = format!("{} {place} {addition:?}", file.name);
                let (added, at) = add(file.name, &text, &place, key).expect(&what);
                if let Addition::Key(_) = addition {
                    let offered = additions(file.name, &added, &place);
                    assert!(!offered.contains(&addition), "{what}: still offered");
                }
                assert_eq!(remove(&added, &at).as_deref(), Ok(text.as_str()), "{what}");
                made += 1;
            }
        }
    }
    assert!(made > 100, "only {made} additions offered");
}

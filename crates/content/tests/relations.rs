//! Relations in each faction (U12a, D-37, P-91): each faction's `relations` in `factions.toml`
//! says how it regards each other faction, one direction each; a `relations.toml` still in a
//! directory is a problem, so an old directory isn't read as if it had none.

use std::fs;
use std::path::{Path, PathBuf};

use factional_content::{
    ContentTexts, Diagnostic, Regard, RelationRow, ValuePath, is_both_ways, load_dir, outline,
    read_texts, relation_rows, set_relation,
};
use factional_core::Fixed;
use factional_reputation::{FactionId, Relation, RelationEnds};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// A fresh copy of content/sample, at `<tmp>/<name>`.
fn sample_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for entry in fs::read_dir(Path::new(REPO).join("content/sample")).expect("the sample") {
        let path = entry.expect("an entry").path();
        fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
    }
    dir
}

fn one_way(from: &str, to: &str, value: &str) -> Relation {
    let id = |id: &str| FactionId::new(id).expect("a valid id");
    Relation {
        ends: RelationEnds::Directed {
            from: id(from),
            to: id(to),
        },
        value: value.parse::<Fixed>().expect("a number"),
    }
}

#[test]
fn riverholds_relations_are_written_in_its_factions_one_way_each() {
    let content = load_dir(&Path::new(REPO).join("content/sample")).expect("the sample loads");
    // DESIGN.md §13: each pair as Riverhold has it, both ways, and the Watch's grudge against
    // the Free Company deeper than theirs against it.
    let expected = [
        one_way("ashen_circle", "city_watch", "-40.0"),
        one_way("ashen_circle", "temple", "-90.0"),
        one_way("city_watch", "ashen_circle", "-40.0"),
        one_way("city_watch", "free_company", "-30.0"),
        one_way("city_watch", "lantern_guild", "-80.0"),
        one_way("city_watch", "temple", "60.0"),
        one_way("free_company", "city_watch", "-10.0"),
        one_way("free_company", "lantern_guild", "20.0"),
        one_way("lantern_guild", "city_watch", "-80.0"),
        one_way("lantern_guild", "free_company", "20.0"),
        one_way("temple", "ashen_circle", "-90.0"),
        one_way("temple", "city_watch", "60.0"),
    ];
    assert_eq!(content.relations, expected);
}

#[test]
fn a_relations_toml_left_in_a_directory_is_a_problem_saying_where_relations_live_now() {
    let dir = sample_copy("stale_relations");
    fs::write(
        dir.join("relations.toml"),
        "[[relation]]\nbetween = [\"city_watch\", \"temple\"]\nvalue = 60.0\n",
    )
    .expect("written");
    let problem = Diagnostic {
        file: "relations.toml".to_owned(),
        key: None,
        message: "relations now live in each faction, in factions.toml, as \
                  relations = { temple = 60.0 }: move them there and remove this file"
            .to_owned(),
        suggestion: None,
    };
    let error = load_dir(&dir).expect_err("an old directory doesn't load");
    assert_eq!(error.diagnostics, std::slice::from_ref(&problem));
    assert_eq!(outline(&dir).problems, [problem]);
}

// Both ways (U12b, D-38)

fn sample() -> ContentTexts {
    read_texts(&Path::new(REPO).join("content/sample")).expect("the sample is there")
}

fn regard(written: Option<&str>, value: &str, band: &str) -> Regard {
    Regard {
        written: written.map(str::to_owned),
        value: Some(value.parse::<Fixed>().expect("a number")),
        band: Some(band.to_owned()),
    }
}

#[test]
fn a_factions_page_has_every_other_faction_both_ways() {
    let rows = relation_rows(&sample(), "temple");
    let zero = || regard(None, "0", "neutral");
    assert_eq!(
        rows,
        [
            // DESIGN.md §13: the Temple and the Circle are enemies, so in conflict.
            RelationRow {
                other: "ashen_circle".to_owned(),
                name: Some("The Ashen Circle".to_owned()),
                regard: regard(Some("-90.0"), "-90", "enemy"),
                back: Some(regard(Some("-90.0"), "-90", "enemy")),
                conflict: true,
            },
            RelationRow {
                other: "city_watch".to_owned(),
                name: Some("The City Watch".to_owned()),
                regard: regard(Some("60.0"), "60", "allied"),
                back: Some(regard(Some("60.0"), "60", "allied")),
                conflict: false,
            },
            // Nothing written either way is 0, neutral.
            RelationRow {
                other: "free_company".to_owned(),
                name: Some("The Free Company".to_owned()),
                regard: zero(),
                back: Some(zero()),
                conflict: false,
            },
            RelationRow {
                other: "lantern_guild".to_owned(),
                name: Some("The Lantern Guild".to_owned()),
                regard: zero(),
                back: Some(zero()),
                conflict: false,
            },
        ]
    );
    assert!(rows.iter().all(RelationRow::linked));
}

#[test]
fn a_grudge_deeper_one_way_isnt_linked() {
    let rows = relation_rows(&sample(), "city_watch");
    let company = rows
        .iter()
        .find(|row| row.other == "free_company")
        .expect("the Free Company");
    assert_eq!(company.regard, regard(Some("-30.0"), "-30", "rival"));
    assert_eq!(company.back, Some(regard(Some("-10.0"), "-10", "neutral")));
    assert!(!company.linked());
    assert!(!company.conflict);
    // The Watch and the Guild regard each other at -80, below the -50 threshold.
    let guild = rows.iter().find(|row| row.other == "lantern_guild");
    assert!(guild.is_some_and(|guild| guild.conflict && guild.linked()));
    assert_eq!(rows.len(), 4, "every other faction, not the Watch itself");
}

/// `factions.toml` with just `text`, and the sample's other files.
fn factions(text: &str) -> ContentTexts {
    let mut texts = sample();
    texts[1] = Some(text.to_owned());
    texts
}

const TWO: &str = "[a]\nname = \"A\"\nalignment = { law = 0.0, good = 0.0 }\ntolerance = 10.0\n\n\
                   [b]\nname = \"B\"\nalignment = { law = 0.0, good = 0.0 }\ntolerance = 10.0\n";

#[test]
fn an_id_written_that_names_no_faction_is_a_row_with_no_way_back() {
    let text = TWO.replace(
        "tolerance = 10.0\n\n",
        "tolerance = 10.0\nrelations = { c = 5.0, b = \"x\" }\n\n",
    );
    let rows = relation_rows(&factions(&text), "a");
    assert_eq!(
        rows,
        [
            RelationRow {
                other: "b".to_owned(),
                name: Some("B".to_owned()),
                // What isn't a number has no value, nor a band.
                regard: Regard {
                    written: Some("x".to_owned()),
                    value: None,
                    band: None,
                },
                back: Some(regard(None, "0", "neutral")),
                conflict: false,
            },
            RelationRow {
                other: "c".to_owned(),
                name: None,
                regard: regard(Some("5.0"), "5", "neutral"),
                back: None,
                conflict: false,
            },
        ]
    );
    assert!(!rows[1].linked(), "with no way back, nothing to link");
}

#[test]
fn the_bands_and_threshold_are_balances_own() {
    let mut texts = sample();
    texts[0] = Some(
        "[relations]\nconflict_threshold = -95.0\nbands = [{ name = \"foe\", up_to = 0.0 }, { name = \"friend\" }]\n"
            .to_owned(),
    );
    let rows = relation_rows(&texts, "temple");
    assert_eq!(rows[0].regard.band.as_deref(), Some("foe"));
    assert!(!rows[0].conflict, "-90 isn't at or below -95");
    // A balance that doesn't read gives the defaults.
    texts[0] = Some("[relations\n".to_owned());
    assert_eq!(
        relation_rows(&texts, "temple")[0].regard.band.as_deref(),
        Some("enemy")
    );
}

#[test]
fn a_relation_is_set_one_way_or_both_in_one_text() {
    let both = set_relation(TWO, "a", "b", "25.0", true).expect("set");
    let read: toml::Table = both.parse().expect("toml");
    assert_eq!(read["a"]["relations"]["b"], toml::Value::Float(25.0));
    assert_eq!(read["b"]["relations"]["a"], toml::Value::Float(25.0));
    let one = set_relation(&both, "a", "b", "-40.0", false).expect("set");
    let read: toml::Table = one.parse().expect("toml");
    assert_eq!(read["a"]["relations"]["b"], toml::Value::Float(-40.0));
    assert_eq!(read["b"]["relations"]["a"], toml::Value::Float(25.0));
    // The rest of the text is as it was.
    assert!(one.starts_with("[a]\nname = \"A\"\n"), "{one}");
    // An entry that isn't there is refused, and nothing is written.
    assert!(set_relation(TWO, "a", "nobody", "1.0", true).is_err());
}

#[test]
fn only_a_faction_has_relations_and_only_its_relations_are_read_both_ways() {
    let texts = sample();
    assert_eq!(relation_rows(&texts, "vex"), []);
    assert_eq!(relation_rows(&texts, "nobody"), []);
    let path = |text: &str| ValuePath::parse(text).expect("a path");
    assert!(is_both_ways("factions.toml", &path("temple.relations")));
    assert!(is_both_ways(
        "factions.toml",
        &path("temple.relations.city_watch")
    ));
    for (file, at) in [
        ("factions.toml", "temple"),
        ("factions.toml", "temple.ranks"),
        ("characters.toml", "vex.standing.factions"),
        ("nowhere.toml", "temple.relations"),
    ] {
        assert!(!is_both_ways(file, &path(at)), "{file}: {at}");
    }
}

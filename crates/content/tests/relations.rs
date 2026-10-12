//! Relations in each faction (U12a, D-37, P-91): each faction's `relations` in `factions.toml`
//! says how it regards each other faction, one direction each; a `relations.toml` still in a
//! directory is a problem, so an old directory isn't read as if it had none.

use std::fs;
use std::path::{Path, PathBuf};

use factional_content::{Diagnostic, load_dir, outline};
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

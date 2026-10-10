//! The loader's fixes (U6c): every "did you mean" as data, the change that makes it, and
//! the writer making it, keeping everything else as written.

use std::path::Path;

use factional_content::{
    Change, ContentTexts, Diagnostic, EditError, Fix, ValuePath, apply_fix, fix_for, load_texts,
    read_texts, rename_key,
};
use factional_core::Suggestion;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn world(dir: &str) -> ContentTexts {
    read_texts(&Path::new(REPO).join(dir)).expect("the directory is there")
}

fn typos() -> ContentTexts {
    world("crates/cli/tests/fixtures/worlds/typos")
}

fn problems(texts: &ContentTexts) -> Vec<Diagnostic> {
    load_texts(texts)
        .err()
        .map(|e| e.diagnostics)
        .unwrap_or_default()
}

fn path(text: &str) -> ValuePath {
    ValuePath::parse(text).expect("a path")
}

fn place(file: &str) -> usize {
    factional_content::CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file")
}

fn suggestion(wrong: &str, right: &str) -> Option<Suggestion> {
    Some(Suggestion {
        wrong: wrong.to_owned(),
        right: right.to_owned(),
    })
}

#[test]
fn every_did_you_mean_comes_with_its_suggestion_as_data() {
    let found: Vec<(String, Option<Suggestion>)> = problems(&typos())
        .into_iter()
        .map(|d| (d.to_string(), d.suggestion))
        .collect();
    assert_eq!(
        found,
        [
            (
                "factions.toml: temple.drift.policy: unknown drift policy 'demot' (did you mean 'demote'?)"
                    .to_owned(),
                suggestion("demot", "demote"),
            ),
            (
                "characters.toml: captain_hale: unknown key 'weigths' (did you mean 'weights'?)"
                    .to_owned(),
                suggestion("weigths", "weights"),
            ),
            (
                "characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)"
                    .to_owned(),
                suggestion("lantern_gild", "lantern_guild"),
            ),
        ]
    );
}

#[test]
fn a_misspelt_word_is_fixed_by_setting_it_and_a_misspelt_key_by_renaming_it() {
    let texts = typos();
    let fixes: Vec<Option<Fix>> = problems(&texts)
        .iter()
        .map(|d| fix_for(&texts, d))
        .collect();
    assert_eq!(
        fixes,
        [
            Some(Fix {
                file: "factions.toml".to_owned(),
                path: path("temple.drift.policy"),
                change: Change::Set("demote".to_owned()),
            }),
            Some(Fix {
                file: "characters.toml".to_owned(),
                path: path("captain_hale.weigths"),
                change: Change::Rename("weights".to_owned()),
            }),
            Some(Fix {
                file: "characters.toml".to_owned(),
                path: path("vex.memberships[0].faction"),
                change: Change::Set("lantern_guild".to_owned()),
            }),
        ]
    );
    assert_eq!(
        fixes[1].as_ref().map(ToString::to_string).as_deref(),
        Some("characters.toml: captain_hale.weigths: rename it 'weights'")
    );
    assert_eq!(
        fixes[2].as_ref().map(ToString::to_string).as_deref(),
        Some("characters.toml: vex.memberships[0].faction: set it to 'lantern_guild'")
    );
}

#[test]
fn a_renamed_key_keeps_its_value_its_place_and_its_comment() {
    let mut texts = typos();
    let characters = place("characters.toml");
    let fix = Fix {
        file: "characters.toml".to_owned(),
        path: path("captain_hale.weigths"),
        change: Change::Rename("weights".to_owned()),
    };
    let text = texts[characters].take().expect("there");
    let fixed = apply_fix(&text, &fix).expect("renamed");
    assert!(
        fixed.contains(
            "alignment = { law = 75.0, good = 30.0 }\n\
             # Like the Watch he serves, Hale judges people by their respect for the law.\n\
             weights = { law = 1.0, good = 0.25 }\n\
             # Hale's convictions have hardened over the years.\n\
             inertia = \"hardening\"\n"
        ),
        "{fixed}"
    );
    assert_eq!(fixed.len(), text.len() - "weigths".len() + "weights".len());
}

#[test]
fn with_every_fix_made_the_world_loads() {
    let mut texts = typos();
    for diagnostic in problems(&texts) {
        let fix = fix_for(&texts, &diagnostic).expect("a fix");
        let at = place(&fix.file);
        let text = texts[at].take().expect("there");
        texts[at] = Some(apply_fix(&text, &fix).expect("made"));
    }
    assert_eq!(problems(&texts), []);
    assert!(load_texts(&texts).is_ok());
}

#[test]
fn a_misspelt_part_of_a_quest_path_is_fixed_in_place() {
    let mut texts = world("content/sample");
    let quests = place("quests.toml");
    let text = texts[quests].take().expect("there");
    texts[quests] = Some(text.replace("watch_oath.patrol.report", "watch_oath.patrol.reprt"));
    let found = problems(&texts);
    let [problem] = &found[..] else {
        panic!("one problem: {found:?}");
    };
    assert_eq!(
        problem.message,
        "unknown choice 'reprt' in watch_oath.patrol (did you mean 'report'?)"
    );
    let fix = fix_for(&texts, problem).expect("a fix");
    assert_eq!(
        fix.change,
        Change::Set("watch_oath.patrol.report".to_owned())
    );
    assert_eq!(Some(fix.path.to_string()), problem.key);
}

#[test]
fn a_misspelt_table_at_the_top_of_a_file_is_renamed() {
    let mut texts = world("content/sample");
    let balance = place("balance.toml");
    let text = texts[balance].take().expect("there");
    texts[balance] = Some(text.replace("[standing]", "[standng]"));
    let found = problems(&texts);
    let [problem] = &found[..] else {
        panic!("one problem: {found:?}");
    };
    assert_eq!(problem.key, None);
    let fix = fix_for(&texts, problem).expect("a fix");
    assert_eq!(fix.path, path("standng"));
    assert_eq!(fix.change, Change::Rename("standing".to_owned()));
    let fixed = apply_fix(texts[balance].as_deref().expect("there"), &fix).expect("made");
    assert!(fixed.contains("\n[standing]\n"));
}

#[test]
fn a_problem_with_nothing_suggested_has_no_fix() {
    let texts = world("crates/cli/tests/fixtures/worlds/broken");
    let found = problems(&texts);
    let law = found
        .iter()
        .find(|d| d.key.as_deref() == Some("vex.alignment.law"))
        .expect("Vex's law is out of range");
    assert_eq!(law.suggestion, None);
    assert_eq!(fix_for(&texts, law), None);
    // A suggestion that matches nothing at its key is no fix either.
    let mut stray = law.clone();
    stray.suggestion = suggestion("nothing", "something");
    assert_eq!(fix_for(&texts, &stray), None);
}

#[test]
fn renaming_a_key_needs_it_there_and_its_new_name_free() {
    let text = "[a]\nx = 1\n# y's comment\ny = 2 # after\nz = { p = 3, q = 4 }\n";
    assert_eq!(
        rename_key(text, &path("a.y"), "w"),
        Ok("[a]\nx = 1\n# y's comment\nw = 2 # after\nz = { p = 3, q = 4 }\n".to_owned())
    );
    assert_eq!(
        rename_key(text, &path("a.z.p"), "r"),
        Ok("[a]\nx = 1\n# y's comment\ny = 2 # after\nz = { r = 3, q = 4 }\n".to_owned())
    );
    assert_eq!(
        rename_key(text, &path("a.y"), "x"),
        Err(EditError::Taken(path("a.x")))
    );
    assert_eq!(
        rename_key(text, &path("a.nothing"), "w"),
        Err(EditError::NotThere(path("a.nothing")))
    );
    assert_eq!(rename_key(text, &path("a.y"), " "), Err(EditError::Blank));
}

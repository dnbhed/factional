//! The session from texts (U14b, P-81, P-95): a world loaded from the texts in memory, as the
//! editor holds them, unsaved edits included, as it would be from a directory; and reloaded
//! from new texts with `reload`'s rules.

use std::path::Path;

use factional_content::{CONTENT_FILES, ContentTexts, read_texts};
use factional_session::{Outcome, Session};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn sample() -> ContentTexts {
    read_texts(&Path::new(REPO).join("content/sample")).expect("the sample is there")
}

fn run(session: &mut Session, line: &str) -> String {
    match session.execute(line) {
        Ok(outcome) => outcome.render(),
        Err(error) => format!("script error: {error}"),
    }
}

/// `texts` with `from` replaced once in `file`.
fn edited(texts: &ContentTexts, file: &str, from: &str, to: &str) -> ContentTexts {
    let mut texts = texts.clone();
    let place = CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file");
    let text = texts[place].as_mut().expect("the file is there");
    assert!(text.contains(from), "{file} has {from:?}");
    *text = text.replacen(from, to, 1);
    texts
}

#[test]
fn a_world_from_texts_is_the_world_from_their_directory() {
    let mut from_dir = Session::new(REPO);
    let mut from_texts = Session::new(REPO);
    assert_eq!(
        run(&mut from_dir, "load content/sample"),
        "loaded 6 characters from content/sample"
    );
    assert_eq!(
        from_texts.load_texts("content/sample", &sample()).render(),
        "loaded 6 characters from content/sample"
    );
    for line in ["characters", "factions", "relations --matrix", "quests"] {
        assert_eq!(
            run(&mut from_texts, line),
            run(&mut from_dir, line),
            "{line}"
        );
    }
}

#[test]
fn unsaved_edits_are_what_loads() {
    let mut session = Session::new(REPO);
    let texts = edited(
        &sample(),
        "characters.toml",
        "name = \"Vex\"",
        "name = \"Vexa\"",
    );
    session.load_texts("content/sample", &texts);
    assert!(run(&mut session, "show character vex").starts_with("vex — Vexa — "));
    // Texts that don't load are refused, saying why, as a directory's would be.
    let broken = edited(&texts, "characters.toml", "name = \"Vexa\"", "name = 3");
    assert_eq!(
        session.load_texts("content/sample", &broken),
        Outcome::Error("characters.toml: vex.name: expected text in quotes".to_owned())
    );
    assert!(run(&mut session, "show character vex").starts_with("vex — Vexa — "));
}

#[test]
fn new_texts_reload_with_reloads_rules() {
    let mut session = Session::new(REPO);
    session.load_texts("content/sample", &sample());
    run(&mut session, "act player steal --target merchant_ava");
    let texts = edited(
        &sample(),
        "characters.toml",
        "name = \"Vex\"",
        "name = \"Vexa\"",
    );
    let reloaded = session.reload_texts(&texts).render();
    assert!(
        reloaded.starts_with("reloaded content/sample and replayed 1 command"),
        "{reloaded}"
    );
    assert!(run(&mut session, "show character vex").starts_with("vex — Vexa — "));
    // A typed reload reloads the texts last given, not the directory.
    assert!(
        run(&mut session, "reload").starts_with("reloaded content/sample and replayed 1 command")
    );
    assert!(run(&mut session, "show character vex").starts_with("vex — Vexa — "));
    // Texts that don't load change nothing.
    let broken = edited(&texts, "characters.toml", "name = \"Vexa\"", "name = 3");
    assert!(matches!(session.reload_texts(&broken), Outcome::Error(_)));
    assert!(run(&mut session, "show character vex").starts_with("vex — Vexa — "));
}

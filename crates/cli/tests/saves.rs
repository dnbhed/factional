//! `save <file>` and `restore <file>` in the REPL (T4, P-54).

use std::fs;
use std::path::{Path, PathBuf};

use factional_cli::{Outcome, Session};

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

fn session() -> Session {
    Session::new(env!("CARGO_TARGET_TMPDIR"))
}

fn run(session: &mut Session, command: &str) -> String {
    session.execute(command).expect("a known command").render()
}

fn ok(session: &mut Session, command: &str) {
    let outcome = session.execute(command).expect("a known command");
    assert!(
        matches!(outcome, Outcome::Output(_)),
        "{command}: {outcome:?}"
    );
}

const QUERIES: [&str; 4] = [
    "events",
    "journal",
    "show character player",
    "disposition captain_hale player",
];

#[test]
fn a_restored_session_answers_every_query_as_the_saved_one_did() {
    sample_copy("cli_save");
    let mut played = session();
    for command in [
        "load cli_save",
        "act player steal --target merchant_ava",
        "act player steal --target merchant_ava",
        "watch player",
    ] {
        ok(&mut played, command);
    }
    assert!(
        run(&mut played, "advance 0").starts_with("error: "),
        "a refusal"
    );
    assert_eq!(
        run(&mut played, "save cli_save.json"),
        // Two thefts of 3 events each (the act, the alignment, Ava's standing; she's in no
        // faction, so nothing spills) and the watch's 1; the refused advance has none.
        "saved 4 commands and 7 events to cli_save.json"
    );
    let mut resumed = session();
    assert_eq!(
        run(&mut resumed, "restore cli_save.json"),
        "restored cli_save.json: cli_save at tick 0, with 4 commands and 7 events"
    );
    for query in QUERIES {
        assert_eq!(run(&mut resumed, query), run(&mut played, query), "{query}");
    }
    // The restored session goes on as the saved one would, and reloads from its content.
    assert_eq!(
        run(
            &mut resumed,
            "act player help_stranger --target merchant_ava"
        ),
        run(
            &mut played,
            "act player help_stranger --target merchant_ava"
        )
    );
    assert_eq!(
        run(&mut resumed, "reload"),
        "reloaded cli_save and replayed 5 commands\nno differences"
    );
}

#[test]
fn restore_refuses_content_that_has_changed_and_keeps_the_world() {
    let dir = sample_copy("cli_changed");
    let mut played = session();
    ok(&mut played, "load cli_changed");
    ok(&mut played, "save cli_changed.json");
    let mut other = session();
    ok(&mut other, "load cli_changed");
    ok(&mut other, "act player steal --target merchant_ava");
    let factions = dir.join("factions.toml");
    let edited = fs::read_to_string(&factions).expect("readable") + "\n# edited\n";
    fs::write(&factions, edited).expect("written");
    assert_eq!(
        run(&mut other, "restore cli_changed.json"),
        "error: the content in cli_changed has changed since this save: factions.toml"
    );
    assert!(
        run(&mut other, "events").contains("player did steal"),
        "the world it had"
    );
}

#[test]
fn a_save_records_the_content_as_it_was_loaded() {
    let dir = sample_copy("cli_loaded");
    let mut played = session();
    ok(&mut played, "load cli_loaded");
    // Edited after loading: the save is of the world built from the files as they were.
    let factions = dir.join("factions.toml");
    let original = fs::read_to_string(&factions).expect("readable");
    fs::write(&factions, format!("{original}\n# edited\n")).expect("written");
    ok(&mut played, "save cli_loaded.json");
    assert!(
        run(&mut session(), "restore cli_loaded.json")
            .starts_with("error: the content in cli_loaded has changed")
    );
    fs::write(&factions, original).expect("restored");
    assert!(run(&mut session(), "restore cli_loaded.json").starts_with("restored "));
}

#[test]
fn save_and_restore_report_what_stops_them() {
    let mut empty = session();
    assert_eq!(
        run(&mut empty, "save nowhere.json"),
        "error: no world is loaded yet: use load <dir> first"
    );
    assert_eq!(
        run(&mut empty, "save"),
        "error: save needs the form: save <file>"
    );
    assert_eq!(
        run(&mut empty, "restore"),
        "error: restore needs the form: restore <file>"
    );
    let missing = run(&mut empty, "restore no_such_save.json");
    assert!(
        missing.starts_with("error: cannot read no_such_save.json: "),
        "{missing}"
    );
    sample_copy("cli_unwritable");
    let mut played = session();
    ok(&mut played, "load cli_unwritable");
    let unwritable = run(&mut played, "save no/such/dir/save.json");
    assert!(
        unwritable.starts_with("error: cannot write no/such/dir/save.json: "),
        "{unwritable}"
    );
}

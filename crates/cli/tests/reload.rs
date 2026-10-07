//! `reload` in the REPL: re-read the loaded content, replay the session on it, and say what
//! changed; or, if that can't be done, change nothing.

use std::fs;
use std::path::{Path, PathBuf};

use factional_cli::{Outcome, Session};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// A fresh copy of content/sample, as `<tmp>/<name>`.
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

/// Replaces `old` with `new` in one of the copy's files.
fn edit(dir: &Path, file: &str, old: &str, new: &str) {
    let path = dir.join(file);
    let text = fs::read_to_string(&path).expect("readable");
    assert!(text.contains(old), "{file} has {old}");
    fs::write(&path, text.replacen(old, new, 1)).expect("written");
}

/// A session with the copy `name` loaded, and `commands` run.
fn session(name: &str, commands: &[&str]) -> Session {
    let mut session = Session::new(env!("CARGO_TARGET_TMPDIR"));
    for command in std::iter::once(format!("load {name}").as_str()).chain(commands.iter().copied())
    {
        let outcome = session.execute(command).expect("a known command");
        assert!(
            matches!(outcome, Outcome::Output(_)),
            "{command}: {outcome:?}"
        );
    }
    session
}

fn run(session: &mut Session, command: &str) -> String {
    session.execute(command).expect("a known command").render()
}

const STEAL: &str = "[steal]\nalignment = { law = -5.0, good = -3.0 }";
const THEFT: &str = "act player steal --target merchant_ava";

#[test]
fn reload_replays_the_session_on_the_edited_content_and_says_what_changed() {
    let dir = sample_copy("reload_steal");
    let mut session = session("reload_steal", &[THEFT, THEFT]);
    let journal = run(&mut session, "journal");
    edit(
        &dir,
        "actions.toml",
        STEAL,
        "[steal]\nalignment = { law = -10.0, good = -3.0 }",
    );
    assert_eq!(
        run(&mut session, "reload"),
        "reloaded reload_steal and replayed 2 commands\n\
         player law: -10.00 → -20.00\n\
         first difference at event #2:\n  \
         before: #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -5.00, good -3.00\n  \
         after: #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -10.00, good -3.00"
    );
    assert_eq!(run(&mut session, "journal"), journal, "the same commands");
    assert_eq!(
        run(&mut session, "show character player"),
        "player — The Player — law -20.00, good -6.00 — True Neutral"
    );
}

#[test]
fn reload_of_unchanged_content_finds_no_differences() {
    sample_copy("reload_same");
    let mut session = session("reload_same", &[THEFT]);
    assert_eq!(
        run(&mut session, "reload"),
        "reloaded reload_same and replayed 1 command\nno differences"
    );
}

#[test]
fn reload_of_content_that_no_longer_loads_keeps_the_world() {
    let dir = sample_copy("reload_broken");
    let mut session = session("reload_broken", &[THEFT]);
    edit(
        &dir,
        "characters.toml",
        "alignment = { law = 0.0, good = 0.0 }",
        "alignment = { law = 120.0, good = 0.0 }",
    );
    assert_eq!(
        run(&mut session, "reload"),
        "error: characters.toml: player.alignment.law: 120.00 is outside -100.00..100.00"
    );
    assert_eq!(
        run(&mut session, "show character player"),
        "player — The Player — law -5.00, good -3.00 — True Neutral",
        "the world it had"
    );
}

#[test]
fn reload_stops_at_a_command_the_new_content_refuses_and_keeps_the_world() {
    let dir = sample_copy("reload_refused");
    let mut session = session("reload_refused", &["join player free_company", THEFT]);
    let events = run(&mut session, "events");
    // The player is 2.50 from the Free Company: within 60.00, but not 1.00.
    edit(
        &dir,
        "factions.toml",
        "tolerance = 60.0\nmember_tolerance = 80.0",
        "tolerance = 1.0\nmember_tolerance = 80.0",
    );
    let reloaded = run(&mut session, "reload");
    assert!(
        reloaded.starts_with(
            "error: reload stopped at command 1, join player free_company: it was accepted, but now it's refused: "
        ),
        "{reloaded}"
    );
    assert!(reloaded.ends_with(". Nothing has changed."), "{reloaded}");
    assert_eq!(run(&mut session, "events"), events, "the world it had");
}

#[test]
fn reload_stops_at_a_refused_command_the_new_content_accepts() {
    let dir = sample_copy("reload_accepted");
    // The player's 2.50 from the Free Company is beyond a tolerance of 1.00.
    edit(
        &dir,
        "factions.toml",
        "tolerance = 60.0\nmember_tolerance = 80.0",
        "tolerance = 1.0\nmember_tolerance = 80.0",
    );
    let mut session = session("reload_accepted", &[]);
    assert!(run(&mut session, "join player free_company").starts_with("error: "));
    edit(
        &dir,
        "factions.toml",
        "tolerance = 1.0\nmember_tolerance = 80.0",
        "tolerance = 60.0\nmember_tolerance = 80.0",
    );
    let reloaded = run(&mut session, "reload");
    assert!(
        reloaded.starts_with(
            "error: reload stopped at command 1, join player free_company: it was refused ("
        ),
        "{reloaded}"
    );
    assert!(
        reloaded.ends_with("), but now it's accepted. Nothing has changed."),
        "{reloaded}"
    );
}

#[test]
fn reload_needs_a_loaded_world() {
    let mut session = Session::new(REPO);
    assert_eq!(
        run(&mut session, "reload"),
        "error: no world is loaded yet: use load <dir> first"
    );
}

//! Saving, restoring and reloading a session with quest progress (Q7).

use std::fs;
use std::path::{Path, PathBuf};

use factional_cli::Session;

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

/// Replaces `old` with `new`, once, in one of the copy's files.
fn edit(dir: &Path, file: &str, old: &str, new: &str) {
    let path = dir.join(file);
    let text = fs::read_to_string(&path).expect("readable");
    assert_eq!(text.matches(old).count(), 1, "{file} has {old} once");
    fs::write(&path, text.replacen(old, new, 1)).expect("written");
}

fn run(session: &mut Session, command: &str) -> String {
    match session.execute(command) {
        Ok(outcome) => outcome.render(),
        Err(error) => format!("script error: {error}"),
    }
}

/// A session on a fresh copy `name`, with the oath started and Vex reported.
fn reported(name: &str) -> (PathBuf, Session) {
    let dir = sample_copy(name);
    let mut session = Session::new(env!("CARGO_TARGET_TMPDIR"));
    for command in [
        format!("load {name}"),
        "start player watch_oath".to_owned(),
        "choose player watch_oath report".to_owned(),
    ] {
        let shown = run(&mut session, &command);
        assert!(!shown.starts_with("error"), "{command}: {shown}");
    }
    (dir, session)
}

fn last_line(shown: &str) -> &str {
    shown.lines().last().unwrap_or_default()
}

#[test]
fn quest_progress_is_saved_and_restored() {
    let (_, mut session) = reported("quests_saved");
    assert!(run(&mut session, "save quests_saved.json").starts_with("saved "));
    let text = fs::read_to_string(Path::new(env!("CARGO_TARGET_TMPDIR")).join("quests_saved.json"))
        .expect("written");
    assert!(text.contains("\"version\": 2,"), "{text}");
    assert_eq!(
        last_line(&run(&mut session, "choose player watch_oath swear")),
        "player finished watch_oath"
    );
    assert!(run(&mut session, "restore quests_saved.json").starts_with("restored "));
    assert_eq!(
        run(&mut session, "progress player"),
        "watch_oath — at oath\nwatch_career — up to step 1 of 4"
    );
    assert_eq!(run(&mut session, "standing player city_watch"), "19.50");
    assert_eq!(
        last_line(&run(&mut session, "choose player watch_oath swear")),
        "player finished watch_oath"
    );
}

#[test]
fn a_save_whose_quest_events_dont_fit_is_refused() {
    let (_, mut session) = reported("quests_tampered");
    run(&mut session, "save quests_tampered.json");
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("quests_tampered.json");
    let text = fs::read_to_string(&path).expect("written");
    assert_eq!(text.matches("\"stage\": \"oath\"").count(), 1, "{text}");
    fs::write(
        &path,
        text.replacen("\"stage\": \"oath\"", "\"stage\": \"oaths\"", 1),
    )
    .expect("written");
    let refused = run(&mut session, "restore quests_tampered.json");
    assert!(
        refused.starts_with("error: this save doesn't fit its quests: "),
        "{refused}"
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "watch_oath — at oath\nwatch_career — up to step 1 of 4"
    );
}

#[test]
fn reload_replays_quest_commands_where_they_came() {
    let (_, mut session) = reported("quests_reload");
    let events = run(&mut session, "events");
    assert_eq!(
        run(&mut session, "reload"),
        "reloaded quests_reload and replayed 2 commands\nno differences"
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "watch_oath — at oath\nwatch_career — up to step 1 of 4"
    );
    assert_eq!(run(&mut session, "events"), events);
}

#[test]
fn reload_stops_where_a_quest_command_comes_out_differently() {
    let (dir, mut session) = reported("quests_reload_refused");
    run(&mut session, "choose player watch_oath swear");
    edit(
        &dir,
        "quests.toml",
        "requires = { standing = { city_watch = 10.0 } }",
        "requires = { standing = { city_watch = 30.0 } }",
    );
    assert_eq!(
        run(&mut session, "reload"),
        "error: reload stopped at command 3, choose player watch_oath swear: it was accepted, but now it's refused: player can't choose at watch_oath.oath yet: it needs standing 30.00 with city_watch, and player has 19.50. Nothing has changed."
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "watch_oath — finished\nwatch_career — up to step 2 of 4"
    );
}

#[test]
fn reload_says_how_quest_progress_changed() {
    let (dir, mut session) = reported("quests_reload_changed");
    edit(
        &dir,
        "quests.toml",
        "{ id = \"report\", outcome = \"turned_in_vex\", next = \"oath\" }",
        "{ id = \"report\", outcome = \"turned_in_vex\", next = \"end\" }",
    );
    // The oath stays reachable: looking away now leads there.
    edit(
        &dir,
        "quests.toml",
        "{ id = \"look_away\", outcome = \"took_a_bribe\", next = \"end\" }",
        "{ id = \"look_away\", outcome = \"took_a_bribe\", next = \"oath\" }",
    );
    // The world comes out the same; only where the oath stands changes.
    assert_eq!(
        run(&mut session, "reload"),
        "reloaded quests_reload_changed and replayed 2 commands\nplayer's watch_oath: at oath → finished"
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "watch_oath — finished\nwatch_career — up to step 2 of 4"
    );
}

#[test]
fn reload_stops_where_a_refused_quest_command_is_now_accepted() {
    let (dir, mut session) = reported("quests_reload_accepted");
    run(&mut session, "outcome fined_by_watch player");
    // -0.50 with the Watch: swearing is refused, and waits.
    assert!(run(&mut session, "choose player watch_oath swear").starts_with("error: "));
    edit(
        &dir,
        "quests.toml",
        "requires = { standing = { city_watch = 10.0 } }",
        "requires = { standing = { city_watch = -10.0 } }",
    );
    assert_eq!(
        run(&mut session, "reload"),
        "error: reload stopped at command 4, choose player watch_oath swear: it was refused, but now it's accepted. Nothing has changed."
    );
}

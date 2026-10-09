//! `outline <dir>` in the REPL (U1): each content file, its entries, and every problem and
//! warning at the entry it's about, as the editor shows them.

use std::fs;
use std::path::Path;

use factional_cli::Session;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn run(session: &mut Session, command: &str) -> String {
    match session.execute(command) {
        Ok(outcome) => outcome.render(),
        Err(error) => format!("script error: {error}"),
    }
}

fn outline(dir: &str) -> String {
    run(&mut Session::new(REPO), &format!("outline {dir}"))
}

#[test]
fn outline_puts_each_problem_at_its_entry() {
    assert_eq!(
        outline("crates/cli/tests/fixtures/worlds/broken"),
        "balance.toml — not there\n\
         factions.toml — not there\n\
         characters.toml — 2 entries\n\
         \x20 hale — 2 problems\n\
         \x20   error: missing 'alignment'\n\
         \x20   error: unknown key 'alignmnet' (did you mean 'alignment'?)\n\
         \x20 vex — 1 problem\n\
         \x20   error: alignment.law: 120.00 is outside -100.00..100.00\n\
         actions.toml — not there\n\
         relations.toml — not there\n\
         outcomes.toml — not there\n\
         quests.toml — not there\n\
         questlines.toml — not there\n\
         crates/cli/tests/fixtures/worlds/broken doesn't load: 3 problems"
    );
}

#[test]
fn outline_puts_each_warning_at_its_entry() {
    assert_eq!(
        outline("crates/cli/tests/fixtures/worlds/odd_jobs"),
        "balance.toml — not there\n\
         factions.toml — 1 entry\n\
         \x20 watch — 1 warning\n\
         \x20   warning: tolerance: no one starts within The Watch's tolerance of 40.00: there are no characters\n\
         characters.toml — not there\n\
         actions.toml — not there\n\
         relations.toml — not there\n\
         outcomes.toml — not there\n\
         quests.toml — 2 entries\n\
         \x20 inspect\n\
         \x20 patrol\n\
         questlines.toml — 1 entry\n\
         \x20 jobs — 1 warning\n\
         \x20   warning: steps[0].leftovers: leftovers = \"close\" has no effect: the step needs all its quests, so none are left over\n\
         crates/cli/tests/fixtures/worlds/odd_jobs loads, with 2 warnings: 0 characters, 1 faction, 0 actions, 0 relations, 0 outcomes, 2 quests and 1 questline"
    );
}

#[test]
fn outline_lists_every_entry_of_a_list_by_its_place() {
    let shown = outline("content/sample");
    let relations: Vec<&str> = shown
        .lines()
        .skip_while(|line| !line.starts_with("relations.toml"))
        .take(8)
        .collect();
    assert_eq!(
        relations,
        [
            "relations.toml — 7 entries",
            "  relation[0]",
            "  relation[1]",
            "  relation[2]",
            "  relation[3]",
            "  relation[4]",
            "  relation[5]",
            "  relation[6]",
        ]
    );
    assert_eq!(
        shown.lines().last(),
        Some(
            "content/sample loads: 6 characters, 5 factions, 6 actions, 7 relations, 6 outcomes, 10 quests and 1 questline"
        )
    );
}

#[test]
fn a_file_that_isnt_toml_keeps_its_problem_and_lists_no_entries() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("outline_not_toml");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    fs::write(dir.join("characters.toml"), "[vex\nname = \"Vex\"\n").expect("written");
    let shown = run(
        &mut Session::new(env!("CARGO_TARGET_TMPDIR")),
        "outline outline_not_toml",
    );
    let characters: Vec<&str> = shown
        .lines()
        .skip_while(|line| !line.starts_with("characters.toml"))
        .take(2)
        .collect();
    assert_eq!(
        characters,
        [
            "characters.toml — can't be read",
            "  error: line 1: unclosed table, expected `]`",
        ]
    );
}

#[test]
fn outline_needs_a_directory() {
    assert_eq!(
        run(&mut Session::new(REPO), "outline"),
        "error: outline needs the form: outline <dir>"
    );
}

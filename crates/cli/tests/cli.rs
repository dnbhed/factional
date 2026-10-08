//! Drives the `factional` binary end to end: its flags, its exit codes, and what it writes to
//! stdout and stderr.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn factional(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factional"))
        .args(args)
        .output()
        .expect("the factional binary runs")
}

/// Runs `factional repl` with `input` piped to it as if typed, then closes its input.
fn repl(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factional"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the factional binary starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("input is written");
    child
        .wait_with_output()
        .expect("the factional binary finishes")
}

/// Runs the binary from the repository's root, so paths are as a designer would type them.
fn factional_in_repo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factional"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("the factional binary runs")
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output is UTF-8")
}

#[test]
fn version_flag_prints_the_name_and_version() {
    let output = factional(&["--version"]);
    assert!(output.status.success());
    assert_eq!(text(&output.stdout), "factional 0.1.0\n");
}

#[test]
fn run_prints_the_transcript_of_a_passing_script() {
    let smoke = format!(
        "{}/../../scenarios/smoke.scenario",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = factional(&["run", &smoke]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        "> echo hello\nhello\n> assert echo hi == hi\n"
    );
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn run_resolves_paths_in_the_script_from_the_current_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_factional"))
        .args(["run", "scenarios/characters.scenario"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("the factional binary runs");
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(text(&output.stdout).contains("loaded 6 characters from content/sample\n"));
}

#[test]
fn run_stops_at_an_unknown_command_and_exits_non_zero() {
    let output = factional(&["run", &fixture("unknown_command.scenario")]);
    assert!(!output.status.success());
    assert_eq!(
        text(&output.stderr),
        "line 3: unknown command 'frobnicate'\n"
    );
    assert!(text(&output.stdout).contains("> frobnicate\n"));
    assert!(!text(&output.stdout).contains("never runs"));
}

#[test]
fn run_reports_a_failed_assert_with_what_was_expected_and_what_came_back() {
    let output = factional(&["run", &fixture("failed_assert.scenario")]);
    assert!(!output.status.success());
    assert_eq!(text(&output.stderr), "line 2: expected 'bye', got 'hi'\n");
}

#[test]
fn run_stops_at_a_command_that_fails_outside_an_assert() {
    let output = factional(&["run", &fixture("command_error.scenario")]);
    assert!(!output.status.success());
    assert_eq!(text(&output.stderr), "line 2: error: boom\n");
    assert!(!text(&output.stdout).contains("never runs"));
}

#[test]
fn run_reports_a_script_it_cannot_read() {
    let output = factional(&["run", "no/such/file.scenario"]);
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("cannot read no/such/file.scenario: "),
        "stderr: {stderr}"
    );
}

#[test]
fn repl_carries_on_after_an_error_and_stops_at_quit() {
    let output = repl("echo hi\nfrobnicate\necho still here\nquit\necho after quit\n");
    assert!(output.status.success());
    let stdout = text(&output.stdout);
    assert!(stdout.contains("hi\n"), "stdout: {stdout}");
    assert!(stdout.contains("still here\n"), "stdout: {stdout}");
    assert!(!stdout.contains("after quit"), "stdout: {stdout}");
    assert!(text(&output.stderr).contains("unknown command 'frobnicate'\n"));
}

#[test]
fn repl_shows_a_failed_command_with_its_error_prefix() {
    let output = repl("fail boom\n");
    assert!(output.status.success());
    assert!(text(&output.stderr).contains("error: boom\n"));
}

#[test]
fn repl_ends_cleanly_at_the_end_of_its_input() {
    let output = repl("echo hi\n");
    assert!(output.status.success());
    assert!(text(&output.stdout).contains("hi\n"));
}

#[test]
fn repl_help_lists_the_commands() {
    let output = repl("help\n");
    assert!(output.status.success());
    assert!(text(&output.stdout).contains("assert <command> == <expected>"));
}

#[test]
fn validate_summarises_a_world_that_loads_and_exits_zero() {
    let output = factional_in_repo(&["validate", "content/sample"]);
    assert!(output.status.success(), "stdout: {}", text(&output.stdout));
    assert_eq!(
        text(&output.stdout),
        "content/sample loads: 6 characters, 5 factions, 6 actions, 7 relations, 6 outcomes, 10 quests and 1 questline\n"
    );
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn validate_lists_every_problem_as_load_does_and_exits_one() {
    let dir = "crates/cli/tests/fixtures/worlds/broken";
    let output = factional_in_repo(&["validate", dir]);
    assert_eq!(output.status.code(), Some(1));
    let mut session = factional_cli::Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(factional_cli::Outcome::Error(problems)) = session.execute(&format!("load {dir}"))
    else {
        panic!("the broken world doesn't load");
    };
    let mut expected: String = problems
        .lines()
        .map(|problem| format!("error: {problem}\n"))
        .collect();
    expected.push_str(&format!("{dir} doesn't load: 3 problems\n"));
    assert_eq!(text(&output.stdout), expected);
    assert!(
        expected.contains(
            "error: characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00\n"
        ),
        "{expected}"
    );
}

#[test]
fn validate_loads_a_world_whose_quests_reconcile() {
    let output = factional_in_repo(&["validate", "docs/examples/riverhold"]);
    assert!(output.status.success(), "stdout: {}", text(&output.stdout));
    assert_eq!(
        text(&output.stdout),
        "docs/examples/riverhold loads: 6 characters, 5 factions, 6 actions, 7 relations, 6 outcomes, 10 quests and 1 questline\n"
    );
}

#[test]
fn validate_refuses_a_world_whose_quests_dont_reconcile() {
    let output = factional_in_repo(&["validate", "crates/cli/tests/fixtures/worlds/lockouts"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert_eq!(stdout.lines().count(), 5, "{stdout}");
    assert!(
        stdout.ends_with("crates/cli/tests/fixtures/worlds/lockouts doesn't load: 4 problems\n"),
        "{stdout}"
    );
}

#[test]
fn validate_reports_a_directory_it_cannot_read() {
    let output = factional_in_repo(&["validate", "no/such/world"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert!(
        stdout.starts_with("error: no/such/world: cannot read the directory: "),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("\nno/such/world doesn't load: 1 problem\n"),
        "{stdout}"
    );
}

#[test]
fn validate_prints_warnings_but_still_exits_zero() {
    let dir = "crates/cli/tests/fixtures/worlds/reformed";
    let output = factional_in_repo(&["validate", dir]);
    assert!(output.status.success(), "stdout: {}", text(&output.stdout));
    assert_eq!(
        text(&output.stdout),
        format!(
            "warning: characters.toml: vex.memberships[0]: vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00\n\
             warning: factions.toml: lantern_guild.tolerance: no one starts within The Lantern Guild's tolerance of 45.00: the nearest is vex, 95.52 away\n\
             {dir} loads, with 2 warnings: 1 character, 1 faction, 0 actions, 0 relations and 0 outcomes\n"
        )
    );
}

#[test]
fn validate_warns_of_a_faction_no_one_starts_within_tolerance_of() {
    let dir = "crates/cli/tests/fixtures/worlds/lonely";
    let output = factional_in_repo(&["validate", dir]);
    assert!(output.status.success(), "stdout: {}", text(&output.stdout));
    assert_eq!(
        text(&output.stdout),
        format!(
            "warning: factions.toml: ashen_circle.tolerance: no one starts within The Ashen Circle's tolerance of 30.00: the nearest is vex, 50.00 away\n\
             {dir} loads, with 1 warning: 3 characters, 2 factions, 0 actions, 0 relations and 0 outcomes\n"
        )
    );
}

#[test]
fn schema_prints_a_files_json_schema() {
    let output = factional(&["schema", "factions"]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert_eq!(
        Some(stdout.clone()),
        factional_content::schema_text("factions")
    );
    let schema: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert!(jsonschema::meta::is_valid(&schema), "valid JSON Schema");
    assert_eq!(
        schema.pointer("/$defs/drift/properties/policy/enum"),
        Some(&serde_json::json!([
            "ignore",
            "flag",
            "demote",
            "expel",
            "probation"
        ]))
    );
}

#[test]
fn schema_without_a_file_lists_the_files() {
    let output = factional(&["schema"]);
    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout),
        "balance\nfactions\ncharacters\nactions\nrelations\noutcomes\nquests\nquestlines\n"
    );
}

#[test]
fn schema_refuses_a_file_that_isnt_content() {
    let output = factional(&["schema", "factionz"]);
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("invalid value 'factionz'"), "{stderr}");
    assert!(stderr.contains("factions"), "{stderr}");
}

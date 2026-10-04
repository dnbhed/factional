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

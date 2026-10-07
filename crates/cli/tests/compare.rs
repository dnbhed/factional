//! `factional compare <scenario> --content A --against B`: the same scenario on two worlds,
//! and what came out differently.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SCENARIO: &str = "crates/cli/tests/fixtures/two_thefts.scenario";

/// Runs the binary from the repository's root, so paths are as a designer would type them.
fn factional(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factional"))
        .args(args)
        .current_dir(REPO)
        .output()
        .expect("the factional binary runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output is UTF-8")
}

/// A fresh copy of content/sample in which stealing moves law by -10.00, not -5.00.
fn harsh_sample(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for entry in fs::read_dir(Path::new(REPO).join("content/sample")).expect("the sample") {
        let path = entry.expect("an entry").path();
        fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
    }
    let actions = dir.join("actions.toml");
    let text = fs::read_to_string(&actions).expect("readable");
    let steal = "[steal]\nalignment = { law = -5.0, good = -3.0 }";
    assert!(text.contains(steal), "the sample's steal is as expected");
    let harsher = text.replace(steal, "[steal]\nalignment = { law = -10.0, good = -3.0 }");
    fs::write(&actions, harsher).expect("written");
    dir
}

#[test]
fn compare_shows_what_changed_and_where_the_runs_first_differ() {
    let harsh = harsh_sample("compare_harsh");
    let harsh = harsh.to_str().expect("a UTF-8 path");
    let output = factional(&[
        "compare",
        SCENARIO,
        "--content",
        "content/sample",
        "--against",
        harsh,
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    // Two thefts at -5.00 each against two at -10.00; good moves -3.00 a time on both.
    assert_eq!(
        text(&output.stdout),
        format!(
            "comparing content/sample → {harsh} over {SCENARIO}\n\
             player law: -10.00 → -20.00\n\
             first difference at event #2:\n  \
             content/sample: #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -5.00, good -3.00\n  \
             {harsh}: #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -10.00, good -3.00\n"
        )
    );
}

#[test]
fn compare_of_a_world_with_itself_finds_no_differences() {
    let output = factional(&[
        "compare",
        SCENARIO,
        "--content",
        "content/sample",
        "--against",
        "content/sample",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!("comparing content/sample → content/sample over {SCENARIO}\nno differences\n")
    );
}

#[test]
fn compare_names_the_side_that_does_not_load_and_exits_one() {
    let broken = "crates/cli/tests/fixtures/worlds/broken";
    let output = factional(&[
        "compare",
        SCENARIO,
        "--content",
        "content/sample",
        "--against",
        broken,
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    assert_eq!(
        text(&output.stderr),
        format!(
            "--against {broken} doesn't load:\n\
             characters.toml: hale: missing 'alignment'\n\
             characters.toml: hale: unknown key 'alignmnet' (did you mean 'alignment'?)\n\
             characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00\n"
        )
    );
}

#[test]
fn compare_needs_a_scenario_that_loads_exactly_one_world() {
    for (scenario, loads) in [
        ("scenarios/distance.scenario", "2 worlds"),
        ("scenarios/smoke.scenario", "no world"),
    ] {
        let output = factional(&[
            "compare",
            scenario,
            "--content",
            "content/sample",
            "--against",
            "content/sample",
        ]);
        assert_eq!(output.status.code(), Some(1), "{scenario}");
        assert_eq!(
            text(&output.stderr),
            format!("{scenario} loads {loads}; compare needs a scenario that loads exactly one\n")
        );
    }
}

#[test]
fn compare_stops_at_a_mistake_in_the_scenario() {
    let scenario = "crates/cli/tests/fixtures/compare_unknown.scenario";
    let output = factional(&[
        "compare",
        scenario,
        "--content",
        "content/sample",
        "--against",
        "content/sample",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        "--content content/sample: line 3: unknown command 'frobnicate'\n"
    );
}

#[test]
fn compare_reports_a_scenario_it_cannot_read() {
    let output = factional(&[
        "compare",
        "no/such.scenario",
        "--content",
        "content/sample",
        "--against",
        "content/sample",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).starts_with("cannot read no/such.scenario: "),
        "{}",
        text(&output.stderr)
    );
}

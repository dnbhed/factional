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
        "content/sample loads: 6 characters, 5 factions, 6 actions, 12 relations, 6 outcomes, 10 quests and 1 questline\n"
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
        "docs/examples/riverhold loads: 6 characters, 5 factions, 6 actions, 12 relations, 6 outcomes, 10 quests and 1 questline\n"
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
        "balance\nfactions\ncharacters\nactions\noutcomes\nquests\nquestlines\n"
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

#[test]
fn world_info_summarises_a_map_file_and_exits_zero() {
    let file = "crates/content/tests/fixtures/maps/hand-made.map";
    let bytes = std::fs::metadata(format!("{}/../../{file}", env!("CARGO_MANIFEST_DIR")))
        .expect("the committed map is there")
        .len();
    let output = factional_in_repo(&["world", "info", file]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!(
            "hand-made.map: 70 × 66 tiles in 2 × 2 chunks, 3 overrides, 3 suggestions (1 dismissed; 1 dismissal not on the map); {bytes} bytes\n\
             recipe: hand-made, source pixels (3, 4) to (37, 36), 2 tiles a source pixel, seed 7; \
             detail 0.25 of the relief, 8.00 pixels, 4 octaves; \
             shallows to 10.00 metres, specks under 4 tiles; \
             levels every 5.00 of Azgaar's heights, specks under 4 tiles; foliage at scale 1.00, trees 2.00, shrubs 1.50 and grass 1.00 tiles apart\n"
        )
    );
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn world_suggestions_lists_a_maps_suggestions_and_exits_zero() {
    let output = factional_in_repo(&[
        "world",
        "suggestions",
        "crates/content/tests/fixtures/maps/hand-made.map",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        "hand-made.map: 1 settlement, 1 route and 1 point of interest (1 dismissed; 1 dismissal not on the map)\n\
         settlement_9 Riverhold: river, capital, port, 24129 people, at tile (12, 40); citadel, plaza and shanty town\n\
         route_4_2 Ferry way: sea route, 3 points, tile (0, 0) to (12, 40)\n\
         poi_0: ruins, at tile (33, 7); Walls of an older keep. (dismissed)\n\
         dismissed, not on the map: route_7\n"
    );
    let refused = factional_in_repo(&["world", "suggestions", "content/sample/factions.toml"]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(text(&refused.stderr), "this isn't a Factional map\n");
}

#[test]
fn world_dismiss_and_restore_rewrite_a_map_and_exit_zero() {
    let dir = std::env::temp_dir().join(format!("factional-w6c-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let map = dir.join("hand-made.map");
    std::fs::copy(
        format!(
            "{}/../../crates/content/tests/fixtures/maps/hand-made.map",
            env!("CARGO_MANIFEST_DIR")
        ),
        &map,
    )
    .expect("copied");
    let path = map.to_str().expect("a UTF-8 path");
    let dismissed = factional_in_repo(&["world", "dismiss", path, "settlement_9", "route_4_2"]);
    assert!(
        dismissed.status.success(),
        "stderr: {}",
        text(&dismissed.stderr)
    );
    assert_eq!(
        text(&dismissed.stdout),
        "dismissed settlement_9 and route_4_2 in hand-made.map; 3 of its 3 suggestions dismissed\n"
    );
    let restored = factional_in_repo(&["world", "restore", path, "route_7", "poi_0"]);
    assert!(
        restored.status.success(),
        "stderr: {}",
        text(&restored.stderr)
    );
    assert_eq!(
        text(&restored.stdout),
        "restored route_7 and poi_0 in hand-made.map; 2 of its 3 suggestions dismissed\n"
    );
    let refused = factional_in_repo(&["world", "restore", path, "poi_0"]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(text(&refused.stderr), "poi_0 isn't dismissed\n");
    std::fs::remove_dir_all(&dir).expect("removed");
}

#[test]
fn world_info_refuses_a_file_that_isnt_a_map_and_exits_one() {
    let output = factional_in_repo(&["world", "info", "content/sample/factions.toml"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    assert_eq!(text(&output.stderr), "this isn't a Factional map\n");
}

#[test]
fn world_source_summarises_an_azgaar_export_and_exits_zero() {
    let file = "crates/content/tests/fixtures/azgaar/tiny.json";
    let export = std::fs::read(format!("{}/../../{file}", env!("CARGO_MANIFEST_DIR")))
        .expect("the fixture is there");
    let output = factional_in_repo(&["world", "source", file]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!(
            "tiny.json: Tiny, from Azgaar 1.154.0, 40 × 30 pixels\n\
             4 cells, 6 vertices; 1 ocean, 1 lake and 1 island; 2 rivers; 2 settlements, 2 routes and 2 points of interest; 2 cultures, 3 religions and 2 states; fingerprint {}\n",
            factional_content::fingerprint_of(&export)
        )
    );
}

#[test]
fn world_source_refuses_text_that_isnt_json_and_exits_one() {
    let output = factional_in_repo(&["world", "source", "content/sample/factions.toml"]);
    assert_eq!(output.status.code(), Some(1));
    let refused = text(&output.stderr);
    assert!(refused.starts_with("this isn't JSON: "), "{refused}");
}

#[test]
fn world_import_writes_a_map_file_and_exits_zero() {
    let dir = std::env::temp_dir().join(format!("factional-w2b-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let map = dir.join("part.map");
    let output = factional_in_repo(&[
        "world",
        "import",
        "crates/content/tests/fixtures/azgaar/raster.json",
        map.to_str().expect("a UTF-8 path"),
        "--region",
        "10,10,20,20",
        "--scale",
        "2",
        "--seed",
        "7",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let bytes = std::fs::metadata(&map).expect("the map is written").len();
    // The tile counts were worked out by a separate script.
    assert_eq!(
        text(&output.stdout),
        format!(
            "imported Raster into part.map: 40 × 40 tiles in 1 × 1 chunks, \
             166 ocean, 58 lake, 0 river and 1376 land (34 sand, 204 grass, 1125 forest floor and 13 rock); heights -187.80 to 1734.65 m; \
             temperatures -3 to 14 °C; precipitation 800 to 3000 mm; 4 biomes; \
             levels 0 to 7, 435 ledges and 14 cliffs; 158 plants (62 trees, 23 shrubs and 73 grass); 1 settlement, 4 routes and 1 point of interest suggested; {bytes} bytes\n"
        )
    );
    std::fs::remove_dir_all(&dir).expect("removed");
}

#[test]
fn world_import_takes_the_detail_and_the_coast() {
    let dir = std::env::temp_dir().join(format!("factional-w3b-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let map = dir.join("coast.map");
    let output = factional_in_repo(&[
        "world",
        "import",
        "crates/content/tests/fixtures/azgaar/raster.json",
        map.to_str().expect("a UTF-8 path"),
        "--seed",
        "3",
        "--detail",
        "0.5",
        "--wavelength",
        "4",
        "--octaves",
        "2",
        "--shallows",
        "25",
        "--specks",
        "0",
        "--levels",
        "10",
        "--level-specks",
        "0",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let bytes = std::fs::metadata(&map).expect("the map is written").len();
    // The tile counts were worked out by a separate script.
    assert_eq!(
        text(&output.stdout),
        format!(
            "imported Raster into coast.map: 40 × 40 tiles in 1 × 1 chunks, \
             294 ocean, 183 lake, 0 river and 1123 land (112 sand, 346 grass, 642 forest floor and 23 rock); heights -381.73 to 2241.02 m; \
             temperatures -3 to 14 °C; precipitation 800 to 3000 mm; 4 biomes; \
             levels 0 to 4, 588 ledges and 24 cliffs; 173 plants (58 trees, 28 shrubs and 87 grass); 3 settlements, 3 routes and 2 points of interest suggested; {bytes} bytes\n"
        )
    );
    let info = factional_in_repo(&["world", "info", map.to_str().expect("a UTF-8 path")]);
    let said = text(&info.stdout);
    assert!(
        said.ends_with(
            "; detail 0.50 of the relief, 4.00 pixels, 2 octaves; shallows to 25.00 metres, \
             specks under 0 tiles; levels every 10.00 of Azgaar's heights, specks under 0 tiles; foliage at scale 1.00, trees 2.00, shrubs 1.50 and grass 1.00 tiles apart\n"
        ),
        "{said}"
    );
    std::fs::remove_dir_all(&dir).expect("removed");
}

#[test]
fn world_import_takes_the_foliages_scale_and_spacings() {
    let dir = std::env::temp_dir().join(format!("factional-w5-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let map = dir.join("foliage.map");
    let output = factional_in_repo(&[
        "world",
        "import",
        "crates/content/tests/fixtures/azgaar/raster.json",
        map.to_str().expect("a UTF-8 path"),
        "--foliage",
        "2",
        "--tree-spacing",
        "1",
        "--shrub-spacing",
        "1",
        "--grass-spacing",
        "0.5",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let said = text(&output.stdout);
    // The plants were worked out by a separate script.
    assert!(
        said.contains("; 520 plants (272 trees, 37 shrubs and 211 grass); "),
        "{said}"
    );
    let info = factional_in_repo(&["world", "info", map.to_str().expect("a UTF-8 path")]);
    let said = text(&info.stdout);
    assert!(
        said.ends_with(
            "; foliage at scale 2.00, trees 1.00, shrubs 1.00 and grass 0.50 tiles apart\n"
        ),
        "{said}"
    );
    std::fs::remove_dir_all(&dir).expect("removed");
}

#[test]
fn world_import_refuses_a_region_off_the_source_and_exits_one() {
    let output = factional_in_repo(&[
        "world",
        "import",
        "crates/content/tests/fixtures/azgaar/raster.json",
        "target/off.map",
        "--region",
        "30,30,20,20",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        "the region runs off the source map: pixels (30, 30) to (49, 49), and the map is 40 × 40 pixels\n"
    );
}

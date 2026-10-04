//! Runs every `scenarios/*.scenario` through the script runner and snapshots its transcript
//! (DECISIONS.md P-27). A scenario that stops early fails the test with the line it stopped at.

use factional_cli::run_script;

#[test]
fn scenario_transcripts() {
    insta::glob!("../../../scenarios", "*.scenario", |path| {
        let source = std::fs::read_to_string(path).expect("the scenario is readable");
        let transcript =
            run_script(&source).unwrap_or_else(|failure| panic!("{}: {failure}", path.display()));
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("a UTF-8 file name");
        // Snapshots stay in this crate's `tests/snapshots/`, where `cargo insta review` finds them.
        insta::with_settings!({
            prepend_module_to_snapshot => false,
            // Set explicitly: glob!'s own suffix is empty while only one file matches, so
            // snapshot names would change when the second scenario arrives.
            snapshot_suffix => name,
        }, {
            insta::assert_snapshot!(transcript);
        });
    });
}

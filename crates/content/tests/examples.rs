//! The complete example content in `docs/examples/` describes settings the engine doesn't
//! read yet, so it can't be loaded. This keeps it at least valid TOML, file by file.

use std::fs;
use std::path::Path;

#[test]
fn every_example_content_file_is_valid_toml() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/riverhold");
    let mut checked = Vec::new();
    for entry in fs::read_dir(&dir).expect("the examples directory exists") {
        let path = entry.expect("a readable entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            let text = fs::read_to_string(&path).expect("a readable file");
            if let Err(error) = text.parse::<toml::Table>() {
                panic!("{} isn't valid TOML: {error}", path.display());
            }
            checked.push(path.file_name().expect("a file name").to_owned());
        }
    }
    checked.sort();
    assert_eq!(
        checked,
        [
            "actions.toml",
            "balance.toml",
            "characters.toml",
            "factions.toml",
            "outcomes.toml",
            "relations.toml"
        ]
    );
}

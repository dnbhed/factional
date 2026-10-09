//! `factional-editor <dir>` from the shell.

use std::process::Command;

#[test]
fn the_editor_needs_a_content_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_factional-editor"))
        .output()
        .expect("the editor runs");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "factional-editor needs a content directory: factional-editor <dir>\n"
    );
}

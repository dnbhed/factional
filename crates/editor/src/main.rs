//! `factional-editor <dir> [--at <file>:<key>]`: the editor on a content directory (DESIGN.md
//! §18), with `--at` the entry selected, as the builder's "Edit in the editor" asks (G8).

use std::process::ExitCode;

use factional_editor::{Editor, opening, options};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let asked = match opening(&args) {
        Ok(asked) => asked,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    let mut editor = Editor::open(&asked.dir);
    if let Some((file, key)) = &asked.at {
        editor.select_at(file, key);
    }
    let title = format!("Factional — {}", asked.dir.display());
    let shown = eframe::run_native(&title, options(), Box::new(|_| Ok(Box::new(editor))));
    match shown {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the editor couldn't start: {error}");
            ExitCode::FAILURE
        }
    }
}

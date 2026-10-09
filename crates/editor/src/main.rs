//! `factional-editor <dir>`: the editor on a content directory (DESIGN.md §18).

use std::process::ExitCode;

use factional_editor::Editor;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir] = &args[..] else {
        eprintln!("factional-editor needs a content directory: factional-editor <dir>");
        return ExitCode::from(2);
    };
    let editor = Editor::open(dir);
    let title = format!("Factional — {dir}");
    let shown = eframe::run_native(
        &title,
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(editor))),
    );
    match shown {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the editor couldn't start: {error}");
            ExitCode::FAILURE
        }
    }
}

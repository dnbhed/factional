//! Opening the other app (G8, D-43, P-130): the editor's "Play in the builder" and the
//! builder's "Edit in the editor", each finding the other and starting it on the same content
//! directory, without waiting for it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The app to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Other {
    Editor,
    Builder,
}

impl Other {
    /// Its executable's name, without the platform's suffix.
    fn name(self) -> &'static str {
        match self {
            Other::Editor => "factional-editor",
            Other::Builder => "factional-builder",
        }
    }

    /// The variable that names its executable, to use instead of looking.
    fn variable(self) -> &'static str {
        match self {
            Other::Editor => "FACTIONAL_EDITOR",
            Other::Builder => "FACTIONAL_BUILDER",
        }
    }

    /// The command that builds it, from the repository.
    fn build(self) -> &'static str {
        match self {
            Other::Editor => "cargo build -p factional-editor",
            Other::Builder => {
                "cargo build --manifest-path crates/bevy/Cargo.toml -p factional-builder --features window"
            }
        }
    }
}

/// Where to look for the other app: the file its variable names, the folder this app is in,
/// and `PATH`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lookup {
    pub named: Option<OsString>,
    pub beside: Option<PathBuf>,
    pub path: Option<OsString>,
}

impl Lookup {
    /// Where to look for `other` from this process: its variable, this executable's folder
    /// and `PATH`.
    pub fn here(other: Other) -> Lookup {
        Lookup {
            named: std::env::var_os(other.variable()),
            beside: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
            path: std::env::var_os("PATH"),
        }
    }
}

/// The other app's executable: the file its variable names, said if that isn't there; else
/// the first found beside this app, then along `PATH`, in order; or how to build it.
pub fn find(other: Other, lookup: &Lookup) -> Result<PathBuf, String> {
    if let Some(named) = &lookup.named {
        let named = PathBuf::from(named);
        return match named.is_file() {
            true => Ok(named),
            false => Err(format!(
                "{} names {}, which isn't there",
                other.variable(),
                named.display()
            )),
        };
    }
    let file = format!("{}{}", other.name(), std::env::consts::EXE_SUFFIX);
    let along_path = lookup.path.iter().flat_map(std::env::split_paths);
    lookup
        .beside
        .clone()
        .into_iter()
        .chain(along_path)
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "{} isn't beside this app or on PATH: build it with `{}`, or set {} to it",
                other.name(),
                other.build(),
                other.variable()
            )
        })
}

/// Starts `other` with `args`, not waiting for it, though it's reaped when it ends; or says
/// why it couldn't.
pub fn open(other: Other, lookup: &Lookup, args: &[OsString]) -> Result<(), String> {
    let app = find(other, lookup)?;
    let mut child = Command::new(&app)
        .args(args)
        .spawn()
        .map_err(|error| format!("couldn't start {}: {error}", app.display()))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

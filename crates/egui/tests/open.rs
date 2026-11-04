//! Opening the other app (G8): found by its variable, then beside this one, then on `PATH`;
//! started on the same directory without waiting for it.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use factional_egui::open::{Lookup, Other, find, open};

/// A fresh, empty folder for one test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("factional_open_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh folder");
    dir
}

/// `name` placed in `dir`, as the app would be named there.
fn app_in(dir: &Path, name: &str) -> PathBuf {
    let app = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    fs::write(&app, "").expect("written");
    app
}

fn path_of(dirs: &[&Path]) -> Option<OsString> {
    Some(std::env::join_paths(dirs).expect("joined"))
}

#[test]
fn the_variable_names_the_app_before_anywhere_else() {
    let root = folder("named");
    let named = app_in(&root, "my-builder");
    let beside = folder("named_beside");
    app_in(&beside, "factional-builder");
    let lookup = Lookup {
        named: Some(named.clone().into_os_string()),
        beside: Some(beside.clone()),
        path: path_of(&[&beside]),
    };
    assert_eq!(find(Other::Builder, &lookup), Ok(named));
}

#[test]
fn a_variable_naming_nothing_there_is_said() {
    let root = folder("named_missing");
    let missing = root.join("nowhere");
    let lookup = Lookup {
        named: Some(missing.clone().into_os_string()),
        ..Lookup::default()
    };
    assert_eq!(
        find(Other::Editor, &lookup),
        Err(format!(
            "FACTIONAL_EDITOR names {}, which isn't there",
            missing.display()
        ))
    );
}

#[test]
fn beside_this_app_comes_before_path() {
    let beside = folder("beside");
    let on_path = folder("beside_path");
    let here = app_in(&beside, "factional-editor");
    app_in(&on_path, "factional-editor");
    let lookup = Lookup {
        named: None,
        beside: Some(beside),
        path: path_of(&[&on_path]),
    };
    assert_eq!(find(Other::Editor, &lookup), Ok(here));
}

#[test]
fn path_is_searched_in_order() {
    let first = folder("path_first");
    let second = folder("path_second");
    let third = folder("path_third");
    let found = app_in(&second, "factional-builder");
    app_in(&third, "factional-builder");
    // An editor beside it isn't the builder.
    app_in(&first, "factional-editor");
    let lookup = Lookup {
        named: None,
        beside: Some(first.clone()),
        path: path_of(&[&first, &second, &third]),
    };
    assert_eq!(find(Other::Builder, &lookup), Ok(found));
}

#[test]
fn not_found_says_how_to_build_it() {
    let empty = folder("not_found");
    let lookup = Lookup {
        named: None,
        beside: Some(empty.clone()),
        path: path_of(&[&empty]),
    };
    assert_eq!(
        find(Other::Builder, &lookup),
        Err("factional-builder isn't beside this app or on PATH: build it with `cargo build --manifest-path crates/bevy/Cargo.toml -p factional-builder --features window`, or set FACTIONAL_BUILDER to it".to_owned())
    );
    assert_eq!(
        find(Other::Editor, &Lookup::default()),
        Err("factional-editor isn't beside this app or on PATH: build it with `cargo build -p factional-editor`, or set FACTIONAL_EDITOR to it".to_owned())
    );
}

#[test]
fn here_reads_the_apps_own_variable() {
    let lookup = Lookup::here(Other::Builder);
    assert_eq!(
        lookup.beside,
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        "the folder this test runs from"
    );
    assert_eq!(lookup.path, std::env::var_os("PATH"));
    assert_eq!(lookup.named, std::env::var_os("FACTIONAL_BUILDER"));
    assert_eq!(
        Lookup::here(Other::Editor).named,
        std::env::var_os("FACTIONAL_EDITOR")
    );
}

#[cfg(unix)]
mod started {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    use super::*;

    /// A stand-in for the other app: a script that writes its arguments to `said`.
    fn stand_in(dir: &Path, said: &Path) -> PathBuf {
        let app = dir.join("factional-builder");
        fs::write(
            &app,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", said.display()),
        )
        .expect("written");
        fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).expect("executable");
        app
    }

    #[test]
    fn opening_starts_it_with_the_arguments_and_doesnt_wait() {
        let dir = folder("started");
        let said = dir.join("said.txt");
        stand_in(&dir, &said);
        let lookup = Lookup {
            beside: Some(dir.clone()),
            ..Lookup::default()
        };
        let args: Vec<OsString> = ["content/sample", "--as", "captain_hale"]
            .map(OsString::from)
            .to_vec();
        assert_eq!(open(Other::Builder, &lookup, &args), Ok(()));
        let started = Instant::now();
        while !said.exists() && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            fs::read_to_string(&said).expect("it ran"),
            "content/sample\n--as\ncaptain_hale\n"
        );
    }

    #[test]
    fn an_app_that_cant_start_is_said() {
        let dir = folder("cant_start");
        let app = app_in(&dir, "factional-builder");
        fs::set_permissions(&app, fs::Permissions::from_mode(0o644)).expect("not executable");
        let lookup = Lookup {
            beside: Some(dir),
            ..Lookup::default()
        };
        let said = open(Other::Builder, &lookup, &[]).expect_err("can't start");
        assert!(
            said.starts_with(&format!("couldn't start {}: ", app.display())),
            "{said}"
        );
    }

    #[test]
    fn opening_what_isnt_found_says_so() {
        let dir = folder("open_not_found");
        let lookup = Lookup {
            beside: Some(dir),
            ..Lookup::default()
        };
        assert_eq!(
            open(Other::Editor, &lookup, &[]),
            Err("factional-editor isn't beside this app or on PATH: build it with `cargo build -p factional-editor`, or set FACTIONAL_EDITOR to it".to_owned())
        );
    }
}

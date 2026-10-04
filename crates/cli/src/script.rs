use std::fmt;
use std::path::Path;

use crate::session::{Outcome, Session, is_blank_or_comment};

/// Why a scenario script stopped: the line it stopped at, what went wrong, and the transcript
/// up to and including that line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    pub line: usize,
    pub message: String,
    pub transcript: String,
}

impl fmt::Display for RunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for RunFailure {}

/// Runs a scenario script and returns its transcript: each command after `> `, followed by its
/// output. Blank lines and `#` comments are skipped. The run stops at the first line that goes
/// wrong: a script error, or a command that fails outside an `assert`. Relative paths in the
/// script, as in `load content/sample`, are resolved against `base_dir`.
pub fn run_script(source: &str, base_dir: &Path) -> Result<String, RunFailure> {
    let mut session = Session::new(base_dir);
    let mut transcript = String::new();
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if is_blank_or_comment(line) {
            continue;
        }
        transcript.push_str(&format!("> {line}\n"));
        let message = match session.execute(line) {
            Ok(Outcome::Output(text)) => {
                if !text.is_empty() {
                    transcript.push_str(&text);
                    transcript.push('\n');
                }
                continue;
            }
            Ok(Outcome::Quit) => break,
            Ok(failed @ Outcome::Error(_)) => failed.render(),
            Err(error) => error.to_string(),
        };
        return Err(RunFailure {
            line: index + 1,
            message,
            transcript,
        });
    }
    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Result<String, RunFailure> {
        run_script(source, Path::new("."))
    }

    #[test]
    fn resolves_relative_paths_against_the_base_directory() {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        assert_eq!(
            run_script("load content/sample", root),
            Ok("> load content/sample\nloaded 6 characters from content/sample\n".into())
        );
    }

    #[test]
    fn a_passing_script_returns_each_command_followed_by_its_output() {
        let transcript = run("echo hello\nassert echo hi == hi\n").unwrap();
        assert_eq!(transcript, "> echo hello\nhello\n> assert echo hi == hi\n");
    }

    #[test]
    fn an_empty_script_passes_with_an_empty_transcript() {
        assert_eq!(run(""), Ok(String::new()));
    }

    #[test]
    fn blank_lines_and_comments_are_skipped_but_still_counted() {
        let failure = run("# a comment\n\n   \nfrobnicate\n").unwrap_err();
        assert_eq!(failure.line, 4);
        assert_eq!(failure.transcript, "> frobnicate\n");
    }

    #[test]
    fn surrounding_whitespace_on_a_line_is_ignored() {
        assert_eq!(run("   echo hi   \n"), Ok("> echo hi\nhi\n".into()));
    }

    #[test]
    fn an_unknown_command_stops_the_run_at_its_line() {
        let failure = run("echo before\nfrobnicate\necho after\n").unwrap_err();
        assert_eq!(failure.to_string(), "line 2: unknown command 'frobnicate'");
        assert_eq!(failure.transcript, "> echo before\nbefore\n> frobnicate\n");
    }

    #[test]
    fn a_failed_assert_stops_the_run_at_its_line() {
        let failure = run("echo before\nassert echo hi == bye\necho after\n").unwrap_err();
        assert_eq!(failure.to_string(), "line 2: expected 'bye', got 'hi'");
    }

    #[test]
    fn a_command_error_outside_an_assert_stops_the_run() {
        let failure = run("fail boom\necho after\n").unwrap_err();
        assert_eq!(failure.to_string(), "line 1: error: boom");
        assert_eq!(failure.transcript, "> fail boom\n");
    }

    #[test]
    fn a_command_error_inside_an_assert_does_not_stop_the_run() {
        let transcript = run("assert fail boom == error: boom\necho after\n").unwrap();
        assert_eq!(
            transcript,
            "> assert fail boom == error: boom\n> echo after\nafter\n"
        );
    }

    #[test]
    fn quit_ends_the_script_early_and_successfully() {
        assert_eq!(
            run("echo one\nquit\necho two\n"),
            Ok("> echo one\none\n> quit\n".into())
        );
    }
}

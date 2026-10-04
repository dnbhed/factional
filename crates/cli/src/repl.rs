use std::io::{self, Write};
use std::process::ExitCode;

use rustyline::error::ReadlineError;
use rustyline::{Config, DefaultEditor};

use crate::session::{Outcome, Session, is_blank_or_comment};

const PROMPT: &str = "factional> ";

/// One read from the line editor.
#[derive(Debug, PartialEq, Eq)]
enum Input {
    Line(String),
    /// Ctrl-C: abandon the line being typed.
    Interrupted,
    /// Ctrl-D, or the end of piped input.
    End,
    Unreadable(String),
}

impl From<rustyline::Result<String>> for Input {
    fn from(read: rustyline::Result<String>) -> Self {
        match read {
            Ok(line) => Input::Line(line),
            Err(ReadlineError::Interrupted) => Input::Interrupted,
            Err(ReadlineError::Eof) => Input::End,
            Err(error) => Input::Unreadable(error.to_string()),
        }
    }
}

/// Runs an interactive session on the terminal until `quit` or end of input. When input is
/// piped rather than typed, it reads it line by line without a prompt.
pub fn run_repl() -> ExitCode {
    let config = Config::builder().auto_add_history(true).build();
    let mut editor = match DefaultEditor::with_config(config) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("cannot start the line editor: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "factional {} — type `help` for commands, `quit` to leave",
        env!("CARGO_PKG_VERSION")
    );
    let read = || Input::from(editor.readline(PROMPT));
    match run_session(read, &mut io::stdout(), &mut io::stderr()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) | Err(_) => ExitCode::FAILURE,
    }
}

/// Reads and runs commands until `quit`, the end of input, or input that can't be read.
/// Returns whether the session ended cleanly.
fn run_session(
    mut read: impl FnMut() -> Input,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<bool> {
    let mut session = Session::default();
    loop {
        let line = match read() {
            Input::Line(line) => line,
            Input::Interrupted => continue,
            Input::End => return Ok(true),
            Input::Unreadable(reason) => {
                writeln!(err, "cannot read input: {reason}")?;
                return Ok(false);
            }
        };
        let line = line.trim();
        if is_blank_or_comment(line) {
            continue;
        }
        match session.execute(line) {
            Ok(Outcome::Quit) => return Ok(true),
            Ok(Outcome::Output(text)) => {
                if !text.is_empty() {
                    writeln!(out, "{text}")?;
                }
            }
            Ok(failed @ Outcome::Error(_)) => writeln!(err, "{}", failed.render())?,
            Err(error) => writeln!(err, "{error}")?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a session over `inputs`, then returns whether it ended cleanly, its stdout and its
    /// stderr.
    fn session(inputs: Vec<Input>) -> (bool, String, String) {
        let mut inputs = inputs.into_iter();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let clean = run_session(|| inputs.next().unwrap_or(Input::End), &mut out, &mut err)
            .expect("writing to memory succeeds");
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).expect("output is UTF-8");
        (clean, text(out), text(err))
    }

    fn line(text: &str) -> Input {
        Input::Line(text.into())
    }

    #[test]
    fn output_goes_to_stdout_and_the_session_ends_cleanly_at_the_end_of_input() {
        assert_eq!(
            session(vec![line("echo hi")]),
            (true, "hi\n".into(), String::new())
        );
    }

    #[test]
    fn errors_go_to_stderr_and_the_session_carries_on() {
        let (clean, out, err) =
            session(vec![line("frobnicate"), line("fail boom"), line("echo hi")]);
        assert!(clean);
        assert_eq!(out, "hi\n");
        assert_eq!(err, "unknown command 'frobnicate'\nerror: boom\n");
    }

    #[test]
    fn quit_stops_reading() {
        assert_eq!(
            session(vec![line("quit"), line("echo after")]),
            (true, String::new(), String::new())
        );
    }

    #[test]
    fn ctrl_c_abandons_the_line_and_the_session_carries_on() {
        assert_eq!(
            session(vec![Input::Interrupted, line("echo hi")]),
            (true, "hi\n".into(), String::new())
        );
    }

    #[test]
    fn unreadable_input_ends_the_session_as_a_failure() {
        assert_eq!(
            session(vec![
                Input::Unreadable("disk on fire".into()),
                line("echo hi")
            ]),
            (
                false,
                String::new(),
                "cannot read input: disk on fire\n".into()
            )
        );
    }

    #[test]
    fn blank_lines_comments_and_empty_output_print_nothing() {
        assert_eq!(
            session(vec![line("   "), line("# a note"), line("echo")]),
            (true, String::new(), String::new())
        );
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(
            session(vec![line("  echo hi  ")]),
            (true, "hi\n".into(), String::new())
        );
    }

    #[test]
    fn editor_results_map_to_inputs() {
        assert_eq!(Input::from(Ok("echo hi".to_owned())), line("echo hi"));
        assert_eq!(
            Input::from(Err(ReadlineError::Interrupted)),
            Input::Interrupted
        );
        assert_eq!(Input::from(Err(ReadlineError::Eof)), Input::End);
        assert_eq!(
            Input::from(Err(ReadlineError::Io(io::Error::other("disk on fire")))),
            Input::Unreadable("disk on fire".into())
        );
    }
}

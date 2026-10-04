use std::fmt;

use factional_core::{Fixed, ParseFixedError};

/// Every command as `(usage, description)`, in the order `help` lists them.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "list these commands"),
    ("quit", "leave the REPL, or end a script early"),
    (
        "calc <a> <op> <b>",
        "the engine's fixed-point arithmetic; <op> is + - * or /",
    ),
    (
        "curve <curve> at <x>",
        "a curve's value at <x>; <curve> is a number or [[x, y], ...]",
    ),
    ("echo <text>", "print <text>"),
    ("fail <message>", "fail with <message>; for testing scripts"),
    (
        "assert <command> == <expected>",
        "check that <command>'s result is exactly <expected>",
    ),
];

/// What running one command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command ran; this is what it printed (possibly nothing).
    Output(String),
    /// The command ran and failed with this message.
    Error(String),
    /// The session should end.
    Quit,
}

impl Outcome {
    /// The text a person sees for this outcome; `assert` compares against exactly this.
    pub fn render(&self) -> String {
        match self {
            Outcome::Output(text) => text.clone(),
            Outcome::Error(message) => format!("error: {message}"),
            Outcome::Quit => String::new(),
        }
    }
}

/// A mistake in the input itself, rather than a command that ran and failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    UnknownCommand(String),
    MalformedAssert,
    AssertionFailed { expected: String, got: String },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::UnknownCommand(name) => write!(f, "unknown command '{name}'"),
            ScriptError::MalformedAssert => {
                f.write_str("assert needs the form: assert <command> == <expected>")
            }
            ScriptError::AssertionFailed { expected, got } => {
                write!(f, "expected '{expected}', got '{got}'")
            }
        }
    }
}

impl std::error::Error for ScriptError {}

/// One CLI session: the state that commands act on, shared by the REPL and the script runner.
#[derive(Debug, Default)]
pub struct Session {}

impl Session {
    /// Runs one command line.
    pub fn execute(&mut self, line: &str) -> Result<Outcome, ScriptError> {
        let (name, rest) = split_command(line);
        match name {
            "help" => Ok(Outcome::Output(help_text())),
            "quit" => Ok(Outcome::Quit),
            "calc" => Ok(calc(rest)),
            "curve" => Ok(curve(rest)),
            "echo" => Ok(Outcome::Output(rest.to_owned())),
            "fail" => Ok(Outcome::Error(rest.to_owned())),
            "assert" => self.assert(rest),
            _ => Err(ScriptError::UnknownCommand(name.to_owned())),
        }
    }

    /// `assert <command> == <expected>`: runs `<command>` and passes if its rendered result is
    /// exactly `<expected>`. A command that fails renders as `error: …`, so failures can be
    /// asserted on too; a mistake in the command itself is still a script error.
    fn assert(&mut self, spec: &str) -> Result<Outcome, ScriptError> {
        let (command, expected) = split_assert(spec).ok_or(ScriptError::MalformedAssert)?;
        let got = self.execute(command)?.render();
        if got == expected {
            Ok(Outcome::Output(String::new()))
        } else {
            Err(ScriptError::AssertionFailed {
                expected: expected.to_owned(),
                got,
            })
        }
    }
}

/// `calc <a> <op> <b>`: the engine's fixed-point arithmetic, so a designer can check how a
/// number rounds (DESIGN.md §4.1).
fn calc(args: &str) -> Outcome {
    match calculate(args) {
        Ok(value) => Outcome::Output(value.to_string()),
        Err(message) => Outcome::Error(message),
    }
}

fn calculate(args: &str) -> Result<Fixed, String> {
    const USAGE: &str = "calc needs the form: calc <a> <op> <b>, where <op> is + - * or /";
    let [a, op, b] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Err(USAGE.to_owned());
    };
    let a: Fixed = a
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    let b: Fixed = b
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    let result = match op {
        "+" => a.checked_add(b),
        "-" => a.checked_sub(b),
        "*" => a.checked_mul(b),
        "/" if b == Fixed::ZERO => return Err(format!("cannot divide by {b}")),
        "/" => a.checked_div(b),
        _ => return Err(USAGE.to_owned()),
    };
    result.ok_or_else(|| "the result is out of range".to_owned())
}

/// `curve <curve> at <x>`: a curve's value at `x`, with the curve written as it would be in a
/// content file, so a designer can try a shape before using it (DESIGN.md §4.2).
fn curve(args: &str) -> Outcome {
    match evaluate_curve(args) {
        Ok(value) => Outcome::Output(value.to_string()),
        Err(message) => Outcome::Error(message),
    }
}

fn evaluate_curve(args: &str) -> Result<Fixed, String> {
    const USAGE: &str = "curve needs the form: curve <curve> at <x>, for example: curve [[0, 1.0], [100, 0.5]] at 25";
    let (spec, x) = args.rsplit_once(" at ").ok_or(USAGE)?;
    let x: Fixed = x
        .trim()
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    let curve = factional_content::parse_curve(spec.trim())?;
    Ok(curve.at(x))
}

/// Whether a (trimmed) line has nothing to run: it's blank, or a `#` comment.
pub(crate) fn is_blank_or_comment(line: &str) -> bool {
    line.is_empty() || line.starts_with('#')
}

/// Splits a line into its command name and the rest of the line.
fn split_command(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (line, ""),
    }
}

/// Splits `<command> == <expected>` at the first ` == `. A trailing ` ==` expects an empty result.
fn split_assert(spec: &str) -> Option<(&str, &str)> {
    spec.split_once(" == ")
        .or_else(|| spec.strip_suffix(" ==").map(|command| (command, "")))
}

/// The list of commands that `help` prints, with their descriptions aligned.
pub fn help_text() -> String {
    let width = COMMANDS
        .iter()
        .map(|(usage, _)| usage.len())
        .max()
        .unwrap_or(0);
    COMMANDS
        .iter()
        .map(|(usage, description)| format!("{usage:<width$}  {description}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(line: &str) -> Result<Outcome, ScriptError> {
        Session::default().execute(line)
    }

    fn passes() -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(String::new()))
    }

    #[test]
    fn echo_prints_the_rest_of_the_line_verbatim() {
        assert_eq!(
            run("echo hello  there"),
            Ok(Outcome::Output("hello  there".into()))
        );
    }

    #[test]
    fn echo_on_its_own_prints_nothing() {
        assert_eq!(run("echo"), Ok(Outcome::Output(String::new())));
    }

    #[test]
    fn fail_reports_its_message_as_a_command_error() {
        assert_eq!(run("fail boom"), Ok(Outcome::Error("boom".into())));
    }

    #[test]
    fn quit_ends_the_session() {
        assert_eq!(run("quit"), Ok(Outcome::Quit));
    }

    #[test]
    fn help_prints_the_command_list() {
        assert_eq!(run("help"), Ok(Outcome::Output(help_text())));
    }

    fn output(text: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(text.into()))
    }

    fn command_error(message: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Error(message.into()))
    }

    #[test]
    fn calc_does_the_engines_fixed_point_arithmetic() {
        assert_eq!(run("calc 4.00 * 0.41"), output("1.64"));
        assert_eq!(run("calc 0.05 * 0.50"), output("0.03"));
        assert_eq!(run("calc 2 / 3"), output("0.67"));
        assert_eq!(run("calc 12.5 + -0.05"), output("12.45"));
        assert_eq!(run("calc 1 - 3.25"), output("-2.25"));
    }

    #[test]
    fn calc_reports_bad_numbers_and_impossible_results_as_command_errors() {
        assert_eq!(run("calc 1 / 0"), command_error("cannot divide by 0.00"));
        assert_eq!(
            run("calc 12.345 + 1"),
            command_error("12.345 has more than 2 decimal places")
        );
        assert_eq!(run("calc 1 + abc"), command_error("'abc' is not a number"));
        assert_eq!(
            run("calc 92233720368547758.07 + 1"),
            command_error("the result is out of range")
        );
        assert_eq!(
            run("calc -92233720368547758.08 - 1"),
            command_error("the result is out of range")
        );
        assert_eq!(
            run("calc 92233720368547758.07 * 2"),
            command_error("the result is out of range")
        );
    }

    #[test]
    fn curve_gives_a_curves_value_at_a_point() {
        assert_eq!(
            run("curve [[0, 1.00], [50, 0.70], [100, 0.30]] at 25"),
            output("0.85")
        );
        assert_eq!(
            run("curve [[0, 50], [60, 0], [200, -50]] at -7"),
            output("50.00")
        );
        assert_eq!(run("curve 1.0 at 999"), output("1.00"));
    }

    #[test]
    fn curve_reports_invalid_curves_and_bad_numbers_as_command_errors() {
        assert_eq!(
            run("curve [[50, 1.0], [40, 0.0]] at 45"),
            command_error("curve points must have increasing x: 50.00 then 40.00")
        );
        assert_eq!(
            run("curve [[0, 1.0], [10, 2.0]] at 1.234"),
            command_error("1.234 has more than 2 decimal places")
        );
    }

    #[test]
    fn curve_needs_a_curve_and_a_point() {
        let usage = command_error(
            "curve needs the form: curve <curve> at <x>, for example: curve [[0, 1.0], [100, 0.5]] at 25",
        );
        assert_eq!(run("curve"), usage);
        assert_eq!(run("curve [[0, 1.0], [100, 0.5]]"), usage);
        assert_eq!(run("curve at 5"), usage);
    }

    #[test]
    fn calc_needs_two_numbers_and_an_operator() {
        let usage =
            command_error("calc needs the form: calc <a> <op> <b>, where <op> is + - * or /");
        assert_eq!(run("calc"), usage);
        assert_eq!(run("calc 1 +"), usage);
        assert_eq!(run("calc 1 + 2 + 3"), usage);
        assert_eq!(run("calc 1 % 2"), usage);
    }

    #[test]
    fn help_lists_every_command_with_its_usage() {
        let help = help_text();
        for usage in [
            "help",
            "quit",
            "calc <a> <op> <b>",
            "curve <curve> at <x>",
            "echo <text>",
            "fail <message>",
            "assert <command> == <expected>",
        ] {
            assert!(help.contains(usage), "help is missing {usage:?}:\n{help}");
        }
    }

    #[test]
    fn an_unknown_command_is_a_script_error_that_names_it() {
        let error = run("frobnicate now").unwrap_err();
        assert_eq!(error.to_string(), "unknown command 'frobnicate'");
    }

    #[test]
    fn a_matching_assert_passes_without_output() {
        assert_eq!(run("assert echo hi == hi"), passes());
    }

    #[test]
    fn a_mismatched_assert_reports_what_was_expected_and_what_came_back() {
        let error = run("assert echo hi == bye").unwrap_err();
        assert_eq!(error.to_string(), "expected 'bye', got 'hi'");
    }

    #[test]
    fn assert_compares_a_failed_command_as_its_error_text() {
        assert_eq!(run("assert fail boom == error: boom"), passes());
    }

    #[test]
    fn assert_splits_at_the_first_separator() {
        let error = run("assert echo a == b == c").unwrap_err();
        assert_eq!(error.to_string(), "expected 'b == c', got 'a'");
    }

    #[test]
    fn assert_can_expect_an_empty_result() {
        assert_eq!(run("assert echo =="), passes());
    }

    #[test]
    fn assert_without_a_separator_is_a_script_error() {
        let error = run("assert echo hi").unwrap_err();
        assert_eq!(
            error.to_string(),
            "assert needs the form: assert <command> == <expected>"
        );
    }

    #[test]
    fn assert_of_an_unknown_command_is_a_script_error() {
        let error = run("assert frobnicate == anything").unwrap_err();
        assert_eq!(error.to_string(), "unknown command 'frobnicate'");
    }

    #[test]
    fn rendering_prefixes_errors_and_shows_output_as_is() {
        assert_eq!(Outcome::Output("hi".into()).render(), "hi");
        assert_eq!(Outcome::Error("boom".into()).render(), "error: boom");
        assert_eq!(Outcome::Quit.render(), "");
    }
}

use std::fmt;
use std::path::PathBuf;

use factional_core::{Fixed, ParseFixedError, suggest};
use factional_reputation::{Change, Character, Command, Event, World};

/// Every command as `(usage, description)`, in the order `help` lists them.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "list these commands"),
    ("quit", "leave the REPL, or end a script early"),
    (
        "load <dir>",
        "load the content files in <dir>, such as content/sample, as a new world",
    ),
    ("characters", "list the loaded characters"),
    (
        "show character <id>",
        "a character's alignment and its label",
    ),
    ("advance <ticks>", "move time forward"),
    ("time", "the current tick"),
    (
        "events [--since <seq>]",
        "what has happened, oldest first; --since shows only later events",
    ),
    (
        "journal",
        "every command issued, and whether it was accepted",
    ),
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
#[derive(Debug)]
pub struct Session {
    /// Relative paths, as in `load content/sample`, are resolved against this.
    base_dir: PathBuf,
    world: Option<World>,
}

impl Default for Session {
    /// A session whose relative paths start from the current directory.
    fn default() -> Session {
        Session::new(".")
    }
}

impl Session {
    pub fn new(base_dir: impl Into<PathBuf>) -> Session {
        Session {
            base_dir: base_dir.into(),
            world: None,
        }
    }

    /// Runs one command line.
    pub fn execute(&mut self, line: &str) -> Result<Outcome, ScriptError> {
        let (name, rest) = split_command(line);
        match name {
            "help" => Ok(Outcome::Output(help_text())),
            "quit" => Ok(Outcome::Quit),
            "load" => Ok(self.load(rest)),
            "characters" => Ok(self.characters()),
            "show" => Ok(self.show(rest)),
            "advance" => Ok(self.advance(rest)),
            "time" => Ok(self.time()),
            "events" => Ok(self.events(rest)),
            "journal" => Ok(self.journal()),
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

    /// `load <dir>`: reads and validates a content directory, replacing the loaded world only
    /// if every file is valid.
    fn load(&mut self, dir: &str) -> Outcome {
        if dir.is_empty() {
            return Outcome::Error("load needs the form: load <dir>".to_owned());
        }
        match factional_content::load_dir(&self.base_dir.join(dir)) {
            Ok(content) => {
                let count = content.characters.len();
                let noun = if count == 1 {
                    "character"
                } else {
                    "characters"
                };
                self.world = Some(World::new(content));
                Outcome::Output(format!("loaded {count} {noun} from {dir}"))
            }
            Err(error) => Outcome::Error(error.to_string()),
        }
    }

    /// `characters`: every loaded character, one per line, in id order.
    fn characters(&self) -> Outcome {
        match &self.world {
            Some(world) => {
                let lines: Vec<String> = world
                    .characters()
                    .map(|character| describe(world, character))
                    .collect();
                Outcome::Output(lines.join("\n"))
            }
            None => no_world(),
        }
    }

    /// `show character <id>`.
    fn show(&self, args: &str) -> Outcome {
        let ["character", id] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("show needs the form: show character <id>".to_owned());
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        match world
            .characters()
            .find(|character| character.id.as_str() == id)
        {
            Some(character) => Outcome::Output(describe(world, character)),
            None => {
                let ids = world.characters().map(|character| character.id.as_str());
                let hint = suggest(id, ids)
                    .map(|close| format!(" (did you mean '{close}'?)"))
                    .unwrap_or_default();
                Outcome::Error(format!("unknown character '{id}'{hint}"))
            }
        }
    }
}

impl Session {
    /// `advance <ticks>`: moves time forward and shows the events it caused.
    fn advance(&mut self, args: &str) -> Outcome {
        let [ticks] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("advance needs the form: advance <ticks>".to_owned());
        };
        let Ok(ticks) = ticks.parse::<u64>() else {
            return Outcome::Error(format!("'{ticks}' is not a whole number of ticks"));
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(Command::AdvanceTime { ticks }) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `time`: the current tick.
    fn time(&self) -> Outcome {
        match &self.world {
            Some(world) => Outcome::Output(format!("tick {}", world.now())),
            None => no_world(),
        }
    }

    /// `events [--since <seq>]`: what has happened, oldest first.
    fn events(&self, args: &str) -> Outcome {
        let since = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [] => None,
            ["--since", seq] => match seq.parse::<u64>() {
                Ok(seq) => Some(seq),
                Err(_) => return Outcome::Error(format!("'{seq}' is not an event number")),
            },
            _ => {
                return Outcome::Error("events needs the form: events [--since <seq>]".to_owned());
            }
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let events = world.events_since(since.unwrap_or(0));
        Outcome::Output(match (events.is_empty(), since) {
            (false, _) => lines(events.iter().map(describe_event)),
            (true, Some(seq)) => format!("no events after #{seq}"),
            (true, None) => "no events yet".to_owned(),
        })
    }

    /// `journal`: every command issued, and whether it was accepted.
    fn journal(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        if world.journal().is_empty() {
            return Outcome::Output("no commands yet".to_owned());
        }
        Outcome::Output(lines(world.journal().iter().enumerate().map(
            |(index, entry)| {
                let result = match &entry.result {
                    Ok(()) => "accepted".to_owned(),
                    Err(refusal) => format!("refused: {refusal}"),
                };
                format!(
                    "{}. {} — {result}",
                    index + 1,
                    describe_command(&entry.command)
                )
            },
        )))
    }
}

/// `#1 at tick 0: time advanced from 0 to 5`.
fn describe_event(event: &Event) -> String {
    let what = match &event.payload {
        Change::TimeAdvanced { from, to } => format!("time advanced from {from} to {to}"),
    };
    format!("#{} at tick {}: {what}", event.seq, event.tick)
}

/// A command written the way it's typed in the CLI.
fn describe_command(command: &Command) -> String {
    match command {
        Command::AdvanceTime { ticks } => format!("advance {ticks}"),
    }
}

fn lines(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join("\n")
}

fn no_world() -> Outcome {
    Outcome::Error("no world is loaded yet: use load <dir> first".to_owned())
}

/// One line about a character: `vex — Vex — law -55.00, good -20.00 — Chaotic Neutral`.
fn describe(world: &World, character: &Character) -> String {
    let alignment = character.alignment;
    format!(
        "{} — {} — law {}, good {} — {}",
        character.id,
        character.name,
        alignment.law(),
        alignment.good(),
        alignment.label(world.balance().label_threshold)
    )
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

    /// The repository's root, so tests can load `content/sample`.
    fn repo() -> Session {
        Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    fn riverhold() -> Session {
        let mut session = repo();
        assert_eq!(
            session.execute("load content/sample"),
            output("loaded 6 characters from content/sample")
        );
        session
    }

    #[test]
    fn show_character_gives_the_alignment_and_its_label() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("show character vex"),
            output("vex — Vex — law -55.00, good -20.00 — Chaotic Neutral")
        );
        assert_eq!(
            session.execute("show character sister_mira"),
            output("sister_mira — Sister Mira — law 35.00, good 85.00 — Lawful Good")
        );
    }

    #[test]
    fn characters_lists_everyone_in_id_order() {
        assert_eq!(
            riverhold().execute("characters"),
            output(
                "brother_ash — Brother Ash — law 25.00, good -70.00 — Neutral Evil\n\
                 captain_hale — Captain Hale — law 75.00, good 30.00 — Lawful Neutral\n\
                 merchant_ava — Merchant Ava — law 20.00, good 10.00 — True Neutral\n\
                 player — The Player — law 0.00, good 0.00 — True Neutral\n\
                 sister_mira — Sister Mira — law 35.00, good 85.00 — Lawful Good\n\
                 vex — Vex — law -55.00, good -20.00 — Chaotic Neutral"
            )
        );
    }

    #[test]
    fn labels_use_the_loaded_threshold() {
        let mut session = repo();
        assert_eq!(
            session.execute("load crates/cli/tests/fixtures/worlds/stern"),
            output("loaded 1 character from crates/cli/tests/fixtures/worlds/stern")
        );
        assert_eq!(
            session.execute("show character vex"),
            output("vex — Vex — law -55.00, good -20.00 — True Neutral")
        );
    }

    #[test]
    fn load_reports_every_problem_and_keeps_the_world_already_loaded() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("load crates/cli/tests/fixtures/worlds/broken"),
            command_error(
                "characters.toml: hale: missing 'alignment'\n\
                 characters.toml: hale: unknown key 'alignmnet' (did you mean 'alignment'?)\n\
                 characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00"
            )
        );
        assert_eq!(
            session.execute("show character vex"),
            output("vex — Vex — law -55.00, good -20.00 — Chaotic Neutral")
        );
    }

    #[test]
    fn load_reports_a_directory_it_cannot_read() {
        let Ok(Outcome::Error(message)) = repo().execute("load no/such/world") else {
            panic!("expected a command error");
        };
        assert!(
            message.contains("no/such/world: cannot read the directory: "),
            "{message}"
        );
    }

    #[test]
    fn load_needs_a_directory() {
        assert_eq!(
            run("load"),
            command_error("load needs the form: load <dir>")
        );
    }

    #[test]
    fn show_suggests_a_close_id_for_an_unknown_character() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("show character vx"),
            command_error("unknown character 'vx' (did you mean 'vex'?)")
        );
        assert_eq!(
            session.execute("show character nobody"),
            command_error("unknown character 'nobody'")
        );
    }

    #[test]
    fn show_needs_a_kind_and_an_id() {
        let mut session = riverhold();
        let usage = command_error("show needs the form: show character <id>");
        for line in [
            "show",
            "show character",
            "show faction vex",
            "show character vex hale",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
    }

    #[test]
    fn world_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        assert_eq!(run("characters"), none);
        assert_eq!(run("show character vex"), none);
    }

    #[test]
    fn advance_moves_time_and_shows_the_events() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("advance 5"),
            output("#1 at tick 0: time advanced from 0 to 5")
        );
        assert_eq!(session.execute("time"), output("tick 5"));
        assert_eq!(
            session.execute("advance 3"),
            output("#2 at tick 5: time advanced from 5 to 8")
        );
    }

    #[test]
    fn advance_reports_a_refusal_and_bad_input() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("advance 0"),
            command_error("ticks must be at least 1")
        );
        for bad in ["x", "-1", "1.5"] {
            assert_eq!(
                session.execute(&format!("advance {bad}")),
                command_error(&format!("'{bad}' is not a whole number of ticks"))
            );
        }
        let usage = command_error("advance needs the form: advance <ticks>");
        assert_eq!(session.execute("advance"), usage);
        assert_eq!(session.execute("advance 1 2"), usage);
        assert_eq!(session.execute("time"), output("tick 0"));
    }

    #[test]
    fn events_lists_what_happened_with_an_optional_starting_point() {
        let mut session = riverhold();
        assert_eq!(session.execute("events"), output("no events yet"));
        for ticks in [1, 2, 3] {
            session.execute(&format!("advance {ticks}")).expect("valid");
        }
        assert_eq!(
            session.execute("events"),
            output(
                "#1 at tick 0: time advanced from 0 to 1\n\
                 #2 at tick 1: time advanced from 1 to 3\n\
                 #3 at tick 3: time advanced from 3 to 6"
            )
        );
        assert_eq!(
            session.execute("events --since 2"),
            output("#3 at tick 3: time advanced from 3 to 6")
        );
        assert_eq!(
            session.execute("events --since 3"),
            output("no events after #3")
        );
        assert_eq!(
            session.execute("events --since x"),
            command_error("'x' is not an event number")
        );
        let usage = command_error("events needs the form: events [--since <seq>]");
        assert_eq!(session.execute("events 3"), usage);
        assert_eq!(session.execute("events --since"), usage);
    }

    #[test]
    fn journal_lists_every_command_and_whether_it_was_accepted() {
        let mut session = riverhold();
        assert_eq!(session.execute("journal"), output("no commands yet"));
        for line in ["advance 5", "advance 0", "advance 2"] {
            session.execute(line).expect("valid");
        }
        assert_eq!(
            session.execute("journal"),
            output(
                "1. advance 5 — accepted\n\
                 2. advance 0 — refused: ticks must be at least 1\n\
                 3. advance 2 — accepted"
            )
        );
    }

    #[test]
    fn loading_starts_a_new_world() {
        let mut session = riverhold();
        session.execute("advance 5").expect("valid");
        session.execute("load content/sample").expect("valid");
        assert_eq!(session.execute("time"), output("tick 0"));
        assert_eq!(session.execute("journal"), output("no commands yet"));
    }

    #[test]
    fn time_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in ["advance 1", "time", "events", "journal"] {
            assert_eq!(run(line), none, "{line}");
        }
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
            "load <dir>",
            "characters",
            "show character <id>",
            "advance <ticks>",
            "time",
            "events [--since <seq>]",
            "journal",
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

//! The `factional` binary: `factional repl` for an interactive session, `factional run
//! <script>` to replay a scenario (DECISIONS.md P-27), and for designers, `factional validate
//! <dir>` and `factional schema [<file>]`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand};
use factional_cli::{compare, run_repl, run_script, validate};
use factional_content::{SCHEMA_FILES, schema_text};

#[derive(Parser)]
#[command(
    name = "factional",
    version,
    about = "Factional's command line: an interactive REPL and a scenario runner"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start an interactive session; `help` lists the commands
    Repl,
    /// Run a scenario script, stopping at the first line that goes wrong
    Run {
        /// The `.scenario` file to run
        script: PathBuf,
    },
    /// Run a scenario on two content directories and show what came out differently
    Compare {
        /// The `.scenario` file to run; it must load exactly one world
        scenario: PathBuf,
        /// The content directory to run it on first
        #[arg(long)]
        content: String,
        /// The content directory to compare it with
        #[arg(long)]
        against: String,
    },
    /// Check a content directory without starting a session
    Validate {
        /// The content directory, such as content/sample
        dir: String,
    },
    /// Print a content file's JSON Schema, for editors; without a file, list the files
    Schema {
        /// The content file, such as factions
        #[arg(value_parser = PossibleValuesParser::new(SCHEMA_FILES))]
        file: Option<String>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Repl => run_repl(),
        Command::Run { script } => run(&script),
        Command::Compare {
            scenario,
            content,
            against,
        } => {
            let source = match fs::read_to_string(&scenario) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("cannot read {}: {error}", scenario.display());
                    return ExitCode::FAILURE;
                }
            };
            let shown = scenario.display().to_string();
            match compare(&shown, &source, &content, &against, Path::new("")) {
                Ok(report) => {
                    print!("{report}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Validate { dir } => {
            let (report, loads) = validate(Path::new(""), &dir);
            print!("{report}");
            if loads {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Command::Schema { file } => {
            match file.as_deref().and_then(schema_text) {
                Some(schema) => print!("{schema}"),
                None => SCHEMA_FILES.iter().for_each(|file| println!("{file}")),
            }
            ExitCode::SUCCESS
        }
    }
}

/// Prints the script's transcript; if it stops early, reports the failing line on stderr.
fn run(path: &Path) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match run_script(&source, Path::new(".")) {
        Ok(transcript) => {
            print!("{transcript}");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            print!("{}", failure.transcript);
            eprintln!("{failure}");
            ExitCode::FAILURE
        }
    }
}

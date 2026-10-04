//! The `factional` binary: `factional repl` for an interactive session, and
//! `factional run <script>` to replay a scenario (DECISIONS.md P-27).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use factional_cli::{run_repl, run_script};

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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Repl => run_repl(),
        Command::Run { script } => run(&script),
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

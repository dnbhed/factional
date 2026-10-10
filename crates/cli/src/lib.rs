//! The `factional` command-line tool: an interactive REPL and a scenario-script runner over
//! the same set of commands, so any manual session can be saved as a scenario and replayed as
//! a regression test (DECISIONS.md P-27).

mod charts;
mod check;
mod compare;
mod form;
mod graph;
mod outline;
mod play;
mod quests;
mod references;
mod repl;
mod script;
mod session;

pub use check::validate;
pub use compare::compare;
pub use repl::run_repl;
pub use script::{RunFailure, run_script};
pub use session::{Outcome, ScriptError, Session, help_text};

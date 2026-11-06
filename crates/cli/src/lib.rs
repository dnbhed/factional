//! The `factional` command-line tool: an interactive REPL and a scenario-script runner over
//! the session's commands (`factional-session`), so any manual session can be saved as a
//! scenario and replayed as a regression test (DECISIONS.md P-27).

mod repl;
mod script;

pub use factional_session::{
    Import, Outcome, ScriptError, Session, compare, help_text, validate, world_dismiss,
    world_import, world_info, world_restore, world_source, world_suggestions,
};
pub use repl::run_repl;
pub use script::{RunFailure, run_script};

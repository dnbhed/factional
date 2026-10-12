//! The REPL's session (D-39, P-95): every command, with its output and `help`, over a loaded
//! world and its journal. The CLI's REPL and scenario runner drive it, and the editor's Play
//! will, so a session typed anywhere can be saved as a scenario and replayed as a test
//! (P-27). It reads and writes content directories and saves, as the CLI always has; the
//! no-I/O rule stays for core, reputation and quests.

mod charts;
mod check;
mod compare;
mod fixes;
mod form;
mod graph;
mod keys;
mod outline;
mod play;
mod quests;
mod references;
mod session;
mod words;

pub use check::validate;
pub use compare::compare;
pub use session::{Outcome, ScriptError, Session, help_text, is_blank_or_comment};

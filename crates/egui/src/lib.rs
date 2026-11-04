//! What the editor and the inspector both draw, in egui (P-118, P-122): the console (G2a) and
//! the relations matrix (G3), so neither is drawn twice; and how the editor and the builder
//! open each other (G8).

pub use egui;

pub mod console;
pub mod open;
pub mod relations;

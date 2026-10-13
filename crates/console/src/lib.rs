//! The console (G2a, P-118, P-122): what a session said, each id in it a link, and the line
//! to type a command on, its history by arrow and Tab completing it; drawn with egui for the
//! editor's Play and the inspector alike. It holds no session: it hands back what to run and
//! which id was followed, and is told what was said.

pub use egui;

use egui::{Key, RichText};
use factional_session::{Mark, Named};

/// A console's log, its line and its history.
pub struct Console {
    /// What it's called, which names its line: "Play" gives "Play command".
    name: String,
    /// Each command run and what it said, with the ids in it marked (U15c); with no command,
    /// what was said on its own, such as what loading said.
    log: Vec<(Option<String>, String, Vec<Mark>)>,
    /// What Tab offered, when it offered more than one (U15c).
    offered: Option<String>,
    /// The command being typed.
    input: String,
    /// The commands run, oldest first, and how far back the up arrow has gone.
    history: Vec<String>,
    back: Option<usize>,
    /// Whether the line is to take the focus next frame.
    focus: bool,
}

/// What the console asks of whoever holds the session, after a frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Asked {
    /// A command typed and entered.
    pub run: Option<String>,
    /// An id followed from what was said.
    pub followed: Option<(Named, String)>,
}

impl Console {
    /// A console called `name`, which names its line: "Play" gives "Play command".
    pub fn new(name: impl Into<String>) -> Console {
        Console {
            name: name.into(),
            log: Vec::new(),
            offered: None,
            input: String::new(),
            history: Vec::new(),
            back: None,
            focus: false,
        }
    }

    /// What was said, after `command`, or with none, on its own (such as what loading said).
    pub fn say(&mut self, command: Option<String>, said: String, marks: Vec<Mark>) {
        self.log.push((command, said, marks));
    }

    /// Each command run and what was said after it, oldest first.
    pub fn log(&self) -> &[(Option<String>, String, Vec<Mark>)] {
        &self.log
    }

    /// The line being typed.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Sets the line being typed.
    pub fn set_input(&mut self, line: &str) {
        line.clone_into(&mut self.input);
    }

    /// What Tab offered, when it offered more than one.
    pub fn offered(&self) -> Option<&str> {
        self.offered.as_deref()
    }

    /// Puts the focus on the line next frame, so what's typed goes there.
    pub fn focus(&mut self) {
        self.focus = true;
    }

    /// The line typed, taken to run and kept in the history; nothing, if it's blank.
    fn enter(&mut self) -> Option<String> {
        let line = self.input.trim().to_owned();
        if line.is_empty() {
            return None;
        }
        self.offered = None;
        self.history.push(line.clone());
        self.input.clear();
        self.back = None;
        Some(line)
    }

    /// A command from the history: the one before what's shown, or with `down` the one after,
    /// and after the last, nothing.
    pub fn recall(&mut self, down: bool) {
        let Some(last) = self.history.len().checked_sub(1) else {
            return;
        };
        self.back = match (self.back, down) {
            (None, false) => Some(last),
            (None, true) => None,
            (Some(at), false) => Some(at.saturating_sub(1)),
            (Some(at), true) => (at < last).then_some(at + 1),
        };
        self.input = self
            .back
            .map(|at| self.history[at].clone())
            .unwrap_or_default();
    }

    /// Completes the last word typed from `found`, what it could be (U15c): to the one thing
    /// it could be, or with more than one, to what they share, listing them.
    pub fn complete(&mut self, found: &[String]) {
        let start = self.input.rfind(' ').map_or(0, |at| at + 1);
        let shared = shared(found);
        match found.len() {
            0 => {}
            1 => {
                self.input.replace_range(start.., &format!("{shared} "));
                self.offered = None;
            }
            _ => {
                self.input.replace_range(start.., &shared);
                self.offered = Some(found.join(", "));
            }
        }
    }

    /// The console: `notice` above what's been said, if there is one; each command after `>`
    /// and each id said a link (U15c); and the line to type one, labelled "<name> command",
    /// Tab completing it from `complete`. What was entered, and the id followed, if any.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        notice: Option<&str>,
        mut complete: impl FnMut(&str) -> Vec<String>,
    ) -> Asked {
        let mut asked = Asked::default();
        let label = format!("{} command", self.name);
        // The line to type on at the bottom, and what's been said filling what's above it.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .id_salt(&label)
                    .hint_text("a command, such as characters")
                    .font(egui::TextStyle::Monospace)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut self.focus) {
                field.request_focus();
            }
            // Tab completes the line, which keeps the focus.
            let tab = ui.input(|input| input.key_pressed(Key::Tab));
            if tab && (field.has_focus() || field.lost_focus()) {
                self.input.retain(|c| c != '\t');
                let found = complete(&self.input);
                self.complete(&found);
                field.request_focus();
                // The cursor goes to the end, after what was completed.
                if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), field.id) {
                    let end = egui::text::CCursor::new(self.input.chars().count());
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), field.id);
                }
            }
            let typed = self.input.clone();
            field.widget_info(|| {
                let mut info = egui::WidgetInfo::text_edit(true, "", typed.as_str(), "");
                info.label = Some(label.clone());
                info
            });
            if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                asked.run = self.enter();
                field.request_focus();
                // What it says comes next frame, once it's run.
                ui.ctx().request_repaint();
            }
            if field.has_focus() {
                for (key, down) in [(Key::ArrowUp, false), (Key::ArrowDown, true)] {
                    if ui.input(|input| input.key_pressed(key)) {
                        self.recall(down);
                    }
                }
            }
            if let Some(offered) = &self.offered {
                ui.weak(offered);
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                if let Some(notice) = notice {
                    ui.colored_label(ui.visuals().warn_fg_color, notice);
                }
                egui::ScrollArea::vertical()
                    .id_salt(&self.name)
                    .auto_shrink(false)
                    // Only the room above the line, however little, so it never covers it.
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height())
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (command, said, marks) in &self.log {
                            if let Some(command) = command {
                                ui.label(
                                    RichText::new(format!("> {command}")).monospace().strong(),
                                );
                            }
                            if let Some(id) = said_ui(ui, said, marks) {
                                asked.followed = Some(id);
                            }
                        }
                    });
            });
        });
        asked
    }
}

/// A piece of a line said: its words, or an id marked in it, with what it names.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'s> {
    Words(&'s str),
    Id(&'s str, Named),
}

/// What was said, a line at a time, each line its words and the ids marked in them, in
/// order; no piece is empty.
pub fn pieces<'s>(said: &'s str, marks: &[Mark]) -> Vec<Vec<Piece<'s>>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for line in said.split('\n') {
        let end = start + line.len();
        let mut pieces = Vec::new();
        let mut at = start;
        for mark in marks
            .iter()
            .filter(|mark| mark.start >= start && mark.end <= end)
        {
            if mark.start > at {
                pieces.push(Piece::Words(&said[at..mark.start]));
            }
            pieces.push(Piece::Id(&said[mark.start..mark.end], mark.named));
            at = mark.end;
        }
        if end > at {
            pieces.push(Piece::Words(&said[at..end]));
        }
        lines.push(pieces);
        start = end + 1;
    }
    lines
}

/// What all of `found` start with.
pub fn shared(found: &[String]) -> String {
    let Some(first) = found.first() else {
        return String::new();
    };
    let length = found
        .iter()
        .map(|other| {
            first
                .chars()
                .zip(other.chars())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .min()
        .unwrap_or_default();
    first.chars().take(length).collect()
}

/// What was said, a line at a time, each id marked in it a link to its entry, labelled "Go to
/// <id>"; a line of words alone, one label. The id followed, if one was.
fn said_ui(ui: &mut egui::Ui, said: &str, marks: &[Mark]) -> Option<(Named, String)> {
    let mut followed = None;
    for line in pieces(said, marks) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for piece in line {
                match piece {
                    Piece::Words(words) => {
                        ui.label(RichText::new(words).monospace());
                    }
                    Piece::Id(id, named) => {
                        let link = ui.link(RichText::new(id).monospace());
                        let label = format!("Go to {id}");
                        link.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &label)
                        });
                        if link.clicked() {
                            followed = Some((named, id.to_owned()));
                        }
                    }
                }
            }
        });
    }
    followed
}

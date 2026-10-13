//! Play (U15b, D-40, P-96): the REPL in the drawer, a `factional-session` session on the
//! content as it stands, unsaved edits included (P-112): loaded from the texts in memory, and
//! reloaded with `reload`'s rules each time they load, saying what that says; while they
//! don't, the last world that did.

use std::path::Path;

use factional_session::{ContentTexts, Mark, Named, Outcome, Session};

use crate::egui::{self, Key, RichText};

/// Play's session, what it has said, and what was typed.
pub(crate) struct Play {
    session: Session,
    /// The directory the content is from, as the session names it.
    name: String,
    /// Each command run and what it said, with the ids in it marked (U15c); with no command,
    /// what loading said.
    log: Vec<(Option<String>, String, Vec<Mark>)>,
    /// What Tab offered, when it offered more than one (U15c).
    offered: Option<String>,
    /// The command being typed.
    input: String,
    /// The commands run, oldest first, and how far back the up arrow has gone.
    history: Vec<String>,
    back: Option<usize>,
    /// The texts the session last loaded or reloaded.
    played: Option<ContentTexts>,
    /// Whether the content in memory doesn't load now, so Play has the last that did.
    stale: bool,
}

impl Play {
    /// Play for the content in `dir`, before anything is loaded.
    pub(crate) fn new(dir: &Path) -> Play {
        Play {
            session: Session::new(dir),
            name: dir.display().to_string(),
            log: Vec::new(),
            offered: None,
            input: String::new(),
            history: Vec::new(),
            back: None,
            played: None,
            stale: false,
        }
    }

    /// Follows the content in memory, `texts`, which `loads` or doesn't: the first that loads
    /// is loaded, and each change that loads after it reloaded with `reload`'s rules.
    pub(crate) fn follow(&mut self, texts: Option<&ContentTexts>, loads: bool) {
        let Some(texts) = texts else {
            return;
        };
        self.stale = !loads && self.played.is_some();
        if !loads || self.played.as_ref() == Some(texts) {
            return;
        }
        let said = match self.played {
            None => self.session.load_texts(&self.name, texts),
            Some(_) => self.session.reload_texts(texts),
        };
        let said = said.render();
        let marks = self.session.marks(&said);
        self.log.push((None, said, marks));
        self.played = Some(texts.clone());
    }

    /// Runs what's typed, keeping it in the history.
    fn run(&mut self) {
        let line = self.input.trim().to_owned();
        if line.is_empty() {
            return;
        }
        let said = match self.session.execute(&line) {
            Ok(Outcome::Quit) => "Play doesn't quit: close the drawer instead".to_owned(),
            Ok(outcome) => outcome.render(),
            Err(error) => format!("error: {error}"),
        };
        let marks = self.session.marks(&said);
        self.log.push((Some(line.clone()), said, marks));
        self.offered = None;
        self.history.push(line);
        self.input.clear();
        self.back = None;
    }

    /// A command from the history: the one before what's shown, or with `down` the one after,
    /// and after the last, nothing.
    fn recall(&mut self, down: bool) {
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

    /// Completes the last word typed (U15c): to the one thing it could be, or with more than
    /// one, to what they share, listing them.
    fn complete(&mut self) {
        let found = self.session.complete(&self.input);
        let start = self.input.rfind(' ').map_or(0, |at| at + 1);
        let shared = shared(&found);
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

    /// Play in the drawer: whether it has the last world that loaded, what's been said, each
    /// command after `>` and each id said a link (U15c), and the line to type one, labelled
    /// "Play command", Tab completing it. The id followed, if one was.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) -> Option<(Named, String)> {
        let mut followed = None;
        // The line to type on at the bottom, and what's been said filling what's above it.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .id_salt("play command")
                    .hint_text("a command, such as characters")
                    .font(egui::TextStyle::Monospace)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY),
            );
            // Tab completes the line, which keeps the focus.
            let tab = ui.input(|input| input.key_pressed(Key::Tab));
            if tab && (field.has_focus() || field.lost_focus()) {
                self.input.retain(|c| c != '\t');
                self.complete();
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
                info.label = Some("Play command".to_owned());
                info
            });
            if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                self.run();
                field.request_focus();
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
                if self.stale {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "The content doesn't load: Play has the last world that did.",
                    );
                }
                egui::ScrollArea::vertical()
                    .id_salt("play")
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
                                followed = Some(id);
                            }
                        }
                    });
            });
        });
        followed
    }
}

/// What all of `found` start with.
fn shared(found: &[String]) -> String {
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

/// A piece of a line Play said: its words, or an id marked in it, with what it names.
#[derive(Debug, PartialEq, Eq)]
enum Piece<'s> {
    Words(&'s str),
    Id(&'s str, Named),
}

/// What was said, a line at a time, each line its words and the ids marked in them, in
/// order; no piece is empty.
fn pieces<'s>(said: &'s str, marks: &[Mark]) -> Vec<Vec<Piece<'s>>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    use factional_content::read_texts;

    const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/sample");

    fn sample() -> ContentTexts {
        read_texts(Path::new(SAMPLE)).expect("the sample is there")
    }

    #[test]
    fn what_was_said_is_its_words_and_its_ids_a_line_at_a_time() {
        let said = "vex steals\n\nfrom merchant_ava, for vex";
        let mark = |start: usize, end: usize| Mark {
            start,
            end,
            named: Named::Character,
        };
        let marks = [mark(0, 3), mark(17, 29), mark(35, 38)];
        assert_eq!(
            pieces(said, &marks),
            [
                vec![Piece::Id("vex", Named::Character), Piece::Words(" steals")],
                vec![],
                vec![
                    Piece::Words("from "),
                    Piece::Id("merchant_ava", Named::Character),
                    Piece::Words(", for "),
                    Piece::Id("vex", Named::Character),
                ],
            ]
        );
    }

    #[test]
    fn what_completions_share_is_their_common_start() {
        let found = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(shared(&found(&["temple", "the_long_winter", "took"])), "t");
        assert_eq!(shared(&found(&["save", "saves"])), "save");
        assert_eq!(shared(&found(&["show"])), "show");
        assert_eq!(shared(&[]), "");
    }

    #[test]
    fn tab_with_nothing_to_complete_to_leaves_what_was_typed() {
        let mut play = Play::new(Path::new(SAMPLE));
        play.input = "zzz".to_owned();
        play.complete();
        assert_eq!(play.input, "zzz");
        assert_eq!(play.offered, None);
    }

    #[test]
    fn the_arrows_go_back_and_forth_through_what_was_run() {
        let mut play = Play::new(Path::new(SAMPLE));
        play.recall(false);
        assert_eq!(play.input, "", "nothing run, nothing to recall");
        play.history = ["a", "b", "c"].map(str::to_owned).to_vec();
        let mut walk = |down: bool| {
            play.recall(down);
            play.input.clone()
        };
        assert_eq!(walk(false), "c");
        assert_eq!(walk(false), "b");
        assert_eq!(walk(false), "a");
        assert_eq!(walk(false), "a", "the first stays");
        assert_eq!(walk(true), "b");
        assert_eq!(walk(true), "c");
        assert_eq!(walk(true), "", "after the last, nothing");
        assert_eq!(walk(true), "");
    }

    #[test]
    fn play_loads_the_first_texts_that_load_and_reloads_each_change_after() {
        let mut play = Play::new(Path::new(SAMPLE));
        let texts = sample();
        play.follow(Some(&texts), false);
        assert!(play.log.is_empty() && !play.stale, "nothing loaded yet");
        play.follow(Some(&texts), true);
        assert_eq!(play.log.len(), 1);
        assert!(play.log[0].1.starts_with("loaded 6 characters from"));
        play.follow(Some(&texts), true);
        assert_eq!(play.log.len(), 1, "the same texts aren't loaded again");
        let mut changed = texts.clone();
        if let Some(text) = changed[2].as_mut() {
            text.push_str("\n# changed\n");
        }
        play.follow(Some(&changed), false);
        assert!(play.stale);
        assert_eq!(play.log.len(), 1);
        play.follow(Some(&changed), true);
        assert!(!play.stale);
        assert_eq!(play.log.len(), 2);
        assert!(play.log[1].1.starts_with("reloaded "), "{}", play.log[1].1);
        play.follow(None, true);
        assert_eq!(play.log.len(), 2);
    }
}

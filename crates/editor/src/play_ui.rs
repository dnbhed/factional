//! Play (U15b, D-40, P-96): the REPL in the drawer, a `factional-session` session on the
//! content as it stands, unsaved edits included (P-112): loaded from the texts in memory, and
//! reloaded with `reload`'s rules each time they load, saying what that says; while they
//! don't, the last world that did.

use std::path::Path;

use factional_session::{ContentTexts, Outcome, Session};

use crate::egui::{self, Key, RichText};

/// Play's session, what it has said, and what was typed.
pub(crate) struct Play {
    session: Session,
    /// The directory the content is from, as the session names it.
    name: String,
    /// Each command run and what it said; with no command, what loading said.
    log: Vec<(Option<String>, String)>,
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
        self.log.push((None, said.render()));
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
        self.log.push((Some(line.clone()), said));
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

    /// Play in the drawer: whether it has the last world that loaded, what's been said, each
    /// command after `>`, and the line to type one, labelled "Play command".
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        // The line to type on at the bottom, and what's been said filling what's above it.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .id_salt("play command")
                    .hint_text("a command, such as characters")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
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
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (command, said) in &self.log {
                            if let Some(command) = command {
                                ui.label(
                                    RichText::new(format!("> {command}")).monospace().strong(),
                                );
                            }
                            if !said.is_empty() {
                                ui.label(RichText::new(said).monospace());
                            }
                        }
                    });
            });
        });
    }
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

//! Play (U15b, D-40, P-96): the REPL in the drawer, a `factional-session` session on the
//! content as it stands, unsaved edits included (P-112): loaded from the texts in memory, and
//! reloaded with `reload`'s rules each time they load, saying what that says; while they
//! don't, the last world that did.

use std::path::Path;

use factional_egui::console::Console;
use factional_session::{ContentTexts, Named, Outcome, Session};

use crate::egui;

/// Play's session, and its console (G2a): what it has said, and what was typed.
pub(crate) struct Play {
    session: Session,
    /// The directory the content is from, as the session names it.
    name: String,
    console: Console,
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
            console: Console::new("Play"),
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
        self.console.say(None, said, marks);
        self.played = Some(texts.clone());
    }

    /// Runs `line` in the session, and says what it said.
    fn run(&mut self, line: String) {
        let said = match self.session.execute(&line) {
            Ok(Outcome::Quit) => "Play doesn't quit: close the drawer instead".to_owned(),
            Ok(outcome) => outcome.render(),
            Err(error) => format!("error: {error}"),
        };
        let marks = self.session.marks(&said);
        self.console.say(Some(line), said, marks);
    }

    /// Play in the drawer: whether it has the last world that loaded, and its console. The id
    /// followed, if one was.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) -> Option<(Named, String)> {
        let notice = self
            .stale
            .then_some("The content doesn't load: Play has the last world that did.");
        let session = &self.session;
        let asked = self.console.ui(ui, notice, |typed| session.complete(typed));
        if let Some(line) = asked.run {
            self.run(line);
        }
        asked.followed
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
    fn play_loads_the_first_texts_that_load_and_reloads_each_change_after() {
        let mut play = Play::new(Path::new(SAMPLE));
        let texts = sample();
        play.follow(Some(&texts), false);
        assert!(
            play.console.log().is_empty() && !play.stale,
            "nothing loaded yet"
        );
        play.follow(Some(&texts), true);
        assert_eq!(play.console.log().len(), 1);
        assert!(
            play.console.log()[0]
                .1
                .starts_with("loaded 6 characters from")
        );
        play.follow(Some(&texts), true);
        assert_eq!(
            play.console.log().len(),
            1,
            "the same texts aren't loaded again"
        );
        let mut changed = texts.clone();
        if let Some(text) = changed[2].as_mut() {
            text.push_str("\n# changed\n");
        }
        play.follow(Some(&changed), false);
        assert!(play.stale);
        assert_eq!(play.console.log().len(), 1);
        play.follow(Some(&changed), true);
        assert!(!play.stale);
        assert_eq!(play.console.log().len(), 2);
        assert!(
            play.console.log()[1].1.starts_with("reloaded "),
            "{}",
            play.console.log()[1].1
        );
        play.follow(None, true);
        assert_eq!(play.console.log().len(), 2);
    }
}

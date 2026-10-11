//! The problems panel (U6c, board 3): every problem, or every warning, each with its file,
//! where it is in words (U10a) as a way to its entry, the loader's words, and its fix as "Use
//! 'right'". The fixes are `factional-content`'s `fix_for`; this draws them.

use factional_content::Fix;

use crate::egui::{self, RichText};
use crate::{Editor, Said};

/// What was chosen in the panel.
pub(crate) enum Listed {
    /// A key followed, as its file and key.
    Go(String, String),
    Fix(Fix),
    /// Problems (`false`) or warnings (`true`) chosen to list.
    Warnings(bool),
}

impl Editor {
    /// The panel's choice of problems or warnings, each counted, then those chosen.
    pub(crate) fn problems_ui(&self, ui: &mut egui::Ui) -> Option<Listed> {
        let mut listed = None;
        let warnings = self.listing_warnings();
        let (problems, cautions): (Vec<&Said>, Vec<&Said>) =
            self.said().iter().partition(|said| !said.warning);
        ui.horizontal(|ui| {
            let label = format!("Problems {}", problems.len());
            if ui.selectable_label(!warnings, label).clicked() {
                listed = Some(Listed::Warnings(false));
            }
            let label = format!("Warnings {}", cautions.len());
            if ui.selectable_label(warnings, label).clicked() {
                listed = Some(Listed::Warnings(true));
            }
        });
        ui.separator();
        let shown = if warnings { cautions } else { problems };
        if shown.is_empty() {
            ui.weak(if warnings {
                "No warnings"
            } else {
                "No problems"
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("problems")
            .show(ui, |ui| {
                for said in shown {
                    if let Some(chosen) = said_ui(ui, said) {
                        listed = Some(chosen);
                    }
                }
            });
        listed
    }
}

/// One problem or warning: its file, where it is as a link, its key on hover, the loader's
/// words and its fix.
fn said_ui(ui: &mut egui::Ui, said: &Said) -> Option<Listed> {
    let mut chosen = None;
    let diagnostic = &said.diagnostic;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&diagnostic.file).monospace().weak());
        if let Some(key) = &diagnostic.key {
            let place = within(said, 0).unwrap_or_else(|| key.clone());
            let link = ui.link(place).on_hover_text(RichText::new(key).monospace());
            if link.clicked() {
                chosen = Some(Listed::Go(diagnostic.file.clone(), key.clone()));
            }
        }
        ui.label(&diagnostic.message);
        if let Some(fixed) = fix_button(ui, said) {
            chosen = Some(Listed::Fix(fixed));
        }
    });
    chosen
}

/// Where a problem or warning is in words, after the first `steps` steps of its key, such as
/// those of its entry; `None` if there's nothing after them.
pub(crate) fn within(said: &Said, steps: usize) -> Option<String> {
    let words: Vec<&str> = said
        .words
        .iter()
        .filter(|word| word.through > steps)
        .map(|word| word.text.as_str())
        .collect();
    (!words.is_empty()).then(|| words.join(" › "))
}

/// "Use 'right'", for a problem or warning with a fix; the fix, if it was clicked.
pub(crate) fn fix_button(ui: &mut egui::Ui, said: &Said) -> Option<Fix> {
    let fix = said.fix.as_ref()?;
    let right = &said.diagnostic.suggestion.as_ref()?.right;
    ui.button(format!("Use '{right}'"))
        .clicked()
        .then(|| fix.clone())
}

#[cfg(test)]
mod tests {
    use factional_content::{Diagnostic, Word};

    use super::*;

    #[test]
    fn a_problems_place_within_its_entry_leaves_out_the_entrys_own_words() {
        let word = |text: &str, through: usize| Word {
            text: text.to_owned(),
            through,
        };
        let said = Said {
            diagnostic: Diagnostic {
                file: "factions.toml".to_owned(),
                key: Some("temple.drift.policy".to_owned()),
                message: "unknown drift policy 'demot'".to_owned(),
                suggestion: None,
            },
            warning: false,
            fix: None,
            words: vec![
                word("Temple of the Dawn", 1),
                word("Drift", 2),
                word("Policy", 3),
            ],
        };
        assert_eq!(
            within(&said, 0).as_deref(),
            Some("Temple of the Dawn › Drift › Policy")
        );
        assert_eq!(within(&said, 1).as_deref(), Some("Drift › Policy"));
        assert_eq!(within(&said, 3), None);
    }
}

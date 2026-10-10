//! The problems panel (U6c, board 3): every problem, or every warning, each with its file,
//! its key as a way to its entry, the loader's words, and its fix as "Use 'right'". The fixes
//! are `factional-content`'s `fix_for`; this draws them.

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

/// One problem or warning: its file, its key as a link, the loader's words and its fix.
fn said_ui(ui: &mut egui::Ui, said: &Said) -> Option<Listed> {
    let mut chosen = None;
    let diagnostic = &said.diagnostic;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&diagnostic.file).monospace().weak());
        if let Some(key) = &diagnostic.key
            && ui.link(key).clicked()
        {
            chosen = Some(Listed::Go(diagnostic.file.clone(), key.clone()));
        }
        ui.label(&diagnostic.message);
        if let Some(fixed) = fix_button(ui, said) {
            chosen = Some(Listed::Fix(fixed));
        }
    });
    chosen
}

/// "Use 'right'", for a problem or warning with a fix; the fix, if it was clicked.
pub(crate) fn fix_button(ui: &mut egui::Ui, said: &Said) -> Option<Fix> {
    let fix = said.fix.as_ref()?;
    let right = &said.diagnostic.suggestion.as_ref()?.right;
    ui.button(format!("Use '{right}'"))
        .clicked()
        .then(|| fix.clone())
}

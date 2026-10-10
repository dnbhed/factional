//! The Content tab's form and what's beside it (U6b, board 1): an entry's values a group at a
//! time, each comment above its value, defaults greyed with Set to write them, choices where
//! there are any; then what the loader says, what refers to the entry, and the entry as
//! written. The form is `factional-content`'s `entry_form`; this draws it.

use factional_content::ValuePath;

use crate::egui::{self, RichText, WidgetInfo, WidgetType};
use crate::{Editor, FieldInput, diagnostics};

/// What was done in the form.
pub(crate) enum Acted {
    /// A value entered, or chosen, for the `n`th field.
    Enter(usize, String),
    /// Set on the `n`th field, a value left out.
    Default(usize),
    Remove(ValuePath),
}

/// How wide a value's field is, so every value lines up and what's after it fits.
const FIELD_WIDTH: f32 = 240.0;

/// The fields in one grid, so their values line up, a group at a time under its name.
pub(crate) fn form_ui(ui: &mut egui::Ui, fields: &mut [FieldInput]) -> Option<Acted> {
    let mut acted = None;
    egui::Grid::new("form").num_columns(2).show(ui, |ui| {
        let mut group = None;
        for (place, field) in fields.iter_mut().enumerate() {
            if field.group.is_some() && field.group != group {
                group.clone_from(&field.group);
                ui.strong(group.as_deref().unwrap_or_default());
                ui.end_row();
            }
            if let Some(done) = row_ui(ui, place, field) {
                acted = Some(done);
            }
        }
    });
    acted
}

/// One field: its comment lines, then its key, and its value with what can be done with it,
/// side by side so a long comment never pushes them apart.
fn row_ui(ui: &mut egui::Ui, place: usize, field: &mut FieldInput) -> Option<Acted> {
    let mut acted = None;
    let FieldInput {
        row,
        input,
        refused,
        ..
    } = field;
    for line in &row.comment {
        ui.label("");
        ui.label(RichText::new(format!("# {line}")).monospace().weak());
        ui.end_row();
    }
    let key = &row.key;
    let described = |label: egui::Response| match &row.description {
        Some(description) => label.on_hover_text(description),
        None => label,
    };
    let Some(written) = &row.written else {
        described(ui.label(RichText::new(key).weak()));
        let default = row.default.as_deref().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{default} — not set, the default"))
                    .italics()
                    .weak(),
            );
            let set = ui.button("Set");
            let said = format!("Set {key}");
            set.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
            if set.clicked() {
                acted = Some(Acted::Default(place));
            }
        });
        ui.end_row();
        return acted;
    };
    let label = described(ui.label(key));
    ui.horizontal(|ui| {
        let edited = ui
            .add(
                egui::TextEdit::singleline(input)
                    .id_salt(row.path.to_string())
                    .desired_width(FIELD_WIDTH),
            )
            .labelled_by(label.id);
        if edited.lost_focus() && *input != written.value {
            acted = Some(Acted::Enter(place, input.clone()));
        }
        if !row.choices.is_empty() {
            let menu = ui.menu_button("⏷", |ui| {
                for choice in &row.choices {
                    if ui.button(choice).clicked() {
                        acted = Some(Acted::Enter(place, choice.clone()));
                    }
                }
            });
            let said = format!("Choose {key}");
            menu.response
                .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
        }
        if ui.button(format!("Remove {key}")).clicked() {
            acted = Some(Acted::Remove(row.path.clone()));
        }
        if let Some(refused) = refused {
            ui.label(refused.as_str());
        }
    });
    ui.end_row();
    acted
}

impl Editor {
    /// Beside the form: what the loader says about the entry, what refers to it, each
    /// reference a link to its entry, and the entry as written. The reference followed, as
    /// its file and key.
    pub(crate) fn inspector_ui(&self, ui: &mut egui::Ui) -> Option<(&'static str, String)> {
        let (_, entry) = self.selected_entry()?;
        let mut going = None;
        ui.strong("What the loader says");
        if entry.problems.is_empty() && entry.warnings.is_empty() {
            ui.label(format!("Nothing at {}", entry.key));
        }
        diagnostics(ui, &entry.problems, &entry.warnings, |d| entry.at(d));
        if let Some(references) = self.referenced_by() {
            ui.separator();
            ui.strong("Referenced by");
            if references.is_empty() {
                ui.label(format!("Nothing names {}", entry.key));
            }
            let mut file = None;
            for reference in references {
                if file != Some(reference.file) {
                    ui.label(RichText::new(reference.file).monospace().weak());
                    file = Some(reference.file);
                }
                if ui.link(&reference.key).clicked() {
                    going = Some((reference.file, reference.key.clone()));
                }
            }
        }
        ui.separator();
        ui.strong("As written");
        ui.monospace(entry.toml.trim_end());
        going
    }
}

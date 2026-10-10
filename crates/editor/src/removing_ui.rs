//! Adding and removing, to board 2 (U6d): each place's keys under one menu with what each
//! is, × to remove, a question before removing an entry something names, and what the last
//! change broke, with Undo beside it.

use factional_content::{Addition, Step, ValuePath, key_info};

use crate::egui::{self, RichText, WidgetInfo, WidgetType};
use crate::{Consequence, PlaceInput};

/// What was chosen in the notice of what the last change broke.
pub(crate) enum Noticed {
    /// The first problem's file and key.
    Show(String, String),
    Undo,
}

/// The notice: the change in words, how many problems it brought and each of them, with
/// "Show it" for the first and "Undo it".
pub(crate) fn consequence_ui(ui: &mut egui::Ui, consequence: &Consequence) -> Option<Noticed> {
    let mut noticed = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let count = match consequence.problems.len() {
            1 => "1 new problem".to_owned(),
            count => format!("{count} new problems"),
        };
        let color = ui.visuals().error_fg_color;
        ui.colored_label(color, format!("{}: {count}", consequence.said));
        for said in &consequence.problems {
            ui.label(RichText::new(said.diagnostic.to_string()).monospace());
        }
        ui.horizontal(|ui| {
            let first = consequence.problems.first().map(|said| &said.diagnostic);
            if let Some(first) = first
                && let Some(key) = &first.key
                && ui.button("Show it").clicked()
            {
                noticed = Some(Noticed::Show(first.file.clone(), key.clone()));
            }
            if ui.button("Undo it").clicked() {
                noticed = Some(Noticed::Undo);
            }
        });
    });
    noticed
}

/// The question before removing `key`, an entry that `count` places name, with what names
/// it listed beside the form: `Some(true)` to remove it anyway, `Some(false)` to keep it.
pub(crate) fn confirm_ui(ui: &mut egui::Ui, key: &str, count: usize) -> Option<bool> {
    let mut chosen = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let places = match count {
            1 => "1 place".to_owned(),
            count => format!("{count} places"),
        };
        ui.label(format!(
            "{key} is named in {places}, listed under Referenced by: removing it breaks each."
        ));
        ui.horizontal(|ui| {
            if ui.button("Remove anyway").clicked() {
                chosen = Some(true);
            }
            if ui.button("Keep it").clicked() {
                chosen = Some(false);
            }
        });
    });
    chosen
}

/// "×", labelled "Remove <name>" for anyone who can't see it.
pub(crate) fn remove_button(ui: &mut egui::Ui, name: &str) -> bool {
    let button = ui.button("×");
    let said = format!("Remove {name}");
    button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
    button.clicked()
}

/// "Add to <name>…": a menu of what the schema allows at the place, each key with what it
/// is, its default and its description, an item for a list, or a new id where it takes one;
/// just its name where nothing more fits. `short` shows it as "+…", as in a table's row,
/// keeping its name for anyone who can't see it. What's chosen goes in `adding`.
pub(crate) fn adding_menu(
    ui: &mut egui::Ui,
    file: &str,
    place: &mut PlaceInput,
    name: &str,
    short: bool,
    adding: &mut Option<(ValuePath, Option<String>)>,
) {
    if place.additions.is_empty() {
        ui.label(RichText::new(name).monospace().weak());
        return;
    }
    let said = format!("Add to {name}…");
    let shown = if short { "+…" } else { said.as_str() };
    let menu = ui.menu_button(shown, |ui| {
        let PlaceInput {
            path,
            additions,
            id,
        } = place;
        for addition in additions.iter() {
            match addition {
                Addition::Key(key) => {
                    if ui.button(format!("Add {key} to {name}")).clicked() {
                        *adding = Some((path.clone(), Some(key.clone())));
                    }
                    if let Some(info) = key_info(file, &path.then(Step::Key(key.clone()))) {
                        let default = info
                            .default
                            .map(|default| format!(", {default} by default"))
                            .unwrap_or_default();
                        ui.label(RichText::new(format!("{}{default}", info.kind)).weak());
                        if let Some(description) = info.description {
                            ui.label(description);
                        }
                    }
                    ui.separator();
                }
                Addition::Item => {
                    if ui.button(format!("Add to {name}")).clicked() {
                        *adding = Some((path.clone(), None));
                    }
                }
                Addition::Id => {
                    let label = ui.label(format!("New id in {name}"));
                    ui.add(egui::TextEdit::singleline(id).desired_width(160.0))
                        .labelled_by(label.id);
                    if ui.button(format!("Add to {name}")).clicked() {
                        *adding = Some((path.clone(), Some(id.clone())));
                    }
                }
            }
        }
    });
    menu.response
        .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
}

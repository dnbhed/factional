//! The Content tab's form and what's beside it (U6b, board 1): an entry's values a group at a
//! time, each comment above its value, defaults greyed with Set to write them, choices where
//! there are any; then what the loader says, what refers to the entry, and the entry as
//! written. The form is `factional-content`'s `entry_form`; this draws it.

use factional_content::{Fix, Step, ValuePath};

use crate::egui::{self, RichText, WidgetInfo, WidgetType};
use crate::layout::{Block, ListTable, blocks, groups};
use crate::problems_ui::{Listed, fix_button};
use crate::removing_ui::{adding_menu, remove_button};
use crate::{Editor, FieldInput, PlaceInput, Said, theme};

/// What was done in the form.
pub(crate) enum Acted {
    /// A value entered, or chosen, for the `n`th field.
    Enter(usize, String),
    /// Set on the `n`th field, a value left out.
    Default(usize),
    Remove(ValuePath),
    /// A problem's fix chosen.
    Fix(Fix),
    /// A key, or with none an item, added at a place.
    Add(ValuePath, Option<String>),
}

/// How wide a value's field is, so every value lines up and what's after it fits; a table's
/// cells are narrower.
const FIELD_WIDTH: f32 = 240.0;
const CELL_WIDTH: f32 = 90.0;
/// A table's columns: an item's number, and a key with room for its choices beside it.
const NUMBER_WIDTH: f32 = 24.0;
const COLUMN_WIDTH: f32 = 130.0;

/// The form in one grid, so its values line up, a group at a time under its name: each value
/// on its row, and each list of tables as a table (U6f) whose rows each have × and what can be
/// added to the item. The places drawn in a table go in `drawn`, so they aren't again.
pub(crate) fn form_ui(
    ui: &mut egui::Ui,
    file: &str,
    entry: &ValuePath,
    fields: &mut [FieldInput],
    places: &mut [PlaceInput],
    drawn: &mut Vec<ValuePath>,
) -> Option<Acted> {
    let mut acted = None;
    let mut done = |action: Option<Acted>| {
        if action.is_some() {
            acted = action;
        }
    };
    let names: Vec<Option<&str>> = fields.iter().map(|field| field.group.as_deref()).collect();
    let ranges = groups(&names);
    egui::Grid::new("form").num_columns(2).show(ui, |ui| {
        for range in ranges {
            if let Some(name) = &fields[range.start].group {
                ui.strong(name);
                ui.end_row();
            }
            let keys: Vec<(usize, ValuePath)> = range
                .filter_map(|place| Some((place, ValuePath::parse(&fields[place].row.key)?)))
                .collect();
            for block in blocks(&keys) {
                match block {
                    Block::Row(place) => done(row_ui(ui, place, &mut fields[place])),
                    Block::Table(table) => {
                        ui.label(RichText::new(&table.list).monospace());
                        done(table_ui(ui, file, entry, &table, fields, places, drawn));
                        ui.end_row();
                    }
                }
            }
        }
    });
    acted
}

/// A list of tables: a header of its keys, then a row an item, numbered from 1, each cell its
/// field, and × and "Add to" for the item. Cells have a fixed width, so the table settles at
/// once inside the form's grid.
fn table_ui(
    ui: &mut egui::Ui,
    file: &str,
    entry: &ValuePath,
    table: &ListTable,
    fields: &mut [FieldInput],
    places: &mut [PlaceInput],
    drawn: &mut Vec<ValuePath>,
) -> Option<Acted> {
    let mut acted = None;
    let list = entry.then(Step::Key(table.list.clone()));
    let cell = |ui: &mut egui::Ui, width: f32, draw: &mut dyn FnMut(&mut egui::Ui)| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                draw(ui);
            },
        );
    };
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            cell(ui, NUMBER_WIDTH, &mut |ui| {
                ui.weak("#");
            });
            for column in &table.columns {
                cell(ui, COLUMN_WIDTH, &mut |ui| {
                    let said = format!("{} · {column}", table.list);
                    ui.weak(column)
                        .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &said));
                });
            }
        });
        for (item, cells) in table.rows.iter().enumerate() {
            ui.separator();
            ui.horizontal(|ui| {
                cell(ui, NUMBER_WIDTH, &mut |ui| {
                    ui.weak((item + 1).to_string());
                });
                for place in cells {
                    cell(ui, COLUMN_WIDTH, &mut |ui| match place {
                        Some(place) => {
                            if let Some(done) = cell_ui(ui, *place, &mut fields[*place]) {
                                acted = Some(done);
                            }
                        }
                        None => {
                            ui.weak("—");
                        }
                    });
                }
                let path = list.then(Step::Index(item));
                let name = format!("{}[{item}]", table.list);
                if remove_button(ui, &name) {
                    acted = Some(Acted::Remove(path.clone()));
                }
                if let Some(place) = places.iter_mut().find(|place| place.path == path) {
                    let mut added = None;
                    adding_menu(ui, file, place, &name, true, &mut added);
                    if let Some((at, key)) = added {
                        acted = Some(Acted::Add(at, key));
                    }
                }
                drawn.push(path);
            });
            // A cell's comment goes under its row, where it has room.
            for (column, place) in table.columns.iter().zip(cells) {
                let Some(place) = place else {
                    continue;
                };
                let said = format!("{}[{item}] · {column}", table.list);
                for line in &fields[*place].row.comment {
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(column)
                            .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &said));
                        ui.label(
                            RichText::new(format!("# {line}"))
                                .monospace()
                                .color(theme::COMMENT),
                        );
                    });
                }
            }
        }
    });
    acted
}

/// A table's cell: its value's field, labelled with its key for anyone who can't see the
/// table, and its choices and problems; or its default with Set.
fn cell_ui(ui: &mut egui::Ui, place: usize, field: &mut FieldInput) -> Option<Acted> {
    let mut acted = None;
    ui.vertical(|ui| {
        acted = value_ui(ui, place, field, None, CELL_WIDTH);
        for said in field.said.iter() {
            if let Some(fix) = said_ui(ui, said) {
                acted = Some(Acted::Fix(fix));
            }
        }
    });
    acted
}

/// One field: its comment lines, then its key, and its value with what can be done with it,
/// side by side so a long comment never pushes them apart.
fn row_ui(ui: &mut egui::Ui, place: usize, field: &mut FieldInput) -> Option<Acted> {
    let mut acted = None;
    for line in &field.row.comment {
        ui.label("");
        ui.label(
            RichText::new(format!("# {line}"))
                .monospace()
                .color(theme::COMMENT),
        );
        ui.end_row();
    }
    let row = &field.row;
    let label = ui
        .horizontal(|ui| {
            // Keys, ids and values are always monospace (board 6).
            let key = RichText::new(&row.key).monospace();
            let label = match &row.written {
                Some(_) => ui.label(key),
                None => ui.label(key.weak()),
            };
            if field.edited {
                edited_mark(ui, &row.key);
            }
            match &row.description {
                Some(description) => label.on_hover_text(description),
                None => label,
            }
        })
        .inner;
    ui.horizontal(|ui| {
        acted = value_ui(ui, place, field, Some(label.id), FIELD_WIDTH);
        let FieldInput { row, refused, .. } = field;
        if row.written.is_some() && remove_button(ui, &row.key) {
            acted = Some(Acted::Remove(row.path.clone()));
        }
        if let Some(refused) = refused {
            ui.label(refused.as_str());
        }
    });
    ui.end_row();
    for said in field.said.iter() {
        ui.label("");
        ui.horizontal_wrapped(|ui| {
            if let Some(fix) = said_ui(ui, said) {
                acted = Some(Acted::Fix(fix));
            }
        });
        ui.end_row();
    }
    acted
}

/// A field's value: its text to edit, labelled by `label` or else by its key, with its
/// choices; or, left out, its default greyed with Set.
fn value_ui(
    ui: &mut egui::Ui,
    place: usize,
    field: &mut FieldInput,
    label: Option<egui::Id>,
    width: f32,
) -> Option<Acted> {
    let mut acted = None;
    let stroke = border_of(ui.visuals(), field);
    let FieldInput { row, input, .. } = field;
    let key = &row.key;
    let Some(written) = &row.written else {
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
        return acted;
    };
    ui.horizontal(|ui| {
        let edited = ui
            .scope(|ui| {
                if let Some(stroke) = stroke {
                    let widgets = &mut ui.visuals_mut().widgets;
                    widgets.inactive.bg_stroke = stroke;
                    widgets.hovered.bg_stroke = stroke;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut *input)
                        .id_salt(row.path.to_string())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(width),
                )
            })
            .inner;
        let edited = match label {
            Some(label) => edited.labelled_by(label),
            None => {
                let mut info = WidgetInfo::text_edit(true, &written.value, input.as_str(), "");
                info.label = Some(key.clone());
                edited.widget_info(|| info.clone());
                edited
            }
        };
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
    });
    acted
}

/// The amber mark of a value changed since the last save, "<key>, edited" for anyone who
/// can't see it.
fn edited_mark(ui: &mut egui::Ui, key: &str) {
    let said = format!("{key}, edited");
    let (rect, mark) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 3.0, ui.visuals().warn_fg_color);
    mark.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &said));
}

/// A field's border (board 6): red if the writer refused what was entered or the loader
/// has a problem at it, amber if it's changed since the last save, else the usual.
fn border_of(visuals: &egui::Visuals, field: &FieldInput) -> Option<egui::Stroke> {
    let problem = field.refused.is_some() || field.said.iter().any(|said| !said.warning);
    let colour = match (problem, field.edited) {
        (true, _) => visuals.error_fg_color,
        (false, true) => visuals.warn_fg_color,
        (false, false) => return None,
    };
    Some(egui::Stroke::new(1.0, colour))
}

/// A problem or warning at a field, in the loader's words, with its fix.
fn said_ui(ui: &mut egui::Ui, said: &Said) -> Option<Fix> {
    let color = match said.warning {
        true => ui.visuals().warn_fg_color,
        false => ui.visuals().error_fg_color,
    };
    ui.colored_label(color, &said.diagnostic.message);
    fix_button(ui, said)
}

impl Editor {
    /// Beside the form: what the loader says about the entry, what refers to it, each
    /// reference a link to its entry, and the entry as written. The reference followed, as
    /// its file and key.
    pub(crate) fn inspector_ui(&self, ui: &mut egui::Ui) -> Option<Listed> {
        let (_, entry) = self.selected_entry()?;
        let mut going = None;
        ui.strong("What the loader says");
        if entry.problems.is_empty() && entry.warnings.is_empty() {
            ui.label(format!("Nothing at {}", entry.key));
        }
        // What's at a field is shown under it; the rest is the entry's.
        let at_a_field = |said: &Said| self.fields.iter().any(|field| field.said.contains(said));
        let about_it: Vec<&Said> = self
            .said()
            .iter()
            .filter(|said| {
                let about = &said.diagnostic;
                entry.problems.contains(about) || entry.warnings.contains(about)
            })
            .collect();
        if about_it.iter().any(|said| at_a_field(said)) {
            ui.label("Some at their fields");
        }
        for said in about_it.into_iter().filter(|said| !at_a_field(said)) {
            let kind = if said.warning { "warning" } else { "error" };
            let message = &said.diagnostic.message;
            ui.horizontal_wrapped(|ui| {
                match entry.at(&said.diagnostic) {
                    Some(key) => ui.label(format!("{kind}: {key}: {message}")),
                    None => ui.label(format!("{kind}: {message}")),
                };
                if let Some(fix) = fix_button(ui, said) {
                    going = Some(Listed::Fix(fix));
                }
            });
        }
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
                    going = Some(Listed::Go(reference.file.to_owned(), reference.key.clone()));
                }
            }
        }
        ui.separator();
        ui.strong("As written");
        ui.monospace(entry.toml.trim_end());
        going
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_fields_border_is_red_for_a_problem_amber_for_an_edit_else_the_usual() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/tests/fixtures/worlds/typos");
        let mut editor = Editor::open(dir);
        editor.select_at("characters.toml", "vex");
        let visuals = crate::theme::visuals();
        let red = Some(egui::Stroke::new(1.0, visuals.error_fg_color));
        let amber = Some(egui::Stroke::new(1.0, visuals.warn_fg_color));
        let find = |key: &str| {
            editor
                .fields()
                .iter()
                .find(|field| field.row.key == key)
                .cloned()
                .expect("the field")
        };
        let mut faction = find("memberships[0].faction");
        assert_eq!(border_of(&visuals, &faction), red);
        faction.edited = true;
        assert_eq!(border_of(&visuals, &faction), red);
        let mut name = find("name");
        assert_eq!(border_of(&visuals, &name), None);
        name.edited = true;
        assert_eq!(border_of(&visuals, &name), amber);
        name.refused = Some("expected a number".to_owned());
        assert_eq!(border_of(&visuals, &name), red);
        // A warning alone doesn't make the border red.
        let mut warned = find("name");
        warned.said = faction.said.clone();
        warned.said[0].warning = true;
        assert_eq!(border_of(&visuals, &warned), None);
    }
}

//! The Content tab's form and what's beside it (U6b, board 1): an entry's values a group at a
//! time, each comment above its value, defaults greyed with Set to write them, choices where
//! there are any; then what the loader says, what refers to the entry, and the entry as
//! written. The form is `factional-content`'s `entry_form`; this draws it.

use factional_content::{Addition, Fix, Offered, Step, ValuePath, Word, key_info};

use crate::egui::{self, RichText, WidgetInfo, WidgetType};
use crate::layout::{Line, ListTable, Shape, cell_removes, column_titles, form_layout};
use crate::problems_ui::{Listed, fix_button, within};
use crate::removing_ui::{adding_menu, remove_button};
use crate::{Editor, FieldInput, OfferInput, PlaceInput, Said, SectionInput, theme};

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
    /// The `m`th offer of the `n`th section made, with an id for one that takes it (U10d).
    Offer(usize, usize, Option<String>),
}

/// How wide a value's field is, so every value lines up and what's after it fits; a table's
/// cells are narrower.
const FIELD_WIDTH: f32 = 240.0;
const CELL_WIDTH: f32 = 90.0;
/// A table's columns: an item's number, and a key with room for its choices beside it.
const NUMBER_WIDTH: f32 = 24.0;
const COLUMN_WIDTH: f32 = 130.0;
/// How far in each heading a value is under goes (U10a).
const INDENT: f32 = 14.0;

/// The form in one grid, so its values line up, a section at a time under its name, every
/// section there even with nothing in it (U10d): each value on its row under headings for where
/// it is, said in words (U10a), and each list of tables as a table (U6f) whose rows each have ×
/// and what can be added to the item. Each place's × is where it is (U10b), and so is its "+"
/// unless a section's offer says the same; each section ends with what it offers to add.
pub(crate) fn form_ui(
    ui: &mut egui::Ui,
    file: &str,
    entry: &ValuePath,
    fields: &mut [FieldInput],
    places: &mut [PlaceInput],
    sections: &mut [SectionInput],
) -> Option<Acted> {
    let mut acted = None;
    let mut done = |action: Option<Acted>| {
        if action.is_some() {
            acted = action;
        }
    };
    let words: Vec<Vec<Word>> = fields.iter().map(|field| field.row.words.clone()).collect();
    let layout = {
        let paths: Vec<ValuePath> = fields
            .iter()
            .map(|field| in_entry(entry, &field.row.path))
            .collect();
        let shapes: Vec<Shape<'_>> = fields
            .iter()
            .zip(&paths)
            .map(|(field, path)| Shape {
                group: field.group.as_deref(),
                path,
                words: &field.row.words,
                empty: field.row.written.is_none() && field.row.default.is_none(),
                pair: field.pair,
            })
            .collect();
        let at: Vec<ValuePath> = places
            .iter()
            .map(|place| in_entry(entry, &place.path))
            .collect();
        let names: Vec<Option<&str>> = sections
            .iter()
            .map(|section| section.name.as_deref())
            .collect();
        form_layout(&names, &shapes, &at)
    };
    // A place whose "+" a section's offer says, as the place the offer adds at or what that's
    // within, has none of its own.
    let covered = sections
        .iter()
        .flat_map(|section| &section.offers)
        .map(|offer| offer.offer.at.clone())
        .collect();
    let mut drawing = Drawing {
        file,
        entry,
        places,
        covered,
    };
    egui::Grid::new("form").num_columns(2).show(ui, |ui| {
        for (place, group) in layout.into_iter().enumerate() {
            if let Some(name) = &group.name {
                ui.horizontal(|ui| {
                    ui.strong(name);
                    done(drawing.controls(ui, &group.places, true));
                });
                ui.end_row();
            }
            for line in group.lines {
                match line {
                    Line::Heading {
                        depth,
                        text,
                        empty,
                        places: here,
                        ..
                    } => {
                        ui.horizontal(|ui| {
                            ui.add_space(indent(depth));
                            ui.label(text);
                            if empty {
                                ui.weak("empty");
                            }
                            done(drawing.controls(ui, &here, true));
                        });
                        ui.end_row();
                    }
                    Line::Row { field, depth } => {
                        done(row_ui(ui, field, &mut fields[field], depth));
                    }
                    Line::Pair {
                        depth,
                        text,
                        fields: [first, second],
                        places: here,
                        ..
                    } => {
                        ui.horizontal(|ui| {
                            ui.add_space(indent(depth));
                            ui.label(text);
                        });
                        ui.horizontal(|ui| {
                            done(value_ui(ui, first, &mut fields[first], CELL_WIDTH));
                            ui.label("↔");
                            done(value_ui(ui, second, &mut fields[second], CELL_WIDTH));
                            done(drawing.controls(ui, &here, true));
                        });
                        ui.end_row();
                        for field in [first, second] {
                            done(said_rows(ui, &fields[field]));
                        }
                    }
                    Line::Table {
                        table,
                        places: here,
                    } => {
                        // Its title, unless the group's name already says it.
                        let list = entry.then(Step::Key(table.list.clone()));
                        let title = key_info(file, &list).and_then(|info| info.title);
                        let title = title.unwrap_or_else(|| table.list.clone());
                        ui.label(if group.name.as_deref() == Some(title.as_str()) {
                            ""
                        } else {
                            &title
                        });
                        let columns = column_titles(&table, &words);
                        done(table_ui(ui, &mut drawing, &table, &columns, &here, fields));
                        ui.end_row();
                    }
                }
            }
            if let Some(section) = sections.get_mut(place)
                && !section.offers.is_empty()
            {
                ui.horizontal_wrapped(|ui| {
                    for (offer, input) in section.offers.iter_mut().enumerate() {
                        if let Some(id) = offer_ui(ui, file, entry, input) {
                            done(Some(Acted::Offer(place, offer, id)));
                        }
                    }
                });
                ui.end_row();
            }
        }
    });
    acted
}

/// One offer's "+": a key's or an item's, added at once, "+ Weights" or "+ Membership"; an id's,
/// a menu of those that exist with their names, and for a table keyed by ids a new one typed.
/// Each is labelled as the place's own "+" would be, for anyone who can't see it: "Add weights
/// to vex", "Add to memberships", "Add to standing.factions…". What's chosen, with its id.
fn offer_ui(
    ui: &mut egui::Ui,
    file: &str,
    entry: &ValuePath,
    input: &mut OfferInput,
) -> Option<Option<String>> {
    let OfferInput { offer, id } = input;
    let at = in_entry(entry, &offer.at);
    let name = match at.0.is_empty() {
        true => entry.to_string(),
        false => at.to_string(),
    };
    let shown = format!("+ {}", offer.label);
    let button = |ui: &mut egui::Ui, shown: &str, said: &str| {
        let button = ui.button(shown);
        button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, said));
        button
    };
    let mut chosen = None;
    match &offer.what {
        Offered::Key(key) => {
            let mut added = button(ui, &shown, &format!("Add {key} to {name}"));
            if let Some(info) = key_info(file, &offer.at.then(Step::Key(key.clone()))) {
                let default = info
                    .default
                    .map(|default| format!(", {default} by default"))
                    .unwrap_or_default();
                let about = info.description.unwrap_or_default();
                added = added.on_hover_text(format!("{}{default}. {about}", info.kind));
            }
            if added.clicked() {
                chosen = Some(None);
            }
        }
        Offered::Item => {
            if button(ui, &shown, &format!("Add to {name}")).clicked() {
                chosen = Some(None);
            }
        }
        Offered::Id(choices) | Offered::Pick(choices) => {
            let said = format!("Add to {name}…");
            let menu = ui.menu_button(format!("{shown}…"), |ui| {
                for choice in choices {
                    let named = match &choice.name {
                        Some(named) => format!("{named} · {}", choice.id),
                        None => choice.id.clone(),
                    };
                    let said = format!("Add {} to {name}", choice.id);
                    if button(ui, &named, &said).clicked() {
                        chosen = Some(Some(choice.id.clone()));
                    }
                }
                // A table keyed by ids takes one not made yet, too.
                if matches!(offer.what, Offered::Id(_)) {
                    let label = ui.label(format!("New id in {name}"));
                    ui.add(egui::TextEdit::singleline(id).desired_width(160.0))
                        .labelled_by(label.id);
                    if ui.button(format!("Add to {name}")).clicked() {
                        chosen = Some(Some(id.clone()));
                    }
                }
            });
            menu.response
                .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
        }
    }
    chosen
}

/// `path`, a place in the entry at `entry`, from the entry's start.
fn in_entry(entry: &ValuePath, path: &ValuePath) -> ValuePath {
    ValuePath(path.0.iter().skip(entry.0.len()).cloned().collect())
}

/// What drawing a part of the form needs besides its own: the file, the entry, its places and
/// where the sections' offers add.
struct Drawing<'d> {
    file: &'d str,
    entry: &'d ValuePath,
    places: &'d mut [PlaceInput],
    covered: Vec<ValuePath>,
}

impl Drawing<'_> {
    /// The × (if `removable`) and "+" of each of `here`, by their place among the places,
    /// side by side; no "+" where a section's offer says it.
    fn controls(&mut self, ui: &mut egui::Ui, here: &[usize], removable: bool) -> Option<Acted> {
        let mut acted = None;
        for &place in here {
            let Some(place) = self.places.get_mut(place) else {
                continue;
            };
            let name = in_entry(self.entry, &place.path).to_string();
            if removable && remove_button(ui, &name) {
                acted = Some(Acted::Remove(place.path.clone()));
            }
            let covered = self
                .covered
                .iter()
                .any(|at| at.0.starts_with(&place.path.0));
            if !covered && let Some((at, key)) = adding(ui, self.file, place, &name) {
                acted = Some(Acted::Add(at, key));
            }
        }
        acted
    }
}

/// A place's "+": nothing where nothing more fits; for a list, one button adding an item,
/// "+ <item>"; else its menu of what can be added, "+…". Its words are on hover; its name,
/// for anyone who can't see it.
fn adding(
    ui: &mut egui::Ui,
    file: &str,
    place: &mut PlaceInput,
    name: &str,
) -> Option<(ValuePath, Option<String>)> {
    if place.additions.is_empty() {
        return None;
    }
    if place.additions == [Addition::Item] {
        let shown = match &place.item {
            Some(item) => format!("+ {item}"),
            None => "+".to_owned(),
        };
        let said = format!("Add to {name}");
        let button = ui
            .button(shown)
            .on_hover_text(format!("Add to {}", place.words));
        button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &said));
        return button.clicked().then(|| (place.path.clone(), None));
    }
    let mut added = None;
    adding_menu(ui, file, place, name, true, &mut added);
    added
}

/// How far in a value or heading under `depth` headings goes.
fn indent(depth: usize) -> f32 {
    INDENT * depth as f32
}

/// A list of tables: a header of its columns in words, then a row an item, numbered from 1,
/// each cell its field, and × and "Add to" for the item; then the list's own "+" (`here`), at
/// its end (U10b), as the list goes with its last item. Cells have a fixed width, so the table
/// settles at once inside the form's grid.
fn table_ui(
    ui: &mut egui::Ui,
    drawing: &mut Drawing<'_>,
    table: &ListTable,
    titles: &[String],
    here: &[usize],
    fields: &mut [FieldInput],
) -> Option<Acted> {
    let mut acted = None;
    let list = drawing.entry.then(Step::Key(table.list.clone()));
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
            for title in titles {
                cell(ui, COLUMN_WIDTH, &mut |ui| {
                    ui.weak(title);
                });
            }
        });
        for (item, cells) in table.rows.iter().enumerate() {
            ui.separator();
            ui.horizontal(|ui| {
                cell(ui, NUMBER_WIDTH, &mut |ui| {
                    ui.weak((item + 1).to_string());
                });
                for field in cells {
                    cell(ui, COLUMN_WIDTH, &mut |ui| match field {
                        Some(field) => {
                            let done = cell_ui(ui, drawing.entry, *field, &mut fields[*field]);
                            if done.is_some() {
                                acted = done;
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
                let place = drawing.places.iter_mut().find(|place| place.path == path);
                if let Some(place) = place {
                    let mut added = None;
                    adding_menu(ui, drawing.file, place, &name, true, &mut added);
                    if let Some((at, key)) = added {
                        acted = Some(Acted::Add(at, key));
                    }
                }
            });
            // A cell's comment goes under its row, where it has room.
            for ((column, title), field) in table.columns.iter().zip(titles).zip(cells) {
                let Some(field) = field else {
                    continue;
                };
                let said = format!("{}[{item}] · {column}", table.list);
                for line in &fields[*field].row.comment {
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(title)
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
        ui.horizontal(|ui| {
            let done = drawing.controls(ui, here, false);
            if done.is_some() {
                acted = done;
            }
        });
    });
    acted
}

/// A table's cell: its value's field, labelled with its key for anyone who can't see the
/// table, with × to remove it (U10b), and its choices and problems; or its default with Set.
fn cell_ui(
    ui: &mut egui::Ui,
    entry: &ValuePath,
    place: usize,
    field: &mut FieldInput,
) -> Option<Acted> {
    let mut acted = None;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            acted = value_ui(ui, place, field, CELL_WIDTH);
            if field.row.written.is_some() {
                let removes = cell_removes(&in_entry(entry, &field.row.path));
                if remove_button(ui, &removes.to_string()) {
                    let mut path = entry.clone();
                    path.0.extend(removes.0);
                    acted = Some(Acted::Remove(path));
                }
            }
        });
        for said in field.said.iter() {
            if let Some(fix) = said_ui(ui, said) {
                acted = Some(Acted::Fix(fix));
            }
        }
    });
    acted
}

/// One field under `depth` headings: its comment lines, then its last word, and its value with
/// what can be done with it, side by side so a long comment never pushes them apart. Its key
/// and description are on hover.
fn row_ui(ui: &mut egui::Ui, place: usize, field: &mut FieldInput, depth: usize) -> Option<Acted> {
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
    ui.horizontal(|ui| {
        ui.add_space(indent(depth));
        let word = row.words.last().map_or(row.key.as_str(), |word| &word.text);
        let label = match &row.written {
            Some(_) => ui.label(word),
            None => ui.label(RichText::new(word).weak()),
        };
        if field.edited {
            edited_mark(ui, &row.key);
        }
        // Keys are always monospace (board 6).
        label.on_hover_ui(|ui| {
            ui.label(RichText::new(&row.key).monospace());
            if let Some(description) = &row.description {
                ui.label(description);
            }
        });
    });
    ui.horizontal(|ui| {
        acted = value_ui(ui, place, field, FIELD_WIDTH);
        let FieldInput { row, refused, .. } = field;
        if row.written.is_some() && remove_button(ui, &row.key) {
            acted = Some(Acted::Remove(row.path.clone()));
        }
        if let Some(refused) = refused {
            ui.label(refused.as_str());
        }
    });
    ui.end_row();
    if let Some(fixed) = said_rows(ui, field) {
        acted = Some(fixed);
    }
    acted
}

/// What the loader says at a field, a row each, under its value, with its fix.
fn said_rows(ui: &mut egui::Ui, field: &FieldInput) -> Option<Acted> {
    let mut acted = None;
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

/// A field's value: its text to edit, labelled by its key for anyone who can't see where it
/// is, with its choices; or, left out, its default greyed with Set.
fn value_ui(ui: &mut egui::Ui, place: usize, field: &mut FieldInput, width: f32) -> Option<Acted> {
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
        let mut info = WidgetInfo::text_edit(true, &written.value, input.as_str(), "");
        info.label = Some(key.clone());
        edited.widget_info(|| info.clone());
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
            // Where in the entry, in words, after the entry's own.
            let steps = ValuePath::parse(&entry.key).map_or(0, |path| path.0.len());
            ui.horizontal_wrapped(|ui| {
                match entry.at(&said.diagnostic).and(within(said, steps)) {
                    Some(place) => ui.label(format!("{kind}: {place}: {message}")),
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
            for (reference, words) in references.iter().zip(self.referenced_words()) {
                if file != Some(reference.file) {
                    ui.label(RichText::new(reference.file).monospace().weak());
                    file = Some(reference.file);
                }
                let link = ui
                    .link(words)
                    .on_hover_text(RichText::new(&reference.key).monospace());
                if link.clicked() {
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
    fn each_heading_a_value_is_under_takes_it_further_in() {
        assert_eq!(indent(0), 0.0);
        assert_eq!(indent(1), INDENT);
        assert_eq!(indent(3), 3.0 * INDENT);
    }

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

//! The Previews tab (U5): the disposition matrix and a faction's alignment map, from the
//! world built from the last content that loaded (D-35). Both are the engine's queries
//! (`disposition_matrix`, `alignment_map`); this draws them and turns clicks into model
//! calls.

use std::cmp::Ordering;

use factional_content::{describe_mark, map_heading};
use factional_reputation::{CharacterId, World};

use factional_core::{Curve, Fixed};

use crate::Editor;
use crate::egui::{
    self, Align2, Color32, FontId, Sense, Shape, Stroke, WidgetInfo, WidgetType, vec2,
};
use crate::layout::{curve_plot, map_cell, map_size};

/// Scores in bands below the middle one are tinted orange, above it blue: the design's
/// hostile and friendly colours.
const HOSTILE: (u8, u8, u8) = (0xf0, 0xa3, 0x5e);
const FRIENDLY: (u8, u8, u8) = (0x5a, 0xa9, 0xe6);

/// What was clicked in the Previews tab.
enum Choice {
    Map(String),
    Curve(String),
    Starter(String),
    Quest(String),
}

impl Editor {
    /// The Previews tab: whether the previews are current, then the matrix, the map, a curve,
    /// and whether a character can start a quest.
    pub(crate) fn previews_ui(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let problems = problems(self.outline.problem_count());
                let Some(world) = &self.preview else {
                    ui.label(format!(
                        "Previews need a world that loads: it has {problems} now."
                    ));
                    return;
                };
                if self.preview_is_stale() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("From the last version that loaded: it has {problems} now."),
                    );
                }
                ui.heading("Disposition: each observer, down, toward each character, across");
                // A world with many characters scrolls its matrix sideways, on its own.
                egui::ScrollArea::horizontal()
                    .id_salt("matrix")
                    .show(ui, |ui| matrix_ui(ui, world));
                ui.separator();
                ui.heading("Alignment map");
                chosen = map_ui(ui, world, self.map_faction()).map(Choice::Map);
                ui.separator();
                ui.heading("Curve");
                let curve = self.curve();
                chosen = curve_ui(ui, world, curve)
                    .map(Choice::Curve)
                    .or(chosen.take());
                ui.separator();
                ui.heading("Can they start it?");
                chosen = self.start_ui(ui, world).or(chosen.take());
            });
        });
        match chosen {
            Some(Choice::Map(faction)) => self.show_map(&faction),
            Some(Choice::Curve(knob)) => self.show_curve(&knob),
            Some(Choice::Starter(character)) => self.ask_start(Some(&character), None),
            Some(Choice::Quest(quest)) => self.ask_start(None, Some(&quest)),
            None => {}
        }
    }

    /// A choice of character and of quest, then whether they can start it at the start of
    /// play, in the engine's words.
    fn start_ui(&self, ui: &mut egui::Ui, world: &World) -> Option<Choice> {
        let mut chosen = None;
        let (starter, quest) = self.asked();
        ui.horizontal_wrapped(|ui| {
            for character in world.characters() {
                let id = character.id.as_str();
                let label = format!("Can start: {id}");
                if ui.selectable_label(starter == Some(id), label).clicked() {
                    chosen = Some(Choice::Starter(id.to_owned()));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for id in self.preview_quests.quests.keys() {
                let id = id.as_str();
                let label = format!("Quest: {id}");
                if ui.selectable_label(quest == Some(id), label).clicked() {
                    chosen = Some(Choice::Quest(id.to_owned()));
                }
            }
        });
        if let Some(assessment) = self.start_assessment() {
            for line in assessment.to_string().lines() {
                ui.label(line);
            }
        }
        chosen
    }
}

/// `1 problem` or `3 problems`.
fn problems(count: usize) -> String {
    match count {
        1 => "1 problem".to_owned(),
        count => format!("{count} problems"),
    }
}

/// Every observer's score toward every character, each tinted by its band and labelled in
/// full for anyone who can't see the grid.
fn matrix_ui(ui: &mut egui::Ui, world: &World) {
    let subjects: Vec<CharacterId> = world.characters().map(|c| c.id.clone()).collect();
    let Some(matrix) = world.disposition_matrix(&subjects) else {
        return;
    };
    let bands: Vec<&str> = world
        .balance()
        .bands
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    egui::Grid::new("matrix").striped(true).show(ui, |ui| {
        ui.label("observer");
        for subject in &matrix.subjects {
            ui.label(subject.as_str());
        }
        ui.end_row();
        for row in &matrix.rows {
            ui.label(row.observer.to_string());
            for (subject, cell) in matrix.subjects.iter().zip(&row.cells) {
                let Some(disposition) = cell else {
                    ui.label("—");
                    continue;
                };
                let band = bands.iter().position(|b| *b == disposition.band);
                let mut text = egui::RichText::new(disposition.score.to_string()).monospace();
                if let Some(tint) = band.and_then(|band| tint(band, bands.len())) {
                    text = text
                        .background_color(tint)
                        .color(ui.visuals().strong_text_color());
                }
                let said = format!(
                    "{} → {subject}: {}, {}",
                    row.observer, disposition.score, disposition.band
                );
                ui.label(text)
                    .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &said));
            }
            ui.end_row();
        }
    });
}

/// A band's tint: the `index`th of `count` bands, lowest first. The middle band, if there's
/// one, has none; the further from the middle, the stronger.
fn tint(index: usize, count: usize) -> Option<Color32> {
    let span = count.checked_sub(1)?;
    let from_middle = 2 * index as i64 - span as i64;
    // A lone band, or the middle one, is at 0: no tint, and nothing to divide.
    let (r, g, b) = match from_middle.cmp(&0) {
        Ordering::Equal => return None,
        Ordering::Less => HOSTILE,
        Ordering::Greater => FRIENDLY,
    };
    let strength = from_middle.unsigned_abs() as f32 / span as f32;
    let alpha = (40.0 + 120.0 * strength).round() as u8;
    Some(Color32::from_rgba_unmultiplied(r, g, b, alpha))
}

/// A choice of knob, then its curve plotted with each point in words, or that it's left
/// out. The knob clicked, if one was.
fn curve_ui(
    ui: &mut egui::Ui,
    world: &World,
    shown: Option<(String, Option<Curve>)>,
) -> Option<String> {
    let mut clicked = None;
    let shown_name = shown.as_ref().map(|(name, _)| name.as_str());
    ui.horizontal_wrapped(|ui| {
        for (name, _) in world.named_curves() {
            if ui
                .selectable_label(shown_name == Some(&name), &name)
                .clicked()
            {
                clicked = Some(name.clone());
            }
        }
    });
    let Some((_, curve)) = shown else {
        return clicked;
    };
    let Some(curve) = curve else {
        ui.label(format!("left out, so {} everywhere", Fixed::ONE));
        return clicked;
    };
    let points = curve.points();
    let (area, _) = ui.allocate_exact_size(vec2(360.0, 160.0), Sense::hover());
    let plotted: Vec<(f32, f32)> = points
        .iter()
        .map(|(x, y)| (points_of(*x), points_of(*y)))
        .collect();
    let line = curve_plot(&plotted, area.shrink(8.0));
    let visuals = ui.visuals().clone();
    ui.painter()
        .rect_filled(area, 4.0, visuals.extreme_bg_color);
    ui.painter()
        .add(Shape::line(line, Stroke::new(2.0, visuals.hyperlink_color)));
    for (x, y) in &points {
        ui.label(format!("at {x}: {y}"));
    }
    clicked
}

/// A number as a point in the plot: hundredths, as egui's layout reads them.
fn points_of(value: Fixed) -> f32 {
    value.hundredths() as f32 / 100.0
}

/// A choice of faction, then its map: the cells within its tolerance filled, the faction
/// and each character marked, and the key in words. The faction clicked, if one was.
fn map_ui(ui: &mut egui::Ui, world: &World, faction: Option<&str>) -> Option<String> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for choice in world.factions() {
            let on = faction == Some(choice.id.as_str());
            if ui.selectable_label(on, &choice.name).clicked() {
                clicked = Some(choice.id.to_string());
            }
        }
    });
    let chosen = world.factions().find(|f| Some(f.id.as_str()) == faction)?;
    let map = world.alignment_map(&chosen.id)?;
    ui.label(map_heading(&chosen.name, &map));
    let (area, _) = ui.allocate_exact_size(map_size(), Sense::hover());
    let visuals = ui.visuals().clone();
    for (row, cells) in map.within.iter().enumerate() {
        for (column, within) in cells.iter().enumerate() {
            let rect = map_cell(area.min, factional_reputation::MapCell { row, column });
            let fill = if *within {
                visuals.selection.bg_fill
            } else {
                visuals.widgets.inactive.weak_bg_fill
            };
            ui.painter().rect_filled(rect, 2.0, fill);
        }
    }
    for (cell, marks) in map.marks() {
        let text = match marks.as_slice() {
            [mark] => mark.to_string(),
            _ => "*".to_owned(),
        };
        ui.painter().text(
            map_cell(area.min, cell).center(),
            Align2::CENTER_CENTER,
            text,
            FontId::monospace(11.0),
            visuals.strong_text_color(),
        );
    }
    ui.label("@ the faction; filled cells are within its tolerance; law runs across, good up");
    for mark in &map.characters {
        ui.label(describe_mark(mark));
    }
    for (cell, shared) in map.marks() {
        if shared.len() > 1 {
            let shared: Vec<String> = shared.iter().map(char::to_string).collect();
            ui.label(format!(
                "* at law {}, good {}: {}",
                cell.law(),
                cell.good(),
                shared.join(", ")
            ));
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_below_the_middle_are_hostile_above_it_friendly_and_stronger_further_out() {
        // Three bands, as the sample's: unfriendly, neutral, friendly.
        assert_eq!(
            tint(0, 3),
            Some(Color32::from_rgba_unmultiplied(0xf0, 0xa3, 0x5e, 160))
        );
        assert_eq!(tint(1, 3), None);
        assert_eq!(
            tint(2, 3),
            Some(Color32::from_rgba_unmultiplied(0x5a, 0xa9, 0xe6, 160))
        );
        // Five: the inner two are half as strong; four have no middle.
        assert_eq!(
            tint(1, 5),
            Some(Color32::from_rgba_unmultiplied(0xf0, 0xa3, 0x5e, 100))
        );
        assert_eq!(
            tint(3, 5),
            Some(Color32::from_rgba_unmultiplied(0x5a, 0xa9, 0xe6, 100))
        );
        assert_eq!(
            tint(1, 4),
            Some(Color32::from_rgba_unmultiplied(0xf0, 0xa3, 0x5e, 80))
        );
        assert_eq!(
            tint(2, 4),
            Some(Color32::from_rgba_unmultiplied(0x5a, 0xa9, 0xe6, 80))
        );
        // One band, or none, can't be told apart.
        assert_eq!(tint(0, 1), None);
        assert_eq!(tint(0, 0), None);
    }

    #[test]
    fn a_curves_numbers_are_plotted_at_their_value() {
        assert_eq!(points_of("50".parse().expect("a number")), 50.0);
        assert_eq!(points_of("-0.25".parse().expect("a number")), -0.25);
        assert_eq!(points_of(Fixed::ZERO), 0.0);
    }

    #[test]
    fn problems_are_counted_in_words() {
        assert_eq!(problems(1), "1 problem");
        assert_eq!(problems(3), "3 problems");
    }
}

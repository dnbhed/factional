//! The relations matrix (U12c, G3): each faction's regard for each other, from the engine's
//! `relations_matrix`, drawn for the editor's Previews and the inspector alike.

use std::cmp::Ordering;

use egui::{Color32, WidgetInfo, WidgetType};
use factional_reputation::World;

/// Scores in bands below the middle one are tinted orange, above it blue: the design's
/// hostile and friendly colours.
const HOSTILE: (u8, u8, u8) = (0xf0, 0xa3, 0x5e);
const FRIENDLY: (u8, u8, u8) = (0x5a, 0xa9, 0xe6);

/// Each faction's regard for each other, tinted by its relation band as the disposition
/// matrix is by its, marked where the pair is in conflict, and labelled in full for anyone who
/// can't see the grid (U12c).
pub fn relations_ui(ui: &mut egui::Ui, world: &World) {
    let matrix = world.relations_matrix();
    let bands: Vec<&str> = world
        .balance()
        .relation_bands
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    egui::Grid::new("relations").striped(true).show(ui, |ui| {
        ui.label("faction");
        for faction in &matrix.factions {
            ui.label(faction.as_str());
        }
        ui.end_row();
        for row in &matrix.rows {
            ui.label(row.from.as_str());
            for cell in &row.cells {
                let Some(cell) = cell else {
                    ui.label("—");
                    continue;
                };
                let regard = &cell.regard;
                let band = bands.iter().position(|b| *b == regard.band);
                let shown = match cell.conflict {
                    true => format!("{} ⚔", regard.value),
                    false => regard.value.to_string(),
                };
                let mut text = egui::RichText::new(shown).monospace();
                if let Some(tint) = band.and_then(|band| tint(band, bands.len())) {
                    text = text
                        .background_color(tint)
                        .color(ui.visuals().strong_text_color());
                }
                let conflict = if cell.conflict { ", in conflict" } else { "" };
                let said = format!(
                    "{} → {}: {}, {}{conflict}",
                    regard.from, regard.to, regard.value, regard.band
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
pub fn tint(index: usize, count: usize) -> Option<Color32> {
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
}

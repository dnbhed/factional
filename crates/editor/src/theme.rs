//! The editor's look (U6e, board 6): egui's own dark visuals, tuned to the design canvas's
//! colours rather than replaced, and the few colours of the editor's own.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Visuals};

/// A world that loads.
pub const LOADS: Color32 = Color32::from_rgb(0x5c, 0xc9, 0x95);
/// A `locks` edge, and hostile tints.
pub const LOCKS: Color32 = Color32::from_rgb(0xf0, 0xa3, 0x5e);
/// A designer's comment.
pub const COMMENT: Color32 = Color32::from_rgb(0x9d, 0xb5, 0x96);

/// A node with a problem, behind its outline.
pub const PROBLEM_FILL: Color32 = Color32::from_rgb(0x24, 0x19, 0x1a);
/// The summary's pill: loads, or doesn't.
pub const LOADS_FILL: Color32 = Color32::from_rgb(0x17, 0x30, 0x2a);
pub const BROKEN_FILL: Color32 = Color32::from_rgb(0x3a, 0x1f, 0x1d);

/// egui's dark visuals with the canvas's colours: fields, panels, windows, side panels,
/// widgets, selection, links, problems, and warnings and edits.
pub fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.extreme_bg_color = Color32::from_rgb(0x10, 0x12, 0x15);
    visuals.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1b);
    visuals.window_fill = Color32::from_rgb(0x1f, 0x23, 0x28);
    visuals.faint_bg_color = Color32::from_rgb(0x1a, 0x1d, 0x21);
    let widget = Color32::from_rgb(0x2a, 0x2e, 0x34);
    visuals.widgets.inactive.weak_bg_fill = widget;
    visuals.widgets.inactive.bg_fill = widget;
    visuals.selection.bg_fill = Color32::from_rgb(0x1f, 0x4a, 0x6b);
    visuals.hyperlink_color = Color32::from_rgb(0x7c, 0xc0, 0xf2);
    visuals.error_fg_color = Color32::from_rgb(0xff, 0x7a, 0x6b);
    visuals.warn_fg_color = Color32::from_rgb(0xf0, 0xc6, 0x6a);
    visuals
}

/// The family for headings: Plex Sans SemiBold.
pub const SEMIBOLD: &str = "semibold";

/// IBM Plex first (U6g): Sans for prose, Mono for monospace, Sans SemiBold as its own family
/// for headings; egui's own faces after each, for anything Plex hasn't.
pub fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 3] = [
        (
            "plex-sans",
            include_bytes!("../fonts/IBMPlexSans-Regular.ttf"),
        ),
        (
            "plex-sans-semibold",
            include_bytes!("../fonts/IBMPlexSans-SemiBold.ttf"),
        ),
        (
            "plex-mono",
            include_bytes!("../fonts/IBMPlexMono-Regular.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let proportional = fonts.families[&FontFamily::Proportional].clone();
    let first = |face: &str, after: &[String]| {
        let mut family = vec![face.to_owned()];
        family.extend_from_slice(after);
        family
    };
    let monospace = first("plex-mono", &fonts.families[&FontFamily::Monospace]);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        first("plex-sans-semibold", &proportional),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, first("plex-sans", &proportional));
    fonts.families.insert(FontFamily::Monospace, monospace);
    fonts
}

/// Board 6's sizes: headings 22 in SemiBold, body and buttons 13, monospace 12.5, small 11.
pub fn text_styles() -> BTreeMap<TextStyle, FontId> {
    [
        (
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Name(SEMIBOLD.into())),
        ),
        (TextStyle::Body, FontId::proportional(13.0)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
        (TextStyle::Small, FontId::proportional(11.0)),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Color32 {
        Color32::from_rgb(r, g, b)
    }

    #[test]
    fn plex_comes_first_in_each_family_with_eguis_faces_after() {
        let fonts = fonts();
        let family = |family: FontFamily| fonts.families.get(&family).cloned().unwrap_or_default();
        let proportional = family(FontFamily::Proportional);
        let monospace = family(FontFamily::Monospace);
        let semibold = family(FontFamily::Name(SEMIBOLD.into()));
        assert_eq!(proportional[0], "plex-sans");
        assert_eq!(monospace[0], "plex-mono");
        assert_eq!(semibold[0], "plex-sans-semibold");
        let defaults = FontDefinitions::default();
        for (found, family) in [
            (&proportional, FontFamily::Proportional),
            (&monospace, FontFamily::Monospace),
        ] {
            assert_eq!(found[1..], defaults.families[&family][..]);
        }
        assert_eq!(
            semibold[1..],
            defaults.families[&FontFamily::Proportional][..]
        );
        for face in ["plex-sans", "plex-sans-semibold", "plex-mono"] {
            assert!(fonts.font_data.contains_key(face), "{face}");
        }
    }

    #[test]
    fn the_sizes_are_board_sixs() {
        let styles = text_styles();
        let style = |style: TextStyle| styles.get(&style).cloned();
        assert_eq!(
            style(TextStyle::Heading),
            Some(FontId::new(22.0, FontFamily::Name(SEMIBOLD.into())))
        );
        assert_eq!(style(TextStyle::Body), Some(FontId::proportional(13.0)));
        assert_eq!(style(TextStyle::Button), Some(FontId::proportional(13.0)));
        assert_eq!(style(TextStyle::Monospace), Some(FontId::monospace(12.5)));
        assert_eq!(style(TextStyle::Small), Some(FontId::proportional(11.0)));
    }

    #[test]
    fn the_visuals_are_the_canvas_colours() {
        let visuals = visuals();
        assert!(visuals.dark_mode);
        assert_eq!(visuals.extreme_bg_color, rgb(0x10, 0x12, 0x15));
        assert_eq!(visuals.panel_fill, rgb(0x16, 0x18, 0x1b));
        assert_eq!(visuals.window_fill, rgb(0x1f, 0x23, 0x28));
        assert_eq!(visuals.faint_bg_color, rgb(0x1a, 0x1d, 0x21));
        assert_eq!(visuals.widgets.inactive.weak_bg_fill, rgb(0x2a, 0x2e, 0x34));
        assert_eq!(visuals.widgets.inactive.bg_fill, rgb(0x2a, 0x2e, 0x34));
        assert_eq!(visuals.selection.bg_fill, rgb(0x1f, 0x4a, 0x6b));
        assert_eq!(visuals.hyperlink_color, rgb(0x7c, 0xc0, 0xf2));
        assert_eq!(visuals.error_fg_color, rgb(0xff, 0x7a, 0x6b));
        assert_eq!(visuals.warn_fg_color, rgb(0xf0, 0xc6, 0x6a));
    }
}

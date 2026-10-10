//! The editor's look (U6e, board 6): egui's own dark visuals, tuned to the design canvas's
//! colours rather than replaced, and the few colours of the editor's own.

use crate::egui::{Color32, Visuals};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Color32 {
        Color32::from_rgb(r, g, b)
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

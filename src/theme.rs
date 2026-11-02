//! `BeamWright`'s visual identity.
//!
//! A blue-grey drawing-tool palette with one clear blue accent, compact controls
//! and modest rounding. The dark theme is the default; the light theme keeps the
//! same roles so a drawing looks the same in either.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

/// Accent used for selection, hovered controls and links.
pub const ACCENT: Color32 = Color32::from_rgb(58, 136, 238);
/// Canvas selection default, deliberately lighter than the interface accent.
pub const CANVAS_SELECTION: &str = "#4EA1DC";

fn dark() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(39, 43, 52);
    visuals.window_fill = Color32::from_rgb(39, 43, 52);
    visuals.extreme_bg_color = Color32::from_rgb(28, 31, 38);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(28, 31, 38));
    visuals.faint_bg_color = Color32::from_rgb(47, 52, 62);
    visuals.code_bg_color = Color32::from_rgb(28, 31, 38);
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(63, 69, 82));
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.hyperlink_color = ACCENT;
    visuals.selection = egui::style::Selection {
        bg_fill: ACCENT,
        stroke: Stroke::new(1.0, Color32::from_rgb(255, 255, 255)),
    };
    visuals.warn_fg_color = Color32::from_rgb(236, 170, 70);
    visuals.error_fg_color = Color32::from_rgb(240, 100, 96);
    visuals.override_text_color = Some(Color32::from_rgb(222, 226, 232));
    visuals.weak_text_color = Some(Color32::from_rgb(148, 155, 168));
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = true;
    visuals.slider_trailing_fill = true;
    let panel = visuals.panel_fill;
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(63, 69, 82));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 43, 52);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(47, 52, 62);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(63, 69, 82));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(54, 61, 74);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(54, 61, 74);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(245, 247, 250));
    visuals.widgets.open.bg_fill = Color32::from_rgb(47, 52, 62);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(74, 82, 98));
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(6);
    }
    visuals
}

fn light() -> Visuals {
    let mut visuals = Visuals::light();
    visuals.panel_fill = Color32::from_rgb(250, 251, 252);
    visuals.window_fill = Color32::from_rgb(255, 255, 255);
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(255, 255, 255));
    visuals.faint_bg_color = Color32::from_rgb(240, 242, 246);
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(208, 212, 220));
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.hyperlink_color = Color32::from_rgb(38, 120, 218);
    visuals.selection = egui::style::Selection {
        bg_fill: Color32::from_rgb(38, 120, 218),
        stroke: Stroke::new(1.0, Color32::WHITE),
    };
    visuals.override_text_color = Some(Color32::from_rgb(38, 42, 50));
    visuals.weak_text_color = Some(Color32::from_rgb(110, 116, 128));
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = true;
    visuals.slider_trailing_fill = true;
    let panel = visuals.panel_fill;
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(208, 212, 220));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(255, 255, 255);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(246, 247, 249);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(195, 203, 214));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(226, 234, 246);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(226, 234, 246);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(38, 120, 218));
    visuals.widgets.active.bg_fill = Color32::from_rgb(38, 120, 218);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(38, 120, 218);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(38, 120, 218));
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.bg_fill = Color32::from_rgb(244, 246, 249);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(208, 212, 220));
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(6);
    }
    visuals
}

/// Visuals for the current theme.
pub fn visuals(dark_mode: bool) -> Visuals {
    if dark_mode { dark() } else { light() }
}

/// Apply the palette, compact spacing and type scale to the whole interface.
///
/// Called once per frame from the app, after the user's dark/light choice is
/// known. Keeping it in one place means every panel, menu and dialog gets the
/// same rhythm.
pub fn apply(ctx: &egui::Context, dark_mode: bool) {
    for (theme, palette) in [(egui::Theme::Dark, dark()), (egui::Theme::Light, light())] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals = palette;
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size = egui::vec2(40.0, 24.0);
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(18.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
        ]
        .into();
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(if dark_mode {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
}

#[cfg(test)]
mod tests {
    use super::{CANVAS_SELECTION, visuals};

    #[test]
    fn both_themes_have_the_accent_and_lighter_corners() {
        for dark in [true, false] {
            let visuals = visuals(dark);
            assert_eq!(
                visuals.interact_cursor,
                Some(egui::CursorIcon::PointingHand)
            );
            assert!(visuals.window_corner_radius.nw > 0);
            assert!(visuals.widgets.inactive.corner_radius.nw > 0);
            assert_eq!(visuals.dark_mode, dark);
        }
    }

    #[test]
    fn the_canvas_selection_colour_is_not_the_interface_accent() {
        assert_ne!(CANVAS_SELECTION.to_ascii_lowercase(), "#3a88ee".to_string());
    }

    #[test]
    fn the_dark_palette_uses_the_blue_grey_surfaces() {
        let visuals = visuals(true);
        assert_eq!(visuals.panel_fill, egui::Color32::from_rgb(39, 43, 52));
        assert_eq!(visuals.window_fill, egui::Color32::from_rgb(39, 43, 52));
        assert_eq!(visuals.widgets.active.bg_fill, super::ACCENT);
    }
}

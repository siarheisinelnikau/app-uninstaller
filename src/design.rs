//! Shared visual roles; no animations or decorative rendering in settled views.
use egui::{Color32, Context, FontId, TextStyle};
pub const BACKGROUND: Color32 = Color32::from_rgb(242, 246, 245);
pub const SURFACE: Color32 = Color32::WHITE;
pub const TEXT: Color32 = Color32::from_rgb(30, 47, 49);
pub const MUTED: Color32 = Color32::from_rgb(83, 103, 106);
pub const ACCENT: Color32 = Color32::from_rgb(13, 103, 98);
pub const SELECTION: Color32 = Color32::from_rgb(219, 237, 232);

pub fn configure(ctx: &Context) {
    #[cfg(windows)]
    if let Ok(windows) = crate::command::windows_directory() {
        let path = std::path::PathBuf::from(windows)
            .join("Fonts")
            .join("segoeui.ttf");
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() < 2 * 1024 * 1024)
            && let Ok(bytes) = std::fs::read(path)
        {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "Windows UI".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Windows UI".into());
            ctx.set_fonts(fonts);
        }
    }
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut(|style| {
        style.wrap_mode = Some(egui::TextWrapMode::Wrap);
        for (role, size) in [
            (TextStyle::Heading, 22.),
            (TextStyle::Body, 15.),
            (TextStyle::Button, 14.),
            (TextStyle::Small, 12.),
        ] {
            style.text_styles.insert(role, FontId::proportional(size));
        }
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.));
        style.spacing.item_spacing = egui::vec2(12., 10.);
        style.spacing.button_padding = egui::vec2(16., 9.);
        style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
        style.visuals.widgets.inactive.fg_stroke.color = TEXT;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(230, 237, 234);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke.color = Color32::WHITE;
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.panel_fill = BACKGROUND;
        style.animation_time = 0.10;
    });
}

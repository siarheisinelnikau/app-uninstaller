//! Shared visual roles; no animations or decorative rendering in settled views.
use egui::{Color32, Context, FontId, TextStyle};
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub selection: Color32,
}
pub fn palette(theme: crate::settings::Theme) -> Palette {
    if theme == crate::settings::Theme::Dark {
        Palette {
            background: Color32::from_rgb(22, 30, 32),
            surface: Color32::from_rgb(30, 40, 42),
            text: Color32::from_rgb(232, 240, 238),
            muted: Color32::from_rgb(173, 192, 188),
            accent: Color32::from_rgb(25, 126, 116),
            accent_text: Color32::from_rgb(107, 220, 201),
            selection: Color32::from_rgb(40, 75, 69),
        }
    } else {
        Palette {
            background: Color32::from_rgb(242, 246, 245),
            surface: Color32::WHITE,
            text: Color32::from_rgb(30, 47, 49),
            muted: Color32::from_rgb(83, 103, 106),
            accent: Color32::from_rgb(13, 103, 98),
            accent_text: Color32::from_rgb(13, 103, 98),
            selection: Color32::from_rgb(219, 237, 232),
        }
    }
}

pub fn apply_theme(ctx: &Context, theme: crate::settings::Theme) {
    let colors = palette(theme);
    let mut visuals = if theme == crate::settings::Theme::Dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.widgets.noninteractive.fg_stroke.color = colors.text;
    visuals.widgets.inactive.fg_stroke.color = colors.text;
    visuals.widgets.inactive.weak_bg_fill = colors.selection;
    visuals.selection.bg_fill = colors.accent;
    visuals.selection.stroke.color = Color32::WHITE;
    visuals.hyperlink_color = if theme == crate::settings::Theme::Dark {
        Color32::from_rgb(107, 220, 201)
    } else {
        colors.accent
    };
    visuals.panel_fill = colors.background;
    visuals.window_fill = colors.surface;
    visuals.extreme_bg_color = colors.background;
    ctx.set_visuals(visuals);
}

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
    apply_theme(ctx, crate::settings::Theme::default());
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
        style.animation_time = 0.10;
    });
}

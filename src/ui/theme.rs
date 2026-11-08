use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference, Visuals};

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Cor de destaque (accent) usada em cursores e realces.
pub fn accent(dark: bool) -> Color32 {
    if dark {
        rgb(0x4C, 0xC2, 0xFF)
    } else {
        rgb(0x00, 0x67, 0xC0)
    }
}

/// Aplica um visual inspirado no Windows 11 (claro) ou um tema escuro equivalente.
pub fn apply(ctx: &egui::Context, dark: bool) {
    let theme = if dark { Theme::Dark } else { Theme::Light };
    ctx.set_theme(if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    });

    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    if dark {
        v.panel_fill = rgb(0x20, 0x20, 0x20);
        v.window_fill = rgb(0x2B, 0x2B, 0x2B);
        v.extreme_bg_color = rgb(0x19, 0x19, 0x19);
        v.faint_bg_color = rgb(0x26, 0x26, 0x26);
        v.selection.bg_fill = rgb(0x2F, 0x4F, 0x72);
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.widgets.hovered.weak_bg_fill = rgb(0x3A, 0x3A, 0x3A);
        v.widgets.hovered.bg_fill = rgb(0x3A, 0x3A, 0x3A);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, rgb(0x3A, 0x3A, 0x3A));
        v.hyperlink_color = accent(true);
    } else {
        v.panel_fill = rgb(0xF3, 0xF3, 0xF3);
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = rgb(0xF7, 0xF7, 0xF7);
        v.selection.bg_fill = rgb(0xCC, 0xE8, 0xFF);
        v.selection.stroke = Stroke::new(1.0, rgb(0x1A, 0x1A, 0x1A));
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, rgb(0x1B, 0x1B, 0x1B));
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, rgb(0xE5, 0xE5, 0xE5));
        v.widgets.inactive.weak_bg_fill = rgb(0xFB, 0xFB, 0xFB);
        v.widgets.inactive.bg_fill = rgb(0xFB, 0xFB, 0xFB);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, rgb(0x1B, 0x1B, 0x1B));
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, rgb(0xE0, 0xE0, 0xE0));
        v.widgets.hovered.weak_bg_fill = rgb(0xEA, 0xEA, 0xEA);
        v.widgets.hovered.bg_fill = rgb(0xEA, 0xEA, 0xEA);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, rgb(0xD0, 0xD0, 0xD0));
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, rgb(0x1B, 0x1B, 0x1B));
        v.widgets.active.weak_bg_fill = rgb(0xDD, 0xDD, 0xDD);
        v.widgets.active.bg_fill = rgb(0xDD, 0xDD, 0xDD);
        v.widgets.active.fg_stroke = Stroke::new(1.0, rgb(0x1B, 0x1B, 0x1B));
        v.widgets.open.weak_bg_fill = rgb(0xE5, 0xE5, 0xE5);
        v.widgets.open.bg_fill = rgb(0xE5, 0xE5, 0xE5);
        v.hyperlink_color = accent(false);
        v.window_stroke = Stroke::new(1.0, rgb(0xDA, 0xDA, 0xDA));
        v.window_shadow.color = Color32::from_black_alpha(40);
    }
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    ctx.set_visuals_of(theme, v);

    ctx.style_mut_of(theme, |s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.text_styles
            .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles
            .insert(TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles
            .insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
        s.text_styles
            .insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional));
        s.text_styles
            .insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
        s.interaction.selectable_labels = false;
    });
}

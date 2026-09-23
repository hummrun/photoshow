//! Visual theme adapter for egui.

/// Cor semântica de destaque usada para seleção e foco visual.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(10, 132, 255);

pub fn apply(theme_name: &str, ctx: &egui::Context) {
    let theme = match theme_name {
        "charcoal" => elegance::Theme::charcoal(),
        "frost" => elegance::Theme::frost(),
        "paper" => elegance::Theme::paper(),
        _ => elegance::Theme::slate(),
    };
    theme.install(ctx);
    tune_style(ctx);
}

fn tune_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::Vec2::new(8.0, 6.0);
        style.spacing.button_padding = egui::Vec2::new(10.0, 6.0);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(8);
        }
        style.visuals.window_corner_radius = egui::CornerRadius::same(12);
        style.visuals.menu_corner_radius = egui::CornerRadius::same(8);
    });
}

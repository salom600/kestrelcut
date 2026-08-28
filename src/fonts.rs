//! egui theme + fonts.

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // (Latin fonts only — UI is English.)
    ctx.set_fonts(fonts);

    let mut style = egui::Style::default();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(egui::Color32::from_rgb(205, 207, 214));
    v.panel_fill = egui::Color32::from_rgb(22, 22, 26);
    v.window_fill = egui::Color32::from_rgb(26, 26, 31);
    v.extreme_bg_color = egui::Color32::from_rgb(10, 10, 12);
    v.faint_bg_color = egui::Color32::from_rgb(30, 30, 36);
    v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 28, 33);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 182, 190));
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 56, 64));
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(35, 35, 41);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(150, 152, 160));
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 52);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.2, egui::Color32::from_rgb(220, 222, 228));
    v.widgets.active.bg_fill = egui::Color32::from_rgb(31, 78, 140);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.2, egui::Color32::from_rgb(240, 242, 248));
    v.selection.bg_fill = egui::Color32::from_rgb(31, 78, 140);
    v.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(47, 129, 247));
    v.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 56, 64));
    v.text_cursor.stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(47, 129, 247));
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(12.5));
    style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(10.0));
    style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(16.0));
    style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
    ctx.set_style(style);
}

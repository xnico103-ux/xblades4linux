use eframe::egui::{self, Color32, Context, RichText, Ui};

pub const ACCENT: Color32 = Color32::from_rgb(0xe8, 0x43, 0x3a); // rouge du panache du logo
pub const OK: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x50);
pub const WARN: Color32 = Color32::from_rgb(0xd9, 0xa3, 0x2b);

pub fn apply(ctx: &Context) {
    ctx.style_mut(|s| {
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.hyperlink_color = ACCENT;
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.slider_width = 220.0;
        let r = egui::Rounding::same(7.0);
        let w = &mut s.visuals.widgets;
        w.noninteractive.rounding = r; w.inactive.rounding = r; w.hovered.rounding = r; w.active.rounding = r; w.open.rounding = r;
        s.visuals.window_rounding = egui::Rounding::same(10.0);
        s.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::WHITE);
        s.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    });
}

/// Section encadrée avec un titre : donne un regroupement visuel clair à chaque onglet.
pub fn card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).rounding(egui::Rounding::same(9.0)).inner_margin(12.0).fill(Color32::from_gray(36)).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(52))).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(15.0));
        ui.add_space(4.0);
        add(ui);
    });
    ui.add_space(8.0);
}


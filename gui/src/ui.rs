//! Widgets et onglets (fonctions pures sur `Config`, sans accès à la souris).
use crate::theme::{card, ACCENT, WARN};
use eframe::egui::{self, Color32, RichText, Ui};
use xblades_proto::config::*;

const PRESETS: [[u8; 3]; 8] = [
    [255, 0, 0], [255, 100, 0], [255, 200, 0], [0, 255, 0], [0, 220, 220], [0, 0, 255], [160, 0, 255], [255, 255, 255],
];

/// Sélecteur de couleur + pastilles prédéfinies. Renvoie vrai si la valeur a changé.
pub fn color_picker(ui: &mut Ui, rgb: &mut [u8; 3]) -> bool {
    let mut changed = ui.color_edit_button_srgb(rgb).changed();
    for p in PRESETS {
        let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
        ui.painter().rect_filled(r, 9.0_f32, Color32::from_rgb(p[0], p[1], p[2]));
        if *rgb == p { ui.painter().circle_stroke(r.center(), 10.0_f32, egui::Stroke::new(2.0_f32, ACCENT)); }
        if resp.on_hover_text(format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2])).clicked() { *rgb = p; changed = true; }
    }
    changed
}

fn dpi_label(step: f64) -> String {
    let s = step as usize & 15;
    format!("{} DPI{}", DPI_TABLE[s], if CONFIRMED_STEPS.contains(&(s as u8)) { "" } else { " (estimé)" })
}

pub fn lighting(ui: &mut Ui, c: &mut Config) {
    card(ui, "Couleur", |ui| {
        ui.label("Une seule couleur pour tous les niveaux DPI et toute la palette.");
        ui.horizontal(|ui| {
            let mut main = c.uniform_color().unwrap_or(c.gear_colors[0]);
            if color_picker(ui, &mut main) { c.set_uniform_color(main); }
            if ui.button("Rouge fixe").on_hover_text("Rouge partout, sans fondu").clicked() { c.set_uniform_color([255, 0, 0]); c.fixed = true; }
        });
        if c.uniform_color().is_none() {
            ui.colored_label(WARN, "Les couleurs diffèrent selon les niveaux / la palette (réglages avancés ci-dessous).");
        }
    });
    card(ui, "Effet", |ui| {
        // Mode (fixe/fondu) et effet partagent une seule liste : 0 = fixe, 1 = fondu, 2.. = autres effets.
        let cur = if c.fixed { 0 } else { c.effect as usize + 1 };
        let name = |i: usize| match i { 0 => "Fixe : la couleur reste allumée".to_string(), 1 => "Fondu : la couleur s'estompe et revient".to_string(), n => format!("Effet {} (non vérifié)", n) };
        let mut sel = cur;
        egui::ComboBox::from_id_salt("effect").width(320.0).selected_text(name(cur)).show_ui(ui, |ui| {
            for i in 0..9usize { ui.selectable_value(&mut sel, i, name(i)); }
        });
        if sel != cur {
            if sel == 0 { c.fixed = true; c.effect = 0; }
            else { c.fixed = false; c.effect = (sel - 1) as u8; }
        }
        ui.label(RichText::new("Fixe et Fondu sont confirmés sur la souris ; les autres effets ne sont pas testés. Changer d'effet redémarre la souris.").color(Color32::from_gray(150)));
        if !c.fixed {
            let mut l = c.speed_level();
            if ui.add(egui::Slider::new(&mut l, 1..=10).text("vitesse du fondu")).changed() { c.set_speed_level(l); }
            ui.label(RichText::new("Sens exact à confirmer : testez 1 et 10.").color(Color32::from_gray(150)));
        }
    });
    card(ui, "Réglages avancés", |ui| {
        egui::CollapsingHeader::new("Couleur par niveau DPI").show(ui, |ui| {
            for g in 0..c.gear_count as usize {
                ui.horizontal(|ui| {
                    ui.label(format!("Niveau {} · {:>5} DPI", g + 1, c.dpi(g)));
                    color_picker(ui, &mut c.gear_colors[g]);
                });
            }
        });
        egui::CollapsingHeader::new("Palette des effets animés (10 couleurs)").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| { for col in c.palette.iter_mut() { ui.color_edit_button_srgb(col); } });
        });
    });
}

pub fn dpi(ui: &mut Ui, c: &mut Config) {
    card(ui, "Niveaux de sensibilité", |ui| {
        ui.add(egui::Slider::new(&mut c.gear_count, 1..=GEARS as u8).text("nombre de niveaux"));
        c.current_gear = c.current_gear.min(c.gear_count - 1);
        ui.add_space(4.0);
        for g in 0..c.gear_count as usize {
            ui.horizontal(|ui| {
                ui.radio_value(&mut c.current_gear, g as u8, "").on_hover_text("Niveau actif");
                ui.label(format!("Niveau {}", g + 1));
                ui.add(egui::Slider::new(&mut c.steps[g], 0..=15).custom_formatter(|v, _| dpi_label(v)).custom_parser(|_| None));
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Valeurs d'usine confirmées : 800, 1600, 2400, 3200, 4800 et 6400 DPI ; les autres sont estimées.").color(Color32::from_gray(150)));
    });
}

pub fn performance(ui: &mut Ui, c: &mut Config, flash: bool) {
    card(ui, "Taux d'interrogation", |ui| {
        if !flash { ui.colored_label(WARN, "Verrouillé : ce réglage écrit dans la mémoire permanente. Active l'écriture permanente dans l'onglet Diagnostic."); }
        ui.add_enabled_ui(flash, |ui| {
            ui.horizontal(|ui| { for hz in POLL_RATES { ui.selectable_value(&mut c.polling, hz, format!("{hz} Hz")); } });
        });
        ui.label(RichText::new("Un changement redémarre la souris quelques secondes (reconnexion automatique).").color(Color32::from_gray(150)));
    });
}

pub fn buttons(ui: &mut Ui, c: &mut Config, profile: &mut usize, flash: bool) {
    if !flash { ui.colored_label(WARN, "Verrouillé : les boutons s'écrivent dans la mémoire permanente. Active l'écriture permanente dans l'onglet Diagnostic."); }
    else { ui.colored_label(WARN, "Expérimental. Après « Appliquer », la souris redémarre quelques secondes. Sauvegarde faite avant écriture."); }
    card(ui, "Actions des boutons", |ui| { ui.add_enabled_ui(flash, |ui| buttons_body(ui, c, profile)); });
}

fn buttons_body(ui: &mut Ui, c: &mut Config, profile: &mut usize) {
    ui.horizontal(|ui| {
        ui.label("Profil à modifier :");
        for p in 0..PROFILES {
            let t = format!("{}", p + 1);
            ui.selectable_value(profile, p, t);
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Choisir un profil ici ne change pas le profil actif de la souris (aucun registre connu pour cela) : il sert seulement à choisir lequel modifier. Le profil actif se change avec le bouton de la souris. « Appliquer » s'active dès qu'une action est modifiée.").color(Color32::from_gray(150)));
    let p = (*profile).min(PROFILES - 1);
    for slot in SLOT_ORDER {
        let cur = Action::decode(c.profiles[p][slot]);
        let mut act = cur;
        ui.horizontal(|ui| {
            ui.add_sized([200.0, 20.0], egui::Label::new(SLOT_NAMES[slot]));
            egui::ComboBox::from_id_salt(("slot", p, slot)).width(230.0).selected_text(cur.label()).show_ui(ui, |ui| {
                ui.selectable_value(&mut act, Action::Disabled, "Désactivé");
                for (m, n) in MOUSE_BUTTONS { ui.selectable_value(&mut act, Action::Mouse(m), n); }
                ui.selectable_value(&mut act, Action::Dpi, "Changer de DPI");
                for (u, n) in CONSUMER { ui.selectable_value(&mut act, Action::Consumer(u), n); }
                if let Action::Raw(_) = cur { ui.selectable_value(&mut act, cur, cur.label()); }
                if let Action::Key { .. } = cur { ui.selectable_value(&mut act, cur, cur.label()); }
            });
            if let Action::Key { mut modifier, mut key } = act {
                for (bit, n) in [(1u8, "Ctrl"), (2, "Maj"), (4, "Alt"), (8, "Win")] {
                    let mut on = modifier & bit != 0;
                    if ui.checkbox(&mut on, n).changed() { modifier ^= bit; }
                }
                egui::ComboBox::from_id_salt(("key", p, slot)).width(90.0).selected_text(key_name(key)).show_ui(ui, |ui| {
                    for k in KEYS { ui.selectable_value(&mut key, *k, key_name(*k)); }
                });
                act = Action::Key { modifier, key };
            } else if ui.small_button("Raccourci…").clicked() {
                act = Action::Key { modifier: 1, key: 0x06 };
            }
            if act != cur { c.profiles[p][slot] = act.encode(); }
        });
    }
    ui.add_space(6.0);
    ui.label("Au moins un « Clic gauche » doit rester dans le profil.");
}

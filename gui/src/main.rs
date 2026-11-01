mod theme;
mod ui;
mod worker;

use eframe::egui::{self, Color32, RichText};
use std::time::{Duration, Instant};
use worker::{Cmd, Event, Worker};
use xblades_proto::{config::Config, hidraw};

const LOGO: &[u8] = include_bytes!("../assets/logo.png");

#[derive(PartialEq, Clone, Copy)]
enum Tab { Lighting, Dpi, Buttons, Performance, Diagnostic }

#[derive(Clone, Copy, PartialEq)]
enum Level { Info, Ok, Warn, Error }

struct App {
    w: Worker,
    logo: egui::TextureHandle,
    tab: Tab,
    profile: Option<usize>,
    saved: Option<Config>,
    edit: Option<Config>,
    node: String,
    busy: bool,
    status: (Level, String),
    last_try: Option<Instant>,
    pending: Option<Config>,
    confirm_restore: bool,
    need_access: bool,
    diag: Option<String>,
    flash_ok: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        theme::apply(&cc.egui_ctx);
        let img = image::load_from_memory(LOGO).expect("logo").to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as _, img.height() as _], img.as_raw());
        App {
            w: worker::spawn(cc.egui_ctx.clone()),
            logo: cc.egui_ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR),
            profile: None,
            tab: match std::env::var("XBLADES_TAB").as_deref() { Ok("dpi") => Tab::Dpi, Ok("perf") => Tab::Performance, Ok("buttons") => Tab::Buttons, Ok("diag") => Tab::Diagnostic, _ => Tab::Lighting }, saved: None, edit: None, node: String::new(), busy: false,
            status: (Level::Info, "Recherche de la souris…".into()), last_try: None, pending: None, confirm_restore: false, need_access: false, diag: None, flash_ok: false,
        }
    }
    fn send(&mut self, c: Cmd) { self.busy = true; let _ = self.w.tx.send(c); }
    fn set(&mut self, l: Level, m: impl Into<String>) { self.status = (l, m.into()); }

    fn handle(&mut self, ev: Event) {
        self.busy = false;
        match ev {
            Event::Connected { node, cfg } => {
                self.node = node; self.need_access = false;
                let msg = match self.pending.take() {
                    Some(w) => {
                        let mut bad = vec![];
                        if w.polling != cfg.polling { bad.push(format!("taux d'interrogation ({} Hz au lieu de {} Hz)", cfg.polling, w.polling)); }
                        if w.profiles != cfg.profiles { bad.push("boutons (la souris a relu d'autres actions)".into()); }
                        if bad.is_empty() { (Level::Ok, "Réglages appliqués après redémarrage de la souris".into()) }
                        else { (Level::Warn, format!("Non pris en compte par la souris : {}", bad.join(", "))) }
                    }
                    None => (Level::Ok, "Connectée".into()),
                };
                self.status = msg;
                self.saved = Some(cfg.clone()); self.edit = Some(cfg);
            }
            Event::Refreshed(cfg) => { self.saved = Some(cfg.clone()); self.edit = Some(cfg); self.set(Level::Ok, "Configuration relue"); }
            Event::Applied { cfg, reloaded, writes } => {
                if reloaded {
                    self.pending = self.edit.clone();
                    self.saved = None; self.edit = None;
                    self.set(Level::Info, "Réglages écrits — la souris redémarre, reconnexion automatique…");
                } else {
                    let same = self.edit.as_ref().is_none_or(|e| {
                        let mut a = e.clone(); a.checksum_ok = cfg.checksum_ok; a.product = cfg.product.clone(); a == cfg
                    });
                    if same { self.set(Level::Ok, format!("{writes} réglage(s) appliqué(s) et vérifié(s)")); }
                    else { self.set(Level::Warn, "Écrit, mais la souris a relu des valeurs différentes (affichées)"); }
                    self.saved = Some(cfg.clone()); self.edit = Some(cfg);
                }
            }
            Event::Restored => {
                self.saved = None; self.edit = None; self.pending = None;
                self.set(Level::Info, "Image écrite — la souris redémarre, reconnexion automatique…");
            }
            Event::Exported(p) => self.set(Level::Ok, format!("Configuration exportée : {}", p.display())),
            Event::Failed { msg, lost } => {
                if lost {
                    self.saved = None; self.edit = None;
                    self.need_access = hidraw::Hidraw::needs_access();
                    if self.pending.is_none() { self.set(Level::Warn, "Souris introuvable — reconnexion automatique…"); }
                } else { self.set(Level::Error, msg); }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(ev) = self.w.rx.try_recv() { self.handle(ev); }
        if self.saved.is_none() && !self.busy && !self.need_access && self.last_try.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
            self.last_try = Some(Instant::now());
            self.send(Cmd::Connect { sim: std::env::var_os("XBLADES_SIM").is_some() });
        }
        if self.saved.is_none() { ctx.request_repaint_after(Duration::from_secs(2)); }
        let dirty = self.saved != self.edit;
        if dirty && !self.busy && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            if let (Some(o), Some(n)) = (self.saved.clone(), self.edit.clone()) { self.send(Cmd::Apply { old: o, new: n, flash: self.flash_ok }); self.set(Level::Info, "Écriture en cours…"); }
        }

        egui::TopBottomPanel::top("head").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.image((self.logo.id(), egui::vec2(44.0, 44.0)));
                ui.vertical(|ui| {
                    ui.label(RichText::new("XBlades Linux").heading());
                    ui.label(RichText::new("EMPIRE GAMING X-Blades · 1bcf:08b8").color(Color32::from_gray(150)));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (c, t) = if self.saved.is_some() { (theme::OK, format!("Connectée ({})", self.node)) } else { (theme::WARN, "Non connectée".into()) };
                    ui.label(RichText::new(t).color(c));
                    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, c);
                });
            });
            ui.add_space(6.0);
        });

        egui::TopBottomPanel::bottom("foot").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let col = match self.status.0 { Level::Info => ui.visuals().text_color(), Level::Ok => theme::OK, Level::Warn => theme::WARN, Level::Error => Color32::from_rgb(0xe5, 0x53, 0x4b) };
                if self.busy { ui.spinner(); }
                ui.label(RichText::new(&self.status.1).color(col));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = self.saved.is_some() && !self.busy;
                    let apply = egui::Button::new(RichText::new("Appliquer").strong().color(Color32::WHITE)).fill(theme::ACCENT);
                    if ui.add_enabled(can && dirty, apply).clicked() {
                        if let (Some(o), Some(n)) = (self.saved.clone(), self.edit.clone()) { self.send(Cmd::Apply { old: o, new: n, flash: self.flash_ok }); self.set(Level::Info, "Écriture en cours…"); }
                    }
                    if ui.add_enabled(can && dirty, egui::Button::new("Annuler")).clicked() { self.edit = self.saved.clone(); }
                    if dirty { ui.label(RichText::new("Modifications non appliquées").color(theme::WARN)); }
                });
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(mut cfg) = self.edit.take() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.image((self.logo.id(), egui::vec2(140.0, 140.0)));
                    ui.add_space(10.0);
                    if self.need_access {
                        ui.label("La souris est détectée mais l'accès est bloqué (première utilisation).");
                        if hidraw::in_flatpak() {
                            ui.label("Version Flatpak : installe la règle udev depuis un terminal (une seule fois) :");
                            let mut cmd = format!("printf '%s' '{}' | sudo tee {} && sudo udevadm control --reload && sudo udevadm trigger", hidraw::UDEV_RULE.replace('\'', ""), hidraw::RULE_PATH);
                            ui.add(egui::TextEdit::multiline(&mut cmd).desired_rows(4).desired_width(560.0));
                        } else if ui.button("Autoriser l'accès (mot de passe, une seule fois)").clicked() {
                            match hidraw::install_access() { Ok(_) => { self.need_access = false; self.last_try = None; } Err(e) => self.set(Level::Error, e) }
                        }
                    } else { ui.label("Branche la souris : la connexion est automatique."); }
                    if ui.button("Recherche de secours (diagnostic)").clicked() { self.diag = Some(hidraw::diagnose()); self.last_try = None; }
                    if let Some(d) = &self.diag { ui.add_space(6.0); ui.label(d.as_str()); ui.label(RichText::new("Si la souris apparaît ici, branche-la et ouvre Restauration. Sinon, le système ne la voit pas.").color(Color32::from_gray(150))); }
                    if ui.button("Mode simulation").clicked() { self.send(Cmd::Connect { sim: true }); }
                });
                return;
            };
            ui.horizontal(|ui| {
                for (t, n) in [(Tab::Lighting, "Éclairage"), (Tab::Dpi, "DPI"), (Tab::Buttons, "Boutons"), (Tab::Performance, "Performance"), (Tab::Diagnostic, "Diagnostic")] {
                    ui.selectable_value(&mut self.tab, t, n);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::Lighting => ui::lighting(ui, &mut cfg),
                Tab::Dpi => ui::dpi(ui, &mut cfg),
                Tab::Buttons => { let mut pr = self.profile.unwrap_or(cfg.active_profile); ui::buttons(ui, &mut cfg, &mut pr, self.flash_ok); self.profile = Some(pr); }
                Tab::Performance => ui::performance(ui, &mut cfg, self.flash_ok),
                Tab::Diagnostic => {
                    theme::card(ui, "Souris", |ui| {
                        ui.label(format!("Produit : {}", cfg.product));
                        ui.label(format!("Nœud : {}", self.node));
                        if cfg.checksum_ok { ui.colored_label(theme::OK, "Checksum de configuration valide"); }
                        else { ui.colored_label(theme::WARN, "Checksum invalide (corrigé au prochain « Appliquer »)"); }
                        ui.label(RichText::new(format!("Sauvegardes : {}", worker::data_dir().display())).color(Color32::from_gray(150)));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!self.busy, egui::Button::new("Relire")).clicked() { self.send(Cmd::Refresh); }
                            if ui.add_enabled(!self.busy, egui::Button::new("Exporter la config complète")).clicked() { self.send(Cmd::Export); }
                        });
                    });
                    theme::card(ui, "Écriture permanente (avancé)", |ui| {
                        ui.checkbox(&mut self.flash_ok, "Autoriser boutons et taux d'interrogation");
                        ui.label(RichText::new("Ces réglages s'écrivent dans la mémoire permanente de la souris. Une écriture invalide peut la rendre inutilisable : désactivé par défaut. Éclairage et DPI n'en ont pas besoin.").color(Color32::from_gray(150)));
                    });
                    theme::card(ui, "Restauration", |ui| {
                        ui.label("Réécrit toute la mémoire de la souris (comme le logiciel d'origine) puis la redémarre. Une sauvegarde de l'état actuel est faite avant.");
                        ui.checkbox(&mut self.confirm_restore, "Je confirme vouloir restaurer");
                        let ok = self.confirm_restore && !self.busy;
                        if ui.add_enabled(ok, egui::Button::new("Réglages d'usine")).clicked() {
                            self.send(Cmd::Restore { img: xblades_proto::FACTORY_IMAGE.to_vec() }); self.confirm_restore = false;
                        }
                        let list = worker::backups();
                        if list.is_empty() { ui.label(RichText::new("Aucune sauvegarde trouvée.").color(Color32::from_gray(150))); }
                        for p in list.iter().take(6) {
                            ui.horizontal(|ui| {
                                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                ui.label(&name);
                                if ui.add_enabled(ok, egui::Button::new("Restaurer")).clicked() {
                                    match std::fs::read(p) { Ok(img) => { self.send(Cmd::Restore { img }); self.confirm_restore = false; } Err(e) => self.set(Level::Error, e.to_string()) }
                                }
                            });
                        }
                    });
                    if !hidraw::in_flatpak() { theme::card(ui, "Accès", |ui| {
                        if ui.button("Désinstaller l'accès (règle udev)").clicked() {
                            match hidraw::remove_access() { Ok(_) => { self.set(Level::Ok, "Règle supprimée"); self.saved = None; self.need_access = true; } Err(e) => self.set(Level::Error, e) }
                        }
                    }); }
                }
            });
            if self.edit.is_none() { self.edit = Some(cfg); }
        });
    }
}

fn main() -> eframe::Result {
    let icon = image::load_from_memory(LOGO).ok().map(|i| { let i = i.to_rgba8(); egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() } });
    let mut vp = egui::ViewportBuilder::default().with_inner_size([600.0, 640.0]).with_min_inner_size([520.0, 480.0]).with_title("XBlades Linux").with_app_id("xblades");
    if let Some(i) = icon { vp = vp.with_icon(std::sync::Arc::new(i)); }
    eframe::run_native("XBlades Linux", eframe::NativeOptions { viewport: vp, ..Default::default() }, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

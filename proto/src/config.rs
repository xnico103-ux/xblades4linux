//! Modèle de configuration de haut niveau, décodé depuis l'image mémoire de la souris.
use crate::{regs, Error};

pub const GEARS: usize = 6; // niveaux DPI livrés (0x2f n'est jamais écrit)
pub const PALETTE: usize = 10;
pub const POLL_RATES: [u16; 4] = [125, 250, 500, 1000];
/// Zone relue après chaque écriture (plus courte = moins de risque de saturer la puce).
pub const CORE_LEN: usize = 0x160; // réglages + 3 profils de boutons
/// Index de palier -> DPI. Seuls les paliers 3, 6, 8, 9, 10, 11 sont confirmés (projet de référence).
pub const DPI_TABLE: [u32; 16] = [100, 200, 400, 800, 1000, 1200, 1600, 2000, 2400, 3200, 4800, 6400, 8000, 10000, 12000, 16000];
pub const CONFIRMED_STEPS: [u8; 6] = [3, 6, 8, 9, 10, 11];

/// Seul l'effet 1 est confirmé sur la souris : il fait un fondu dans la couleur du niveau DPI.
/// (L'appli Windows nomme les modes DPI Color, Colorful Light, Rainbow, Flow Light, Waltz Light, 4-Seasons.)
pub fn effect_name(e: u8) -> String {
    match e {
        0 => "Effet 1 : fondu, couleur du niveau DPI".into(),
        n => format!("Effet {} (non vérifié)", n + 1),
    }
}

pub const PROFILES: usize = 3;
pub const SLOTS: usize = 12;
pub const PROFILE_BASES: [u16; PROFILES] = [0xc0, 0xf1, 0x122];
pub const PROFILE_STRIDE: u16 = 0x31; // 12 x 4 octets + checksum

/// Action d'un bouton (4 octets : type + 3 paramètres). Format déduit de dumps réels
/// (`01 xx` bouton souris, `03 00 lo hi` touche multimédia, `02 mod touche` raccourci clavier, `09` DPI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Disabled,
    Mouse(u8),
    Dpi,
    Consumer(u16),
    Key { modifier: u8, key: u8 },
    /// Action non décodée : conservée telle quelle.
    Raw([u8; 4]),
}
pub const MOUSE_BUTTONS: [(u8, &str); 5] = [(1, "Clic gauche"), (2, "Clic droit"), (4, "Clic molette"), (8, "Précédent (latéral)"), (0x10, "Suivant (latéral)")];
pub const CONSUMER: [(u16, &str); 7] = [(0xe9, "Volume +"), (0xea, "Volume −"), (0xe2, "Muet"), (0xcd, "Lecture / Pause"), (0xb5, "Piste suivante"), (0xb6, "Piste précédente"), (0xb7, "Stop")];

impl Action {
    pub fn decode(b: [u8; 4]) -> Action {
        match b {
            [0, 0, 0, 0] => Action::Disabled,
            [1, m, 0, 0] if MOUSE_BUTTONS.iter().any(|(k, _)| *k == m) => Action::Mouse(m),
            [9, 0, 0, 0] => Action::Dpi,
            [3, 0, lo, hi] => Action::Consumer(lo as u16 | (hi as u16) << 8),
            [2, modifier, key, 0] if key != 0 => Action::Key { modifier, key },
            _ => Action::Raw(b),
        }
    }
    pub fn encode(&self) -> [u8; 4] {
        match *self {
            Action::Disabled => [0; 4],
            Action::Mouse(m) => [1, m, 0, 0],
            Action::Dpi => [9, 0, 0, 0],
            Action::Consumer(u) => [3, 0, u as u8, (u >> 8) as u8],
            Action::Key { modifier, key } => [2, modifier, key, 0],
            Action::Raw(b) => b,
        }
    }
    pub fn label(&self) -> String {
        match *self {
            Action::Disabled => "Désactivé".into(),
            Action::Mouse(m) => MOUSE_BUTTONS.iter().find(|(k, _)| *k == m).map_or("?", |(_, n)| n).into(),
            Action::Dpi => "Changer de DPI".into(),
            Action::Consumer(u) => CONSUMER.iter().find(|(k, _)| *k == u).map_or(format!("Touche multimédia 0x{u:04x}"), |(_, n)| (*n).into()),
            Action::Key { modifier, key } => {
                let mut s = String::new();
                for (bit, n) in [(1u8, "Ctrl+"), (2, "Maj+"), (4, "Alt+"), (8, "Win+")] { if modifier & bit != 0 { s += n; } }
                format!("Raccourci {s}{}", key_name(key))
            }
            Action::Raw(b) => format!("Action spéciale {:02x} {:02x} {:02x} {:02x} (conservée)", b[0], b[1], b[2], b[3]),
        }
    }
}

/// Nom d'une touche (codes d'usage clavier HID).
pub fn key_name(k: u8) -> String {
    match k {
        0x04..=0x1d => ((b'A' + k - 4) as char).to_string(),
        0x1e..=0x26 => ((b'1' + k - 0x1e) as char).to_string(),
        0x27 => "0".into(),
        0x28 => "Entrée".into(), 0x29 => "Échap".into(), 0x2a => "Retour arrière".into(), 0x2b => "Tab".into(), 0x2c => "Espace".into(),
        0x3a..=0x45 => format!("F{}", k - 0x39),
        0x4a => "Début".into(), 0x4b => "Page ↑".into(), 0x4c => "Suppr".into(), 0x4d => "Fin".into(), 0x4e => "Page ↓".into(),
        0x4f => "→".into(), 0x50 => "←".into(), 0x51 => "↓".into(), 0x52 => "↑".into(),
        _ => format!("0x{k:02x}"),
    }
}
pub const KEYS: &[u8] = &[
    0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
    0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c,
    0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52,
];
/// Boutons réellement utilisés (les suivants sont vides sur cette souris).
pub const USED_SLOTS: usize = 9;
/// Position physique de chaque emplacement. Le manuel montre une souris symétrique (2 boutons latéraux de
/// chaque côté) ; l'ordre est déduit du profil droitier (0xF1) et de son miroir gaucher (0xC0), où les
/// emplacements 1/4 et 5/8 échangent leurs rôles (précédent/suivant <-> volume −/+).
pub const SLOT_NAMES: [&str; USED_SLOTS] = [
    "Bouton gauche", "Latéral gauche · arrière", "Bouton dessous (profil ?)",
    "Bouton droit", "Latéral gauche · avant", "Latéral droit · avant", "Molette (clic)", "Bouton DPI", "Latéral droit · arrière",
];
/// Ordre d'affichage : les quatre boutons latéraux d'abord.
pub const SLOT_ORDER: [usize; USED_SLOTS] = [4, 1, 5, 8, 0, 3, 6, 7, 2];

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub profiles: [[[u8; 4]; SLOTS]; PROFILES],
    /// Profil de boutons actif (déduit de 0xA4, à confirmer).
    pub active_profile: usize,
    pub gear_count: u8,
    pub steps: [u8; GEARS],
    pub current_gear: u8,
    /// Couleur de chaque niveau DPI : c'est elle qu'affiche l'effet « Fixe ».
    pub gear_colors: [[u8; 3]; GEARS],
    /// Palette de 10 couleurs utilisée par les effets animés.
    pub palette: [[u8; 3]; PALETTE],
    pub effect: u8,
    /// Éclairage fixe (octet 0xA4 quartet bas = 2) au lieu du fondu (1). Confirmé par le décompilage de Btn_Fixed.
    pub fixed: bool,
    /// Quartet haut de 0xA4, conservé tel quel.
    pub led_level: u8,
    /// Octet 0xAA en mode fondu (valeur mémorisée pour pouvoir revenir du mode fixe).
    pub fade_aa: u8,
    /// Octet 0x38 (durée du fondu, 5..50 dans le logiciel d'origine ; plus petit = fondu plus court).
    pub breath_speed: u8,
    pub polling: u16,
    pub product: String,
    pub checksum_ok: bool,
}

impl Config {
    pub fn from_mem(m: &[u8]) -> Result<Self, Error> {
        if m.len() < CORE_LEN {
            return Err(Error::Refused("image de configuration trop courte"));
        }
        let mut c = Config {
            gear_count: (m[regs::DPI_COUNT as usize].saturating_add(1)).clamp(1, GEARS as u8),
            steps: [0; GEARS],
            current_gear: 0,
            gear_colors: [[0; 3]; GEARS],
            palette: [[0; 3]; PALETTE],
            profiles: [[[0; 4]; SLOTS]; PROFILES],
            breath_speed: m[regs::BREATH_SPEED as usize],
            fixed: m[regs::LED_TYPE as usize] & 0x0f == 2,
            led_level: m[regs::LED_TYPE as usize] >> 4,
            fade_aa: if m[0xaa] != 0 { m[0xaa] } else { 0x18 },
            active_profile: 1,
            effect: (m[regs::LED_MODE as usize] & 0x0f).saturating_sub(1).min(7),
            polling: POLL_RATES[((m[regs::REPORT_RATE as usize] >> 4) & 3) as usize],
            product: String::from_utf8_lossy(&m[0x6f..0x80].iter().copied().take_while(|b| *b != 0).collect::<Vec<_>>()).into(),
            checksum_ok: regs::section_checksum(&m[..regs::CHECKSUM as usize]) == m[regs::CHECKSUM as usize],
        };
        for (i, s) in c.steps.iter_mut().enumerate() {
            let b = m[regs::DPI_STEPS as usize + i / 2];
            *s = if i % 2 == 0 { b & 0xf } else { b >> 4 };
        }
        c.current_gear = m[regs::CUR_DPI as usize].min(c.gear_count - 1);
        for i in 0..GEARS { c.gear_colors[i].copy_from_slice(&m[0x57 + 3 * i..0x5a + 3 * i]); }
        for i in 0..PALETTE { c.palette[i].copy_from_slice(&m[0x39 + 3 * i..0x3c + 3 * i]); }
        for (p, base) in PROFILE_BASES.iter().enumerate() {
            for s in 0..SLOTS { let a = *base as usize + 4 * s; c.profiles[p][s].copy_from_slice(&m[a..a + 4]); }
        }
        Ok(c)
    }

    /// Octets 0xA4, 0xA8, 0xA9, 0xAA correspondant au mode (même valeurs que le logiciel d'origine).
    pub fn led_bytes(&self) -> [u8; 4] {
        let a4 = (self.led_level << 4) | if self.fixed { 2 } else { 1 };
        if self.fixed { [a4, 0, 0, 0] } else { [a4, 0x0f, 0, self.fade_aa] }
    }
    /// Vitesse du fondu 1..10 (10 = octet 5, formule du logiciel d'origine : octet = 55 - 5 x niveau).
    pub fn speed_level(&self) -> u8 { (55u8.saturating_sub(self.breath_speed) / 5).clamp(1, 10) }
    pub fn set_speed_level(&mut self, l: u8) { self.breath_speed = 55 - 5 * l.clamp(1, 10); }

    /// Reporte le taux d'interrogation et les profils de boutons dans une image mémoire (sans checksums).
    pub fn patch_image(&self, img: &mut [u8]) {
        let i = POLL_RATES.iter().position(|r| *r == self.polling).unwrap_or(2) as u8;
        img[regs::BREATH_SPEED as usize] = self.breath_speed;
        let [a4, a8, a9, aa] = self.led_bytes();
        img[regs::LED_TYPE as usize] = a4;
        img[regs::LED_AUX as usize..regs::LED_AUX as usize + 3].copy_from_slice(&[a8, a9, aa]);
        img[regs::REPORT_RATE as usize] = (img[regs::REPORT_RATE as usize] & !0x30) | (i << 4);
        for (p, base) in PROFILE_BASES.iter().enumerate() {
            for s in 0..SLOTS { let a = *base as usize + 4 * s; img[a..a + 4].copy_from_slice(&self.profiles[p][s]); }
        }
    }
    pub fn dpi(&self, gear: usize) -> u32 { DPI_TABLE[self.steps[gear] as usize & 15] }
    /// Un profil modifié doit garder un clic gauche, sinon la souris serait inutilisable.
    pub fn validate_against(&self, old: &Config) -> Result<(), String> {
        for p in 0..PROFILES {
            if self.profiles[p] != old.profiles[p]
                && !self.profiles[p].iter().any(|s| Action::decode(*s) == Action::Mouse(1)) {
                return Err(format!("Profil {} : il doit rester un « Clic gauche »", p + 1));
            }
        }
        Ok(())
    }
    pub fn set_all_gear_colors(&mut self, rgb: [u8; 3]) { self.gear_colors = [rgb; GEARS]; }
    /// Une seule couleur partout : tous les niveaux (effet Fixe) et toute la palette (autres effets).
    pub fn set_uniform_color(&mut self, rgb: [u8; 3]) { self.gear_colors = [rgb; GEARS]; self.palette = [rgb; PALETTE]; }
    /// Couleur commune si tout est uniforme, sinon `None`.
    pub fn uniform_color(&self) -> Option<[u8; 3]> {
        let c = self.gear_colors[0];
        (self.gear_colors.iter().chain(self.palette.iter()).all(|x| *x == c)).then_some(c)
    }
}

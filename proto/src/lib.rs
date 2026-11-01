//! Protocole HID de la souris EMPIRE GAMING X-Blades (Sunplus SPCP6651B, 1bcf:08b8).
//!
//! Déduit par analyse statique du logiciel Windows (voir docs/PROTOCOL.md).
//! Tout est transporté par des rapports *feature* de 8 octets :
//!   - ID 0x40 : commande (SET_FEATURE)
//!   - ID 0x41 : réponse, 7 octets de données (GET_FEATURE)

pub mod config;
pub mod hidraw;
pub mod regs;
pub mod sim;

use std::io;
use std::time::Duration;

pub const VID: u16 = 0x1bcf;
pub const PID: u16 = 0x08b8;
pub const REPORT_LEN: usize = 8;
pub const CMD_ID: u8 = 0x40;
pub const DATA_ID: u8 = 0x41;
pub const CMD_WRITE: u8 = 0x05;
pub const CMD_READ: u8 = 0x09;
pub const CMD_BLOCK_WRITE: u8 = 0x07; // écriture d'un bloc : 40 07 AH AL LH LL puis ⌈L/7⌉ rapports 41 de 7 octets
pub const CMD_HELLO: u8 = 0x0d;
pub const CMD_RELOAD: u8 = 0x0e; // rechargement du firmware (la souris se reconnecte)
pub const MEM_SIZE: usize = 0x800;
/// Image d'usine SPCP6651B (ressource SPCP6651BDATA_CFG1 du logiciel d'origine ; CFG1, 2 et 3 sont identiques).
pub const FACTORY_IMAGE: &[u8] = include_bytes!("../data/factory-6651b.bin");

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NotFound,
    BadHandshake([u8; 7]),
    Refused(&'static str),
    VerifyFailed { addr: u16, wrote: Vec<u8>, read: Vec<u8> },
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self { Error::Io(e) }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "erreur E/S : {e}"),
            Error::NotFound => write!(f, "souris 1bcf:08b8 introuvable (droits udev ?)"),
            Error::BadHandshake(b) => write!(f, "réponse d'identification inattendue : {b:02x?}"),
            Error::Refused(why) => write!(f, "écriture refusée par les garde-fous : {why}"),
            Error::VerifyFailed { addr, wrote, read } => {
                write!(f, "vérification échouée @0x{addr:04x} : écrit {wrote:02x?}, relu {read:02x?}")
            }
        }
    }
}
impl std::error::Error for Error {}

/// Transport de rapports feature (hidraw réel ou simulateur).
pub trait Transport {
    fn set_feature(&mut self, report: &[u8; REPORT_LEN]) -> io::Result<()>;
    fn get_feature(&mut self, id: u8) -> io::Result<[u8; REPORT_LEN]>;
}

/// Résultat d'une application de configuration.
#[derive(Debug, Default)]
pub struct Applied {
    pub writes: usize,
    /// Vrai si le firmware a été rechargé : la souris se reconnecte.
    pub reloaded: bool,
}

pub struct Device<T: Transport> {
    t: T,
    pub firmware: u8,
    /// Journal des paquets (diagnostic, --dry-run).
    pub log: Vec<String>,
    pub dry_run: bool,
}

use config::{Config, GEARS, PALETTE, PROFILES, PROFILE_BASES, SLOTS};

impl<T: Transport> Device<T> {
    /// Ouvre la session : HELLO (0x0d) puis vérifie la signature 0x3a.
    pub fn connect(t: T) -> Result<Self, Error> {
        let mut d = Device { t, firmware: 0, log: vec![], dry_run: false };
        d.send(CMD_HELLO, [0; 6])?;
        let r = d.read_data()?;
        if r[1] != 0x3a { return Err(Error::BadHandshake(r)); }
        d.firmware = r[0];
        Ok(d)
    }

    fn send(&mut self, cmd: u8, p: [u8; 6]) -> io::Result<()> {
        let pkt = [CMD_ID, cmd, p[0], p[1], p[2], p[3], p[4], p[5]];
        self.log.push(format!("SET {pkt:02x?}"));
        if self.dry_run && matches!(cmd, CMD_WRITE | CMD_RELOAD | CMD_BLOCK_WRITE) { return Ok(()); }
        let r = self.t.set_feature(&pkt);
        std::thread::sleep(Duration::from_millis(12)); // même rythme que le pilote d'origine
        r
    }

    fn read_data(&mut self) -> io::Result<[u8; 7]> {
        let r = self.t.get_feature(DATA_ID)?;
        std::thread::sleep(Duration::from_micros(2500));
        let mut o = [0u8; 7];
        o.copy_from_slice(&r[1..]);
        Ok(o)
    }

    /// Lecture mémoire : un flux de ⌈len/7⌉ rapports qu'il ne faut JAMAIS abandonner à moitié
    /// (cela bloque le canal de configuration jusqu'au débranchement).
    pub fn read_mem(&mut self, addr: u16, len: usize) -> Result<Vec<u8>, Error> {
        if len == 0 || addr as usize + len > MEM_SIZE { return Err(Error::Refused("plage hors 0..0x800")); }
        self.send(CMD_READ, [(addr >> 8) as u8, addr as u8, (len >> 8) as u8, len as u8, 0, 0])?;
        let mut out = Vec::with_capacity(len + 7);
        while out.len() < len { out.extend_from_slice(&self.read_data()?); }
        out.truncate(len);
        Ok(out)
    }

    pub fn dump(&mut self) -> Result<Vec<u8>, Error> { self.read_mem(0, MEM_SIZE) }
    pub fn read_core(&mut self) -> Result<Vec<u8>, Error> { self.read_mem(0, config::CORE_LEN) }
    pub fn read_config(&mut self) -> Result<Config, Error> { Config::from_mem(&self.read_core()?) }

    /// Écriture brute de 1 à 3 octets (liste blanche) ; pas de relecture ici.
    pub fn write_bytes(&mut self, addr: u16, data: &[u8]) -> Result<(), Error> {
        if data.is_empty() || data.len() > 3 { return Err(Error::Refused("1 à 3 octets par paquet")); }
        regs::check_writable(addr, data.len())?;
        let mut p = [(addr >> 8) as u8, addr as u8, 0, 0, 0, data.len() as u8];
        p[2..2 + data.len()].copy_from_slice(data);
        self.send(CMD_WRITE, p)?;
        Ok(())
    }

    /// Écrit, attend 900 ms puis relit une seule fois (relire trop souvent sature la puce).
    pub fn write_verified(&mut self, addr: u16, data: &[u8]) -> Result<(), Error> {
        self.write_bytes(addr, data)?;
        if self.dry_run { return Ok(()); }
        std::thread::sleep(Duration::from_millis(900));
        let back = self.read_mem(addr, data.len())?;
        if back == data { Ok(()) } else { Err(Error::VerifyFailed { addr, wrote: data.to_vec(), read: back }) }
    }

    /// Écrit l'image complète de 2 Kio, comme le logiciel d'origine (commande 0x07, flux de rapports 0x41).
    /// L'image doit venir d'une relecture de la souris : VID/PID et checksums sont contrôlés avant envoi.
    pub fn write_image(&mut self, img: &[u8]) -> Result<(), Error> {
        if img.len() != MEM_SIZE { return Err(Error::Refused("l'image doit faire 0x800 octets")); }
        if img[..4] != [0x1b, 0xcf, 0x08, 0xb8] { return Err(Error::Refused("VID/PID de l'image inattendus")); }
        if !regs::image_checksums_ok(img) { return Err(Error::Refused("checksums de l'image invalides")); }
        self.send(CMD_BLOCK_WRITE, [0, 0, (MEM_SIZE >> 8) as u8, MEM_SIZE as u8, 0, 0])?;
        for chunk in img.chunks(7) {
            let mut pkt = [0xffu8; REPORT_LEN]; // dernier paquet complété par 0xFF, comme l'original
            pkt[0] = DATA_ID;
            pkt[1..1 + chunk.len()].copy_from_slice(chunk);
            self.log.push(format!("SET {pkt:02x?}"));
            if self.dry_run { continue; }
            self.t.set_feature(&pkt)?;
            std::thread::sleep(Duration::from_millis(12));
        }
        Ok(())
    }

    /// Restaure une image complète (sauvegarde ou usine) puis redémarre la souris.
    pub fn restore_image(&mut self, img: &[u8]) -> Result<(), Error> {
        self.write_image(img)?;
        std::thread::sleep(Duration::from_millis(300));
        self.reload()
    }

    /// Commande 0x0E : recharge le firmware, la souris se reconnecte.
    pub fn reload(&mut self) -> Result<(), Error> {
        self.send(CMD_RELOAD, [0; 6])?;
        std::thread::sleep(Duration::from_millis(500));
        Ok(())
    }

    /// Recalcule le checksum 0xA3 (la commande d'écriture ne le met pas à jour).
    pub fn fix_checksum(&mut self) -> Result<u8, Error> {
        let c = regs::section_checksum(&self.read_mem(0, regs::CHECKSUM as usize)?);
        self.write_bytes(regs::CHECKSUM, &[c])?;
        Ok(c)
    }

    // ---- réglages unitaires --------------------------------------------------------------

    pub fn set_gear_color(&mut self, gear: usize, rgb: [u8; 3]) -> Result<(), Error> {
        if gear >= GEARS { return Err(Error::Refused("niveau 0..5")); }
        self.write_bytes(regs::GEAR_COLORS + 3 * gear as u16, &rgb)
    }
    pub fn set_palette_color(&mut self, slot: usize, rgb: [u8; 3]) -> Result<(), Error> {
        if slot >= PALETTE { return Err(Error::Refused("palette 0..9")); }
        self.write_bytes(regs::PALETTE + 3 * slot as u16, &rgb)
    }
    pub fn set_gear_count(&mut self, n: u8) -> Result<(), Error> {
        if !(1..=GEARS as u8).contains(&n) { return Err(Error::Refused("1..6 niveaux")); }
        self.write_bytes(regs::DPI_COUNT, &[n - 1])
    }
    /// Écrit les paliers (3 octets) ; ils ne sont pris en compte qu'après `select_gear`.
    pub fn set_gear_steps(&mut self, steps: &[u8; GEARS]) -> Result<(), Error> {
        if steps.iter().any(|s| *s > 15) { return Err(Error::Refused("palier 0..15")); }
        for i in 0..GEARS / 2 {
            self.write_bytes(regs::DPI_STEPS + i as u16, &[(steps[2 * i + 1] << 4) | steps[2 * i]])?;
        }
        Ok(())
    }
    /// Sélectionne le niveau actif. Avec `bump`, passe d'abord par un voisin : le firmware ne relit
    /// les paliers qu'au changement de niveau.
    pub fn select_gear(&mut self, gear: u8, bump: bool) -> Result<(), Error> {
        if gear as usize >= GEARS { return Err(Error::Refused("niveau 0..5")); }
        if bump {
            self.write_bytes(regs::CUR_DPI, &[if gear == 0 { 1 } else { gear - 1 }])?;
            std::thread::sleep(Duration::from_millis(100));
        }
        self.write_bytes(regs::CUR_DPI, &[gear])
    }
    /// Écrit l'action d'un bouton (3 octets puis 1 octet). Pensez à `fix_profile_checksum`.
    pub fn set_slot(&mut self, profile: usize, slot: usize, b: [u8; 4]) -> Result<(), Error> {
        if profile >= PROFILES || slot >= SLOTS { return Err(Error::Refused("profil 0..2, bouton 0..11")); }
        let a = PROFILE_BASES[profile] + 4 * slot as u16;
        self.write_bytes(a, &b[..3])?;
        self.write_bytes(a + 3, &b[3..])
    }
    /// Checksum d'un profil de boutons : somme des 48 octets XOR 0x5a, stocké juste après.
    pub fn fix_profile_checksum(&mut self, profile: usize) -> Result<u8, Error> {
        if profile >= PROFILES { return Err(Error::Refused("profil 0..2")); }
        let base = PROFILE_BASES[profile];
        let c = regs::section_checksum(&self.read_mem(base, 0x30)?);
        self.write_bytes(base + 0x30, &[c])?;
        Ok(c)
    }
    /// Effet 0..7 : octet 0xA5 = quartet haut conservé | (effet + 1), comme le logiciel d'origine.
    pub fn set_led_effect(&mut self, effect: u8) -> Result<(), Error> {
        if effect > 7 { return Err(Error::Refused("effet 0..7")); }
        let cur = self.read_mem(regs::LED_MODE, 1)?[0];
        let hi = if cur & 0xf0 == 0 { 0x80 } else { cur & 0xf0 };
        self.write_bytes(regs::LED_MODE, &[hi | (effect + 1)])
    }
    /// Luminosité basse du fondu : 255 = identique à la haute (0x00FF) donc plus de fondu.
    /// Bascule fixe / fondu : mêmes écritures directes que le logiciel d'origine (0xA4, puis 0xA8..0xAA).
    pub fn set_led_type(&mut self, b: [u8; 4]) -> Result<(), Error> {
        self.write_bytes(regs::LED_TYPE, &[b[0]])?;
        self.write_bytes(regs::LED_AUX, &b[1..])
    }
    /// Bits 4-5 de 0x04 uniquement ; à suivre d'un `reload`.
    pub fn set_polling(&mut self, hz: u16) -> Result<(), Error> {
        let i = config::POLL_RATES.iter().position(|r| *r == hz).ok_or(Error::Refused("125, 250, 500 ou 1000"))? as u8;
        let r4 = self.read_mem(regs::REPORT_RATE, 1)?[0];
        self.write_bytes(regs::REPORT_RATE, &[(r4 & !0x30) | (i << 4)])
    }

    /// Applique uniquement la différence `old` -> `new`, recalcule le checksum, recharge si besoin.
    pub fn apply(&mut self, old: &Config, new: &Config) -> Result<Applied, Error> { self.apply_opt(old, new, true) }

    /// `flash = false` : n'écrit jamais l'image permanente (0x07) ; refuse boutons et taux d'interrogation.
    pub fn apply_opt(&mut self, old: &Config, new: &Config, flash: bool) -> Result<Applied, Error> {
        if !flash && (new.polling != old.polling || (0..PROFILES).any(|p| new.profiles[p] != old.profiles[p])) {
            return Err(Error::Refused("boutons et taux d'interrogation demandent le mode avancé (écriture permanente)"));
        }
        new.validate_against(old).map_err(|_| Error::Refused("un profil de boutons doit garder un clic gauche"))?;
        let mut r = Applied::default();
        // Changer d'effet peut faire perdre les couleurs : on les réécrit alors toutes, après l'effet.
        let effect_changed = new.effect != old.effect;
        if effect_changed {
            self.set_led_effect(new.effect)?;
            r.writes += 1;
            std::thread::sleep(Duration::from_millis(150));
        }
        for i in (0..PALETTE).filter(|i| effect_changed || new.palette[*i] != old.palette[*i]) { self.set_palette_color(i, new.palette[i])?; r.writes += 1; }
        for i in (0..GEARS).filter(|i| effect_changed || new.gear_colors[*i] != old.gear_colors[*i]) { self.set_gear_color(i, new.gear_colors[i])?; r.writes += 1; }
        if new.gear_count != old.gear_count { self.set_gear_count(new.gear_count)?; r.writes += 1; }
        if new.led_bytes() != old.led_bytes() { self.set_led_type(new.led_bytes())?; r.writes += 1; }
        let steps_changed = new.steps != old.steps;
        if steps_changed { self.set_gear_steps(&new.steps)?; r.writes += 1; }
        if steps_changed || new.current_gear != old.current_gear {
            self.select_gear(new.current_gear.min(new.gear_count - 1), steps_changed)?;
            r.writes += 1;
        }
        // Boutons et taux d'interrogation vivent dans la mémoire flash : le logiciel d'origine
        // réécrit toute l'image (0x07) puis recharge. On relit l'image (qui contient déjà les
        // réglages « live » ci-dessus), on y reporte ces deux réglages et on la réécrit.
        let persist = flash && (new.polling != old.polling || new.led_bytes() != old.led_bytes() || new.breath_speed != old.breath_speed || (0..PROFILES).any(|p| new.profiles[p] != old.profiles[p]));
        if persist {
            let mut img = self.read_mem(0, MEM_SIZE)?;
            let before = img.clone();
            new.patch_image(&mut img);
            regs::fix_image_checksums(&mut img);
            regs::image_diff_allowed(&before, &img)?;
            self.write_image(&img)?;
            r.writes += 1;
            std::thread::sleep(Duration::from_millis(300));
            self.reload()?;
            r.reloaded = true;
        } else if r.writes > 0 {
            self.fix_checksum()?;
        }
        Ok(r)
    }
}

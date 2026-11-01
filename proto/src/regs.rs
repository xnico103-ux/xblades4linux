//! Carte des registres. Sources : logiciel Windows (analyse statique), recoupé avec le projet public
//! spcp6651-studio et avec un dump réel de la souris (proto/tests/fixtures/dump.bin).
use crate::Error;

pub const REPORT_RATE: u16 = 0x04;  // bits 4-5 : 0=125, 1=250, 2=500, 3=1000 Hz
pub const BREATH_MIN: u16 = 0x30;   // 0x30-0x31 : luminosité basse du fondu (u16 big-endian) ; 0x32-0x33 : haute
pub const BREATH_SPEED: u16 = 0x38; // durée de descente du fondu : le logiciel d'origine écrit 55 − 5×niveau (niveau 1..10, soit 50..5)
pub const DPI_COUNT: u16 = 0x2b;    // nombre de niveaux MOINS 1
pub const DPI_STEPS: u16 = 0x2c;    // 0x2c..0x2e : 2 niveaux/octet (bas = pair), index de table 0..15
pub const PALETTE: u16 = 0x39;      // 10 couleurs RGB (effets animés)
pub const GEAR_COLORS: u16 = 0x57;  // couleur RGB de chaque niveau DPI (effet fixe)
pub const CHECKSUM: u16 = 0xa3;     // somme(0x00..=0xa2) ^ 0x5a
pub const LED_MODE: u16 = 0xa5;     // quartet bas = effet + 1 ; quartet haut conservé (0x8)
pub const LED_TYPE: u16 = 0xa4;     // quartet bas : 1 = fondu, 2 = fixe (décompilé : Btn_Fixed) ; quartet haut = niveau 0..F
pub const LED_AUX: u16 = 0xa8;      // 0xA8..0xAA : 0F 00 xx en fondu, 00 00 00 en fixe
pub const CUR_DPI: u16 = 0xa6;      // niveau DPI actif

/// Liste blanche des écritures. VID/PID, firmware, boutons et checksums de profils sont interdits.
pub fn check_writable(addr: u16, len: usize) -> Result<(), Error> {
    let (a, end) = (addr as usize, addr as usize + len);
    let ok = |lo: usize, hi: usize| a >= lo && end <= hi;
    if ok(0x04, 0x05) || ok(0x2b, 0x2f) || ok(0x30, 0x39) || ok(0x39, 0x6f) || ok(0xc0, 0x153) || ok(0xa3, 0xa4) || ok(0xa4, 0xa5) || ok(0xa5, 0xa7) || ok(0xa7, 0xab) {
        Ok(())
    } else {
        Err(Error::Refused("adresse hors liste blanche"))
    }
}

/// Garde-fou de l'écriture permanente : seuls ces octets peuvent différer de l'image relue.
/// 0x04 taux, 0x38 vitesse, 0xA3 checksum, 0xA4..0xAA éclairage, 0xBF checksum, 0xC0..0x152 profils.
/// Tout le reste (VID/PID, 0x2A, DPI, couleurs, octets inconnus) doit rester tel que la souris l'a rendu.
pub fn image_diff_allowed(before: &[u8], after: &[u8]) -> Result<(), Error> {
    if before.len() != after.len() { return Err(Error::Refused("tailles d'image différentes")); }
    for i in 0..before.len() {
        if before[i] != after[i] && !matches!(i, 0x04 | 0x38 | 0xa3..=0xaa | 0xbf | 0xc0..=0x152) {
            return Err(Error::Refused("l'image modifie un octet non autorisé"));
        }
    }
    Ok(())
}

/// Somme des octets XOR 0x5a.
pub fn section_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b)) ^ 0x5a
}

/// Recalcule tous les checksums d'une image de 0x800 octets : zone 0x00..0xA3 (-> 0xA3),
/// bloc profil 0xA4..0xBE (-> 0xBF) et les trois profils de boutons (-> base + 0x30).
pub fn fix_image_checksums(img: &mut [u8]) {
    img[0xa3] = section_checksum(&img[..0xa3]);
    img[0xbf] = section_checksum(&img[0xa4..0xbf]);
    for b in crate::config::PROFILE_BASES { let b = b as usize; img[b + 0x30] = section_checksum(&img[b..b + 0x30]); }
}
pub fn image_checksums_ok(img: &[u8]) -> bool {
    let mut c = img.to_vec();
    fix_image_checksums(&mut c);
    c[0xa3] == img[0xa3] && c[0xbf] == img[0xbf] && crate::config::PROFILE_BASES.iter().all(|b| c[*b as usize + 0x30] == img[*b as usize + 0x30])
}

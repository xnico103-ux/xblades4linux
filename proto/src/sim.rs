//! Souris simulée (tests et démo sans matériel). Reproduit le protocole déduit.
use crate::*;
use std::collections::VecDeque;

pub struct Sim { pub mem: Vec<u8>, q: VecDeque<u8>, pub sent: Vec<[u8; 8]>, blk: Option<(usize, usize)> }

impl Sim {
    /// Souris simulée à partir d'une image mémoire (ex. un vrai dump).
    pub fn from_image(mem: Vec<u8>) -> Self { Sim { mem, q: VecDeque::new(), sent: vec![], blk: None } }
    /// Démarre sur un vrai dump de souris.
    pub fn new() -> Self { Self::from_image(include_bytes!("../tests/fixtures/dump.bin").to_vec()) }
}
impl Transport for Sim {
    fn set_feature(&mut self, r: &[u8; 8]) -> std::io::Result<()> {
        self.sent.push(*r);
        if r[0] == DATA_ID {
            if let Some((a, left)) = self.blk.as_mut() {
                for b in &r[1..] { if *left > 0 { self.mem[*a] = *b; *a += 1; *left -= 1; } }
                if *left == 0 { self.blk = None; }
            }
            return Ok(());
        }
        match r[1] {
            CMD_BLOCK_WRITE => { self.blk = Some((((r[2] as usize) << 8) | r[3] as usize, ((r[4] as usize) << 8) | r[5] as usize)); }
            CMD_HELLO => { self.q.clear(); self.q.extend([0x41, 0x3a, 0, 0, 0, 0, 0]); }
            CMD_READ => {
                let a = ((r[2] as usize) << 8) | r[3] as usize;
                let n = ((r[4] as usize) << 8) | r[5] as usize;
                self.q.clear();
                let n7 = n.div_ceil(7) * 7;
                for i in 0..n7 { self.q.push_back(*self.mem.get(a + i).unwrap_or(&0)); }
            }
            CMD_WRITE => {
                let a = ((r[2] as usize) << 8) | r[3] as usize;
                for i in 0..r[7] as usize { self.mem[a + i] = r[4 + i]; }
            }
            _ => {}
        }
        Ok(())
    }
    fn get_feature(&mut self, id: u8) -> std::io::Result<[u8; 8]> {
        let mut o = [id, 0, 0, 0, 0, 0, 0, 0];
        for i in 1..8 { if let Some(b) = self.q.pop_front() { o[i] = b; } }
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, Config};
    const DUMP: &[u8] = include_bytes!("../tests/fixtures/dump.bin");

    fn dev() -> Device<Sim> { Device::connect(Sim::from_image(DUMP.to_vec())).unwrap() }

    #[test]
    fn spcp6653b_is_read_only() {
        let mut d = dev();
        assert_eq!(d.chip(), crate::Chip::Spcp6651b);
        d.firmware = 0x20; // version < 0x40 : SPCP6653B
        assert_eq!(d.chip(), crate::Chip::Spcp6653b);
        assert!(d.read_mem(0, 16).is_ok());
        assert!(matches!(d.write_bytes(0x38, &[1]), Err(crate::Error::Refused(_))));
        assert!(matches!(d.write_image(DUMP), Err(crate::Error::Refused(_))));
        assert!(matches!(d.reload(), Err(crate::Error::Refused(_))));
    }

    #[test]
    fn real_dump_decodes() {
        let c = Config::from_mem(DUMP).unwrap();
        assert!(c.checksum_ok);
        assert_eq!((c.gear_count, c.current_gear, c.polling, c.effect), (6, 2, 500, 0));
        assert_eq!(c.steps, [3, 6, 8, 9, 10, 10]);
        assert_eq!(c.dpi(0), 800);
        assert_eq!(c.product, "OE-MO-901");
        assert_eq!(c.gear_colors[0], [255, 0, 0]);
    }
    #[test]
    fn apply_writes_only_diff_and_fixes_checksum() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.set_all_gear_colors([0, 255, 0]);
        new.effect = 1;
        let r = d.apply(&old, &new).unwrap();
        assert!(!r.reloaded);
        let back = d.read_config().unwrap();
        assert_eq!((back.effect, back.gear_colors[3], back.checksum_ok), (1, [0, 255, 0], true));
        assert_eq!(back.palette, old.palette); // intact
    }
    #[test]
    fn effect_change_rewrites_all_colors_and_uniform_color() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        assert_eq!(old.uniform_color(), Some([255, 0, 0]));
        let mut new = old.clone();
        new.effect = 1;
        let r = d.apply(&old, &new).unwrap();
        assert!(r.writes >= 1 + 10 + 6); // effet + toute la palette + tous les niveaux
        let mut blue = d.read_config().unwrap();
        let before = blue.clone();
        blue.set_uniform_color([0, 0, 255]);
        d.apply(&before, &blue).unwrap();
        let back = d.read_config().unwrap();
        assert_eq!(back.uniform_color(), Some([0, 0, 255]));
        assert!(back.checksum_ok);
    }
    #[test]
    fn nothing_to_do_writes_nothing() {
        let mut d = dev();
        let c = d.read_config().unwrap();
        assert_eq!(d.apply(&c, &c).unwrap().writes, 0);
    }
    #[test]
    fn fixed_mode_roundtrip() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        assert!(!old.fixed); // 0xF1 : fondu
        let mut new = old.clone();
        new.fixed = true;
        assert!(d.apply(&old, &new).unwrap().reloaded);
        let img = d.read_mem(0, MEM_SIZE).unwrap();
        assert_eq!(&img[0xa4..0xab], &[0xf2, 0x81, 0x02, 0x04, 0, 0, 0]);
        assert!(regs::image_checksums_ok(&img));
        let back = d.read_config().unwrap();
        assert!(back.fixed);
        let mut again = back.clone();
        again.fixed = false;
        d.apply(&back, &again).unwrap();
        let img = d.read_mem(0, MEM_SIZE).unwrap();
        assert_eq!(&img[0xa4..0xab], &DUMP[0xa4..0xab]); // retour exact au fondu d'origine
    }
    #[test]
    fn speed_level_matches_vendor_formula() {
        let mut c = Config::from_mem(DUMP).unwrap();
        c.set_speed_level(10); assert_eq!(c.breath_speed, 5);
        c.set_speed_level(1); assert_eq!(c.breath_speed, 50);
        c.breath_speed = 8; assert_eq!(c.speed_level(), 9);
    }
    #[test]
    fn polling_reloads() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.polling = 1000;
        assert!(d.apply(&old, &new).unwrap().reloaded);
        assert_eq!(d.read_config().unwrap().polling, 1000);
    }
    #[test]
    fn dpi_steps_roundtrip() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.steps = [1, 2, 3, 4, 5, 6];
        d.apply(&old, &new).unwrap();
        assert_eq!(d.read_config().unwrap().steps, [1, 2, 3, 4, 5, 6]);
    }
    #[test]
    fn actions_roundtrip_on_real_dump() {
        let c = Config::from_mem(DUMP).unwrap();
        for p in c.profiles { for s in p { assert_eq!(Action::decode(s).encode(), s); } }
        assert_eq!(Action::decode(c.profiles[2][5]), Action::Key { modifier: 8, key: 7 }); // Win+D
        assert_eq!(Action::decode(c.profiles[1][1]), Action::Mouse(8));
        assert_eq!(Action::decode(c.profiles[1][5]), Action::Consumer(0xe9));
        assert_eq!(Action::decode(c.profiles[1][7]), Action::Dpi);
    }
    #[test]
    fn button_edit_updates_profile_checksum_only() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.profiles[1][1] = Action::Consumer(0xcd).encode(); // Précédent -> Lecture/Pause
        assert!(d.apply(&old, &new).unwrap().reloaded);
        let img = d.read_mem(0, 0x153).unwrap();
        assert_eq!(&img[0xf1 + 4..0xf1 + 8], &[3, 0, 0xcd, 0]);
        assert_eq!(regs::section_checksum(&img[0xf1..0xf1 + 0x30]), img[0xf1 + 0x30]);
        assert_eq!(&img[0xc0..0xf1], &DUMP[0xc0..0xf1]); // autres profils intacts
        assert_eq!(&img[0x122..0x153], &DUMP[0x122..0x153]);
    }
    #[test]
    fn image_write_changes_only_what_is_asked() {
        assert!(regs::image_checksums_ok(DUMP));
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.profiles[1][4] = Action::Mouse(2).encode();
        new.polling = 1000;
        assert!(d.apply(&old, &new).unwrap().reloaded);
        let img = d.read_mem(0, MEM_SIZE).unwrap();
        assert!(regs::image_checksums_ok(&img));
        let diff: Vec<usize> = (0..MEM_SIZE).filter(|i| img[*i] != DUMP[*i]).collect();
        // octet 0x04 (taux), 1 action (0xf1 + 16 .. +2), checksums 0xa3 / 0xbf? / profil 2 (0x121)
        assert!(diff.iter().all(|i| [0x04, 0xa3, 0xf1 + 16 + 1, 0xf1 + 0x30].contains(i)), "{diff:x?}");
        assert_eq!(img[..4], DUMP[..4]);
        assert_eq!(&img[0x153..], &DUMP[0x153..]);
    }
    #[test]
    fn factory_image_is_valid_and_restorable() {
        assert!(regs::image_checksums_ok(FACTORY_IMAGE));
        assert_eq!(FACTORY_IMAGE.len(), MEM_SIZE);
        let mut d = dev();
        d.restore_image(FACTORY_IMAGE).unwrap();
        assert_eq!(d.read_mem(0, MEM_SIZE).unwrap(), FACTORY_IMAGE);
        let c = Config::from_mem(FACTORY_IMAGE).unwrap();
        assert_eq!((c.polling, c.gear_count), (500, 6));
    }
    #[test]
    fn refuses_profile_without_left_click() {
        let mut d = dev();
        let old = d.read_config().unwrap();
        let mut new = old.clone();
        new.profiles[1][0] = Action::Disabled.encode();
        assert!(new.validate_against(&old).is_err());
        assert!(d.apply(&old, &new).is_err());
    }
    #[test]
    fn guards() {
        let mut d = dev();
        for (a, n) in [(0x00u16, 1usize), (0x2f, 1), (0xb0, 1), (0x153, 1), (0x160, 1), (0x08, 1)] {
            assert!(d.write_bytes(a, &vec![0; n]).is_err(), "0x{a:x} devrait être refusé");
        }
        assert!(d.write_bytes(0x39, &[0; 4]).is_err());
        assert!(d.set_gear_steps(&[16, 0, 0, 0, 0, 0]).is_err());
    }
    #[test]
    fn packet_format() {
        let mut d = dev();
        d.write_bytes(0x39, &[1, 2, 3]).unwrap();
        assert!(d.log.last().unwrap().contains("[40, 05, 00, 39, 01, 02, 03, 03]"));
    }

    #[test]
    fn image_guard_refuses_unknown_bytes() {
        let a = DUMP.to_vec();
        let mut b = a.clone();
        b[0x2a] = 0;
        assert!(regs::image_diff_allowed(&a, &b).is_err());
        let mut c = a.clone();
        c[0x04] ^= 0x10;
        assert!(regs::image_diff_allowed(&a, &c).is_ok());
        assert!(regs::check_writable(0x2a, 1).is_err());
    }
}

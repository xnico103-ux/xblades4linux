//! Transport Linux via /dev/hidrawN (ioctl HIDIOCSFEATURE / HIDIOCGFEATURE), sans libhidapi.
use crate::{Transport, PID, REPORT_LEN, VID};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;

fn ioc(dir: u64, nr: u64, len: usize) -> u64 {
    (dir << 30) | ((len as u64) << 16) | (0x48 << 8) | nr
}
const RW: u64 = 3;

pub struct Hidraw { f: File }

impl Hidraw {
    /// Liste les /dev/hidrawN dont HID_ID correspond à 1bcf:08b8.
    pub fn find() -> Vec<String> {
        let mut v = vec![];
        if let Ok(rd) = std::fs::read_dir("/sys/class/hidraw") {
            for e in rd.flatten() {
                let p = e.path().join("device/uevent");
                if let Ok(s) = std::fs::read_to_string(p) {
                    let want = format!("HID_ID=0003:{:08X}:{:08X}", VID, PID);
                    if s.lines().any(|l| l.eq_ignore_ascii_case(&want)) {
                        v.push(format!("/dev/{}", e.file_name().to_string_lossy()));
                    }
                }
            }
        }
        v.sort();
        v
    }
    pub fn open(path: &str) -> io::Result<Self> {
        Ok(Hidraw { f: OpenOptions::new().read(true).write(true).open(path)? })
    }
}

impl Transport for Hidraw {
    fn set_feature(&mut self, r: &[u8; REPORT_LEN]) -> io::Result<()> {
        let rc = unsafe { libc::ioctl(self.f.as_raw_fd(), ioc(RW, 6, REPORT_LEN) as _, r.as_ptr()) };
        if rc < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }
    fn get_feature(&mut self, id: u8) -> io::Result<[u8; REPORT_LEN]> {
        let mut b = [0u8; REPORT_LEN];
        b[0] = id;
        let rc = unsafe { libc::ioctl(self.f.as_raw_fd(), ioc(RW, 7, REPORT_LEN) as _, b.as_mut_ptr()) };
        if rc < 0 { Err(io::Error::last_os_error()) } else { Ok(b) }
    }
}

/// Essaie chaque nœud hidraw jusqu'à obtenir une session valide.
pub fn connect_any() -> Result<(crate::Device<Hidraw>, String), crate::Error> {
    let nodes = Hidraw::find();
    if nodes.is_empty() { return Err(crate::Error::NotFound); }
    let mut last = crate::Error::NotFound;
    for n in nodes {
        match Hidraw::open(&n).map_err(crate::Error::from).and_then(crate::Device::connect) {
            Ok(d) => return Ok((d, n)),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Diagnostic de secours : liste ce que le système voit (USB et hidraw) pour le fabricant 1bcf, quel que soit le PID.
pub fn diagnose() -> String {
    let mut out = String::new();
    let rd = |p: std::path::PathBuf| std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut n_usb = 0;
    if let Ok(d) = std::fs::read_dir("/sys/bus/usb/devices") {
        for e in d.flatten() {
            let p = e.path();
            if rd(p.join("idVendor")).eq_ignore_ascii_case("1bcf") {
                n_usb += 1;
                out += &format!("USB : {} 1bcf:{} {}\n", e.file_name().to_string_lossy(), rd(p.join("idProduct")), rd(p.join("product")));
            }
        }
    }
    if n_usb == 0 { out += "USB : aucun périphérique 1bcf. Le système ne voit pas la souris (câble, port, ou micrologiciel bloqué).\n"; }
    let mut n_hid = 0;
    if let Ok(d) = std::fs::read_dir("/sys/class/hidraw") {
        for e in d.flatten() {
            let u = rd(e.path().join("device/uevent"));
            if u.lines().any(|l| l.to_uppercase().starts_with("HID_ID=0003:00001BCF")) { n_hid += 1; out += &format!("hidraw : /dev/{}\n", e.file_name().to_string_lossy()); }
        }
    }
    if n_hid == 0 { out += "hidraw : aucun nœud 1bcf.\n"; }
    out
}

pub const UDEV_RULE: &str = "KERNEL==\"hidraw*\", ATTRS{idVendor}==\"1bcf\", ATTRS{idProduct}==\"08b8\", TAG+=\"uaccess\", MODE=\"0660\"\n";
pub const RULE_PATH: &str = "/etc/udev/rules.d/70-xblades.rules";

impl Hidraw {
    /// Vrai si la souris est détectée mais qu'on n'a pas le droit d'ouvrir son nœud hidraw.
    pub fn needs_access() -> bool {
        let n = Self::find();
        !n.is_empty() && n.iter().all(|p| matches!(Hidraw::open(p), Err(e) if e.kind() == io::ErrorKind::PermissionDenied))
    }
}

/// Vrai si l'appli tourne dans un bac à sable Flatpak (pas de pkexec ni d'écriture dans /etc).
pub fn in_flatpak() -> bool { std::path::Path::new("/.flatpak-info").exists() }
const FLATPAK_MSG: &str = "Dans Flatpak, la règle udev doit être installée sur le système (voir les instructions affichées).";

/// Installe la règle udev via polkit (fenêtre de mot de passe graphique), puis recharge udev.
pub fn install_access() -> Result<(), String> {
    if in_flatpak() { return Err(FLATPAK_MSG.into()); }
    use std::process::Command;
    let script = format!(
        "printf '%s' '{}' > {RULE_PATH} && udevadm control --reload && udevadm trigger --subsystem-match=hidraw --action=change",
        UDEV_RULE.replace('\'', "")
    );
    let st = Command::new("pkexec").args(["sh", "-c", &script]).status()
        .map_err(|e| format!("pkexec indisponible : {e}"))?;
    if st.success() { std::thread::sleep(std::time::Duration::from_millis(800)); Ok(()) }
    else { Err("installation annulée ou refusée".into()) }
}

/// Supprime la règle udev installée par `install_access` (demande le mot de passe via polkit).
pub fn remove_access() -> Result<(), String> {
    if in_flatpak() { return Err(FLATPAK_MSG.into()); }
    use std::process::Command;
    let script = format!("rm -f {RULE_PATH} && udevadm control --reload && udevadm trigger --subsystem-match=hidraw --action=change");
    let st = Command::new("pkexec").args(["sh", "-c", &script]).status()
        .map_err(|e| format!("pkexec indisponible : {e}"))?;
    if st.success() { Ok(()) } else { Err("désinstallation annulée ou refusée".into()) }
}

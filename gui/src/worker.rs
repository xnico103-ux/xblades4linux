//! Fil d'exécution dédié à la souris : l'interface ne gèle jamais pendant une écriture.
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use xblades_proto::{config::Config, hidraw, sim::Sim, Applied, Device, Error, Transport};

pub enum Cmd { Connect { sim: bool }, Refresh, Apply { old: Config, new: Config, flash: bool }, Export, Restore { img: Vec<u8> } }

pub enum Event {
    Connected { node: String, cfg: Config },
    Refreshed(Config),
    Applied { cfg: Config, reloaded: bool, writes: usize },
    Exported(PathBuf),
    Restored,
    Failed { msg: String, lost: bool },
}

trait Backend: Send {
    fn node(&self) -> String;
    fn dump(&mut self) -> Result<Vec<u8>, Error>;
    fn read(&mut self) -> Result<Config, Error>;
    fn apply(&mut self, old: &Config, new: &Config, flash: bool) -> Result<Applied, Error>;
    fn restore(&mut self, img: &[u8]) -> Result<(), Error>;
}
struct Dev<T: Transport + Send> { d: Device<T>, node: String }
impl<T: Transport + Send> Backend for Dev<T> {
    fn node(&self) -> String { self.node.clone() }
    fn dump(&mut self) -> Result<Vec<u8>, Error> { self.d.dump() }
    fn read(&mut self) -> Result<Config, Error> { self.d.read_config() }
    fn apply(&mut self, o: &Config, n: &Config, flash: bool) -> Result<Applied, Error> { self.d.apply_opt(o, n, flash) }
    fn restore(&mut self, img: &[u8]) -> Result<(), Error> { self.d.restore_image(img) }
}

pub struct Worker { pub tx: Sender<Cmd>, pub rx: Receiver<Event> }

/// Dossier des sauvegardes : $XDG_DATA_HOME/xblades (ou ~/.local/share/xblades).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("xblades")
}
/// Sauvegardes disponibles (plus anciennes d'abord : la première est celle d'avant toute modification).
pub fn backups() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(data_dir()).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.retain(|p| p.extension().is_some_and(|e| e == "bin") && std::fs::metadata(p).is_ok_and(|m| m.len() == 0x800));
    v.sort();
    v
}
fn save_dump(img: &[u8], tag: &str) -> std::io::Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
    let p = dir.join(format!("{tag}-{t}.bin"));
    std::fs::write(&p, img)?;
    Ok(p)
}
fn is_lost(e: &Error) -> bool {
    matches!(e, Error::NotFound) || matches!(e, Error::Io(io) if matches!(io.raw_os_error(), Some(19) | Some(32) | Some(5)))
}

pub fn spawn(ctx: eframe::egui::Context) -> Worker {
    let (tx, crx) = channel::<Cmd>();
    let (etx, rx) = channel::<Event>();
    std::thread::spawn(move || {
        let mut be: Option<Box<dyn Backend>> = None;
        let mut backed_up = false;
        while let Ok(cmd) = crx.recv() {
            let ev = (|| -> Result<Event, Error> {
                match cmd {
                    Cmd::Connect { sim } => {
                        let mut b: Box<dyn Backend> = if sim {
                            Box::new(Dev { d: Device::connect(Sim::new())?, node: "simulateur".into() })
                        } else {
                            let (d, node) = hidraw::connect_any()?;
                            Box::new(Dev { d, node })
                        };
                        // Première connexion : sauvegarde complète (l'écriture est persistante).
                        let cfg = if !backed_up {
                            let img = b.dump()?;
                            let _ = save_dump(&img, "sauvegarde");
                            backed_up = true;
                            Config::from_mem(&img)?
                        } else { b.read()? };
                        let node = b.node();
                        be = Some(b);
                        Ok(Event::Connected { node, cfg })
                    }
                    Cmd::Refresh => Ok(Event::Refreshed(be.as_mut().ok_or(Error::NotFound)?.read()?)),
                    Cmd::Export => {
                        let img = be.as_mut().ok_or(Error::NotFound)?.dump()?;
                        Ok(Event::Exported(save_dump(&img, "export")?))
                    }
                    Cmd::Restore { img } => {
                        let b = be.as_mut().ok_or(Error::NotFound)?;
                        let _ = save_dump(&b.dump()?, "avant-restauration"); // filet de sécurité
                        b.restore(&img)?;
                        be = None;
                        Ok(Event::Restored)
                    }
                    Cmd::Apply { old, new, flash } => {
                        let b = be.as_mut().ok_or(Error::NotFound)?;
                        let r = b.apply(&old, &new, flash)?;
                        if r.reloaded { be = None; return Ok(Event::Applied { cfg: new, reloaded: true, writes: r.writes }); }
                        std::thread::sleep(std::time::Duration::from_millis(900)); // une seule relecture, après stabilisation
                        let cfg = be.as_mut().unwrap().read()?;
                        Ok(Event::Applied { cfg, reloaded: false, writes: r.writes })
                    }
                }
            })();
            let ev = ev.unwrap_or_else(|e| { let lost = is_lost(&e); if lost { be = None; } Event::Failed { msg: e.to_string(), lost } });
            if etx.send(ev).is_err() { break; }
            ctx.request_repaint();
        }
    });
    Worker { tx, rx }
}

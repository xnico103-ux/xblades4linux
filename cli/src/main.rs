use std::process::exit;
use xblades_proto::{config::*, hidraw, sim::Sim, Device, Error, Transport};

const HELP: &str = "xblades — outil Linux pour la souris X-Blades (1bcf:08b8)

LECTURE SEULE :
  info | status | dump [fichier.bin] | get ADDR [LEN]
ÉCRITURE (simulation sans --apply ; sauvegarde automatique avant) :
  gear-color N R G B     couleur du niveau DPI N (0..5), visible en effet Fixe
  palette N R G B        couleur n°N (0..9) de la palette des effets animés
  effect N               0=fixe 1=arc-en-ciel (2..7 non vérifiés)
  dpi-step N PALIER      palier 0..15 du niveau N (0..5)
  dpi-count N | dpi-current N
  polling HZ             125/250/500/1000 (la souris redémarre)
  reload | checksum | restore factory|FICHIER | uninstall
Options : --apply  --sim  --no-backup
";

fn num(s: &str) -> u16 {
    let r = match s.strip_prefix("0x") { Some(h) => u16::from_str_radix(h, 16), None => s.parse() };
    r.unwrap_or_else(|_| { eprintln!("nombre invalide : {s}"); exit(2) })
}
fn arg(a: &[String], i: usize) -> u16 { num(a.get(i).unwrap_or_else(|| { print!("{HELP}"); exit(2) })) }

fn run<T: Transport>(mut d: Device<T>, node: &str, a: &[String], apply: bool, backup: bool) -> Result<(), Error> {
    d.dry_run = !apply;
    let writes = !matches!(a[0].as_str(), "info" | "status" | "dump" | "get");
    if writes && apply && backup {
        let p = format!("xblades-backup-{}.bin", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0));
        std::fs::write(&p, d.dump()?)?;
        eprintln!("sauvegarde préalable : {p}");
    }
    let old = d.read_config()?;
    let mut new = old.clone();
    match a[0].as_str() {
        "info" => println!("nœud {node}, puce {:?}, firmware 0x{:02x}, produit « {} »", d.chip(), d.firmware, old.product),
        "status" => {
            println!("checksum {} | effet {} | {} Hz | niveau actif {}/{}", if old.checksum_ok { "ok" } else { "INVALIDE" }, effect_name(old.effect), old.polling, old.current_gear + 1, old.gear_count);
            for g in 0..old.gear_count as usize { println!("  niveau {}: palier {} (~{} DPI) couleur {:02x?}", g + 1, old.steps[g], old.dpi(g), old.gear_colors[g]); }
        }
        "dump" => {
            let img = d.dump()?;
            match a.get(1) { Some(p) => { std::fs::write(p, &img)?; println!("{} octets -> {p}", img.len()) } None => for (i, c) in img.chunks(16).enumerate() { println!("{:04x}: {:02x?}", i * 16, c) } }
        }
        "get" => println!("{:02x?}", d.read_mem(arg(a, 1), a.get(2).map_or(1, |s| num(s) as usize))?),
        "gear-color" => new.gear_colors[arg(a, 1) as usize % GEARS] = [arg(a, 2) as u8, arg(a, 3) as u8, arg(a, 4) as u8],
        "palette" => new.palette[arg(a, 1) as usize % PALETTE] = [arg(a, 2) as u8, arg(a, 3) as u8, arg(a, 4) as u8],
        "effect" => new.effect = arg(a, 1) as u8,
        "dpi-step" => new.steps[arg(a, 1) as usize % GEARS] = arg(a, 2) as u8,
        "dpi-count" => new.gear_count = arg(a, 1) as u8,
        "dpi-current" => new.current_gear = arg(a, 1) as u8,
        "polling" => new.polling = arg(a, 1),
        "reload" => d.reload()?,
        "restore" => {
            let img = match a.get(1).map(String::as_str) { Some("factory") => xblades_proto::FACTORY_IMAGE.to_vec(), Some(p) => std::fs::read(p)?, None => { print!("{HELP}"); exit(2) } };
            std::fs::write("avant-restauration.bin", d.dump()?)?;
            d.restore_image(&img)?;
            println!("image écrite ; la souris redémarre (sauvegarde précédente : avant-restauration.bin)");
        }
        "checksum" => println!("checksum 0x{:02x}", d.fix_checksum()?),
        _ => { print!("{HELP}"); exit(2) }
    }
    if new != old {
        let r = d.apply(&old, &new)?;
        println!("{} écriture(s){}", r.writes, if r.reloaded { ", firmware rechargé" } else { "" });
    }
    if !apply && writes {
        for l in d.log.iter().filter(|l| l.contains("[40, 05") || l.contains("[40, 0e")) { println!("[simulation] {l}"); }
    }
    Ok(())
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |f: &str| { let n = args.len(); args.retain(|a| a != f); n != args.len() };
    let (apply, sim, nobk) = (flag("--apply"), flag("--sim"), flag("--no-backup"));
    if args.is_empty() { print!("{HELP}"); exit(2) }
    if args[0] == "uninstall" {
        match hidraw::remove_access() { Ok(_) => println!("règle supprimée ({}).", hidraw::RULE_PATH), Err(e) => { eprintln!("{e}"); exit(1) } }
        return;
    }
    let r = if sim { Device::connect(Sim::new()).and_then(|d| run(d, "simulateur", &args, apply, !nobk)) }
            else { hidraw::connect_any().and_then(|(d, n)| run(d, &n, &args, apply, !nobk)) };
    if let Err(e) = r { eprintln!("erreur : {e}"); exit(1) }
}

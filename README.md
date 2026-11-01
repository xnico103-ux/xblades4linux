# XBlades for Linux

Outil **non officiel**, natif et libre, pour régler la souris **EMPIRE GAMING X-Blades**
(USB `1bcf:08b8`, contrôleur SunplusIT SPCP6651B) sous Linux : éclairage (couleur, fondu, fixe, vitesse),
DPI, fréquence d'interrogation, boutons latéraux et profils.

- Sans Wine, sans service permanent, 100 % hors ligne (aucun accès réseau).
- Interface simple (egui), en français ; mode simulation pour essayer sans souris.
- Protocole retrouvé par décompilation du logiciel Windows : voir [`docs/PROTOCOL.md`](docs/PROTOCOL.md).
- Garde-fous : commandes inconnues refusées, réponse de la souris vérifiée, sauvegarde automatique avant toute écriture permanente.

> ⚠️ **Avertissement.** Ce logiciel écrit dans la mémoire de la souris. Il est fourni sans aucune garantie (GPL-3.0).
> Le code refuse les commandes et les octets inconnus et vérifie les réponses de la souris, mais seules les fonctions
> décrites dans `docs/PROTOCOL.md` ont été validées. L'écriture permanente est désactivée par défaut. Utilisation à vos risques.

## Installation

### AppImage
Télécharger `XBlades-Linux-x86_64.AppImage` depuis les [Releases](../../releases), puis :
```
chmod +x XBlades-Linux-x86_64.AppImage && ./XBlades-Linux-x86_64.AppImage
```
Au premier lancement, « Autoriser l'accès » installe une règle udev (mot de passe, une seule fois).

### Flatpak
Pas encore sur Flathub. Construction locale :
```
flatpak install --user flathub org.freedesktop.Platform//24.08 org.freedesktop.Sdk//24.08 org.freedesktop.Sdk.Extension.rust-stable//24.08
cd packaging/flatpak
flatpak-builder --user --install --force-clean build io.github.xnico103_ux.XBladesLinux.yml
```
Dans Flatpak, la règle udev doit être installée à la main (la commande est affichée dans l'appli).

### Depuis les sources
```
cargo build --release --locked      # binaires : target/release/xblades-gui et xblades (CLI)
cargo test                          # 16 tests, avec un simulateur de souris
packaging/build-appimage.sh         # AppImage (nécessite appimagetool)
```

## Désinstaller
Onglet Diagnostic → « Désinstaller l'accès » (ou `xblades uninstall`), puis supprimer l'application.
Sauvegardes de la souris : `~/.local/share/xblades/`.

## Licence
GPL-3.0-or-later, voir [`LICENSE`](LICENSE) et [`ASSETS.md`](ASSETS.md) pour le logo.
Projet indépendant, non affilié à EMPIRE GAMING ni à SunplusIT.

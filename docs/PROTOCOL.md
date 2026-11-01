# Protocole X-Blades (Sunplus SPCP6651B) — déduit par analyse statique

Source : `X-BLADES APP-EN-1.1.exe` (Delphi/VCL, titre « SPCP6651B Gaming Mouse v1.0 »). Aucune capture USB réelle
n'a encore confirmé ces éléments : tout est **non validé sur matériel**.

## Transport
Rapports HID *feature* de 8 octets via `HidD_SetFeature` / `HidD_GetFeature` (Linux : ioctl hidraw).
Interface retenue si `FeatureReportByteLength == 8` et VID/PID = 1bcf:08b8.

| Rapport | Sens | Rôle |
|---|---|---|
| `40 cmd p1..p6` | SET | commande |
| `41 d0..d6`    | GET | 7 octets de réponse |

## Commandes
| cmd | Format | Effet |
|---|---|---|
| 0x0d | `40 0d 00…` | HELLO ; la réponse `41` doit avoir `d1 == 0x3a` (`d0` = version firmware, > 0x3f pour 6651B) |
| 0x09 | `40 09 AH AL LH LL 00 00` | lecture mémoire ; lire ensuite ⌈L/7⌉ rapports `41` |
| 0x05 | `40 05 AH AL d0 d1 d2 N` | écriture de N (1–3) octets à l'adresse AH:AL |
| 0x0e | `40 0e 00…` | appelé après certains réglages, attente 500 ms ; rôle exact (commit flash ?) **inconnu** |
| 0x06, 0x07, 0x01, 0x02 | divers | lecture alternative / XDATA / ingénierie : non utilisés ici |

## Carte mémoire (2 Kio lisibles ; sections avec checksum = somme octets XOR 0x5a)
| Adresse | Contenu |
|---|---|
| 0x00–0x03 | VID/PID (**ne jamais écrire**) |
| 0x04 | taux d'interrogation (champ de bits) |
| 0x2a | mode respiration |
| 0x2b | nombre de niveaux DPI |
| 0x2c–0x2f | paliers DPI, 2 niveaux/octet (quartet bas = niveau impair), palier 0–10 |
| 0x30–0x38 | bornes/temps respiration, 0x38 = vitesse (FallTime) |
| 0x39–0x56 | 10 couleurs RGB LED |
| 0x57–0x6e | 8 couleurs RGB d'indicateur DPI |
| 0xa3 | checksum zone vendor (0x00–0xa2) |
| 0xa4 | profil ; 0xa5 = LED (quartet haut mode, bas type) ; 0xa6 = DPI courant ; 0xa7–0xaa boutons/profil |
| 0xbf, 0xc0–0x152 | checksum user, profils 1–3 (12 touches × 4 octets) |

## Incertitudes à lever avec le matériel
1. Le logiciel d'origine ne recalcule pas le checksum 0xa3 après ses écritures « live » : probablement RAM, non persistant sans cmd 0x0e.
2. Correspondance palier → DPI réel (11 paliers) et noms des modes LED.
3. Boutons/macros : non implémentés (protocole via cmd 0x05 page ≥1, blocs de 3 octets).
Procédure de validation : `xblades dump avant.bin`, changer UN réglage dans l'appli Windows (VM avec passthrough USB), `dump apres.bin`, comparer.

## Boutons (non validé sur matériel)
- 3 profils de 12 emplacements de 4 octets en 0xC0 / 0xF1 / 0x122 ; checksum à base+0x30 = somme des 48 octets XOR 0x5A. 0xA4 (octet bas) = base du profil actif.
- Actions : `01 m` bouton souris (01 G, 02 D, 04 molette, 08 précédent, 10 suivant) ; `03 00 lo hi` touche multimédia ; `02 mod touche` raccourci clavier ; `09` DPI ; autres conservées telles quelles.
- Écriture : 3 octets puis 1 octet, checksum du profil recalculé, refus si plus de « clic gauche ».
- Effet 1 = fondu (respiration) sur cette souris.

## LED et boutons : ce que montre l'appli Windows 
- Modes (octet 0xA5, quartet bas = mode 1..7, quartet haut = nombre de couleurs du cycle + bit 7) ; ordre des libellés de l'appli : DPI Color, Colorful Light, Rainbow, Flow Light, Waltz Light, 4-Seasons. Seuls les deux premiers sont confirmés sur la souris.
- Les couleurs sont dans les mêmes registres que chez nous : 0x39 (palette ×10) et 0x57 (couleurs de niveau DPI ×8). Le mode « DPI Color » affiche la couleur du niveau actif ; le fondu parcourt la palette.
- Boutons : la souris est symétrique (2 latéraux de chaque côté). Emplacements : 1 = latéral gauche arrière, 4 = latéral gauche avant, 5 = latéral droit avant, 8 = latéral droit arrière, 0/3 = clic gauche/droit, 6 = molette, 7 = DPI, 2 = bouton dessous (action 0E). Le profil 0xC0 est l'image miroir (gaucher) du profil 0xF1.

## Divers
- Observation utilisateur : le premier effet (0xA5 = 0x_1) est un fondu dans la couleur du niveau DPI, pas une lumière fixe.
- 0x30–0x33 : luminosité basse / haute du fondu (u16 big-endian, dump : 0 et 255). Hypothèse : basse = haute = 255 donne une lumière fixe (à valider).
- Les profils de boutons sont écrits puis la souris est redémarrée (0x0E) ; l'appli compare ensuite les boutons relus.

## enregistrement par image complète (trouvé par décompilation Ghidra)
Le logiciel d'origine (fonctions `DefaultParameterConfirm1/2/3`, `FUN_005c3470`) n'écrit pas les profils avec la commande 0x05 : il réécrit **toute l'image de 0x800 octets** :
`40 07 AH AL LH LL 00 00` (ici `40 07 00 00 08 00 00 00`) puis ⌈0x800/7⌉ = 293 rapports feature `41 d0..d6` (le dernier complété par 0xFF), puis commande 0x0E (rechargement).
Les écritures 0x05 sont « live » (RAM) : elles servent aux couleurs/DPI, mais ni les boutons ni le taux d'interrogation (flash) ne sont pris en compte sans cette écriture d'image.
Checksums : 0xA3 = somme(0x00..0xA2)^0x5A ; 0xBF = somme(0xA4..0xBE)^0x5A ; base+0x30 pour chaque profil.

## nouvelles trouvailles (Ghidra)
- Images d'usine : ressources `SPCP6651BDATA_CFG1/2/3` du logiciel (identiques entre elles) = `proto/data/factory-6651b.bin`. Les profils de boutons de l'usine sont identiques à ceux du dump de référence.
- Taux d'interrogation : le logiciel écrit l'octet 0x04 en « live » (0x05) avec bits 5:4 = 00/01/10/11 pour 125/250/500/1000 Hz, **sans rechargement** ; la persistance passe par l'écriture d'image (0x07). Recharger après une écriture live restaure donc l'ancienne valeur (cause du blocage à 500 Hz).
- 0xA4 n'est PAS l'adresse du profil actif : le logiciel l'écrit `(n<<4)+m` (m = 1 ou 2, n = 0..10, 10 -> 0xF) lors du choix de profil/« Fixed Control ». Le profil actif n'est donc pas lisible ici.
- 0x38 : vitesse du fondu, écrite `55 − 5×niveau` (niveau 1..10).

## mode fixe / fondu (décompilage de Btn_Fixed 0x5a634c et cb_ProfileChange 0x57cd98)
- 0xA4 : quartet bas = type d'éclairage, **1 = fondu, 2 = fixe** ; quartet haut = niveau (0..10, 10 -> 0xF).
- Fondu : 0xA8 = 0x0F, 0xA9 = 0, 0xAA = valeur du curseur (dump : 0x18). Fixe : 0xA8 = 0xA9 = 0xAA = 0.
- 0xA7 = 0 (ou valeur d'un contrôle de l'appli). Tous écrits en direct (0x05) puis persistés par l'image complète.
- Les boutons ne sont jamais écrits en direct par l'appli d'origine (KeyStatusOnClick modifie un tampon en mémoire PC, la sauvegarde envoie l'image).
- 0x38 (FallTime_OnClick, libellé « LED Speed ») : octet = 55 - 5 x niveau, niveau 1..10.
- L'hypothèse « luminosité minimale = 0x30/0x31 » est abandonnée.

## Octet 0x2A : à ne PAS écrire
- Le décompilé montre 0x2A comme masque de 8 cases « respiration » (dump et usine : 0xFE). Écrire cet octet à 0 puis réécrire l'image peut laisser la souris sans énumération USB (seul le capteur s'allume). Cause supposée, non prouvée : configuration EEPROM invalide pour la ROM.
- Le contrôleur SPCP6651B1 a un firmware en Mask ROM (32 Ko) et une EEPROM interne pour les réglages ; aucun mode DFU/ISP utilisateur documenté.
- Règle : 0x2A est retiré de la liste blanche. Une écriture d'image (0x07) ne peut plus différer de l'image relue que sur 0x04, 0x38, 0xA3..0xAA, 0xBF et 0xC0..0x152 (`image_diff_allowed`). L'écriture permanente est désactivée par défaut dans l'interface (option avancée).
- Ne jamais écrire une valeur jamais observée sur une vraie souris sans moyen de restauration.

## bouton « Default » du logiciel d'origine (Btn_DefaultClick 0x57f7e8 -> 0x5a3bb8)
- Il copie la ressource d'usine (0x800 octets, `SPCP6651BDATA_CFGn`) dans le tampon PC, rafraîchit l'interface, puis appelle `FUN_005c3470(0, 0x800, tampon, 0x7ff)` = **écriture de bloc 0x07 (paquets de 7 octets)**. Aucune commande de « reset » dédiée n'existe : « défaut » = réécrire l'image d'usine, comme notre fonction Restauration.
- L'exe contient aussi des ressources `SPCP6653BDATA_CFG1..3` (autre puce, non utilisées ici).
- Le logiciel d'origine ne fait aucun chemin de récupération pour une souris qui ne s'énumère plus : tout passe par le canal HID.

## cb_ProfileChange (0x57cd98) : c'est l'éclairage, pas les boutons
- Malgré son nom, cette fonction pilote l'**éclairage** : combo « type » (0x13ac) = 0/1/2, combo « niveau » (0x13b4) = 0..10.
- Types 0 et 2 : écrit 0xA8 = 0xAA = 0xA9 = 0 ; type 1 : 0xA8 = 0x0F, 0xA9 = 0, 0xAA = curseur. 0xA4 = (niveau << 4) + type (niveau 10 -> 0xF0 + type).
- 0xA7 : 0 si l'option 0x1ce2 est décochée ; sinon valeur du combo 0x1240 (+4 en type 1 si 0x1ce3 est cochée).
- Conclusion boutons : aucun registre « profil actif » n'est écrit par le logiciel. Les trois profils de boutons ne sont qu'édités dans le tampon puis enregistrés avec l'image ; le profil actif se choisit avec le bouton physique de la souris. Le sélecteur de profil de notre appli ne peut donc pas changer le profil actif (et « Appliquer » reste grisé tant que rien n'est modifié : comportement normal).

## Garde-fous
- Le champ `breath_mask` et la constante 0x2A ne sont pas gérés par le code (ni lecture, ni écriture, ni interface). Voir ci-dessus.

<p align="center">
  <img src="assets/icon-256.png" width="128" alt="Adio">
</p>

<h1 align="center">Adio</h1>

<p align="center">
  Un petit lecteur audio portable, ultra rapide, écrit en Rust.<br>
  Un seul <code>.exe</code> de ~8 Mo, aucune installation.
</p>

<p align="center">
  <a href="https://github.com/joshsssn/Adio/releases/latest"><b>⬇ Télécharger Adio.exe</b></a>
</p>

<p align="center">
  <img src="docs/screenshot.png" width="720" alt="Adio en thème clair et sombre">
</p>

## Fonctionnalités

- **Interface minimaliste** : une carte compacte sans bordure, en mode jour ou nuit (bascule en bas à droite, ou suit Windows). Elle affiche la pochette, le format audio (`FLAC · 44.1 kHz · 16-bit`), une barre de progression fine, les commandes et le volume sur une seule ligne.
- **Mode mini** : clique sur la pochette pour ne garder que l'artwork, le titre, la progression et précédent / lecture / suivant. Un nouveau clic revient au mode complet.
- **File d'attente** (comme Spotify) : le bouton ⊕ sur chaque titre l'ajoute à la file, qui passe avant la suite du dossier. Dans l'onglet *File d'attente*, glisse les titres pour les réordonner et retire-les avec ✕. Le clic droit propose aussi *Lire ensuite*.
- **Lecture de dossier** : ouvre un son et Adio charge tout son dossier. *Suivant* joue le titre suivant du même dossier, en ordre naturel (`2` avant `10`).
- **Formats** : MP3, WAV, AIFF, FLAC, ALAC / AAC (`.m4a`), OGG Vorbis.
- **Pochette et tags** : affiche l'artwork intégré, le titre, l'artiste et l'album. Sans tags, le nom du fichier sert de titre.
- **Lecture aléatoire** : chaque titre passe une fois avant que le cycle recommence.
- **Répétition** : désactivée, toute la liste ou un seul titre.
- **Barre de progression** : clique ou glisse pour te déplacer dans le morceau. Volume réglable.
- **Historique** : les 1000 dernières écoutes, datées. Un clic relance le titre. Dans l'onglet *Historique*.
- **Playlists** : au format `.m3u8` standard, compatibles VLC, foobar2000, etc. Ajoute le titre en cours avec le bouton de l'en-tête, ou fais un clic droit sur un titre de la liste.
- **Touches multimédia** : les touches play/pause/suivant/précédent du clavier, et l'overlay média de Windows avec titre et pochette.
- **Glisser-déposer** de fichiers ou de dossiers sur la fenêtre.
- **« Ouvrir avec »** : clic droit sur un son → *Ouvrir avec* → `Adio.exe`.
- **Portable** : réglages, historique et playlists sont rangés dans `AdioData/`, à côté de l'exe. Copie le tout sur une clé USB et tout suit.
- **Léger** : démarrage instantané, ~0 % CPU en pause. Réduis la hauteur de la fenêtre pour ne garder que le lecteur.

## Raccourcis

| Touche | Action |
|---|---|
| `Espace` | Lecture / pause |
| `Ctrl + →` / `Ctrl + ←` | Titre suivant / précédent |
| `→` / `←` | Avancer / reculer de 5 s |
| `↑` / `↓` | Volume (ou molette sur le curseur de volume) |
| `S` | Lecture aléatoire |
| `R` | Répétition (off → tout → un) |
| `M` | Muet |
| `Ctrl + O` | Ouvrir un dossier |
| `D` / `Q` / `H` / `P` | Onglet Dossier / File d'attente / Historique / Playlists |
| `N` | Mode mini |
| `Échap` | Revenir à l'onglet Dossier |

Le chemin affiché dans la barre de titre est cliquable : il ouvre un nouveau dossier.

## Utilisation

1. Télécharge `Adio.exe` depuis les [Releases](https://github.com/joshsssn/Adio/releases/latest) et place-le où tu veux.
2. Lance-le, puis glisse un dossier ou un son dans la fenêtre. Tu peux aussi faire clic droit sur un son → *Ouvrir avec* → *Choisir une autre application* → `Adio.exe`.
3. Au prochain lancement, Adio reprend là où tu t'étais arrêté, en pause.

> Windows SmartScreen peut afficher un avertissement au premier lancement, car l'exe n'est pas signé. Clique sur *Informations complémentaires* puis *Exécuter quand même*.

## Compiler

Il faut [Rust](https://rustup.rs) en version stable avec la toolchain MSVC.

```sh
git clone https://github.com/joshsssn/Adio.git
cd Adio
cargo build --release
# → target/release/adio.exe
```

Le binaire est lié statiquement au runtime C (`+crt-static`), il n'a donc besoin d'aucune DLL Visual C++.

### Stack

[egui/eframe](https://github.com/emilk/egui) pour l'interface, [rodio](https://github.com/RustAudio/rodio) et [symphonia](https://github.com/pdeljanov/Symphonia) pour le décodage et la sortie audio, [lofty](https://github.com/Serial-ATA/lofty-rs) pour les tags et pochettes, [souvlaki](https://github.com/Sinono3/souvlaki) pour les touches multimédia et SMTC.

### Tests

```sh
cargo test
# test des formats sur un dossier de vrais sons :
ADIO_TEST_DIR="C:/chemin/vers/sons" cargo test formats -- --nocapture
```

## Licence

[MIT](LICENSE). La police [Inter](https://rsms.me/inter/) est distribuée sous licence SIL OFL (`assets/fonts/LICENSE.txt`).

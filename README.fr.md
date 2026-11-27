<div align="center">

### Norbanner — un shell de bureau façon Caelestia pour Windows

  Un cadre autour de l'écran, une barre verticale et des panneaux qui sortent
  de ses bords, colorés avec Material You d'après votre fond d'écran.

  [English](README.md) · [Español](README.es.md) · [Français](README.fr.md)

</div>

Norbanner apporte l'allure et le comportement de
[Caelestia](https://github.com/caelestia-dots/shell), le shell Quickshell pour
Hyprland, à Windows 10 et 11. Il est écrit en Rust sur Direct3D,
Direct2D et DirectComposition ; il partage le design de Caelestia, pas son code.

## Fonctions

- **Une seule surface continue** : un cadre fin autour de chaque écran, la
  barre à gauche, et des panneaux qui fondent hors du cadre avec le mouvement
  expressif de Material 3. Les fenêtres maximisées restent dans le cadre.
- **Barre** : logo (ouvre le lanceur), bureaux virtuels avec indicateur animé
  et leurs applications, fenêtre active, icônes masquées, horloge verticale,
  état réseau/Bluetooth/batterie, bouton d'alimentation.
- **Tableau de bord** (bord haut) : utilisateur, grande horloge, calendrier,
  ressources et lecture en cours.
- **Lanceur** (touche Windows seule) : recherche floue des applications du
  menu Démarrer ; `>` pour les commandes (calculatrice, fonds d'écran,
  variantes de couleurs, clair/sombre, session).
- **Menu de session**, **curseurs de volume et luminosité** (DDC/CI).
- **Material You** à partir du fond d'écran, de la couleur d'accent ou d'une
  couleur choisie.
- **Paramètres** façon Nexus en huit langues ; `shell.json` modifiable à la main.

Fonctionnement et feuille de route : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Installer

**[Télécharger Norbanner-Setup.exe](https://github.com/NobodyNot543/Norbanner/releases/latest/download/Norbanner-Setup.exe)**
(Windows 10 1903 ou ultérieur, ou Windows 11 ; 64 bits).

## Compiler

```powershell
cargo run -p norbanner
cd apps\settings; pnpm install; pnpm tauri dev
```

## Crédits

Norbanner a été construit à partir de deux projets open source :

- **[Muelle](https://github.com/crafter-station/Muelle)** de Crafter Station
  et contributeurs (MIT) : la base de code Windows dont Norbanner est parti.
- **[Caelestia](https://github.com/caelestia-dots/shell)** de soramane et
  contributeurs : le design que suit Norbanner.

## Licence

[MIT](LICENSE)

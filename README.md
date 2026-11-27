<div align="center">

### Norbanner — a Caelestia-style desktop shell for Windows

  A frame around the screen, a vertical bar and drawers that grow out of its
  edges, coloured with Material You from your wallpaper.

  [English](README.md) · [Español](README.es.md) · [Français](README.fr.md)

</div>

Norbanner brings the look and feel of [Caelestia](https://github.com/caelestia-dots/shell),
the Quickshell shell for Hyprland, to Windows 10 and 11. It is written in Rust on
Direct3D, Direct2D and DirectComposition; it shares
Caelestia's design, not its code.

## Features

- **One continuous surface.** A thin frame surrounds every monitor, with the
  bar on the left. Drawers are part of the same shape: they melt out of the
  frame with Material 3 expressive motion instead of popping up as windows.
  Maximized windows stay inside the frame.
- **Bar.** Logo (opens the launcher), virtual desktops as workspaces with an
  animated indicator and the apps on each one, the active window, hidden tray
  icons, a vertical clock, network/Bluetooth/battery/Caps Lock status and a
  power button. Scroll on the bar to switch desktops, change the volume (top
  half) or brightness (bottom half).
- **Dashboard** (hover the top edge): you and your uptime, a large clock, the
  month, CPU/memory/disk meters and what is playing, with controls and a
  progress arc around the cover.
- **Launcher** (tap the Windows key, or click the logo): fuzzy search over the
  Start menu's apps with their icons; type `>` for commands — calculator,
  wallpaper switcher, colour scheme variant, light/dark, lock, sign out,
  sleep, restart, shut down. Arithmetic shows its result as you type.
- **Session menu** (power button): sign out, shut down, sleep, restart, with
  keyboard navigation.
- **Volume and brightness sliders** on the right edge when the level changes
  or when you hover there. Brightness works on external monitors over DDC/CI.
- **Material You.** The whole shell is coloured from the wallpaper (or
  Windows' accent colour, or a colour you pick), in nine scheme variants,
  dark or light, and recolours when the wallpaper changes.
- **Settings app** styled after Caelestia's Nexus, in eight languages, with a
  live preview of the colours. Changes apply immediately; `shell.json` can
  also be edited by hand.
- **Scriptable:** `norbanner shell drawers toggle launcher`, `norbanner shell scheme
  variant vibrant`, `norbanner shell wallpaper set <file>`… (see
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#ipc)).
- A watchdog restores the Windows taskbar and work area if the shell exits
  unexpectedly.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how it works and which
Caelestia modules are done and planned.

## Install

**[Download Norbanner-Setup.exe](https://github.com/NobodyNot543/Norbanner/releases/latest/download/Norbanner-Setup.exe)**
(Windows 10 version 1903 or later, or Windows 11; 64-bit). It installs for
your user only, without administrator rights. SmartScreen may not recognise
it yet: choose **More info → Run anyway**.

## Build and run

Prerequisites: Rust with the MSVC toolchain and the Visual Studio C++ Build
Tools, Node.js and pnpm.

```powershell
cargo run -p norbanner                 # the shell
cargo run -p norbanner -- --restore    # bring the taskbar and work area back by hand
```

To try it without hiding your taskbar, point it at a test configuration:

```powershell
'{ "general": { "replaceTaskbar": false, "reserveSpace": false } }' > $env:TEMP\shell.json
$env:NORBANNER_CONFIG = "$env:TEMP\shell.json"; cargo run -p norbanner
```

Settings:

```powershell
cd apps\settings
pnpm install
pnpm tauri dev
```

Tests and checks (as CI runs them):

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps\settings; pnpm test; pnpm build
```

## Project layout

| Path | Purpose |
|---|---|
| `apps/shell` | The shell (`norbanner.exe`) |
| `apps/settings` | Tauri settings app (`norbanner-settings.exe`) |
| `crates/norbanner-core` | Platform-independent motion, geometry, distance field, search |
| `crates/norbanner-theme` | Material You colour schemes |
| `crates/norbanner-config` | `shell.json` schema and persistence |
| `crates/norbanner-shell` | Windows services |
| `assets/fonts` | Rubik and a Material Symbols Rounded subset (`scripts/subset-icons.py`) |
| `norbanner` | Website |

## Privacy

Norbanner collects no data and has no telemetry. Its only network request is
optional: Settings asks GitHub Releases for a newer version when it opens
(turn it off in Settings → General → *Check for updates*).

## Credits

Norbanner was built from two open-source projects:

- **[Muelle](https://github.com/crafter-station/Muelle)** by Crafter Station
  and contributors (MIT): the Windows code base Norbanner started from.
- **[Caelestia](https://github.com/caelestia-dots/shell)** by soramane and
  contributors: the design Norbanner follows.

Fonts: [Rubik](https://fonts.google.com/specimen/Rubik)
(OFL) and [Material Symbols](https://fonts.google.com/icons) (Apache 2.0).
Colour science: [material-colors](https://crates.io/crates/material-colors)
(MIT/Apache 2.0).

## License

[MIT](LICENSE)

# Architecture

Norbanner is a desktop shell for Windows modelled on
[Caelestia](https://github.com/caelestia-dots/shell), the Quickshell shell for
Hyprland. It is an independent implementation in Rust: no Caelestia code is
used (Caelestia is GPL-3.0; Norbanner stays MIT). What it shares is the design:
the layout, the motion, the Material 3 tokens and the behaviour of each module.

## The idea

One continuous surface per monitor:

```
┌──────────────────────────────────────────────────────┐
│█│               ╭── dashboard ──╮                    │  thin frame (10 px)
│█│               ╰───────────────╯                    │
│█│                                                ╭─╮ │
│B│            application windows                 │s│ │  session / OSD
│A│            (inside the frame)                  ╰─╯ │  on the right edge
│R│                  ╭─── launcher ───╮                │
│█│                  ╰────────────────╯                │
└──────────────────────────────────────────────────────┘
```

The frame and every open drawer are a single shape: a signed distance field
(`norbanner_core::sdf`) where drawers are rounded rectangles merged with the
frame through a smooth union. The GPU draws it with a pixel shader
(`apps/shell/src/render/blob.hlsl`); the CPU evaluates the same field at the
cursor to decide whether a click belongs to the shell or to the window
below. A drawer opening is just its rectangle moving out of the frame along
an expressive Material 3 curve, so it seems to grow out of the edge like
liquid.

## Crates

| Crate | What it holds | Windows? |
|---|---|---|
| `crates/norbanner-core` | Motion (M3 easing curves, `Anim`), geometry, the SDF, fuzzy search, calculator, calendar grids | No — unit-tested anywhere |
| `crates/norbanner-theme` | Material You: source colour → full M3 scheme (`material-colors`) | No |
| `crates/norbanner-config` | `shell.json`: schema, defaults, sanitising, locked atomic writes, JSON patches | No |
| `crates/norbanner-shell` | Windows services: windows/apps, audio, network, battery, virtual desktops, session actions, work area, taskbar takeover, wallpaper, DDC/CI brightness, IPC | Yes |
| `apps/shell` (`norbanner.exe`) | The shell itself | Yes |
| `apps/settings` (`norbanner-settings.exe`) | Tauri + React settings app, styled after Caelestia's *Nexus* | Yes |

## The shell (`apps/shell`)

- **`main.rs`** — command line and the message loop. The loop drains every
  pending Windows message, then the shell's own event queue, then draws a
  frame if something changed or is animating; otherwise it sleeps in
  `WaitMessage`. Idle costs nothing.
- **`events.rs`** — window procedures and hooks only *record* events; the
  loop handles them afterwards. That removes re-entrancy (Windows calls window
  procedures from inside many APIs) without locks.
- **`app.rs`** — the controller: services, drawers, hover rules, keyboard
  focus, actions, IPC commands, theme and work-area reservation.
- **`screen.rs`** — one per monitor: a topmost, layered, `WS_EX_TRANSPARENT`
  popup covering the monitor. Transparency to the mouse is switched off only
  while the cursor is over the surface or a clickable region.
- **`render/`** — Direct3D 11 + DirectComposition swap chain per screen;
  `blob.rs` draws the surface shader, `paint.rs` draws content with
  Direct2D/DirectWrite in logical pixels, `fonts.rs` loads the embedded
  Rubik and Material Symbols Rounded (variable axes: weight and FILL).
- **`modules/`** — immediate-mode modules. Each draws from the current state
  every frame and registers what is clickable; `modules/mod.rs` places
  drawers on frame edges.
- **`hotkeys.rs`** — a tap of the Windows key alone toggles the launcher
  (Super on Caelestia); other shortcuts use `RegisterHotKey`.
- **`guard.rs`** — a watchdog process that restores the work area and the
  taskbar if the shell exits or hangs.

## Caelestia → Windows map

| Caelestia | Norbanner | Status |
|---|---|---|
| `modules/drawers` (frame + blob surface) | `screen.rs`, `render/blob.hlsl`, `norbanner_core::sdf` | ✅ |
| Exclusive zones | `apps/shell/src/exclusive.rs`: four hidden AppBars per monitor (Explorer then fits windows, desktop icons and its panels inside the frame) | ✅ |
| `modules/bar`: logo, workspaces, active window, tray, clock, status icons, power | `modules/bar.rs` | ✅ (tray opens Windows' hidden-icons flyout) |
| Hyprland workspaces | Windows virtual desktops (registry + `IVirtualDesktopManager`, switching via Ctrl+Win+←/→) | ✅ |
| `modules/dashboard` Dash tab (user, date/time, calendar, resources, media) | `modules/dashboard.rs` | ✅ |
| Dashboard Media and Performance tabs, weather | — | Phase 2 |
| `modules/launcher` (apps, `>` actions, calculator, wallpapers, scheme variants) | `modules/launcher.rs` (Start menu apps via `shell:AppsFolder`) | ✅ |
| `modules/session` | `modules/session.rs` + `norbanner_shell::session` | ✅ |
| `modules/osd` (volume, brightness) | `modules/osd.rs` (brightness via DDC/CI on external monitors) | ✅ |
| `services/Colours` (Material You from the wallpaper) | `norbanner-theme` + `norbanner_shell::wallpaper` | ✅ |
| IPC (`caelestia shell …`) | `norbanner shell …` over `WM_COPYDATA` | ✅ |
| Config `~/.config/caelestia/shell.json`, live reload | `%APPDATA%\Norbanner\shell.json`, live reload | ✅ |
| `modules/nexus` (settings) | `apps/settings` (Tauri) | ✅ first version |
| Bar popouts (network, audio, battery, tray menus, window preview) | — | Phase 2 |
| `modules/notifications`, `modules/sidebar` | Needs `UserNotificationListener` (requires package identity) | Phase 3 |
| `modules/utilities` (toggles, recording, idle inhibit) | — | Phase 3 |
| `modules/lock` | Windows owns the lock screen; Norbanner uses `LockWorkStation` | Not planned |
| `modules/background` (desktop clock, visualiser) | Needs a window behind desktop icons (WorkerW) | Phase 3 |
| `modules/areapicker` (screenshots) | — | Phase 3 |

## Configuration

`%APPDATA%\Norbanner\shell.json`; every field is optional. Set `NORBANNER_CONFIG`
to use another file (handy to try settings without touching the real one).

```json
{
  "border": { "thickness": 10, "rounding": 25, "smoothing": 20 },
  "theme": { "source": "wallpaper", "mode": "dark", "variant": "tonalSpot" },
  "bar": { "persistent": true, "clock": { "showDate": true } },
  "launcher": { "hotkey": "super", "maxShown": 7 },
  "general": { "replaceTaskbar": true, "reserveSpace": true }
}
```

## IPC

```powershell
norbanner shell drawers toggle launcher   # or dashboard, session
norbanner shell drawers close
norbanner shell scheme variant vibrant
norbanner shell scheme mode light
norbanner shell wallpaper set C:\path\to\image.jpg
norbanner shell mpris playPause            # next, previous
norbanner shell workspace 3
norbanner shell reload
norbanner --quit
```

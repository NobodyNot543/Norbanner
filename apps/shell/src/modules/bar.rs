//! The vertical bar on the left edge.
//!
//! Its look is Norbanner's own: two gradient orbs (the logo and power) close
//! the bar at each end; workspaces are numbered orbs, the current one glowing
//! in the wallpaper's colours, each with tiles for the apps on it; a record
//! disc for what is playing, with its progress ring; large gradient clock
//! digits; and a battery drawn as a capsule of liquid. Everything takes its
//! colours from the Material You scheme.

use std::time::Instant;

use norbanner_core::{
    anim::{Anim, Curve},
    geom::Rect,
};
use norbanner_shell::status::Network;
use norbanner_theme::Color;

use crate::{actions::Action, icons, render::paint::TextStyle, state::image_key, ui::Ui};

/// Per-screen bar state that outlives a frame.
pub struct BarState {
    /// Where the active app's selector sits, relative to the workspace
    /// island; it slides between tiles.
    selector: Anim,
    selector_shown: bool,
    /// The current workspace's app tiles last frame, for the windows panel.
    pub app_tiles: Vec<(Rect, crate::state::DesktopApp)>,
    /// The tray button last frame, for the background apps bubble.
    pub tray: Option<Rect>,
}

impl Default for BarState {
    fn default() -> Self {
        Self {
            selector: Anim::new(0.0),
            selector_shown: false,
            app_tiles: Vec::new(),
            tray: None,
        }
    }
}

impl BarState {
    pub fn animating(&self, now: Instant) -> bool {
        self.selector.animating(now)
    }
}

/// The bar's full width for these tokens and border.
pub fn width(ui_inner: f32, border: f32, padding_small: f32) -> f32 {
    ui_inner + padding_small.max(border) * 2.0
}

enum Entry {
    Logo,
    Workspaces,
    Spacer,
    ActiveWindow,
    Tray,
    Clock,
    StatusIcons,
    Power,
}

fn entry(id: &str) -> Option<Entry> {
    Some(match id {
        "logo" => Entry::Logo,
        "workspaces" => Entry::Workspaces,
        "spacer" => Entry::Spacer,
        "activeWindow" => Entry::ActiveWindow,
        "tray" => Entry::Tray,
        "clock" => Entry::Clock,
        "statusIcons" => Entry::StatusIcons,
        "power" => Entry::Power,
        _ => return None,
    })
}

/// What the bar leaves out when the screen is too short for everything.
/// The open apps go last: first the weekday and the tray button, then fewer
/// app tiles.
#[derive(Clone, Copy, Debug)]
struct Fit {
    max_tiles: usize,
    no_tray: bool,
    no_weekday: bool,
}

impl Fit {
    /// From roomiest to tightest.
    fn levels(max_tiles: usize) -> Vec<Self> {
        let full = Self {
            max_tiles,
            no_tray: false,
            no_weekday: false,
        };
        let mut levels = vec![
            full,
            Self {
                no_weekday: true,
                ..full
            },
            Self {
                no_weekday: true,
                no_tray: true,
                ..full
            },
            Self {
                no_weekday: true,
                no_tray: true,
                ..full
            },
        ];
        let tightest = levels[levels.len() - 1];
        for tiles in (0..max_tiles).rev() {
            levels.push(Self {
                max_tiles: tiles,
                ..tightest
            });
        }
        levels
    }
}

/// The two accent colours every gradient runs between.
fn accents(ui: &Ui) -> (Color, Color) {
    (ui.pal.primary, ui.pal.tertiary)
}

pub fn draw(ui: &mut Ui, bar: Rect, state: &mut BarState, monitor: isize) {
    let t = *ui.t;
    let inner = t.bar_inner;
    let x = bar.x + (bar.w - inner) / 2.0;
    let entries: Vec<Entry> = ui
        .config
        .bar
        .entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| entry(&e.id))
        .collect();

    // Heights of fixed entries. The active window takes what its name
    // needs (up to what is left); spacers share the rest. Entries with
    // nothing to show take no room at all.
    let active_natural = {
        let style = TextStyle::new(t.font.normal).weight(600.0);
        let (w, _) = ui.p.measure(&active_label(ui), style);
        // A little slack: text laid out at exactly its width gets ellipsised.
        w + 6.0
    };
    let has_active = entries.iter().any(|e| matches!(e, Entry::ActiveWindow));
    let spacing = t.spacing.medium;
    let v_padding = t.padding.large;
    let heights_for = |fit: Fit| -> Vec<Option<f32>> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Spacer | Entry::ActiveWindow => None,
                Entry::Logo => Some(inner),
                Entry::Workspaces => Some(workspaces_height(ui, fit)),
                Entry::Tray => Some(if fit.no_tray { 0.0 } else { inner * 0.6 }),
                Entry::Clock => Some(clock_height(ui, fit)),
                Entry::StatusIcons => Some(status_height(ui)),
                Entry::Power => Some(inner),
            })
            .collect()
    };
    let fixed_for = |heights: &[Option<f32>]| {
        let shown = heights.iter().filter(|h| h.is_none_or(|h| h > 0.0)).count();
        heights.iter().flatten().sum::<f32>() + spacing * shown.saturating_sub(1) as f32
    };
    // The app's name counts when choosing what to leave out (up to a
    // reasonable length), so it is not the first thing squeezed.
    let wanted_name = if has_active {
        active_natural.min(t.bar_inner * 4.0) + spacing
    } else {
        0.0
    };
    // The roomiest layout that fits.
    let available = bar.h - v_padding * 2.0;
    let levels = Fit::levels(ui.config.bar.workspaces.max_window_icons as usize);
    let (fit, heights) = levels
        .iter()
        .map(|&fit| (fit, heights_for(fit)))
        .find(|(_, heights)| fixed_for(heights) + wanted_name <= available)
        .unwrap_or_else(|| {
            let fit = levels[levels.len() - 1];
            (fit, heights_for(fit))
        });
    let fixed = fixed_for(&heights);
    let left = (available - fixed).max(0.0);
    let active_h = if has_active {
        active_natural.min(left)
    } else {
        0.0
    };
    let spacers = entries
        .iter()
        .filter(|e| matches!(e, Entry::Spacer))
        .count()
        .max(1) as f32;
    let free = ((left - active_h) / spacers).max(0.0);

    // Scrolling: workspaces over the workspace island, volume over the top
    // half, brightness over the bottom half.
    let top_half = Rect::new(bar.x, bar.y, bar.w, bar.h / 2.0);
    let bottom_half = Rect::new(bar.x, bar.y + bar.h / 2.0, bar.w, bar.h / 2.0);
    if ui.config.bar.scroll_actions.volume {
        ui.scroll(top_half, Action::VolumeStep(0));
    }
    if ui.config.bar.scroll_actions.brightness {
        ui.scroll(bottom_half, Action::BrightnessStep(monitor, 0));
    }

    state.app_tiles.clear();
    state.tray = None;
    let mut y = bar.y + v_padding;
    for (entry, height) in entries.iter().zip(&heights) {
        if height.is_some_and(|h| h <= 0.0) {
            continue;
        }
        let h = height.unwrap_or(if matches!(entry, Entry::ActiveWindow) {
            active_h
        } else {
            free
        });
        let rect = Rect::new(x, y, inner, h);
        match entry {
            Entry::Logo => logo(ui, rect),
            Entry::Workspaces => workspaces(ui, rect, state, fit),
            Entry::Spacer => {}
            Entry::ActiveWindow => active_window(ui, rect),
            Entry::Tray => {
                state.tray = Some(rect);
                tray(ui, rect);
            }
            Entry::Clock => clock(ui, rect, fit),
            Entry::StatusIcons => status_icons(ui, rect),
            Entry::Power => power_button(ui, rect),
        }
        y += h + spacing;
    }
}

/// The logo, on its own (no background): the image set in `bar.logo`, or
/// Norbanner's. It grows a little under the pointer. Click: the launcher.
fn logo(ui: &mut Ui, rect: Rect) {
    let hovered = ui.hovered(rect);
    ui.click(rect, Action::ToggleLauncher);
    let side = rect.w.min(rect.h) * if hovered { 0.92 } else { 0.82 };
    let image = rect.centered(side, side);
    if let Some(logo) = ui.state.logo.clone() {
        ui.p.image_fit(image_key(&format!("logo:{}", logo.width)), &logo, image);
    } else {
        let (cx, cy) = rect.center();
        let colour = ui.pal.primary;
        ui.p.icon(icons::DEPLOYED_CODE, cx, cy, side * 0.8, 1.0, colour);
    }
}

/// The workspaces holding windows, in order. A new one appears as soon as
/// something opens on it.
fn visible_desktops(ui: &Ui) -> Vec<usize> {
    ui.state
        .desktop_apps
        .iter()
        .enumerate()
        .filter(|(_, apps)| !apps.is_empty())
        .map(|(i, _)| i)
        .collect()
}

/// Side of one app tile in a workspace.
fn app_cell(t: &crate::tokens::Tokens) -> f32 {
    (t.bar_inner * 0.72).round()
}

fn apps_shown(ui: &Ui, index: usize, fit: Fit) -> usize {
    if !ui.config.bar.workspaces.show_windows {
        return 0;
    }
    ui.state
        .desktop_apps
        .get(index)
        .map_or(0, |apps| apps.len().min(fit.max_tiles))
}

fn item_height(ui: &Ui, index: usize, fit: Fit) -> f32 {
    let apps = apps_shown(ui, index, fit) as f32;
    (apps * (app_cell(ui.t) + 2.0) - 2.0).max(0.0)
}

fn workspaces_height(ui: &Ui, fit: Fit) -> f32 {
    let t = ui.t;
    let groups: Vec<f32> = visible_desktops(ui)
        .iter()
        .map(|&i| item_height(ui, i, fit))
        .filter(|&h| h > 0.0)
        .collect();
    if groups.is_empty() {
        return 0.0;
    }
    groups.iter().sum::<f32>() + group_gap(t) * (groups.len() - 1) as f32 + t.padding.small * 2.0
}

/// Space between two workspaces' apps, with a divider in it.
fn group_gap(t: &crate::tokens::Tokens) -> f32 {
    t.spacing.medium
}

fn workspaces(ui: &mut Ui, rect: Rect, state: &mut BarState, fit: Fit) {
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, b) = accents(ui);
    let scale = ui.config.appearance.anim_scale;
    // Squarer than a pill, so the tiles inside read as tiles.
    ui.p.rect(rect, t.rounding.medium.min(rect.w / 2.0), ui.layer);
    if ui.config.bar.scroll_actions.workspaces {
        ui.scroll(rect, Action::StepDesktop(0));
    }
    let current = ui.state.desktops.current;
    let front = ui.state.active.as_ref().map(|w| w.hwnd);
    let side = app_cell(&t);
    let (cx, _) = rect.center();

    // Lay out each workspace's apps, a divider between workspaces.
    let mut tiles: Vec<(usize, Rect, crate::state::DesktopApp)> = Vec::new();
    let mut y = rect.y + t.padding.small;
    let mut first = true;
    for i in visible_desktops(ui) {
        let shown = apps_shown(ui, i, fit);
        if shown == 0 {
            continue;
        }
        if !first {
            let gap = group_gap(&t);
            ui.p.rect(
                Rect::new(cx - side * 0.3, y + gap / 2.0 - 0.75, side * 0.6, 1.5),
                0.75,
                pal.outline_variant,
            );
            y += gap;
        }
        first = false;
        let apps = ui.state.desktop_apps.get(i).cloned().unwrap_or_default();
        for app in apps.into_iter().take(shown) {
            tiles.push((i, Rect::new(cx - side / 2.0, y, side, side), app));
            y += side + 2.0;
        }
        y -= 2.0;
    }

    // The selector slides to the app in front: a soft tinted square with a
    // short accent bar on the island's edge.
    let target = tiles
        .iter()
        .find(|(_, _, app)| front.is_some_and(|hwnd| app.windows.contains(&hwnd)))
        .map(|(_, tile, _)| tile.y - rect.y);
    match target {
        Some(y) if !state.selector_shown => {
            state.selector.snap(y);
            state.selector_shown = true;
        }
        Some(y) => state
            .selector
            .set(y, Curve::ExpressiveFastSpatial, scale, ui.now),
        None => state.selector_shown = false,
    }
    if state.selector_shown {
        let y = rect.y + state.selector.value(ui.now);
        let square = Rect::new(cx - side / 2.0, y, side, side);
        ui.p.rect(square, side * 0.24, a.with_alpha(0.22));
        ui.p.rect_gradient(
            Rect::new(rect.x + 1.5, y + side * 0.25, 3.0, side * 0.5),
            1.5,
            a,
            b,
        );
    }

    for (i, tile, app) in tiles {
        let here = i == current;
        let is_front = front.is_some_and(|hwnd| app.windows.contains(&hwnd));
        if here {
            state.app_tiles.push((tile, app.clone()));
        }
        // A pinned app with no window opens it.
        let action = if app.windows.is_empty() {
            Action::Launch(app.app_id.clone())
        } else {
            Action::ActivateApp {
                desktop: i,
                windows: app.windows.clone(),
            }
        };
        ui.button(tile, side * 0.24, pal.on_surface, action);
        let icon = tile.inset(side * 0.12);
        // Apps on other workspaces are dimmed.
        // Pinned apps that are not open are dimmed too.
        let opacity = if app.windows.is_empty() {
            0.55
        } else if is_front || here {
            1.0
        } else {
            0.5
        };
        if let Some(image) = ui.state.icons.get(&app.app_id) {
            ui.p.faded(opacity, |p| {
                p.image_fit(image_key(&app.app_id), &image, icon);
            });
        } else {
            ui.p.circle(cx, tile.center().1, side / 6.0, pal.outline);
        }
        // Several windows: a short bar under the icon, longer with more.
        let n = app.windows.len();
        if n > 1 {
            let w = (4.0 + 4.0 * (n.min(4) as f32 - 1.0)).min(side * 0.6);
            ui.p.rect(
                Rect::new(cx - w / 2.0, tile.bottom() - 3.0, w, 2.5),
                1.25,
                if is_front { b } else { pal.outline },
            );
        }
    }
}

/// The active app's name ("Antigravity IDE"), not its window title. Until
/// the name is known, the part of the title after the last " - ", which is
/// where most apps put their own name.
fn app_label(ui: &Ui, active: &crate::state::ActiveWindow) -> String {
    ui.state.icons.name(&active.app_id).unwrap_or_else(|| {
        active
            .title
            .rsplit(" - ")
            .next()
            .unwrap_or(&active.title)
            .trim()
            .to_string()
    })
}

/// What the active-window entry reads: the active app's name, or the
/// project's name on the bare desktop, so the bar is never left empty there.
fn active_label(ui: &Ui) -> String {
    ui.state
        .active
        .as_ref()
        .map_or_else(|| DESKTOP_LABEL.to_string(), |active| app_label(ui, active))
}

const DESKTOP_LABEL: &str = "Norbanner";

fn active_window(ui: &mut Ui, rect: Rect) {
    let active = ui.state.active.clone();
    let t = *ui.t;
    let (a, b) = accents(ui);
    // Just the app's name, reading top to bottom a quarter turn, in the
    // accent gradient.
    let label = active_label(ui);
    let style = TextStyle::new(t.font.normal).weight(600.0);
    let (text_w, _) = ui.p.measure(&label, style);
    let shown = (text_w + 6.0).min(rect.h);
    if shown < 20.0 {
        return;
    }
    let (cx, _) = rect.center();
    if let Some(active) = active {
        ui.click(
            Rect::new(rect.x, rect.y, rect.w, shown),
            Action::FocusWindow(active.hwnd),
        );
    }
    let line = t.font.normal * 1.5;
    let cy = rect.y + shown / 2.0;
    let text_rect = Rect::new(cx - shown / 2.0, cy - line / 2.0, shown, line);
    ui.p.rotated(90.0, cx, cy, |p| {
        p.text_gradient(&label, text_rect, style, a, b);
    });
}

fn tray(ui: &mut Ui, rect: Rect) {
    let pal = *ui.pal;
    let t = *ui.t;
    let side = rect.h.min(rect.w * 0.75);
    ui.icon_button(
        rect.centered(side, side),
        icons::EXPAND_LESS,
        t.icon.medium * 0.9,
        0.0,
        None,
        pal.outline,
        Action::OpenTray,
    );
}

fn digit_line(t: &crate::tokens::Tokens) -> f32 {
    t.font.large * 1.12
}

fn clock_height(ui: &Ui, fit: Fit) -> f32 {
    let t = ui.t;
    let c = &ui.config.bar.clock;
    let mut h = t.padding.small * 2.0 + digit_line(t) * 2.0 + t.spacing.extra_small;
    if c.show_icon && !fit.no_weekday {
        h += t.font.small * 1.25;
    }
    if c.show_date {
        h += t.font.normal * 1.4;
    }
    if c.show_seconds {
        h += t.font.normal * 1.2;
    }
    h
}

fn clock(ui: &mut Ui, rect: Rect, fit: Fit) {
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, b) = accents(ui);
    let c = ui.config.bar.clock.clone();
    let clock = ui.state.clock;
    ui.button(
        rect,
        t.rounding.large,
        pal.on_surface,
        Action::ToggleDashboard,
    );
    let mut y = rect.y + t.padding.small;
    if c.show_icon && !fit.no_weekday {
        let h = t.font.small * 1.25;
        let day = crate::state::format_date(clock.year, clock.month, clock.day, "ddd")
            .trim_end_matches('.')
            .to_uppercase();
        let short: String = day.chars().take(3).collect();
        ui.p.text(
            &short,
            Rect::new(rect.x, y, rect.w, h),
            TextStyle::new(t.font.small * 0.85).weight(700.0).center(),
            b,
        );
        y += h;
    }
    if c.show_date {
        let h = t.font.normal * 1.4;
        ui.p.text(
            &clock.day.to_string(),
            Rect::new(rect.x, y, rect.w, h),
            TextStyle::new(t.font.normal).weight(500.0).center(),
            pal.on_surface_variant,
        );
        y += h;
    }
    // Big gradient hours over light minutes, with two dots between.
    let line = digit_line(&t);
    let twelve = ui.config.services.use_twelve_hour_clock;
    let size = t.font.large * 1.0;
    ui.p.text_gradient(
        &clock.hour_text(twelve),
        Rect::new(rect.x, y, rect.w, line),
        TextStyle::new(size).weight(800.0).center(),
        a,
        b,
    );
    y += line;
    let (cx, _) = rect.center();
    let dot_y = y + t.spacing.extra_small / 2.0;
    ui.p.circle(cx - 4.0, dot_y, 1.8, a);
    ui.p.circle(cx + 4.0, dot_y, 1.8, b);
    y += t.spacing.extra_small;
    ui.p.text(
        &format!("{:02}", clock.minute),
        Rect::new(rect.x, y, rect.w, line),
        TextStyle::new(size).weight(300.0).center(),
        pal.on_surface,
    );
    y += line;
    if c.show_seconds {
        ui.p.text(
            &format!("{:02}", clock.second),
            Rect::new(rect.x, y, rect.w, t.font.normal * 1.2),
            TextStyle::new(t.font.normal).center(),
            pal.on_surface_variant,
        );
    }
}

#[derive(Clone, Copy)]
enum Status {
    Lock,
    Audio,
    Microphone,
    KbLayout,
    Network,
    Bluetooth,
    Battery,
}

fn statuses(ui: &Ui) -> Vec<Status> {
    ui.config
        .bar
        .status_icons
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| {
            Some(match e.id.as_str() {
                "lockStatus" if ui.state.caps_lock || ui.state.num_lock => Status::Lock,
                "audio" => Status::Audio,
                "microphone" => Status::Microphone,
                "kbLayout" if ui.state.kb_layout.is_some() => Status::KbLayout,
                "network" => Status::Network,
                "bluetooth" if ui.state.bluetooth.is_some() => Status::Bluetooth,
                "battery" if ui.state.battery.is_some() => Status::Battery,
                _ => return None,
            })
        })
        .collect()
}

/// Height of one status row; the battery capsule is taller.
fn status_row(t: &crate::tokens::Tokens, status: Status) -> f32 {
    match status {
        Status::Battery => t.icon.small * 1.1 + t.spacing.extra_small + t.font.small * 1.3,
        _ => t.icon.small + t.spacing.small,
    }
}

fn status_height(ui: &Ui) -> f32 {
    let t = ui.t;
    let rows: f32 = statuses(ui).iter().map(|&s| status_row(t, s)).sum();
    rows.max(t.icon.small) + t.padding.medium * 2.0
}

fn network_icon(network: Option<Network>) -> char {
    match network {
        Some(Network::Ethernet) => icons::LAN,
        Some(Network::Wifi { bars, .. }) => match bars {
            0 => icons::SIGNAL_WIFI_0_BAR,
            1 => icons::NETWORK_WIFI_1_BAR,
            2 => icons::NETWORK_WIFI_2_BAR,
            3 => icons::NETWORK_WIFI_3_BAR,
            _ => icons::SIGNAL_WIFI_4_BAR,
        },
        Some(Network::Offline) | None => icons::WIFI_OFF,
    }
}

pub fn volume_icon(percent: f32, muted: bool) -> char {
    if muted {
        icons::VOLUME_OFF
    } else if percent >= 0.5 {
        icons::VOLUME_UP
    } else if percent > 0.0 {
        icons::VOLUME_DOWN
    } else {
        icons::VOLUME_MUTE
    }
}

/// The battery as a capsule filling with gradient liquid, its percentage
/// underneath.
fn battery_capsule(ui: &mut Ui, area: Rect, percent: u8, charging: bool) {
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, b) = accents(ui);
    let low = percent <= 15 && !charging;
    let (from, to) = if low { (pal.error, pal.error) } else { (b, a) };
    let body_h = t.icon.small * 1.1;
    let body_w = body_h * 0.52;
    let (cx, _) = area.center();
    let body = Rect::new(
        cx - body_w / 2.0,
        area.y + t.spacing.extra_small,
        body_w,
        body_h,
    );
    // Terminal nub, shell, then the liquid.
    ui.p.rect(
        Rect::new(cx - body_w * 0.2, body.y - 2.5, body_w * 0.4, 3.0),
        1.5,
        pal.outline,
    );
    ui.p.rect(body, body_w * 0.3, pal.surface_container_highest);
    let inner = body.inset(2.5);
    let level = inner.h * f32::from(percent.min(100)) / 100.0;
    if level > 0.5 {
        let liquid = Rect::new(inner.x, inner.bottom() - level, inner.w, level);
        ui.p.rect_gradient(liquid, (inner.w * 0.3).min(level / 2.0), from, to);
    }
    if charging {
        let (lx, ly) = body.center();
        ui.p.icon(
            icons::BATTERY_CHARGING_FULL,
            lx,
            ly,
            body_h * 0.62,
            1.0,
            pal.on_surface,
        );
    }
    let label = Rect::new(area.x, body.bottom() + 2.0, area.w, t.font.small * 1.3);
    ui.p.text(
        &percent.to_string(),
        label,
        TextStyle::new(t.font.small * 0.85).weight(700.0).center(),
        if low { pal.error } else { pal.on_surface },
    );
}

fn status_icons(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, _) = accents(ui);
    ui.p.rect(rect, t.full, ui.layer);
    let mut y = rect.y + t.padding.medium;
    let (cx, _) = rect.center();
    for status in statuses(ui) {
        let h = status_row(&t, status);
        let cell = Rect::new(rect.x, y, rect.w, h);
        let cy = y + t.icon.small / 2.0;
        match status {
            Status::Lock => {
                ui.p.icon(icons::KEYBOARD_CAPSLOCK, cx, cy, t.icon.small, 1.0, a);
            }
            Status::Audio => {
                let (percent, muted) = ui
                    .state
                    .volume
                    .map_or((0.0, true), |v| (f32::from(v.percent) / 100.0, v.muted));
                ui.p.icon(volume_icon(percent, muted), cx, cy, t.icon.small, 1.0, a);
                ui.click(cell, Action::ToggleMute);
            }
            Status::Microphone => {
                ui.p.icon(icons::MIC, cx, cy, t.icon.small, 1.0, a);
                ui.click(cell, Action::OpenSoundSettings);
            }
            Status::KbLayout => {
                let layout = ui.state.kb_layout.clone().unwrap_or_default();
                let short: String = layout.chars().take(2).collect::<String>().to_uppercase();
                ui.p.text(
                    &short,
                    cell,
                    TextStyle::new(t.font.smaller).weight(700.0).center().mono(),
                    a,
                );
            }
            Status::Network => {
                let online = !matches!(ui.state.network, Some(Network::Offline) | None);
                let colour = if online { a } else { pal.outline };
                let icon = network_icon(ui.state.network);
                ui.p.icon(icon, cx, cy, t.icon.small, 1.0, colour);
                ui.click(cell, Action::OpenNetworkSettings);
            }
            Status::Bluetooth => {
                let (icon, colour) = if ui.state.bluetooth == Some(true) {
                    (icons::BLUETOOTH, a)
                } else {
                    (icons::BLUETOOTH_DISABLED, pal.outline)
                };
                ui.p.icon(icon, cx, cy, t.icon.small, 1.0, colour);
                ui.click(cell, Action::OpenBluetoothSettings);
            }
            Status::Battery => {
                if let Some(battery) = ui.state.battery {
                    battery_capsule(ui, cell, battery.percent, battery.charging);
                    ui.click(cell, Action::OpenPowerSettings);
                }
            }
        }
        y += h;
    }
}

/// The power button: a gradient ring around a dark well with the power
/// symbol drawn in; red under the pointer. Opens the session menu. Drawn in
/// the launcher's search field (and in the bar if its `power` entry is on).
pub fn power_button(ui: &mut Ui, rect: Rect) {
    let pal = *ui.pal;
    let (a, b) = accents(ui);
    let hovered = ui.hovered(rect);
    ui.click(rect, Action::ToggleSession);
    let side = rect.w.min(rect.h) * 0.9;
    let shape = rect.centered(side, side);
    let (cx, cy) = shape.center();
    // A gradient ring around a dark well; under the pointer the well turns
    // to the error colour, warning that this ends the session.
    let (inner, ink, ink2) = if hovered {
        ui.p.glow(shape, side / 2.0, pal.error, 10.0);
        (pal.error_container, pal.error, pal.error)
    } else {
        (pal.surface_container_high, a, b)
    };
    ui.p.rect_gradient(shape, side / 2.0, a, b);
    ui.p.circle(cx, cy, side / 2.0 - 2.5, inner);
    // The power symbol, drawn: an open ring and a stem through its gap.
    let r = side * 0.24;
    let stroke = (side * 0.075).max(2.0);
    let gap = 0.75;
    ui.p.arc(
        cx,
        cy,
        r,
        gap,
        std::f32::consts::TAU - gap * 2.0,
        stroke,
        ink,
    );
    ui.p.rect(
        Rect::new(cx - stroke / 2.0, cy - r * 1.3, stroke, r * 1.1),
        stroke / 2.0,
        ink2,
    );
}

//! One monitor's shell: a click-through, always-on-top window covering the
//! monitor, the frame and bar drawn around its edge and the drawers that
//! grow out of it.
//!
//! Input reaches the window only where the surface is: everywhere else the
//! window is made transparent to the mouse (`WS_EX_TRANSPARENT`), so
//! clicks go straight to the applications underneath.

use std::time::Instant;

use anyhow::Result;
use norbanner_config::Config;
use norbanner_core::{
    anim::{Anim, Curve},
    geom::Rect,
    sdf::{Blob, Scene},
};
use norbanner_theme::Palette;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::HMONITOR,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT,
                TrackMouseEvent, VK_CONTROL, VK_SHIFT,
            },
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, LWA_ALPHA, LoadCursorW,
                MA_NOACTIVATE, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
                SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_KEYDOWN, WM_LBUTTONDOWN,
                WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
                WM_RBUTTONUP, WM_SYSKEYDOWN, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
                WS_POPUP,
            },
        },
    },
    core::w,
};

use crate::{
    events::{self, Event},
    modules::{
        Drawer, Edge, Placement,
        bar::{self, BarState},
        dashboard::{self, DashState},
        launcher::{self, LauncherState},
        lyrics,
        osd::{self, OsdState},
        place,
        session::{self, SessionState},
        window_list,
    },
    render::{BlobScene, Gpu, Surface},
    state::SystemState,
    tokens::Tokens,
    ui::{Ctx, Hit, HitKind, Ui},
};

const CLASS: windows::core::PCWSTR = w!("NorbannerShellSurface");
/// Not in `WindowsAndMessaging` (it lives with the common controls).
const WM_MOUSELEAVE: u32 = 0x02A3;

fn lo(lparam: LPARAM) -> i32 {
    i32::from((lparam.0 & 0xffff) as u16 as i16)
}

fn hi(lparam: LPARAM) -> i32 {
    i32::from(((lparam.0 >> 16) & 0xffff) as u16 as i16)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let id = hwnd.0 as isize;
    match msg {
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: Asks for WM_MOUSELEAVE on this window.
            unsafe {
                let _ = TrackMouseEvent(&mut track);
            }
            events::push(Event::MouseMove {
                hwnd: id,
                x: lo(lparam),
                y: hi(lparam),
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            // SAFETY: Keeps receiving the drag outside the window.
            unsafe { SetCapture(hwnd) };
            events::push(Event::MouseDown {
                hwnd: id,
                x: lo(lparam),
                y: hi(lparam),
                right: msg == WM_RBUTTONDOWN,
            });
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            // SAFETY: Ends the capture begun on button down.
            unsafe {
                let _ = ReleaseCapture();
            }
            events::push(Event::MouseUp {
                hwnd: id,
                x: lo(lparam),
                y: hi(lparam),
                right: msg == WM_RBUTTONUP,
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            events::push(Event::Wheel {
                hwnd: id,
                x: lo(lparam),
                y: hi(lparam),
                delta: i32::from(((wparam.0 >> 16) & 0xffff) as u16 as i16),
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            events::push(Event::MouseLeave { hwnd: id });
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // SAFETY: Reads modifier state for this key message.
            let (ctrl, shift) = unsafe {
                (
                    GetKeyState(i32::from(VK_CONTROL.0)) < 0,
                    GetKeyState(i32::from(VK_SHIFT.0)) < 0,
                )
            };
            events::push(Event::Key {
                vk: wparam.0 as u16,
                ctrl,
                shift,
            });
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(c) = char::from_u32(wparam.0 as u32)
                && !c.is_control()
            {
                events::push(Event::Char(c));
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xffff) as u32 == WA_INACTIVE {
                events::push(Event::Deactivated { hwnd: id });
            }
            LRESULT(0)
        }
        // Alt+F4 on a focused drawer must not close the shell.
        WM_CLOSE => LRESULT(0),
        // SAFETY: Default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn register_class() -> Result<()> {
    // SAFETY: Registers a window class with a static name and procedure.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&class);
    }
    Ok(())
}

/// A drag in progress on a slider.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub rect: Rect,
    pub slider: crate::ui::Slider,
}

pub struct Screen {
    pub hwnd: HWND,
    pub monitor: isize,
    /// The monitor in physical virtual-screen pixels.
    pub rect: RECT,
    /// Physical pixels per logical pixel.
    pub scale: f32,
    surface: Surface,
    pub dashboard: Drawer,
    pub launcher: Drawer,
    pub session: Drawer,
    pub osd: Drawer,
    /// The small lyrics island at the top.
    pub lyrics: Drawer,
    /// The panel listing an app's windows, beside its tile.
    pub windows: Drawer,
    /// Which app that panel lists, and the height it opens at.
    pub window_app: Option<(f32, crate::state::DesktopApp)>,
    /// The dashboard was opened by a shortcut or click, not hover: it stays
    /// until the pointer has been over it and left.
    pub dashboard_pinned: bool,
    pub bar_state: BarState,
    pub dash_state: DashState,
    pub osd_state: OsdState,
    /// Another monitor continues past this edge: the pointer crosses it on
    /// its way there, so hovering it must not open anything.
    pub right_shared: bool,
    pub top_shared: bool,
    /// The bar's width while it slides in and out (non-persistent bar).
    bar_width: Anim,
    pub bar_hovered: bool,
    /// 1 while a full-screen app is in front: the frame folds away.
    fullscreen: Anim,
    pub is_fullscreen: bool,
    pub hits: Vec<Hit>,
    /// The strips reserved along this monitor's edges.
    pub exclusive: crate::exclusive::Exclusive,
    /// Last frame's surface, in logical pixels, for hit testing.
    scene: Scene,
    pub placements: Placements,
    pub pointer: Option<(f32, f32)>,
    pub pressed: Option<crate::actions::Action>,
    pub drag: Option<Drag>,
    pub dirty: bool,
    transparent: bool,
    activatable: bool,
    hidden: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Placements {
    pub dashboard: Placement,
    pub launcher: Placement,
    pub session: Placement,
    pub osd: Placement,
    pub lyrics: Placement,
    pub windows: Placement,
    /// Where each drawer would be fully open (for hover triggers).
    pub dashboard_open: Rect,
    pub osd_open: Rect,
    pub bar_width: f32,
    pub border: f32,
    pub size: (f32, f32),
}

impl Screen {
    pub fn new(gpu: &Gpu, monitor: isize, rect: RECT, scale: f32) -> Result<Self> {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        // SAFETY: Creates a top-level popup of our registered class; the
        // handle is destroyed in Drop.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT,
                CLASS,
                w!("Norbanner"),
                WS_POPUP,
                rect.left,
                rect.top,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            // Layered (needed for click-through) at full opacity: what
            // shows is the composition swap chain's own alpha.
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
            hwnd
        };
        let surface = gpu.surface(hwnd, width as u32, height as u32)?;
        // SAFETY: Shows the window without taking the focus.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(Self {
            hwnd,
            monitor,
            rect,
            scale,
            surface,
            dashboard: Drawer::default(),
            launcher: Drawer::default(),
            session: Drawer::default(),
            osd: Drawer::default(),
            lyrics: Drawer::default(),
            windows: Drawer::default(),
            window_app: None,
            dashboard_pinned: false,
            bar_state: BarState::default(),
            dash_state: DashState::default(),
            osd_state: OsdState::default(),
            right_shared: false,
            top_shared: false,
            bar_width: Anim::new(-1.0),
            bar_hovered: false,
            fullscreen: Anim::new(0.0),
            is_fullscreen: false,
            hits: Vec::new(),
            exclusive: crate::exclusive::Exclusive::default(),
            scene: Scene::default(),
            placements: Placements::default(),
            pointer: None,
            pressed: None,
            drag: None,
            dirty: true,
            transparent: true,
            activatable: false,
            hidden: false,
        })
    }

    pub fn id(&self) -> isize {
        self.hwnd.0 as isize
    }

    /// Moves to a new monitor rectangle and scale.
    pub fn relocate(&mut self, gpu: &Gpu, rect: RECT, scale: f32) -> Result<()> {
        self.rect = rect;
        self.scale = scale;
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        // SAFETY: Moves our own window.
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                rect.left,
                rect.top,
                w,
                h,
                SWP_NOACTIVATE,
            )?;
        }
        self.surface.resize(gpu, w as u32, h as u32)?;
        self.dirty = true;
        Ok(())
    }

    pub fn logical_size(&self) -> (f32, f32) {
        (
            (self.rect.right - self.rect.left) as f32 / self.scale,
            (self.rect.bottom - self.rect.top) as f32 / self.scale,
        )
    }

    /// Converts a point in this window's client pixels to logical pixels.
    pub fn to_logical(&self, x: i32, y: i32) -> (f32, f32) {
        (x as f32 / self.scale, y as f32 / self.scale)
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, anim_scale: f32, now: Instant) {
        if self.is_fullscreen == fullscreen {
            return;
        }
        self.is_fullscreen = fullscreen;
        self.fullscreen.set(
            if fullscreen { 1.0 } else { 0.0 },
            Curve::Emphasized,
            anim_scale,
            now,
        );
        if fullscreen {
            self.dashboard.set(false, anim_scale, now);
            self.launcher.set(false, anim_scale, now);
            self.session.set(false, anim_scale, now);
            self.osd.set(false, anim_scale, now);
            self.lyrics.set(false, anim_scale, now);
            self.windows.set(false, anim_scale, now);
        } else if self.hidden {
            // SAFETY: Shows our own window again without focusing it.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.hidden = false;
        }
        self.dirty = true;
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.dashboard.animating(now)
            || self.launcher.animating(now)
            || self.session.animating(now)
            || self.osd.animating(now)
            || self.lyrics.animating(now)
            || self.windows.animating(now)
            || self.bar_width.animating(now)
            || self.fullscreen.animating(now)
            || self.bar_state.animating(now)
    }

    /// Whether the pointer at logical `(x, y)` belongs to the shell.
    pub fn owns(&self, x: f32, y: f32) -> bool {
        self.scene.contains(x, y) || self.hits.iter().any(|h| h.rect.contains(x, y))
    }

    /// Lets the mouse through everywhere except where the shell is.
    pub fn update_transparency(&mut self, cursor: Option<(f32, f32)>) {
        let owned = self.drag.is_some()
            || self.pressed.is_some()
            || cursor.is_some_and(|(x, y)| self.owns(x, y));
        let transparent = !owned;
        if transparent == self.transparent {
            return;
        }
        self.transparent = transparent;
        self.apply_styles();
    }

    /// Lets the window take the keyboard while the launcher or session
    /// menu is open.
    pub fn set_activatable(&mut self, activatable: bool) {
        if self.activatable == activatable {
            return;
        }
        self.activatable = activatable;
        self.apply_styles();
    }

    fn apply_styles(&self) {
        let mut style =
            WS_EX_NOREDIRECTIONBITMAP.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_LAYERED.0;
        if self.transparent {
            style |= WS_EX_TRANSPARENT.0;
        }
        if !self.activatable {
            style |= WS_EX_NOACTIVATE.0;
        }
        // SAFETY: Changes our own window's extended style.
        unsafe {
            if GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32 != style {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, style as isize);
            }
        }
    }

    /// Computes where everything goes this frame (logical pixels).
    pub fn layout(&mut self, ctx: &Ctx, launcher_state: &LauncherState, now: Instant) {
        let config = ctx.config;
        let t = ctx.t;
        let (w, h) = self.logical_size();
        let fs = self.fullscreen.value(now).clamp(0.0, 1.0);
        let border = config.border.thickness * (1.0 - fs);
        let full_bar = bar::width(t.bar_inner, config.border.thickness, t.padding.small);
        let bar_target = if config.bar.persistent || self.bar_hovered {
            full_bar
        } else {
            config.border.thickness
        } * (1.0 - fs);
        if self.bar_width.target() < 0.0 {
            self.bar_width.snap(bar_target);
        } else {
            let curve = if bar_target > self.bar_width.target() {
                Curve::ExpressiveDefaultSpatial
            } else {
                Curve::Emphasized
            };
            self.bar_width
                .set(bar_target, curve, config.appearance.anim_scale, now);
        }
        let bar_w = self.bar_width.value(now).max(border);
        let inner = Rect::new(
            bar_w,
            border,
            (w - bar_w - border).max(0.0),
            (h - border * 2.0).max(0.0),
        );
        let radius = t.rounding.extra_large;
        let k = config.border.smoothing;

        let (dw, dh) = dashboard::size(ctx);
        let (lw, lh) = launcher::size(ctx, launcher_state);
        let (sw, sh) = session::size(ctx);
        let (ow, oh) = osd::size(ctx, self.monitor);
        let (yw, yh) = lyrics::size(ctx);
        let center_x = inner.x + inner.w / 2.0;
        let center_y = inner.y + inner.h / 2.0;
        let session_p = self.session.progress(now);
        let session_place = place(inner, Edge::Right, center_y, sw, sh, session_p, radius, k);
        // The OSD sits beside the session menu when both are open.
        let osd_inner = Rect::new(
            inner.x,
            inner.y,
            inner.w - sw * session_p.clamp(0.0, 1.0),
            inner.h,
        );
        self.placements = Placements {
            dashboard: place(
                inner,
                Edge::Top,
                center_x,
                dw,
                dh,
                self.dashboard.progress(now),
                radius,
                k,
            ),
            launcher: place(
                inner,
                Edge::Bottom,
                center_x,
                lw,
                lh,
                self.launcher.progress(now),
                radius,
                k,
            ),
            session: session_place,
            lyrics: place(
                inner,
                Edge::Top,
                center_x,
                yw.min(inner.w * 0.6),
                yh,
                self.lyrics.progress(now),
                yh / 2.0,
                k,
            ),
            windows: {
                let app = self.window_app.as_ref().map(|(_, app)| app);
                let (ww, wh) = window_list::size(ctx, app);
                let along = self.window_app.as_ref().map_or(center_y, |(y, _)| *y);
                // The top of the panel lines up with the tile.
                place(
                    inner,
                    Edge::Left,
                    along - ctx.t.bar_inner * 0.4 + wh / 2.0,
                    ww,
                    wh,
                    self.windows.progress(now),
                    radius,
                    k,
                )
            },
            osd: place(
                osd_inner,
                Edge::Right,
                center_y,
                ow,
                oh,
                self.osd.progress(now),
                radius,
                k,
            ),
            dashboard_open: Rect::new(center_x - dw / 2.0, inner.y, dw, dh),
            osd_open: Rect::new(inner.right() - ow, center_y - oh / 2.0, ow, oh),
            bar_width: bar_w,
            border,
            size: (w, h),
        };
        let blobs: Vec<Blob> = [
            self.placements.dashboard,
            self.placements.launcher,
            self.placements.session,
            self.placements.osd,
            self.placements.lyrics,
            self.placements.windows,
        ]
        .iter()
        .filter_map(|p| p.blob)
        .collect();
        self.scene = Scene {
            screen: Rect::new(0.0, 0.0, w, h),
            inner,
            inner_radius: config.border.rounding * (1.0 - fs),
            smoothing: k,
            blobs,
        };
    }

    /// Draws a frame.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        config: &Config,
        tokens: &Tokens,
        palette: &Palette,
        state: &SystemState,
        launcher_state: &LauncherState,
        session_state: &SessionState,
        now: Instant,
    ) -> Result<()> {
        let ctx = Ctx {
            t: tokens,
            config,
            state,
        };
        self.layout(&ctx, launcher_state, now);
        self.dirty = false;

        let fs = self.fullscreen.value(now);
        if self.is_fullscreen && fs >= 0.999 && !self.animating(now) {
            if !self.hidden {
                // SAFETY: Hides our own window so the full-screen app can be
                // presented directly.
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                self.hidden = true;
            }
            return Ok(());
        }

        let transparency = &config.appearance.transparency;
        let surface_alpha = if transparency.enabled {
            transparency.base
        } else {
            1.0
        };
        let surface = palette.surface.with_alpha(surface_alpha);
        let layer = if state.backdrop.is_some() {
            // Cards over the glass let it through.
            palette.surface_container_highest.with_alpha(0.45)
        } else if transparency.enabled {
            palette
                .surface_container
                .with_alpha(transparency.layers.max(surface_alpha * 0.6))
        } else {
            palette.surface_container
        };
        let s = self.scale;
        let physical = Scene {
            screen: self.scene.screen.scale(s),
            inner: self.scene.inner.scale(s),
            inner_radius: self.scene.inner_radius * s,
            smoothing: self.scene.smoothing * s,
            blobs: self
                .scene
                .blobs
                .iter()
                .map(|b| Blob {
                    rect: b.rect.scale(s),
                    radius: b.radius * s,
                })
                .collect(),
        };
        let blob_scene = BlobScene {
            scene: physical,
            color: surface,
            shadow: palette.shadow,
            shadow_size: 14.0 * s,
            shadow_alpha: 0.55 * (1.0 - fs),
            // Frosted glass: the blurred wallpaper under the theme's surface.
            glass: state.backdrop.clone(),
            tint: 0.6,
        };
        let placements = self.placements;
        let pointer = self.pointer;
        let pressed = self.pressed.is_some() || self.drag.is_some();
        let mut hits = Vec::new();
        let bar_state = &mut self.bar_state;
        let dash_state = &self.dash_state;
        let monitor = self.monitor;
        let window_app = self.window_app.clone();
        gpu.frame(&mut self.surface, &blob_scene, 96.0 * s, |p| {
            let mut ui = Ui {
                p,
                pal: palette,
                t: tokens,
                config,
                state,
                hits: &mut hits,
                pointer,
                pressed,
                now,
                layer,
            };
            let (_, h) = placements.size;
            let bar_rect = Rect::new(0.0, 0.0, placements.bar_width, h);
            if placements.bar_width > config.border.thickness + 1.0 {
                let full = bar::width(
                    tokens.bar_inner,
                    config.border.thickness,
                    tokens.padding.small,
                );
                let reveal = ((placements.bar_width - config.border.thickness)
                    / (full - config.border.thickness).max(1.0))
                .clamp(0.0, 1.0);
                ui.p.clip(bar_rect);
                let full_rect = Rect::new(placements.bar_width - full, 0.0, full, h);
                let before = ui.p.opacity;
                ui.p.opacity = before * reveal;
                bar::draw(&mut ui, full_rect, bar_state, monitor);
                ui.p.opacity = before;
                ui.p.unclip();
            }
            drawer(&mut ui, &placements.lyrics, lyrics::draw);
            drawer(&mut ui, &placements.dashboard, |ui, r| {
                dashboard::draw(ui, r, dash_state)
            });
            if let Some(blob) = placements.launcher.blob {
                launcher::backdrop(&mut ui, blob, placements.launcher.content);
            }
            drawer(&mut ui, &placements.launcher, |ui, r| {
                launcher::draw(ui, r, launcher_state)
            });
            drawer(&mut ui, &placements.session, |ui, r| {
                session::draw(ui, r, session_state)
            });
            drawer(&mut ui, &placements.osd, |ui, r| osd::draw(ui, r, monitor));
            let window_app = window_app.as_ref().map(|(_, app)| app);
            drawer(&mut ui, &placements.windows, |ui, r| {
                window_list::draw(ui, r, window_app);
            });
        })?;
        self.hits = hits;
        Ok(())
    }

    /// The topmost hit region under the pointer.
    pub fn hit_at(&self, x: f32, y: f32, want: impl Fn(&HitKind) -> bool) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.rect.contains(x, y) && want(&h.kind))
    }
}

/// Draws a drawer's content clipped to the part that has come out.
fn drawer(ui: &mut Ui, placement: &Placement, f: impl FnOnce(&mut Ui, Rect)) {
    if placement.blob.is_none() || placement.clip.w < 1.0 || placement.clip.h < 1.0 {
        return;
    }
    ui.p.clip(placement.clip);
    let hits_before = ui.hits.len();
    f(ui, placement.content);
    // Only what shows can be clicked.
    let clip = placement.clip;
    for hit in &mut ui.hits[hits_before..] {
        let r = hit.rect;
        let x = r.x.max(clip.x);
        let y = r.y.max(clip.y);
        let right = r.right().min(clip.right());
        let bottom = r.bottom().min(clip.bottom());
        hit.rect = Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0));
    }
    ui.p.unclip();
}

impl Drop for Screen {
    fn drop(&mut self) {
        // SAFETY: Destroys our own window.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// The HMONITOR as an isize key.
pub fn monitor_key(monitor: HMONITOR) -> isize {
    monitor.0 as isize
}

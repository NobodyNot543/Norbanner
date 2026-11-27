//! Exclusive zones, as Hyprland gives layer surfaces: four invisible windows
//! per monitor registered as AppBars, one on each edge, as deep as the bar
//! (left) or the frame (top, right, bottom). Explorer then shrinks the work
//! area itself, so maximized windows, desktop icons and Windows' own panels
//! stay inside the frame. Explorer drops the reservation by itself when the
//! windows go away, including when the shell crashes.

use std::sync::atomic::{AtomicU32, Ordering};

use norbanner_shell::{
    appbar::{AppBar, Edge},
    workarea::Reserve,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::w,
};

use crate::events::{self, Event};

const CLASS: windows::core::PCWSTR = w!("NorbannerShellEdge");
/// The message Explorer sends AppBars (position changes, full screen).
pub const WM_APPBAR: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 30;

static REGISTERED: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn edge_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_APPBAR {
        events::push(Event::Service(WM_APPBAR));
        return LRESULT(0);
    }
    // SAFETY: Default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn register_class() {
    if REGISTERED.swap(1, Ordering::Relaxed) == 1 {
        return;
    }
    // SAFETY: Registers a class with a static name and procedure.
    unsafe {
        if let Ok(instance) = GetModuleHandleW(None) {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(edge_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..Default::default()
            });
        }
    }
}

struct Bar {
    edge: Edge,
    hwnd: HWND,
    appbar: Option<AppBar>,
}

impl Drop for Bar {
    fn drop(&mut self) {
        // Unregister before the window goes.
        self.appbar = None;
        // SAFETY: Destroys our own hidden window.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// One monitor's four edge reservations.
#[derive(Default)]
pub struct Exclusive {
    bars: Vec<Bar>,
    current: Option<(RECT, Reserve)>,
}

impl Exclusive {
    /// Reserves `reserve` (physical pixels) on the edges of `monitor`, or
    /// releases everything with `None`. `force` re-asserts an unchanged
    /// reservation (after Explorer moved bars around).
    pub fn set(&mut self, monitor: RECT, reserve: Option<Reserve>, force: bool) {
        let Some(reserve) = reserve else {
            self.bars.clear();
            self.current = None;
            return;
        };
        if !force && self.current == Some((monitor, reserve)) {
            return;
        }
        if self.bars.is_empty() {
            register_class();
            for edge in [Edge::Left, Edge::Top, Edge::Right, Edge::Bottom] {
                // SAFETY: A hidden, never-shown popup owned by this thread.
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                        CLASS,
                        w!("Norbanner edge"),
                        WS_POPUP,
                        0,
                        0,
                        0,
                        0,
                        None,
                        None,
                        GetModuleHandleW(None).ok().map(Into::into),
                        None,
                    )
                };
                let Ok(hwnd) = hwnd else { continue };
                let appbar = AppBar::register(hwnd, WM_APPBAR);
                self.bars.push(Bar { edge, hwnd, appbar });
            }
        }
        for bar in &mut self.bars {
            let depth = match bar.edge {
                Edge::Left => reserve.left,
                Edge::Top => reserve.top,
                Edge::Right => reserve.right,
                Edge::Bottom => reserve.bottom,
            };
            if let Some(appbar) = &mut bar.appbar {
                if force {
                    appbar.forget();
                }
                appbar.reserve(bar.edge, monitor, depth.max(1));
            }
        }
        self.current = Some((monitor, reserve));
    }
}

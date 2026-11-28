//! Windows' own volume and brightness indicator, hidden while Norbanner's
//! sliders show the level instead.
//!
//! Explorer creates the indicator when a level changes: a small XAML island
//! window (`XamlExplorerHostIslandWindow`) with no title, centred near the
//! bottom (or top) of the screen. Every other island of that class has a
//! title (Alt+Tab is "Task switching") or is larger, or sits in the middle
//! of the screen (the virtual desktop name).
//!
//! Hiding it once shown always leaves it on screen for a frame or two (a
//! flicker), so the shell also makes it fully transparent: Explorer keeps
//! the same window and shows it again for every change, still transparent,
//! so from then on nothing appears at all. It is made transparent as soon
//! as the shell finds it (at start, or the first time it shows), and given
//! back as it was when the sliders are turned off or the shell exits. It is
//! still hidden each time, so the invisible window never takes clicks.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                EnumWindows, FindWindowW, GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW,
                GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindow, LWA_ALPHA,
                SW_HIDE, SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindowAsync,
                WS_EX_LAYERED,
            },
        },
    },
    core::{BOOL, PCWSTR, w},
};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// The indicator made transparent, and whether it was layered before.
static VANISHED: Mutex<Option<(isize, bool)>> = Mutex::new(None);

/// Whether Windows' indicator is hidden (while Norbanner's sliders are on).
/// On, an indicator already there is made transparent now; off, it is
/// given back as it was.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        if let Some(hwnd) = find_indicator() {
            vanish(hwnd);
        }
    } else {
        restore();
    }
}

/// Hides `hwnd` if it is Windows' volume or brightness indicator, just
/// shown; returns whether it was.
pub fn hide_if_indicator(hwnd: HWND) -> bool {
    if !ENABLED.load(Ordering::Relaxed) || !is_indicator(hwnd) {
        return false;
    }
    // Transparent from now on: its next appearances show nothing at all.
    vanish(hwnd);
    // SAFETY: Asks another process's window to hide without waiting for it.
    unsafe { ShowWindowAsync(hwnd, SW_HIDE) }.as_bool()
}

/// Gives the indicator back as it was (the shell is exiting).
pub fn restore() {
    let Ok(mut vanished) = VANISHED.lock() else {
        return;
    };
    let Some((raw, was_layered)) = vanished.take() else {
        return;
    };
    let hwnd = HWND(raw as *mut _);
    // SAFETY: Style changes on another process's window, checked to still
    // exist; opaque again before the layered style is removed.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        if !was_layered {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
        }
    }
}

/// Makes the indicator opaque again without knowing what the shell did to
/// it: for the watchdog, after the shell has gone (Explorer's own indicator
/// is not layered).
pub fn give_back() {
    let Some(hwnd) = find_indicator() else {
        return;
    };
    // SAFETY: Style changes on another process's window, found just now.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED.0 as isize != 0 {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
        }
    }
}

/// Makes `hwnd` (the indicator) fully transparent, once.
fn vanish(hwnd: HWND) {
    let Ok(mut vanished) = VANISHED.lock() else {
        return;
    };
    if vanished.is_some_and(|(raw, _)| raw == hwnd.0 as isize) {
        return;
    }
    // A new indicator (Explorer restarted): the old one is gone.
    // SAFETY: Style changes on another process's window; a layered window
    // at alpha 0 draws nothing, its XAML content included.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let was_layered = ex & WS_EX_LAYERED.0 as isize != 0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
        if SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA).is_ok() {
            *vanished = Some((hwnd.0 as isize, was_layered));
        } else if !was_layered {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        }
    }
}

/// The indicator, if Explorer has made it already (it stays, hidden,
/// between changes).
fn find_indicator() -> Option<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the Option below, alive for the enumeration.
        let found = unsafe { &mut *(data.0 as *mut Option<HWND>) };
        if is_indicator(hwnd) {
            *found = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut found: Option<HWND> = None;
    // SAFETY: The callback only writes to `found`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&raw mut found as isize));
    }
    found
}

fn is_indicator(hwnd: HWND) -> bool {
    let mut class = [0u16; 64];
    // SAFETY: Read-only queries on a window handle; buffers have the sizes
    // passed.
    unsafe {
        let len = GetClassNameW(hwnd, &mut class) as usize;
        if String::from_utf16_lossy(&class[..len]) != "XamlExplorerHostIslandWindow"
            || GetWindowTextLengthW(hwnd) != 0
        {
            return false;
        }
        // Explorer's own, not another program's island.
        let (mut pid, mut explorer) = (0u32, 0u32);
        GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
        let Ok(tray) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) else {
            return false;
        };
        GetWindowThreadProcessId(tray, Some(&raw mut explorer));
        if pid == 0 || pid != explorer {
            return false;
        }
        let mut window = RECT::default();
        if GetWindowRect(hwnd, &raw mut window).is_err() {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &raw mut info).as_bool() {
            return false;
        }
        let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        looks_like_indicator(window, info.rcMonitor, scale)
    }
}

/// The indicator's shape and place: small, horizontally centred on the
/// screen (give or take the bar), near its bottom or top edge.
fn looks_like_indicator(window: RECT, screen: RECT, scale: f32) -> bool {
    let w = (window.right - window.left) as f32 / scale;
    let h = (window.bottom - window.top) as f32 / scale;
    if w <= 0.0 || h <= 0.0 || w > 480.0 || h > 160.0 {
        return false;
    }
    let screen_w = (screen.right - screen.left) as f32;
    let screen_h = (screen.bottom - screen.top) as f32;
    let cx = (window.left + window.right) as f32 / 2.0 - screen.left as f32;
    let cy = (window.top + window.bottom) as f32 / 2.0 - screen.top as f32;
    (cx - screen_w / 2.0).abs() < screen_w * 0.12 && (cy < screen_h * 0.25 || cy > screen_h * 0.75)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn recognises_the_indicator_and_nothing_else() {
        let screen = rect(0, 0, 1920, 1080);
        // As seen on Windows 11: above the bottom edge, centred on the space
        // beside the bar.
        assert!(looks_like_indicator(
            rect(870, 1008, 1104, 1070),
            screen,
            1.0
        ));
        // At the top, as Windows can be set to show it.
        assert!(looks_like_indicator(rect(843, 20, 1077, 82), screen, 1.0));
        // Same size at 150 %.
        assert!(looks_like_indicator(
            rect(1305, 1512, 1656, 1605),
            rect(0, 0, 2880, 1620),
            1.5
        ));
        // A second monitor on the left.
        assert!(looks_like_indicator(
            rect(-800, 690, -566, 752),
            rect(-1366, 0, 0, 768),
            1.0
        ));
        // The virtual desktop name, in the middle of the screen.
        assert!(!looks_like_indicator(
            rect(840, 500, 1080, 580),
            screen,
            1.0
        ));
        // A panel near a corner.
        assert!(!looks_like_indicator(
            rect(1500, 20, 1840, 170),
            screen,
            1.0
        ));
        // Full-screen islands (Alt+Tab).
        assert!(!looks_like_indicator(rect(0, 10, 1920, 1070), screen, 1.0));
    }
}

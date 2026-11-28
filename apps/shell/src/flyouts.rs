//! Windows' own volume and brightness indicator, hidden while Norbanner's
//! sliders show the level instead.
//!
//! Explorer creates the indicator when a level changes: a small XAML island
//! window (`XamlExplorerHostIslandWindow`) with no title, centred near the
//! bottom (or top) of the screen. Every other island of that class has a
//! title (Alt+Tab is "Task switching") or is larger, or sits in the middle
//! of the screen (the virtual desktop name). The shell's window-event hook
//! sees it appear and hides it before it has been on screen long; Explorer
//! shows it again for the next change, and it is hidden again.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                FindWindowW, GetClassNameW, GetWindowRect, GetWindowTextLengthW,
                GetWindowThreadProcessId, SW_HIDE, ShowWindowAsync,
            },
        },
    },
    core::{PCWSTR, w},
};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether Windows' indicator is hidden (while Norbanner's sliders are on).
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Hides `hwnd` if it is Windows' volume or brightness indicator, just
/// shown; returns whether it was.
pub fn hide_if_indicator(hwnd: HWND) -> bool {
    if !ENABLED.load(Ordering::Relaxed) || !is_indicator(hwnd) {
        return false;
    }
    // SAFETY: Asks another process's window to hide without waiting for it.
    unsafe { ShowWindowAsync(hwnd, SW_HIDE) }.as_bool()
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

//! Keeps maximized windows inside the shell's frame by shrinking each
//! monitor's work area, the way Hyprland reserves a layer surface's
//! exclusive zone.
//!
//! The first time a monitor's work area is changed, its original is saved to
//! `%LOCALAPPDATA%\Norbanner\workarea-original`, so it can be put back exactly
//! even after a crash (the watchdog calls [`restore_all`]). Restore it
//! *before* giving the taskbar back: Explorer then reserves the taskbar's
//! strip itself.

use std::{env, fs, path::PathBuf};

use windows::Win32::{
    Foundation::{POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint},
    UI::WindowsAndMessaging::{SPI_SETWORKAREA, SPIF_SENDCHANGE, SystemParametersInfoW},
};

/// Insets reserved from each edge of a monitor, in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reserve {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

fn state_file() -> PathBuf {
    PathBuf::from(env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("Norbanner")
        .join("workarea-original")
}

fn encode(monitor: RECT, work: RECT) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        monitor.left,
        monitor.top,
        monitor.right,
        monitor.bottom,
        work.left,
        work.top,
        work.right,
        work.bottom
    )
}

fn decode(line: &str) -> Option<(RECT, RECT)> {
    let n: Vec<i32> = line
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    let [a, b, c, d, e, f, g, h] = n.as_slice().try_into().ok()?;
    Some((
        RECT {
            left: a,
            top: b,
            right: c,
            bottom: d,
        },
        RECT {
            left: e,
            top: f,
            right: g,
            bottom: h,
        },
    ))
}

fn saved() -> Vec<(RECT, RECT)> {
    fs::read_to_string(state_file())
        .map(|text| text.lines().filter_map(decode).collect())
        .unwrap_or_default()
}

fn set(mut area: RECT) {
    // SAFETY: `area` is a valid RECT for the duration of the call. The
    // change is broadcast so maximized windows resize into the new area.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_SETWORKAREA,
            0,
            Some(std::ptr::from_mut(&mut area).cast()),
            SPIF_SENDCHANGE,
        );
    }
}

/// The work area Windows currently reports for the monitor at `monitor`.
fn current(monitor: RECT) -> Option<RECT> {
    // SAFETY: Read-only monitor query with a correctly sized struct.
    unsafe {
        let handle = MonitorFromPoint(
            POINT {
                x: monitor.left,
                y: monitor.top,
            },
            MONITOR_DEFAULTTONULL,
        );
        if handle.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(handle, &mut info)
            .as_bool()
            .then_some(info.rcWork)
    }
}

/// Sets the work area of the monitor whose rectangle is `monitor`.
pub fn reserve(monitor: RECT, reserve: Reserve) {
    let area = RECT {
        left: monitor.left + reserve.left,
        top: monitor.top + reserve.top,
        right: monitor.right - reserve.right,
        bottom: monitor.bottom - reserve.bottom,
    };
    let Some(now) = current(monitor) else { return };
    if now == area {
        return;
    }
    let mut originals = saved();
    if !originals.iter().any(|(m, _)| *m == monitor) {
        originals.push((monitor, now));
        let text: Vec<String> = originals.iter().map(|(m, w)| encode(*m, *w)).collect();
        let file = state_file();
        let written = file
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&file, text.join("\n")));
        if let Err(error) = written {
            // Never change what cannot be put back.
            crate::log::line(&format!("could not save the work area: {error}"));
            return;
        }
    }
    set(area);
}

/// Puts back every work area changed by [`reserve`]. Does nothing if none
/// was changed.
pub fn restore_all() {
    let originals = saved();
    for (monitor, work) in &originals {
        if current(*monitor).is_some() {
            set(*work);
        }
    }
    let _ = fs::remove_file(state_file());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_saved_areas() {
        let monitor = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1032,
        };
        assert_eq!(decode(&encode(monitor, work)), Some((monitor, work)));
        assert_eq!(decode("1 2 3"), None);
    }
}

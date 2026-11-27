//! Screen brightness through DDC/CI, the protocol external monitors speak
//! over their video cable (what `ddcutil` does on Linux). Built-in laptop
//! panels are not DDC/CI devices; for them Windows' own brightness keys and
//! quick settings keep working.
//!
//! DDC/CI is slow (tens of milliseconds per request), so call these off the
//! UI thread.

use windows::Win32::{
    Devices::Display::{
        DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
    },
    Graphics::Gdi::HMONITOR,
};

struct Physical(Vec<PHYSICAL_MONITOR>);

impl Drop for Physical {
    fn drop(&mut self) {
        for monitor in &self.0 {
            // SAFETY: Each handle came from GetPhysicalMonitorsFromHMONITOR.
            unsafe {
                let _ = DestroyPhysicalMonitor(monitor.hPhysicalMonitor);
            }
        }
    }
}

fn physical(monitor: isize) -> Option<Physical> {
    let handle = HMONITOR(monitor as *mut _);
    let mut count = 0u32;
    // SAFETY: Count first, then fill a buffer of that length.
    unsafe {
        GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut count).ok()?;
        if count == 0 {
            return None;
        }
        let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
        GetPhysicalMonitorsFromHMONITOR(handle, &mut monitors).ok()?;
        Some(Physical(monitors))
    }
}

/// Brightness of the monitor `HMONITOR`, 0–1, if it supports DDC/CI.
#[must_use]
pub fn get(monitor: isize) -> Option<f32> {
    let monitors = physical(monitor)?;
    let first = monitors.0.first()?;
    let (mut min, mut current, mut max) = (0u32, 0u32, 0u32);
    // SAFETY: Out-pointers are valid; the handle is alive until `monitors` drops.
    unsafe {
        if GetMonitorBrightness(first.hPhysicalMonitor, &mut min, &mut current, &mut max) == 0 {
            return None;
        }
    }
    (max > min).then(|| (current.saturating_sub(min)) as f32 / (max - min) as f32)
}

/// Sets every physical monitor behind `HMONITOR` to `value` (0–1).
pub fn set(monitor: isize, value: f32) -> bool {
    let Some(monitors) = physical(monitor) else {
        return false;
    };
    let mut any = false;
    for physical in &monitors.0 {
        let (mut min, mut current, mut max) = (0u32, 0u32, 0u32);
        // SAFETY: As in `get`.
        unsafe {
            if GetMonitorBrightness(physical.hPhysicalMonitor, &mut min, &mut current, &mut max)
                == 0
                || max <= min
            {
                continue;
            }
            let level = min + ((max - min) as f32 * value.clamp(0.0, 1.0)).round() as u32;
            any |= SetMonitorBrightness(physical.hPhysicalMonitor, level) != 0;
        }
    }
    any
}

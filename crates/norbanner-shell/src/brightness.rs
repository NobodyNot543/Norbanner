//! Screen brightness through DDC/CI, the protocol external monitors speak
//! over their video cable (what `ddcutil` does on Linux), and for a
//! laptop's built-in panel, which is not a DDC/CI device, through Windows'
//! LCD device (the same control as the brightness keys).
//!
//! DDC/CI is slow (tens of milliseconds per request), so call these off the
//! UI thread.

use windows::Win32::{
    Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
        DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DestroyPhysicalMonitor,
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, GetMonitorBrightness,
        GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
        QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig, SetMonitorBrightness,
    },
    Foundation::ERROR_SUCCESS,
    Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO, MONITORINFOEXW},
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

/// Brightness of the monitor `HMONITOR`, 0–1, if it can be read: DDC/CI,
/// or the LCD device for the built-in panel.
#[must_use]
pub fn get(monitor: isize) -> Option<f32> {
    ddc_get(monitor).or_else(|| {
        if is_built_in(monitor) {
            lcd::get()
        } else {
            None
        }
    })
}

/// Sets the monitor `HMONITOR` to `value` (0–1).
pub fn set(monitor: isize, value: f32) -> bool {
    ddc_set(monitor, value) || (is_built_in(monitor) && lcd::set(value))
}

fn ddc_get(monitor: isize) -> Option<f32> {
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
fn ddc_set(monitor: isize, value: f32) -> bool {
    let Some(monitors) = physical(monitor) else {
        return false;
    };
    let mut any = false;
    for physical in &monitors.0 {
        let (mut min, mut current, mut max) = (0u32, 0u32, 0u32);
        // SAFETY: As in `ddc_get`.
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

/// Whether `HMONITOR` shows the computer's built-in panel: Windows' display
/// configuration names the output technology of each active screen.
fn is_built_in(monitor: isize) -> bool {
    // SAFETY: Out-pointers and arrays have the sizes passed.
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(
            HMONITOR(monitor as *mut _),
            (&raw mut info).cast::<MONITORINFO>(),
        )
        .as_bool()
        {
            return false;
        }
        let (mut paths, mut modes) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &raw mut paths, &raw mut modes)
            != ERROR_SUCCESS
        {
            return false;
        }
        let mut path_list = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
        let mut mode_list = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &raw mut paths,
            path_list.as_mut_ptr(),
            &raw mut modes,
            mode_list.as_mut_ptr(),
            None,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        path_list.truncate(paths as usize);
        path_list.iter().any(|path| {
            let technology = path.targetInfo.outputTechnology;
            if technology != DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                && technology != DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                && technology != DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
            {
                return false;
            }
            let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            name.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            name.header.adapterId = path.sourceInfo.adapterId;
            name.header.id = path.sourceInfo.id;
            DisplayConfigGetDeviceInfo(&raw mut name.header) == 0
                && name.viewGdiDeviceName == info.szDevice
        })
    }
}

/// The built-in panel through the LCD device (video IOCTLs, levels 0–100).
mod lcd {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            Storage::FileSystem::{
                CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                OPEN_EXISTING,
            },
            System::IO::DeviceIoControl,
        },
        core::w,
    };

    /// IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS.
    const QUERY: u32 = 0x0023_0498;
    /// IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS.
    const SET: u32 = 0x0023_049C;
    /// DISPLAYPOLICY_BOTH: on battery and plugged in alike.
    const BOTH: u8 = 3;
    const GENERIC_READ_WRITE: u32 = 0x8000_0000 | 0x4000_0000;

    struct Device(HANDLE);

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: The handle came from CreateFileW and is closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn open() -> Option<Device> {
        // SAFETY: Opens the LCD device; the handle is owned by `Device`.
        unsafe {
            CreateFileW(
                w!(r"\\.\LCD"),
                GENERIC_READ_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
            .ok()
            .map(Device)
        }
    }

    pub fn get() -> Option<f32> {
        let device = open()?;
        // DISPLAY_BRIGHTNESS: policy, plugged-in level, battery level.
        let mut current = [0u8; 3];
        let mut returned = 0u32;
        // SAFETY: The output buffer has the size passed.
        unsafe {
            DeviceIoControl(
                device.0,
                QUERY,
                None,
                0,
                Some(current.as_mut_ptr().cast()),
                3,
                Some(&raw mut returned),
                None,
            )
            .ok()?;
        }
        (returned >= 3).then(|| f32::from(current[1].max(current[2]).min(100)) / 100.0)
    }

    pub fn set(value: f32) -> bool {
        let Some(device) = open() else {
            return false;
        };
        let level = (value.clamp(0.0, 1.0) * 100.0).round() as u8;
        let request = [BOTH, level, level];
        // SAFETY: The input buffer has the size passed.
        unsafe {
            DeviceIoControl(
                device.0,
                SET,
                Some(request.as_ptr().cast()),
                3,
                None,
                0,
                None,
                None,
            )
            .is_ok()
        }
    }
}

//! Windows virtual desktops as the bar's workspaces.
//!
//! Windows offers no documented API to list or switch desktops, but Explorer
//! keeps their ids and the current one in the registry, and the documented
//! `IVirtualDesktopManager` says which desktop a window is on. Switching
//! sends the system shortcut (Ctrl+Win+Left/Right), which every Windows 10
//! and 11 build understands; creating sends Ctrl+Win+D.

use windows::{
    Win32::{
        Foundation::HWND,
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegGetValueW},
            RemoteDesktop::ProcessIdToSessionId,
        },
        UI::{
            Input::KeyboardAndMouse::{VK_CONTROL, VK_D, VK_LEFT, VK_LWIN, VK_RIGHT},
            Shell::{IVirtualDesktopManager, VirtualDesktopManager},
        },
    },
    core::{GUID, HSTRING, PCWSTR, w},
};

/// Registry key (under HKCU) whose changes mean the desktops changed.
pub const WATCH_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desktop {
    pub id: GUID,
    /// The name the user gave it, if any.
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Desktops {
    pub list: Vec<Desktop>,
    /// Index of the current desktop in `list`.
    pub current: usize,
}

fn read_binary(key: &str, value: PCWSTR) -> Option<Vec<u8>> {
    let key = HSTRING::from(key);
    let mut size = 0u32;
    // SAFETY: First call only measures; second fills a buffer of that size.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            value,
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut data = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            value,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        data.truncate(size as usize);
        Some(data)
    }
}

fn read_string(key: &str, value: PCWSTR) -> Option<String> {
    let key = HSTRING::from(key);
    let mut buffer = [0u16; 260];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: The buffer and its byte size are valid.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..len]);
    (!text.is_empty()).then_some(text)
}

fn guids(bytes: &[u8]) -> Vec<GUID> {
    bytes
        .as_chunks::<16>()
        .0
        .iter()
        .map(|raw| {
            GUID::from_values(
                u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
                u16::from_le_bytes([raw[4], raw[5]]),
                u16::from_le_bytes([raw[6], raw[7]]),
                [
                    raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
                ],
            )
        })
        .collect()
}

fn braced(id: &GUID) -> String {
    format!("{{{id:?}}}").to_uppercase()
}

/// The desktops and which one is current. With a single desktop Windows
/// may not have written the list yet; that reads as one desktop.
#[must_use]
pub fn read() -> Desktops {
    let ids = read_binary(KEY, w!("VirtualDesktopIDs"))
        .map(|bytes| guids(&bytes))
        .unwrap_or_default();
    // Windows 11 keeps the current desktop next to the list; Windows 10 in
    // the session's own key.
    let current = read_binary(KEY, w!("CurrentVirtualDesktop")).or_else(|| {
        let mut session = 0u32;
        // SAFETY: Out-pointer is a valid u32.
        unsafe { ProcessIdToSessionId(std::process::id(), &mut session) }.ok()?;
        read_binary(
            &format!(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\SessionInfo\\{session}\\VirtualDesktops"
            ),
            w!("CurrentVirtualDesktop"),
        )
    });
    let current = current.and_then(|bytes| guids(&bytes).first().copied());
    if ids.is_empty() {
        return Desktops {
            list: vec![Desktop {
                id: current.unwrap_or_default(),
                name: None,
            }],
            current: 0,
        };
    }
    let list: Vec<Desktop> = ids
        .iter()
        .map(|id| Desktop {
            id: *id,
            name: read_string(&format!("{KEY}\\Desktops\\{}", braced(id)), w!("Name")),
        })
        .collect();
    let current = current
        .and_then(|current| ids.iter().position(|id| *id == current))
        .unwrap_or(0);
    Desktops { list, current }
}

/// Asks which desktop windows are on. Create once per UI thread (COM must
/// be initialized there).
pub struct WindowDesktops(IVirtualDesktopManager);

impl WindowDesktops {
    #[must_use]
    pub fn new() -> Option<Self> {
        // SAFETY: Creates the documented in-process shell object.
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_INPROC_SERVER) }
            .ok()
            .map(Self)
    }

    /// The desktop `window` belongs to; `None` for windows on every desktop
    /// or that Windows does not place on one.
    #[must_use]
    pub fn desktop_of(&self, window: isize) -> Option<GUID> {
        // SAFETY: Read-only query for a window handle.
        unsafe { self.0.GetWindowDesktopId(HWND(window as *mut _)) }
            .ok()
            .filter(|id| *id != GUID::zeroed())
    }

    #[must_use]
    pub fn is_on_current(&self, window: isize) -> bool {
        // SAFETY: Read-only query for a window handle.
        unsafe {
            self.0
                .IsWindowOnCurrentVirtualDesktop(HWND(window as *mut _))
        }
        .map_or(true, |on| on.as_bool())
    }
}

/// Moves `steps` desktops right (positive) or left (negative).
pub fn step(steps: i32) {
    let arrow = if steps > 0 { VK_RIGHT } else { VK_LEFT };
    for _ in 0..steps.unsigned_abs().min(20) {
        crate::launcher::tap(&[VK_CONTROL, VK_LWIN, arrow]);
    }
}

/// Switches to desktop `index`, creating desktops if needed up to it.
pub fn go_to(index: usize) {
    let desktops = read();
    let mut count = desktops.list.len();
    while count <= index && count < 20 {
        // Creating a desktop also switches to it.
        crate::launcher::tap(&[VK_CONTROL, VK_LWIN, VK_D]);
        count += 1;
    }
    let current = if count == desktops.list.len() {
        desktops.current
    } else {
        count - 1
    };
    step(index as i32 - current as i32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_guids() {
        let id = GUID::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&id.data1.to_le_bytes());
        bytes.extend_from_slice(&id.data2.to_le_bytes());
        bytes.extend_from_slice(&id.data3.to_le_bytes());
        bytes.extend_from_slice(&id.data4);
        assert_eq!(guids(&bytes), vec![id]);
    }

    #[test]
    fn always_reports_a_desktop() {
        let desktops = read();
        assert!(!desktops.list.is_empty());
        assert!(desktops.current < desktops.list.len());
    }
}

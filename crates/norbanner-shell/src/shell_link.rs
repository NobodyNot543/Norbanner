//! How other Norbanner processes (Settings, `norbanner shell ...` from a terminal,
//! a keyboard macro tool) talk to the running shell.
//!
//! Commands travel as UTF-8 text in a `WM_COPYDATA` message to the shell's
//! hidden control window, so a command line like
//! `norbanner shell drawers toggle launcher` works from any script.

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::DataExchange::COPYDATASTRUCT,
        UI::WindowsAndMessaging::{
            FindWindowW, PostMessageW, RegisterWindowMessageW, SMTO_ABORTIFHUNG,
            SendMessageTimeoutW, WM_COPYDATA,
        },
    },
    core::{PCWSTR, w},
};

/// Class of the shell's hidden control window.
pub const WINDOW_CLASS: PCWSTR = w!("NorbannerShellControl");
/// Registered message: "the configuration file changed".
pub const CONFIG_CHANGED: PCWSTR = w!("Norbanner.ConfigChanged");
/// `dwData` that marks a `WM_COPYDATA` as a shell command.
pub const COMMAND_TAG: usize = 0x4d55_454c; // "MUEL"

/// The control window of the shell running in this session.
#[must_use]
pub fn window() -> Option<HWND> {
    // SAFETY: Read-only window lookup.
    unsafe { FindWindowW(WINDOW_CLASS, None).ok() }
}

#[must_use]
pub fn is_running() -> bool {
    window().is_some()
}

/// Asks the shell, if running, to reload `shell.json`.
pub fn notify_config_changed() {
    if let Some(shell) = window() {
        // SAFETY: Posts a message without associated data.
        unsafe {
            let message = RegisterWindowMessageW(CONFIG_CHANGED);
            let _ = PostMessageW(Some(shell), message, WPARAM(0), LPARAM(0));
        }
    }
}

/// Sends `command` (for example `drawers toggle launcher`) to the running
/// shell. Returns the shell's reply code: 0 unknown command, 1 done; `None`
/// when no shell is running or it did not answer within a second.
pub fn send(command: &str) -> Option<isize> {
    let shell = window()?;
    let bytes = command.as_bytes();
    let data = COPYDATASTRUCT {
        dwData: COMMAND_TAG,
        cbData: u32::try_from(bytes.len()).ok()?,
        lpData: bytes.as_ptr() as *mut core::ffi::c_void,
    };
    let mut result = 0usize;
    // SAFETY: `data` and `bytes` outlive the synchronous call; the receiver
    // copies the text before returning.
    let sent = unsafe {
        SendMessageTimeoutW(
            shell,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(std::ptr::from_ref(&data) as isize),
            SMTO_ABORTIFHUNG,
            1000,
            Some(&mut result),
        )
    };
    (sent.0 != 0).then_some(result as isize)
}

/// Reads a command out of a received `WM_COPYDATA`.
///
/// # Safety
/// `lparam` must be the `LPARAM` of a `WM_COPYDATA` being handled.
#[must_use]
pub unsafe fn read_command(lparam: LPARAM) -> Option<String> {
    // SAFETY: Guaranteed by the caller; the sender keeps the data alive
    // until the message returns.
    unsafe {
        let data = (lparam.0 as *const COPYDATASTRUCT).as_ref()?;
        if data.dwData != COMMAND_TAG || data.lpData.is_null() {
            return None;
        }
        let len = (data.cbData as usize).min(4096);
        let bytes = std::slice::from_raw_parts(data.lpData as *const u8, len);
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

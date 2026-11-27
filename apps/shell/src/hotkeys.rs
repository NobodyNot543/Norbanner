//! Global shortcuts.
//!
//! Tapping the Windows key on its own toggles the launcher, as tapping
//! Super does on Caelestia. A low-level keyboard hook sees the key go down
//! and up with nothing in between; it then sends an unassigned key before
//! the release reaches Explorer, so Explorer sees "Windows + something" and
//! does not open Start. Every other Windows shortcut keeps working.
//!
//! Other shortcuts (`alt+space`, `ctrl+alt+end` …) use `RegisterHotKey`.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
            RegisterHotKey, SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_CAPITAL, VK_LWIN,
            VK_NUMLOCK, VK_RWIN,
        },
        WindowsAndMessaging::{
            CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, SetWindowsHookExW,
            UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

use crate::events::{self, Event};

/// An unassigned virtual key that masks the Windows key's release.
const MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

static SUPER_TAP: AtomicBool = AtomicBool::new(false);
static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static WIN_ALONE: AtomicBool = AtomicBool::new(false);
/// Workspace (1-based) asked for with Win+digit, switched to once Windows
/// key is released (switching sends Ctrl+Win+arrows, which must not mix
/// with the Win key still held). 0 = none.
static PENDING_WORKSPACE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The digit key whose release must be swallowed too.
static SWALLOWED: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

pub const HOTKEY_LAUNCHER: i32 = 1;
pub const HOTKEY_SESSION: i32 = 2;

pub struct Hooks {
    hook: Option<HHOOK>,
    hwnd: HWND,
    registered: Vec<i32>,
}

fn send_mask() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: MASK_KEY,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    // SAFETY: The array lives for the duration of the call.
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: For HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
        let vk = VIRTUAL_KEY(info.vkCode as u16);
        let message = wparam.0 as u32;
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if !injected {
            let is_win = vk == VK_LWIN || vk == VK_RWIN;
            if is_win && down {
                if !WIN_DOWN.swap(true, Ordering::Relaxed) {
                    WIN_ALONE.store(true, Ordering::Relaxed);
                }
            } else if is_win && up {
                WIN_DOWN.store(false, Ordering::Relaxed);
                let workspace = PENDING_WORKSPACE.swap(0, Ordering::Relaxed);
                if workspace > 0 {
                    // Mask the release so Explorer does not open Start.
                    send_mask();
                    events::push(Event::Workspace(workspace - 1));
                } else if WIN_ALONE.swap(false, Ordering::Relaxed)
                    && SUPER_TAP.load(Ordering::Relaxed)
                {
                    send_mask();
                    events::push(Event::SuperTap);
                }
            } else if down {
                WIN_ALONE.store(false, Ordering::Relaxed);
                // Win+1 … Win+9: workspaces, as Super+N on Hyprland, instead
                // of Windows' "open the Nth taskbar app".
                if WIN_DOWN.load(Ordering::Relaxed) && (0x31..=0x39).contains(&vk.0) {
                    PENDING_WORKSPACE.store(usize::from(vk.0 - 0x30), Ordering::Relaxed);
                    SWALLOWED.store(vk.0, Ordering::Relaxed);
                    return LRESULT(1);
                }
            } else if up && vk.0 != 0 && SWALLOWED.load(Ordering::Relaxed) == vk.0 {
                SWALLOWED.store(0, Ordering::Relaxed);
                return LRESULT(1);
            }
            if up && (vk == VK_CAPITAL || vk == VK_NUMLOCK) {
                events::push(Event::Service(crate::app::WM_LOCK_KEYS));
            }
        }
    }
    // SAFETY: Passes the event on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Parses `ctrl+alt+end` style shortcuts.
fn parse(spec: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut modifiers = MOD_NOREPEAT;
    let mut key = None;
    for part in spec.to_ascii_lowercase().split('+').map(str::trim) {
        match part {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "super" | "meta" => modifiers |= MOD_WIN,
            "space" => key = Some(0x20),
            "enter" | "return" => key = Some(0x0D),
            "end" => key = Some(0x23),
            "home" => key = Some(0x24),
            "delete" | "del" => key = Some(0x2E),
            "insert" | "ins" => key = Some(0x2D),
            "escape" | "esc" => key = Some(0x1B),
            "tab" => key = Some(0x09),
            f if f.len() >= 2
                && f.starts_with('f')
                && f[1..].parse::<u32>().is_ok_and(|n| (1..=24).contains(&n)) =>
            {
                key = f[1..].parse::<u32>().ok().map(|n| 0x6F + n);
            }
            c if c.len() == 1 => {
                let ch = c.chars().next()?.to_ascii_uppercase();
                if ch.is_ascii_alphanumeric() {
                    key = Some(ch as u32);
                }
            }
            _ => return None,
        }
    }
    key.map(|key| (modifiers, key))
}

impl Hooks {
    pub fn new(hwnd: HWND) -> Self {
        // SAFETY: A low-level hook needs no module; it runs on this thread,
        // which pumps messages.
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) }.ok();
        Self {
            hook,
            hwnd,
            registered: Vec::new(),
        }
    }

    /// Applies the launcher and session shortcuts from the configuration.
    pub fn configure(&mut self, launcher: &str, session: &str) {
        for id in self.registered.drain(..) {
            // SAFETY: Unregisters a hotkey this window registered.
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), id);
            }
        }
        let launcher = launcher.trim().to_ascii_lowercase();
        SUPER_TAP.store(launcher == "super", Ordering::Relaxed);
        for (id, spec) in [
            (HOTKEY_LAUNCHER, launcher.as_str()),
            (HOTKEY_SESSION, session),
        ] {
            if spec == "super" || spec == "none" || spec.is_empty() {
                continue;
            }
            match parse(spec) {
                // SAFETY: Registers a hotkey for this window.
                Some((modifiers, key)) => unsafe {
                    if RegisterHotKey(Some(self.hwnd), id, modifiers, key).is_ok() {
                        self.registered.push(id);
                    } else {
                        norbanner_shell::log::line(&format!(
                            "shortcut `{spec}` is taken by another app"
                        ));
                    }
                },
                None => norbanner_shell::log::line(&format!("could not read shortcut `{spec}`")),
            }
        }
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        self.configure("none", "none");
        if let Some(hook) = self.hook.take() {
            // SAFETY: Removes the hook installed in `new`.
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        assert_eq!(parse("alt+space"), Some((MOD_NOREPEAT | MOD_ALT, 0x20)));
        assert_eq!(
            parse("ctrl+alt+end"),
            Some((MOD_NOREPEAT | MOD_CONTROL | MOD_ALT, 0x23))
        );
        assert_eq!(parse("win+f5"), Some((MOD_NOREPEAT | MOD_WIN, 0x74)));
        assert_eq!(
            parse("ctrl+x"),
            Some((MOD_NOREPEAT | MOD_CONTROL, u32::from(b'X')))
        );
        assert_eq!(parse("ctrl+banana"), None);
    }
}

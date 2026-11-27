//! The session menu's actions: lock, sign out, sleep, hibernate, restart and
//! shut down.

use std::{os::windows::process::CommandExt, process::Command};

use windows::Win32::System::{
    Power::SetSuspendState,
    Shutdown::{EWX_LOGOFF, ExitWindowsEx, LockWorkStation, SHTDN_REASON_NONE},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lock,
    Logout,
    Sleep,
    Hibernate,
    Reboot,
    Shutdown,
}

impl Action {
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "lock" => Self::Lock,
            "logout" => Self::Logout,
            "sleep" | "suspend" => Self::Sleep,
            "hibernate" => Self::Hibernate,
            "reboot" | "restart" => Self::Reboot,
            "shutdown" | "poweroff" => Self::Shutdown,
            _ => return None,
        })
    }
}

/// Runs `action`. Restart and shut down go through `shutdown.exe`, which
/// holds the privilege Windows requires and lets apps save their work.
pub fn run(action: Action) -> windows::core::Result<()> {
    // SAFETY: Each call only asks Windows to change the session state.
    unsafe {
        match action {
            Action::Lock => LockWorkStation(),
            Action::Logout => ExitWindowsEx(EWX_LOGOFF, SHTDN_REASON_NONE),
            Action::Sleep => {
                let _ = SetSuspendState(false, false, false);
                Ok(())
            }
            Action::Hibernate => {
                let _ = SetSuspendState(true, false, false);
                Ok(())
            }
            Action::Reboot => shutdown(&["/r", "/t", "0"]),
            Action::Shutdown => shutdown(&["/s", "/hybrid", "/t", "0"]),
        }
    }
}

fn shutdown(args: &[&str]) -> windows::core::Result<()> {
    Command::new("shutdown.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
        .map_err(|error| windows::core::Error::new(windows::core::HRESULT(-1), error.to_string()))
}

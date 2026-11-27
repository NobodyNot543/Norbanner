//! Norbanner: a Caelestia-style desktop shell for Windows.
//!
//! A frame around every screen, a vertical bar on the left and drawers that
//! grow out of the frame's edges (dashboard at the top, launcher at the
//! bottom, session menu and sliders on the right), coloured with Material
//! You from the wallpaper.
//!
//! Usage:
//! - `norbanner`                      starts the shell
//! - `norbanner shell <command>`      sends a command to the running shell,
//!   e.g. `norbanner shell drawers toggle launcher`
//! - `norbanner --quit`               closes the running shell
//! - `norbanner --restore`            restores the Windows taskbar and work area
//! - `norbanner --recover`            replaces a stuck shell with a fresh one
//! - `norbanner --autostart on|off`   starts the shell with Windows, or not
//! - `norbanner --guard <pid>`        internal watchdog process

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod backdrop;
mod bluetooth;
mod events;
mod exclusive;
mod guard;
mod hotkeys;
mod icons;
mod lyrics;
mod media;
mod modules;
mod render;
mod screen;
mod state;
mod strings;
mod tokens;
mod ui;

use std::env;

use anyhow::{Context, Result, bail};
use norbanner_shell::{autostart, shell_link, taskbar, workarea};
use windows::{
    Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, SetErrorMode},
            Threading::CreateMutexW,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{
                DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
                WaitMessage,
            },
        },
    },
    core::{Owned, w},
};

/// Passed by the watchdog when it restarts the shell after a failure.
pub const RECOVERED_FLAG: &str = "--recovered";

fn main() -> Result<()> {
    // Suppress system modal error dialogs from third-party shell extensions.
    // SAFETY: Affects the process error mode only.
    unsafe {
        let _ = SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX);
    }
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(guard::FLAG) => {
            let pid = args
                .get(1)
                .and_then(|pid| pid.parse().ok())
                .context("--guard needs the shell's process id")?;
            return guard::run(pid);
        }
        Some("--restore") => {
            workarea::restore_all();
            taskbar::give_back();
            return Ok(());
        }
        Some("--recover") => return guard::recover_existing(),
        Some("--quit") => {
            shell_link::send("quit");
            return Ok(());
        }
        Some("--autostart") => {
            let exe = env::current_exe()?;
            return match args.get(1).map(String::as_str) {
                Some("on") => Ok(autostart::set(Some(&exe.display().to_string()))?),
                Some("off") => Ok(autostart::set(None)?),
                _ => bail!("usage: norbanner --autostart on|off"),
            };
        }
        Some("shell") => {
            let command = args[1..].join(" ");
            return match shell_link::send(&command) {
                Some(1) => Ok(()),
                Some(_) => bail!("the shell does not know `{command}`"),
                None => bail!("Norbanner is not running"),
            };
        }
        _ => {}
    }
    run()
}

fn run() -> Result<()> {
    // SAFETY: Process-wide setup before any window exists.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    // One shell per session.
    // SAFETY: The mutex lives until the process exits.
    let _mutex = unsafe { Owned::new(CreateMutexW(None, true, w!("Local\\Norbanner.Shell"))?) };
    // SAFETY: Reads the error of the call just made.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Already running: a second launch opens the launcher, like a dock
        // click would.
        shell_link::send("drawers toggle launcher");
        return Ok(());
    }
    let _guard = guard::spawn()
        .map_err(|error| norbanner_shell::log::line(&format!("no watchdog: {error:#}")))
        .ok();
    norbanner_shell::log::line("shell starting");
    let mut app = app::App::new()?;
    let mut message = MSG::default();
    loop {
        // Handle everything pending (input included) before each frame.
        // SAFETY: Standard message pump on this thread.
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    app.quit = true;
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        while let Some(event) = events::pop() {
            app.handle(event);
        }
        if app.quit {
            break;
        }
        if app.wants_frame() {
            app.render();
        } else {
            // SAFETY: Blocks until a message arrives.
            unsafe {
                let _ = WaitMessage();
            }
        }
    }
    drop(app);
    norbanner_shell::log::flush();
    Ok(())
}

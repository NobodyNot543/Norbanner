//! Window procedures and hooks only record what happened; the main loop
//! handles it afterwards. That keeps every handler free of re-entrancy:
//! Windows calls window procedures from inside many APIs the shell uses
//! (focusing, showing, resizing windows).

use std::{cell::RefCell, collections::VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    MouseMove {
        hwnd: isize,
        x: i32,
        y: i32,
    },
    MouseDown {
        hwnd: isize,
        x: i32,
        y: i32,
        right: bool,
    },
    MouseUp {
        hwnd: isize,
        x: i32,
        y: i32,
        right: bool,
    },
    Wheel {
        hwnd: isize,
        x: i32,
        y: i32,
        delta: i32,
    },
    MouseLeave {
        hwnd: isize,
    },
    Key {
        vk: u16,
        ctrl: bool,
        shift: bool,
    },
    Char(char),
    Deactivated {
        hwnd: isize,
    },
    Command(String),
    /// A service posted its message (see `app::WM_*`).
    Service(u32),
    SettingChange {
        action: u32,
        area: String,
    },
    DisplayChange,
    Timer(usize),
    /// The Windows key was tapped on its own.
    SuperTap,
    /// Win+digit: switch to this workspace (0-based).
    Workspace(usize),
    /// A volume key the shell took from Windows: +1, −1, or 0 for mute.
    VolumeKey(i32),
    Hotkey(i32),
    Foreground,
    WindowsChanged,
    TaskbarCreated,
    Quit,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
}

pub fn push(event: Event) {
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        // Coalesce pointer motion: only the latest position matters.
        if let Event::MouseMove { hwnd, .. } = event
            && let Some(Event::MouseMove { hwnd: last, .. }) = q.back()
            && *last == hwnd
        {
            q.pop_back();
        }
        if matches!(event, Event::WindowsChanged | Event::Foreground) && q.contains(&event) {
            return;
        }
        q.push_back(event);
    });
}

pub fn pop() -> Option<Event> {
    QUEUE.with(|q| q.borrow_mut().pop_front())
}

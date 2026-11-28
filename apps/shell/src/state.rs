//! What the shell shows about the system, gathered from the services and
//! refreshed when they report a change. Drawing only reads this.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Instant,
};

use norbanner_shell::{
    apps::App,
    desktops::Desktops,
    icons::IconImage,
    status::{Network, Volume},
    sysinfo::Usage,
    system::Battery,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, SYSTEMTIME, WPARAM},
        Globalization::{GetDateFormatEx, GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            SystemInformation::GetLocalTime,
        },
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::{HSTRING, PCWSTR},
};

use crate::media::Track;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Clock {
    pub fn now() -> Self {
        // SAFETY: Read-only.
        let t = unsafe { GetLocalTime() };
        Self {
            year: i32::from(t.wYear),
            month: u32::from(t.wMonth),
            day: u32::from(t.wDay),
            weekday: u32::from(t.wDayOfWeek),
            hour: u32::from(t.wHour),
            minute: u32::from(t.wMinute),
            second: u32::from(t.wSecond),
        }
    }

    pub fn hour_text(&self, twelve: bool) -> String {
        let hour = if twelve {
            match self.hour % 12 {
                0 => 12,
                h => h,
            }
        } else {
            self.hour
        };
        format!("{hour:02}")
    }
}

/// Formats a date with a Windows date picture ("dddd", "d MMMM", "MMMM yyyy")
/// in the user's locale.
pub fn format_date(year: i32, month: u32, day: u32, picture: &str) -> String {
    let date = SYSTEMTIME {
        wYear: year as u16,
        wMonth: month as u16,
        wDay: day as u16,
        wDayOfWeek: norbanner_core::calendar::weekday(year, month, day) as u16,
        ..Default::default()
    };
    let mut buffer = [0u16; 128];
    // SAFETY: The buffer and picture live for the call.
    let len = unsafe {
        GetDateFormatEx(
            None,
            Default::default(),
            Some(&date),
            &HSTRING::from(picture),
            Some(&mut buffer),
            None,
        )
    };
    String::from_utf16_lossy(&buffer[..(len - 1).max(0) as usize])
}

/// The user's first day of the week, 0 = Sunday.
pub fn first_weekday() -> u32 {
    let mut buffer = [0u16; 4];
    // SAFETY: The buffer is valid for its length.
    let len = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_IFIRSTDAYOFWEEK, Some(&mut buffer)) };
    if len <= 1 {
        return 1;
    }
    // Windows: 0 = Monday … 6 = Sunday.
    let windows_day = u32::from(buffer[0].saturating_sub(u16::from(b'0'))).min(6);
    (windows_day + 1) % 7
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveWindow {
    pub hwnd: isize,
    pub title: String,
    pub app_id: String,
}

/// An app with windows on one virtual desktop.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopApp {
    pub app_id: String,
    pub windows: Vec<isize>,
}

/// Everything the modules draw.
#[derive(Default)]
pub struct SystemState {
    pub clock: Clock,
    pub volume: Option<Volume>,
    pub network: Option<Network>,
    pub bluetooth: Option<bool>,
    pub battery: Option<Battery>,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub kb_layout: Option<String>,
    pub desktops: Desktops,
    /// App ids of the windows on each desktop, in `desktops.list` order.
    pub desktop_apps: Vec<Vec<DesktopApp>>,
    pub active: Option<ActiveWindow>,
    pub media: Option<Track>,
    /// Apps running in the background with a tray icon, for the bubble.
    pub tray: Vec<TrayEntry>,
    /// Pictures of windows for the windows panel: the image cache key and
    /// the picture, per window handle.
    pub previews: HashMap<isize, (u64, Arc<IconImage>)>,
    /// The playing song's synced lyrics, while the lyrics island is on.
    pub lyrics: Option<LyricsView>,
    pub usage: Usage,
    pub user_name: String,
    pub avatar: Option<Arc<IconImage>>,
    pub logo: Option<Arc<IconImage>>,
    /// The wallpaper, small and blurred, for frosted panels; the number
    /// changes with each new one (the image cache key).
    pub backdrop: Option<(u32, Arc<IconImage>)>,
    pub apps: Arc<Vec<App>>,
    pub icons: Icons,
    /// Brightness per HMONITOR, 0–1, for screens that support DDC/CI.
    pub brightness: HashMap<isize, f32>,
    pub started: Option<Instant>,
    /// A newer Norbanner on GitHub (its version), until the user opens
    /// Settings to install it or closes the bubble.
    pub update: Option<String>,
}

/// An app in the background apps bubble.
#[derive(Clone)]
pub struct TrayEntry {
    /// The running program, opened again on a click.
    pub path: String,
    pub name: String,
    /// Image cache key of its icon.
    pub key: u64,
    pub icon: Option<Arc<IconImage>>,
}

/// The lyrics island's content: the song's lines, the one being sung and
/// the one before it, and when they last changed (for the slide between);
/// how many words of the line are sung (for the colour sweep) and since
/// when; and the user's timing correction for this song.
#[derive(Clone)]
pub struct LyricsView {
    pub lines: Arc<Vec<norbanner_core::lyrics::Line>>,
    pub current: Option<usize>,
    pub previous: Option<usize>,
    pub changed: Instant,
    pub words: usize,
    pub previous_words: usize,
    pub word_changed: Instant,
    /// Milliseconds added to the song's position (scroll on the island).
    pub offset_ms: i32,
    /// When the correction last changed: it shows for a moment.
    pub offset_changed: Option<Instant>,
}

/// What the icon worker sends back: the app id and, if found, its icon
/// and display name.
type Loaded = (String, Option<(IconImage, String)>);

/// App icons, loaded off the UI thread (the shell can take a quarter of a
/// second per app the first time).
#[derive(Default)]
pub struct Icons {
    loaded: RefCell<HashMap<String, Arc<IconImage>>>,
    names: RefCell<HashMap<String, String>>,
    requested: RefCell<HashSet<String>>,
    tx: Option<Sender<String>>,
    rx: Option<Receiver<Loaded>>,
}

/// Message the icon worker posts to the control window when icons arrive.
pub const WM_ICONS: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 40;

impl Icons {
    pub fn start(&mut self, notify: HWND) {
        let (tx, worker_rx) = mpsc::channel::<String>();
        let (worker_tx, rx) = mpsc::channel();
        let notify = notify.0 as isize;
        let cache_dir = norbanner_config::data_dir().join("icons");
        let spawned = thread::Builder::new().name("icons".into()).spawn(move || {
            // SAFETY: Initializes COM for this worker thread only.
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let cache = norbanner_shell::icon_cache::IconCache::new(cache_dir);
            while let Ok(id) = worker_rx.recv() {
                let icon = cache
                    .get(&id, 64)
                    .ok()
                    .map(|visual| (visual.icon, visual.name));
                if worker_tx.send((id, icon)).is_err() {
                    break;
                }
                // SAFETY: Posting to a window handle is safe even if it
                // has been destroyed meanwhile.
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(notify as *mut _)), WM_ICONS, WPARAM(0), LPARAM(0));
                }
            }
        });
        if spawned.is_ok() {
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
    }

    /// The icon of `app_id`, asking the worker for it the first time.
    pub fn get(&self, app_id: &str) -> Option<Arc<IconImage>> {
        if let Some(icon) = self.loaded.borrow().get(app_id) {
            return Some(icon.clone());
        }
        if self.requested.borrow_mut().insert(app_id.to_string())
            && let Some(tx) = &self.tx
        {
            let _ = tx.send(app_id.to_string());
        }
        None
    }

    /// The app's display name ("Google Chrome"), once its icon was loaded.
    pub fn name(&self, app_id: &str) -> Option<String> {
        let _ = self.get(app_id);
        self.names.borrow().get(app_id).cloned()
    }

    /// Takes in icons the worker finished. Returns whether any arrived.
    pub fn receive(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut any = false;
        while let Ok((id, icon)) = rx.try_recv() {
            if let Some((icon, name)) = icon {
                if !name.is_empty() {
                    self.names.borrow_mut().insert(id.clone(), name);
                }
                self.loaded.borrow_mut().insert(id, Arc::new(icon));
                any = true;
            }
        }
        any
    }
}

/// A stable cache key for an image identified by text.
pub fn image_key(text: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

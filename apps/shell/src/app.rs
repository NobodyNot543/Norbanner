//! The shell: one hidden control window that receives system notifications
//! and IPC commands, one [`Screen`] per monitor, the services they draw and
//! everything the user can do.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use norbanner_config::{Config, Mode, SchemeSource};
use norbanner_shell::{
    desktops::{self, WindowDesktops},
    icons::IconImage,
    launcher as shell_launcher, session as shell_session, shell_link,
    status::{AudioWatch, NetworkWatch},
    sysinfo::Sampler,
    system, taskbar,
    tracker::{self, WindowTracker},
    watch::RegistryWatch,
    workarea::{self, Reserve},
};
use norbanner_theme::{Theme, Variant};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
            Threading::{AttachThreadInput, GetCurrentThreadId},
        },
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::{
                GetKeyState, SetFocus, VK_BACK, VK_CAPITAL, VK_DOWN, VK_ESCAPE, VK_LEFT,
                VK_NUMLOCK, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
            },
            Shell::ShellExecuteW,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW,
                EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
                GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, KillTimer,
                OBJID_WINDOW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
                SW_SHOWNORMAL, SetForegroundWindow, SetTimer, WINEVENT_OUTOFCONTEXT,
                WINEVENT_SKIPOWNPROCESS, WM_APP, WM_CLOSE, WM_COPYDATA, WM_DISPLAYCHANGE,
                WM_ENDSESSION, WM_HOTKEY, WM_QUERYENDSESSION, WM_SETTINGCHANGE, WM_TIMER,
                WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::{
    actions::Action,
    bluetooth, events,
    events::Event,
    hotkeys::{self, Hooks},
    media::{self, Control},
    modules::{bar, launcher::LauncherState, session::SessionState},
    render::{Gpu, paint},
    screen::{self, Drag, Screen},
    state::{ActiveWindow, Clock, DesktopApp, LyricsView, SystemState},
    strings,
    tokens::Tokens,
    ui::{HitKind, Slider},
};

pub const HEALTH_CHECK: u32 = WM_APP + 1;
pub const WM_AUDIO: u32 = WM_APP + 10;
pub const WM_NETWORK: u32 = WM_APP + 11;
pub const WM_BLUETOOTH: u32 = WM_APP + 12;
pub const WM_MEDIA: u32 = WM_APP + 13;
pub const WM_DESKTOPS: u32 = WM_APP + 14;
pub const WM_WORKER: u32 = WM_APP + 15;
pub const WM_LOCK_KEYS: u32 = WM_APP + 17;
pub const WM_CONFIG: u32 = WM_APP + 18;
/// The wallpaper's registry key changed.
pub const WM_WALLPAPER: u32 = WM_APP + 19;
/// A lyrics lookup ended.
pub const WM_LYRICS: u32 = WM_APP + 20;
/// Windows was about to show its volume or brightness indicator (hidden).
const WM_SYSTEM_OSD: u32 = WM_APP + 21;

const TIMER_POLL: usize = 1;
const TIMER_SECOND: usize = 2;
const TIMER_WINDOWS: usize = 3;
const TIMER_OSD: usize = 4;
/// A moment after the wallpaper's key changes, so the new file is in place.
const TIMER_WALLPAPER: usize = 5;
/// When the next line of the lyrics starts.
const TIMER_LYRICS: usize = 6;
/// Moves the lyrics island's wave while a song plays.
const TIMER_WAVE: usize = 7;
/// Lines show this much early, so their slide is under way as they are
/// sung (the word sweep itself follows the exact position).
const LYRICS_LEAD: Duration = Duration::from_millis(80);

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static CONFIG_CHANGED: AtomicU32 = AtomicU32::new(0);

/// Results from background workers.
enum Work {
    Theme(u32),
    /// Every app, the ids of those pinned to Windows' taskbar, and how
    /// much Windows has seen each used.
    Apps(
        Vec<norbanner_shell::apps::App>,
        Vec<norbanner_shell::pins::Pin>,
        std::collections::HashMap<String, u32>,
    ),
    Avatar(IconImage),
    Logo(IconImage),
    Backdrop(IconImage),
    /// Pictures of windows for the windows panel.
    Previews(Vec<(isize, Option<IconImage>)>),
    /// The apps running in the background, for the tray bubble.
    Tray(Vec<norbanner_shell::tray_apps::TrayApp>),
    Brightness(isize, f32),
}

unsafe extern "system" fn control_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            // SAFETY: This is a WM_COPYDATA being handled.
            let Some(command) = (unsafe { shell_link::read_command(lparam) }) else {
                return LRESULT(0);
            };
            let known = is_known_command(&command);
            if known {
                events::push(Event::Command(command));
            }
            LRESULT(isize::from(known))
        }
        HEALTH_CHECK => LRESULT(1),
        WM_TIMER => {
            events::push(Event::Timer(wparam.0));
            LRESULT(0)
        }
        WM_HOTKEY => {
            events::push(Event::Hotkey(wparam.0 as i32));
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            let area = if lparam.0 == 0 {
                String::new()
            } else {
                // SAFETY: For WM_SETTINGCHANGE, a non-null lparam is a string.
                unsafe {
                    PCWSTR(lparam.0 as *const u16)
                        .to_string()
                        .unwrap_or_default()
                }
            };
            events::push(Event::SettingChange {
                action: wparam.0 as u32,
                area,
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            events::push(Event::DisplayChange);
            LRESULT(0)
        }
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION | WM_CLOSE => {
            events::push(Event::Quit);
            LRESULT(0)
        }
        m if (WM_APP + 10..WM_APP + 60).contains(&m) => {
            events::push(Event::Service(m));
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            events::push(Event::TaskbarCreated);
            LRESULT(0)
        }
        m if m != 0 && m == CONFIG_CHANGED.load(Ordering::Relaxed) => {
            events::push(Event::Service(WM_CONFIG));
            LRESULT(0)
        }
        // SAFETY: Default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != 0 || hwnd.is_invalid() {
        return;
    }
    match event {
        // Windows' volume or brightness indicator: hidden at once, and
        // Norbanner's sliders show the level instead.
        EVENT_OBJECT_SHOW if crate::flyouts::hide_if_indicator(hwnd) => {
            events::push(Event::Service(WM_SYSTEM_OSD));
        }
        EVENT_SYSTEM_FOREGROUND => events::push(Event::Foreground),
        EVENT_OBJECT_NAMECHANGE => {
            // SAFETY: Read-only query.
            if unsafe { GetForegroundWindow() } == hwnd {
                events::push(Event::Foreground);
            }
        }
        _ => events::push(Event::WindowsChanged),
    }
}

/// Whether the shell understands `command` (checked in the window procedure
/// so the sender gets an immediate answer).
pub fn is_known_command(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    matches!(
        words.as_slice(),
        ["drawers", "toggle" | "open" | "close", ..]
            | [
                "launcher" | "session" | "dashboard",
                "toggle" | "open" | "close"
            ]
            | ["quit"]
            | ["reload"]
            | ["wallpaper", "set", ..]
            | ["scheme", "variant" | "mode", _]
            | ["mpris", "playPause" | "next" | "previous"]
            | ["lyrics", "toggle"]
            | ["workspace", _]
            | ["osd", "show"]
            | ["update", "available", _]
    )
}

/// What identifies a song for its lyrics; `None` until the player has
/// reported its length.
fn song_of(track: &media::Track) -> Option<crate::lyrics::Song> {
    let length = track.timeline?.duration;
    Some(crate::lyrics::Song {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        seconds: length.as_secs(),
    })
}

fn monitors() -> Vec<(isize, RECT, f32)> {
    unsafe extern "system" fn each(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows::core::BOOL {
        // SAFETY: `data` is the Vec passed below, alive for the enumeration.
        let list = unsafe { &mut *(data.0 as *mut Vec<(isize, RECT, f32)>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: Read-only queries with valid out-pointers.
        unsafe {
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                let (mut x, mut y) = (96u32, 96u32);
                let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
                list.push((
                    screen::monitor_key(monitor),
                    info.rcMonitor,
                    x as f32 / 96.0,
                ));
            }
        }
        true.into()
    }
    let mut list: Vec<(isize, RECT, f32)> = Vec::new();
    // SAFETY: The callback only writes into `list`.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(each),
            LPARAM(std::ptr::from_mut(&mut list) as isize),
        );
    }
    list
}

/// Gives `hwnd` the keyboard even though another app is in front: Windows
/// only lets the foreground thread do that, so borrow its input state.
fn take_focus(hwnd: HWND) {
    // SAFETY: Thread attachment is undone before returning.
    unsafe {
        let front = GetForegroundWindow();
        let front_thread = GetWindowThreadProcessId(front, None);
        let me = GetCurrentThreadId();
        let attached = front_thread != 0
            && front_thread != me
            && AttachThreadInput(me, front_thread, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if attached {
            let _ = AttachThreadInput(me, front_thread, false);
        }
    }
}

/// The wallpaper file and when it was written, to notice a new wallpaper
/// even when Windows reuses the same path.
fn wallpaper_stamp() -> Option<(PathBuf, Option<std::time::SystemTime>)> {
    let path = norbanner_shell::wallpaper::current()?;
    let written = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    Some((path, written))
}

/// The bar's logo: the image at `path` (any format Windows decodes, PNG
/// with transparency included), or Norbanner's own when empty or unreadable.
/// Requires COM on the calling thread.
fn load_logo(path: &str) -> Option<IconImage> {
    static BUNDLED: &[u8] = include_bytes!("../../../assets/brand/norbanner-1024.png");
    let path = path.trim();
    if !path.is_empty()
        && let Some(image) = norbanner_shell::wallpaper::thumbnail(std::path::Path::new(path), 256)
    {
        return Some(trim_transparent(image));
    }
    norbanner_shell::imagery::decode(BUNDLED, 128, 4_000_000)
}

/// Crops transparent margins (logos often come with a wide empty border),
/// keeping the result square so it centres in its slot.
fn trim_transparent(image: IconImage) -> IconImage {
    let (w, h) = (image.width as usize, image.height as usize);
    let alpha = |x: usize, y: usize| image.bgra[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) > 24 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || y0 > y1 {
        return image;
    }
    // Square around the content's centre.
    let side = (x1 - x0 + 1).max(y1 - y0 + 1);
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let left = cx.saturating_sub(side / 2);
    let top = cy.saturating_sub(side / 2);
    let mut bgra = vec![0u8; side * side * 4];
    for y in 0..side {
        for x in 0..side {
            let (sx, sy) = (left + x, top + y);
            if sx < w && sy < h {
                let from = (sy * w + sx) * 4;
                let to = (y * side + x) * 4;
                bgra[to..to + 4].copy_from_slice(&image.bgra[from..from + 4]);
            }
        }
    }
    IconImage {
        width: side as u32,
        height: side as u32,
        bgra,
    }
}

/// Norbanner's Settings app, installed beside the shell.
fn settings_exe() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("norbanner-settings.exe")))
        .filter(|path| path.is_file())
}

/// How long between background looks for a newer version.
const UPDATE_EVERY: Duration = Duration::from_secs(20 * 60 * 60);
/// How long without input counts as the user being away (an install
/// restarts the shell).
const UPDATE_IDLE: Duration = Duration::from_secs(5 * 60);

/// What the shell remembers about updates between runs: when Settings last
/// looked, and the version whose bubble the user closed. Kept beside
/// `shell.json` as two lines (Unix seconds, version).
#[derive(Debug, Default, PartialEq, Eq)]
struct UpdateCheck {
    last: Option<SystemTime>,
    dismissed: Option<String>,
}

impl UpdateCheck {
    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Self {
        let mut lines = text.lines().map(str::trim);
        let last = lines
            .next()
            .and_then(|secs| secs.parse::<u64>().ok())
            .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
        let dismissed = lines
            .next()
            .filter(|version| !version.is_empty())
            .map(str::to_string);
        Self { last, dismissed }
    }

    fn text(&self) -> String {
        let secs = self
            .last
            .and_then(|last| last.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        format!("{secs}\n{}\n", self.dismissed.as_deref().unwrap_or(""))
    }

    fn save(&self, path: &Path) {
        if let Err(error) = std::fs::write(path, self.text()) {
            norbanner_shell::log::line(&format!("could not save the update check: {error}"));
        }
    }

    /// Whether it is time to look again. A clock set back counts as due.
    fn due(&self, now: SystemTime) -> bool {
        self.last.is_none_or(|last| match now.duration_since(last) {
            Ok(since) => since >= UPDATE_EVERY,
            Err(_) => true,
        })
    }
}

fn update_check_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("update-check.txt")
}

/// How long since the user last touched the keyboard or mouse.
fn idle_for() -> Duration {
    let mut info = windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO {
        cbSize: size_of::<windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: Out-pointer is a valid local of the size given.
    unsafe {
        if !windows::Win32::UI::Input::KeyboardAndMouse::GetLastInputInfo(&raw mut info).as_bool() {
            return Duration::ZERO;
        }
        let now = windows::Win32::System::SystemInformation::GetTickCount();
        Duration::from_millis(u64::from(now.wrapping_sub(info.dwTime)))
    }
}

fn open_uri(uri: &str) {
    // SAFETY: The strings live for the call.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(uri),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

fn copy_to_clipboard(owner: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: The global block is sized for the text and handed to the
    // clipboard, which then owns it.
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(block) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let target = GlobalLock(block).cast::<u16>();
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
                let _ = GlobalUnlock(block);
                let _ = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(block.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

pub struct App {
    gpu: Gpu,
    control: HWND,
    screens: Vec<Screen>,
    config: Config,
    config_path: PathBuf,
    config_stamp: Option<SystemTime>,
    tokens: Tokens,
    theme: Theme,
    wallpaper_source: Option<u32>,
    state: SystemState,
    launcher: LauncherState,
    session: SessionState,
    audio: Option<AudioWatch>,
    _network: Option<NetworkWatch>,
    bluetooth: Option<bluetooth::Monitor>,
    media: Option<media::Watch>,
    lyrics: Option<crate::lyrics::Fetcher>,
    /// Numbers the window pictures, for their image cache keys.
    preview_counter: u64,
    /// Whether the wave timer runs (an island shows a playing song).
    wave_running: bool,
    /// Timing corrections the user made per song this session, in ms.
    lyrics_offsets: std::collections::HashMap<crate::lyrics::Song, i32>,
    desktops_watch: Option<RegistryWatch>,
    /// `Control Panel\Desktop`, where Windows records the wallpaper however
    /// it is changed (Settings, slideshow, Spotlight, another app).
    wallpaper_watch: Option<RegistryWatch>,
    /// The wallpaper the colours and the glass were last made from: its
    /// path and when the file was written.
    wallpaper_stamp: Option<(PathBuf, Option<std::time::SystemTime>)>,
    window_desktops: Option<WindowDesktops>,
    tracker: WindowTracker,
    sampler: Sampler,
    hooks: Hooks,
    win_hooks: Vec<HWINEVENTHOOK>,
    work_tx: Sender<Work>,
    work_rx: Receiver<Work>,
    brightness_tx: Option<Sender<(isize, Option<f32>)>>,
    /// When Settings last looked for a newer version in the background,
    /// and the version whose bubble the user closed.
    update_check: UpdateCheck,
    /// The screen whose window has the keyboard (launcher/session open).
    keyboard: Option<isize>,
    /// Whether media timeline updates are subscribed (only while a
    /// dashboard shows the progress arc).
    media_progress: bool,
    /// Windows to bring forward once the switch to their desktop lands.
    pending_activation: Option<(usize, Vec<isize>)>,
    /// Apps pinned to Windows' taskbar: always in the bar, first.
    pins: Vec<norbanner_shell::pins::Pin>,
    /// The window in front before the launcher took the keyboard, for
    /// tools (clipboard history, emoji) that act on it.
    previous_front: isize,
    /// When each app and window was first seen, so the bar keeps them in
    /// opening order instead of Windows' most-recently-used order.
    first_seen: std::collections::HashMap<String, u64>,
    window_seen: std::collections::HashMap<isize, u64>,
    seen_counter: u64,
    pub quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let config_path = norbanner_config::default_path();
        let (config, error) = norbanner_config::load_or_default(&config_path);
        if let Some(error) = error {
            norbanner_shell::log::line(&format!("{error}; using defaults"));
        }
        strings::set_language(config.general.language);
        paint::with_cache(|cache| {
            cache.sans = config.appearance.font.family.sans.clone();
            cache.mono = config.appearance.font.family.mono.clone();
        });
        let gpu = Gpu::new()?;
        screen::register_class()?;
        // SAFETY: Registers the control class and creates its hidden
        // window; registered message ids are process-wide constants.
        let control = unsafe {
            let instance = GetModuleHandleW(None)?;
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(control_proc),
                hInstance: instance.into(),
                lpszClassName: shell_link::WINDOW_CLASS,
                ..Default::default()
            });
            TASKBAR_CREATED.store(
                RegisterWindowMessageW(w!("TaskbarCreated")),
                Ordering::Relaxed,
            );
            CONFIG_CHANGED.store(
                RegisterWindowMessageW(shell_link::CONFIG_CHANGED),
                Ordering::Relaxed,
            );
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                shell_link::WINDOW_CLASS,
                w!("Norbanner"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .context("could not create the control window")?
        };
        let (work_tx, work_rx) = mpsc::channel();
        let tokens = Tokens::new(&config);
        let mut app = Self {
            gpu,
            control,
            screens: Vec::new(),
            tokens,
            theme: Theme::default(),
            wallpaper_source: None,
            state: SystemState::default(),
            launcher: LauncherState::default(),
            session: SessionState::default(),
            audio: AudioWatch::new(control.0 as isize, WM_AUDIO),
            _network: NetworkWatch::new(control.0 as isize, WM_NETWORK),
            bluetooth: bluetooth::Monitor::new(control.0 as isize, WM_BLUETOOTH).ok(),
            media: media::Watch::start(control, WM_MEDIA),
            lyrics: crate::lyrics::Fetcher::start(control, WM_LYRICS),
            lyrics_offsets: std::collections::HashMap::new(),
            preview_counter: 0,
            wave_running: false,
            desktops_watch: RegistryWatch::new(
                desktops::WATCH_KEY,
                true,
                control.0 as isize,
                WM_DESKTOPS,
            ),
            wallpaper_watch: RegistryWatch::new(
                w!(r"Control Panel\Desktop"),
                false,
                control.0 as isize,
                WM_WALLPAPER,
            ),
            wallpaper_stamp: wallpaper_stamp(),
            window_desktops: WindowDesktops::new(),
            tracker: WindowTracker::default(),
            sampler: Sampler::default(),
            hooks: Hooks::new(control),
            win_hooks: Vec::new(),
            work_tx,
            work_rx,
            brightness_tx: None,
            update_check: UpdateCheck::load(&update_check_path(&config_path)),
            keyboard: None,
            media_progress: false,
            pending_activation: None,
            pins: Vec::new(),
            previous_front: 0,
            first_seen: std::collections::HashMap::new(),
            window_seen: std::collections::HashMap::new(),
            seen_counter: 0,
            quit: false,
            config_stamp: std::fs::metadata(&config_path)
                .and_then(|m| m.modified())
                .ok(),
            config_path,
            config,
        };
        app.state.started = Some(Instant::now());
        app.state.user_name = norbanner_shell::user::display_name();
        app.state.icons.start(control);
        app.install_win_events();
        app.hooks
            .configure(&app.config.launcher.hotkey, &app.config.session.hotkey);
        crate::flyouts::set_enabled(app.config.osd.enabled);
        if app.config.general.replace_taskbar {
            taskbar::take_over();
        }
        app.sync_screens()?;
        app.refresh_all();
        app.start_workers();
        app.recompute_theme();
        // SAFETY: Timers on our own window.
        unsafe {
            SetTimer(Some(control), TIMER_POLL, 33, None);
            SetTimer(Some(control), TIMER_SECOND, 1000, None);
        }
        app.reserve_space();
        Ok(app)
    }

    fn install_win_events(&mut self) {
        let ranges = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
        ];
        let _ = (EVENT_OBJECT_DESTROY, EVENT_OBJECT_SHOW);
        for (min, max) in ranges {
            // SAFETY: Out-of-context hooks call back on this thread's
            // message loop; removed in Drop.
            let hook = unsafe {
                SetWinEventHook(
                    min,
                    max,
                    None,
                    Some(win_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if !hook.is_invalid() {
                self.win_hooks.push(hook);
            }
        }
    }

    fn start_workers(&mut self) {
        let control = self.control.0 as isize;
        let notify = move || {
            // SAFETY: Posting to a possibly destroyed window is harmless.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(HWND(control as *mut _)),
                    WM_WORKER,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        };
        self.load_backdrop();
        // Apps, the account picture and the logo, once.
        let tx = self.work_tx.clone();
        let logo_path = self.config.bar.logo.clone();
        let _ = thread::Builder::new()
            .name("catalogue".into())
            .spawn(move || {
                // SAFETY: COM for this worker thread only.
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                if let Some(logo) = load_logo(&logo_path) {
                    let _ = tx.send(Work::Logo(logo));
                }
                if let Some(avatar) = norbanner_shell::user::picture()
                    .and_then(|p| norbanner_shell::wallpaper::thumbnail(&p, 160))
                {
                    let _ = tx.send(Work::Avatar(avatar));
                }
                notify();
                let apps = norbanner_shell::apps::list();
                let pins = norbanner_shell::pins::taskbar(&apps);
                let used = norbanner_shell::userassist::scores(&apps);
                let _ = tx.send(Work::Apps(apps, pins, used));
                notify();
            });
        // Brightness over DDC/CI is slow: one worker owns it.
        let (btx, brx) = mpsc::channel::<(isize, Option<f32>)>();
        let tx = self.work_tx.clone();
        let monitors: Vec<isize> = self.screens.iter().map(|s| s.monitor).collect();
        let spawned = thread::Builder::new()
            .name("brightness".into())
            .spawn(move || {
                for monitor in monitors {
                    if let Some(value) = norbanner_shell::brightness::get(monitor) {
                        let _ = tx.send(Work::Brightness(monitor, value));
                        notify();
                    }
                }
                while let Ok(first) = brx.recv() {
                    // Only the latest request of each kind per screen in a
                    // burst matters: a drag's last value, one read per
                    // screen (all of them are asked at once).
                    let mut burst = vec![first];
                    while let Ok(next) = brx.try_recv() {
                        burst.retain(|&(monitor, request): &(isize, Option<f32>)| {
                            monitor != next.0 || request.is_some() != next.1.is_some()
                        });
                        burst.push(next);
                    }
                    for (monitor, request) in burst {
                        match request {
                            Some(value) => {
                                norbanner_shell::brightness::set(monitor, value);
                            }
                            None => {
                                if let Some(value) = norbanner_shell::brightness::get(monitor) {
                                    let _ = tx.send(Work::Brightness(monitor, value));
                                    notify();
                                }
                            }
                        }
                    }
                }
            });
        if spawned.is_ok() {
            self.brightness_tx = Some(btx);
        }
    }

    /// Follows a new wallpaper: the colours (when they come from it) and the
    /// launcher's glass are made again from it. Unless `force`, does nothing
    /// when the wallpaper file is the same as last time.
    fn wallpaper_changed(&mut self, force: bool) {
        let stamp = wallpaper_stamp();
        if !force && stamp == self.wallpaper_stamp {
            return;
        }
        self.wallpaper_stamp = stamp;
        // The current colours stay until the new ones are ready.
        self.compute_wallpaper_source();
        self.load_backdrop();
    }

    /// Blurs the wallpaper for the launcher's frosted glass, off the UI
    /// thread.
    fn load_backdrop(&self) {
        let tx = self.work_tx.clone();
        let control = self.control.0 as isize;
        let _ = thread::Builder::new()
            .name("backdrop".into())
            .spawn(move || {
                // SAFETY: COM for this worker thread only.
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let Some(mut image) = norbanner_shell::wallpaper::current()
                    .and_then(|path| norbanner_shell::wallpaper::thumbnail(&path, 360))
                else {
                    return;
                };
                let radius = (image.width.max(image.height) / 30).max(2) as usize;
                crate::backdrop::blur(&mut image, radius);
                let _ = tx.send(Work::Backdrop(image));
                // SAFETY: As in `start_workers`.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(HWND(control as *mut _)),
                        WM_WORKER,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
    }

    fn compute_wallpaper_source(&self) {
        let tx = self.work_tx.clone();
        let control = self.control.0 as isize;
        let _ = thread::Builder::new()
            .name("wallpaper-colours".into())
            .spawn(move || {
                // SAFETY: COM for this worker thread only.
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let source = norbanner_shell::wallpaper::current()
                    .and_then(|path| norbanner_shell::wallpaper::pixels(&path))
                    .and_then(|pixels| norbanner_theme::source_from_pixels(&pixels));
                if let Some(source) = source {
                    let _ = tx.send(Work::Theme(source));
                    // SAFETY: As in `start_workers`.
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(HWND(control as *mut _)),
                            WM_WORKER,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            });
    }

    /// Rebuilds the colour scheme from the configured source.
    fn recompute_theme(&mut self) {
        let dark = match self.config.theme.mode {
            Mode::Dark => true,
            Mode::Light => false,
            Mode::System => !system::uses_light_theme(),
        };
        let variant = Variant::ALL
            .into_iter()
            .find(|v| v.id() == self.config.theme.variant)
            .unwrap_or_default();
        let source = match self.config.theme.source {
            SchemeSource::Wallpaper => {
                if self.wallpaper_source.is_none() {
                    self.compute_wallpaper_source();
                }
                self.wallpaper_source
            }
            SchemeSource::Accent => system::accent_color()
                .map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)),
            SchemeSource::Custom => norbanner_config::parse_hex(&self.config.theme.source_color),
        }
        .unwrap_or(norbanner_theme::DEFAULT_SOURCE);
        self.theme = Theme::from_source(source, dark, variant);
        self.mark_all_dirty();
    }

    fn mark_all_dirty(&mut self) {
        for screen in &mut self.screens {
            screen.dirty = true;
        }
    }

    /// Creates, moves or removes screens to match the monitors.
    fn sync_screens(&mut self) -> Result<()> {
        let monitors = monitors();
        let rects: Vec<RECT> = monitors.iter().map(|(_, rect, _)| *rect).collect();
        self.screens
            .retain(|s| monitors.iter().any(|(m, _, _)| *m == s.monitor));
        for (monitor, rect, scale) in monitors {
            if let Some(screen) = self.screens.iter_mut().find(|s| s.monitor == monitor) {
                if screen.rect != rect || (screen.scale - scale).abs() > f32::EPSILON {
                    screen.relocate(&self.gpu, rect, scale)?;
                }
            } else {
                self.screens
                    .push(Screen::new(&self.gpu, monitor, rect, scale)?);
            }
        }
        // Which edges another monitor continues past.
        for screen in &mut self.screens {
            let r = screen.rect;
            let overlaps_v = |o: &RECT| o.top < r.bottom && o.bottom > r.top;
            let overlaps_h = |o: &RECT| o.left < r.right && o.right > r.left;
            screen.right_shared = rects.iter().any(|o| o.left == r.right && overlaps_v(o));
            screen.top_shared = rects.iter().any(|o| o.bottom == r.top && overlaps_h(o));
        }
        Ok(())
    }

    /// Keeps maximized windows inside the frame.
    fn reserve_space(&mut self) {
        self.reserve_space_with(false);
    }

    /// Reserves the bar and frame along every monitor's edges (AppBars), or
    /// releases them when `general.reserveSpace` is off.
    fn reserve_space_with(&mut self, force: bool) {
        let enabled = self.config.general.reserve_space;
        let border = self.config.border.thickness;
        for screen in &mut self.screens {
            let t = Tokens::for_scale(&self.config, screen.scale);
            let bar = if self.config.bar.persistent {
                bar::width(t.bar_inner, border, t.padding.small)
            } else {
                border
            };
            let s = screen.scale;
            let px = |v: f32| (v * s).round() as i32;
            let reserve = enabled.then(|| Reserve {
                left: px(bar),
                top: px(border),
                right: px(border),
                bottom: px(border),
            });
            screen.exclusive.set(screen.rect, reserve, force);
        }
    }

    fn refresh_all(&mut self) {
        self.state.clock = Clock::now();
        self.state.battery = system::battery();
        self.state.network = Some(norbanner_shell::status::network());
        self.state.bluetooth = self.bluetooth.as_ref().map(bluetooth::Monitor::is_on);
        self.state.volume = self.audio.as_mut().and_then(AudioWatch::volume);
        self.state.media = self.media.as_ref().and_then(media::Watch::snapshot);
        self.state.kb_layout = system::input_language();
        self.refresh_lock_keys();
        self.refresh_desktops();
        self.refresh_active();
        self.state.usage = self.sampler.sample();
    }

    fn refresh_lock_keys(&mut self) {
        // SAFETY: Read-only key state.
        unsafe {
            self.state.caps_lock = GetKeyState(i32::from(VK_CAPITAL.0)) & 1 != 0;
            self.state.num_lock = GetKeyState(i32::from(VK_NUMLOCK.0)) & 1 != 0;
        }
    }

    fn refresh_desktops(&mut self) {
        self.state.desktops = desktops::read();
        let count = self.state.desktops.list.len();
        let mut apps: Vec<Vec<DesktopApp>> = vec![Vec::new(); count];
        for window in self.tracker.list_with_workspaces(true) {
            let index = self
                .window_desktops
                .as_ref()
                .and_then(|d| d.desktop_of(window.hwnd))
                .and_then(|id| self.state.desktops.list.iter().position(|d| d.id == id))
                .unwrap_or(self.state.desktops.current);
            let Some(list) = apps.get_mut(index) else {
                continue;
            };
            match list.iter_mut().find(|app| app.app_id == window.app_id) {
                Some(app) => app.windows.push(window.hwnd),
                None => list.push(DesktopApp {
                    app_id: window.app_id.clone(),
                    windows: vec![window.hwnd],
                }),
            }
        }
        // Stable order: apps (and each app's windows) where they first
        // appeared; switching between them never reshuffles the bar.
        let mut alive_apps = std::collections::HashSet::new();
        let mut alive_windows = std::collections::HashSet::new();
        for list in &mut apps {
            for app in list.iter_mut() {
                alive_apps.insert(app.app_id.clone());
                for &hwnd in &app.windows {
                    alive_windows.insert(hwnd);
                    if !self.window_seen.contains_key(&hwnd) {
                        self.seen_counter += 1;
                        self.window_seen.insert(hwnd, self.seen_counter);
                    }
                }
                if !self.first_seen.contains_key(&app.app_id) {
                    self.seen_counter += 1;
                    self.first_seen
                        .insert(app.app_id.clone(), self.seen_counter);
                }
                app.windows
                    .sort_by_key(|hwnd| self.window_seen.get(hwnd).copied().unwrap_or(u64::MAX));
            }
            list.sort_by_key(|app| {
                self.first_seen
                    .get(&app.app_id)
                    .copied()
                    .unwrap_or(u64::MAX)
            });
        }
        // Forget what closed, so an app reopened later goes to the end.
        self.first_seen.retain(|id, _| alive_apps.contains(id));
        self.window_seen
            .retain(|hwnd, _| alive_windows.contains(hwnd));
        // The taskbar's pins lead the current desktop, open or not, as on
        // Windows' taskbar; the other open apps follow.
        if let Some(list) = apps.get_mut(self.state.desktops.current) {
            let mut front: Vec<DesktopApp> = Vec::new();
            for pin in &self.pins {
                match list.iter().position(|app| pin.owns(&app.app_id)) {
                    Some(i) => front.push(list.remove(i)),
                    None => front.push(DesktopApp {
                        app_id: pin.id.clone(),
                        windows: Vec::new(),
                    }),
                }
            }
            front.append(list);
            *list = front;
        }
        for list in &apps {
            for app in list {
                let _ = self.state.icons.get(&app.app_id);
            }
        }
        if let Some((desktop, windows)) = self.pending_activation.take() {
            if self.state.desktops.current == desktop {
                shell_launcher::activate(&windows);
            } else {
                self.pending_activation = Some((desktop, windows));
            }
        }
        self.state.desktop_apps = apps;
        self.sync_window_panels();
        self.mark_all_dirty();
    }

    /// Keeps each open windows panel in step with the app's windows: a
    /// closed window leaves its list, and the panel closes with the app's
    /// last window. Pictures of windows that closed are dropped.
    fn sync_window_panels(&mut self) {
        let now = Instant::now();
        let scale = self.config.appearance.anim_scale;
        let apps = &self.state.desktop_apps;
        let find = |app_id: &str| {
            apps.iter()
                .flatten()
                .find(|app| app.app_id == app_id && !app.windows.is_empty())
                .cloned()
        };
        for screen in &mut self.screens {
            let Some((y, shown)) = screen.window_app.clone() else {
                continue;
            };
            match find(&shown.app_id) {
                Some(app) => {
                    if app.windows != shown.windows {
                        screen.window_app = Some((y, app));
                        screen.dirty = true;
                    }
                }
                None => {
                    screen.windows.set(false, scale, now);
                    screen.dirty = true;
                }
            }
        }
        let alive: std::collections::HashSet<isize> = self
            .state
            .desktop_apps
            .iter()
            .flatten()
            .flat_map(|app| app.windows.iter().copied())
            .collect();
        self.state.previews.retain(|hwnd, (key, _)| {
            let keep = alive.contains(hwnd);
            if !keep {
                paint::forget_image(*key);
            }
            keep
        });
    }

    /// Finds the apps running in the background, off the UI thread.
    fn request_tray(&self) {
        let tx = self.work_tx.clone();
        let control = self.control.0 as isize;
        let _ = thread::Builder::new().name("tray".into()).spawn(move || {
            // SAFETY: COM for this worker thread only (icons decode with WIC).
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let _ = tx.send(Work::Tray(norbanner_shell::tray_apps::running()));
            // SAFETY: As in `start_workers`.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(HWND(control as *mut _)),
                    WM_WORKER,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });
    }

    /// Takes pictures of `windows` for the windows panel, off the UI thread.
    fn request_previews(&self, windows: Vec<isize>) {
        let tx = self.work_tx.clone();
        let control = self.control.0 as isize;
        let _ = thread::Builder::new()
            .name("previews".into())
            .spawn(move || {
                let pictures = windows
                    .into_iter()
                    .map(|hwnd| (hwnd, crate::previews::capture(hwnd)))
                    .collect();
                let _ = tx.send(Work::Previews(pictures));
                // SAFETY: As in `start_workers`.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(HWND(control as *mut _)),
                        WM_WORKER,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
    }

    fn refresh_active(&mut self) {
        let foreground = tracker::foreground_window();
        let own = self.screens.iter().any(|s| s.id() == foreground)
            || foreground == self.control.0 as isize;
        if own {
            return;
        }
        let tracked = self
            .tracker
            .list()
            .into_iter()
            .find(|w| w.hwnd == foreground);
        self.state.active = tracked.map(|w| ActiveWindow {
            hwnd: w.hwnd,
            title: tracker::title(w.hwnd),
            app_id: w.app_id,
        });
        if let Some(active) = &self.state.active {
            let _ = self.state.icons.get(&active.app_id);
        }
        for screen in &mut self.screens {
            let fullscreen = tracker::foreground_is_fullscreen(screen.rect);
            screen.set_fullscreen(
                fullscreen,
                self.config.appearance.anim_scale,
                Instant::now(),
            );
        }
        self.mark_all_dirty();
    }

    fn reload_config(&mut self) {
        let (config, error) = norbanner_config::load_or_default(&self.config_path);
        if let Some(error) = error {
            norbanner_shell::log::line(&format!("{error}; keeping the previous configuration"));
            return;
        }
        let theme_changed = config.theme != self.config.theme;
        if config.bar.logo != self.config.bar.logo {
            let tx = self.work_tx.clone();
            let control = self.control.0 as isize;
            let path = config.bar.logo.clone();
            let _ = thread::Builder::new().name("logo".into()).spawn(move || {
                // SAFETY: COM for this worker thread only.
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                if let Some(logo) = load_logo(&path) {
                    let _ = tx.send(Work::Logo(logo));
                    // SAFETY: Posting to a possibly destroyed window is harmless.
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(HWND(control as *mut _)),
                            WM_WORKER,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            });
        }
        self.config = config;
        self.config_stamp = std::fs::metadata(&self.config_path)
            .and_then(|m| m.modified())
            .ok();
        self.tokens = Tokens::new(&self.config);
        strings::set_language(self.config.general.language);
        paint::with_cache(|cache| {
            cache.sans = self.config.appearance.font.family.sans.clone();
            cache.mono = self.config.appearance.font.family.mono.clone();
            cache.clear_fonts();
        });
        self.hooks
            .configure(&self.config.launcher.hotkey, &self.config.session.hotkey);
        crate::flyouts::set_enabled(self.config.osd.enabled);
        if self.config.general.replace_taskbar {
            taskbar::take_over();
        } else {
            taskbar::give_back();
        }
        if theme_changed {
            self.recompute_theme();
        }
        self.launcher.refresh(&self.state.apps, &self.config);
        self.reserve_space();
        self.mark_all_dirty();
    }

    fn save_config(&mut self, edit: impl FnOnce(&mut Config)) {
        let mut config = self.config.clone();
        edit(&mut config);
        match norbanner_config::update(&self.config_path, |mut saved| {
            saved.theme = config.theme.clone();
            saved.dashboard.lyrics = config.dashboard.lyrics;
            saved
        }) {
            Ok(saved) => {
                self.config.theme = saved.theme;
                self.config.dashboard.lyrics = saved.dashboard.lyrics;
            }
            Err(error) => {
                norbanner_shell::log::line(&format!("could not save the configuration: {error}"));
                self.config.theme = config.theme;
                self.config.dashboard.lyrics = config.dashboard.lyrics;
            }
        }
        self.config_stamp = std::fs::metadata(&self.config_path)
            .and_then(|m| m.modified())
            .ok();
    }

    /// Brings the lyrics island up to date with the playing song: looks the
    /// song up if needed, picks the line and word being sung, and sets a
    /// one-shot timer for the next change. Nothing runs in between.
    fn sync_lyrics(&mut self) {
        let now = Instant::now();
        let track = self
            .state
            .media
            .as_ref()
            .filter(|_| self.config.dashboard.lyrics);
        // Only once the player has said how long the song is: without it
        // another recording's timings could be taken (and kept).
        let song = track.and_then(song_of);
        // `None` while not known yet (looking, or the last look failed);
        // `Some(None)` when the song has no synced lyrics.
        let looked = match (&mut self.lyrics, track, &song) {
            (Some(fetcher), Some(track), Some(song)) => {
                fetcher.get(song, track.timeline.map(|timeline| timeline.duration))
            }
            _ => None,
        };
        let waiting = track.is_some() && self.lyrics.is_some() && looked.is_none();
        let lines = looked.flatten();
        let timeline = track.and_then(|track| track.timeline);
        let (Some(lines), Some(song)) = (lines, song) else {
            // SAFETY: Re-arms or stops our own timer.
            unsafe {
                if waiting {
                    // Ask again in a moment: a busy worker, a failed look
                    // (no network yet, the service slow) or a length still
                    // to come all clear up on their own.
                    SetTimer(Some(self.control), TIMER_LYRICS, 5000, None);
                } else {
                    let _ = KillTimer(Some(self.control), TIMER_LYRICS);
                }
            }
            if self.state.lyrics.take().is_some() {
                self.mark_all_dirty();
            }
            return;
        };
        let offset_ms = self.lyrics_offsets.get(&song).copied().unwrap_or(0);
        let position = timeline.map(|timeline| {
            let at = timeline.position_at(now) + LYRICS_LEAD;
            if offset_ms >= 0 {
                at + Duration::from_millis(offset_ms.unsigned_abs().into())
            } else {
                at.saturating_sub(Duration::from_millis(offset_ms.unsigned_abs().into()))
            }
        });
        let current =
            position.and_then(|position| norbanner_core::lyrics::index_at(&lines, position));
        // Whole-line animations need no word steps (and no wake-ups for them).
        let by_word =
            crate::modules::lyrics::Mode::from(&self.config.dashboard.lyrics_animation).by_word();
        let words = current
            .zip(position)
            .filter(|_| by_word)
            .map_or(0, |(i, position)| {
                norbanner_core::lyrics::words_at(&lines[i], position)
            });
        let mut dirty = false;
        match &mut self.state.lyrics {
            Some(view) if Arc::ptr_eq(&view.lines, &lines) => {
                if view.current != current {
                    view.previous = view.current;
                    view.current = current;
                    view.changed = now;
                    view.previous_words = 0;
                    view.words = words;
                    view.word_changed = now;
                    dirty = true;
                } else if view.words != words {
                    view.previous_words = view.words;
                    view.words = words;
                    view.word_changed = now;
                    dirty = true;
                }
                if view.offset_ms != offset_ms {
                    view.offset_ms = offset_ms;
                    view.offset_changed = Some(now);
                    dirty = true;
                }
            }
            _ => {
                self.state.lyrics = Some(LyricsView {
                    lines: lines.clone(),
                    current,
                    previous: None,
                    changed: now,
                    words,
                    previous_words: 0,
                    word_changed: now,
                    offset_ms,
                    offset_changed: None,
                });
                dirty = true;
            }
        }
        if dirty {
            self.mark_all_dirty();
        }
        let next = timeline
            .filter(|timeline| timeline.advancing(now))
            .zip(position)
            .and_then(|(_, position)| {
                let next = if by_word {
                    norbanner_core::lyrics::next_change(&lines, position)
                } else {
                    lines
                        .get(lines.partition_point(|line| line.at <= position))
                        .map(|line| line.at)
                };
                next.map(|next| next.saturating_sub(position))
            });
        // SAFETY: (Re)arms or stops a timer on our own window.
        unsafe {
            match next {
                Some(wait) => {
                    let ms = wait.as_millis().clamp(16, 60_000) as u32;
                    SetTimer(Some(self.control), TIMER_LYRICS, ms, None);
                }
                None => {
                    let _ = KillTimer(Some(self.control), TIMER_LYRICS);
                }
            }
        }
    }

    /// The screen under the pointer, else the first.
    fn pointer_screen(&self) -> Option<usize> {
        let mut point = POINT::default();
        // SAFETY: Out-pointer is a valid local.
        if unsafe { GetCursorPos(&mut point) }.is_ok()
            && let Some(i) = self.screens.iter().position(|s| {
                point.x >= s.rect.left
                    && point.x < s.rect.right
                    && point.y >= s.rect.top
                    && point.y < s.rect.bottom
            })
        {
            return Some(i);
        }
        (!self.screens.is_empty()).then_some(0)
    }

    fn screen_index(&self, hwnd: isize) -> Option<usize> {
        self.screens.iter().position(|s| s.id() == hwnd)
    }

    // ---- drawers -------------------------------------------------------

    fn open_keyboard_drawer(&mut self, index: usize, launcher: bool) {
        let now = Instant::now();
        let scale = self.config.appearance.anim_scale;
        for (i, screen) in self.screens.iter_mut().enumerate() {
            if i != index {
                screen.launcher.set(false, scale, now);
                screen.session.set(false, scale, now);
                screen.set_activatable(false);
            }
        }
        let screen = &mut self.screens[index];
        if launcher {
            screen.session.set(false, scale, now);
            // The dashboard hangs from the top; on short screens the two
            // would meet in the middle, so only one shows.
            screen.dashboard.set(false, scale, now);
            screen.dashboard_pinned = false;
            screen.launcher.set(true, scale, now);
            self.launcher.clear();
            self.state.usage = self.sampler.sample();
            self.launcher.refresh(&self.state.apps, &self.config);
            for item in
                self.launcher.items.iter().take(
                    crate::modules::launcher::GRID_COLUMNS * crate::modules::launcher::GRID_ROWS,
                )
            {
                if let crate::modules::launcher::Kind::App(id) = &item.kind {
                    let _ = self.state.icons.get(id);
                }
            }
        } else {
            screen.launcher.set(false, scale, now);
            screen.session.set(true, scale, now);
            self.session.focused = 0;
        }
        screen.set_activatable(true);
        screen.dirty = true;
        let hwnd = screen.hwnd;
        let id = screen.id();
        let front = tracker::foreground_window();
        if !self.screens.iter().any(|s| s.id() == front) {
            self.previous_front = front;
        }
        self.keyboard = Some(id);
        take_focus(hwnd);
    }

    fn close_keyboard_drawers(&mut self) {
        let now = Instant::now();
        let scale = self.config.appearance.anim_scale;
        for screen in &mut self.screens {
            screen.launcher.set(false, scale, now);
            screen.session.set(false, scale, now);
            screen.set_activatable(false);
            screen.dirty = true;
        }
        self.keyboard = None;
    }

    fn toggle_launcher(&mut self) {
        if !self.config.launcher.enabled {
            return;
        }
        let open = self.screens.iter().any(|s| s.launcher.wanted);
        if open {
            self.close_keyboard_drawers();
        } else if let Some(i) = self.pointer_screen() {
            self.open_keyboard_drawer(i, true);
        }
    }

    fn toggle_session(&mut self) {
        if !self.config.session.enabled {
            return;
        }
        let open = self.screens.iter().any(|s| s.session.wanted);
        if open {
            self.close_keyboard_drawers();
        } else if let Some(i) = self.pointer_screen() {
            self.open_keyboard_drawer(i, false);
        }
    }

    fn toggle_dashboard(&mut self) {
        if !self.config.dashboard.enabled {
            return;
        }
        let Some(i) = self.pointer_screen() else {
            return;
        };
        let now = Instant::now();
        let scale = self.config.appearance.anim_scale;
        let screen = &mut self.screens[i];
        let open = !screen.dashboard.wanted;
        let launcher_open = screen.launcher.wanted;
        screen.dashboard.set(open, scale, now);
        screen.dashboard_pinned = open;
        screen.dirty = true;
        if open {
            self.state.usage = self.sampler.sample();
            // Never both: asking for the dashboard puts the launcher away.
            if launcher_open {
                self.close_keyboard_drawers();
            }
        }
    }

    /// Settings found a newer version (and the user installs by hand): its
    /// bubble comes out, unless the user closed this version's already.
    fn update_available(&mut self, version: &str) {
        let version = version.trim_start_matches('v');
        if !self.config.general.check_updates
            || self.update_check.dismissed.as_deref() == Some(version)
            || self.state.update.as_deref() == Some(version)
        {
            return;
        }
        self.state.update = Some(version.to_string());
        self.mark_all_dirty();
    }

    /// Once a day, while the user is away, starts Settings in the
    /// background: it looks for a newer version, then installs it or tells
    /// the shell (Settings → automatic updates). The shell itself never
    /// touches the network.
    fn maybe_check_for_update(&mut self) {
        if !self.config.general.check_updates
            || self.state.update.is_some()
            || !self.update_check.due(SystemTime::now())
            || idle_for() < UPDATE_IDLE
        {
            return;
        }
        let Some(settings) = settings_exe() else {
            return;
        };
        // Recorded first: a failing start is not retried every minute.
        self.update_check.last = Some(SystemTime::now());
        self.update_check
            .save(&update_check_path(&self.config_path));
        if let Err(error) = std::process::Command::new(&settings)
            .arg("--update")
            .spawn()
        {
            norbanner_shell::log::line(&format!("could not look for updates: {error}"));
        }
    }

    fn show_osd(&mut self) {
        if !self.config.osd.enabled {
            return;
        }
        let Some(i) = self.pointer_screen() else {
            return;
        };
        let now = Instant::now();
        let delay = Duration::from_millis(u64::from(self.config.osd.hide_delay));
        let screen = &mut self.screens[i];
        screen.osd_state.until = Some(now + delay);
        screen.osd.set(true, self.config.appearance.anim_scale, now);
        screen.dirty = true;
        // SAFETY: One-shot timer on our window (re-armed on every change).
        unsafe {
            SetTimer(
                Some(self.control),
                TIMER_OSD,
                self.config.osd.hide_delay + 20,
                None,
            );
        }
    }

    // ---- actions -------------------------------------------------------

    pub fn perform(&mut self, action: Action) {
        let now = Instant::now();
        match action {
            Action::ToggleLauncher => self.toggle_launcher(),
            Action::ToggleSession => self.toggle_session(),
            Action::ToggleDashboard => self.toggle_dashboard(),
            Action::CloseDrawers => {
                self.close_keyboard_drawers();
                let scale = self.config.appearance.anim_scale;
                for screen in &mut self.screens {
                    screen.dashboard.set(false, scale, now);
                    screen.dashboard_pinned = false;
                    screen.osd.set(false, scale, now);
                }
            }
            Action::Launch(id) => {
                self.launcher.record(&id);
                self.close_keyboard_drawers();
                if let Err(error) = shell_launcher::open(&id) {
                    norbanner_shell::log::line(&format!("{error}"));
                }
            }
            Action::LauncherRun(index) => match self.launcher.run(index, &self.config) {
                Some(action) => self.perform(action),
                None => {
                    self.launcher.refresh(&self.state.apps, &self.config);
                    self.mark_all_dirty();
                }
            },
            Action::LauncherListScroll(delta) => {
                self.launcher.scroll_list(delta * 3);
                self.request_visible_icons();
                self.mark_all_dirty();
            }
            Action::LauncherView(view) => {
                self.launcher.show(view);
                self.request_visible_icons();
                self.mark_all_dirty();
            }
            Action::Screenshot => {
                self.close_keyboard_drawers();
                open_uri("ms-screenclip:");
            }
            Action::ClipboardHistory | Action::EmojiPanel => {
                self.close_keyboard_drawers();
                // Give the keyboard back to the app in front, then open the
                // panel for it.
                let front = self.previous_front;
                if front != 0 {
                    shell_launcher::activate(&[front]);
                }
                let clipboard = action == Action::ClipboardHistory;
                let _ = thread::Builder::new().name("panel".into()).spawn(move || {
                    thread::sleep(Duration::from_millis(180));
                    if clipboard {
                        shell_launcher::open_clipboard_history();
                    } else {
                        shell_launcher::open_emoji_panel();
                    }
                });
            }
            Action::WebSearch(query) => {
                self.close_keyboard_drawers();
                let encoded: String = query
                    .bytes()
                    .map(|b| match b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                            (b as char).to_string()
                        }
                        b' ' => "+".to_string(),
                        _ => format!("%{b:02X}"),
                    })
                    .collect();
                open_uri(&format!("https://www.google.com/search?q={encoded}"));
            }
            Action::OpenPath(path) => {
                self.close_keyboard_drawers();
                if let Err(error) = shell_launcher::open(&path.display().to_string()) {
                    norbanner_shell::log::line(&format!("{error}"));
                }
            }
            Action::LauncherSelect(index) => {
                self.launcher.selected = index.min(self.launcher.items.len().saturating_sub(1));
                self.mark_all_dirty();
            }
            Action::Session(action) => {
                self.perform(Action::CloseDrawers);
                if let Err(error) = shell_session::run(action) {
                    norbanner_shell::log::line(&format!("session action failed: {error}"));
                }
            }
            Action::ActivateApp { desktop, windows } => self.activate_app(desktop, &windows),
            Action::StepDesktop(delta) => desktops::step(delta),
            Action::Media(control) => {
                if let (Some(watch), Some(track)) = (&self.media, &self.state.media) {
                    watch.send(track, control);
                }
            }
            Action::ToggleLyrics => {
                self.save_config(|c| c.dashboard.lyrics = !c.dashboard.lyrics);
                self.sync_lyrics();
                self.mark_all_dirty();
            }
            Action::LyricsOffset(step) => {
                if let Some(track) = &self.state.media {
                    let Some(song) = song_of(track) else {
                        return;
                    };
                    let offset = self.lyrics_offsets.entry(song).or_insert(0);
                    *offset = (*offset + step * 250).clamp(-10_000, 10_000);
                }
                self.sync_lyrics();
            }
            Action::ToggleMute => {
                if let Some(audio) = &mut self.audio {
                    let _ = audio.toggle_mute();
                }
            }
            Action::VolumeStep(delta) => {
                if let Some(audio) = &mut self.audio
                    && let Some(volume) = self.state.volume
                {
                    let step = (self.config.services.audio_increment * 100.0).round() as i32;
                    let max = (self.config.services.max_volume * 100.0).round() as i32;
                    let next = (i32::from(volume.percent) + delta * step).clamp(0, max);
                    let _ = audio.set_percent(next as u8);
                }
                self.show_osd();
            }
            Action::BrightnessStep(monitor, delta) => {
                let step = self.config.services.brightness_increment;
                if let Some(value) = self.state.brightness.get(&monitor).copied() {
                    self.set_brightness(monitor, value + delta as f32 * step);
                }
                self.show_osd();
            }
            Action::OpenTrayApp(path) => {
                // Its window if it has one open; otherwise open the program
                // again, which brings most tray apps back.
                let window = self
                    .state
                    .desktop_apps
                    .iter()
                    .flatten()
                    .find(|app| app.app_id.eq_ignore_ascii_case(&path))
                    .and_then(|app| app.windows.first().copied());
                match window {
                    Some(hwnd) => self.perform(Action::FocusWindow(hwnd)),
                    None => {
                        if let Err(error) = shell_launcher::open(&path) {
                            norbanner_shell::log::line(&format!("could not open {path}: {error}"));
                        }
                    }
                }
                for screen in &mut self.screens {
                    screen
                        .tray
                        .set(false, self.config.appearance.anim_scale, Instant::now());
                }
                self.mark_all_dirty();
            }
            Action::CloseTrayApp(path) => {
                // It leaves the bubble at once; the list is read again once
                // the app has closed.
                self.state.tray.retain(|app| app.path != path);
                self.mark_all_dirty();
                let tx = self.work_tx.clone();
                let control = self.control.0 as isize;
                let _ = thread::Builder::new()
                    .name("tray-quit".into())
                    .spawn(move || {
                        norbanner_shell::tray_apps::quit(&path);
                        // SAFETY: COM for this worker thread only (icons decode with WIC).
                        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                        let _ = tx.send(Work::Tray(norbanner_shell::tray_apps::running()));
                        // SAFETY: As in `start_workers`.
                        unsafe {
                            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                Some(HWND(control as *mut _)),
                                WM_WORKER,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                    });
            }
            Action::ShowTrayBubble => {
                // The bubble already opens on hover; a click (or a touch)
                // opens it too, on the screen under the pointer.
                if let Some(i) = self.pointer_screen() {
                    let scale = self.config.appearance.anim_scale;
                    let screen = &mut self.screens[i];
                    if !screen.tray.wanted {
                        screen.tray.set(true, scale, Instant::now());
                        screen.dirty = true;
                        self.request_tray();
                    }
                }
            }
            Action::OpenNetworkSettings => open_uri("ms-settings:network"),
            Action::OpenBluetoothSettings => open_uri("ms-settings:bluetooth"),
            Action::OpenSoundSettings => open_uri("ms-settings:sound"),
            Action::OpenPowerSettings => open_uri("ms-settings:powersleep"),
            Action::OpenDateSettings => open_uri("ms-settings:dateandtime"),
            Action::OpenSettings => {
                self.close_keyboard_drawers();
                match settings_exe() {
                    Some(path) => open_uri(&path.display().to_string()),
                    None => open_uri(&self.config_path.display().to_string()),
                }
            }
            Action::InstallUpdate => {
                // Settings finds the same version and offers to install it.
                self.state.update = None;
                self.mark_all_dirty();
                self.perform(Action::OpenSettings);
            }
            Action::DismissUpdate => {
                if let Some(version) = self.state.update.take() {
                    self.update_check.dismissed = Some(version);
                    self.update_check
                        .save(&update_check_path(&self.config_path));
                }
                self.mark_all_dirty();
            }
            Action::FocusWindow(hwnd) => shell_launcher::activate(&[hwnd]),
            Action::CloseWindow(hwnd) => shell_launcher::close(&[hwnd]),
            Action::CalendarMonth(delta) => {
                for screen in &mut self.screens {
                    if screen.dashboard.wanted {
                        screen.dash_state.month_offset += delta;
                        screen.dirty = true;
                    }
                }
            }
            Action::SetWallpaper(path) => {
                self.close_keyboard_drawers();
                if let Err(error) = norbanner_shell::wallpaper::set(&path) {
                    norbanner_shell::log::line(&format!("could not set the wallpaper: {error}"));
                }
                self.wallpaper_changed(true);
            }
            Action::SetVariant(variant) => {
                self.save_config(|c| c.theme.variant = variant);
                self.recompute_theme();
                self.launcher.refresh(&self.state.apps, &self.config);
            }
            Action::SetMode(dark) => {
                self.save_config(|c| c.theme.mode = if dark { Mode::Dark } else { Mode::Light });
                self.recompute_theme();
            }
            Action::CopyText(text) => {
                copy_to_clipboard(self.control, &text);
                self.close_keyboard_drawers();
            }
            Action::Quit => self.quit = true,
        }
        self.mark_all_dirty();
    }

    fn activate_app(&mut self, desktop: usize, windows: &[isize]) {
        if windows.is_empty() {
            return;
        }
        if desktop != self.state.desktops.current {
            // Activating a window on another desktop does not always switch
            // there: switch first, then activate when the switch lands.
            self.pending_activation = Some((desktop, windows.to_vec()));
            desktops::go_to(desktop);
            return;
        }
        let front = tracker::foreground_window();
        match windows.iter().position(|&w| w == front) {
            // In front with one window: minimize, as the taskbar does.
            Some(_) if windows.len() == 1 => shell_launcher::minimize(windows),
            // In front with several: the next one.
            Some(i) => shell_launcher::activate(&[windows[(i + 1) % windows.len()]]),
            None => shell_launcher::activate(windows),
        }
    }

    fn set_brightness(&mut self, monitor: isize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        self.state.brightness.insert(monitor, value);
        if let Some(tx) = &self.brightness_tx {
            let _ = tx.send((monitor, Some(value)));
        }
        self.mark_all_dirty();
    }

    fn slide(&mut self, drag: Drag, y: f32) {
        let value = ((drag.rect.bottom() - y) / drag.rect.h).clamp(0.0, 1.0);
        match drag.slider {
            Slider::Volume => {
                if let Some(audio) = &mut self.audio {
                    let max = self.config.services.max_volume;
                    let _ = audio.set_percent((value * max * 100.0).round() as u8);
                }
            }
            Slider::Brightness(monitor) => self.set_brightness(monitor, value),
        }
        if let Some(i) = self.pointer_screen() {
            let delay = Duration::from_millis(u64::from(self.config.osd.hide_delay));
            self.screens[i].osd_state.until = Some(Instant::now() + delay);
        }
    }

    fn command(&mut self, command: &str) {
        let words: Vec<&str> = command.split_whitespace().collect();
        match words.as_slice() {
            ["drawers", "close"] => self.perform(Action::CloseDrawers),
            ["drawers", verb, name, ..] | [name, verb]
                if matches!(*verb, "toggle" | "open" | "close") =>
            {
                let name = name.trim();
                let open_now = match name {
                    "launcher" => self.screens.iter().any(|s| s.launcher.wanted),
                    "session" => self.screens.iter().any(|s| s.session.wanted),
                    "dashboard" => self.screens.iter().any(|s| s.dashboard.wanted),
                    _ => false,
                };
                let want = match *verb {
                    "open" => true,
                    "close" => false,
                    _ => !open_now,
                };
                if want != open_now {
                    match name {
                        "launcher" => self.toggle_launcher(),
                        "session" => self.toggle_session(),
                        "dashboard" => self.toggle_dashboard(),
                        _ => {}
                    }
                }
            }
            ["quit"] => self.quit = true,
            ["reload"] => self.reload_config(),
            ["osd", "show"] => self.show_osd(),
            ["update", "available", version] => self.update_available(version),
            ["wallpaper", "set", rest @ ..] => {
                self.perform(Action::SetWallpaper(PathBuf::from(rest.join(" "))))
            }
            ["scheme", "variant", v] => self.perform(Action::SetVariant((*v).to_string())),
            ["scheme", "mode", m] => self.perform(Action::SetMode(*m != "light")),
            ["mpris", "playPause"] => self.perform(Action::Media(Control::PlayPause)),
            ["mpris", "next"] => self.perform(Action::Media(Control::Next)),
            ["mpris", "previous"] => self.perform(Action::Media(Control::Previous)),
            ["lyrics", "toggle"] => self.perform(Action::ToggleLyrics),
            ["workspace", n] => {
                if let Ok(n) = n.parse::<usize>() {
                    desktops::go_to(n.saturating_sub(1));
                }
            }
            _ => {}
        }
    }

    // ---- events ----------------------------------------------------------

    pub fn handle(&mut self, event: Event) {
        let now = Instant::now();
        match event {
            Event::MouseMove { hwnd, x, y } => {
                if let Some(i) = self.screen_index(hwnd) {
                    let (lx, ly) = self.screens[i].to_logical(x, y);
                    if let Some(drag) = self.screens[i].drag {
                        self.slide(drag, ly);
                    }
                    self.pointer_moved(i, Some((lx, ly)));
                }
            }
            Event::MouseDown { hwnd, x, y, right } => {
                let Some(i) = self.screen_index(hwnd) else {
                    return;
                };
                let (lx, ly) = self.screens[i].to_logical(x, y);
                let screen = &mut self.screens[i];
                screen.pointer = Some((lx, ly));
                if right {
                    return;
                }
                let hit = screen
                    .hit_at(lx, ly, |k| {
                        matches!(k, HitKind::Click(_) | HitKind::VSlider(_))
                    })
                    .cloned();
                match hit.map(|h| (h.rect, h.kind)) {
                    Some((_, HitKind::Click(action))) => screen.pressed = Some(action),
                    Some((rect, HitKind::VSlider(slider))) => {
                        let drag = Drag { rect, slider };
                        screen.drag = Some(drag);
                        self.slide(drag, ly);
                    }
                    _ => {}
                }
                self.screens[i].dirty = true;
            }
            Event::MouseUp { hwnd, x, y, right } => {
                let Some(i) = self.screen_index(hwnd) else {
                    return;
                };
                let (lx, ly) = self.screens[i].to_logical(x, y);
                let screen = &mut self.screens[i];
                screen.drag = None;
                let pressed = screen.pressed.take();
                screen.dirty = true;
                if right {
                    return;
                }
                let released = screen
                    .hit_at(lx, ly, |k| matches!(k, HitKind::Click(_)))
                    .and_then(|h| match &h.kind {
                        HitKind::Click(action) => Some(action.clone()),
                        _ => None,
                    });
                if let (Some(pressed), Some(released)) = (pressed, released)
                    && pressed == released
                {
                    self.perform(pressed);
                }
            }
            Event::Wheel { hwnd, x, y, delta } => {
                let Some(i) = self.screen_index(hwnd) else {
                    return;
                };
                let screen = &self.screens[i];
                // Wheel coordinates are screen coordinates.
                let (lx, ly) = screen.to_logical(x - screen.rect.left, y - screen.rect.top);
                let up = delta > 0;
                let target = screen
                    .hit_at(lx, ly, |k| matches!(k, HitKind::Scroll(_)))
                    .and_then(|h| match &h.kind {
                        HitKind::Scroll(action) => Some(action.clone()),
                        _ => None,
                    });
                let step = if up { 1 } else { -1 };
                match target {
                    Some(Action::VolumeStep(_)) => self.perform(Action::VolumeStep(step)),
                    Some(Action::BrightnessStep(m, _)) => {
                        self.perform(Action::BrightnessStep(m, step))
                    }
                    Some(Action::StepDesktop(_)) => self.perform(Action::StepDesktop(-step)),
                    Some(Action::CalendarMonth(_)) => self.perform(Action::CalendarMonth(-step)),
                    Some(Action::LyricsOffset(_)) => self.perform(Action::LyricsOffset(step)),
                    Some(Action::LauncherListScroll(_)) => {
                        self.perform(Action::LauncherListScroll(-step));
                    }
                    Some(Action::LauncherSelect(_)) => {
                        self.launcher
                            .scroll(-step, self.config.launcher.max_shown as usize);
                        self.mark_all_dirty();
                    }
                    _ => {}
                }
            }
            Event::MouseLeave { .. } => {}
            Event::Key { vk, ctrl, shift } => self.key(vk, ctrl, shift),
            Event::Char(c) => {
                if self.screens.iter().any(|s| s.launcher.wanted) {
                    self.launcher.type_text(&c.to_string());
                    self.launcher.refresh(&self.state.apps, &self.config);
                    self.request_visible_icons();
                    self.mark_all_dirty();
                }
            }
            Event::Deactivated { hwnd } => {
                if self.keyboard == Some(hwnd) {
                    self.close_keyboard_drawers();
                }
            }
            Event::Command(command) => self.command(&command),
            Event::Service(message) => self.service(message),
            Event::SettingChange { action, area } => {
                // SPI_SETDESKWALLPAPER
                if action == 0x0014 {
                    self.wallpaper_changed(true);
                }
                if area == "ImmersiveColorSet" {
                    self.recompute_theme();
                }
            }
            Event::DisplayChange => {
                if let Err(error) = self.sync_screens() {
                    norbanner_shell::log::line(&format!(
                        "could not follow the displays: {error:#}"
                    ));
                }
                self.reserve_space();
                self.mark_all_dirty();
            }
            Event::Timer(TIMER_POLL) => self.poll_pointer(),
            Event::Timer(TIMER_SECOND) => self.second(),
            Event::Timer(TIMER_WINDOWS) => {
                // SAFETY: One-shot: stop it.
                unsafe {
                    let _ = KillTimer(Some(self.control), TIMER_WINDOWS);
                }
                self.refresh_desktops();
            }
            Event::Timer(TIMER_WALLPAPER) => {
                // SAFETY: One-shot: stop it.
                unsafe {
                    let _ = KillTimer(Some(self.control), TIMER_WALLPAPER);
                }
                self.wallpaper_changed(false);
            }
            Event::Timer(TIMER_LYRICS) => self.sync_lyrics(),
            Event::Timer(TIMER_WAVE) => {
                for screen in &mut self.screens {
                    screen.dirty |= screen.lyrics.wanted;
                }
            }
            Event::Timer(TIMER_OSD) => {
                // SAFETY: One-shot: stop it.
                unsafe {
                    let _ = KillTimer(Some(self.control), TIMER_OSD);
                }
                self.hover(now);
            }
            Event::Timer(_) => {}
            Event::SuperTap | Event::Hotkey(hotkeys::HOTKEY_LAUNCHER) => self.toggle_launcher(),
            Event::Hotkey(hotkeys::HOTKEY_SESSION) => self.toggle_session(),
            Event::Workspace(index) => desktops::go_to(index),
            Event::Hotkey(_) => {}
            Event::Foreground => self.refresh_active(),
            Event::WindowsChanged => {
                // Bursts of window events: refresh once they settle.
                // SAFETY: Timer on our own window.
                unsafe {
                    SetTimer(Some(self.control), TIMER_WINDOWS, 250, None);
                }
            }
            Event::TaskbarCreated => {
                if self.config.general.replace_taskbar {
                    taskbar::take_over();
                }
                // Explorer restarted and forgot every AppBar.
                self.reserve_space_with(true);
            }
            Event::Quit => self.quit = true,
        }
    }

    fn request_visible_icons(&self) {
        let shown = (self.config.launcher.max_shown as usize)
            .max(crate::modules::launcher::GRID_COLUMNS * crate::modules::launcher::GRID_ROWS);
        for item in self
            .launcher
            .items
            .iter()
            .skip(self.launcher.first)
            .take(shown)
        {
            if let crate::modules::launcher::Kind::App(id) = &item.kind {
                let _ = self.state.icons.get(id);
            }
        }
    }

    fn key(&mut self, vk: u16, ctrl: bool, shift: bool) {
        let launcher_open = self.screens.iter().any(|s| s.launcher.wanted);
        let session_open = self.screens.iter().any(|s| s.session.wanted);
        let shown = self.config.launcher.max_shown as usize;
        let vk = windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk);
        if launcher_open {
            match vk {
                VK_ESCAPE | VK_BACK
                    if self.launcher.home
                        && self.launcher.view != crate::modules::launcher::View::Categories =>
                {
                    self.launcher
                        .show(crate::modules::launcher::View::Categories);
                }
                VK_ESCAPE => self.close_keyboard_drawers(),
                VK_RETURN => self.perform(Action::LauncherRun(self.launcher.selected)),
                VK_UP if self.launcher.home => self.launcher.move_grid(0, -1),
                VK_DOWN if self.launcher.home => self.launcher.move_grid(0, 1),
                VK_LEFT if self.launcher.home => self.launcher.move_grid(-1, 0),
                VK_RIGHT if self.launcher.home => self.launcher.move_grid(1, 0),
                VK_UP => self.launcher.move_selection(-1, shown),
                VK_DOWN => self.launcher.move_selection(1, shown),
                VK_TAB => self
                    .launcher
                    .move_selection(if shift { -1 } else { 1 }, shown),
                VK_BACK => {
                    self.launcher.backspace(ctrl);
                    self.launcher.refresh(&self.state.apps, &self.config);
                }
                _ if ctrl && vk.0 == u16::from(b'J') => self.launcher.move_selection(1, shown),
                _ if ctrl && vk.0 == u16::from(b'K') => self.launcher.move_selection(-1, shown),
                _ => return,
            }
            self.request_visible_icons();
        } else if session_open {
            match vk {
                VK_ESCAPE => self.close_keyboard_drawers(),
                VK_UP => self.session.move_focus(-1),
                VK_DOWN | VK_TAB => self.session.move_focus(if shift { -1 } else { 1 }),
                VK_RETURN => {
                    let (action, _) = crate::modules::session::BUTTONS[self.session.focused];
                    self.perform(Action::Session(action));
                }
                _ => return,
            }
        }
        self.mark_all_dirty();
    }

    fn service(&mut self, message: u32) {
        match message {
            WM_AUDIO => {
                let before = self.state.volume;
                self.state.volume = self.audio.as_mut().and_then(AudioWatch::volume);
                let dragging = self.screens.iter().any(|s| s.drag.is_some());
                if before.is_some() && before != self.state.volume && !dragging {
                    self.show_osd();
                }
            }
            WM_NETWORK => self.state.network = Some(norbanner_shell::status::network()),
            WM_BLUETOOTH => {
                self.state.bluetooth = self.bluetooth.as_ref().map(bluetooth::Monitor::is_on)
            }
            WM_MEDIA => {
                self.state.media = self.media.as_ref().and_then(media::Watch::snapshot);
                self.sync_lyrics();
            }
            WM_LYRICS => {
                if let Some(fetcher) = &mut self.lyrics {
                    fetcher.collect();
                }
                self.sync_lyrics();
            }
            WM_DESKTOPS => {
                if let Some(watch) = &mut self.desktops_watch {
                    watch.arm();
                }
                self.refresh_desktops();
            }
            WM_WALLPAPER => {
                if let Some(watch) = &mut self.wallpaper_watch {
                    watch.arm();
                }
                // SAFETY: One-shot timer on our window, restarted by each change.
                unsafe {
                    SetTimer(Some(self.control), TIMER_WALLPAPER, 400, None);
                }
            }
            WM_LOCK_KEYS => self.refresh_lock_keys(),
            WM_SYSTEM_OSD => {
                // The brightness keys change it behind the shell's back:
                // read it again (the volume has its own notification).
                if let Some(tx) = &self.brightness_tx {
                    for screen in &self.screens {
                        let _ = tx.send((screen.monitor, None));
                    }
                }
                self.show_osd();
            }
            WM_CONFIG => {
                self.reload_config();
                self.sync_lyrics();
            }
            crate::state::WM_ICONS => {
                if self.state.icons.receive() {
                    self.mark_all_dirty();
                }
            }
            WM_WORKER => self.receive_work(),
            _ => {}
        }
        self.mark_all_dirty();
    }

    fn receive_work(&mut self) {
        while let Ok(work) = self.work_rx.try_recv() {
            match work {
                Work::Theme(source) => {
                    self.wallpaper_source = Some(source);
                    self.recompute_theme();
                }
                Work::Apps(apps, pins, used) => {
                    self.state.apps = Arc::new(apps);
                    self.launcher.taskbar = pins.iter().map(|p| p.id.clone()).collect();
                    self.pins = pins;
                    self.refresh_desktops();
                    self.launcher.windows_usage = used;
                    self.launcher.refresh(&self.state.apps, &self.config);
                }
                Work::Avatar(image) => self.state.avatar = Some(Arc::new(image)),
                Work::Logo(image) => self.state.logo = Some(Arc::new(image)),
                Work::Backdrop(image) => {
                    let next = self.state.backdrop.as_ref().map_or(0, |(n, _)| n + 1);
                    self.state.backdrop = Some((next, Arc::new(image)));
                    self.mark_all_dirty();
                }
                Work::Previews(pictures) => {
                    for (hwnd, picture) in pictures {
                        let Some(picture) = picture else { continue };
                        self.preview_counter += 1;
                        let key = crate::state::image_key(&format!(
                            "preview:{hwnd}:{}",
                            self.preview_counter
                        ));
                        if let Some((old, _)) =
                            self.state.previews.insert(hwnd, (key, Arc::new(picture)))
                        {
                            paint::forget_image(old);
                        }
                    }
                    self.mark_all_dirty();
                }
                Work::Tray(apps) => {
                    self.state.tray = apps
                        .into_iter()
                        .map(|app| crate::state::TrayEntry {
                            key: crate::state::image_key(&format!(
                                "tray:{}:{}",
                                app.path,
                                app.icon.as_ref().map_or(0, |i| i.bgra.len())
                            )),
                            path: app.path,
                            name: app.name,
                            icon: app.icon.map(Arc::new),
                        })
                        .collect();
                    self.mark_all_dirty();
                }
                Work::Brightness(monitor, value) => {
                    self.state.brightness.insert(monitor, value);
                }
            }
        }
    }

    /// Once a second: the clock, battery, resources and the config file.
    fn second(&mut self) {
        let clock = Clock::now();
        // Some changes (a slideshow, Spotlight) may not announce themselves.
        if clock.second.is_multiple_of(10) {
            self.wallpaper_changed(false);
        }
        let show_seconds = self.config.bar.clock.show_seconds;
        if clock.minute != self.state.clock.minute
            || show_seconds
            || clock.day != self.state.clock.day
        {
            self.mark_all_dirty();
        }
        self.state.clock = clock;
        let battery = system::battery();
        if battery != self.state.battery {
            self.state.battery = battery;
            self.mark_all_dirty();
        }
        let dashboard_open = self
            .screens
            .iter()
            .any(|s| s.dashboard.wanted || s.launcher.wanted);
        if dashboard_open {
            self.state.usage = self.sampler.sample();
            for screen in &mut self.screens {
                if screen.dashboard.wanted || screen.launcher.wanted {
                    screen.dirty = true;
                }
            }
        }
        let layout = system::input_language();
        if layout != self.state.kb_layout {
            self.state.kb_layout = layout;
            self.mark_all_dirty();
        }
        // Hand edits to shell.json apply live.
        let stamp = std::fs::metadata(&self.config_path)
            .and_then(|m| m.modified())
            .ok();
        if stamp != self.config_stamp {
            self.config_stamp = stamp;
            self.reload_config();
        }
        if self.config.general.replace_taskbar && taskbar::needs_visibility_repair() {
            taskbar::take_over();
        }
        if clock.second == 0 {
            self.maybe_check_for_update();
        }
        // Full-screen state also changes without a foreground change
        // (a video going full screen).
        let now = Instant::now();
        for screen in &mut self.screens {
            let fullscreen = tracker::foreground_is_fullscreen(screen.rect);
            screen.set_fullscreen(fullscreen, self.config.appearance.anim_scale, now);
        }
    }

    fn poll_pointer(&mut self) {
        let mut point = POINT::default();
        // SAFETY: Out-pointer is a valid local.
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return;
        }
        for i in 0..self.screens.len() {
            let s = &self.screens[i];
            let inside = point.x >= s.rect.left
                && point.x < s.rect.right
                && point.y >= s.rect.top
                && point.y < s.rect.bottom;
            let logical = inside.then(|| s.to_logical(point.x - s.rect.left, point.y - s.rect.top));
            self.pointer_moved(i, logical);
        }
    }

    fn pointer_moved(&mut self, index: usize, pointer: Option<(f32, f32)>) {
        let screen = &mut self.screens[index];
        let previous_hit = screen
            .pointer
            .and_then(|(x, y)| screen.hit_at(x, y, |_| true).map(|h| h.rect));
        let hit = pointer.and_then(|(x, y)| screen.hit_at(x, y, |_| true).map(|h| h.rect));
        if previous_hit != hit {
            screen.dirty = true;
        }
        screen.pointer = pointer;
        screen.update_transparency(pointer);
        self.hover(Instant::now());
    }

    /// Opens and closes the drawers that follow the pointer.
    fn hover(&mut self, now: Instant) {
        let config = &self.config;
        let scale = config.appearance.anim_scale;
        let rounding = config.border.rounding;
        let mut wanted_previews: Option<Vec<isize>> = None;
        let mut wanted_tray = false;
        for screen in &mut self.screens {
            let t = Tokens::for_scale(config, screen.scale);
            let full_bar = bar::width(t.bar_inner, config.border.thickness, t.padding.small);
            let pointer = screen.pointer;
            let p = screen.placements;
            if screen.is_fullscreen {
                continue;
            }
            // Dashboard: the top edge above it, and the drawer itself.
            if config.dashboard.enabled && config.dashboard.show_on_hover {
                let open = p.dashboard_open;
                let shown = if screen.dashboard.wanted {
                    p.dashboard.clip.h.max(open.h * 0.5)
                } else {
                    0.0
                };
                // Not over an open launcher: the two would collide.
                let reachable =
                    (screen.dashboard.wanted || !screen.top_shared) && !screen.launcher.wanted;
                let inside = reachable
                    && pointer.is_some_and(|(x, y)| {
                        x >= open.x - rounding
                            && x <= open.right() + rounding
                            && y < p.border.max(3.0) + shown
                    });
                if screen.dashboard_pinned {
                    if inside {
                        screen.dashboard_pinned = false;
                    }
                } else if inside != screen.dashboard.wanted {
                    screen.dashboard.set(inside, scale, now);
                    screen.dirty = true;
                    if inside {
                        self.state.usage = self.sampler.sample();
                    }
                }
            }
            // OSD: the right edge beside it, the drawer itself, or a recent
            // volume change.
            if config.osd.enabled {
                let (w, _) = p.size;
                let open = p.osd_open;
                let shown = if screen.osd.wanted { open.w } else { 0.0 };
                let reachable = screen.osd.wanted || !screen.right_shared;
                let hovered = reachable
                    && pointer.is_some_and(|(x, y)| {
                        x > w - p.border.max(3.0) - shown && y >= open.y && y <= open.bottom()
                    });
                let timed = screen.osd_state.until.is_some_and(|until| now < until);
                let dragging = screen.drag.is_some();
                let want = hovered || timed || dragging;
                if want != screen.osd.wanted {
                    screen.osd.set(want, scale, now);
                    screen.dirty = true;
                }
            }
            // Update bubble: out while a newer version waits.
            let want = self.state.update.is_some();
            if want != screen.update.wanted {
                screen.update.set(want, scale, now);
                screen.dirty = true;
            }
            // Background apps bubble: over the tray button or the bubble.
            {
                let button = screen.bar_state.tray;
                let bubble = p.tray.content;
                let over_button =
                    button.is_some_and(|r| pointer.is_some_and(|(x, y)| r.contains(x, y)));
                let over_bubble = screen.tray.wanted
                    && pointer.is_some_and(|(x, y)| {
                        x <= bubble.right() && y >= bubble.y && y <= bubble.bottom()
                    });
                let want = over_button || over_bubble;
                if want != screen.tray.wanted {
                    screen.tray.set(want, scale, now);
                    screen.dirty = true;
                    if want {
                        wanted_tray = true;
                    }
                }
            }
            // Windows panel: over an app tile (which one it lists follows
            // the pointer), or over the panel itself.
            {
                let tile = pointer.and_then(|(x, y)| {
                    screen
                        .bar_state
                        .app_tiles
                        .iter()
                        // A pinned app with no window has nothing to list.
                        .find(|(rect, app)| rect.contains(x, y) && !app.windows.is_empty())
                        .cloned()
                });
                let panel = p.windows.content;
                let over_panel = screen.windows.wanted
                    && pointer.is_some_and(|(x, y)| {
                        x <= panel.right() && y >= panel.y && y <= panel.bottom()
                    });
                if let Some((rect, app)) = &tile {
                    let changed = screen
                        .window_app
                        .as_ref()
                        .is_none_or(|(_, shown)| shown.app_id != app.app_id);
                    if changed {
                        screen.window_app = Some((rect.center().1, app.clone()));
                        screen.dirty = true;
                        wanted_previews = Some(app.windows.clone());
                    }
                }
                let want = tile.is_some() || over_panel;
                if want != screen.windows.wanted {
                    screen.windows.set(want, scale, now);
                    screen.dirty = true;
                    // Fresh pictures each time the panel opens.
                    if want && wanted_previews.is_none() {
                        wanted_previews = screen
                            .window_app
                            .as_ref()
                            .map(|(_, app)| app.windows.clone());
                    }
                }
            }
            // A non-persistent bar slides out on hover.
            if !config.bar.persistent && config.bar.show_on_hover {
                let reach = if screen.bar_hovered {
                    full_bar
                } else {
                    config.border.thickness.max(3.0)
                };
                let hovered = pointer.is_some_and(|(x, _)| x < reach);
                if hovered != screen.bar_hovered {
                    screen.bar_hovered = hovered;
                    screen.dirty = true;
                }
            }
        }
        if let Some(windows) = wanted_previews {
            self.request_previews(windows);
        }
        if wanted_tray {
            self.request_tray();
        }
    }

    // ---- frames ----------------------------------------------------------

    /// Draws every screen that changed. Returns whether another frame is
    /// needed right away (something is animating).
    pub fn render(&mut self) -> bool {
        let now = Instant::now();
        let dashboard_open = self
            .screens
            .iter()
            .any(|s| s.dashboard.wanted || s.launcher.wanted);
        // The lyrics follow the song's position too.
        let lyrics_on = self.config.dashboard.lyrics && self.state.media.is_some();
        let progress = dashboard_open || lyrics_on;
        if progress != self.media_progress {
            self.media_progress = progress;
            if let Some(media) = &self.media {
                media.show_progress(progress);
            }
        }
        // The island shows on the bare desktop only (no app in front, so it
        // never sits over work), and gives way to the dashboard, which
        // opens over the same spot.
        let anim_scale = self.config.appearance.anim_scale;
        let on_desktop = self.state.active.is_none();
        let line_moving = self
            .state
            .lyrics
            .as_ref()
            .is_some_and(|view| crate::modules::lyrics::moving(view, anim_scale, now));
        for screen in &mut self.screens {
            let want = lyrics_on && on_desktop && !screen.dashboard.wanted && !screen.is_fullscreen;
            if want != screen.lyrics.wanted {
                screen.lyrics.set(want, anim_scale, now);
                screen.dirty = true;
            }
            if line_moving && screen.lyrics.wanted {
                screen.dirty = true;
            }
        }
        let mut animating = false;
        let playing = self
            .state
            .media
            .as_ref()
            .and_then(|track| track.timeline)
            .is_some_and(|timeline| timeline.advancing(now));
        for screen in &mut self.screens {
            let moving = screen.animating(now);
            if !(screen.dirty || moving) {
                continue;
            }
            let tokens = Tokens::for_scale(&self.config, screen.scale);
            if let Err(error) = screen.draw(
                &self.gpu,
                &self.config,
                &tokens,
                &self.theme.palette,
                &self.state,
                &self.launcher,
                &self.session,
                now,
            ) {
                norbanner_shell::log::line(&format!("could not draw: {error:#}"));
                if self.gpu.is_lost() {
                    // The guard restarts the shell with a fresh device.
                    self.quit = true;
                }
            }
            animating |= screen.animating(now);
        }
        // The progress arc moves smoothly while a dashboard shows it.
        animating |= dashboard_open && playing;
        // A line slides in for a moment; then nothing until the next one.
        let island = self.screens.iter().any(|s| s.lyrics.wanted);
        animating |= line_moving && island;
        // The wave moves at its own low rate, only while a song plays.
        let wave = island && self.state.media.as_ref().is_some_and(|t| t.playing);
        if wave != self.wave_running {
            self.wave_running = wave;
            // SAFETY: Starts or stops a timer on our own window.
            unsafe {
                if wave {
                    SetTimer(
                        Some(self.control),
                        TIMER_WAVE,
                        crate::modules::lyrics::WAVE_FRAME.as_millis() as u32,
                        None,
                    );
                } else {
                    let _ = KillTimer(Some(self.control), TIMER_WAVE);
                }
            }
        }
        if dashboard_open && playing {
            for screen in &mut self.screens {
                screen.dirty |= screen.dashboard.wanted || screen.launcher.wanted;
            }
        }
        animating
    }

    pub fn wants_frame(&self) -> bool {
        let now = Instant::now();
        self.screens.iter().any(|s| s.dirty || s.animating(now))
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for hook in self.win_hooks.drain(..) {
            // SAFETY: Removes hooks installed in `install_win_events`.
            unsafe {
                let _ = UnhookWinEvent(hook);
            }
        }
        self.screens.clear();
        // Work area first: giving the taskbar back then lets Explorer
        // reserve the taskbar's strip itself.
        workarea::restore_all();
        if self.config.general.replace_taskbar {
            taskbar::give_back_on_exit();
        }
        // SAFETY: Ends the message loop if it is still running.
        unsafe { PostQuitMessage(0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_its_commands() {
        assert!(is_known_command("drawers toggle launcher"));
        assert!(is_known_command("launcher toggle"));
        assert!(is_known_command("scheme variant vibrant"));
        assert!(is_known_command("wallpaper set C:\\walls\\a b.jpg"));
        assert!(!is_known_command("rm -rf"));
        assert!(is_known_command("drawers close"));
        assert!(is_known_command("osd show"));
        assert!(is_known_command("update available 0.1.36"));
        assert!(!is_known_command("update available"));
        assert!(!is_known_command(""));
    }

    #[test]
    fn the_update_check_survives_a_restart() {
        let check = UpdateCheck {
            last: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
            dismissed: Some("0.1.36".into()),
        };
        assert_eq!(UpdateCheck::parse(&check.text()), check);
        // Nothing saved yet, or a damaged file: look now, nothing closed.
        assert_eq!(UpdateCheck::parse(""), UpdateCheck::default());
        assert_eq!(UpdateCheck::parse("garbage"), UpdateCheck::default());
    }

    #[test]
    fn the_update_check_comes_once_a_day() {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let check = UpdateCheck {
            last: Some(last),
            dismissed: None,
        };
        assert!(UpdateCheck::default().due(last));
        assert!(!check.due(last + Duration::from_secs(60 * 60)));
        assert!(check.due(last + UPDATE_EVERY));
        // A clock set back does not hold the next look off.
        assert!(check.due(last - Duration::from_secs(60)));
    }
}

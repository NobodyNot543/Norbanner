//! Everything the user can ask the shell to do, from a click, a key, a
//! hotkey or an IPC command.

use norbanner_shell::session;

use crate::media::Control;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    ToggleLauncher,
    ToggleSession,
    ToggleDashboard,
    CloseDrawers,
    /// Opens the app with this shell id and closes the launcher.
    Launch(String),
    /// Runs the launcher row at this index.
    LauncherRun(usize),
    LauncherSelect(usize),
    /// Scrolls the launcher's app list (the wheel gives the direction).
    LauncherListScroll(i32),
    /// Shows categories, one category or the A–Z list in the launcher.
    LauncherView(crate::modules::launcher::View),
    /// Opens a file (a recent document's shortcut) and closes the launcher.
    OpenPath(std::path::PathBuf),
    /// Windows' screen snip overlay.
    Screenshot,
    /// Windows' clipboard history, for the window that was in front.
    ClipboardHistory,
    /// Windows' emoji panel, for the window that was in front.
    EmojiPanel,
    /// Opens a web search for this text.
    WebSearch(String),
    Session(session::Action),
    /// Brings an app's windows forward (switching desktop if needed); if it
    /// is already in front, cycles its windows or minimizes it, as the
    /// taskbar does.
    ActivateApp {
        desktop: usize,
        windows: Vec<isize>,
    },
    StepDesktop(i32),
    Media(Control),
    /// Shows or hides the lyrics island.
    ToggleLyrics,
    /// Moves the lyrics earlier (+) or later (−) by steps of a quarter second.
    LyricsOffset(i32),
    ToggleMute,
    VolumeStep(i32),
    BrightnessStep(isize, i32),
    OpenTray,
    OpenNetworkSettings,
    OpenBluetoothSettings,
    OpenSoundSettings,
    OpenPowerSettings,
    OpenDateSettings,
    OpenSettings,
    FocusWindow(isize),
    /// Asks a window to close, as its own close button does.
    CloseWindow(isize),
    CalendarMonth(i32),
    SetWallpaper(std::path::PathBuf),
    SetVariant(String),
    SetMode(bool),
    CopyText(String),
    Quit,
}

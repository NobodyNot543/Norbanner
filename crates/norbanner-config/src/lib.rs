//! The shell's configuration, `%APPDATA%\Norbanner\shell.json`.
//!
//! Every field has a default and missing fields take it, so a file only has
//! to name what it changes:
//!
//! ```json
//! { "border": { "thickness": 14 }, "bar": { "clock": { "showDate": true } } }
//! ```
//!
//! The running shell watches the file and applies changes live; Settings
//! writes it through [`update`].

use std::{
    env,
    fs::{self, File, TryLockError},
    io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not access `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{path}` is not a valid configuration")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not serialize the configuration")]
    Serialize(#[from] serde_json::Error),
}

macro_rules! section {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $field:ident : $ty:ty = $default:expr),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        pub struct $name {
            $($(#[$fmeta])* pub $field: $ty,)*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)* }
            }
        }
    };
}

/// One entry of an ordered, switchable list (bar entries, status icons).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

const fn yes() -> bool {
    true
}

fn entries(list: &[(&str, bool)]) -> Vec<Entry> {
    list.iter()
        .map(|&(id, enabled)| Entry {
            id: id.to_string(),
            enabled,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    System,
    En,
    Es,
    Fr,
    Pt,
    Zh,
    Hi,
    Ar,
    Bn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Dark,
    Light,
    /// Follow Windows' app mode.
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemeSource {
    /// Material You from the current wallpaper.
    #[default]
    Wallpaper,
    /// Windows' accent colour.
    Accent,
    /// `theme.sourceColor`.
    Custom,
}

section!(
    /// Colours.
    ThemeConfig {
        source: SchemeSource = SchemeSource::Wallpaper,
        /// `#rrggbb`, used when `source` is `custom`.
        source_color: String = "#9ccbfb".into(),
        mode: Mode = Mode::Dark,
        /// Material scheme variant: tonalSpot, vibrant, expressive, neutral,
        /// monochrome, fidelity, content, rainbow, fruitSalad.
        variant: String = "tonalSpot".into(),
    }
);

section!(Transparency {
    enabled: bool = false,
    /// Opacity of the frame and drawers.
    base: f32 = 0.85,
    /// Opacity of the cards inside drawers.
    layers: f32 = 0.4,
});

section!(Scale { scale: f32 = 1.0 });

section!(FontFamily {
    sans: String = "Rubik".into(),
    mono: String = "Cascadia Code".into(),
    material: String = "Material Symbols Rounded".into(),
});

section!(FontConfig {
    family: FontFamily = FontFamily::default(),
    size: Scale = Scale::default(),
});

section!(
    /// Global looks.
    Appearance {
        rounding: Scale = Scale::default(),
        spacing: Scale = Scale::default(),
        padding: Scale = Scale::default(),
        font: FontConfig = FontConfig::default(),
        /// Multiplies every animation duration; 0 turns animations off.
        anim_scale: f32 = 1.0,
        transparency: Transparency = Transparency::default(),
    }
);

section!(General {
    language: Language = Language::System,
    /// Hide the Windows taskbar while the shell runs.
    replace_taskbar: bool = true,
    /// Keep maximized windows inside the frame.
    reserve_space: bool = true,
    /// Look for a newer version while Settings is open, and once a day in
    /// the background (the shell shows a bubble when there is one).
    check_updates: bool = true,
    /// Install updates in the background, once a day.
    auto_update: bool = false,
});

section!(
    /// The frame drawn around every screen.
    Border {
        thickness: f32 = 10.0,
        rounding: f32 = 25.0,
        /// How softly drawers melt into the frame.
        smoothing: f32 = 20.0,
    }
);

section!(Workspaces {
    shown: u32 = 3,
    active_indicator: bool = true,
    show_windows: bool = true,
    max_window_icons: u32 = 10,
});

section!(BarClock {
    show_icon: bool = true,
    show_date: bool = false,
    show_seconds: bool = false,
});

section!(ScrollActions {
    workspaces: bool = true,
    volume: bool = true,
    brightness: bool = true,
});

section!(BarPopouts {
    active_window: bool = true,
    tray: bool = true,
    status_icons: bool = true,
});

section!(
    /// The vertical bar on the left edge.
    Bar {
        /// Always shown and reserved; otherwise it slides out on hover.
        persistent: bool = true,
        /// An image file for the logo at the top (PNG with transparency
        /// works best); empty uses Norbanner's.
        logo: String = String::new(),
        show_on_hover: bool = true,
        drag_threshold: f32 = 20.0,
        entries: Vec<Entry> = entries(&[
            ("logo", true),
            ("workspaces", true),
            ("spacer", true),
            ("activeWindow", true),
            ("spacer", true),
            ("tray", true),
            ("clock", true),
            ("statusIcons", true),
            ("power", false),
        ]),
        status_icons: Vec<Entry> = entries(&[
            ("lockStatus", true),
            ("audio", false),
            ("microphone", false),
            ("kbLayout", false),
            ("network", true),
            ("bluetooth", true),
            ("battery", true),
        ]),
        workspaces: Workspaces = Workspaces::default(),
        clock: BarClock = BarClock::default(),
        scroll_actions: ScrollActions = ScrollActions::default(),
        popouts: BarPopouts = BarPopouts::default(),
    }
);

section!(
    /// The drawer at the top: date, calendar, media, resources.
    Dashboard {
        enabled: bool = true,
        show_on_hover: bool = true,
        drag_threshold: f32 = 50.0,
        /// Refresh interval of the resource meters, in milliseconds.
        resource_update_interval: u32 = 1000,
        /// A small island at the top shows the playing song's synced lyrics.
        /// Off by default: it looks the song up on lrclib.net.
        lyrics: bool = false,
        /// How the lyrics move: `sweep` (colour fills word by word), `word`
        /// (the word being sung stands out), `line` (whole lines slide in)
        /// or `fade` (lines fade into each other).
        lyrics_animation: String = "sweep".into(),
        /// Size of the dashboard, 1 = normal.
        scale: f32 = 1.0,
    }
);

section!(
    /// The app launcher at the bottom.
    Launcher {
        enabled: bool = true,
        show_on_hover: bool = false,
        drag_threshold: f32 = 50.0,
        max_shown: u32 = 7,
        action_prefix: String = ">".into(),
        /// `super` (tap the Windows key), `alt+space`, `ctrl+space`, or `none`.
        hotkey: String = "super".into(),
        enable_calculator: bool = true,
    }
);

section!(
    /// Volume and brightness sliders on the right edge.
    Osd {
        enabled: bool = true,
        hide_delay: u32 = 2000,
        enable_brightness: bool = true,
        enable_microphone: bool = false,
    }
);

section!(
    /// Lock, sign out, shut down, hibernate and restart on the right edge.
    Session {
        enabled: bool = true,
        drag_threshold: f32 = 30.0,
        /// `ctrl+alt+delete`-free shortcut for the session menu.
        hotkey: String = "ctrl+alt+end".into(),
    }
);

section!(Services {
    use_twelve_hour_clock: bool = false,
    /// Maximum volume the sliders reach, 1 = 100 %.
    max_volume: f32 = 1.0,
    audio_increment: f32 = 0.05,
    brightness_increment: f32 = 0.1,
    weather_location: String = String::new(),
    use_fahrenheit: bool = false,
});

section!(Paths {
    /// Folder the wallpaper switcher lists; empty means Pictures\Wallpapers.
    wallpaper_dir: String = String::new(),
});

section!(
    /// The whole configuration.
    Config {
        version: u32 = CURRENT_VERSION,
        appearance: Appearance = Appearance::default(),
        theme: ThemeConfig = ThemeConfig::default(),
        general: General = General::default(),
        border: Border = Border::default(),
        bar: Bar = Bar::default(),
        dashboard: Dashboard = Dashboard::default(),
        launcher: Launcher = Launcher::default(),
        osd: Osd = Osd::default(),
        session: Session = Session::default(),
        services: Services = Services::default(),
        paths: Paths = Paths::default(),
    }
);

impl Config {
    /// Clamps values the shell cannot draw sensibly.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let b = &mut self.border;
        b.thickness = b.thickness.clamp(2.0, 60.0);
        b.rounding = b.rounding.clamp(0.0, 80.0);
        b.smoothing = b.smoothing.clamp(0.0, 80.0);
        let a = &mut self.appearance;
        a.anim_scale = a.anim_scale.clamp(0.0, 5.0);
        for scale in [
            &mut a.rounding,
            &mut a.spacing,
            &mut a.padding,
            &mut a.font.size,
        ] {
            scale.scale = scale.scale.clamp(0.25, 4.0);
        }
        a.transparency.base = a.transparency.base.clamp(0.0, 1.0);
        a.transparency.layers = a.transparency.layers.clamp(0.0, 1.0);
        self.bar.workspaces.shown = self.bar.workspaces.shown.clamp(1, 20);
        self.launcher.max_shown = self.launcher.max_shown.clamp(1, 20);
        self.dashboard.scale = self.dashboard.scale.clamp(0.7, 1.5);
        if !matches!(
            self.dashboard.lyrics_animation.as_str(),
            "sweep" | "word" | "line" | "fade"
        ) {
            self.dashboard.lyrics_animation = "sweep".into();
        }
        let s = &mut self.services;
        s.max_volume = s.max_volume.clamp(0.1, 1.0);
        s.audio_increment = s.audio_increment.clamp(0.01, 0.5);
        s.brightness_increment = s.brightness_increment.clamp(0.01, 0.5);
        self.version = CURRENT_VERSION;
        self
    }
}

/// `%APPDATA%\Norbanner\shell.json`, or `NORBANNER_CONFIG` when set (handy for
/// trying settings without touching the real file).
#[must_use]
pub fn default_path() -> PathBuf {
    env::var_os("NORBANNER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir().join("shell.json"))
}

/// `%APPDATA%\Norbanner`.
#[must_use]
pub fn data_dir() -> PathBuf {
    PathBuf::from(env::var_os("APPDATA").unwrap_or_default()).join("Norbanner")
}

/// Reads the configuration; `None` when the file does not exist yet.
pub fn load(path: &Path) -> Result<Option<Config>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    // Tolerate a byte-order mark left by editors such as Notepad.
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(text)
        .map(|config: Config| Some(config.sanitized()))
        .map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
}

/// Reads the configuration, falling back to defaults when it is missing or
/// broken (a broken file is reported, never overwritten).
#[must_use]
pub fn load_or_default(path: &Path) -> (Config, Option<ConfigError>) {
    match load(path) {
        Ok(config) => (config.unwrap_or_default(), None),
        Err(error) => (Config::default(), Some(error)),
    }
}

pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let _lock = lock_for_write(path)?;
    write_config(path, config)
}

/// Reads, edits and writes the configuration under a lock, so Settings and
/// the shell never overwrite each other's changes.
pub fn update(path: &Path, edit: impl FnOnce(Config) -> Config) -> Result<Config, ConfigError> {
    let _lock = lock_for_write(path)?;
    let config = edit(load(path)?.unwrap_or_default()).sanitized();
    write_config(path, &config)?;
    Ok(config)
}

/// Merges a JSON patch (as Settings sends it) into the saved configuration.
/// Objects merge recursively; any other value replaces the saved one.
pub fn patch(path: &Path, patch: serde_json::Value) -> Result<Config, ConfigError> {
    let _lock = lock_for_write(path)?;
    let saved = load(path)?.unwrap_or_default();
    let mut value = serde_json::to_value(saved)?;
    merge(&mut value, patch);
    let config: Config = serde_json::from_value(value).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })?;
    let config = config.sanitized();
    write_config(path, &config)?;
    Ok(config)
}

fn merge(into: &mut serde_json::Value, from: serde_json::Value) {
    match (into, from) {
        (serde_json::Value::Object(into), serde_json::Value::Object(from)) => {
            for (key, value) in from {
                merge(into.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (into, from) => *into = from,
    }
}

fn lock_for_write(path: &Path) -> Result<File, ConfigError> {
    let io_error = |source| ConfigError::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(io_error)?;
    }
    // Lock a separate file so the atomic rename cannot replace the lock.
    // Closing the handle releases it, including when a process exits.
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("json.lock"))
        .map_err(io_error)?;
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(TryLockError::WouldBlock) if start.elapsed() < Duration::from_millis(200) => {
                thread::sleep(Duration::from_millis(4));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io_error(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another Norbanner process is saving the configuration",
                )));
            }
            Err(TryLockError::Error(error)) => return Err(io_error(error)),
        }
    }
}

fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let io_error = |source| ConfigError::Io {
        path: path.to_owned(),
        source,
    };
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(config)?).map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

/// Parses `#rrggbb` / `rrggbb`.
#[must_use]
pub fn parse_hex(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_take_defaults() -> Result<(), serde_json::Error> {
        let config: Config = serde_json::from_str(
            r#"{"border":{"thickness":14},"bar":{"clock":{"showDate":true}}}"#,
        )?;
        assert_eq!(config.border.thickness, 14.0);
        assert_eq!(config.border.rounding, 25.0);
        assert!(config.bar.clock.show_date);
        assert!(config.bar.clock.show_icon);
        assert_eq!(config.bar.entries.len(), 9);
        Ok(())
    }

    #[test]
    fn round_trips() -> Result<(), serde_json::Error> {
        let config = Config::default();
        let back: Config = serde_json::from_str(&serde_json::to_string(&config)?)?;
        assert_eq!(back, config);
        Ok(())
    }

    #[test]
    fn sanitizes_out_of_range_values() {
        let mut config = Config::default();
        config.border.thickness = 500.0;
        config.appearance.anim_scale = -1.0;
        config.dashboard.scale = 9.0;
        config.dashboard.lyrics_animation = "spin".into();
        let config = config.sanitized();
        assert_eq!(config.border.thickness, 60.0);
        assert_eq!(config.appearance.anim_scale, 0.0);
        assert_eq!(config.dashboard.scale, 1.5);
        assert_eq!(config.dashboard.lyrics_animation, "sweep");
    }

    #[test]
    fn patch_merges_nested_objects() -> Result<(), ConfigError> {
        let dir = env::temp_dir().join(format!("norbanner-config-test-{}", std::process::id()));
        let path = dir.join("shell.json");
        let _ = fs::remove_file(&path);
        save(&path, &Config::default())?;
        let config = patch(
            &path,
            serde_json::json!({"bar": {"clock": {"showSeconds": true}}}),
        )?;
        assert!(config.bar.clock.show_seconds);
        assert!(config.bar.clock.show_icon);
        assert!(config.bar.persistent);
        let _ = fs::remove_dir_all(dir);
        Ok(())
    }

    #[test]
    fn parses_hex() {
        assert_eq!(parse_hex("#9ccbfb"), Some(0x9ccbfb));
        assert_eq!(parse_hex("zz"), None);
    }
}

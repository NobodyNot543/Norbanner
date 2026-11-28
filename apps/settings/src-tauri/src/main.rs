//! Norbanner settings: a separate process that exists only while its window is
//! open. It edits `shell.json` and tells the running shell to reload it.
//!
//! With `--update` it opens no window: the shell starts it that way once a
//! day, while the user is away, to look for a newer version. It installs it
//! (automatic updates) or tells the shell, which shows a bubble. It exits as
//! soon as it is done.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod wallpapers;

use std::{env, io, path::PathBuf, process::Command, sync::Mutex, time::Duration};

use norbanner_config::Config;
use norbanner_shell::{autostart, launcher, shell_link};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The shell is installed next to this app.
fn shell_exe() -> io::Result<PathBuf> {
    Ok(env::current_exe()?.with_file_name("norbanner.exe"))
}

#[tauri::command]
fn get_config() -> Config {
    norbanner_config::load(&norbanner_config::default_path())
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// The defaults live in `norbanner-config`; the window asks for them to reset.
#[tauri::command]
fn default_config() -> Config {
    Config::default()
}

/// Merges `patch` (nested objects merge) into `shell.json`.
#[tauri::command]
async fn set_config(patch: serde_json::Value) -> Result<Config, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = norbanner_config::patch(&norbanner_config::default_path(), patch)
            .map_err(|error| error.to_string())?;
        shell_link::notify_config_changed();
        Ok(config)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Read-only animation preference for the Settings view. Windows remains
/// the source of truth; the shell owns status-item preference notifications.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowsPreferences {
    animations: bool,
}

#[tauri::command]
fn windows_preferences() -> WindowsPreferences {
    WindowsPreferences {
        animations: norbanner_shell::system::animations_enabled(),
    }
}

/// The page of Windows Settings with that switch: "Animation effects" on
/// Windows 11, "Show animations in Windows" on Windows 10.
#[tauri::command]
fn open_animation_settings() -> Result<(), String> {
    let page = if norbanner_shell::system::is_windows_11() {
        "ms-settings:easeofaccess-visualeffects"
    } else {
        "ms-settings:easeofaccess-display"
    };
    launcher::open(page).map_err(|error| error.to_string())
}

/// The Material You colours the shell would use with `theme`, as
/// `#rrggbb` per role, so the window can preview (and wear) them.
#[tauri::command]
async fn scheme_preview(
    theme: norbanner_config::ThemeConfig,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use norbanner_config::{Mode, SchemeSource};
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
        // SAFETY: COM for this blocking worker thread only.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let source = match theme.source {
            SchemeSource::Wallpaper => norbanner_shell::wallpaper::current()
                .and_then(|path| norbanner_shell::wallpaper::pixels(&path))
                .and_then(|pixels| norbanner_theme::source_from_pixels(&pixels)),
            SchemeSource::Accent => norbanner_shell::system::accent_color()
                .map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)),
            SchemeSource::Custom => norbanner_config::parse_hex(&theme.source_color),
        }
        .unwrap_or(norbanner_theme::DEFAULT_SOURCE);
        let dark = match theme.mode {
            Mode::Dark => true,
            Mode::Light => false,
            Mode::System => !norbanner_shell::system::uses_light_theme(),
        };
        let variant = norbanner_theme::Variant::ALL
            .into_iter()
            .find(|v| v.id() == theme.variant)
            .unwrap_or_default();
        let p = norbanner_theme::Theme::from_source(source, dark, variant).palette;
        let roles = [
            ("source", norbanner_theme::Color::from_u32(source)),
            ("primary", p.primary),
            ("onPrimary", p.on_primary),
            ("primaryContainer", p.primary_container),
            ("onPrimaryContainer", p.on_primary_container),
            ("secondary", p.secondary),
            ("secondaryContainer", p.secondary_container),
            ("onSecondaryContainer", p.on_secondary_container),
            ("tertiary", p.tertiary),
            ("tertiaryContainer", p.tertiary_container),
            ("error", p.error),
            ("surface", p.surface),
            ("surfaceContainerLow", p.surface_container_low),
            ("surfaceContainer", p.surface_container),
            ("surfaceContainerHigh", p.surface_container_high),
            ("surfaceContainerHighest", p.surface_container_highest),
            ("onSurface", p.on_surface),
            ("onSurfaceVariant", p.on_surface_variant),
            ("outline", p.outline),
            ("outlineVariant", p.outline_variant),
        ];
        Ok(roles
            .into_iter()
            .map(|(name, colour)| (name.to_string(), format!("#{}", colour.hex())))
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn get_autostart() -> Result<bool, String> {
    let shell = shell_exe().map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(
            autostart::enabled_for(&shell.to_string_lossy()).unwrap_or_else(|error| {
                norbanner_shell::log::line(&format!(
                    "no se pudo consultar el inicio automático: {error}"
                ));
                // The cached choice still lets Settings open if the scheduler
                // service is unavailable. An explicit change reports its error.
                autostart::is_enabled()
            }),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn set_autostart(enabled: bool) -> Result<(), String> {
    let shell = shell_exe().map_err(|error| error.to_string())?;
    let target = enabled.then(|| shell.to_string_lossy().into_owned());
    tauri::async_runtime::spawn_blocking(move || {
        autostart::set(target.as_deref()).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The update found by the last check, kept until the user installs it.
#[derive(Default)]
struct PendingUpdate(Mutex<Option<Update>>);

/// Asks GitHub Releases for a newer version; returns it, if any. Only runs
/// while Settings is open: the shell itself never touches the network.
#[tauri::command]
async fn check_update(
    app: tauri::AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let info = update.as_ref().map(|update| UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone().filter(|notes| is_release_page(notes)),
    });
    *pending.0.lock().map_err(|error| error.to_string())? = update;
    Ok(info)
}

/// What the window shows about an available update.
#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    /// The release page, when the manifest points at one.
    notes: Option<String>,
}

/// Download progress, sent to the window as `update-progress`.
#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

const RELEASES: &str = "https://github.com/NobodyNot543/Norbanner/releases/";

/// The update manifest is not signed (only the installer is), so the only
/// link it may open is a release page of this repository.
fn is_release_page(url: &str) -> bool {
    url.starts_with(RELEASES) && !url.contains(char::is_whitespace)
}

#[tauri::command]
fn open_release_notes(url: String) -> Result<(), String> {
    if !is_release_page(&url) {
        return Err("enlace no permitido".into());
    }
    launcher::open(&url).map_err(|error| error.to_string())
}

/// A fixed destination: this command cannot launch arbitrary frontend input.
#[tauri::command]
async fn open_repository() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        launcher::open("https://github.com/NobodyNot543/Norbanner")
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// A small data URI, never a path or screenshot of the user's desktop.
#[tauri::command]
async fn wallpaper_preview(window: tauri::WebviewWindow) -> Option<String> {
    let hwnd = window.hwnd().ok()?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let image = norbanner_shell::imagery::wallpaper_preview(hwnd)?;
        let mut rgba = image.bgra;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            // Wallpaper decodes are opaque in normal use. Unpremultiply when
            // a PNG wallpaper does carry alpha, before encoding straight RGBA.
            let alpha = u32::from(pixel[3]);
            pixel.swap(0, 2);
            for value in &mut pixel[..3] {
                *value = (u32::from(*value) * 255)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
        }
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .ok()
    .flatten()
}

/// Asks for an image with Windows' own open dialog; the chosen file's path,
/// or `None` when cancelled. `filter` names the image types in the dialog.
#[tauri::command]
async fn pick_logo(window: tauri::WebviewWindow, filter: String) -> Option<String> {
    let owner = window.hwnd().ok()?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || pick_image(owner, &filter))
        .await
        .ok()
        .flatten()
}

fn pick_image(owner: isize, filter: &str) -> Option<String> {
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                Common::COMDLG_FILTERSPEC, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
            },
        },
        core::{HSTRING, PCWSTR, w},
    };
    let name = HSTRING::from(filter);
    // SAFETY: COM on this worker thread only, balanced by CoUninitialize; the
    // strings outlive the dialog, and the returned path is freed once read.
    unsafe {
        let initialized =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let picked = (|| {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let types = [COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: w!("*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.ico"),
            }];
            dialog.SetFileTypes(&types).ok()?;
            dialog.Show(Some(HWND(owner as *mut _))).ok()?;
            let path = dialog
                .GetResult()
                .ok()?
                .GetDisplayName(SIGDN_FILESYSPATH)
                .ok()?;
            let text = path.to_string().ok();
            CoTaskMemFree(Some(path.0 as *const _));
            text
        })();
        if initialized {
            CoUninitialize();
        }
        picked
    }
}

/// The image at `path` as a data URI for the logo preview; `None` when it
/// cannot be read or is not a small image.
#[tauri::command]
async fn logo_preview(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let path = std::path::Path::new(&path);
        let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            "ico" => "image/x-icon",
            _ => return None,
        };
        if std::fs::metadata(path).ok()?.len() > 8 * 1024 * 1024 {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .ok()
    .flatten()
}

/// Downloads the signed installer and runs it; the installer stops the shell,
/// replaces both apps and starts the shell again. This process exits.
#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    // Cloned, not taken: if the download fails the user can try again.
    let update = pending
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or("no hay ninguna actualización pendiente")?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())
}

/// The argument the shell starts this app with for a background update.
const UPDATE_ARG: &str = "--update";

/// Checks for a newer version. With automatic updates on, installs it
/// without asking; the installer stops and restarts the shell. Otherwise
/// tells the shell, whose bubble offers to install it here. Exits when
/// there is nothing to do or anything fails: the next try is tomorrow.
async fn update_in_background(app: AppHandle) {
    let config = get_config();
    let found = match app.updater_builder().timeout(Duration::from_secs(60)) {
        // The installer starts the shell again; this app need not come back.
        builder if config.general.check_updates => {
            match builder.restart_after_install(false).build() {
                Ok(updater) => updater.check().await.ok().flatten(),
                Err(_) => None,
            }
        }
        _ => None,
    };
    // Opening Settings meanwhile lands in this process (single instance):
    // then it stays as the user's Settings, which shows the update itself,
    // and installing must not close it under them.
    let window_opened = || app.get_webview_window("main").is_some();
    match found.filter(|_| !window_opened()) {
        Some(update) if config.general.auto_update => {
            if let Err(error) = update.download_and_install(|_, _| {}, || {}).await {
                eprintln!("norbanner-settings: actualización: {error}");
            }
        }
        Some(update) => {
            let _ = shell_link::send(&format!("update available {}", update.version));
        }
        None => {}
    }
    if !window_opened() {
        app.exit(0);
    }
}

/// Opens the Settings window, or brings it forward if it is open.
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.first() else {
        return;
    };
    if let Err(error) =
        WebviewWindowBuilder::from_config(app, config).and_then(|window| window.build())
    {
        eprintln!("norbanner-settings: {error}");
    }
}

fn main() {
    let background = env::args().any(|arg| arg == UPDATE_ARG);
    // The Start menu entry opens this app: it also brings back a shell the user quit.
    if !background
        && !shell_link::is_running()
        && let Err(error) = shell_exe().and_then(|shell| Command::new(shell).spawn())
    {
        eprintln!("norbanner-settings: could not start the shell: {error}");
    }
    let result = tauri::Builder::default()
        // First, so a second launch exits before doing anything else.
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            // The open window checks for updates itself; a background update
            // started meanwhile has nothing to do.
            if !args.iter().any(|arg| arg == UPDATE_ARG) {
                show_window(app);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate::default())
        .setup(move |app| {
            if background {
                tauri::async_runtime::spawn(update_in_background(app.handle().clone()));
            } else {
                show_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            default_config,
            set_config,
            get_autostart,
            set_autostart,
            windows_preferences,
            open_animation_settings,
            scheme_preview,
            check_update,
            install_update,
            open_release_notes,
            open_repository,
            wallpaper_preview,
            pick_logo,
            logo_preview,
            wallpapers::wallpaper_selection,
            wallpapers::apply_wallpaper,
            wallpapers::apply_wallpaper_file
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("norbanner-settings: {error}");
    }
}

//! The desktop wallpaper: which one is set, its colours (the shell's
//! Material You source) and changing it from the launcher's switcher.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use windows::{
    Win32::{
        System::Com::{CLSCTX_ALL, CoCreateInstance},
        UI::{
            Shell::{DesktopWallpaper, IDesktopWallpaper},
            WindowsAndMessaging::{
                SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::icons::IconImage;

const MAX_FILE: u64 = 64 * 1024 * 1024;
const IMAGE_EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "bmp", "webp", "jxr", "gif"];

/// The wallpaper file of the primary desktop, if it is an image file.
#[must_use]
pub fn current() -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    // SAFETY: The buffer is valid for its length in characters.
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buffer.len() as u32,
            Some(buffer.as_mut_ptr().cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..len]));
    path.is_file().then_some(path)
}

fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The image scaled to fit `side`×`side`. Requires COM on the thread.
#[must_use]
pub fn thumbnail(path: &Path, side: u32) -> Option<IconImage> {
    crate::imagery::decode(&read_limited(path)?, side, 200_000_000)
}

/// The image's pixels as 0xRRGGBB, scaled to fit 128×128, for picking the
/// Material You source colour. Requires COM on the thread.
#[must_use]
pub fn pixels(path: &Path) -> Option<Vec<u32>> {
    let image = thumbnail(path, 128)?;
    Some(
        image
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 0)
            .map(|px| {
                // Undo premultiplication for partly transparent images.
                let a = u32::from(px[3]);
                let un = |c: u8| (u32::from(c) * 255 / a).min(255);
                (un(px[2]) << 16) | (un(px[1]) << 8) | un(px[0])
            })
            .collect(),
    )
}

/// Sets `path` as the wallpaper of every monitor. Requires COM.
pub fn set(path: &Path) -> windows::core::Result<()> {
    // SAFETY: Documented shell object; the path outlives the call.
    unsafe {
        let desktop: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?;
        desktop.SetWallpaper(PCWSTR::null(), &HSTRING::from(path.as_os_str()))
    }
}

/// The folder the switcher lists: `configured`, or `Pictures\Wallpapers`,
/// or `Pictures`.
#[must_use]
pub fn folder(configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        let path = PathBuf::from(configured.trim());
        return path.is_dir().then_some(path);
    }
    let pictures = PathBuf::from(std::env::var_os("USERPROFILE")?).join("Pictures");
    let wallpapers = pictures.join("Wallpapers");
    if wallpapers.is_dir() {
        Some(wallpapers)
    } else {
        pictures.is_dir().then_some(pictures)
    }
}

/// Image files directly inside `folder`, sorted by name.
#[must_use]
pub fn list(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut images: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    images.sort();
    images
}

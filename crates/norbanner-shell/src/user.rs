//! Who is signed in, for the dashboard's user card.

use std::path::PathBuf;

use windows::{Win32::System::WindowsProgramming::GetUserNameW, core::PWSTR};

/// The account's display name, falling back to its user name.
#[must_use]
pub fn display_name() -> String {
    use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut buffer = [0u16; 256];
    let mut len = buffer.len() as u32;
    // SAFETY: The buffer and its length are valid.
    if unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buffer.as_mut_ptr())), &mut len) } && len > 0
    {
        return String::from_utf16_lossy(&buffer[..len as usize]);
    }
    user_name()
}

/// The account's user name (`DOMAIN\` stripped).
#[must_use]
pub fn user_name() -> String {
    let mut buffer = [0u16; 257];
    let mut len = buffer.len() as u32;
    // SAFETY: The buffer and its length are valid.
    if unsafe { GetUserNameW(Some(PWSTR(buffer.as_mut_ptr())), &mut len) }.is_ok() && len > 0 {
        // `len` includes the terminating null.
        return String::from_utf16_lossy(&buffer[..(len as usize).saturating_sub(1)]);
    }
    std::env::var("USERNAME").unwrap_or_default()
}

/// The account picture Windows shows on the sign-in screen, if one was set:
/// a `~/.face` image as on Linux, else the picture Windows registered for
/// this account (`C:\Users\Public\AccountPictures\<SID>`), else the largest
/// image in `%APPDATA%\Microsoft\Windows\AccountPictures`.
#[must_use]
pub fn picture() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("USERPROFILE")?);
    for face in [".face", ".face.png", ".face.jpg"] {
        let path = home.join(face);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(sid) = crate::autostart::user_sid() {
        if let Some(path) = registered_picture(&sid) {
            return Some(path);
        }
        if let Some(public) = std::env::var_os("PUBLIC")
            && let Some(path) =
                largest_image(&PathBuf::from(public).join("AccountPictures").join(&sid))
        {
            return Some(path);
        }
    }
    let folder = PathBuf::from(std::env::var_os("APPDATA")?)
        .join("Microsoft")
        .join("Windows")
        .join("AccountPictures");
    largest_image(&folder)
}

/// The account picture recorded under
/// `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\AccountPicture\Users\<SID>`,
/// a mid-sized one first.
fn registered_picture(sid: &str) -> Option<PathBuf> {
    use windows::{
        Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
        core::HSTRING,
    };
    let key = HSTRING::from(format!(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\AccountPicture\Users\{sid}"
    ));
    for value in ["Image448", "Image424", "Image240", "Image1080", "Image192"] {
        let mut buffer = [0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: The buffer and its size in bytes are valid.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &HSTRING::from(value),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            continue;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let path = PathBuf::from(String::from_utf16_lossy(&buffer[..len]));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// The largest PNG/JPEG/BMP in `folder`.
fn largest_image(folder: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "bmp"
                    )
                })
        })
        .max_by_key(|entry| entry.metadata().map(|m| m.len()).unwrap_or(0))
        .map(|entry| entry.path())
}

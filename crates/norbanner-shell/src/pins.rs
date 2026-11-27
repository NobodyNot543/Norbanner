//! The apps pinned to the Windows taskbar, so the launcher can show the
//! same ones first.
//!
//! Desktop pins are shortcuts in `%APPDATA%\Microsoft\Internet Explorer\
//! Quick Launch\User Pinned\TaskBar`; Store app pins only appear, by their
//! AppUserModelID, inside the `Favorites` value of the `Taskband` key.

use std::path::{Path, PathBuf};

use windows::{
    Win32::{
        Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath},
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegGetValueW},
        UI::Shell::PropertiesSystem::{
            GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName,
        },
    },
    core::{HSTRING, w},
};

use crate::{apps::App, tracker::APPS_FOLDER};

/// An app pinned to the taskbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    /// Its id among the apps (as in [`App::id`]): opens it and finds its
    /// icon and name.
    pub id: String,
    /// Lowercase ids its windows may have (see `tracker`): the AppsFolder
    /// id, the shortcut's AppUserModelID, the program it starts.
    keys: Vec<String>,
}

impl Pin {
    /// Whether a window's app id (from the tracker) is this pin's app.
    #[must_use]
    pub fn owns(&self, window_app: &str) -> bool {
        let id = window_app.to_lowercase();
        self.keys.contains(&id)
    }
}

/// The taskbar's pinned apps found among `apps`, desktop shortcuts first,
/// then Store apps. Requires COM.
#[must_use]
pub fn taskbar(apps: &[App]) -> Vec<Pin> {
    let mut pins: Vec<Pin> = Vec::new();
    let mut push = |id: Option<String>, mut extra: Vec<String>| {
        let Some(id) = id else { return };
        if pins.iter().any(|p| p.id == id) {
            return;
        }
        extra.push(id.to_lowercase());
        extra.retain(|k| !k.is_empty());
        pins.push(Pin { id, keys: extra });
    };
    for shortcut in shortcuts() {
        let name = shortcut
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let aumid = shortcut_property(&shortcut, &PKEY_AppUserModel_ID);
        let target = shortcut_property(&shortcut, &PKEY_Link_TargetParsingPath);
        let found = aumid
            .as_deref()
            .and_then(|aumid| by_aumid(apps, aumid))
            .or_else(|| by_name(apps, &name));
        let mut keys = Vec::new();
        // File Explorer's shortcut points at a shell item, not a program;
        // its windows belong to explorer.exe.
        if aumid
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case("Microsoft.Windows.Explorer"))
            && let Some(windir) = std::env::var_os("WINDIR")
        {
            keys.push(
                PathBuf::from(windir)
                    .join("explorer.exe")
                    .to_string_lossy()
                    .to_lowercase(),
            );
        }
        if let Some(aumid) = aumid {
            keys.push(format!("{APPS_FOLDER}{aumid}").to_lowercase());
        }
        if let Some(target) = target {
            keys.push(target.to_lowercase());
        }
        push(found, keys);
    }
    for aumid in store_pins() {
        let key = format!("{APPS_FOLDER}{aumid}").to_lowercase();
        push(by_aumid(apps, &aumid), vec![key]);
    }
    pins
}

fn by_aumid(apps: &[App], aumid: &str) -> Option<String> {
    apps.iter()
        .find(|app| {
            app.id
                .strip_prefix(APPS_FOLDER)
                .is_some_and(|id| id.eq_ignore_ascii_case(aumid))
        })
        .map(|app| app.id.clone())
}

fn by_name(apps: &[App], name: &str) -> Option<String> {
    apps.iter()
        .find(|app| app.name.eq_ignore_ascii_case(name))
        .map(|app| app.id.clone())
}

/// The pinned shortcuts, by name.
fn shortcuts() -> Vec<PathBuf> {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return Vec::new();
    };
    let folder = PathBuf::from(appdata)
        .join("Microsoft")
        .join("Internet Explorer")
        .join("Quick Launch")
        .join("User Pinned")
        .join("TaskBar");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut list: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .collect();
    list.sort();
    list
}

/// A text property of a shortcut (its AppUserModelID, its target).
fn shortcut_property(path: &Path, key: &windows::Win32::Foundation::PROPERTYKEY) -> Option<String> {
    // SAFETY: COM call with a path string alive for its duration.
    let value = unsafe {
        let store: IPropertyStore =
            SHGetPropertyStoreFromParsingName(&HSTRING::from(path.as_os_str()), None, GPS_DEFAULT)
                .ok()?;
        store.GetValue(key).ok()?
    };
    let text = value.to_string();
    (!text.is_empty()).then_some(text)
}

/// Store apps' AppUserModelIDs in the taskbar's `Favorites` value.
fn store_pins() -> Vec<String> {
    let mut size = 0u32;
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband");
    // SAFETY: A size query, then a read into a buffer of that size.
    unsafe {
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
        .is_err()
        {
            return Vec::new();
        }
        let mut bytes = vec![0u8; size as usize];
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            Some(bytes.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .is_err()
        {
            return Vec::new();
        }
        bytes.truncate(size as usize);
        aumids_in(&bytes)
    }
}

/// Finds `Name_publisherid!App` ids among the UTF-16 strings of a blob.
fn aumids_in(bytes: &[u8]) -> Vec<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let mut found: Vec<String> = Vec::new();
    for token in text.split(|c: char| !(c.is_ascii_alphanumeric() || "._-!".contains(c))) {
        let Some((family, app)) = token.split_once('!') else {
            continue;
        };
        let publisher = family.rsplit_once('_').map(|(_, p)| p);
        let looks_right = !app.is_empty()
            && publisher.is_some_and(|p| {
                p.len() == 13
                    && p.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            });
        if looks_right && !found.iter().any(|f| f == token) {
            found.push(token.to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn finds_store_ids_in_a_blob() {
        let mut blob = vec![0u8, 1, 2, 3];
        blob.extend(utf16(
            "\0\u{1}Microsoft.WindowsStore_8wekyb3d8bbwe!App\0junk!x\0",
        ));
        blob.extend(utf16(
            "C:\\path\\file.lnk\0Microsoft.WindowsStore_8wekyb3d8bbwe!App",
        ));
        assert_eq!(
            aumids_in(&blob),
            ["Microsoft.WindowsStore_8wekyb3d8bbwe!App"]
        );
    }

    #[test]
    fn matches_by_id_then_name() {
        let app = |id: &str, name: &str| App {
            id: format!("{APPS_FOLDER}{id}"),
            name: name.into(),
            search: name.to_lowercase(),
        };
        let apps = [
            app("Microsoft.Windows.Explorer", "Explorador de archivos"),
            app("Brave", "Brave"),
        ];
        assert_eq!(
            by_aumid(&apps, "microsoft.windows.explorer").as_deref(),
            Some(format!("{APPS_FOLDER}Microsoft.Windows.Explorer").as_str())
        );
        assert_eq!(
            by_name(&apps, "brave").as_deref(),
            Some(format!("{APPS_FOLDER}Brave").as_str())
        );
        assert_eq!(by_name(&apps, "Edge"), None);
    }

    #[test]
    fn a_pin_owns_its_windows_by_any_key() {
        let pin = Pin {
            id: format!("{APPS_FOLDER}Brave"),
            keys: vec![
                format!("{APPS_FOLDER}brave").to_lowercase(),
                r"c:\program files\brave\brave.exe".into(),
            ],
        };
        assert!(pin.owns(r"C:\Program Files\Brave\brave.exe"));
        assert!(pin.owns(&format!("{APPS_FOLDER}Brave")));
        assert!(!pin.owns(r"c:\windows\explorer.exe"));
    }
}

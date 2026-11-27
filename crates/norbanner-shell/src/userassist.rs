//! How much each app has been used, from Windows' own record (UserAssist,
//! what Start uses for "Most used"), so the launcher can put the apps the
//! user really opens first.
//!
//! `HKCU\...\Explorer\UserAssist\{GUID}\Count` holds a value per program or
//! shortcut: the name is ROT13 of its AppUserModelID or path, the data
//! (72 bytes on Windows 7 and later) has the run count at byte 4 and the
//! time in focus, in milliseconds, at byte 12.

use std::collections::HashMap;

use windows::{
    Win32::{
        Foundation::ERROR_NO_MORE_ITEMS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumValueW, RegOpenKeyExW,
        },
    },
    core::PWSTR,
};

use crate::{apps::App, tracker::APPS_FOLDER};

/// Programs (by AppUserModelID or path) and shortcuts.
const KEYS: [&str; 2] = [
    "{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}",
    "{F4E57C4B-2036-45F0-A9AB-443BCFE33D9F}",
];

/// A score per app id (as in [`App::id`]): runs plus minutes in focus.
/// Apps Windows has no record of are left out.
#[must_use]
pub fn scores(apps: &[App]) -> HashMap<String, u32> {
    let entries = entries();
    let mut by_id: HashMap<String, u32> = HashMap::new();
    let mut by_name: HashMap<String, u32> = HashMap::new();
    for (name, score) in entries {
        let lower = name.to_lowercase();
        if let Some(stem) = lower.strip_suffix(".lnk") {
            let stem = stem.rsplit('\\').next().unwrap_or(stem).to_string();
            *by_name.entry(stem).or_insert(0) += score;
        } else {
            *by_id.entry(lower).or_insert(0) += score;
        }
    }
    let mut out = HashMap::new();
    for app in apps {
        let id = app
            .id
            .strip_prefix(APPS_FOLDER)
            .unwrap_or(&app.id)
            .to_lowercase();
        let score = by_id.get(&id).copied().unwrap_or(0)
            + by_name.get(&app.name.to_lowercase()).copied().unwrap_or(0);
        if score > 0 {
            out.insert(app.id.clone(), score);
        }
    }
    out
}

/// Every decoded entry with its score.
fn entries() -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for guid in KEYS {
        let path =
            format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist\{guid}\Count");
        let mut key = HKEY::default();
        // SAFETY: Opens a key we close below; the path outlives the call.
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &windows::core::HSTRING::from(path),
                Some(0),
                KEY_READ,
                &mut key,
            )
        };
        if opened.is_err() {
            continue;
        }
        let mut index = 0u32;
        loop {
            let mut name = [0u16; 1024];
            let mut name_len = name.len() as u32;
            let mut data = [0u8; 128];
            let mut data_len = data.len() as u32;
            let mut kind = 0u32;
            // SAFETY: Buffers and their lengths are valid for the call.
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                )
            };
            index += 1;
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status.is_err() {
                continue;
            }
            let name = rot13(&String::from_utf16_lossy(&name[..name_len as usize]));
            if let Some(score) = score(&data[..data_len as usize]) {
                out.push((name, score));
            }
        }
        // SAFETY: Closes the key opened above.
        unsafe {
            let _ = RegCloseKey(key);
        }
    }
    out
}

/// Runs plus minutes in focus, from a 72-byte record.
fn score(data: &[u8]) -> Option<u32> {
    if data.len() < 16 {
        return None;
    }
    let read = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let runs = read(4);
    let focus_minutes = read(12) / 60_000;
    let score = runs.saturating_add(focus_minutes);
    (score > 0).then_some(score)
}

fn rot13(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_names() {
        assert_eq!(
            rot13("Zvpebfbsg.Jvaqbjf.Rkcybere"),
            "Microsoft.Windows.Explorer"
        );
        assert_eq!(rot13(rot13("C:\\a_b!1").as_str()), "C:\\a_b!1");
    }

    #[test]
    fn scores_runs_and_focus() {
        let mut data = [0u8; 72];
        data[4..8].copy_from_slice(&3u32.to_le_bytes());
        data[12..16].copy_from_slice(&(5 * 60_000u32).to_le_bytes());
        assert_eq!(score(&data), Some(8));
        assert_eq!(score(&[0u8; 72]), None);
        assert_eq!(score(&[1u8; 8]), None);
    }
}

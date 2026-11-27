//! Every app the Start menu lists, for the launcher.
//!
//! `shell:AppsFolder` is the virtual folder behind Start's "All apps": it
//! merges Start menu shortcuts and Store/packaged apps and already hides
//! uninstallers and duplicates. Each item opens with
//! `shell:AppsFolder\<id>`, which [`crate::launcher::open`] understands.

use windows::{
    Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{
            BHID_EnumItems, IEnumShellItems, IShellItem, SHCreateItemFromParsingName,
            SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
        },
    },
    core::{PWSTR, w},
};

use crate::tracker::APPS_FOLDER;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// Shell id: `shell:AppsFolder\<AppUserModelID or path>`.
    pub id: String,
    pub name: String,
    /// `name` lowercased without accents, for searching.
    pub search: String,
}

unsafe fn take(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: Guaranteed by the caller: a CoTaskMem string from the shell.
    unsafe {
        let value = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const core::ffi::c_void));
        value.filter(|v| !v.is_empty())
    }
}

/// Lists the apps, sorted by name. Takes tens of milliseconds: call it off
/// the UI thread or once at startup. Requires COM on the calling thread.
#[must_use]
pub fn list() -> Vec<App> {
    let mut apps = Vec::new();
    // SAFETY: Shell enumeration through documented interfaces; every string
    // is copied and freed.
    unsafe {
        let Ok(folder) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(w!("shell:AppsFolder"), None)
        else {
            return apps;
        };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else {
            return apps;
        };
        loop {
            let mut item = [None];
            let mut fetched = 0;
            if items.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = item[0].take() else { break };
            let Some(name) = item
                .GetDisplayName(SIGDN_NORMALDISPLAY)
                .ok()
                .and_then(|t| take(t))
            else {
                continue;
            };
            let Some(parsing) = item
                .GetDisplayName(SIGDN_PARENTRELATIVEPARSING)
                .ok()
                .and_then(|t| take(t))
            else {
                continue;
            };
            apps.push(App {
                id: format!("{APPS_FOLDER}{parsing}"),
                search: norbanner_fold(&name),
                name,
            });
        }
    }
    apps.sort_by(|a, b| a.search.cmp(&b.search));
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

/// Same folding as the launcher's search (`norbanner_core::fuzzy::fold`),
/// duplicated here so this crate does not depend on the core crate.
fn norbanner_fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

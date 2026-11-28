//! Apps running in the background with an icon in the notification area.
//!
//! Windows does not let another program host or press those icons. What it
//! does keep, under `HKCU\Control Panel\NotifyIconSettings`, is one entry
//! per app that has shown a tray icon: its program and a picture of the
//! icon. Those whose program is running now are the ones in the tray.
//!
//! Icons are decoded with WIC: call [`running`] on a thread with COM.

use windows::Win32::Foundation::{HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS},
        System::{
            ProcessStatus::K32EnumProcesses,
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey,
                RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW,
                TerminateProcess, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    },
    core::{BOOL, HSTRING, PCWSTR, PWSTR, w},
};

/// How long an app gets to close itself before it is ended.
const GRACE_MS: u32 = 3000;

/// Closes the program at `path`: asks each of its windows (hidden ones
/// too, where tray apps keep theirs) to close, then ends the processes
/// still running after a grace period. Tray apps often only hide on
/// close, so asking is not always enough. Blocks: call it off the UI
/// thread.
pub fn quit(path: &str) {
    let pids: Vec<u32> = running_programs()
        .into_iter()
        .filter(|(_, running)| running.eq_ignore_ascii_case(path))
        .map(|(pid, _)| pid)
        .collect();
    if pids.is_empty() {
        return;
    }
    unsafe extern "system" fn ask(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the address of `pids` for this enumeration.
        let pids = unsafe { &*(data.0 as *const Vec<u32>) };
        let mut pid = 0u32;
        // SAFETY: Plain window queries; the message is posted by value.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
            if pids.contains(&pid) {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        BOOL(1)
    }
    // SAFETY: `pids` outlives the enumeration, which is synchronous; each
    // process handle is closed after use.
    unsafe {
        let _ = EnumWindows(Some(ask), LPARAM(&raw const pids as isize));
        for pid in pids {
            let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid)
            else {
                continue;
            };
            if WaitForSingleObject(process, GRACE_MS) != WAIT_OBJECT_0 {
                let _ = TerminateProcess(process, 0);
            }
            let _ = CloseHandle(process);
        }
    }
}

use crate::{icons::IconImage, imagery};

/// An app with a tray icon whose program is running.
pub struct TrayApp {
    /// The running program's full path (opens it again).
    pub path: String,
    /// Its tooltip's first line, or the program's name.
    pub name: String,
    pub icon: Option<IconImage>,
}

/// The tray's apps, in the order Windows lists them.
#[must_use]
pub fn running() -> Vec<TrayApp> {
    let processes = running_programs();
    let mut seen = std::collections::HashSet::new();
    let mut apps = Vec::new();
    for entry in entries() {
        // Explorer draws the system's own icons (volume, network…).
        if leaf(&entry.program) == "explorer.exe" {
            continue;
        }
        let Some((_, path)) = processes
            .iter()
            .find(|(_, path)| same_program(&entry.program, path))
        else {
            continue;
        };
        if !seen.insert(path.to_lowercase()) {
            continue;
        }
        let name = entry
            .tooltip
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map_or_else(|| stem(path), str::to_string);
        apps.push(TrayApp {
            path: path.clone(),
            name,
            icon: entry
                .icon
                .and_then(|png| imagery::decode(&png, 64, 1_000_000)),
        });
    }
    apps
}

/// Whether the registry's `program` is the running `path`. The registry
/// writes known folders as GUIDs (`{…}\Steam\steam.exe`), so what follows
/// the GUID must end the path; a plain path must be the whole path.
fn same_program(program: &str, path: &str) -> bool {
    let program = program.trim().to_lowercase().replace('/', "\\");
    let path = path.to_lowercase();
    match program
        .strip_prefix('{')
        .and_then(|rest| rest.split_once('}'))
    {
        Some((_, rest)) if rest.len() > 1 && rest.starts_with('\\') => path.ends_with(rest),
        _ => !program.is_empty() && path == program,
    }
}

/// The file name of a path, lower case (`C:\A\Steam.exe` → `steam.exe`).
fn leaf(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// The file name without its extension, as written.
fn stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or_default();
    file.rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_string()
}

struct Entry {
    program: String,
    tooltip: String,
    icon: Option<Vec<u8>>,
}

const SETTINGS: PCWSTR = w!(r"Control Panel\NotifyIconSettings");

fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    // SAFETY: Registry reads into buffers of the sizes passed; the key is
    // closed before returning.
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, SETTINGS, None, KEY_READ, &raw mut key) != ERROR_SUCCESS
        {
            return out;
        }
        for index in 0..512 {
            let mut name = [0u16; 128];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &raw mut len,
                None,
                None,
                None,
                None,
            ) != ERROR_SUCCESS
            {
                break;
            }
            let sub = HSTRING::from_wide(&name[..len as usize]);
            let Some(program) = string(key, &sub, w!("ExecutablePath")) else {
                continue;
            };
            out.push(Entry {
                program,
                tooltip: string(key, &sub, w!("InitialTooltip")).unwrap_or_default(),
                icon: binary(key, &sub, w!("IconSnapshot")),
            });
        }
        let _ = RegCloseKey(key);
    }
    out
}

/// # Safety
/// `key` is an open registry key.
unsafe fn string(key: HKEY, sub: &HSTRING, value: PCWSTR) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: Reads at most `size` bytes into `buffer`.
    let result = unsafe {
        RegGetValueW(
            key,
            sub,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut size),
        )
    };
    if result != ERROR_SUCCESS {
        return None;
    }
    let chars = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..chars.min(buffer.len())]))
}

/// # Safety
/// `key` is an open registry key.
unsafe fn binary(key: HKEY, sub: &HSTRING, value: PCWSTR) -> Option<Vec<u8>> {
    let mut size = 0u32;
    // SAFETY: A size query, then a read into a buffer of that size.
    unsafe {
        if RegGetValueW(
            key,
            sub,
            value,
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&raw mut size),
        ) != ERROR_SUCCESS
            || size == 0
            || size > 1024 * 1024
        {
            return None;
        }
        let mut bytes = vec![0u8; size as usize];
        if RegGetValueW(
            key,
            sub,
            value,
            RRF_RT_REG_BINARY,
            None,
            Some(bytes.as_mut_ptr().cast()),
            Some(&raw mut size),
        ) != ERROR_SUCCESS
        {
            return None;
        }
        bytes.truncate(size as usize);
        Some(bytes)
    }
}

/// Every running program: its process id and full path.
fn running_programs() -> Vec<(u32, String)> {
    let mut map = Vec::new();
    let mut pids = vec![0u32; 4096];
    let mut needed = 0u32;
    // SAFETY: The id buffer has the size passed; each process handle is
    // closed after its name is read.
    unsafe {
        if !K32EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &raw mut needed).as_bool()
        {
            return map;
        }
        pids.truncate(needed as usize / 4);
        for pid in pids {
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            if QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &raw mut len,
            )
            .is_ok()
            {
                map.push((pid, String::from_utf16_lossy(&buffer[..len as usize])));
            }
            let _ = CloseHandle(process);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{leaf, same_program, stem};

    #[test]
    fn names_programs() {
        assert_eq!(leaf(r"{GUID}\Some App\App.EXE"), "app.exe");
        assert_eq!(leaf("plain.exe"), "plain.exe");
        assert_eq!(stem(r"C:\Tools\My Tool.exe"), "My Tool");
        assert_eq!(stem("noext"), "noext");
    }

    #[test]
    fn matches_programs_by_their_path() {
        let packaged = r"{6D809377-0000}\WindowsApps\Some_1.0_x64\app\tool.exe";
        assert!(same_program(
            packaged,
            r"C:\Program Files\WindowsApps\Some_1.0_x64\app\Tool.exe"
        ));
        // The same file name, another program.
        assert!(!same_program(packaged, r"C:\Users\me\bin\tool.exe"));
        assert!(same_program(r"C:\A\b.exe", r"c:\a\B.EXE"));
        assert!(!same_program(r"C:\A\b.exe", r"C:\X\b.exe"));
        assert!(!same_program("", r"C:\A\b.exe"));
    }
}

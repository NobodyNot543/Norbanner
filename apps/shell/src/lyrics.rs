//! Synced lyrics for the playing song, looked up on lrclib.net on a worker
//! thread. Only the title, the artist and the length are sent, and only
//! while the user has the lyrics island on. Results (found or not) are kept
//! for the session, so a song is looked up once.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

use norbanner_core::lyrics::{self, Line};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Networking::WinHttp::{
            INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
            WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
            WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
            WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
            WinHttpSetTimeouts,
        },
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::{HSTRING, PCWSTR, w},
};

const HOST: PCWSTR = w!("lrclib.net");
/// lrclib asks clients to name themselves.
const AGENT: PCWSTR = w!("Norbanner (https://github.com/NobodyNot543/Norbanner)");
/// Lyric files are small; anything bigger is not one.
const MAX_BYTES: usize = 512 * 1024;

/// Which song a lookup is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Song {
    pub title: String,
    pub artist: String,
    /// Helps choose between versions of the song.
    pub album: String,
    /// The recording's length in whole seconds. Part of the key: right
    /// after a song change a player can report the new title with the old
    /// song's length, and that lookup must not stand in for the real one.
    pub seconds: u64,
}

/// The lyrics of a song: `None` when it has no synced lyrics.
pub type Found = (Song, Option<Arc<Vec<Line>>>);

pub struct Fetcher {
    requests: SyncSender<(Song, Option<Duration>)>,
    results: Arc<Mutex<Option<Found>>>,
    cache: HashMap<Song, Option<Arc<Vec<Line>>>>,
    pending: Option<Song>,
}

impl Fetcher {
    /// Starts the worker; it posts `message` to `hwnd` when a lookup ends.
    pub fn start(hwnd: HWND, message: u32) -> Option<Self> {
        let (requests, inbox) = mpsc::sync_channel(1);
        let results = Arc::new(Mutex::new(None));
        let window = hwnd.0 as isize;
        {
            let results = results.clone();
            thread::Builder::new()
                .name("norbanner-lyrics".into())
                .spawn(move || run(&inbox, &results, window, message))
                .ok()?;
        }
        Some(Self {
            requests,
            results,
            cache: HashMap::new(),
            pending: None,
        })
    }

    /// The lyrics for `song` if they are known; otherwise starts a lookup
    /// (one at a time; a newer song replaces a queued one) and returns
    /// `None` until it ends.
    pub fn get(&mut self, song: &Song, length: Option<Duration>) -> Option<Option<Arc<Vec<Line>>>> {
        if let Some(known) = self.cache.get(song) {
            return Some(known.clone());
        }
        if self.pending.as_ref() != Some(song)
            && self.requests.try_send((song.clone(), length)).is_ok()
        {
            self.pending = Some(song.clone());
        }
        None
    }

    /// Takes a finished lookup into the cache.
    pub fn collect(&mut self) {
        let Some((song, lines)) = self.results.lock().ok().and_then(|mut r| r.take()) else {
            return;
        };
        if self.pending.as_ref() == Some(&song) {
            self.pending = None;
        }
        self.cache.insert(song, lines);
    }
}

fn run(
    inbox: &Receiver<(Song, Option<Duration>)>,
    results: &Mutex<Option<Found>>,
    window: isize,
    message: u32,
) {
    while let Ok((song, length)) = inbox.recv() {
        let lines = lookup(&song, length).map(Arc::new);
        if let Ok(mut slot) = results.lock() {
            *slot = Some((song, lines));
        }
        // SAFETY: Posts a notification by value to the shell's own window.
        unsafe {
            let _ = PostMessageW(Some(HWND(window as *mut _)), message, WPARAM(0), LPARAM(0));
        }
    }
}

/// Looks the song up as lyric services name it (players add "(Official
/// Video)", "Artist - " and guest lists that the services do not have):
/// first by title and main artist, then with a general search, then with
/// the player's own words. [`lyrics::pick`] chooses among the versions with
/// synced lyrics each time; the first search that yields one wins.
fn lookup(song: &Song, length: Option<Duration>) -> Option<Vec<Line>> {
    let title = lyrics::clean_title(&song.title, &song.artist);
    let artist = lyrics::main_artist(&song.artist);
    if title.is_empty() {
        return None;
    }
    let mut searches = Vec::new();
    let mut by_fields = format!("/api/search?track_name={}", encode(&title));
    if !artist.is_empty() {
        by_fields.push_str(&format!("&artist_name={}", encode(&artist)));
    }
    searches.push(by_fields);
    searches.push(format!(
        "/api/search?q={}",
        encode(format!("{title} {artist}").trim())
    ));
    if title != song.title.trim() || artist != song.artist.trim() {
        let mut raw = format!("/api/search?track_name={}", encode(song.title.trim()));
        if !song.artist.trim().is_empty() {
            raw.push_str(&format!("&artist_name={}", encode(song.artist.trim())));
        }
        searches.push(raw);
    }
    let artist_lower = artist.to_lowercase();
    searches.iter().find_map(|path| {
        let body = get(path)?;
        let results: Vec<serde_json::Value> = serde_json::from_slice(&body).ok()?;
        let mut versions = Vec::new();
        let mut candidates = Vec::new();
        for result in &results {
            // A general search also finds other artists' songs of that name.
            let theirs = result
                .get("artistName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_lowercase();
            if !artist_lower.is_empty() && !theirs.contains(&artist_lower) {
                continue;
            }
            let Some(lines) = result
                .get("syncedLyrics")
                .and_then(serde_json::Value::as_str)
                .map(lyrics::parse)
                .filter(|lines| !lines.is_empty())
            else {
                continue;
            };
            candidates.push(lyrics::Candidate {
                duration: result.get("duration").and_then(serde_json::Value::as_f64),
                album: result
                    .get("albumName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                first: lines[0].at,
            });
            versions.push(lines);
        }
        let chosen = lyrics::pick(
            &candidates,
            length.map(|length| length.as_secs_f64()),
            &song.album,
        )?;
        Some(versions.swap_remove(chosen))
    })
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Closes a WinHTTP handle when dropped.
struct Handle(*mut core::ffi::c_void);

impl Handle {
    fn new(raw: *mut core::ffi::c_void) -> Option<Self> {
        (!raw.is_null()).then_some(Self(raw))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: The handle came from WinHTTP and is closed once.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// An HTTPS GET of `path` on lrclib; the body of a 200 response.
fn get(path: &str) -> Option<Vec<u8>> {
    // SAFETY: Plain WinHTTP calls on handles owned by this function; each
    // buffer outlives the call that fills it.
    unsafe {
        let session = Handle::new(WinHttpOpen(
            AGENT,
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 8000).ok()?;
        let connection = Handle::new(WinHttpConnect(
            session.0,
            HOST,
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ))?;
        let object = HSTRING::from(path);
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &object,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &raw mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &raw mut available).ok()?;
            if available == 0 {
                break;
            }
            let start = body.len();
            if start + available as usize > MAX_BYTES {
                return None;
            }
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                body[start..].as_mut_ptr().cast(),
                available,
                &raw mut read,
            )
            .ok()?;
            body.truncate(start + read as usize);
        }
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("A b&c"), "A%20b%26c");
        assert_eq!(encode("ñ"), "%C3%B1");
        assert_eq!(encode("safe-_.~"), "safe-_.~");
    }
}

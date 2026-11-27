//! Small pictures of windows for the windows panel, taken when it opens.
//!
//! `PrintWindow` with full-content rendering asks each window to draw
//! itself (GPU-drawn apps included) into a bitmap, which is scaled down to
//! a thumbnail. It runs on a worker thread: a window that does not answer
//! delays only its own picture, never the shell.

use norbanner_shell::icons::IconImage;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HALFTONE, HGDIOBJ,
        ReleaseDC, SRCCOPY, SelectObject, SetStretchBltMode, StretchBlt,
    },
    Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
    UI::WindowsAndMessaging::{GetWindowRect, IsIconic, PW_RENDERFULLCONTENT},
};

/// Thumbnails are at most this wide and tall, in pixels.
const MAX_W: i32 = 360;
const MAX_H: i32 = 220;

/// A picture of `hwnd`, or `None` if it is minimized or cannot draw itself.
pub fn capture(hwnd: isize) -> Option<IconImage> {
    let window = HWND(hwnd as *mut _);
    // SAFETY: Plain GDI on objects created and released here; `bits` points
    // into the DIB section, which lives until it is deleted below.
    unsafe {
        if IsIconic(window).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        GetWindowRect(window, &raw mut rect).ok()?;
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        if w < 8 || h < 8 || w > 16_384 || h > 16_384 {
            return None;
        }
        let scale = (MAX_W as f32 / w as f32)
            .min(MAX_H as f32 / h as f32)
            .min(1.0);
        let (tw, th) = (
            ((w as f32 * scale) as i32).max(1),
            ((h as f32 * scale) as i32).max(1),
        );

        let screen = GetDC(None);
        let full_dc = CreateCompatibleDC(Some(screen));
        let full = CreateCompatibleBitmap(screen, w, h);
        let small_dc = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: tw,
                // Negative: rows top to bottom.
                biHeight: -th,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let small = CreateDIBSection(
            Some(screen),
            &raw const info,
            DIB_RGB_COLORS,
            &raw mut bits,
            None,
            0,
        );

        let mut image = None;
        if let Ok(small) = small
            && !full.is_invalid()
            && !bits.is_null()
        {
            let old_full = SelectObject(full_dc, HGDIOBJ(full.0));
            let old_small = SelectObject(small_dc, HGDIOBJ(small.0));
            if PrintWindow(window, full_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool() {
                SetStretchBltMode(small_dc, HALFTONE);
                if StretchBlt(small_dc, 0, 0, tw, th, Some(full_dc), 0, 0, w, h, SRCCOPY).as_bool()
                {
                    let len = (tw * th * 4) as usize;
                    let mut bgra = std::slice::from_raw_parts(bits.cast::<u8>(), len).to_vec();
                    // GDI leaves alpha undefined; a window is opaque.
                    for px in bgra.as_chunks_mut::<4>().0 {
                        px[3] = 255;
                    }
                    // A window that drew nothing comes back all black.
                    if bgra
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|px| px[0] | px[1] | px[2] != 0)
                    {
                        image = Some(IconImage {
                            width: tw as u32,
                            height: th as u32,
                            bgra,
                        });
                    }
                }
            }
            SelectObject(full_dc, old_full);
            SelectObject(small_dc, old_small);
            let _ = DeleteObject(HGDIOBJ(small.0));
        }
        if !full.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(full.0));
        }
        let _ = DeleteDC(small_dc);
        let _ = DeleteDC(full_dc);
        ReleaseDC(None, screen);
        image
    }
}

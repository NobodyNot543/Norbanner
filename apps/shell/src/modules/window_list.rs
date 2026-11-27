//! The panel listing an app's open windows, out of the bar beside the app's
//! tile while the pointer rests on it: a picture of each window (taken when
//! the panel opens) and its title. Click a row to bring that window
//! forward, or its ✕ to close it.

use norbanner_core::geom::Rect;

use crate::{
    actions::Action,
    icons,
    render::paint::TextStyle,
    state::{DesktopApp, image_key},
    ui::{Ctx, Ui},
};

/// At most this many rows; more windows than that is unusual.
const MAX_ROWS: usize = 6;

/// The window pictures' size, 16:9.
fn thumb_size(ui: &Ctx) -> (f32, f32) {
    let k = ui.t.bar_inner / 44.0;
    (136.0 * k, 76.5 * k)
}

fn row_height(ui: &Ctx) -> f32 {
    thumb_size(ui).1 + ui.t.padding.small * 2.0
}

fn header_height(ui: &Ctx) -> f32 {
    ui.t.bar_inner * 0.85
}

pub fn size(ui: &Ctx, app: Option<&DesktopApp>) -> (f32, f32) {
    let t = ui.t;
    let rows = app.map_or(1, |a| a.windows.len().clamp(1, MAX_ROWS)) as f32;
    let w = 380.0 * (t.bar_inner / 44.0) + t.padding.medium * 2.0;
    let h = t.padding.medium * 2.0 + header_height(ui) + rows * row_height(ui);
    (w, h)
}

pub fn draw(ui: &mut Ui, rect: Rect, app: Option<&DesktopApp>) {
    let Some(app) = app else { return };
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, b) = (pal.primary, pal.tertiary);
    let ctx = ui.ctx();
    let header_h = header_height(&ctx);
    let row_h = row_height(&ctx);
    let (tw, th) = thumb_size(&ctx);
    let area = rect.inset(t.padding.medium);

    // Header: icon and the app's name.
    let icon_side = header_h * 0.62;
    let icon = Rect::new(
        area.x + t.padding.small,
        area.y + (header_h - icon_side) / 2.0,
        icon_side,
        icon_side,
    );
    if let Some(image) = ui.state.icons.get(&app.app_id) {
        ui.p.image_fit(image_key(&app.app_id), &image, icon);
    }
    let name = ui.state.icons.name(&app.app_id).unwrap_or_else(|| {
        app.app_id
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_string()
    });
    let name_x = icon.right() + t.spacing.medium;
    ui.p.text(
        &name,
        Rect::new(name_x, area.y, area.right() - name_x, header_h),
        TextStyle::new(t.font.normal).weight(700.0),
        a,
    );

    // One row per window.
    let front = ui.state.active.as_ref().map(|w| w.hwnd);
    let mut y = area.y + header_h;
    for &hwnd in app.windows.iter().take(MAX_ROWS) {
        let row = Rect::new(area.x, y, area.w, row_h - 2.0);
        let is_front = front == Some(hwnd);
        if is_front {
            ui.p.rect(row, t.rounding.medium, a.with_alpha(0.12));
            ui.p.rect_gradient(
                Rect::new(row.x + 4.0, row.y + row.h * 0.25, 3.0, row.h * 0.5),
                1.5,
                a,
                b,
            );
        }
        let hovered = ui.button(
            row,
            t.rounding.medium,
            pal.on_surface,
            Action::FocusWindow(hwnd),
        );
        // Close button at the row's end, pushed after the row so it wins
        // the click.
        let close_side = t.bar_inner * 0.6;
        let close = Rect::new(
            row.right() - close_side - (row.h - close_side) / 2.0,
            row.y + (row.h - close_side) / 2.0,
            close_side,
            close_side,
        );
        ui.icon_button(
            close,
            icons::CLOSE,
            close_side * 0.6,
            0.0,
            None,
            if hovered || is_front {
                pal.on_surface
            } else {
                pal.on_surface_variant.with_alpha(0.6)
            },
            Action::CloseWindow(hwnd),
        );
        // The window's picture, or the app's icon until there is one (and
        // for minimized windows, which cannot draw themselves).
        let thumb = Rect::new(row.x + t.padding.medium, row.y + (row.h - th) / 2.0, tw, th);
        match ui.state.previews.get(&hwnd) {
            Some((key, image)) => {
                let (iw, ih) = (image.width as f32, image.height as f32);
                let fit = (tw / iw).min(th / ih);
                let (w, h) = (iw * fit, ih * fit);
                let r = Rect::new(thumb.x + (tw - w) / 2.0, thumb.y + (th - h) / 2.0, w, h);
                ui.p.rect(thumb, t.rounding.small, pal.surface_container_highest);
                ui.p.image(*key, image, r, t.rounding.small);
            }
            None => {
                ui.p.rect(thumb, t.rounding.small, pal.surface_container_highest);
                if let Some(image) = ui.state.icons.get(&app.app_id) {
                    let side = th * 0.45;
                    let (cx, cy) = thumb.center();
                    ui.p.image_fit(
                        image_key(&app.app_id),
                        &image,
                        Rect::new(cx - side / 2.0, cy - side / 2.0, side, side),
                    );
                }
            }
        }
        let mut title = norbanner_shell::tracker::title(hwnd);
        if title.trim().is_empty() {
            title.clone_from(&name);
        }
        let text_x = thumb.right() + t.spacing.medium;
        ui.p.text(
            &title,
            Rect::new(
                text_x,
                row.y + t.padding.small,
                close.x - text_x - t.spacing.small,
                row.h - t.padding.small * 2.0,
            ),
            TextStyle::new(t.font.smaller)
                .weight(if is_front { 600.0 } else { 400.0 })
                .wrap(),
            if is_front {
                pal.on_surface
            } else {
                pal.on_surface_variant
            },
        );
        y += row_h;
    }
}

//! The panel listing an app's open windows, out of the bar beside the app's
//! tile while the pointer rests on it. Click a row to bring that window
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
const MAX_ROWS: usize = 8;

fn row_height(ui: &Ctx) -> f32 {
    ui.t.bar_inner * 0.9
}

fn header_height(ui: &Ctx) -> f32 {
    ui.t.bar_inner * 0.85
}

pub fn size(ui: &Ctx, app: Option<&DesktopApp>) -> (f32, f32) {
    let t = ui.t;
    let rows = app.map_or(1, |a| a.windows.len().clamp(1, MAX_ROWS)) as f32;
    let w = 300.0 * (t.bar_inner / 44.0) + t.padding.medium * 2.0;
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
        let close_side = row.h * 0.72;
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
        let mut title = norbanner_shell::tracker::title(hwnd);
        if title.trim().is_empty() {
            title.clone_from(&name);
        }
        ui.p.text(
            &title,
            Rect::new(
                row.x + t.padding.medium + 4.0,
                row.y,
                close.x - row.x - t.padding.medium - 4.0,
                row.h,
            ),
            TextStyle::new(t.font.smaller).weight(if is_front { 600.0 } else { 400.0 }),
            if is_front {
                pal.on_surface
            } else {
                pal.on_surface_variant
            },
        );
        y += row_h;
    }
}

//! A bubble out of the bar beside the tray button, while the pointer rests
//! on it: the icons of the apps running in the background (those with an
//! icon in Windows' notification area). Clicking one brings the app
//! forward; its ✕ closes it.

use norbanner_core::geom::Rect;

use crate::{
    actions::Action,
    icons,
    render::paint::TextStyle,
    ui::{Ctx, Ui},
};

const COLUMNS: usize = 4;

fn cell(ctx: &Ctx) -> f32 {
    ctx.t.bar_inner * 0.95
}

fn label_height(ctx: &Ctx) -> f32 {
    ctx.t.font.small * 1.8
}

pub fn size(ctx: &Ctx) -> (f32, f32) {
    let t = ctx.t;
    let cells = ctx.state.tray.len().max(1);
    let columns = cells.clamp(1, COLUMNS);
    let rows = cells.div_ceil(COLUMNS);
    let c = cell(ctx);
    (
        columns as f32 * c + t.padding.medium * 2.0,
        rows as f32 * c + label_height(ctx) + t.padding.medium * 2.0,
    )
}

pub fn draw(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    let ctx = ui.ctx();
    let c = cell(&ctx);
    let label_h = label_height(&ctx);
    let area = rect.inset(t.padding.medium);
    let apps = ui.state.tray.clone();
    let mut hovered_name = None;
    for (i, app) in apps.iter().enumerate() {
        let r = Rect::new(
            area.x + (i % COLUMNS) as f32 * c,
            area.y + (i / COLUMNS) as f32 * c,
            c,
            c,
        );
        let tile = r.inset(t.padding.extra_small);
        let over = ui.hovered(tile);
        if ui.button(
            tile,
            t.rounding.medium,
            pal.on_surface,
            Action::OpenTrayApp(app.path.clone()),
        ) {
            hovered_name = Some(app.name.clone());
        }
        let side = tile.w * 0.58;
        let (cx, cy) = tile.center();
        let icon_rect = Rect::new(cx - side / 2.0, cy - side / 2.0, side, side);
        // The program's own icon (sharp at any size); until it loads, the
        // picture Windows keeps of its tray icon.
        if let Some(icon) = ui.state.icons.get(&app.path) {
            ui.p.image_fit(crate::state::image_key(&app.path), &icon, icon_rect);
        } else if let Some(icon) = &app.icon {
            ui.p.image_fit(app.key, icon, icon_rect);
        } else {
            ui.p.icon(icons::APPS, cx, cy, side, 0.0, pal.on_surface_variant);
        }
        // Close, in the corner while the pointer is on the app. Pushed after
        // the tile so it wins the click.
        if over {
            let x_side = tile.w * 0.42;
            let close = Rect::new(
                tile.right() - x_side * 0.8,
                tile.y - x_side * 0.2,
                x_side,
                x_side,
            );
            if ui.icon_button(
                close,
                icons::CLOSE,
                x_side * 0.7,
                0.0,
                Some(pal.error_container),
                pal.on_error_container,
                Action::CloseTrayApp(app.path.clone()),
            ) {
                hovered_name = Some(format!(
                    "{} · {}",
                    crate::strings::text().tray_close,
                    app.name
                ));
            }
        }
    }
    // The name of the app under the pointer.
    let rows = apps.len().max(1).div_ceil(COLUMNS) as f32;
    let label = Rect::new(area.x, area.y + rows * c, area.w, label_h);
    let name = hovered_name.unwrap_or_else(|| crate::strings::text().tray_title.to_string());
    ui.p.text(
        &name,
        label,
        TextStyle::new(t.font.small).weight(500.0).center(),
        pal.on_surface_variant,
    );
}

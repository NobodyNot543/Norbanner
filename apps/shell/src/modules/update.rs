//! A bubble out of the bar's lower end while a newer Norbanner is out: its
//! version, a button that opens Settings to install it, and ✕ to put it
//! away until the next version. The shell never asks the network itself:
//! Settings, started in the background once a day, tells it.

use norbanner_core::geom::Rect;

use crate::{
    actions::Action,
    icons,
    render::paint::TextStyle,
    ui::{Ctx, Ui},
};

fn badge(ctx: &Ctx) -> f32 {
    ctx.t.bar_inner * 0.9
}

fn button_height(ctx: &Ctx) -> f32 {
    ctx.t.font.normal * 2.6
}

pub fn size(ctx: &Ctx) -> (f32, f32) {
    let t = ctx.t;
    (
        t.bar_inner * 5.5 + t.padding.large * 2.0,
        badge(ctx) + t.spacing.medium + button_height(ctx) + t.padding.large * 2.0,
    )
}

pub fn draw(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    let ctx = ui.ctx();
    let Some(version) = ui.state.update.clone() else {
        return;
    };
    let text = crate::strings::text();
    let area = rect.inset(t.padding.large);
    let side = badge(&ctx);
    let button_h = button_height(&ctx);

    // The package badge, then what is new, then ✕.
    let badge_rect = Rect::new(area.x, area.y, side, side);
    ui.p.rect(badge_rect, side / 2.0, pal.primary_container);
    let (bx, by) = badge_rect.center();
    ui.p.icon(
        icons::DEPLOYED_CODE,
        bx,
        by,
        side * 0.55,
        1.0,
        pal.on_primary_container,
    );
    let close_side = side * 0.75;
    let close = Rect::new(
        area.right() - close_side,
        area.y + (side - close_side) / 2.0,
        close_side,
        close_side,
    );
    let text_x = badge_rect.right() + t.spacing.medium;
    let text_w = (close.x - t.spacing.small - text_x).max(0.0);
    ui.p.text(
        text.update_title,
        Rect::new(text_x, area.y, text_w, side * 0.5),
        TextStyle::new(t.font.small).weight(500.0),
        pal.on_surface_variant,
    );
    ui.p.text(
        &format!("Norbanner {version}"),
        Rect::new(text_x, area.y + side * 0.45, text_w, side * 0.55),
        TextStyle::new(t.font.normal).weight(600.0),
        pal.on_surface,
    );
    ui.icon_button(
        close,
        icons::CLOSE,
        close_side * 0.6,
        0.0,
        None,
        pal.on_surface_variant,
        Action::DismissUpdate,
    );

    // Install: Settings opens and shows the update itself.
    let button = Rect::new(area.x, area.bottom() - button_h, area.w, button_h);
    ui.p.rect(button, button_h / 2.0, pal.primary);
    ui.button(
        button,
        button_h / 2.0,
        pal.on_primary,
        Action::InstallUpdate,
    );
    ui.p.text(
        text.update_install,
        button,
        TextStyle::new(t.font.normal).weight(600.0).center(),
        pal.on_primary,
    );
}

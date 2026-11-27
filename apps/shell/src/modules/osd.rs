//! Volume and brightness sliders on the right edge. They show when the
//! level changes (keys, another app) or when the pointer rests on the right
//! edge, and can be dragged or scrolled.
//!
//! Each slider is a slim column: the percentage on top, a narrow track
//! filling with the accent gradient and a knob at the level, the icon
//! underneath.

use std::time::Instant;

use norbanner_core::geom::Rect;

use crate::{
    actions::Action,
    icons,
    modules::bar::volume_icon,
    render::paint::TextStyle,
    tokens::Tokens,
    ui::{Ctx, Slider, Ui},
};

#[derive(Default)]
pub struct OsdState {
    /// When the OSD should hide again unless hovered.
    pub until: Option<Instant>,
}

fn sliders(ui: &Ctx, monitor: isize) -> usize {
    let mut n = 1;
    if ui.config.osd.enable_brightness && ui.state.brightness.contains_key(&monitor) {
        n += 1;
    }
    n
}

/// A slider column's width: wide enough for the percentage and to grab.
fn column_width(t: &Tokens) -> f32 {
    t.osd_slider_width * 2.2
}

fn label_height(t: &Tokens) -> f32 {
    t.font.small * 1.4
}

fn column_height(t: &Tokens) -> f32 {
    label_height(t) + t.spacing.small + t.osd_slider_height + t.spacing.small + t.icon.small
}

pub fn size(ui: &Ctx, monitor: isize) -> (f32, f32) {
    let t = ui.t;
    let n = sliders(ui, monitor) as f32;
    let w = column_width(t) + t.padding.medium * 2.0;
    let h = n * column_height(t) + (n - 1.0) * t.spacing.large + t.padding.medium * 2.0;
    (w, h)
}

fn slider(ui: &mut Ui, column: Rect, value: f32, glyph: char, target: Slider, scroll: Action) {
    let t = *ui.t;
    let pal = *ui.pal;
    let (a, b) = (pal.primary, pal.tertiary);
    let value = value.clamp(0.0, 1.0);
    let (cx, _) = column.center();

    // Percentage on top.
    let label = Rect::new(column.x, column.y, column.w, label_height(&t));
    ui.p.text(
        &format!("{}", (value * 100.0).round() as u32),
        label,
        TextStyle::new(t.font.small).weight(600.0).center(),
        pal.on_surface,
    );

    // The track: dark groove, gradient fill from the bottom, knob on top.
    let w = t.osd_slider_width;
    let track = Rect::new(
        cx - w / 2.0,
        label.bottom() + t.spacing.small,
        w,
        t.osd_slider_height,
    );
    ui.p.rect(track, w / 2.0, pal.surface_container_highest);
    let filled = (track.h * value).max(w);
    let fill = Rect::new(track.x, track.bottom() - filled, w, filled);
    ui.p.rect_gradient(fill, w / 2.0, b, a);
    let knob_y = fill.y + w / 2.0;
    let hovered = ui.hovered(column);
    let knob = w * if hovered { 0.62 } else { 0.5 };
    ui.p.circle(cx, knob_y, knob, pal.on_primary);
    ui.p.circle(cx, knob_y, knob * 0.45, a);

    // The icon underneath.
    let icon_y = track.bottom() + t.spacing.small + t.icon.small / 2.0;
    ui.p.icon(glyph, cx, icon_y, t.icon.small, 1.0, a);

    // Grab anywhere in the column's track height; scroll over the column.
    ui.slider(Rect::new(column.x, track.y, column.w, track.h), target);
    ui.scroll(column, scroll);
}

pub fn draw(ui: &mut Ui, rect: Rect, monitor: isize) {
    let t = *ui.t;
    let x = rect.x + t.padding.medium;
    let mut y = rect.y + t.padding.medium;
    let column = |y: f32| Rect::new(x, y, column_width(&t), column_height(&t));
    let (volume, muted) = ui
        .state
        .volume
        .map_or((0.0, true), |v| (f32::from(v.percent) / 100.0, v.muted));
    let max = ui.config.services.max_volume;
    slider(
        ui,
        column(y),
        volume / max,
        volume_icon(volume, muted),
        Slider::Volume,
        Action::VolumeStep(0),
    );
    y += column_height(&t) + t.spacing.large;
    if ui.config.osd.enable_brightness
        && let Some(&brightness) = ui.state.brightness.get(&monitor)
    {
        let glyph = if brightness < 0.34 {
            icons::BRIGHTNESS_LOW
        } else if brightness < 0.67 {
            icons::BRIGHTNESS_MEDIUM
        } else {
            icons::BRIGHTNESS_HIGH
        };
        slider(
            ui,
            column(y),
            brightness,
            glyph,
            Slider::Brightness(monitor),
            Action::BrightnessStep(monitor, 0),
        );
    }
}

//! A small island at the top that shows the line of the song being sung.
//! It hangs from the frame like the dashboard and gives way to it when the
//! pointer comes near the top edge.
//!
//! Between lines nothing is drawn: a timer wakes the shell when the next line
//! starts, and the change is a short slide (the old line rises and fades,
//! the new one comes up from below).

use std::time::{Duration, Instant};

use norbanner_core::{anim::Curve, geom::Rect};

use crate::{
    icons,
    render::paint::TextStyle,
    state::LyricsView,
    ui::{Ctx, Ui},
};

/// How long a line change takes at the normal animation speed.
pub const CHANGE: Duration = Duration::from_millis(420);

pub fn size(ctx: &Ctx) -> (f32, f32) {
    let t = ctx.t;
    let h = t.font.normal * 1.6 + t.padding.medium * 2.0;
    (t.dash_media_width * 2.6, h)
}

/// How far the latest change has gone, 0–1 (1 when settled).
pub fn change_progress(view: &LyricsView, anim_scale: f32, now: Instant) -> f32 {
    let length = CHANGE.as_secs_f32() * anim_scale.max(0.0);
    if length <= 0.0 {
        return 1.0;
    }
    (now.saturating_duration_since(view.changed).as_secs_f32() / length).clamp(0.0, 1.0)
}

pub fn draw(ui: &mut Ui, rect: Rect) {
    let Some(view) = ui.state.lyrics.clone() else {
        return;
    };
    let t = *ui.t;
    let pal = *ui.pal;
    let progress = Curve::EmphasizedDecel.ease(change_progress(
        &view,
        ui.config.appearance.anim_scale,
        ui.now,
    ));
    let inner = Rect::new(
        rect.x + t.padding.large,
        rect.y + t.padding.medium,
        rect.w - t.padding.large * 2.0,
        rect.h - t.padding.medium * 2.0,
    );
    let rise = inner.h * 0.8;
    let style = TextStyle::new(t.font.normal).weight(500.0).center();
    let before = ui.p.opacity;
    let line = |ui: &mut Ui, index: Option<usize>, offset: f32, alpha: f32| {
        if alpha <= 0.01 {
            return;
        }
        ui.p.opacity = before * alpha;
        let text = index
            .and_then(|i| view.lines.get(i))
            .map(|l| l.text.as_str())
            .unwrap_or_default();
        let r = Rect::new(inner.x, inner.y + offset, inner.w, inner.h);
        if text.is_empty() {
            // Before the first line, and instrumental gaps.
            let (cx, cy) = r.center();
            ui.p.icon(icons::MUSIC_NOTE, cx, cy, t.icon.medium, 1.0, pal.primary);
        } else {
            ui.p.text(text, r, style, pal.primary);
        }
    };
    if progress < 1.0 {
        line(ui, view.previous, -rise * progress, 1.0 - progress);
    }
    line(ui, view.current, rise * (1.0 - progress), progress);
    ui.p.opacity = before;
}

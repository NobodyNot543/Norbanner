//! A small island at the top that shows the line of the song being sung.
//! It hangs from the frame like the dashboard and gives way to it when the
//! pointer comes near the top edge.
//!
//! The line is drawn dim and a sweep of the accent colour fills it word by
//! word as it is sung. Between changes nothing is drawn: a timer wakes the
//! shell when the next word or line starts, and each change is a short
//! animation (the sweep reaching the new word, or the old line rising while
//! the new one comes up from below). Scrolling over the island moves the
//! timing a quarter second at a time, for versions that are slightly off.

use std::time::{Duration, Instant};

use norbanner_core::{anim::Curve, geom::Rect};

use crate::{
    actions::Action,
    icons,
    render::paint::TextStyle,
    state::LyricsView,
    ui::{Ctx, Ui},
};

/// How long a line change takes at the normal animation speed.
const LINE: Duration = Duration::from_millis(420);
/// How long the sweep takes to reach a new word.
const WORD: Duration = Duration::from_millis(180);
/// How long a timing correction stays on the island after a scroll.
const OFFSET_SHOWN: Duration = Duration::from_millis(1600);

pub fn size(ctx: &Ctx) -> (f32, f32) {
    let t = ctx.t;
    let h = t.font.normal * 1.6 + t.padding.medium * 2.0;
    (t.dash_media_width * 2.6, h)
}

fn progress(since: Instant, length: Duration, anim_scale: f32, now: Instant) -> f32 {
    let length = length.as_secs_f32() * anim_scale.max(0.0);
    if length <= 0.0 {
        return 1.0;
    }
    (now.saturating_duration_since(since).as_secs_f32() / length).clamp(0.0, 1.0)
}

/// Whether anything on the island is still moving (frames are needed).
pub fn moving(view: &LyricsView, anim_scale: f32, now: Instant) -> bool {
    progress(view.changed, LINE, anim_scale, now) < 1.0
        || progress(view.word_changed, WORD, anim_scale, now) < 1.0
        || view
            .offset_changed
            .is_some_and(|at| now.saturating_duration_since(at) < OFFSET_SHOWN)
}

pub fn draw(ui: &mut Ui, rect: Rect) {
    let Some(view) = ui.state.lyrics.clone() else {
        return;
    };
    ui.scroll(rect, Action::LyricsOffset(0));
    let t = *ui.t;
    let pal = *ui.pal;
    let scale = ui.config.appearance.anim_scale;
    let now = ui.now;
    let line_p = Curve::EmphasizedDecel.ease(progress(view.changed, LINE, scale, now));
    let word_p = Curve::StandardDecel.ease(progress(view.word_changed, WORD, scale, now));
    let inner = Rect::new(
        rect.x + t.padding.large,
        rect.y + t.padding.medium,
        rect.w - t.padding.large * 2.0,
        rect.h - t.padding.medium * 2.0,
    );
    let rise = inner.h * 0.8;
    let before = ui.p.opacity;
    // `sung` is how many words are filled, and how many were before the
    // sweep started (it moves from one to the other).
    let line =
        |ui: &mut Ui, index: Option<usize>, offset: f32, alpha: f32, sung: (usize, usize)| {
            if alpha <= 0.01 {
                return;
            }
            ui.p.opacity = before * alpha;
            let Some(line) = index
                .and_then(|i| view.lines.get(i))
                .filter(|l| !l.text.is_empty())
            else {
                // Before the first line, and instrumental gaps.
                let (cx, cy) = Rect::new(inner.x, inner.y + offset, inner.w, inner.h).center();
                ui.p.icon(icons::MUSIC_NOTE, cx, cy, t.icon.medium, 1.0, pal.primary);
                return;
            };
            let mut style = TextStyle::new(t.font.normal).weight(500.0);
            let (full, _) = ui.p.measure(&line.text, style);
            if full > inner.w {
                style.size *= (inner.w / full).max(0.72);
            }
            let (width, _) = ui.p.measure(&line.text, style);
            let (cx, _) = inner.center();
            let r = Rect::new(cx - width / 2.0, inner.y + offset, width + 4.0, inner.h);
            ui.p.text(
                &line.text,
                r,
                style,
                pal.on_surface_variant.with_alpha(0.55),
            );
            let reach = |ui: &mut Ui, words: usize| -> f32 {
                match words.checked_sub(1).and_then(|i| line.words.get(i)) {
                    Some(word) => ui.p.measure(&line.text[..word.end], style).0,
                    None => 0.0,
                }
            };
            let (to, from) = sung;
            let fill = if to == from {
                reach(ui, to)
            } else {
                let start = reach(ui, from);
                start + (reach(ui, to) - start) * word_p
            };
            if fill > 0.5 {
                ui.p.clip(Rect::new(r.x - 2.0, r.y, fill + 2.0, r.h));
                ui.p.text(&line.text, r, style, pal.primary);
                ui.p.unclip();
            }
        };
    if line_p < 1.0 {
        // The line that just ended leaves fully sung.
        let all = view
            .previous
            .and_then(|i| view.lines.get(i))
            .map_or(0, |l| l.words.len());
        line(ui, view.previous, -rise * line_p, 1.0 - line_p, (all, all));
    }
    line(
        ui,
        view.current,
        rise * (1.0 - line_p),
        line_p,
        (view.words, view.previous_words),
    );
    ui.p.opacity = before;

    // The timing correction, for a moment after a scroll.
    if let Some(at) = view.offset_changed {
        let shown = now.saturating_duration_since(at);
        if shown < OFFSET_SHOWN {
            let fade = ((OFFSET_SHOWN - shown).as_secs_f32() / 0.4).clamp(0.0, 1.0);
            let label = format!("{:+.2} s", view.offset_ms as f32 / 1000.0);
            let style = TextStyle::new(t.font.small).weight(500.0);
            let (w, _) = ui.p.measure(&label, style);
            ui.p.opacity = before * fade;
            ui.p.text(
                &label,
                Rect::new(
                    rect.right() - w - t.padding.medium,
                    inner.y,
                    w + 4.0,
                    inner.h,
                ),
                style,
                pal.on_surface_variant,
            );
            ui.p.opacity = before;
        }
    }
}

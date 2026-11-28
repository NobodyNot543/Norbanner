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

/// How the lyrics move (Settings → Panels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The accent colour fills the line word by word.
    Sweep,
    /// The word being sung lights up.
    Word,
    /// Whole lines, sliding up.
    Line,
    /// Whole lines, fading into each other.
    Fade,
}

impl Mode {
    pub fn from(name: &str) -> Self {
        match name {
            "word" => Self::Word,
            "line" => Self::Line,
            "fade" => Self::Fade,
            _ => Self::Sweep,
        }
    }

    /// Whether words are followed (the shell then wakes for each word, not
    /// only for each line).
    pub fn by_word(self) -> bool {
        matches!(self, Self::Sweep | Self::Word)
    }
}

/// The song's title and artist, small, above the line.
fn header_height(t: &crate::tokens::Tokens) -> f32 {
    t.font.small * 1.5
}

pub fn size(ctx: &Ctx) -> (f32, f32) {
    let t = ctx.t;
    let h = t.padding.medium * 2.0 + header_height(t) + t.font.normal * 1.6;
    (t.dash_media_width * 2.6, h)
}

/// The little wave beside the title: bars that rise and fall while the
/// song plays and rest low while it is paused. `time` is in seconds.
fn wave(ui: &mut Ui, x: f32, cy: f32, height: f32, playing: bool, time: f32) -> f32 {
    const BARS: [(f32, f32); 5] = [(7.1, 0.0), (9.3, 1.3), (6.2, 2.1), (8.4, 0.7), (7.7, 2.8)];
    let pal = *ui.pal;
    let bar = (height * 0.16).max(2.0);
    let gap = bar * 0.8;
    for (i, (speed, phase)) in BARS.iter().enumerate() {
        let level = if playing {
            0.3 + 0.7 * (time * speed + phase).sin().abs()
        } else {
            0.25
        };
        let h = height * level;
        let bx = x + i as f32 * (bar + gap);
        ui.p.rect(Rect::new(bx, cy - h / 2.0, bar, h), bar / 2.0, pal.primary);
    }
    BARS.len() as f32 * (bar + gap) - gap
}

/// The title row: the wave, then "title · artist", centred together.
fn header(ui: &mut Ui, row: Rect) {
    let Some(track) = ui.state.media.clone() else {
        return;
    };
    let t = *ui.t;
    let pal = *ui.pal;
    let label = if track.artist.is_empty() {
        track.title.clone()
    } else {
        format!("{} · {}", track.title, track.artist)
    };
    let style = TextStyle::new(t.font.small).weight(500.0);
    let wave_h = row.h * 0.62;
    // Five bars and four gaps of 0.8 bar (see `wave`).
    let wave_w = (wave_h * 0.16).max(2.0) * 8.2;
    let gap = t.spacing.small;
    let (text_w, _) = ui.p.measure(&label, style);
    let text_w = text_w.min(row.w - wave_w - gap);
    let x = row.x + (row.w - (wave_w + gap + text_w)) / 2.0;
    let (_, cy) = row.center();
    let started = ui.state.started.unwrap_or(ui.now);
    let time = ui.now.saturating_duration_since(started).as_secs_f32();
    let drawn = wave(ui, x, cy, wave_h, track.playing, time);
    ui.p.text(
        &label,
        Rect::new(x + drawn + gap, row.y, text_w + 4.0, row.h),
        style,
        pal.on_surface_variant,
    );
}

fn progress(since: Instant, length: Duration, anim_scale: f32, now: Instant) -> f32 {
    let length = length.as_secs_f32() * anim_scale.max(0.0);
    if length <= 0.0 {
        return 1.0;
    }
    (now.saturating_duration_since(since).as_secs_f32() / length).clamp(0.0, 1.0)
}

/// How often the wave moves while a song plays: smooth enough to read as
/// motion, far below the screen's rate (the island is all that changes).
pub const WAVE_FRAME: Duration = Duration::from_millis(66);

/// Whether anything on the island is still moving (frames are needed).
pub fn moving(view: &LyricsView, anim_scale: f32, now: Instant) -> bool {
    progress(view.changed, LINE, anim_scale, now) < 1.0
        || progress(view.word_changed, WORD, anim_scale, now) < 1.0
        || view
            .offset_changed
            .is_some_and(|at| now.saturating_duration_since(at) < OFFSET_SHOWN)
}

pub fn draw(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    let content = Rect::new(
        rect.x + t.padding.large,
        rect.y + t.padding.medium,
        rect.w - t.padding.large * 2.0,
        rect.h - t.padding.medium * 2.0,
    );
    let header_h = header_height(&t);
    header(ui, Rect::new(content.x, content.y, content.w, header_h));
    let inner = Rect::new(
        content.x,
        content.y + header_h,
        content.w,
        content.h - header_h,
    );
    let Some(view) = ui.state.lyrics.clone() else {
        // No synced lyrics for this song (or still looking): a note.
        let (cx, cy) = inner.center();
        ui.p.icon(icons::MUSIC_NOTE, cx, cy, t.icon.medium, 1.0, pal.primary);
        return;
    };
    ui.scroll(rect, Action::LyricsOffset(0));
    let scale = ui.config.appearance.anim_scale;
    let now = ui.now;
    let line_p = Curve::EmphasizedDecel.ease(progress(view.changed, LINE, scale, now));
    let word_p = Curve::StandardDecel.ease(progress(view.word_changed, WORD, scale, now));
    let mode = Mode::from(ui.config.dashboard.lyrics_animation.as_str());
    // Fade swaps lines in place; the others slide them up.
    let rise = if mode == Mode::Fade {
        0.0
    } else {
        inner.h * 0.8
    };
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
            if !mode.by_word() {
                ui.p.text(&line.text, r, style, pal.primary);
                return;
            }
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
            if mode == Mode::Word {
                // Words already sung settle to the text colour; the one
                // being sung lights up in the accent.
                let done = reach(ui, to.saturating_sub(1));
                let end = reach(ui, to);
                if done > 0.5 {
                    ui.p.clip(Rect::new(r.x - 2.0, r.y, done + 2.0, r.h));
                    ui.p.text(&line.text, r, style, pal.on_surface);
                    ui.p.unclip();
                }
                if end > done {
                    let lit = if to == from { 1.0 } else { word_p };
                    let saved = ui.p.opacity;
                    ui.p.opacity = saved * lit;
                    ui.p.clip(Rect::new(r.x + done, r.y, end - done + 2.0, r.h));
                    ui.p.text(&line.text, r, style, pal.primary);
                    ui.p.unclip();
                    ui.p.opacity = saved;
                }
                return;
            }
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

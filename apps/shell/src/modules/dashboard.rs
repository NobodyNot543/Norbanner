//! The drawer at the top of the screen: who is signed in and for how long,
//! a large clock, the month, resource meters and what is playing.
//!
//! ```text
//! ┌──────────────────────────────────────┬─────────┐
//! │ user · uptime                        │         │
//! ├────────┬────────────────┬────────────┤  media  │
//! │ clock  │    calendar    │ resources  │         │
//! └────────┴────────────────┴────────────┴─────────┘
//! ```

use std::time::Instant;

use norbanner_core::{calendar, geom::Rect};
use norbanner_shell::sysinfo::format_bytes;

use crate::{
    actions::Action,
    icons,
    media::Control,
    render::paint::TextStyle,
    state::{self, format_date},
    strings::text,
    ui::{Ctx, Ui},
};

#[derive(Default)]
pub struct DashState {
    /// Months away from the current one the calendar shows.
    pub month_offset: i32,
}

struct Layout {
    user_h: f32,
    row_h: f32,
    date_w: f32,
    calendar_w: f32,
    resources_w: f32,
    media_w: f32,
}

fn layout(ui: &Ctx) -> Layout {
    let t = ui.t;
    let cell = t.font.normal * 2.2;
    Layout {
        user_h: 64.0 + t.padding.large * 2.0,
        row_h: cell * 7.0 + t.font.large * 2.0 + t.padding.large * 2.0,
        date_w: t.dash_date_width,
        calendar_w: cell * 7.0 + t.padding.large * 2.0,
        resources_w: 3.0 * 36.0 + t.padding.large * 2.0,
        media_w: t.dash_media_width,
    }
}

/// The drawer's natural size.
pub fn size(ui: &Ctx) -> (f32, f32) {
    let t = ui.t;
    let l = layout(ui);
    let gap = t.spacing.medium;
    let left = l.date_w + l.calendar_w + l.resources_w + gap * 2.0;
    let w = left + gap + l.media_w + t.padding.large * 2.0;
    let h = l.user_h + gap + l.row_h + t.padding.large * 2.0;
    (w, h)
}

pub fn draw(ui: &mut Ui, rect: Rect, dash: &DashState) {
    let t = *ui.t;
    let l = layout(&ui.ctx());
    let gap = t.spacing.medium;
    let area = rect.inset(t.padding.large);
    let left_w = l.date_w + l.calendar_w + l.resources_w + gap * 2.0;
    let user = Rect::new(area.x, area.y, left_w, l.user_h);
    let row_y = area.y + l.user_h + gap;
    let date = Rect::new(area.x, row_y, l.date_w, l.row_h);
    let calendar = Rect::new(date.right() + gap, row_y, l.calendar_w, l.row_h);
    let resources = Rect::new(calendar.right() + gap, row_y, l.resources_w, l.row_h);
    let media = Rect::new(area.x + left_w + gap, area.y, l.media_w, area.h);
    user_card(ui, user);
    date_card(ui, date);
    calendar_card(ui, calendar, dash);
    resources_card(ui, resources);
    media_card(ui, media);
}

fn user_card(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.card(rect, t.rounding.extra_large);
    let side = rect.h - t.padding.large * 2.0;
    let avatar = Rect::new(
        rect.x + t.padding.large,
        rect.y + t.padding.large,
        side,
        side,
    );
    if let Some(image) = ui.state.avatar.clone() {
        ui.p.image(state::image_key("avatar"), &image, avatar, side / 2.0);
    } else {
        ui.p.circle(
            avatar.x + side / 2.0,
            avatar.y + side / 2.0,
            side / 2.0,
            pal.primary_container,
        );
        ui.p.icon(
            icons::ACCOUNT_CIRCLE,
            avatar.x + side / 2.0,
            avatar.y + side / 2.0,
            side * 0.7,
            1.0,
            pal.on_primary_container,
        );
    }
    let x = avatar.right() + t.spacing.large;
    let w = rect.right() - x - t.padding.large;
    let name = ui.state.user_name.clone();
    ui.p.text(
        &name,
        Rect::new(x, rect.y + t.padding.large, w, side * 0.55),
        TextStyle::new(t.font.large).weight(500.0),
        pal.on_surface,
    );
    let uptime = format_uptime(norbanner_shell::sysinfo::uptime());
    let line = Rect::new(x, rect.y + t.padding.large + side * 0.55, w, side * 0.45);
    ui.p.icon(
        icons::SCHEDULE,
        line.x + t.icon.small / 2.0,
        line.y + line.h / 2.0,
        t.icon.small,
        0.0,
        pal.tertiary,
    );
    ui.p.text(
        &format!("{} {uptime}", text().up_for),
        Rect::new(
            line.x + t.icon.small + t.spacing.small,
            line.y,
            line.w - t.icon.small,
            line.h,
        ),
        TextStyle::new(t.font.normal),
        pal.on_surface_variant,
    );
}

fn format_uptime(uptime: std::time::Duration) -> String {
    let s = text();
    let minutes = uptime.as_secs() / 60;
    let (days, hours, mins) = (minutes / 1440, (minutes / 60) % 24, minutes % 60);
    let unit = |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    if days > 0 {
        format!(
            "{}, {}",
            unit(days, s.day, s.days),
            unit(hours, s.hour, s.hours)
        )
    } else if hours > 0 {
        format!(
            "{}, {}",
            unit(hours, s.hour, s.hours),
            unit(mins, s.minute, s.minutes)
        )
    } else {
        unit(mins, s.minute, s.minutes)
    }
}

fn date_card(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.card(rect, t.rounding.large);
    ui.click(rect, Action::OpenDateSettings);
    let clock = ui.state.clock;
    let twelve = ui.config.services.use_twelve_hour_clock;
    let big = TextStyle::new(t.font.extra_large * 1.25)
        .weight(600.0)
        .center();
    let line = big.size * 1.2;
    let total = line * 2.0 + t.font.normal * 1.6 + t.font.normal * 1.4 + t.spacing.small * 2.0;
    let mut y = rect.y + (rect.h - total) / 2.0;
    ui.p.text(
        &clock.hour_text(twelve),
        Rect::new(rect.x, y, rect.w, line),
        big,
        pal.secondary,
    );
    y += line;
    ui.p.text(
        "•••",
        Rect::new(rect.x, y - line * 0.25, rect.w, line * 0.5),
        TextStyle::new(t.font.small).center(),
        pal.primary,
    );
    ui.p.text(
        &format!("{:02}", clock.minute),
        Rect::new(rect.x, y, rect.w, line),
        big,
        pal.secondary,
    );
    y += line + t.spacing.small;
    let weekday = format_date(clock.year, clock.month, clock.day, "dddd");
    let day = format_date(clock.year, clock.month, clock.day, "d MMM");
    ui.p.text(
        &weekday,
        Rect::new(rect.x, y, rect.w, t.font.normal * 1.6),
        TextStyle::new(t.font.normal).weight(500.0).center(),
        pal.tertiary,
    );
    y += t.font.normal * 1.6;
    ui.p.text(
        &day,
        Rect::new(rect.x, y, rect.w, t.font.normal * 1.4),
        TextStyle::new(t.font.smaller).center(),
        pal.on_surface_variant,
    );
}

fn calendar_card(ui: &mut Ui, rect: Rect, dash: &DashState) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.card(rect, t.rounding.extra_large);
    let today = ui.state.clock;
    let (year, month) = calendar::add_months(today.year, today.month, dash.month_offset);
    let area = rect.inset(t.padding.large);
    ui.scroll(rect, Action::CalendarMonth(0));

    // Header: month name between previous/next buttons.
    let header_h = t.font.large * 2.0;
    let title = format_date(year, month, 1, "MMMM yyyy");
    let button = header_h * 0.9;
    ui.icon_button(
        Rect::new(area.x, area.y + (header_h - button) / 2.0, button, button),
        icons::CHEVRON_LEFT,
        t.icon.medium,
        0.0,
        None,
        pal.tertiary,
        Action::CalendarMonth(-1),
    );
    ui.icon_button(
        Rect::new(
            area.right() - button,
            area.y + (header_h - button) / 2.0,
            button,
            button,
        ),
        icons::CHEVRON_RIGHT,
        t.icon.medium,
        0.0,
        None,
        pal.tertiary,
        Action::CalendarMonth(1),
    );
    let title_rect = Rect::new(area.x + button, area.y, area.w - button * 2.0, header_h);
    ui.click(title_rect, Action::CalendarMonth(-dash.month_offset));
    ui.p.text(
        &title,
        title_rect,
        TextStyle::new(t.font.normal).weight(500.0).center(),
        pal.primary,
    );

    let first = state::first_weekday();
    let cell = area.w / 7.0;
    let mut y = area.y + header_h;
    // Weekday initials.
    for i in 0..7u32 {
        let wd = (first + i) % 7;
        // 2023-01-01 was a Sunday: day 1 + wd is that weekday.
        let name = format_date(2023, 1, 1 + wd, "ddd");
        let short: String = name.chars().take(2).collect();
        let c = if wd == 0 || wd == 6 {
            pal.secondary
        } else {
            pal.on_surface_variant
        };
        ui.p.text(
            &short,
            Rect::new(area.x + cell * i as f32, y, cell, cell * 0.8),
            TextStyle::new(t.font.smaller).weight(500.0).center(),
            c,
        );
    }
    y += cell * 0.8;
    let rows = (area.bottom() - y) / 6.0;
    for (i, day) in calendar::grid(year, month, first).iter().enumerate() {
        let col = (i % 7) as f32;
        let row = (i / 7) as f32;
        let cell_rect = Rect::new(area.x + col * cell, y + row * rows, cell, rows);
        let is_today = day.in_month && dash.month_offset == 0 && day.day == today.day;
        let mut colour = if day.in_month {
            pal.on_surface
        } else {
            pal.outline
        };
        if is_today {
            let side = cell.min(rows) * 0.92;
            let (cx, cy) = cell_rect.center();
            ui.p.circle(cx, cy, side / 2.0, pal.primary);
            colour = pal.on_primary;
        }
        let style =
            TextStyle::new(t.font.smaller)
                .center()
                .weight(if is_today { 600.0 } else { 400.0 });
        ui.p.text(&day.day.to_string(), cell_rect, style, colour);
    }
}

fn resources_card(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.card(rect, t.rounding.large);
    let usage = ui.state.usage;
    let s = text();
    let meters = [
        (icons::MEMORY, usage.cpu, s.cpu.to_string(), pal.primary),
        (
            icons::MEMORY_ALT,
            usage.memory(),
            format!("{} {}", s.memory, format_bytes(usage.memory_used)),
            pal.secondary,
        ),
        (
            icons::HARD_DRIVE,
            usage.disk(),
            format!("{} {}", s.storage, format_bytes(usage.disk_used)),
            pal.tertiary,
        ),
    ];
    let area = rect.inset(t.padding.large);
    let column = area.w / 3.0;
    let bar_w = t.dash_progress * 1.6;
    let icon_h = t.icon.medium + t.spacing.small;
    let percent_h = t.font.small * 1.6;
    for (i, (glyph, value, label, colour)) in meters.into_iter().enumerate() {
        let x = area.x + column * i as f32 + (column - bar_w) / 2.0;
        let track = Rect::new(x, area.y + percent_h, bar_w, area.h - icon_h - percent_h);
        ui.p.rect(track, t.full, pal.secondary_container);
        let filled = track.h * value.clamp(0.0, 1.0);
        if filled > 0.5 {
            ui.p.rect(
                Rect::new(
                    track.x,
                    track.bottom() - filled,
                    track.w,
                    filled.max(track.w),
                ),
                t.full,
                colour,
            );
        }
        let cx = x + bar_w / 2.0;
        ui.p.text(
            &format!("{}%", (value * 100.0).round() as u32),
            Rect::new(area.x + column * i as f32, area.y, column, percent_h),
            TextStyle::new(t.font.small).center(),
            pal.on_surface_variant,
        );
        let cell = Rect::new(
            area.x + column * i as f32,
            area.bottom() - icon_h,
            column,
            icon_h,
        );
        ui.p.icon(
            glyph,
            cx,
            cell.y + cell.h / 2.0 + t.spacing.small / 2.0,
            t.icon.medium,
            0.0,
            colour,
        );
        let _ = label;
    }
}

fn media_card(ui: &mut Ui, rect: Rect) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.card(rect, t.rounding.extra_large * 2.0);
    let track = ui.state.media.clone();
    let cover = t.dash_cover.min(rect.w - t.padding.large * 2.0);
    let (cx, _) = rect.center();
    let cy = rect.y + t.padding.large * 1.5 + cover / 2.0;
    // Progress arc around the cover: half a turn, starting bottom-left.
    let ring = cover / 2.0 + t.dash_progress * 1.5;
    let sweep = std::f32::consts::PI;
    let start = std::f32::consts::PI * 1.25;
    ui.p.arc(
        cx,
        cy,
        ring,
        start,
        sweep,
        t.dash_progress,
        pal.secondary_container,
    );
    if let Some(track) = &track
        && let Some(timeline) = track.timeline
    {
        let duration = timeline.duration.as_secs_f32();
        if duration > 0.0 {
            let progress =
                (timeline.position_at(Instant::now()).as_secs_f32() / duration).clamp(0.0, 1.0);
            ui.p.arc(
                cx,
                cy,
                ring,
                start,
                sweep * progress,
                t.dash_progress,
                pal.primary,
            );
        }
    }
    // Lyrics island switch, in the corner: filled while it is on.
    let lyrics_on = ui.config.dashboard.lyrics;
    let switch = t.icon.medium + t.padding.small * 2.0;
    ui.icon_button(
        Rect::new(
            rect.right() - switch - t.padding.medium,
            rect.y + t.padding.medium,
            switch,
            switch,
        ),
        icons::MUSIC_NOTE,
        t.icon.medium,
        if lyrics_on { 1.0 } else { 0.0 },
        lyrics_on.then_some(pal.secondary_container),
        if lyrics_on {
            pal.on_secondary_container
        } else {
            pal.on_surface_variant
        },
        Action::ToggleLyrics,
    );
    let cover_rect = Rect::new(cx - cover / 2.0, cy - cover / 2.0, cover, cover);
    match track.as_ref().and_then(|track| track.artwork.clone()) {
        Some(art) => {
            let key = state::image_key(&format!(
                "cover:{}:{}",
                track.as_ref().map_or("", |t| &t.title),
                art.width
            ));
            ui.p.image(key, &art, cover_rect, cover / 2.0);
        }
        None => {
            ui.p.circle(cx, cy, cover / 2.0, pal.surface_container_high);
            ui.p.icon(
                icons::MUSIC_NOTE,
                cx,
                cy,
                cover * 0.4,
                1.0,
                pal.on_surface_variant,
            );
        }
    }

    let mut y = cy + cover / 2.0 + t.spacing.extra_large;
    let text_w = rect.w - t.padding.large * 2.0;
    let x = rect.x + t.padding.large;
    let s = text();
    let (title, artist) = match &track {
        Some(track) => (
            track.title.clone(),
            if track.artist.is_empty() {
                s.unknown_artist.to_string()
            } else {
                track.artist.clone()
            },
        ),
        None => (s.nothing_playing.to_string(), String::new()),
    };
    ui.p.text(
        &title,
        Rect::new(x, y, text_w, t.font.normal * 1.6),
        TextStyle::new(t.font.normal).weight(500.0).center(),
        pal.primary,
    );
    y += t.font.normal * 1.6;
    ui.p.text(
        &artist,
        Rect::new(x, y, text_w, t.font.normal * 1.4),
        TextStyle::new(t.font.smaller).center(),
        pal.secondary,
    );
    y += t.font.normal * 1.4 + t.spacing.medium;

    // Previous, play/pause, next.
    let button = t.icon.large + t.padding.small * 2.0;
    let playing = track.as_ref().is_some_and(|t| t.playing);
    let row_w = button * 3.0 + t.spacing.small * 2.0;
    let mut bx = cx - row_w / 2.0;
    let controls = [
        (
            icons::SKIP_PREVIOUS,
            Control::Previous,
            track.as_ref().is_some_and(|t| t.previous),
        ),
        (
            if playing {
                icons::PAUSE
            } else {
                icons::PLAY_ARROW
            },
            Control::PlayPause,
            track.as_ref().is_some_and(|t| t.toggle),
        ),
        (
            icons::SKIP_NEXT,
            Control::Next,
            track.as_ref().is_some_and(|t| t.next),
        ),
    ];
    for (glyph, control, enabled) in controls {
        let b = Rect::new(bx, y, button, button);
        let is_play = control == Control::PlayPause;
        let (bg, fg) = if is_play {
            (Some(pal.primary), pal.on_primary)
        } else {
            (None, pal.on_surface)
        };
        if enabled {
            ui.icon_button(b, glyph, t.icon.large, 1.0, bg, fg, Action::Media(control));
        } else {
            let (bcx, bcy) = b.center();
            ui.p.icon(glyph, bcx, bcy, t.icon.large, 1.0, pal.outline);
        }
        bx += button + t.spacing.small;
    }
}

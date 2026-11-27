//! The session menu on the right edge: sign out, shut down, sleep and
//! restart, as a column of large buttons navigable with the keyboard.

use norbanner_core::geom::Rect;
use norbanner_shell::session::Action as SessionAction;

use crate::{
    actions::Action,
    icons,
    ui::{Ctx, Ui},
};

pub const BUTTONS: [(SessionAction, char); 4] = [
    (SessionAction::Logout, icons::LOGOUT),
    (SessionAction::Shutdown, icons::POWER_SETTINGS_NEW),
    (SessionAction::Sleep, icons::BEDTIME),
    (SessionAction::Reboot, icons::CACHED),
];

#[derive(Default)]
pub struct SessionState {
    pub focused: usize,
}

impl SessionState {
    pub fn move_focus(&mut self, delta: i32) {
        self.focused = (self.focused as i32 + delta).rem_euclid(BUTTONS.len() as i32) as usize;
    }
}

pub fn size(ui: &Ctx) -> (f32, f32) {
    let t = ui.t;
    let n = BUTTONS.len() as f32;
    let w = t.session_button + t.padding.large * 2.0;
    let h = n * t.session_button + (n - 1.0) * t.spacing.large + t.padding.large * 2.0;
    (w, h)
}

pub fn draw(ui: &mut Ui, rect: Rect, session: &SessionState) {
    let t = *ui.t;
    let pal = *ui.pal;
    let mut y = rect.y + t.padding.large;
    for (i, (action, glyph)) in BUTTONS.iter().enumerate() {
        let r = Rect::new(
            rect.x + t.padding.large,
            y,
            t.session_button,
            t.session_button,
        );
        let focused = i == session.focused;
        let hovered = ui.hovered(r);
        let (bg, fg) = if focused {
            (pal.secondary_container, pal.on_secondary_container)
        } else {
            (ui.layer, pal.on_surface)
        };
        let radius = if hovered && ui.pressed {
            t.rounding.medium
        } else if focused {
            t.rounding.extra_large
        } else {
            t.rounding.large_increased
        };
        ui.p.rect(r, radius, bg);
        ui.button(r, radius, fg, Action::Session(*action));
        let (cx, cy) = r.center();
        ui.p.icon(
            *glyph,
            cx,
            cy,
            t.icon.large * 1.3,
            if focused { 1.0 } else { 0.0 },
            fg,
        );
        y += t.session_button + t.spacing.large;
    }
}

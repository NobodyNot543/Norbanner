//! Immediate-mode drawing helpers. Modules draw every frame from the
//! current state and register what is clickable as they go; the app
//! matches the pointer against those regions.

use std::time::Instant;

use norbanner_config::Config;
use norbanner_core::geom::Rect;
use norbanner_theme::{Color, Palette};

use crate::{actions::Action, render::Painter, state::SystemState, tokens::Tokens};

/// What dragging or scrolling a region controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slider {
    Volume,
    Brightness(isize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum HitKind {
    Click(Action),
    /// A vertical slider: the value follows the pointer's height.
    VSlider(Slider),
    /// Scrolling over the region sends `Action::Scroll` with this target.
    Scroll(Action),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub kind: HitKind,
}

/// What layout needs without drawing: sizes are computed before the frame.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub t: &'a Tokens,
    pub config: &'a Config,
    pub state: &'a SystemState,
}

pub struct Ui<'a, 'g> {
    pub p: &'a mut Painter<'g>,
    pub pal: &'a Palette,
    pub t: &'a Tokens,
    pub config: &'a Config,
    pub state: &'a SystemState,
    pub hits: &'a mut Vec<Hit>,
    /// The pointer in this screen's logical pixels, if over it.
    pub pointer: Option<(f32, f32)>,
    pub pressed: bool,
    pub now: Instant,
    /// Layer colour for cards inside drawers.
    pub layer: Color,
}

impl Ui<'_, '_> {
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            t: self.t,
            config: self.config,
            state: self.state,
        }
    }

    pub fn hovered(&self, rect: Rect) -> bool {
        self.pointer.is_some_and(|(x, y)| rect.contains(x, y))
    }

    pub fn click(&mut self, rect: Rect, action: Action) {
        self.hits.push(Hit {
            rect,
            kind: HitKind::Click(action),
        });
    }

    pub fn scroll(&mut self, rect: Rect, action: Action) {
        self.hits.push(Hit {
            rect,
            kind: HitKind::Scroll(action),
        });
    }

    pub fn slider(&mut self, rect: Rect, slider: Slider) {
        self.hits.push(Hit {
            rect,
            kind: HitKind::VSlider(slider),
        });
    }

    /// Material state layer over `rect`: `on` at 8 % when hovered, 12 %
    /// when pressed.
    pub fn state_layer(&mut self, rect: Rect, radius: f32, on: Color) -> bool {
        let hovered = self.hovered(rect);
        if hovered {
            let opacity = if self.pressed { 0.12 } else { 0.08 };
            self.p.rect(rect, radius, on.with_alpha(opacity));
        }
        hovered
    }

    /// A clickable area with a state layer.
    pub fn button(&mut self, rect: Rect, radius: f32, on: Color, action: Action) -> bool {
        self.click(rect, action);
        self.state_layer(rect, radius, on)
    }

    /// A round icon button.
    #[allow(clippy::too_many_arguments)]
    pub fn icon_button(
        &mut self,
        rect: Rect,
        glyph: char,
        size: f32,
        fill: f32,
        background: Option<Color>,
        on: Color,
        action: Action,
    ) -> bool {
        let radius = rect.w.min(rect.h) / 2.0;
        if let Some(background) = background {
            self.p.rect(rect, radius, background);
        }
        let hovered = self.button(rect, radius, on, action);
        let (cx, cy) = rect.center();
        self.p.icon(glyph, cx, cy, size, fill, on);
        hovered
    }

    pub fn card(&mut self, rect: Rect, radius: f32) {
        self.p.rect(rect, radius, self.layer);
    }
}

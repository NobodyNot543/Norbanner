//! The shell's modules. The bar is part of the frame; every other module is
//! a drawer that grows out of one edge of the frame.

pub mod bar;
pub mod categories;
pub mod dashboard;
pub mod launcher;
pub mod lyrics;
pub mod osd;
pub mod session;
pub mod window_list;

use std::time::Instant;

use norbanner_core::{
    anim::{Anim, Curve},
    geom::Rect,
    sdf::Blob,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Right,
    /// Out of the bar, toward the screen's middle.
    Left,
}

/// A drawer's open/closed animation.
#[derive(Clone, Copy, Debug)]
pub struct Drawer {
    open: Anim,
    pub wanted: bool,
}

impl Default for Drawer {
    fn default() -> Self {
        Self {
            open: Anim::new(0.0),
            wanted: false,
        }
    }
}

impl Drawer {
    /// Opens or closes. Opening uses the expressive spatial curve, which
    /// overshoots a little so the drawer seems to spring out of the frame.
    pub fn set(&mut self, open: bool, anim_scale: f32, now: Instant) {
        if self.wanted == open {
            return;
        }
        self.wanted = open;
        if open {
            self.open
                .set(1.0, Curve::ExpressiveDefaultSpatial, anim_scale, now);
        } else {
            self.open.set(0.0, Curve::Emphasized, anim_scale * 0.8, now);
        }
    }

    pub fn progress(&self, now: Instant) -> f32 {
        self.open.value(now)
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.open.animating(now)
    }
}

/// Where a drawer is this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Placement {
    /// The content's full rectangle (partly behind the frame while opening).
    pub content: Rect,
    /// The part of the content that shows.
    pub clip: Rect,
    /// The drawer's shape for the surface.
    pub blob: Option<Blob>,
}

/// Places a drawer of natural size `w`×`h` on `edge` of the hole `inner`,
/// centred at `along` (an x for top/bottom, a y for right), open by
/// `progress`. The shape reaches through the frame to the screen edge so it
/// melts into it; fully closed it hides `smoothing` behind the frame's
/// inner edge so it leaves no bump.
#[allow(clippy::too_many_arguments)]
pub fn place(
    inner: Rect,
    edge: Edge,
    along: f32,
    w: f32,
    h: f32,
    progress: f32,
    radius: f32,
    smoothing: f32,
) -> Placement {
    if progress <= 0.001 {
        return Placement::default();
    }
    let reach = 200.0;
    let hide = (1.0 - progress.min(1.0)) * smoothing;
    match edge {
        Edge::Top => {
            let x = (along - w / 2.0).clamp(inner.x, (inner.right() - w).max(inner.x));
            let shown = h * progress;
            let content = Rect::new(x, inner.y - h + shown, w, h);
            Placement {
                content,
                clip: Rect::new(x, inner.y, w, shown.max(0.0)),
                blob: Some(Blob {
                    rect: Rect::new(x, inner.y - reach, w, reach + shown - hide),
                    radius,
                }),
            }
        }
        Edge::Bottom => {
            let x = (along - w / 2.0).clamp(inner.x, (inner.right() - w).max(inner.x));
            let shown = h * progress;
            let content = Rect::new(x, inner.bottom() - shown, w, h);
            Placement {
                content,
                clip: Rect::new(x, inner.bottom() - shown, w, shown.max(0.0)),
                blob: Some(Blob {
                    rect: Rect::new(x, inner.bottom() - shown + hide, w, shown - hide + reach),
                    radius,
                }),
            }
        }
        Edge::Right => {
            let y = (along - h / 2.0).clamp(inner.y, (inner.bottom() - h).max(inner.y));
            let shown = w * progress;
            let content = Rect::new(inner.right() - shown, y, w, h);
            Placement {
                content,
                clip: Rect::new(inner.right() - shown, y, shown.max(0.0), h),
                blob: Some(Blob {
                    rect: Rect::new(inner.right() - shown + hide, y, shown - hide + reach, h),
                    radius,
                }),
            }
        }
        Edge::Left => {
            let y = (along - h / 2.0).clamp(inner.y, (inner.bottom() - h).max(inner.y));
            let shown = w * progress;
            let content = Rect::new(inner.x - w + shown, y, w, h);
            Placement {
                content,
                clip: Rect::new(inner.x, y, shown.max(0.0), h),
                blob: Some(Blob {
                    rect: Rect::new(inner.x - reach, y, reach + shown - hide, h),
                    radius,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_drawers_have_no_shape() {
        let inner = Rect::new(60.0, 10.0, 1000.0, 800.0);
        assert!(
            place(inner, Edge::Top, 500.0, 300.0, 200.0, 0.0, 28.0, 20.0)
                .blob
                .is_none()
        );
    }

    #[test]
    fn open_top_drawer_hangs_from_the_frame() {
        let inner = Rect::new(60.0, 10.0, 1000.0, 800.0);
        let p = place(inner, Edge::Top, 500.0, 300.0, 200.0, 1.0, 28.0, 20.0);
        assert_eq!(p.content.y, 10.0);
        assert_eq!(p.clip.h, 200.0);
        let blob = p.blob.unwrap_or_default();
        assert_eq!(blob.rect.bottom(), 210.0);
        assert!(blob.rect.y < 0.0);
    }

    #[test]
    fn drawers_stay_inside_the_hole_horizontally() {
        let inner = Rect::new(60.0, 10.0, 400.0, 800.0);
        let p = place(inner, Edge::Bottom, 70.0, 300.0, 200.0, 1.0, 28.0, 20.0);
        assert_eq!(p.content.x, 60.0);
    }
}

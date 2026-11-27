//! The shell's surface as a signed distance field.
//!
//! The surface is one continuous shape: a frame around the screen (thick on
//! the bar's side, thin elsewhere) merged with every open drawer through a
//! smooth union, so drawers grow out of the frame like liquid instead of
//! appearing as separate windows. Distance is negative inside the surface.
//!
//! The GPU shader evaluates the same functions per pixel to draw it; the CPU
//! evaluates them at the cursor to decide whether a click belongs to the
//! shell or passes through to the window below. Keep both in step: the HLSL
//! version lives in the shell app's `render/blob.hlsl`.

use crate::geom::Rect;

/// A drawer's shape: a rounded rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blob {
    pub rect: Rect,
    pub radius: f32,
}

/// Everything that makes up a screen's surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// The screen in the window's pixels.
    pub screen: Rect,
    /// The hole inside the frame: where application windows show through.
    pub inner: Rect,
    /// Corner radius of the hole.
    pub inner_radius: f32,
    /// How far apart two shapes start melting together.
    pub smoothing: f32,
    pub blobs: Vec<Blob>,
}

/// Distance from `p` to a rounded box centered at `c` with half size `h`.
#[must_use]
pub fn rounded_box(px: f32, py: f32, rect: &Rect, radius: f32) -> f32 {
    let hw = rect.w / 2.0;
    let hh = rect.h / 2.0;
    let r = radius.min(hw).min(hh).max(0.0);
    let qx = (px - (rect.x + hw)).abs() - hw + r;
    let qy = (py - (rect.y + hh)).abs() - hh + r;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Quadratic smooth minimum: like `min`, but blends the two shapes within
/// distance `k` of each other.
#[must_use]
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

impl Scene {
    /// Signed distance to the surface at `(x, y)`.
    #[must_use]
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        // The frame is everything outside the hole.
        let mut d = if self.inner.w <= 0.0 || self.inner.h <= 0.0 {
            -1.0e4
        } else {
            -rounded_box(x, y, &self.inner, self.inner_radius)
        };
        for blob in &self.blobs {
            if blob.rect.w <= 0.5 || blob.rect.h <= 0.5 {
                continue;
            }
            d = smooth_min(
                d,
                rounded_box(x, y, &blob.rect, blob.radius),
                self.smoothing,
            );
        }
        d
    }

    /// Whether the shell owns the pixel at `(x, y)`; elsewhere input goes
    /// to the windows underneath.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.screen.contains(x, y) && self.distance(x, y) < 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene {
            screen: Rect::new(0.0, 0.0, 1000.0, 800.0),
            inner: Rect::new(60.0, 10.0, 930.0, 780.0),
            inner_radius: 25.0,
            smoothing: 20.0,
            blobs: Vec::new(),
        }
    }

    #[test]
    fn frame_is_solid_and_hole_is_clear() {
        let s = scene();
        assert!(s.contains(5.0, 400.0));
        assert!(s.contains(30.0, 400.0));
        assert!(!s.contains(500.0, 400.0));
        // The hole's corner is rounded: its very corner belongs to the frame.
        assert!(s.contains(62.0, 12.0));
    }

    #[test]
    fn open_drawer_is_solid() {
        let mut s = scene();
        s.blobs.push(Blob {
            rect: Rect::new(400.0, 0.0, 200.0, 150.0),
            radius: 28.0,
        });
        assert!(s.contains(500.0, 100.0));
        assert!(!s.contains(500.0, 300.0));
    }

    #[test]
    fn rounded_box_distance() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(rounded_box(50.0, 50.0, &r, 10.0) < 0.0);
        assert!((rounded_box(150.0, 50.0, &r, 10.0) - 50.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_min_never_exceeds_min() {
        for (a, b) in [(1.0, 2.0), (5.0, 5.0), (-3.0, 4.0)] {
            assert!(smooth_min(a, b, 10.0) <= a.min(b));
        }
    }
}

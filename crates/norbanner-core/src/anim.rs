//! Material 3 motion: easing curves, duration tokens and a time-based
//! animated value.
//!
//! The curves are the public Material Design 3 easing tokens, including the
//! "expressive" spatial curves whose control points overshoot past 1 so a
//! drawer springs slightly beyond its resting place before settling.

use std::time::{Duration, Instant};

/// A cubic Bézier from (0, 0) to (1, 1) with control points `(x1, y1)` and
/// `(x2, y2)`, as in CSS `cubic-bezier()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    #[must_use]
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn sample(a1: f32, a2: f32, t: f32) -> f32 {
        // B(t) for P0 = 0, P3 = 1.
        let u = 1.0 - t;
        3.0 * u * u * t * a1 + 3.0 * u * t * t * a2 + t * t * t
    }

    fn slope(a1: f32, a2: f32, t: f32) -> f32 {
        let u = 1.0 - t;
        3.0 * u * u * a1 + 6.0 * u * t * (a2 - a1) + 3.0 * t * t * (1.0 - a2)
    }

    /// The parameter `t` whose x equals `x`. x(t) is monotonic because the
    /// control points' x lie in [0, 1].
    fn solve_t(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..8 {
            let error = Self::sample(self.x1, self.x2, t) - x;
            if error.abs() < 1e-5 {
                return t;
            }
            let d = Self::slope(self.x1, self.x2, t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= error / d;
        }
        // Newton did not converge (flat slope): bisect.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..32 {
            let value = Self::sample(self.x1, self.x2, t);
            if (value - x).abs() < 1e-5 {
                break;
            }
            if value < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        t
    }

    #[must_use]
    pub fn ease(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        Self::sample(self.y1, self.y2, self.solve_t(x))
    }
}

/// The Material 3 easing tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Curve {
    /// Two-segment path; the default for things entering and changing.
    #[default]
    Emphasized,
    EmphasizedAccel,
    EmphasizedDecel,
    Standard,
    StandardAccel,
    StandardDecel,
    ExpressiveFastSpatial,
    ExpressiveDefaultSpatial,
    ExpressiveSlowSpatial,
    ExpressiveFastEffects,
    ExpressiveDefaultEffects,
    ExpressiveSlowEffects,
    Linear,
}

impl Curve {
    /// Progress (usually 0–1, overshooting for expressive spatial curves)
    /// at linear time `t` in [0, 1].
    #[must_use]
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Emphasized => {
                // M 0,0 C 0.05,0 0.133333,0.06 0.166666,0.4 C 0.208333,0.82 0.25,1 1,1
                const SPLIT_X: f32 = 0.166_666;
                const SPLIT_Y: f32 = 0.4;
                if t < SPLIT_X {
                    let local = t / SPLIT_X;
                    let first =
                        CubicBezier::new(0.05 / SPLIT_X, 0.0, 0.133_333 / SPLIT_X, 0.06 / SPLIT_Y);
                    first.ease(local) * SPLIT_Y
                } else {
                    let span_x = 1.0 - SPLIT_X;
                    let span_y = 1.0 - SPLIT_Y;
                    let local = (t - SPLIT_X) / span_x;
                    let second = CubicBezier::new(
                        (0.208_333 - SPLIT_X) / span_x,
                        (0.82 - SPLIT_Y) / span_y,
                        (0.25 - SPLIT_X) / span_x,
                        1.0,
                    );
                    SPLIT_Y + second.ease(local) * span_y
                }
            }
            Self::Linear => t,
            other => other.bezier().map_or(t, |b| b.ease(t)),
        }
    }

    const fn bezier(self) -> Option<CubicBezier> {
        Some(match self {
            Self::EmphasizedAccel => CubicBezier::new(0.3, 0.0, 0.8, 0.15),
            Self::EmphasizedDecel => CubicBezier::new(0.05, 0.7, 0.1, 1.0),
            Self::Standard => CubicBezier::new(0.2, 0.0, 0.0, 1.0),
            Self::StandardAccel => CubicBezier::new(0.3, 0.0, 1.0, 1.0),
            Self::StandardDecel => CubicBezier::new(0.0, 0.0, 0.0, 1.0),
            Self::ExpressiveFastSpatial => CubicBezier::new(0.42, 1.67, 0.21, 0.9),
            Self::ExpressiveDefaultSpatial => CubicBezier::new(0.38, 1.21, 0.22, 1.0),
            Self::ExpressiveSlowSpatial => CubicBezier::new(0.39, 1.29, 0.35, 0.98),
            Self::ExpressiveFastEffects => CubicBezier::new(0.31, 0.94, 0.34, 1.0),
            Self::ExpressiveDefaultEffects => CubicBezier::new(0.34, 0.8, 0.34, 1.0),
            Self::ExpressiveSlowEffects => CubicBezier::new(0.34, 0.88, 0.34, 1.0),
            Self::Emphasized | Self::Linear => return None,
        })
    }

    /// The token duration that goes with this curve, before the user's
    /// animation speed scale.
    #[must_use]
    pub const fn duration_ms(self) -> u32 {
        match self {
            Self::ExpressiveFastSpatial => 350,
            Self::ExpressiveDefaultSpatial => 500,
            Self::ExpressiveSlowSpatial => 650,
            Self::ExpressiveFastEffects => 150,
            Self::ExpressiveDefaultEffects => 200,
            Self::ExpressiveSlowEffects => 300,
            Self::Emphasized | Self::EmphasizedDecel | Self::EmphasizedAccel => 500,
            Self::Standard | Self::StandardDecel | Self::StandardAccel | Self::Linear => 400,
        }
    }
}

/// A value that moves to its target over time along a [`Curve`].
///
/// Retargeting mid-flight starts the new transition from the current
/// on-screen value, so interrupted drawers never jump.
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    curve: Curve,
}

impl Anim {
    #[must_use]
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            duration: Duration::ZERO,
            curve: Curve::Emphasized,
        }
    }

    #[must_use]
    pub fn target(&self) -> f32 {
        self.to
    }

    /// Moves toward `to`. `scale` multiplies the curve's token duration
    /// (the user's animation speed; 0 jumps immediately).
    pub fn set(&mut self, to: f32, curve: Curve, scale: f32, now: Instant) {
        if (self.to - to).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.curve = curve;
        let ms = curve.duration_ms() as f32 * scale.max(0.0);
        self.duration = Duration::from_secs_f32(ms / 1000.0);
    }

    /// Jumps to `value` without animating.
    pub fn snap(&mut self, value: f32) {
        self.from = value;
        self.to = value;
        self.duration = Duration::ZERO;
    }

    #[must_use]
    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t =
            now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32();
        if t >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * self.curve.ease(t)
    }

    #[must_use]
    pub fn animating(&self, now: Instant) -> bool {
        !self.duration.is_zero() && now.saturating_duration_since(self.start) < self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_at_zero_and_end_at_one() {
        for curve in [
            Curve::Emphasized,
            Curve::EmphasizedDecel,
            Curve::Standard,
            Curve::ExpressiveDefaultSpatial,
            Curve::ExpressiveFastEffects,
        ] {
            assert!(curve.ease(0.0).abs() < 1e-4, "{curve:?}");
            assert!((curve.ease(1.0) - 1.0).abs() < 1e-4, "{curve:?}");
        }
    }

    #[test]
    fn emphasized_is_continuous_at_the_split() {
        let before = Curve::Emphasized.ease(0.1666);
        let after = Curve::Emphasized.ease(0.1667);
        assert!((before - after).abs() < 0.01);
    }

    #[test]
    fn expressive_spatial_overshoots() {
        let peak = (0..100)
            .map(|i| Curve::ExpressiveDefaultSpatial.ease(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.0);
    }

    #[test]
    fn retarget_continues_from_current_value() {
        let start = Instant::now();
        let mut anim = Anim::new(0.0);
        anim.set(1.0, Curve::Standard, 1.0, start);
        let mid = start + Duration::from_millis(200);
        let value = anim.value(mid);
        anim.set(0.0, Curve::Standard, 1.0, mid);
        assert!((anim.value(mid) - value).abs() < 1e-5);
    }

    #[test]
    fn zero_scale_jumps() {
        let now = Instant::now();
        let mut anim = Anim::new(0.0);
        anim.set(5.0, Curve::Emphasized, 0.0, now);
        assert_eq!(anim.value(now), 5.0);
        assert!(!anim.animating(now));
    }
}

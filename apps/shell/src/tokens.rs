//! Design tokens: the Material 3 scale of corner radii, spacing, type sizes
//! and component sizes the shell is laid out with, in logical pixels, after
//! the user's scale factors from `appearance`.

use norbanner_config::Config;

#[derive(Clone, Copy, Debug)]
pub struct Steps {
    pub extra_small: f32,
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub large_increased: f32,
    pub extra_large: f32,
}

impl Steps {
    fn scaled(by: f32) -> Self {
        Self {
            extra_small: 4.0 * by,
            small: 8.0 * by,
            medium: 12.0 * by,
            large: 16.0 * by,
            large_increased: 20.0 * by,
            extra_large: 28.0 * by,
        }
    }
}

/// Type sizes in DIPs (Qt points × 4/3, as the original design is in points).
#[derive(Clone, Copy, Debug)]
pub struct FontSizes {
    pub small: f32,
    pub smaller: f32,
    pub normal: f32,
    pub large: f32,
    pub extra_large: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct IconSizes {
    pub small: f32,
    pub medium: f32,
    pub large: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub rounding: Steps,
    pub spacing: Steps,
    pub padding: Steps,
    pub font: FontSizes,
    pub icon: IconSizes,
    pub full: f32,
    // Components.
    pub bar_inner: f32,
    pub dash_date_width: f32,
    pub dash_media_width: f32,
    pub dash_cover: f32,
    pub dash_progress: f32,
    pub osd_slider_width: f32,
    pub osd_slider_height: f32,
    pub session_button: f32,
}

/// How much to shrink the design on a screen Windows scales by `scale`.
///
/// Windows' scale makes text readable from a laptop's distance, but scaled
/// fully the shell's chrome takes a large share of a small screen. Up to
/// 125 % the design is used as is; beyond, it grows only part of the way
/// (150 % → about 1.25× the 100 % size), never below 70 %.
#[must_use]
pub fn density(scale: f32) -> f32 {
    (1.25 / scale.max(0.5)).clamp(0.7, 1.0)
}

impl Tokens {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self::with_density(config, 1.0)
    }

    /// Tokens for a screen Windows scales by `scale`.
    #[must_use]
    pub fn for_scale(config: &Config, scale: f32) -> Self {
        Self::with_density(config, density(scale))
    }

    /// The dashboard's tokens: the screen's, times the user's size for it.
    #[must_use]
    pub fn for_dashboard(config: &Config, scale: f32) -> Self {
        Self::with_density(config, density(scale) * config.dashboard.scale)
    }

    #[must_use]
    pub fn with_density(config: &Config, d: f32) -> Self {
        let a = &config.appearance;
        let font = a.font.size.scale * 4.0 / 3.0 * d;
        Self {
            rounding: Steps::scaled(a.rounding.scale * d),
            spacing: Steps::scaled(a.spacing.scale * d),
            padding: Steps::scaled(a.padding.scale * d),
            font: FontSizes {
                small: 11.0 * font,
                smaller: 12.0 * font,
                normal: 13.0 * font,
                large: 18.0 * font,
                extra_large: 28.0 * font,
            },
            icon: IconSizes {
                small: 18.0 * font,
                medium: 20.0 * font,
                large: 24.0 * font,
            },
            full: 10_000.0,
            bar_inner: 44.0 * d,
            dash_date_width: 110.0 * d,
            dash_media_width: 200.0 * d,
            dash_cover: 150.0 * d,
            dash_progress: 6.0 * d,
            osd_slider_width: 14.0 * d,
            osd_slider_height: 120.0 * d,
            session_button: 80.0 * d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_dpi_screens_grow_only_part_of_the_way() {
        assert_eq!(density(1.0), 1.0);
        assert_eq!(density(1.25), 1.0);
        assert!((density(1.5) * 1.5 - 1.25).abs() < 1e-4);
        assert_eq!(density(3.0), 0.7);
    }
}

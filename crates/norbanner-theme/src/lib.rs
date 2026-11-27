//! Material You colours for the shell.
//!
//! A source colour (picked from the wallpaper, Windows' accent colour or a
//! fixed value) becomes a full Material 3 scheme: surfaces, containers and
//! the primary/secondary/tertiary roles with their "on" colours. Everything
//! the shell draws picks a role from [`Palette`], never a literal colour, so
//! changing the wallpaper recolours the whole shell.

use material_colors::{
    color::Rgb, dynamic_color::Variant as McVariant, image::extract_color, scheme::Scheme,
    theme::ThemeBuilder,
};
use serde::{Deserialize, Serialize};

/// A straight (non-premultiplied) RGBA colour, each channel in 0–1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    #[must_use]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    #[must_use]
    pub fn from_u32(rgb: u32) -> Self {
        Self::rgba(
            ((rgb >> 16) & 0xff) as f32 / 255.0,
            ((rgb >> 8) & 0xff) as f32 / 255.0,
            (rgb & 0xff) as f32 / 255.0,
            1.0,
        )
    }

    fn from_rgb(rgb: Rgb) -> Self {
        Self::rgba(
            f32::from(rgb.red) / 255.0,
            f32::from(rgb.green) / 255.0,
            f32::from(rgb.blue) / 255.0,
            1.0,
        )
    }

    #[must_use]
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Mixes `amount` of `other` into this colour.
    #[must_use]
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let l = |a: f32, b: f32| a + (b - a) * amount;
        Self::rgba(
            l(self.r, other.r),
            l(self.g, other.g),
            l(self.b, other.b),
            l(self.a, other.a),
        )
    }

    /// Material state layer: `layer` drawn over this colour at `opacity`.
    #[must_use]
    pub fn state(self, layer: Self, opacity: f32) -> Self {
        let a = self.a;
        self.mix(layer.with_alpha(a), opacity)
    }

    #[must_use]
    pub fn hex(self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }
}

/// The Material 3 colour roles the shell uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Palette {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub surface: Color,
    pub surface_dim: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,
    pub shadow: Color,
    pub scrim: Color,
}

impl Palette {
    fn from_scheme(s: &Scheme) -> Self {
        let c = Color::from_rgb;
        Self {
            primary: c(s.primary),
            on_primary: c(s.on_primary),
            primary_container: c(s.primary_container),
            on_primary_container: c(s.on_primary_container),
            secondary: c(s.secondary),
            on_secondary: c(s.on_secondary),
            secondary_container: c(s.secondary_container),
            on_secondary_container: c(s.on_secondary_container),
            tertiary: c(s.tertiary),
            on_tertiary: c(s.on_tertiary),
            tertiary_container: c(s.tertiary_container),
            on_tertiary_container: c(s.on_tertiary_container),
            error: c(s.error),
            on_error: c(s.on_error),
            error_container: c(s.error_container),
            on_error_container: c(s.on_error_container),
            surface: c(s.surface),
            surface_dim: c(s.surface_dim),
            surface_bright: c(s.surface_bright),
            surface_container_lowest: c(s.surface_container_lowest),
            surface_container_low: c(s.surface_container_low),
            surface_container: c(s.surface_container),
            surface_container_high: c(s.surface_container_high),
            surface_container_highest: c(s.surface_container_highest),
            on_surface: c(s.on_surface),
            on_surface_variant: c(s.on_surface_variant),
            outline: c(s.outline),
            outline_variant: c(s.outline_variant),
            inverse_surface: c(s.inverse_surface),
            inverse_on_surface: c(s.inverse_on_surface),
            inverse_primary: c(s.inverse_primary),
            shadow: c(s.shadow),
            scrim: c(s.scrim),
        }
    }
}

/// How the scheme spreads the source colour over the roles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Variant {
    #[default]
    TonalSpot,
    Vibrant,
    Expressive,
    Neutral,
    Monochrome,
    Fidelity,
    Content,
    Rainbow,
    FruitSalad,
}

impl Variant {
    pub const ALL: [Self; 9] = [
        Self::TonalSpot,
        Self::Vibrant,
        Self::Expressive,
        Self::Neutral,
        Self::Monochrome,
        Self::Fidelity,
        Self::Content,
        Self::Rainbow,
        Self::FruitSalad,
    ];

    const fn material(self) -> McVariant {
        match self {
            Self::TonalSpot => McVariant::TonalSpot,
            Self::Vibrant => McVariant::Vibrant,
            Self::Expressive => McVariant::Expressive,
            Self::Neutral => McVariant::Neutral,
            Self::Monochrome => McVariant::Monochrome,
            Self::Fidelity => McVariant::Fidelity,
            Self::Content => McVariant::Content,
            Self::Rainbow => McVariant::Rainbow,
            Self::FruitSalad => McVariant::FruitSalad,
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::TonalSpot => "tonalSpot",
            Self::Vibrant => "vibrant",
            Self::Expressive => "expressive",
            Self::Neutral => "neutral",
            Self::Monochrome => "monochrome",
            Self::Fidelity => "fidelity",
            Self::Content => "content",
            Self::Rainbow => "rainbow",
            Self::FruitSalad => "fruitSalad",
        }
    }
}

/// A resolved colour theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub source: Color,
    pub dark: bool,
    pub variant: Variant,
    pub palette: Palette,
}

impl Theme {
    /// Builds the scheme for `source` (0xRRGGBB).
    #[must_use]
    pub fn from_source(source: u32, dark: bool, variant: Variant) -> Self {
        let theme = ThemeBuilder::with_source(Rgb::from_u32(source))
            .variant(variant.material())
            .build();
        let scheme = if dark {
            &theme.schemes.dark
        } else {
            &theme.schemes.light
        };
        Self {
            source: Color::from_u32(source),
            dark,
            variant,
            palette: Palette::from_scheme(scheme),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        // Caelestia's signature blue, used until a wallpaper is read.
        Self::from_source(DEFAULT_SOURCE, true, Variant::TonalSpot)
    }
}

pub const DEFAULT_SOURCE: u32 = 0x9c_cb_fb;

/// The most suitable source colour among `pixels` (0xRRGGBB each).
/// Downscale the image to about 128×128 first: quantising takes time in
/// proportion to the pixel count.
#[must_use]
pub fn source_from_pixels(pixels: &[u32]) -> Option<u32> {
    if pixels.is_empty() {
        return None;
    }
    let rgb: Vec<Rgb> = pixels.iter().map(|&p| Rgb::from_u32(p)).collect();
    Some(extract_color(&rgb).as_u32())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_scheme_has_dark_surface_and_light_text() {
        let theme = Theme::from_source(0x4285f4, true, Variant::TonalSpot);
        let p = theme.palette;
        assert!(p.surface.r + p.surface.g + p.surface.b < 0.6);
        assert!(p.on_surface.r + p.on_surface.g + p.on_surface.b > 2.0);
    }

    #[test]
    fn light_scheme_is_light() {
        let theme = Theme::from_source(0x4285f4, false, Variant::Vibrant);
        let p = theme.palette;
        assert!(p.surface.r + p.surface.g + p.surface.b > 2.4);
    }

    #[test]
    fn picks_dominant_colour() {
        let mut pixels = vec![0x20_40_d0u32; 900];
        pixels.extend(std::iter::repeat_n(0xff_ff_ffu32, 100));
        let source = source_from_pixels(&pixels).unwrap_or_default();
        let blue = source & 0xff;
        let red = source >> 16;
        assert!(blue > red, "{source:06x}");
    }
}

//! Direct2D drawing in logical pixels, with the caches that make it cheap
//! to redraw a whole screen every animation frame.

use std::{cell::RefCell, collections::HashMap};

use norbanner_core::geom::Rect;
use norbanner_shell::icons::IconImage;
use norbanner_theme::Color;
use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{
                D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
            },
            D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE,
            D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1,
            D2D1_CAP_STYLE_ROUND, D2D1_DRAW_TEXT_OPTIONS_CLIP,
            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_ELLIPSE,
            D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_ROUNDED_RECT,
            D2D1_STROKE_STYLE_PROPERTIES1, D2D1_SWEEP_DIRECTION_CLOCKWISE, ID2D1Bitmap1,
            ID2D1BitmapBrush1, ID2D1SolidColorBrush, ID2D1StrokeStyle,
        },
        DirectWrite::{
            DWRITE_FONT_AXIS_VALUE, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
            DWRITE_WORD_WRAPPING_WRAP, IDWriteTextFormat, IDWriteTextLayout,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    },
    core::Interface,
};
use windows_numerics::{Matrix3x2, Vector2};

use windows::Win32::Graphics::Direct2D::{
    Common::D2D1_GRADIENT_STOP, D2D1_BUFFER_PRECISION_8BPC_UNORM,
    D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED, D2D1_COLOR_SPACE_SRGB, D2D1_EXTEND_MODE_CLAMP,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, ID2D1Brush, ID2D1LinearGradientBrush,
};

use super::{Gpu, fonts};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: f32,
    pub mono: bool,
    pub align: Align,
    pub wrap: bool,
}

impl TextStyle {
    #[must_use]
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            weight: 400.0,
            mono: false,
            align: Align::Start,
            wrap: false,
        }
    }

    #[must_use]
    pub const fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    #[must_use]
    pub const fn center(mut self) -> Self {
        self.align = Align::Center;
        self
    }

    /// Wraps onto as many lines as the rect holds, from the top, the last
    /// one trimmed with an ellipsis.
    #[must_use]
    pub const fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    #[must_use]
    pub const fn mono(mut self) -> Self {
        self.mono = true;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Family {
    Sans,
    Mono,
    Icon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FormatKey {
    family: Family,
    /// Tenths of a DIP.
    size: u32,
    weight: u16,
    /// Percent.
    fill: u8,
}

/// Caches kept across frames.
pub struct Cache {
    pub sans: String,
    pub mono: String,
    formats: HashMap<FormatKey, IDWriteTextFormat>,
    images: HashMap<u64, ID2D1Bitmap1>,
    brush: Option<ID2D1SolidColorBrush>,
    round: Option<ID2D1StrokeStyle>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            sans: "Rubik".into(),
            mono: "Cascadia Code".into(),
            formats: HashMap::new(),
            images: HashMap::new(),
            brush: None,
            round: None,
        }
    }
}

impl Cache {
    pub fn clear_fonts(&mut self) {
        self.formats.clear();
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Runs `f` with the UI thread's paint cache.
pub fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    CACHE.with(|cache| f(&mut cache.borrow_mut()))
}

fn color(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}

fn rounded(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    D2D1_ROUNDED_RECT {
        rect: d2d_rect(r),
        radiusX: radius,
        radiusY: radius,
    }
}

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub struct Painter<'a> {
    gpu: &'a Gpu,
    transforms: Vec<Matrix3x2>,
    clips: usize,
    pub opacity: f32,
}

impl<'a> Painter<'a> {
    pub(super) fn new(gpu: &'a Gpu, _dpi: f32) -> Self {
        // SAFETY: Resets the device context's transform for this frame.
        unsafe { gpu.dc.SetTransform(&Matrix3x2::identity()) };
        Self {
            gpu,
            transforms: Vec::new(),
            clips: 0,
            opacity: 1.0,
        }
    }

    fn brush(&self, c: Color) -> Option<ID2D1SolidColorBrush> {
        let c = c.with_alpha(c.a * self.opacity);
        with_cache(|cache| {
            if cache.brush.is_none() {
                // SAFETY: Creates a brush on this device context.
                cache.brush = unsafe { self.gpu.dc.CreateSolidColorBrush(&color(c), None) }.ok();
            }
            let brush = cache.brush.clone()?;
            // SAFETY: Recolours the shared brush before its single use.
            unsafe { brush.SetColor(&color(c)) };
            Some(brush)
        })
    }

    fn round_stroke(&self) -> Option<ID2D1StrokeStyle> {
        with_cache(|cache| {
            if cache.round.is_none() {
                // SAFETY: Plain factory call.
                cache.round = unsafe {
                    self.gpu.d2d_factory.CreateStrokeStyle(
                        &D2D1_STROKE_STYLE_PROPERTIES1 {
                            startCap: D2D1_CAP_STYLE_ROUND,
                            endCap: D2D1_CAP_STYLE_ROUND,
                            dashCap: D2D1_CAP_STYLE_ROUND,
                            ..Default::default()
                        },
                        None,
                    )
                }
                .ok()
                .and_then(|s| s.cast().ok());
            }
            cache.round.clone()
        })
    }

    pub fn rect(&mut self, r: Rect, radius: f32, c: Color) {
        if c.a <= 0.0 || r.w <= 0.0 || r.h <= 0.0 {
            return;
        }
        let Some(brush) = self.brush(c) else { return };
        // SAFETY: Drawing between BeginDraw and EndDraw.
        unsafe {
            if radius <= 0.0 {
                self.gpu.dc.FillRectangle(&d2d_rect(r), &brush);
            } else {
                self.gpu
                    .dc
                    .FillRoundedRectangle(&rounded(r, radius), &brush);
            }
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, c: Color) {
        if c.a <= 0.0 || radius <= 0.0 {
            return;
        }
        let Some(brush) = self.brush(c) else { return };
        // SAFETY: As in `rect`.
        unsafe {
            self.gpu.dc.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 { X: cx, Y: cy },
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// An arc around `(cx, cy)` starting at `start` radians (0 = 12 o'clock,
    /// clockwise) and spanning `sweep` radians, with round caps.
    pub fn arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start: f32,
        sweep: f32,
        width: f32,
        c: Color,
    ) {
        if sweep.abs() < 0.001 || c.a <= 0.0 {
            return;
        }
        if sweep.abs() >= std::f32::consts::TAU - 0.001 {
            let Some(brush) = self.brush(c) else { return };
            // SAFETY: As in `rect`.
            unsafe {
                self.gpu.dc.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 { X: cx, Y: cy },
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                    width,
                    None,
                );
            }
            return;
        }
        let point = |angle: f32| Vector2 {
            X: cx + radius * angle.sin(),
            Y: cy - radius * angle.cos(),
        };
        let Some(brush) = self.brush(c) else { return };
        let style = self.round_stroke();
        // SAFETY: Geometry is built and drawn within this frame.
        unsafe {
            let Ok(path) = self.gpu.d2d_factory.CreatePathGeometry() else {
                return;
            };
            let Ok(sink) = path.Open() else { return };
            sink.BeginFigure(point(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(start + sweep),
                size: D2D_SIZE_F {
                    width: radius,
                    height: radius,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > std::f32::consts::PI {
                    D2D1_ARC_SIZE_LARGE
                } else {
                    D2D1_ARC_SIZE_SMALL
                },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_err() {
                return;
            }
            self.gpu
                .dc
                .DrawGeometry(&path, &brush, width, style.as_ref());
        }
    }

    fn format(
        &self,
        family: Family,
        size: f32,
        weight: f32,
        fill: f32,
    ) -> Option<IDWriteTextFormat> {
        let key = FormatKey {
            family,
            size: (size * 10.0).round() as u32,
            weight: weight.round() as u16,
            fill: (fill * 100.0).round() as u8,
        };
        with_cache(|cache| {
            if let Some(format) = cache.formats.get(&key) {
                return Some(format.clone());
            }
            let name = match family {
                Family::Sans => cache.sans.clone(),
                Family::Mono => cache.mono.clone(),
                Family::Icon => fonts::ICON_FAMILY.to_string(),
            };
            let mut axes = vec![DWRITE_FONT_AXIS_VALUE {
                axisTag: fonts::WEIGHT,
                value: weight,
            }];
            if family == Family::Icon {
                axes.push(DWRITE_FONT_AXIS_VALUE {
                    axisTag: fonts::FILL,
                    value: fill,
                });
            }
            let format: IDWriteTextFormat = self
                .gpu
                .fonts
                .format(&self.gpu.dwrite, &name, size, &axes)
                .ok()?
                .cast()
                .ok()?;
            cache.formats.insert(key, format.clone());
            Some(format)
        })
    }

    fn layout(
        &self,
        text: &str,
        style: TextStyle,
        max_w: f32,
        max_h: f32,
    ) -> Option<IDWriteTextLayout> {
        let family = if style.mono {
            Family::Mono
        } else {
            Family::Sans
        };
        let format = self.format(family, style.size, style.weight, 0.0)?;
        // SAFETY: Plain factory calls on owned objects.
        unsafe {
            let layout = self
                .gpu
                .dwrite
                .CreateTextLayout(&utf16(text), &format, max_w.max(1.0), max_h.max(1.0))
                .ok()?;
            let align: DWRITE_TEXT_ALIGNMENT = match style.align {
                Align::Start => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            };
            let _ = layout.SetTextAlignment(align);
            let _ = layout.SetWordWrapping(if style.wrap {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            });
            if let Ok(sign) = self.gpu.dwrite.CreateEllipsisTrimmingSign(&format) {
                let _ = layout.SetTrimming(
                    &DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        ..Default::default()
                    },
                    &sign,
                );
            }
            Some(layout)
        }
    }

    /// Width and height `text` takes in `style`, unconstrained.
    pub fn measure(&mut self, text: &str, style: TextStyle) -> (f32, f32) {
        let Some(layout) = self.layout(
            text,
            TextStyle {
                wrap: false,
                ..style
            },
            8192.0,
            2048.0,
        ) else {
            return (0.0, style.size * 1.2);
        };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: Out-pointer is a valid local.
        if unsafe { layout.GetMetrics(&mut metrics) }.is_err() {
            return (0.0, style.size * 1.2);
        }
        (metrics.widthIncludingTrailingWhitespace, metrics.height)
    }

    /// Draws `text` in `rect`, vertically centred, aligned horizontally per
    /// `style`, ellipsised if too wide.
    pub fn text(&mut self, text: &str, rect: Rect, style: TextStyle, c: Color) {
        if text.is_empty() || c.a <= 0.0 {
            return;
        }
        let Some(layout) = self.layout(text, style, rect.w, rect.h) else {
            return;
        };
        let Some(brush) = self.brush(c) else { return };
        // SAFETY: Drawing between BeginDraw and EndDraw.
        unsafe {
            let _ = layout.SetParagraphAlignment(if style.wrap {
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR
            } else {
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER
            });
            self.gpu.dc.DrawTextLayout(
                Vector2 {
                    X: rect.x,
                    Y: rect.y,
                },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
    }

    /// A Material Symbols glyph centred on `(cx, cy)`. `fill` 0–1 morphs
    /// between the outlined and filled styles.
    pub fn icon(&mut self, glyph: char, cx: f32, cy: f32, size: f32, fill: f32, c: Color) {
        self.icon_weighted(glyph, cx, cy, size, fill, 400.0, c);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn icon_weighted(
        &mut self,
        glyph: char,
        cx: f32,
        cy: f32,
        size: f32,
        fill: f32,
        weight: f32,
        c: Color,
    ) {
        if c.a <= 0.0 {
            return;
        }
        let Some(format) = self.format(Family::Icon, size, weight, fill.clamp(0.0, 1.0)) else {
            return;
        };
        let Some(brush) = self.brush(c) else { return };
        let mut buffer = [0u16; 2];
        let text = glyph.encode_utf16(&mut buffer);
        let box_size = size * 2.0;
        // SAFETY: As in `text`.
        unsafe {
            let Ok(layout) = self
                .gpu
                .dwrite
                .CreateTextLayout(text, &format, box_size, box_size)
            else {
                return;
            };
            let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            self.gpu.dc.DrawTextLayout(
                Vector2 {
                    X: cx - box_size / 2.0,
                    Y: cy - box_size / 2.0,
                },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
        }
    }

    fn bitmap(&self, key: u64, image: &IconImage) -> Option<ID2D1Bitmap1> {
        with_cache(|cache| {
            if let Some(bitmap) = cache.images.get(&key) {
                return Some(bitmap.clone());
            }
            if image.width == 0 || image.height == 0 {
                return None;
            }
            // SAFETY: `bgra` holds width*height*4 premultiplied bytes.
            let bitmap = unsafe {
                self.gpu.dc.CreateBitmap(
                    D2D_SIZE_U {
                        width: image.width,
                        height: image.height,
                    },
                    Some(image.bgra.as_ptr().cast()),
                    image.width * 4,
                    &D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                        ..Default::default()
                    },
                )
            }
            .ok()?;
            cache.images.insert(key, bitmap.clone());
            Some(bitmap)
        })
    }

    /// Draws `image` covering `rect` (cropping to keep its aspect), with
    /// rounded corners. `key` identifies the image in the cache.
    pub fn image(&mut self, key: u64, image: &IconImage, rect: Rect, radius: f32) {
        let Some(bitmap) = self.bitmap(key, image) else {
            return;
        };
        let (iw, ih) = (image.width as f32, image.height as f32);
        let scale = (rect.w / iw).max(rect.h / ih);
        let (dw, dh) = (iw * scale, ih * scale);
        let ox = rect.x + (rect.w - dw) / 2.0;
        let oy = rect.y + (rect.h - dh) / 2.0;
        // SAFETY: Brush and bitmap belong to this device context.
        unsafe {
            let Ok(brush): windows::core::Result<ID2D1BitmapBrush1> =
                self.gpu.dc.CreateBitmapBrush(&bitmap, None, None)
            else {
                return;
            };
            brush.SetInterpolationMode1(D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC);
            brush.SetOpacity(self.opacity);
            brush.SetTransform(&(Matrix3x2::scale(scale, scale) * Matrix3x2::translation(ox, oy)));
            self.gpu
                .dc
                .FillRoundedRectangle(&rounded(rect, radius), &brush);
        }
    }

    /// Draws `image` fitted inside `rect` without cropping (app icons).
    pub fn image_fit(&mut self, key: u64, image: &IconImage, rect: Rect) {
        let Some(bitmap) = self.bitmap(key, image) else {
            return;
        };
        let (iw, ih) = (image.width as f32, image.height as f32);
        let scale = (rect.w / iw).min(rect.h / ih);
        let (dw, dh) = (iw * scale, ih * scale);
        let dest = Rect::new(
            rect.x + (rect.w - dw) / 2.0,
            rect.y + (rect.h - dh) / 2.0,
            dw,
            dh,
        );
        // SAFETY: As in `image`.
        unsafe {
            self.gpu.dc.DrawBitmap(
                &bitmap,
                Some(&d2d_rect(dest)),
                self.opacity,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            );
        }
    }

    /// A linear gradient brush from `from` at `(x1, y1)` to `to` at `(x2, y2)`.
    fn gradient(
        &self,
        (x1, y1): (f32, f32),
        (x2, y2): (f32, f32),
        from: Color,
        to: Color,
    ) -> Option<ID2D1LinearGradientBrush> {
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: color(from),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: color(to),
            },
        ];
        // SAFETY: Creates brush resources on this device context for one draw.
        unsafe {
            let collection = self
                .gpu
                .dc
                .CreateGradientStopCollection(
                    &stops,
                    D2D1_COLOR_SPACE_SRGB,
                    D2D1_COLOR_SPACE_SRGB,
                    D2D1_BUFFER_PRECISION_8BPC_UNORM,
                    D2D1_EXTEND_MODE_CLAMP,
                    D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED,
                )
                .ok()?;
            let brush = self
                .gpu
                .dc
                .CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 { X: x1, Y: y1 },
                        endPoint: Vector2 { X: x2, Y: y2 },
                    },
                    None,
                    &collection,
                )
                .ok()?;
            brush.SetOpacity(self.opacity);
            Some(brush)
        }
    }

    /// A rounded rectangle filled with a diagonal gradient (top-left `from`,
    /// bottom-right `to`).
    pub fn rect_gradient(&mut self, r: Rect, radius: f32, from: Color, to: Color) {
        if r.w <= 0.0 || r.h <= 0.0 {
            return;
        }
        let Some(brush) = self.gradient((r.x, r.y), (r.right(), r.bottom()), from, to) else {
            return;
        };
        let brush: ID2D1Brush = brush.into();
        // SAFETY: Drawing between BeginDraw and EndDraw.
        unsafe {
            self.gpu
                .dc
                .FillRoundedRectangle(&rounded(r, radius), &brush)
        };
    }

    /// A soft glow around `r`: stacked, fading outlines.
    pub fn glow(&mut self, r: Rect, radius: f32, c: Color, spread: f32) {
        const STEPS: usize = 7;
        for i in (1..=STEPS).rev() {
            let k = i as f32 / STEPS as f32;
            let grow = spread * k;
            let alpha = c.a * (1.0 - k).powi(2) * 0.35;
            self.rect(
                Rect::new(r.x - grow, r.y - grow, r.w + grow * 2.0, r.h + grow * 2.0),
                radius + grow,
                c.with_alpha(alpha),
            );
        }
    }

    /// Text filled with a vertical gradient.
    pub fn text_gradient(
        &mut self,
        text: &str,
        rect: Rect,
        style: TextStyle,
        from: Color,
        to: Color,
    ) {
        if text.is_empty() {
            return;
        }
        let Some(layout) = self.layout(text, style, rect.w, rect.h) else {
            return;
        };
        let Some(brush) = self.gradient((rect.x, rect.y), (rect.x, rect.bottom()), from, to) else {
            return;
        };
        let brush: ID2D1Brush = brush.into();
        // SAFETY: Drawing between BeginDraw and EndDraw.
        unsafe {
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            self.gpu.dc.DrawTextLayout(
                Vector2 {
                    X: rect.x,
                    Y: rect.y,
                },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
    }

    pub fn clip(&mut self, r: Rect) {
        // SAFETY: Balanced by `unclip`.
        unsafe {
            self.gpu
                .dc
                .PushAxisAlignedClip(&d2d_rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        self.clips += 1;
    }

    pub fn unclip(&mut self) {
        if self.clips > 0 {
            // SAFETY: Pops a clip pushed by `clip`.
            unsafe { self.gpu.dc.PopAxisAlignedClip() };
            self.clips -= 1;
        }
    }

    fn current(&self) -> Matrix3x2 {
        self.transforms
            .last()
            .copied()
            .unwrap_or_else(Matrix3x2::identity)
    }

    fn apply(&self) {
        // SAFETY: Sets the transform for subsequent drawing.
        unsafe { self.gpu.dc.SetTransform(&self.current()) };
    }

    /// Draws `f` rotated by `degrees` around `(cx, cy)`.
    pub fn rotated(&mut self, degrees: f32, cx: f32, cy: f32, f: impl FnOnce(&mut Self)) {
        let next = Matrix3x2::translation(-cx, -cy)
            * Matrix3x2::rotation(degrees)
            * Matrix3x2::translation(cx, cy)
            * self.current();
        self.transforms.push(next);
        self.apply();
        f(self);
        self.transforms.pop();
        self.apply();
    }

    /// Draws `f` at `opacity` (multiplied with the current opacity).
    pub fn faded(&mut self, opacity: f32, f: impl FnOnce(&mut Self)) {
        let before = self.opacity;
        self.opacity *= opacity.clamp(0.0, 1.0);
        if self.opacity > 0.003 {
            f(self);
        }
        self.opacity = before;
    }
}

impl Drop for Painter<'_> {
    fn drop(&mut self) {
        while self.clips > 0 {
            self.unclip();
        }
        // SAFETY: Leaves the context untransformed for the next frame.
        unsafe { self.gpu.dc.SetTransform(&Matrix3x2::identity()) };
    }
}

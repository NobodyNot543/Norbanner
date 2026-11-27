//! Rectangles in logical or physical pixels.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    #[must_use]
    pub fn inset(&self, by: f32) -> Self {
        Self::new(
            self.x + by,
            self.y + by,
            (self.w - by * 2.0).max(0.0),
            (self.h - by * 2.0).max(0.0),
        )
    }

    #[must_use]
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            (self.w - dx * 2.0).max(0.0),
            (self.h - dy * 2.0).max(0.0),
        )
    }

    #[must_use]
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    #[must_use]
    pub fn scale(&self, by: f32) -> Self {
        Self::new(self.x * by, self.y * by, self.w * by, self.h * by)
    }

    /// Splits off `size` from the top, returning (top, rest).
    #[must_use]
    pub fn take_top(&self, size: f32) -> (Self, Self) {
        let size = size.min(self.h);
        (
            Self::new(self.x, self.y, self.w, size),
            Self::new(self.x, self.y + size, self.w, self.h - size),
        )
    }

    /// Splits off `size` from the left, returning (left, rest).
    #[must_use]
    pub fn take_left(&self, size: f32) -> (Self, Self) {
        let size = size.min(self.w);
        (
            Self::new(self.x, self.y, size, self.h),
            Self::new(self.x + size, self.y, self.w - size, self.h),
        )
    }

    /// A rect of `w`×`h` centered in this one.
    #[must_use]
    pub fn centered(&self, w: f32, h: f32) -> Self {
        Self::new(
            self.x + (self.w - w) / 2.0,
            self.y + (self.h - h) / 2.0,
            w,
            h,
        )
    }

    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Self::new(
            l(self.x, other.x),
            l(self.y, other.y),
            l(self.w, other.w),
            l(self.h, other.h),
        )
    }
}

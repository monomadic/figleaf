use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

/// Straight alpha, encoded sRGB channels, each 0 to 1: Figma's own numbers.
pub type Color = [f32; 4];

/// An opaque colour from `0xRRGGBB`.
pub const fn hex(rgb: u32) -> Color {
    hexa(rgb << 8 | 0xFF)
}

/// A colour from `0xRRGGBBAA`.
pub const fn hexa(rgba: u32) -> Color {
    [
        (rgba >> 24 & 0xFF) as f32 / 255.,
        (rgba >> 16 & 0xFF) as f32 / 255.,
        (rgba >> 8 & 0xFF) as f32 / 255.,
        (rgba & 0xFF) as f32 / 255.,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn validate(self) -> Result<(), Error> {
        let ends = [self.x + self.w, self.y + self.h];
        let all = [self.x, self.y, self.w, self.h];
        if !all.iter().chain(&ends).all(|v| v.is_finite()) || self.w < 0. || self.h < 0. {
            return Err(Error("invalid rectangle"));
        }
        Ok(())
    }
    pub fn intersect(self, b: Self) -> Self {
        let x = self.x.max(b.x);
        let y = self.y.max(b.y);
        Self {
            x,
            y,
            w: ((self.x + self.w).min(b.x + b.w) - x).max(0.),
            h: ((self.y + self.h).min(b.y + b.h) - y).max(0.),
        }
    }
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] < self.x + self.w && p[1] >= self.y && p[1] < self.y + self.h
    }
    /// Grows every side by `d`; a negative `d` shrinks, stopping at empty.
    pub fn inflate(self, d: f32) -> Self {
        let w = (self.w + 2. * d).max(0.);
        let h = (self.h + 2. * d).max(0.);
        Self {
            x: self.x + (self.w - w) / 2.,
            y: self.y + (self.h - h) / 2.,
            w,
            h,
        }
    }
    pub fn translate(self, by: [f32; 2]) -> Self {
        Self {
            x: self.x + by[0],
            y: self.y + by[1],
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_colours_and_rect_helpers() {
        assert_eq!(hex(0xFF0080), [1., 0., 128. / 255., 1.]);
        assert_eq!(hexa(0x00000080)[3], 128. / 255.);
        let r = Rect::new(10., 10., 20., 10.);
        assert_eq!(r.inflate(2.), Rect::new(8., 8., 24., 14.));
        assert_eq!(r.inflate(-8.), Rect::new(18., 15., 4., 0.));
        assert!(r.contains([10., 10.]) && !r.contains([30., 10.]));
        assert_eq!(
            r.intersect(Rect::new(0., 0., 15., 15.)),
            Rect::new(10., 10., 5., 5.)
        );
    }
}

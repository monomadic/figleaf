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

/// Figma's two affine rows: `[[a, c, tx], [b, d, ty]]`.
/// Maps local points into canvas coordinates. No perspective or singular maps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform(pub [[f32; 3]; 2]);
impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Transform {
    pub const IDENTITY: Self = Self([[1., 0., 0.], [0., 1., 0.]]);
    pub const fn new(rows: [[f32; 3]; 2]) -> Self {
        Self(rows)
    }
    pub const fn translation(x: f32, y: f32) -> Self {
        Self([[1., 0., x], [0., 1., y]])
    }
    pub fn point(self, p: [f32; 2]) -> [f32; 2] {
        self.0.map(|r| r[0] * p[0] + r[1] * p[1] + r[2])
    }
    /// Parent composed with child: applies `child` first, then `self`.
    pub fn compose(self, child: Self) -> Self {
        Self(self.0.map(|r| {
            [
                r[0] * child.0[0][0] + r[1] * child.0[1][0],
                r[0] * child.0[0][1] + r[1] * child.0[1][1],
                r[0] * child.0[0][2] + r[1] * child.0[1][2] + r[2],
            ]
        }))
    }
    pub fn inverse(self) -> Result<Self, Error> {
        let [[a, c, x], [b, d, y]] = self.0;
        let det = a as f64 * d as f64 - b as f64 * c as f64;
        if self.0.iter().flatten().any(|v| !v.is_finite()) || det == 0. {
            return Err(Error("non-finite or singular transform"));
        }
        let inv = Self([
            [
                (d as f64 / det) as f32,
                (-c as f64 / det) as f32,
                ((c as f64 * y as f64 - d as f64 * x as f64) / det) as f32,
            ],
            [
                (-b as f64 / det) as f32,
                (a as f64 / det) as f32,
                ((b as f64 * x as f64 - a as f64 * y as f64) / det) as f32,
            ],
        ]);
        if inv
            .0
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e7)
        {
            return Err(Error("transform inverse exceeds numeric limits"));
        }
        Ok(inv)
    }
    pub fn bounds(self, r: Rect) -> Rect {
        let p = [
            [r.x, r.y],
            [r.x + r.w, r.y],
            [r.x, r.y + r.h],
            [r.x + r.w, r.y + r.h],
        ]
        .map(|p| self.point(p));
        let lo = [0, 1].map(|i| p.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min));
        let hi = [0, 1].map(|i| p.iter().map(|p| p[i]).fold(f32::NEG_INFINITY, f32::max));
        Rect::new(lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1])
    }
    /// Largest singular value: maximum magnification along any direction.
    pub fn max_scale(self) -> f32 {
        let [[a, c, _], [b, d, _]] = self.0;
        let (a, b, c, d) = (a as f64, b as f64, c as f64, d as f64);
        let trace = a * a + b * b + c * c + d * d;
        ((trace + ((a * a + b * b - c * c - d * d).powi(2) + 4. * (a * c + b * d).powi(2)).sqrt())
            / 2.)
            .sqrt() as f32
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn pixels(self, input_scale: f32, output_scale: f32) -> Self {
        Self(self.0.map(|r| {
            [
                r[0] * output_scale / input_scale,
                r[1] * output_scale / input_scale,
                r[2] * output_scale,
            ]
        }))
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;
    #[test]
    fn composition_inverse_bounds_and_degeneracy() {
        let a = Transform::new([[0., -2., 30.], [3., 1., 4.]]);
        let b = Transform::new([[-1., 0.5, 12.], [0., 1., 8.]]);
        let p = [7., 9.];
        assert_eq!(a.compose(b).point(p), a.point(b.point(p)));
        let q = a.inverse().unwrap().point(a.point(p));
        assert!((q[0] - p[0]).abs() < 1e-5 && (q[1] - p[1]).abs() < 1e-5);
        assert_eq!(
            b.bounds(Rect::new(0., 0., 10., 20.)),
            Rect::new(2., 8., 20., 20.)
        );
        assert_eq!(
            Transform::new([[2., 0., 0.], [0., -3., 0.]]).max_scale(),
            3.
        );
        assert!(
            Transform::new([[1., 2., 0.], [2., 4., 0.]])
                .inverse()
                .is_err()
        );
        assert!(
            Transform::new([[f32::NAN, 0., 0.], [0., 1., 0.]])
                .inverse()
                .is_err()
        );
        assert!(
            Transform::new([[1e-9, 0., 0.], [0., 1., 0.]])
                .inverse()
                .is_err()
        );
    }
}

//! Host-owned image pixels and native drawing tiles. No file or network I/O.
use crate::{Canvas, Error, Rect, Transform};
use std::sync::Arc;

/// Straight-alpha, encoded-sRGB RGBA8 pixels, in row order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub size: [u32; 2],
    pub rgba: Arc<[u8]>,
}
impl Image {
    pub fn new(size: [u32; 2], rgba: impl Into<Arc<[u8]>>) -> Self {
        Self {
            size,
            rgba: rgba.into(),
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        let pixels = u64::from(self.size[0]) * u64::from(self.size[1]);
        if self.size.contains(&0) || pixels > 16_000_000 || pixels * 4 != self.rgba.len() as u64 {
            return Err(Error(
                "image requires valid RGBA8 pixels, at most 16 million pixels",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageScaleMode {
    Fill,
    Fit,
    Crop(Transform),
    Tile(f32),
}
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePaint {
    pub image: Image,
    pub scale_mode: ImageScaleMode,
    pub rotation: f32,
    pub opacity: f32,
}
impl ImagePaint {
    pub fn new(image: Image) -> Self {
        Self {
            image,
            scale_mode: ImageScaleMode::Fill,
            rotation: 0.,
            opacity: 1.,
        }
    }
    pub fn scale_mode(mut self, mode: ImageScaleMode) -> Self {
        self.scale_mode = mode;
        self
    }
    pub fn rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }
    pub fn opacity(mut self, value: f32) -> Self {
        self.opacity = value;
        self
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Rectangular,
    HorizontalHexagonal,
    VerticalHexagonal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternAlignment {
    Start,
    Center,
    End,
}
/// A native drawing repeated as a paint. The source occupies `[0, 0, width, height]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub source: Canvas,
    pub size: [f32; 2],
    pub scaling_factor: f32,
    /// Fractions of the scaled tile dimensions; 0.2 adds a 20% gap.
    pub spacing: [f32; 2],
    pub tile_type: TileType,
    pub horizontal_alignment: PatternAlignment,
    pub vertical_alignment: PatternAlignment,
    pub opacity: f32,
}
impl Pattern {
    pub fn new(source: Canvas, size: [f32; 2]) -> Self {
        Self {
            source,
            size,
            scaling_factor: 1.,
            spacing: [0.; 2],
            tile_type: TileType::Rectangular,
            horizontal_alignment: PatternAlignment::Start,
            vertical_alignment: PatternAlignment::Start,
            opacity: 1.,
        }
    }
    pub fn scaling_factor(mut self, value: f32) -> Self {
        self.scaling_factor = value;
        self
    }
    pub fn spacing(mut self, value: [f32; 2]) -> Self {
        self.spacing = value;
        self
    }
    pub fn tile_type(mut self, value: TileType) -> Self {
        self.tile_type = value;
        self
    }
    pub fn vertical_alignment(mut self, value: PatternAlignment) -> Self {
        self.vertical_alignment = value;
        self
    }
    pub fn horizontal_alignment(mut self, value: PatternAlignment) -> Self {
        self.horizontal_alignment = value;
        self
    }
    pub fn opacity(mut self, value: f32) -> Self {
        self.opacity = value;
        self
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TexturePaint<'a> {
    Image(&'a ImagePaint),
    Pattern(&'a Pattern),
}
impl TexturePaint<'_> {
    pub fn opacity(self) -> f32 {
        match self {
            Self::Image(p) => p.opacity,
            Self::Pattern(p) => p.opacity,
        }
    }
    /// Local-point to image UV, plus tile period and tiling kind (0 fill, 1 fit/crop,
    /// 2 repeat, 3 stagger rows, 4 stagger columns).
    pub(crate) fn mapping(self, rect: Rect) -> Result<(Transform, [f32; 4]), Error> {
        let result = self.mapping_inner(rect)?;
        if !result
            .0
            .0
            .iter()
            .flatten()
            .chain(result.1.iter())
            .all(|v| v.is_finite())
        {
            return Err(Error("image/pattern mapping is out of range"));
        }
        result.0.bounds(rect).validate()?;
        Ok(result)
    }
    fn mapping_inner(self, rect: Rect) -> Result<(Transform, [f32; 4]), Error> {
        if rect.w <= 0.
            || rect.h <= 0.
            || !self.opacity().is_finite()
            || !(0. ..=1.).contains(&self.opacity())
        {
            return Err(Error("invalid image/pattern bounds or opacity"));
        }
        let normalize = Transform::new([
            [1. / rect.w, 0., -rect.x / rect.w],
            [0., 1. / rect.h, -rect.y / rect.h],
        ]);
        match self {
            Self::Image(p) => {
                p.image.validate()?;
                if !p.rotation.is_finite() || p.rotation % 90. != 0. {
                    return Err(Error("image rotation must be a multiple of 90 degrees"));
                }
                if let ImageScaleMode::Crop(m) = p.scale_mode {
                    m.inverse()?;
                    if p.rotation != 0. {
                        return Err(Error("crop rotation must be in the image transform"));
                    }
                    return Ok((m.compose(normalize), [1., 1., 1., 0.]));
                }
                let angle = p.rotation.to_radians();
                let (sin, cos) = angle.sin_cos();
                let [w, h] = p.image.size.map(|v| v as f32);
                let rotated = [cos.abs() * w + sin.abs() * h, sin.abs() * w + cos.abs() * h];
                let (scale, kind) = match p.scale_mode {
                    ImageScaleMode::Fill => ((rect.w / rotated[0]).max(rect.h / rotated[1]), 0.),
                    ImageScaleMode::Fit => ((rect.w / rotated[0]).min(rect.h / rotated[1]), 1.),
                    ImageScaleMode::Tile(s) => (s, 2.),
                    _ => unreachable!(),
                };
                if !scale.is_finite() || scale <= 0. {
                    return Err(Error("invalid image tile scale"));
                }
                let center = if kind == 2. {
                    [
                        rect.x + rotated[0] * scale / 2.,
                        rect.y + rotated[1] * scale / 2.,
                    ]
                } else {
                    [rect.x + rect.w / 2., rect.y + rect.h / 2.]
                };
                let a = cos / (w * scale);
                let b = sin / (w * scale);
                let c = -sin / (h * scale);
                let d = cos / (h * scale);
                Ok((
                    Transform::new([
                        [a, b, 0.5 - a * center[0] - b * center[1]],
                        [c, d, 0.5 - c * center[0] - d * center[1]],
                    ]),
                    [1., 1., kind, 0.],
                ))
            }
            Self::Pattern(p) => {
                if !p.size.iter().all(|v| v.is_finite() && *v > 0.)
                    || !p.scaling_factor.is_finite()
                    || p.scaling_factor <= 0.
                    || !p.spacing.iter().all(|v| v.is_finite() && *v >= 0.)
                {
                    return Err(Error("invalid pattern size, scale or spacing"));
                }
                let [w, h] = p.size.map(|v| v * p.scaling_factor);
                let period = [1. + p.spacing[0], 1. + p.spacing[1]];
                let offset = match p.horizontal_alignment {
                    PatternAlignment::Start => 0.,
                    PatternAlignment::Center => (rect.w - w) / 2.,
                    PatternAlignment::End => rect.w - w,
                };
                let offset_y = match p.vertical_alignment {
                    PatternAlignment::Start => 0.,
                    PatternAlignment::Center => (rect.h - h) / 2.,
                    PatternAlignment::End => rect.h - h,
                };
                let kind = match p.tile_type {
                    TileType::Rectangular => 2.,
                    TileType::HorizontalHexagonal => 3.,
                    TileType::VerticalHexagonal => 4.,
                };
                Ok((
                    Transform::new([
                        [1. / w, 0., -(rect.x + offset) / w],
                        [0., 1. / h, -(rect.y + offset_y) / h],
                    ]),
                    [period[0], period[1], kind, 0.],
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_rejects_nonfinite_values_and_pixels() {
        let image = Image::new([1, 1], vec![255; 4]);
        for p in [
            ImagePaint::new(image.clone()).opacity(f32::NAN),
            ImagePaint::new(image.clone()).scale_mode(ImageScaleMode::Tile(f32::MIN_POSITIVE)),
            ImagePaint::new(image).rotation(f32::INFINITY),
        ] {
            assert!(
                TexturePaint::Image(&p)
                    .mapping(Rect::new(0., 0., 100., 100.))
                    .is_err()
            );
        }
        assert!(Image::new([0, 1], Vec::new()).validate().is_err());
        assert!(
            Image::new([u32::MAX, u32::MAX], Vec::new())
                .validate()
                .is_err()
        );
    }
}

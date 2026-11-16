//! Scoped paint compositing. A mask is consumed once by its content canvas.
use crate::Canvas;
#[cfg(feature = "wgpu")]
use crate::{Error, Paint, paint::Item};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskType {
    Vector,
    Alpha,
    Luminance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    pub kind: MaskType,
    pub canvas: Canvas,
    pub opacity: f32,
}
impl Mask {
    pub fn new(kind: MaskType, canvas: Canvas) -> Self {
        Self {
            kind,
            canvas,
            opacity: 1.,
        }
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn source(&self) -> Result<Canvas, Error> {
        if !self.opacity.is_finite() || !(0. ..=1.).contains(&self.opacity) {
            return Err(Error("invalid mask opacity"));
        }
        if self.kind != MaskType::Vector {
            return Ok(self.canvas.clone());
        }
        let mut canvas = Canvas::new();
        for item in self.canvas.items() {
            let Item::Shape(shape) = item else {
                return Err(Error("vector masks require shape geometry"));
            };
            if let Some(reason) = shape.problem() {
                return Err(Error(reason));
            }
            let mut shape = shape.clone();
            shape.fills = shape
                .fills
                .iter()
                .filter(|p| match p {
                    Paint::Image(p) => {
                        p.opacity > 0.
                            && p.image
                                .rgba
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .any(|pixel| pixel[3] > 0)
                    }
                    Paint::Pattern(p) => p.opacity > 0.,
                    Paint::Solid(c) => c[3] > 0.,
                    Paint::Linear(g) | Paint::Gradient(g) => {
                        g.opacity > 0. && g.stops.iter().any(|s| s.color[3] > 0.)
                    }
                })
                .map(|_| Paint::Solid([1.; 4]))
                .collect();
            let stroke_visible = if shape.stroke_paints.is_empty() {
                shape.stroke.is_some_and(|s| s.color[3] > 0.)
            } else {
                shape.stroke_paints.iter().any(|p| match p {
                    Paint::Image(p) => {
                        p.opacity > 0.
                            && p.image
                                .rgba
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .any(|pixel| pixel[3] > 0)
                    }
                    Paint::Pattern(p) => p.opacity > 0.,
                    Paint::Solid(c) => c[3] > 0.,
                    Paint::Linear(g) | Paint::Gradient(g) => {
                        g.opacity > 0. && g.stops.iter().any(|s| s.color[3] > 0.)
                    }
                })
            };
            shape.stroke_paints.clear();
            if !stroke_visible {
                shape.stroke = None;
                shape.stroke_weights = None;
                shape.stroke_geometry = None;
                shape.stroke_paths = None;
                shape.path_stroke = Default::default();
                shape.dash_pattern.clear();
            }
            if let Some(stroke) = &mut shape.stroke {
                stroke.color = [1.; 4];
            }
            shape.shadows.clear();
            canvas.shape(shape);
        }
        Ok(canvas)
    }
}

/// One atomic display-list operation, not a scene node. Coordinates in both
/// canvases are in the same space as the containing canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Masked {
    pub mask: Mask,
    pub content: Canvas,
}

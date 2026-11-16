//! Independent canvas-space clips, intersected in the order supplied.
use crate::{Error, Rect, Transform, VectorPath, path};

#[derive(Debug, Clone, PartialEq)]
pub enum ClipGeometry {
    RoundedRect { rect: Rect, radii: [f32; 4] },
    Paths(Vec<VectorPath>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub geometry: ClipGeometry,
    pub transform: Transform,
}
impl Clip {
    pub fn rect(rect: Rect) -> Self {
        Self::rounded_rect(rect, [0.; 4])
    }
    /// Radii in top-left, top-right, bottom-right, bottom-left order.
    pub fn rounded_rect(rect: Rect, radii: [f32; 4]) -> Self {
        Self {
            geometry: ClipGeometry::RoundedRect { rect, radii },
            transform: Transform::IDENTITY,
        }
    }
    /// Regions are unioned within this clip; an empty list clips everything out.
    pub fn paths(paths: Vec<VectorPath>) -> Self {
        Self {
            geometry: ClipGeometry::Paths(paths),
            transform: Transform::IDENTITY,
        }
    }
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.transform.inverse()?;
        let bounds = match &self.geometry {
            ClipGeometry::RoundedRect { rect, radii } => {
                rect.validate()?;
                if radii
                    .iter()
                    .any(|r| !r.is_finite() || !(0. ..=1e7).contains(r))
                {
                    return Err(Error("invalid clip corner radius"));
                }
                *rect
            }
            ClipGeometry::Paths(paths) => {
                if paths
                    .iter()
                    .any(|p| p.start_cap.is_some() || p.end_cap.is_some())
                {
                    return Err(Error("clip paths cannot have stroke endpoint overrides"));
                }
                path::bounds(paths)?
            }
        };
        self.transform.bounds(bounds).validate()
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn edges(&self, scale: f32) -> Result<Vec<path::Edge>, Error> {
        self.validate()?;
        match &self.geometry {
            ClipGeometry::RoundedRect { rect, radii } => {
                if rect.w == 0. || rect.h == 0. {
                    return Ok(vec![]);
                }
                let radii = radii.map(|r| r.min(rect.w.min(rect.h) / 2.));
                path::flatten_transformed(
                    &[path::primitive_path(*rect, radii, false)],
                    [0.; 2],
                    scale,
                    self.transform,
                )
            }
            ClipGeometry::Paths(paths) => {
                path::flatten_transformed(paths, [0.; 2], scale, self.transform)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Shape, StrokeCap, WindingRule};
    #[test]
    fn clip_validation_is_available_without_renderer() {
        let rect = Rect::new(0., 0., 20., 20.);
        for clip in [
            Clip::rounded_rect(rect, [-1., 0., 0., 0.]),
            Clip::rect(rect).transform(Transform::new([[0.; 3]; 2])),
            Clip::paths(vec![
                VectorPath::new(WindingRule::NonZero).move_to([f32::NAN, 0.]),
            ]),
            Clip::paths(vec![
                VectorPath::new(WindingRule::NonZero).start_cap(StrokeCap::Round),
            ]),
        ] {
            assert!(clip.validate().is_err());
            assert!(Shape::rect(rect).intersect_clip(clip).problem().is_some());
        }
        assert!(Clip::paths(vec![]).validate().is_ok());
        let mut shape = Shape::rect(rect);
        shape.clips = vec![Clip::rect(rect); 129];
        assert!(shape.problem().is_some());
    }
}

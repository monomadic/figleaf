//! Parameterized Figma primitives lowered to the shared vector path API.
use crate::{Error, Rect, VectorPath, WindingRule};
use std::f32::consts::{PI, TAU};

/// Figma's ellipse sweep: radians clockwise from +x, and inner/outer radius ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcData {
    pub starting_angle: f32,
    pub ending_angle: f32,
    pub inner_radius: f32,
}
impl ArcData {
    pub const fn new(starting_angle: f32, ending_angle: f32, inner_radius: f32) -> Self {
        Self {
            starting_angle,
            ending_angle,
            inner_radius,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    Arc(ArcData),
    Polygon { point_count: u32 },
    Star { point_count: u32, inner_radius: f32 },
}
impl Primitive {
    pub fn validate(self, radius: f32) -> Result<(), Error> {
        if !radius.is_finite() || !(0. ..=1e7).contains(&radius) {
            return Err(Error("invalid primitive corner radius"));
        }
        match self {
            Self::Arc(a) => {
                if !a.starting_angle.is_finite()
                    || !a.ending_angle.is_finite()
                    || a.starting_angle.abs() > 1e7
                    || a.ending_angle.abs() > 1e7
                    || !(0. ..=1.).contains(&a.inner_radius)
                    || (a.ending_angle - a.starting_angle).abs() > TAU + 1e-5
                {
                    return Err(Error("invalid ellipse arc parameters"));
                }
                if radius != 0. {
                    return Err(Error("rounded ellipse arc corners are unsupported"));
                }
            }
            Self::Polygon { point_count } | Self::Star { point_count, .. } => {
                if !(3..=256).contains(&point_count) {
                    return Err(Error("primitive point count must be 3 to 256"));
                }
                if let Self::Star { inner_radius, .. } = self
                    && !(0. ..=1.).contains(&inner_radius)
                {
                    return Err(Error("star inner radius must be 0 to 1"));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn is_open_arc(self) -> bool {
        matches!(self, Self::Arc(a) if a.inner_radius == 1.)
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn render_paths(
        self,
        rect: Rect,
        radius: f32,
        stroke: Option<crate::Stroke>,
    ) -> Result<Vec<VectorPath>, Error> {
        if let (Self::Arc(a), Some(stroke)) = (self, stroke)
            && a.inner_radius == 1.
            && stroke.align != crate::StrokeAlign::Center
        {
            self.validate(radius)?;
            rect.validate()?;
            let offset = stroke.width / 2.
                * if stroke.align == crate::StrokeAlign::Inside {
                    -1.
                } else {
                    1.
                };
            let rx = rect.w / 2.;
            let ry = rect.h / 2.;
            if rx == 0. || ry == 0. {
                return Ok(vec![]);
            }
            if offset < 0. && -offset >= rx.min(ry).powi(2) / rx.max(ry) {
                return Err(Error("inside arc stroke exceeds ellipse curvature radius"));
            }
            let sweep = a.ending_angle - a.starting_angle;
            if sweep == 0. {
                return Ok(vec![]);
            }
            let sample = |t: f32| {
                let v = [t.cos() / rx, t.sin() / ry];
                let d = [-t.sin() / rx, t.cos() / ry];
                let len = v[0].hypot(v[1]);
                let dot = (v[0] * d[0] + v[1] * d[1]) / (len * len);
                let p = point(rect, t, 1.);
                (
                    [p[0] + offset * v[0] / len, p[1] + offset * v[1] / len],
                    [
                        -rx * t.sin() + offset * (d[0] - v[0] * dot) / len,
                        ry * t.cos() + offset * (d[1] - v[1] * dot) / len,
                    ],
                )
            };
            let mut p = VectorPath::new(WindingRule::None).move_to(sample(a.starting_angle).0);
            let count = arc_segments(sweep);
            let step = sweep / count as f32;
            for i in 0..count {
                let (x, dx) = sample(a.starting_angle + step * i as f32);
                let (y, dy) = sample(a.starting_angle + step * (i + 1) as f32);
                let k = 4. / 3. * (step / 4.).tan();
                p = p.cubic_to(
                    [x[0] + k * dx[0], x[1] + k * dx[1]],
                    [y[0] - k * dy[0], y[1] - k * dy[1]],
                    y,
                );
            }
            if (sweep.abs() - TAU).abs() < 1e-5 {
                p = p.close();
            }
            return Ok(vec![p]);
        }
        self.paths(rect, radius)
    }
    /// Coordinates include the supplied rectangle origin. Parameters stay on Shape;
    /// callers can also inspect/reuse these ordinary paths without a renderer.
    pub fn paths(self, rect: Rect, radius: f32) -> Result<Vec<VectorPath>, Error> {
        self.validate(radius)?;
        rect.validate()?;
        if rect.w == 0. || rect.h == 0. {
            return Ok(vec![]);
        }
        match self {
            Self::Arc(a) => Ok(arc(rect, a)),
            Self::Polygon { point_count } => Ok(vec![polygon(rect, point_count, None, radius)]),
            Self::Star {
                point_count,
                inner_radius,
            } => Ok(vec![polygon(rect, point_count, Some(inner_radius), radius)]),
        }
    }
}
fn point(r: Rect, t: f32, ratio: f32) -> [f32; 2] {
    [
        r.x + r.w / 2. + r.w / 2. * ratio * t.cos(),
        r.y + r.h / 2. + r.h / 2. * ratio * t.sin(),
    ]
}
// Figma bisects partial arcs until each segment spans at most pi/4.
// Keep these boundaries: dash fitting treats each cubic as a segment.
fn arc_segments(sweep: f32) -> u32 {
    if (sweep.abs() - TAU).abs() < 1e-5 {
        4
    } else {
        ((sweep.abs() as f64 / (std::f64::consts::PI / 4.))
            .ceil()
            .max(1.) as u32)
            .next_power_of_two()
    }
}
fn append_arc(mut p: VectorPath, r: Rect, start: f32, sweep: f32, ratio: f32) -> VectorPath {
    let count = arc_segments(sweep);
    let step = sweep / count as f32;
    for i in 0..count {
        let a = start + step * i as f32;
        let b = a + step;
        let k = 4. / 3. * (step / 4.).tan();
        let x = point(r, a, ratio);
        let y = point(r, b, ratio);
        p = p.cubic_to(
            [
                x[0] - k * r.w / 2. * ratio * a.sin(),
                x[1] + k * r.h / 2. * ratio * a.cos(),
            ],
            [
                y[0] + k * r.w / 2. * ratio * b.sin(),
                y[1] - k * r.h / 2. * ratio * b.cos(),
            ],
            y,
        );
    }
    p
}
fn arc(r: Rect, a: ArcData) -> Vec<VectorPath> {
    let sweep = a.ending_angle - a.starting_angle;
    if sweep == 0. {
        return vec![];
    }
    let full = (sweep.abs() - TAU).abs() < 1e-5;
    let mut p = VectorPath::new(if a.inner_radius == 1. {
        WindingRule::None
    } else {
        WindingRule::NonZero
    })
    .move_to(point(r, a.starting_angle, 1.));
    p = append_arc(p, r, a.starting_angle, sweep, 1.);
    if a.inner_radius == 1. {
        if full {
            p = p.close();
        }
        return vec![p];
    }
    if full {
        p = p.close();
        if a.inner_radius > 0. {
            p = p.move_to(point(r, a.ending_angle, a.inner_radius));
            p = append_arc(p, r, a.ending_angle, -sweep, a.inner_radius).close();
        }
    } else {
        if a.inner_radius > 0. {
            p = p.line_to(point(r, a.ending_angle, a.inner_radius));
            p = append_arc(p, r, a.ending_angle, -sweep, a.inner_radius);
        } else {
            p = p.line_to([r.x + r.w / 2., r.y + r.h / 2.]);
        }
        p = p.close();
    }
    vec![p]
}
fn polygon(r: Rect, n: u32, inner: Option<f32>, radius: f32) -> VectorPath {
    let count = if inner.is_some() { n * 2 } else { n };
    let pts: Vec<_> = (0..count)
        .map(|i| {
            point(
                r,
                -PI / 2. + TAU * i as f32 / count as f32,
                if i % 2 == 1 { inner.unwrap_or(1.) } else { 1. },
            )
        })
        .collect();
    let count = pts.len();
    let mut dirs = Vec::new();
    let mut lengths = Vec::new();
    for i in 0..count {
        let d = [
            pts[(i + 1) % count][0] - pts[i][0],
            pts[(i + 1) % count][1] - pts[i][1],
        ];
        let len = d[0].hypot(d[1]);
        lengths.push(len);
        dirs.push(if len > 0. {
            [d[0] / len, d[1] / len]
        } else {
            [0.; 2]
        });
    }
    let turns: Vec<_> = (0..count)
        .map(|i| {
            let a = dirs[(i + count - 1) % count];
            let b = dirs[i];
            (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1])
        })
        .collect();
    let wanted: Vec<_> = turns
        .iter()
        .map(|t| {
            if t.abs() > PI - 1e-5 {
                0.
            } else {
                radius * (t.abs() / 2.).tan()
            }
        })
        .collect();
    let distances: Vec<_> = (0..count)
        .map(|i| {
            let prev = (i + count - 1) % count;
            let next = (i + 1) % count;
            let limit = (lengths[prev] / (wanted[prev] + wanted[i]).max(1e-20))
                .min(lengths[i] / (wanted[i] + wanted[next]).max(1e-20))
                .min(1.);
            wanted[i] * limit
        })
        .collect();
    let mut p = VectorPath::new(WindingRule::NonZero);
    for i in 0..count {
        let v = pts[i];
        let incoming = dirs[(i + count - 1) % count];
        let outgoing = dirs[i];
        let d = distances[i];
        let a = [v[0] - incoming[0] * d, v[1] - incoming[1] * d];
        let b = [v[0] + outgoing[0] * d, v[1] + outgoing[1] * d];
        p = if i == 0 { p.move_to(a) } else { p.line_to(a) };
        if d > 0. {
            let effective = d / (turns[i].abs() / 2.).tan();
            let k = 4. / 3. * (turns[i].abs() / 4.).tan() * effective;
            p = p.cubic_to(
                [a[0] + incoming[0] * k, a[1] + incoming[1] * k],
                [b[0] - outgoing[0] * k, b[1] - outgoing[1] * k],
                b,
            );
        }
    }
    p.close()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Shape, path::Command};
    #[test]
    fn inspector_parameters_and_invalid_geometry() {
        let r = Rect::new(10., 20., 100., 80.);
        let triangle = Primitive::Polygon { point_count: 3 }.paths(r, 0.).unwrap();
        match triangle[0].commands[0] {
            Command::MoveTo(p) => {
                assert!((p[0] - 60.).abs() < 1e-4);
                assert!((p[1] - 20.).abs() < 1e-4);
            }
            _ => panic!("missing move"),
        }
        // Inscribed in the inspector rectangle, not stretched to tight bounds.
        match triangle[0].commands[1] {
            Command::LineTo(p) => {
                assert!((p[0] - 103.30127).abs() < 1e-4);
                assert!((p[1] - 80.).abs() < 1e-4);
            }
            _ => panic!("missing edge"),
        }
        for n in [0, 2, 257, u32::MAX] {
            assert!(Shape::polygon(r, n).problem().is_some());
        }
        for inner in [-1., 1.1, f32::NAN] {
            assert!(Shape::star(r, 5, inner).problem().is_some());
        }
        for a in [
            ArcData::new(0., TAU + 0.1, 0.),
            ArcData::new(0., 1., 2.),
            ArcData::new(f32::NAN, 1., 0.),
        ] {
            assert!(Shape::ellipse_arc(r, a).problem().is_some());
        }
        assert!(
            Shape::ellipse_arc(r, ArcData::new(0., 1., 0.))
                .radius(2.)
                .problem()
                .is_some()
        );
        assert!(
            Primitive::Polygon { point_count: 3 }
                .paths(r, f32::MAX)
                .is_err()
        );
    }
    #[test]
    fn signed_arcs_holes_and_empty_sweeps() {
        for (sweep, expected) in [(1., 2), (2.5, 4), (4., 8), (TAU, 4)] {
            let p = Primitive::Arc(ArcData::new(0., sweep, 0.))
                .paths(Rect::new(0., 0., 100., 80.), 0.)
                .unwrap();
            assert_eq!(
                p[0].commands
                    .iter()
                    .filter(|c| matches!(c, Command::CubicTo(..)))
                    .count(),
                expected
            );
        }
        let r = Rect::new(0., 0., 100., 80.);
        assert!(
            Primitive::Arc(ArcData::new(1., 1., 0.))
                .paths(r, 0.)
                .unwrap()
                .is_empty()
        );
        let ring = Primitive::Arc(ArcData::new(0., TAU, 0.5))
            .paths(r, 0.)
            .unwrap();
        assert_eq!(
            ring[0]
                .commands
                .iter()
                .filter(|c| matches!(c, Command::Close))
                .count(),
            2
        );
        let p = Primitive::Arc(ArcData::new(PI, 0., 1.))
            .paths(r, 0.)
            .unwrap();
        assert_eq!(p[0].winding_rule, WindingRule::None);
        assert!(!p[0].commands.contains(&Command::Close));
        if let Command::CubicTo(_, _, p) = p[0].commands[1] {
            assert!(p[1] > 40.);
        } else {
            panic!("missing arc");
        }
        for inner in [0., 0.5, 1.] {
            for radius in [0., 8., 100.] {
                let paths = Primitive::Star {
                    point_count: 5,
                    inner_radius: inner,
                }
                .paths(r, radius)
                .unwrap();
                assert!(crate::path::bounds(&paths).is_ok());
            }
        }
        assert!(
            Primitive::Polygon { point_count: 3 }
                .paths(Rect::new(0., 0., 0., 80.), 8.)
                .unwrap()
                .is_empty()
        );
    }
}

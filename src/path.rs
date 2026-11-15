//! Filled vector paths. Curves are subdivided at the renderer's pixel scale;
//! open subpaths close implicitly for filling. Stroking is a separate operation.
use crate::{Error, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindingRule {
    NonZero,
    EvenOdd,
    /// Preserved open/unfilled network branches; no fill coverage.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    MoveTo([f32; 2]),
    LineTo([f32; 2]),
    QuadraticTo([f32; 2], [f32; 2]),
    CubicTo([f32; 2], [f32; 2], [f32; 2]),
    Close,
}

/// One filled region, potentially with several subpaths and holes. Several
/// regions in a shape are unioned before paint is applied, never alpha-stacked.
/// Coordinates are local to the shape's rectangle; they may extend beyond it.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorPath {
    pub winding_rule: WindingRule,
    pub commands: Vec<Command>,
    /// Endpoint overrides for every open subpath; closed contours have no caps.
    pub start_cap: Option<StrokeCap>,
    pub end_cap: Option<StrokeCap>,
}

impl VectorPath {
    pub fn new(winding_rule: WindingRule) -> Self {
        Self {
            winding_rule,
            commands: Vec::new(),
            start_cap: None,
            end_cap: None,
        }
    }
    pub fn start_cap(mut self, cap: StrokeCap) -> Self {
        self.start_cap = Some(cap);
        self
    }
    pub fn end_cap(mut self, cap: StrokeCap) -> Self {
        self.end_cap = Some(cap);
        self
    }
    pub fn move_to(mut self, to: [f32; 2]) -> Self {
        self.commands.push(Command::MoveTo(to));
        self
    }
    pub fn line_to(mut self, to: [f32; 2]) -> Self {
        self.commands.push(Command::LineTo(to));
        self
    }
    pub fn quadratic_to(mut self, control: [f32; 2], to: [f32; 2]) -> Self {
        self.commands.push(Command::QuadraticTo(control, to));
        self
    }
    pub fn cubic_to(mut self, c1: [f32; 2], c2: [f32; 2], to: [f32; 2]) -> Self {
        self.commands.push(Command::CubicTo(c1, c2, to));
        self
    }
    pub fn close(mut self) -> Self {
        self.commands.push(Command::Close);
        self
    }
}

/// Fill bounds include control points, so curves cannot escape them. Limits are
/// explicit errors, shared by the paint description and GPU preparation.
pub fn bounds(paths: &[VectorPath]) -> Result<Rect, Error> {
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    let mut count = 0;
    if paths.len() > 256 {
        return Err(Error("vector exceeds 256 regions"));
    }
    for path in paths {
        let mut started = false;
        for command in &path.commands {
            count += 1;
            if count > 4096 {
                return Err(Error("vector exceeds 4096 commands"));
            }
            let points: &[[f32; 2]] = match command {
                Command::MoveTo(p) => {
                    started = true;
                    std::slice::from_ref(p)
                }
                Command::LineTo(p) => std::slice::from_ref(p),
                Command::QuadraticTo(_, _) | Command::CubicTo(_, _, _) => &[],
                Command::Close => &[],
            };
            if !started {
                return Err(Error("vector subpath must start with move_to"));
            }
            let controls = match *command {
                Command::QuadraticTo(a, b) => vec![a, b],
                Command::CubicTo(a, b, c) => vec![a, b, c],
                _ => points.to_vec(),
            };
            for p in controls {
                for axis in 0..2 {
                    if !p[axis].is_finite() || p[axis].abs() > 1e7 {
                        return Err(Error("invalid vector coordinate"));
                    }
                    if path.winding_rule != WindingRule::None {
                        lo[axis] = lo[axis].min(p[axis]);
                        hi[axis] = hi[axis].max(p[axis]);
                    }
                }
            }
        }
    }
    if !lo[0].is_finite() {
        return Ok(Rect::default());
    }
    Ok(Rect::new(lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]))
}

/// A directed fill edge in physical pixels, with the end of each region marked.
#[cfg(any(feature = "wgpu", test))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Edge {
    pub points: [f32; 4],
    pub even_odd: bool,
    pub end_region: bool,
}

// Renderer-independent subdivision also allows default-build tests to verify
// geometry. Distance to the *segment*, not the infinite chord, catches cusps
// and collinear curves that double back beyond their endpoints.
#[cfg(test)]
pub(crate) fn flatten(
    paths: &[VectorPath],
    origin: [f32; 2],
    scale: f32,
) -> Result<Vec<Edge>, Error> {
    flatten_transformed(paths, origin, scale, crate::Transform::IDENTITY)
}

#[cfg(any(feature = "wgpu", test))]
pub(crate) fn flatten_transformed(
    paths: &[VectorPath],
    origin: [f32; 2],
    scale: f32,
    transform: crate::Transform,
) -> Result<Vec<Edge>, Error> {
    transform.inverse()?;
    bounds(paths)?;
    if !scale.is_finite() || scale <= 0. {
        return Err(Error("invalid vector scale"));
    }
    let mut edges = Vec::new();
    let point = |p: [f32; 2]| -> Result<[f32; 2], Error> {
        let p = transform
            .point([p[0] + origin[0], p[1] + origin[1]])
            .map(|v| v * scale);
        if p.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err(Error("vector exceeds physical coordinate limit"));
        }
        Ok(p)
    };
    for path in paths {
        if path.winding_rule == WindingRule::None {
            continue;
        }
        let first_edge = edges.len();
        let mut current = [0.; 2];
        let mut start = None;
        let even_odd = path.winding_rule == WindingRule::EvenOdd;
        for command in &path.commands {
            match *command {
                Command::MoveTo(p) => {
                    if let Some(first) = start {
                        edge(&mut edges, current, first, even_odd)?;
                    }
                    current = point(p)?;
                    start = Some(current);
                }
                Command::LineTo(p) => {
                    let p = point(p)?;
                    edge(&mut edges, current, p, even_odd)?;
                    current = p;
                }
                Command::QuadraticTo(c, p) => {
                    let c = point(c)?;
                    let p = point(p)?;
                    curve(
                        &mut edges,
                        [current, mix(current, c, 2. / 3.), mix(p, c, 2. / 3.), p],
                        even_odd,
                        0,
                    )?;
                    current = p;
                }
                Command::CubicTo(c1, c2, p) => {
                    let p = point(p)?;
                    curve(
                        &mut edges,
                        [current, point(c1)?, point(c2)?, p],
                        even_odd,
                        0,
                    )?;
                    current = p;
                }
                Command::Close => {
                    if let Some(first) = start {
                        edge(&mut edges, current, first, even_odd)?;
                        current = first;
                    }
                }
            }
        }
        if let Some(first) = start {
            edge(&mut edges, current, first, even_odd)?;
        }
        if edges.len() > first_edge {
            edges.last_mut().unwrap().end_region = true;
        }
    }
    Ok(edges)
}
#[cfg(any(feature = "wgpu", test))]
fn mix(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
#[cfg(any(feature = "wgpu", test))]
fn edge(edges: &mut Vec<Edge>, a: [f32; 2], b: [f32; 2], even_odd: bool) -> Result<(), Error> {
    if a == b {
        return Ok(());
    }
    if edges.len() >= 8192 {
        return Err(Error("vector exceeds 8192 flattened edges"));
    }
    edges.push(Edge {
        points: [a[0], a[1], b[0], b[1]],
        even_odd,
        end_region: false,
    });
    Ok(())
}
#[cfg(any(feature = "wgpu", test))]
fn curve(edges: &mut Vec<Edge>, p: [[f32; 2]; 4], rule: bool, depth: u32) -> Result<(), Error> {
    let distance = |q: [f32; 2]| {
        let d = [p[3][0] - p[0][0], p[3][1] - p[0][1]];
        let length = d[0] * d[0] + d[1] * d[1];
        let t = if length > 0. {
            ((q[0] - p[0][0]) * d[0] + (q[1] - p[0][1]) * d[1]) / length
        } else {
            0.
        };
        let at = mix(p[0], p[3], t.clamp(0., 1.));
        (q[0] - at[0]).hypot(q[1] - at[1])
    };
    if distance(p[1]).max(distance(p[2])) <= 0.125 {
        return edge(edges, p[0], p[3], rule);
    }
    if depth >= 20 {
        return Err(Error("vector subdivision limit exceeded"));
    }
    let a = mix(p[0], p[1], 0.5);
    let b = mix(p[1], p[2], 0.5);
    let c = mix(p[2], p[3], 0.5);
    let d = mix(a, b, 0.5);
    let e = mix(b, c, 0.5);
    let f = mix(d, e, 0.5);
    curve(edges, [p[0], a, d, f], rule, depth + 1)?;
    curve(edges, [f, e, c, p[3]], rule, depth + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_contours_close_and_regions_keep_their_rule() {
        let p = VectorPath::new(WindingRule::EvenOdd)
            .move_to([0., 0.])
            .line_to([10., 0.])
            .line_to([0., 10.]);
        let edges = flatten(&[p], [3., 4.], 2.).unwrap();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[2].points, [6., 28., 6., 8.]);
        assert!(edges.iter().all(|e| e.even_odd));
        assert!(edges[2].end_region);
        assert!(!edges[0].end_region);
    }
    #[test]
    fn curves_subdivide_at_scale_and_do_not_erase_collinear_reversals() {
        let p = VectorPath::new(WindingRule::NonZero)
            .move_to([0., 0.])
            .quadratic_to([10., 20.], [20., 0.])
            .close();
        assert!(
            flatten(std::slice::from_ref(&p), [0.; 2], 4.)
                .unwrap()
                .len()
                > flatten(&[p], [0.; 2], 1.).unwrap().len()
        );
        let looped = VectorPath::new(WindingRule::NonZero)
            .move_to([0., 0.])
            .cubic_to([100., 0.], [-100., 0.], [0., 0.]);
        let e = flatten(&[looped], [0.; 2], 1.).unwrap();
        assert!(e.iter().any(|e| e.points[0] > 20.));
        assert!(e.iter().any(|e| e.points[0] < -20.));
    }
    #[test]
    fn invalid_commands_and_limits_are_errors() {
        let p = VectorPath::new(WindingRule::NonZero).line_to([1., 2.]);
        assert!(bounds(&[p]).is_err());
        let p = VectorPath::new(WindingRule::NonZero).move_to([f32::NAN, 0.]);
        assert!(bounds(&[p]).is_err());
        let mut p = VectorPath::new(WindingRule::NonZero);
        p.commands = vec![Command::MoveTo([0.; 2]); 4097];
        assert!(bounds(&[p]).is_err());
        assert!(flatten(&[], [0.; 2], 0.).is_err());
    }
    #[test]
    fn multiple_subpaths_close_independently_and_empty_paths_are_empty() {
        let p = VectorPath::new(WindingRule::NonZero)
            .move_to([0., 0.])
            .line_to([1., 0.])
            .move_to([10., 10.])
            .line_to([11., 10.])
            .close();
        let e = flatten(&[p], [0.; 2], 1.).unwrap();
        assert_eq!(e.len(), 4);
        assert!(e.iter().all(|e| (e.points[2] - e.points[0]).abs() <= 1.));
        assert!(
            flatten(&[VectorPath::new(WindingRule::NonZero)], [0.; 2], 1.)
                .unwrap()
                .is_empty()
        );
    }
}

/// Shape of an open contour's ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeCap {
    #[default]
    Butt,
    Square,
    Round,
    ArrowLines,
    ArrowEquilateral,
    TriangleFilled,
    DiamondFilled,
    CircleFilled,
}
/// Join between consecutive path segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}
/// Figma's cap, join and SVG-style miter ratio. Applies to vector strokes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathStroke {
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
}
impl Default for PathStroke {
    fn default() -> Self {
        Self {
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
            miter_limit: 4.,
        }
    }
}

/// Bounds including unfilled/open stroke centerlines.
pub fn stroke_bounds(paths: &[VectorPath]) -> Result<Rect, Error> {
    let mut paths = paths.to_vec();
    for p in &mut paths {
        p.winding_rule = WindingRule::NonZero;
    }
    bounds(&paths)
}

#[cfg(feature = "wgpu")]
mod stroke;
#[cfg(feature = "wgpu")]
pub(crate) use stroke::{primitive_path, stroke_outlines};

pub(crate) fn validate_dash(dash: &[f32]) -> Result<(), Error> {
    if dash.len() > 32 || dash.iter().any(|v| !v.is_finite() || *v < 0. || *v > 1e7) {
        return Err(Error(
            "dash pattern needs at most 32 finite nonnegative lengths",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod dash_tests {
    use super::*;
    #[test]
    fn dash_values_fail_closed_without_renderer_dependencies() {
        for bad in [
            vec![-1., 2.],
            vec![f32::NAN, 2.],
            vec![f32::INFINITY],
            vec![1.; 33],
        ] {
            assert!(validate_dash(&bad).is_err());
            let shape = crate::Shape::vector(Rect::default(), vec![]).dash_pattern(bad);
            assert!(shape.problem().is_some());
        }
        for good in [
            vec![],
            vec![0., 0.],
            vec![12.],
            vec![8., 4., 2.],
            vec![0., 8.],
        ] {
            assert!(validate_dash(&good).is_ok());
        }
    }
}

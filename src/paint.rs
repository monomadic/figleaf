//! Shapes and text described with the values Figma's inspector shows, collected
//! in a [`Canvas`] in paint order.
//!
//! A [`Shape`] lowers to [`Layer`]s: plain coverage problems a renderer can draw
//! without knowing what a stroke alignment or a spread is. The wgpu renderer
//! draws them; a host with another renderer can draw them itself.
use crate::{Color, Rect, Transform, VectorPath, path, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
    pub align: StrokeAlign,
}

/// A drop or inner shadow: Figma's X, Y, Blur and Spread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub inner: bool,
    pub color: Color,
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    /// Drop shadows only. Figma hides the part of a drop shadow under the shape
    /// unless "show behind transparent areas" is ticked; this is that tick.
    pub behind: bool,
}

impl Shadow {
    pub fn drop(color: Color, offset: [f32; 2], blur: f32) -> Self {
        Self {
            inner: false,
            color,
            offset,
            blur,
            spread: 0.,
            behind: false,
        }
    }
    pub fn inner(color: Color, offset: [f32; 2], blur: f32) -> Self {
        Self {
            inner: true,
            ..Self::drop(color, offset, blur)
        }
    }
    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
    pub fn show_behind(mut self) -> Self {
        self.behind = true;
        self
    }
}

/// The most stops a gradient can have.
pub const MAX_STOPS: usize = 64;

/// A gradient stop: Figma's position (0 to 1) and colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    pub offset: f32,
    pub color: Color,
}

/// Where a gradient runs, relative to the shape's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    /// From `start` to `end`, each a fraction of the box (`[0, 0]` is its top
    /// left, `[1, 1]` its bottom right), with bands square to that line.
    Between { start: [f32; 2], end: [f32; 2] },
    /// The first row `[a, b, tx]` of Figma's `gradientTransform`: the
    /// position along the gradient is `a * u + b * v + tx`, where `u` and `v`
    /// are fractions of the box.
    Transform([f32; 3]),
    /// Full Figma transform for an elliptical radial gradient.
    Radial([[f32; 3]; 2]),
    /// Full Figma transform for an angular gradient.
    Angular([[f32; 3]; 2]),
    /// Full Figma transform for a diamond gradient.
    Diamond([[f32; 3]; 2]),
}

/// Figma's gradient paint. Stops blend the way Figma blends them:
/// colour and alpha separately, not premultiplied.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub axis: Axis,
    /// One to [`MAX_STOPS`] stops. Two stops at one offset make a hard edge.
    pub stops: Vec<Stop>,
    /// The paint's own opacity, over the stops' alpha.
    pub opacity: f32,
}

/// Compatibility name for existing linear-gradient callers.
pub type LinearGradient = Gradient;

impl Gradient {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            stops: Vec::new(),
            opacity: 1.,
        }
    }
    pub fn between(start: [f32; 2], end: [f32; 2]) -> Self {
        Self::new(Axis::Between { start, end })
    }
    /// Left to right across the box.
    pub fn horizontal() -> Self {
        Self::new(Axis::Transform([1., 0., 0.]))
    }
    /// Top to bottom down the box.
    pub fn vertical() -> Self {
        Self::new(Axis::Transform([0., 1., 0.]))
    }
    /// From the first row of Figma's `gradientTransform`.
    pub fn transform(row: [f32; 3]) -> Self {
        Self::new(Axis::Transform(row))
    }
    pub fn radial(matrix: [[f32; 3]; 2]) -> Self {
        Self::new(Axis::Radial(matrix))
    }
    pub fn angular(matrix: [[f32; 3]; 2]) -> Self {
        Self::new(Axis::Angular(matrix))
    }
    pub fn diamond(matrix: [[f32; 3]; 2]) -> Self {
        Self::new(Axis::Diamond(matrix))
    }
    pub fn stop(mut self, offset: f32, color: Color) -> Self {
        self.stops.push(Stop { offset, color });
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// The gradient placed on a box: `t = x * axis[0] + y * axis[1] + axis[2]`.
    fn ramp(&self, rect: Rect) -> Option<Ramp> {
        let mut axis_y = [0.; 3];
        let mut kind = 0.;
        let axis = match self.axis {
            Axis::Radial(m) | Axis::Angular(m) | Axis::Diamond(m) => {
                if rect.w <= 0.
                    || rect.h <= 0.
                    || !m.iter().flatten().all(|x| x.is_finite())
                    || (m[0][0] * m[1][1] - m[0][1] * m[1][0]).abs() < 1e-12
                {
                    return None;
                }
                kind = match self.axis {
                    Axis::Radial(_) => 1.,
                    Axis::Angular(_) => 2.,
                    _ => 3.,
                };
                let map = |[a, b, c]: [f32; 3]| {
                    [
                        a / rect.w,
                        b / rect.h,
                        c - a / rect.w * rect.x - b / rect.h * rect.y,
                    ]
                };
                axis_y = map(m[1]);
                map(m[0])
            }
            Axis::Transform([a, b, tx]) => {
                let (ax, ay) = (
                    if a == 0. { 0. } else { a / rect.w },
                    if b == 0. { 0. } else { b / rect.h },
                );
                [ax, ay, tx - ax * rect.x - ay * rect.y]
            }
            Axis::Between { start, end } => {
                let d = [(end[0] - start[0]) * rect.w, (end[1] - start[1]) * rect.h];
                let len2 = d[0] * d[0] + d[1] * d[1];
                let (ax, ay) = (d[0] / len2, d[1] / len2);
                let from = [rect.x + start[0] * rect.w, rect.y + start[1] * rect.h];
                [ax, ay, -ax * from[0] - ay * from[1]]
            }
        };
        if !axis.iter().chain(&axis_y).all(|v| v.is_finite()) {
            return None;
        }
        let mut sorted = self.stops.clone();
        sorted.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let last = *sorted.last()?;
        let mut stops = [last; MAX_STOPS];
        stops[..sorted.len()].copy_from_slice(&sorted);
        Some(Ramp {
            axis,
            axis_y,
            kind,
            count: sorted.len(),
            stops,
        })
    }
}

/// One of a shape's fill or stroke paints.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Color),
    Linear(LinearGradient),
    Gradient(Gradient),
    Image(crate::ImagePaint),
    Pattern(crate::Pattern),
}

impl Paint {
    fn from_gradient(g: Gradient) -> Self {
        match g.axis {
            Axis::Between { .. } | Axis::Transform(_) => Self::Linear(g),
            _ => Self::Gradient(g),
        }
    }
}

/// A rectangle or ellipse with fills, a stroke and effects, a
/// [line](Self::line), or filled [vector paths](Self::vector).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// Local geometry/effects to canvas; clips have an independent transform.
    pub transform: Transform,
    pub clip_transform: Transform,
    /// Additional clips, intersected in canvas space; each has its own transform.
    pub clips: Vec<crate::Clip>,
    /// The shape's box. For a line: from its start (`x`, `y`) to its end
    /// (`x + w`, `y + h`), so `w` and `h` may be negative.
    pub rect: Rect,
    /// Filled vector regions in local coordinates, unioned before painting.
    pub vector_paths: Option<Vec<VectorPath>>,
    /// Inspector parameters lowered to vector paths by the renderer.
    pub primitive: Option<crate::Primitive>,
    /// Optional centerlines independent of resolved fill regions (vector networks).
    pub stroke_paths: Option<Vec<VectorPath>>,
    /// Resolved stroke outlines, already expanded to the stroke width.
    pub stroke_geometry: Option<Vec<VectorPath>>,
    pub path_stroke: path::PathStroke,
    pub dash_pattern: Vec<f32>,
    pub ellipse: bool,
    /// The width of a straight line with round ends, when the shape is one.
    pub line: Option<f32>,
    /// Top-left, top-right, bottom-right, bottom-left. Ignored by ellipses.
    pub radii: [f32; 4],
    /// Painted in order, as Figma lists them bottom to top.
    pub fills: Vec<Paint>,
    /// Missing entries are Normal; non-normal paints need an enclosing Composite.
    pub fill_blend_modes: Vec<crate::BlendMode>,
    pub stroke_blend_modes: Vec<crate::BlendMode>,
    pub stroke: Option<Stroke>,
    /// Bottom-to-top stroke paints. Empty uses `stroke.color` for compatibility.
    /// All paints share the stroke width, alignment and geometry.
    pub stroke_paints: Vec<Paint>,
    /// Individual top, right, bottom, left widths, overriding `stroke.width`.
    pub stroke_weights: Option<[f32; 4]>,
    /// Painted in order: the first is furthest back.
    pub shadows: Vec<Shadow>,
    /// Multiplies every layer. Unlike Figma's layer opacity it does not flatten
    /// the shape first, so a translucent fill shows through a translucent stroke.
    pub opacity: f32,
    /// Figma's "clip content" on an ancestor frame: the frame's own outline.
    pub clip: Option<Outline>,
}

impl Shape {
    pub fn rect(rect: Rect) -> Self {
        Self {
            rect,
            transform: Transform::IDENTITY,
            clip_transform: Transform::IDENTITY,
            clips: Vec::new(),
            vector_paths: None,
            primitive: None,
            stroke_paths: None,
            stroke_geometry: None,
            path_stroke: Default::default(),
            dash_pattern: Vec::new(),
            ellipse: false,
            line: None,
            radii: [0.; 4],
            fills: Vec::new(),
            fill_blend_modes: Vec::new(),
            stroke_blend_modes: Vec::new(),
            stroke: None,
            stroke_paints: Vec::new(),
            stroke_weights: None,
            shadows: Vec::new(),
            opacity: 1.,
            clip: None,
        }
    }
    /// Filled paths, positioned at `rect.x/y`; `rect.w/h` anchor gradients.
    pub fn vector(rect: Rect, paths: Vec<VectorPath>) -> Self {
        Self {
            vector_paths: Some(paths),
            ..Self::rect(rect)
        }
    }
    /// Ellipse sector/ring, or an unfilled arc when inner_radius is one.
    pub fn ellipse_arc(rect: Rect, arc: crate::ArcData) -> Self {
        Self {
            primitive: Some(crate::Primitive::Arc(arc)),
            ..Self::rect(rect)
        }
    }
    /// Regular polygon inscribed in the rectangle, with the first point at top.
    pub fn polygon(rect: Rect, point_count: u32) -> Self {
        Self {
            primitive: Some(crate::Primitive::Polygon { point_count }),
            ..Self::rect(rect)
        }
    }
    /// Alternating outer and inner vertices, inscribed in the rectangle.
    pub fn star(rect: Rect, point_count: u32, inner_radius: f32) -> Self {
        Self {
            primitive: Some(crate::Primitive::Star {
                point_count,
                inner_radius,
            }),
            ..Self::rect(rect)
        }
    }
    /// Use resolved stroke outlines instead of stroking centerlines. As in
    /// Figma's strokeGeometry, inside/outside outlines must already use twice
    /// the nominal width; the renderer clips them against the vector fill.
    /// Requires a vector and a positive-width stroke. Caps, dashes, variable
    /// width profiles and supported vector brushes are baked into these paths;
    /// the nominal width never widens or rescales an already resolved outline.
    /// Keep the source fill domain for inside/outside alignment. Centered
    /// outlines need no filled domain and can accompany unfilled centerlines.
    pub fn stroke_geometry(mut self, paths: Vec<VectorPath>) -> Self {
        self.stroke_geometry = Some(paths);
        self
    }
    pub fn stroke_paths(mut self, paths: Vec<VectorPath>) -> Self {
        self.stroke_paths = Some(paths);
        self
    }
    /// Figma dash/gap lengths, fitted per segment. Empty/all-zero means solid.
    /// Supported on vector strokes and uniform primitive borders without effects.
    pub fn dash_pattern(mut self, pattern: Vec<f32>) -> Self {
        self.dash_pattern = pattern;
        self
    }
    pub fn stroke_cap(mut self, cap: path::StrokeCap) -> Self {
        self.path_stroke.cap = cap;
        self
    }
    pub fn stroke_join(mut self, join: path::StrokeJoin) -> Self {
        self.path_stroke.join = join;
        self
    }
    pub fn stroke_miter_limit(mut self, limit: f32) -> Self {
        self.path_stroke.miter_limit = limit;
        self
    }
    /// The ellipse inscribed in `rect`.
    pub fn ellipse(rect: Rect) -> Self {
        Self {
            ellipse: true,
            ..Self::rect(rect)
        }
    }
    /// A straight line `width` thick with round ends: one segment of a Figma
    /// vector with a centre stroke and round caps, such as an icon's. Its
    /// colour is a solid [`fill`](Self::fill). It takes drop shadows and a
    /// clip; a stroke, a gradient or an inner shadow on it is a problem.
    pub fn line(from: [f32; 2], to: [f32; 2], width: f32) -> Self {
        Self {
            line: Some(width),
            ..Self::rect(Rect::new(
                from[0],
                from[1],
                to[0] - from[0],
                to[1] - from[1],
            ))
        }
    }
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }
    /// Clip coordinates belong to this independent ancestor transform.
    /// Intersect another clip without replacing any earlier clip.
    pub fn intersect_clip(mut self, clip: crate::Clip) -> Self {
        self.clips.push(clip);
        self
    }
    pub fn clip_transformed(mut self, rect: Rect, radius: f32, transform: Transform) -> Self {
        self = self.clip_rounded(rect, radius);
        self.clip_transform = transform;
        self
    }
    pub fn radius(self, radius: f32) -> Self {
        self.radii([radius; 4])
    }
    pub fn radii(mut self, radii: [f32; 4]) -> Self {
        self.radii = radii;
        self
    }
    /// Adds a solid fill over any the shape already has.
    pub fn paint(mut self, paint: Paint) -> Self {
        self.fills.push(paint);
        self
    }
    /// Set the blend mode of the most recently added fill.
    pub fn fill_blend(mut self, mode: crate::BlendMode) -> Self {
        self.fill_blend_modes
            .resize(self.fills.len(), crate::BlendMode::Normal);
        if let Some(last) = self.fill_blend_modes.last_mut() {
            *last = mode;
        }
        self
    }
    pub fn stroke_blend_modes(mut self, modes: Vec<crate::BlendMode>) -> Self {
        self.stroke_blend_modes = modes;
        self
    }
    pub fn fill(mut self, color: Color) -> Self {
        self.fills.push(Paint::Solid(color));
        self
    }
    /// Adds a linear gradient fill over any the shape already has.
    pub fn gradient(mut self, gradient: LinearGradient) -> Self {
        self.fills.push(Paint::from_gradient(gradient));
        self
    }
    pub fn stroke(mut self, width: f32, color: Color, align: StrokeAlign) -> Self {
        self.stroke_weights = None;
        self.stroke_paints.clear();
        self.stroke_blend_modes.clear();
        self.stroke = Some(Stroke {
            width,
            color,
            align,
        });
        self
    }
    /// Replace the stroke color with ordered solid/linear paints. Call after
    /// `stroke` or `strokes`; an empty list restores the legacy stroke color.
    pub fn stroke_paints(mut self, paints: Vec<Paint>) -> Self {
        self.stroke_paints = paints;
        self.stroke_blend_modes.clear();
        self
    }
    /// Set a single linear-gradient stroke.
    pub fn stroke_gradient(self, width: f32, gradient: LinearGradient, align: StrokeAlign) -> Self {
        self.stroke(width, [1.; 4], align)
            .stroke_paints(vec![Paint::from_gradient(gradient)])
    }
    /// Individual frame/rectangle borders, in top, right, bottom, left order.
    /// Adjacent borders join through elliptical inner/outer corner contours.
    pub fn strokes(mut self, widths: [f32; 4], color: Color, align: StrokeAlign) -> Self {
        self = self.stroke(0., color, align);
        self.stroke_weights = Some(widths);
        self
    }
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn clip(self, clip: Rect) -> Self {
        self.clip_rounded(clip, 0.)
    }
    /// Clips to a rounded rectangle, as a frame with a corner radius does.
    pub fn clip_rounded(mut self, rect: Rect, radius: f32) -> Self {
        self.clip_transform = Transform::IDENTITY;
        self.clip = Some(Outline {
            rect,
            radii: [radius; 4],
            radii_y: [radius; 4],
        });
        self
    }

    fn outline(&self, grow: f32) -> Outline {
        let rect = self.rect.inflate(grow);
        // A square corner stays square when the shape grows, as a mitred
        // stroke does; a round one keeps its centre.
        let limit = rect.w.min(rect.h) / 2.;
        let radii = self.radii.map(|r| {
            if r > 0. {
                (r.min(self.rect.w.min(self.rect.h) / 2.) + grow).clamp(0., limit)
            } else {
                0.
            }
        });
        Outline {
            rect,
            radii,
            radii_y: radii,
        }
    }

    fn outline_sides(&self, grow: [f32; 4]) -> Outline {
        if grow.iter().all(|v| *v == grow[0]) {
            return self.outline(grow[0]);
        }
        let [top, right, bottom, left] = grow;
        let rect = Rect::new(
            self.rect.x - left,
            self.rect.y - top,
            (self.rect.w + left + right).max(0.),
            (self.rect.h + top + bottom).max(0.),
        );
        let base = self.radii.map(|r| r.min(self.rect.w.min(self.rect.h) / 2.));
        let xs = [left, right, right, left];
        let ys = [top, top, bottom, bottom];
        let mut radii = [0.; 4];
        let mut radii_y = [0.; 4];
        for i in 0..4 {
            if base[i] > 0. {
                radii[i] = (base[i] + xs[i]).clamp(0., rect.w / 2.);
                radii_y[i] = (base[i] + ys[i]).clamp(0., rect.h / 2.);
            }
        }
        Outline {
            rect,
            radii,
            radii_y,
        }
    }

    /// Why the shape can't be drawn, if it can't.
    pub fn problem(&self) -> Option<&'static str> {
        if self.fill_blend_modes.len() > self.fills.len()
            || self.stroke_blend_modes.len()
                > self
                    .stroke_paints
                    .len()
                    .max(usize::from(self.stroke.is_some()))
            || self
                .fill_blend_modes
                .iter()
                .chain(&self.stroke_blend_modes)
                .any(|m| *m == crate::BlendMode::PassThrough)
        {
            return Some("invalid paint blend modes");
        }
        if self.clips.len() > 128 {
            return Some("clip stack exceeds 128 clips");
        }
        for clip in &self.clips {
            if let Err(e) = clip.validate() {
                return Some(e.0);
            }
        }
        if let Err(e) = self
            .transform
            .inverse()
            .and_then(|_| self.clip_transform.inverse())
        {
            return Some(e.0);
        }
        if let Some(paths) = &self.stroke_geometry {
            if self.vector_paths.is_none()
                || self.stroke.is_none_or(|s| {
                    self.stroke_weights
                        .unwrap_or([s.width; 4])
                        .iter()
                        .all(|w| *w <= 0.)
                })
                || self.stroke_paths.is_some()
                || !self.dash_pattern.is_empty()
                || self.path_stroke != path::PathStroke::default()
            {
                return Some(
                    "resolved stroke geometry requires a vector stroke without centerline styles",
                );
            }
            if paths.iter().any(|p| {
                p.winding_rule == crate::WindingRule::None
                    || p.start_cap.is_some()
                    || p.end_cap.is_some()
            }) {
                return Some(
                    "resolved stroke geometry requires filled outlines without endpoint overrides",
                );
            }
            if let Err(e) = path::bounds(paths) {
                return Some(e.0);
            }
        }
        if let Some(primitive) = self.primitive {
            if let Err(e) = primitive.validate(self.radii[0]) {
                return Some(e.0);
            }
            if self.vector_paths.is_some()
                || self.stroke_paths.is_some()
                || self.line.is_some()
                || self.ellipse
                || self.stroke_weights.is_some()
                || !self.shadows.is_empty()
                || self.radii.iter().any(|r| *r != self.radii[0])
            {
                return Some(
                    "parameterized primitives do not support effects, individual borders or unequal corner radii",
                );
            }
        }
        if let Some(paths) = &self.vector_paths {
            if self.ellipse
                || self.line.is_some()
                || self.radii != [0.; 4]
                || (self.stroke_weights.is_some() && self.stroke_geometry.is_none())
            {
                return Some(
                    "vector paths require resolved outlines for individual borders and cannot have corner radii",
                );
            }
            if let Err(e) = path::bounds(paths) {
                return Some(e.0);
            }
        }
        if self.vector_paths.is_none()
            && self.primitive.is_none()
            && (self.stroke_paths.is_some()
                || (self.dash_pattern.is_empty()
                    && self.path_stroke != path::PathStroke::default()))
        {
            return Some("path stroke settings require a vector");
        }
        if let Some(paths) = &self.stroke_paths
            && let Err(e) = path::stroke_bounds(paths)
        {
            return Some(e.0);
        }
        if !self.path_stroke.miter_limit.is_finite() || self.path_stroke.miter_limit < 1. {
            return Some("stroke miter limit must be finite and at least 1");
        }
        if let Err(e) = path::validate_dash(&self.dash_pattern) {
            return Some(e.0);
        }
        if !self.dash_pattern.is_empty()
            && (self.line.is_some()
                || !self.shadows.is_empty()
                || self
                    .stroke_weights
                    .is_some_and(|w| w.iter().any(|v| *v != w[0])))
        {
            return Some("dashes require uniform borders or vector strokes without effects");
        }
        let mut numbers = self.radii.to_vec();
        numbers.extend(self.clip.map_or([0.; 4], |c| c.radii));
        numbers.extend(self.clip.map_or([0.; 4], |c| c.radii_y));
        numbers.push(self.opacity);
        numbers.extend(self.stroke_weights.into_iter().flatten());
        let mut bad_gradient = false;
        for (index, fill) in self.fills.iter().chain(&self.stroke_paints).enumerate() {
            match fill {
                Paint::Image(p) => {
                    if let Err(e) = crate::image::TexturePaint::Image(p).mapping(self.rect) {
                        return Some(e.0);
                    }
                }
                Paint::Pattern(p) => {
                    if let Err(e) = crate::image::TexturePaint::Pattern(p).mapping(self.rect) {
                        return Some(e.0);
                    }
                }
                Paint::Solid(color) => numbers.extend(color),
                Paint::Linear(g) | Paint::Gradient(g) => {
                    numbers.push(g.opacity);
                    for s in &g.stops {
                        numbers.push(s.offset);
                        numbers.extend(s.color);
                    }
                    let visible = index >= self.fills.len()
                        || self.vector_paths.is_some()
                        || (self.rect.w > 0. && self.rect.h > 0.);
                    bad_gradient |= g.stops.is_empty()
                        || g.stops.len() > MAX_STOPS
                        || (visible && g.ramp(self.rect).is_none());
                }
            }
        }
        if let Some(s) = self.stroke {
            numbers.push(s.width);
            numbers.extend(s.color);
        }
        for s in &self.shadows {
            numbers.extend([s.blur, s.spread, s.offset[0], s.offset[1]]);
            numbers.extend(s.color);
        }
        numbers.extend(self.line);
        // A line's box runs from its start to its end, either way round.
        let rect = match self.line {
            Some(_) => Rect::new(
                self.rect.x,
                self.rect.y,
                self.rect.w.abs(),
                self.rect.h.abs(),
            ),
            None => self.rect,
        };
        if !self.stroke_paints.is_empty() && self.stroke.is_none() {
            Some("stroke paints require stroke geometry")
        } else if self.stroke_paints.len() > 32 {
            Some("stroke paint stack exceeds 32 paints")
        } else if rect.validate().is_err() || self.clip.is_some_and(|c| c.rect.validate().is_err())
        {
            Some("invalid rectangle")
        } else if !numbers.iter().all(|v| v.is_finite()) {
            Some("non-finite shape value")
        } else if self.radii.iter().any(|r| *r < 0.)
            || self.clip.is_some_and(|c| c.radii.iter().any(|r| *r < 0.))
            || self.stroke.is_some_and(|s| s.width < 0.)
            || self
                .stroke_weights
                .is_some_and(|ws| ws.iter().any(|w| *w < 0.))
            || self.shadows.iter().any(|s| s.blur < 0.)
            || self.line.is_some_and(|w| w < 0.)
        {
            Some("negative radius, stroke width or blur")
        } else if self.line.is_some()
            && (self.stroke.is_some()
                || self.shadows.iter().any(|s| s.inner)
                || self.fills.iter().any(|f| !matches!(f, Paint::Solid(_))))
        {
            Some("a line takes solid fills and drop shadows only")
        } else if self.stroke_weights.is_some()
            && (self.ellipse || self.line.is_some() || self.stroke.is_none())
        {
            Some("individual strokes require a rectangle and a stroke")
        } else if self.clip.is_some_and(|c| c.radii != c.radii_y) {
            Some("clip corners must be circular")
        } else if bad_gradient {
            Some("gradient needs 1 to 64 stops and a start apart from its end")
        } else {
            None
        }
    }

    fn path_shadow_bounds(&self, shadow: Shadow) -> Rect {
        let mut bounds =
            path::bounds(self.vector_paths.as_deref().unwrap_or_default()).unwrap_or_default();
        if let Some(paths) = &self.stroke_geometry {
            let other = path::bounds(paths).unwrap_or_default();
            let x = bounds.x.min(other.x);
            let y = bounds.y.min(other.y);
            bounds = Rect::new(
                x,
                y,
                (bounds.x + bounds.w).max(other.x + other.w) - x,
                (bounds.y + bounds.h).max(other.y + other.h) - y,
            );
        } else if let Some(stroke) = self.stroke {
            bounds = bounds.inflate(stroke.width * self.path_stroke.miter_limit.max(6.));
        }
        bounds.translate([self.rect.x, self.rect.y]).inflate(
            shadow.spread.abs()
                + shadow.blur * 1.5
                + shadow.offset[0].abs().max(shadow.offset[1].abs())
                + 1.,
        )
    }

    /// The shape as coverage layers, back to front: drop shadows, fill,
    /// stroke, inner shadows. Nothing for a shape with a [`problem`](Self::problem).
    pub fn layers<'a>(&'a self, out: &mut Vec<Layer<'a>>) {
        if self.problem().is_some() {
            return;
        }
        // How far the stroke reaches outside and inside the shape's edge.
        let widths = self
            .stroke
            .map_or([0.; 4], |s| self.stroke_weights.unwrap_or([s.width; 4]));
        let (outside, inside) = match self.stroke.map(|s| s.align) {
            Some(StrokeAlign::Inside) => ([0.; 4], widths),
            Some(StrokeAlign::Center) => (widths.map(|w| w / 2.), widths.map(|w| w / 2.)),
            Some(StrokeAlign::Outside) => (widths, [0.; 4]),
            None => ([0.; 4], [0.; 4]),
        };
        // Effects follow the fill and the stroke together.
        let silhouette = self.outline_sides(outside);
        let start = out.len();
        let clip = self.clip.map(|c| {
            let limit = c.rect.w.min(c.rect.h) / 2.;
            Outline {
                rect: c.rect,
                radii: c.radii.map(|r| r.min(limit)),
                radii_y: c.radii_y.map(|r| r.min(limit)),
            }
        });
        let layer = |mode, a, b, color: Color, sigma| Layer {
            mode,
            transform: self.transform,
            clip_transform: self.clip_transform,
            vector_paths: self.vector_paths.as_deref(),
            primitive: self.primitive,
            vector_origin: [self.rect.x, self.rect.y],
            vector_stroke: None,
            stroke_geometry: None,
            path_shadow: None,
            shadow_stroke: None,
            shadow_has_fill: false,
            shadow_bounds: None,
            dash_pattern: &self.dash_pattern,
            a,
            b,
            ellipse: self.ellipse,
            line: self.line.is_some(),
            color: [
                color[0],
                color[1],
                color[2],
                color[3] * self.opacity.clamp(0., 1.),
            ],
            ramp: None,
            texture: None,
            sigma,
            clip,
        };
        if let Some(width) = self.line {
            // The segment itself, with half its width in the radii.
            let segment = |grow: f32, by: [f32; 2]| Outline {
                rect: self.rect.translate(by),
                radii: [(width / 2. + grow).max(0.); 4],
                radii_y: [(width / 2. + grow).max(0.); 4],
            };
            for s in &self.shadows {
                let knockout = (!s.behind).then_some(segment(0., [0.; 2]));
                let a = segment(s.spread, s.offset);
                out.push(layer(Mode::Drop, a, knockout, s.color, s.blur / 2.));
            }
            for fill in &self.fills {
                if let Paint::Solid(color) = fill {
                    out.push(layer(Mode::Band, segment(0., [0.; 2]), None, *color, 0.));
                }
            }
            let mut i = 0;
            out.retain(|l| {
                i += 1;
                i <= start || l.color[3] > 0.
            });
            return;
        }
        for s in self.shadows.iter().filter(|s| !s.inner) {
            let mut a = self.outline_sides(outside.map(|w| w + s.spread));
            a.rect = a.rect.translate(s.offset);
            let knockout = (!s.behind).then_some(silhouette);
            let mut l = layer(Mode::Drop, a, knockout, s.color, s.blur / 2.);
            if self.vector_paths.is_some() {
                l.path_shadow = Some(*s);
                l.shadow_stroke = self.stroke;
                l.shadow_has_fill = !self.fills.is_empty();
                l.shadow_bounds = Some(self.path_shadow_bounds(*s));
            }
            out.push(l);
        }
        // Two antialiased edges that meet let the background through.
        // Under an opaque stroke the fill can run on to the middle of
        // the stroke's outer part, where it is hidden, and close the seam.
        let opaque = self.stroke.is_some_and(|s| {
            if self.stroke_paints.is_empty() {
                s.color[3] >= 1.
            } else {
                self.stroke_paints.iter().any(|p| match p {
                    Paint::Image(_) | Paint::Pattern(_) => false,
                    Paint::Solid(c) => c[3] >= 1.,
                    Paint::Linear(g) | Paint::Gradient(g) => {
                        g.opacity >= 1. && g.stops.iter().all(|s| s.color[3] >= 1.)
                    }
                })
            }
        });
        let under = self.outline_sides(if opaque {
            outside.map(|w| w / 2.)
        } else {
            [0.; 4]
        });
        for fill in &self.fills {
            match fill {
                Paint::Image(p) => out.push(Layer {
                    texture: Some(crate::image::TexturePaint::Image(p)),
                    ..layer(Mode::Band, under, None, [1., 1., 1., p.opacity], 0.)
                }),
                Paint::Pattern(p) => out.push(Layer {
                    texture: Some(crate::image::TexturePaint::Pattern(p)),
                    ..layer(Mode::Band, under, None, [1., 1., 1., p.opacity], 0.)
                }),
                Paint::Solid(color) => out.push(layer(Mode::Band, under, None, *color, 0.)),
                Paint::Linear(g) | Paint::Gradient(g) => {
                    // The gradient is placed on the shape's own box, whatever
                    // the stroke adds.
                    if let Some(ramp) = g.ramp(self.rect) {
                        let tint = [1., 1., 1., g.opacity.clamp(0., 1.)];
                        out.push(Layer {
                            ramp: Some(ramp),
                            ..layer(Mode::Band, under, None, tint, 0.)
                        });
                    }
                }
            }
        }
        if let Some(s) = self.stroke.filter(|_| widths.iter().any(|w| *w > 0.)) {
            let hole = self.outline_sides(inside.map(|w| -w));
            let hole = (hole.rect.w > 0. && hole.rect.h > 0.).then_some(hole);
            let mut stroke_layer = layer(Mode::Band, silhouette, hole, s.color, 0.);
            if self.vector_paths.is_some()
                || self.primitive.is_some()
                || !self.dash_pattern.is_empty()
            {
                stroke_layer.vector_paths = self
                    .stroke_paths
                    .as_deref()
                    .or(self.vector_paths.as_deref());
                stroke_layer.vector_stroke = Some((
                    Stroke {
                        width: widths[0],
                        align: if self.primitive.is_some_and(crate::Primitive::is_open_arc) {
                            StrokeAlign::Center
                        } else {
                            s.align
                        },
                        ..s
                    },
                    self.path_stroke,
                ));
            }
            stroke_layer.stroke_geometry = self.stroke_geometry.as_deref();
            if self.stroke_paints.is_empty() {
                out.push(stroke_layer);
            } else {
                for paint in &self.stroke_paints {
                    let texture = match paint {
                        Paint::Image(p) => Some(crate::image::TexturePaint::Image(p)),
                        Paint::Pattern(p) => Some(crate::image::TexturePaint::Pattern(p)),
                        _ => None,
                    };
                    let (color, ramp) = match paint {
                        Paint::Image(p) => ([1., 1., 1., p.opacity], None),
                        Paint::Pattern(p) => ([1., 1., 1., p.opacity], None),
                        Paint::Solid(c) => (*c, None),
                        Paint::Linear(g) | Paint::Gradient(g) => {
                            ([1., 1., 1., g.opacity.clamp(0., 1.)], g.ramp(self.rect))
                        }
                    };
                    out.push(Layer {
                        color: [
                            color[0],
                            color[1],
                            color[2],
                            color[3] * self.opacity.clamp(0., 1.),
                        ],
                        ramp,
                        texture,
                        ..stroke_layer
                    });
                }
            }
        }
        for s in self.shadows.iter().filter(|s| s.inner) {
            let mut a = self.outline_sides(outside.map(|w| w - s.spread));
            a.rect = a.rect.translate(s.offset);
            let mut l = layer(Mode::Inner, a, Some(silhouette), s.color, s.blur / 2.);
            if self.vector_paths.is_some() {
                l.path_shadow = Some(*s);
                l.shadow_stroke = self.stroke;
                l.shadow_has_fill = !self.fills.is_empty();
                l.shadow_bounds = Some(self.path_shadow_bounds(*s));
            }
            out.push(l);
        }
        let mut i = 0;
        out.retain(|l| {
            i += 1;
            i <= start || l.color[3] > 0.
        });
    }
}

/// A rounded rectangle, or the ellipse inscribed in `rect`. In a
/// [line](Layer::line) layer: the segment from (`x`, `y`) to (`x + w`,
/// `y + h`), with half its width in `radii[0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub rect: Rect,
    /// Horizontal corner radii: top-left, top-right, bottom-right, bottom-left.
    pub radii: [f32; 4],
    /// Vertical corner radii; equal to `radii` for circular corners.
    pub radii_y: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Inside `a` and outside `b`: a fill, or a stroke with `b` as its hole.
    Band,
    /// `a` blurred by `sigma`, hidden inside `b` when there is one.
    Drop,
    /// Inside `b`, wherever blurred `a` does not reach.
    Inner,
}

/// A linear gradient placed in logical units: the position along it at
/// `[x, y]` is `x * axis[0] + y * axis[1] + axis[2]`, held to the first stop's
/// offset below and the last one's above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    pub axis: [f32; 3],
    pub axis_y: [f32; 3],
    pub kind: f32,
    pub count: usize,
    /// In offset order. Unused places repeat the last stop.
    pub stops: [Stop; MAX_STOPS],
}

/// One tinted coverage layer, in logical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer<'a> {
    pub transform: Transform,
    pub clip_transform: Transform,
    /// When present, union these filled regions instead of the `a`/`b` outlines.
    /// The paths borrow the source shape and are positioned by `vector_origin`.
    pub vector_paths: Option<&'a [VectorPath]>,
    pub primitive: Option<crate::Primitive>,
    pub vector_origin: [f32; 2],
    pub vector_stroke: Option<(Stroke, path::PathStroke)>,
    /// Pre-expanded stroke outlines; fill paths provide the alignment domain.
    pub stroke_geometry: Option<&'a [VectorPath]>,
    /// Vector shadow parameters; distance is evaluated before the layer transform.
    pub path_shadow: Option<Shadow>,
    /// Stroke alignment contributing to a vector shadow silhouette.
    pub shadow_stroke: Option<Stroke>,
    /// Whether filled regions contribute to the vector shadow silhouette.
    pub shadow_has_fill: bool,
    /// Conservative local bounds including vector shadow spread and blur.
    pub shadow_bounds: Option<Rect>,
    pub dash_pattern: &'a [f32],
    pub mode: Mode,
    pub a: Outline,
    pub b: Option<Outline>,
    pub ellipse: bool,
    /// `a` and `b` are round-ended line segments, not boxes.
    pub line: bool,
    /// The layer's colour. With a `ramp` the colour comes from the ramp and
    /// only this alpha is used, as an opacity over it.
    pub color: Color,
    pub ramp: Option<Ramp>,
    pub texture: Option<crate::image::TexturePaint<'a>>,
    /// Gaussian standard deviation: half of Figma's blur value.
    pub sigma: f32,
    pub clip: Option<Outline>,
}

impl Layer<'_> {
    /// Everything the layer can touch, before clipping.
    pub fn extent(&self) -> Rect {
        self.transform.bounds(self.local_extent())
    }
    fn local_extent(&self) -> Rect {
        if let Some(bounds) = self.shadow_bounds {
            return bounds;
        }
        if let Some(paths) = self.stroke_geometry {
            return path::bounds(paths)
                .ok()
                .unwrap_or_default()
                .translate(self.vector_origin);
        }
        if self.primitive.is_some() {
            return self.a.rect.inflate(
                self.vector_stroke
                    .map_or(0., |(s, style)| s.width * style.miter_limit.max(6.)),
            );
        }
        if let Some(paths) = self.vector_paths {
            if let Some((stroke, style)) = self.vector_stroke {
                return path::stroke_bounds(paths)
                    .unwrap_or_default()
                    .translate(self.vector_origin)
                    .inflate(stroke.width * style.miter_limit.max(6.));
            }
            return path::bounds(paths)
                .unwrap_or_default()
                .translate(self.vector_origin);
        }
        let a = match self.line {
            true => {
                let r = self.a.rect;
                Rect::new(r.x.min(r.x + r.w), r.y.min(r.y + r.h), r.w.abs(), r.h.abs())
                    .inflate(self.a.radii[0])
            }
            false => self.a.rect,
        };
        match (self.mode, self.b) {
            (Mode::Inner, Some(b)) => b.rect,
            (Mode::Drop, _) => a.inflate(3. * self.sigma),
            _ => a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// Text in a box: Figma's text layer with a fixed line height. One line,
/// unless it [wraps](Self::wrap).
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub transform: Transform,
    pub clip_transform: Transform,
    pub clips: Vec<crate::Clip>,
    pub string: String,
    /// Explicit styled runs. When nonempty, the plain string must be empty.
    pub runs: Vec<text::TextRun>,
    pub text_decoration: text::TextDecoration,
    pub text_case: text::TextCase,
    pub open_type_features: Vec<text::OpenTypeFeature>,
    pub list_options: text::ListOptions,
    pub indentation: usize,
    pub list_spacing: f32,
    pub hanging_list: bool,
    pub text_truncation: text::TextTruncation,
    pub max_lines: Option<usize>,
    pub paragraph_spacing: f32,
    pub paragraph_indent: f32,
    /// The line is `line_height` tall and placed in this box by the alignments.
    pub rect: Rect,
    /// Index into the fonts the text backend was built with.
    pub font: usize,
    pub size: f32,
    pub line_height: f32,
    /// Figma's letter spacing in pixels.
    pub letter_spacing: f32,
    pub color: Color,
    /// Paint coordinates cover the whole text box, including wrapped lines.
    pub paints: Option<Vec<Paint>>,
    pub stroke: Option<Stroke>,
    pub stroke_paints: Vec<Paint>,
    pub stroke_blend_modes: Vec<crate::BlendMode>,
    pub align: Align,
    pub vertical_align: Align,
    /// Figma's inner shadow effect on a text layer: colour, X and Y, blur.
    /// Spread is not supported on text.
    pub inner_shadow: Option<(Color, [f32; 2], f32)>,
    /// Figma's drop shadow effect on a text layer: colour, X and Y, blur.
    /// The offset lands on whole physical pixels.
    pub drop_shadow: Option<(Color, [f32; 2], f32)>,
    /// Break lines to fit the box's width, and at `\n` in the string.
    pub wrap: bool,
    pub clip: Option<Rect>,
}

impl Text {
    /// Black 12/16 text in font 0, top left in `rect`.
    pub fn new(string: impl Into<String>, rect: Rect) -> Self {
        Self {
            transform: Transform::IDENTITY,
            clip_transform: Transform::IDENTITY,
            clips: Vec::new(),
            string: string.into(),
            runs: Vec::new(),
            text_decoration: text::TextDecoration::None,
            text_case: text::TextCase::Original,
            open_type_features: Vec::new(),
            list_options: text::ListOptions::None,
            indentation: 0,
            list_spacing: 0.,
            hanging_list: false,
            text_truncation: text::TextTruncation::Disabled,
            max_lines: None,
            paragraph_spacing: 0.,
            paragraph_indent: 0.,
            rect,
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
            color: [0., 0., 0., 1.],
            paints: None,
            stroke: None,
            stroke_paints: Vec::new(),
            stroke_blend_modes: Vec::new(),
            align: Align::Start,
            vertical_align: Align::Start,
            inner_shadow: None,
            drop_shadow: None,
            wrap: false,
            clip: None,
        }
    }
    /// Mixed-style text. Each run supplies its own font, size, line height,
    /// tracking and color; layer alignment, wrapping and clips still apply.
    pub fn rich(runs: Vec<text::TextRun>, rect: Rect) -> Self {
        let mut text = Self::new("", rect);
        text.runs = runs;
        text
    }
    /// Ending fits an ellipsis into the last visible line; Disabled preserves overflow.
    pub fn text_truncation(mut self, value: text::TextTruncation) -> Self {
        self.text_truncation = value;
        self
    }
    /// Limit visible lines when text truncation is Ending. Zero is invalid.
    pub fn max_lines(mut self, value: usize) -> Self {
        self.max_lines = Some(value);
        self
    }
    pub fn open_type_features(mut self, features: Vec<text::OpenTypeFeature>) -> Self {
        self.open_type_features = features;
        self
    }
    pub fn list_options(mut self, options: text::ListOptions) -> Self {
        self.list_options = options;
        if options != text::ListOptions::None && self.indentation == 0 {
            self.indentation = 1;
        }
        self
    }
    pub fn indentation(mut self, level: usize) -> Self {
        self.indentation = level;
        self
    }
    pub fn list_spacing(mut self, spacing: f32) -> Self {
        self.list_spacing = spacing;
        self
    }
    pub fn hanging_list(mut self, hanging: bool) -> Self {
        self.hanging_list = hanging;
        self
    }
    pub fn text_decoration(mut self, value: text::TextDecoration) -> Self {
        self.text_decoration = value;
        self
    }
    pub fn text_case(mut self, value: text::TextCase) -> Self {
        self.text_case = value;
        self
    }
    /// Extra space between paragraphs, excluding soft wraps and the final tail.
    pub fn paragraph_spacing(mut self, value: f32) -> Self {
        self.paragraph_spacing = value;
        self
    }
    /// First-line offset from the left edge; continuation lines use the full width.
    pub fn paragraph_indent(mut self, value: f32) -> Self {
        self.paragraph_indent = value;
        self
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn needs_layout(&self) -> bool {
        !self.open_type_features.is_empty()
            || self.list_options != text::ListOptions::None
            || self.indentation != 0
            || self.list_spacing != 0.
            || self.hanging_list
            || self.paints.is_some()
            || self.stroke.is_some()
            || !self.stroke_paints.is_empty()
            || !self.stroke_blend_modes.is_empty()
            || !self.runs.is_empty()
            || !self.string.is_ascii()
            || self.text_truncation != text::TextTruncation::Disabled
            || self.max_lines.is_some()
            || self.text_decoration != text::TextDecoration::None
            || self.text_case != text::TextCase::Original
            || self.line_height == 0.
            || self.paragraph_spacing != 0.
            || self.paragraph_indent != 0.
    }
    /// Explicit fills; an empty stack supports stroke-only text.
    /// For rich text, set paints on each run instead.
    pub fn paints(mut self, paints: Vec<Paint>) -> Self {
        self.paints = Some(paints);
        self
    }
    pub fn stroke(mut self, width: f32, color: Color, align: StrokeAlign) -> Self {
        self.stroke = Some(Stroke {
            width,
            color,
            align,
        });
        self.stroke_paints.clear();
        self.stroke_blend_modes.clear();
        self
    }
    pub fn stroke_paints(mut self, paints: Vec<Paint>) -> Self {
        self.stroke_paints = paints;
        self.stroke_blend_modes.clear();
        self
    }
    pub fn stroke_blend_modes(mut self, modes: Vec<crate::BlendMode>) -> Self {
        self.stroke_blend_modes = modes;
        self
    }
    pub fn font(mut self, font: usize) -> Self {
        self.font = font;
        self
    }
    /// Font size and line height, as in Figma's "13 / 16".
    pub fn size(mut self, size: f32, line_height: f32) -> Self {
        (self.size, self.line_height) = (size, line_height);
        self
    }
    /// Use the selected font’s natural line height (stored as zero).
    pub fn auto_line_height(mut self) -> Self {
        self.line_height = 0.;
        self
    }
    pub fn inner_shadow(mut self, color: Color, offset: [f32; 2], blur: f32) -> Self {
        self.inner_shadow = Some((color, offset, blur));
        self
    }
    pub fn drop_shadow(mut self, color: Color, offset: [f32; 2], blur: f32) -> Self {
        self.drop_shadow = Some((color, offset, blur));
        self
    }
    /// Figma's fixed-width text box: lines break at spaces to fit the box's
    /// width, and at every `\n`. The block of lines is placed in the box by
    /// the vertical alignment, and each line by the horizontal one.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
    pub fn letter_spacing(mut self, pixels: f32) -> Self {
        self.letter_spacing = pixels;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn align(mut self, horizontal: Align, vertical: Align) -> Self {
        (self.align, self.vertical_align) = (horizontal, vertical);
        self
    }
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }
    /// Intersect another clip without replacing any earlier clip.
    pub fn intersect_clip(mut self, clip: crate::Clip) -> Self {
        self.clips.push(clip);
        self
    }
    pub fn clip_transformed(mut self, clip: Rect, transform: Transform) -> Self {
        self.clip = Some(clip);
        self.clip_transform = transform;
        self
    }
    pub fn clip(mut self, clip: Rect) -> Self {
        self.clip_transform = Transform::IDENTITY;
        self.clip = Some(clip);
        self
    }
    pub fn style(&self) -> text::Style {
        text::Style {
            font: self.font,
            size: self.size,
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
        }
    }
    /// Top-left of a line `advance` wide, in logical units.
    /// Requires a resolved explicit height and zero paragraph spacing/indentation;
    /// use `text::layout_text` for paragraph-aware and truncated placement.
    pub fn origin(&self, advance: f32) -> [f32; 2] {
        self.line_origin(advance, 0, 1)
    }
    /// Top-left of line `index` of `count`, `advance` wide.
    pub fn line_origin(&self, advance: f32, index: usize, count: usize) -> [f32; 2] {
        let block = self.line_height * count as f32;
        let place = |align, spare: f32| match align {
            Align::Start => 0.,
            Align::Center => spare / 2.,
            Align::End => spare,
        };
        [
            self.rect.x + place(self.align, self.rect.w - advance),
            self.rect.y
                + place(self.vertical_align, self.rect.h - block)
                + self.line_height * index as f32,
        ]
    }
}

// Keep ordinary shapes inline: boxing would add an allocation to every paint item.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Composite(crate::Composite),
    Masked(crate::Masked),
    Shape(Shape),
    Text(Text),
}

/// A frame's shapes and text, back to front. Rebuild it when the UI changes;
/// the renderer uploads only when the result differs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Canvas {
    items: Vec<Item>,
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn shape(&mut self, shape: Shape) -> &mut Self {
        self.items.push(Item::Shape(shape));
        self
    }
    pub fn text(&mut self, text: Text) -> &mut Self {
        self.items.push(Item::Text(text));
        self
    }
    /// Composite the content once through this mask. The mask itself is not painted.
    pub fn masked(&mut self, mask: crate::Mask, content: Canvas) -> &mut Self {
        self.items
            .push(Item::Masked(crate::Masked { mask, content }));
        self
    }
    pub fn composite(&mut self, composite: crate::Composite) -> &mut Self {
        self.items.push(Item::Composite(composite));
        self
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn individual_widths_preserve_uniform_strokes_and_reject_invalid_uses() {
        let shape = Shape::rect(Rect::new(10., 20., 60., 40.)).radius(12.);
        for align in [
            StrokeAlign::Inside,
            StrokeAlign::Center,
            StrokeAlign::Outside,
        ] {
            assert_eq!(
                layers(&shape.clone().stroke(3., hex(0xFF0000), align)),
                layers(&shape.clone().strokes([3.; 4], hex(0xFF0000), align)),
            );
        }
        let invalid = shape
            .clone()
            .strokes([0., f32::NAN, 1., 0.], hex(0), StrokeAlign::Inside);
        assert!(invalid.problem().is_some());
        assert!(layers(&invalid).is_empty());
        let mut ellipse = shape.strokes([1., 2., 3., 4.], hex(0), StrokeAlign::Inside);
        ellipse.ellipse = true;
        assert_eq!(
            ellipse.problem(),
            Some("individual strokes require a rectangle and a stroke")
        );
    }

    fn layers(s: &Shape) -> Vec<Layer<'_>> {
        let mut out = Vec::new();
        s.layers(&mut out);
        out
    }

    #[test]
    fn stroke_alignment_sets_the_ring() {
        let base = Shape::rect(Rect::new(10., 10., 40., 20.)).radius(6.);
        let ring = |align| {
            let shape = base.clone().stroke(2., hex(0), align);
            let l = layers(&shape);
            (l[0].a, l[0].b.unwrap())
        };
        let (outer, hole) = ring(StrokeAlign::Inside);
        assert_eq!(outer.rect, Rect::new(10., 10., 40., 20.));
        assert_eq!(
            (hole.rect, hole.radii[0]),
            (Rect::new(12., 12., 36., 16.), 4.)
        );
        let (outer, hole) = ring(StrokeAlign::Center);
        assert_eq!(
            (outer.rect, outer.radii[0]),
            (Rect::new(9., 9., 42., 22.), 7.)
        );
        assert_eq!(hole.rect, Rect::new(11., 11., 38., 18.));
        let (outer, hole) = ring(StrokeAlign::Outside);
        assert_eq!(
            (outer.rect, outer.radii[0]),
            (Rect::new(8., 8., 44., 24.), 8.)
        );
        assert_eq!(hole.rect, Rect::new(10., 10., 40., 20.));
    }

    #[test]
    fn square_corners_stay_square_and_thick_strokes_lose_the_hole() {
        let s = Shape::rect(Rect::new(0., 0., 10., 10.));
        let outside = s.clone().stroke(2., hex(0), StrokeAlign::Outside);
        let l = layers(&outside);
        assert_eq!(l[0].a.radii, [0.; 4]);
        let inside = s.stroke(5., hex(0), StrokeAlign::Inside);
        let l = layers(&inside);
        assert_eq!(l[0].b, None);
    }

    #[test]
    fn oversized_radii_make_a_pill() {
        let s = Shape::rect(Rect::new(0., 0., 44., 24.)).radius(999.);
        let shape = s.fill(hex(0));
        let l = layers(&shape);
        assert_eq!(l[0].a.radii, [12.; 4]);
    }

    #[test]
    fn layers_come_back_to_front_and_shadows_follow_the_stroke() {
        let s = Shape::rect(Rect::new(0., 0., 20., 20.))
            .radius(4.)
            .fill(hex(0xFFFFFF))
            .stroke(1., hex(0), StrokeAlign::Outside)
            .shadow(Shadow::inner(hex(0), [0., 1.], 2.))
            .shadow(Shadow::drop(hex(0), [0., 2.], 8.).spread(1.))
            .opacity(0.5);
        let l = layers(&s);
        assert_eq!(
            l.iter().map(|l| l.mode).collect::<Vec<_>>(),
            [Mode::Drop, Mode::Band, Mode::Band, Mode::Inner]
        );
        // The fill runs half way under the opaque outside stroke.
        assert_eq!(l[1].a.rect, Rect::new(-0.5, -0.5, 21., 21.));
        assert_eq!(l[1].a.radii, [4.5; 4]);
        // Drop shadow: stroke 1 + spread 1 outwards, moved down 2, sigma 4.
        assert_eq!(l[0].a.rect, Rect::new(-2., 0., 24., 24.));
        assert_eq!((l[0].sigma, l[0].color[3]), (4., 0.5));
        assert_eq!(l[0].b.unwrap().rect, Rect::new(-1., -1., 22., 22.));
        assert_eq!(l[0].extent(), Rect::new(-14., -12., 48., 48.));
        assert_eq!(l[3].b.unwrap().rect, Rect::new(-1., -1., 22., 22.));
        assert_eq!(l[3].a.rect, Rect::new(-1., 0., 22., 22.));
    }

    #[test]
    fn fills_stack_and_gradients_land_on_the_box() {
        let r = Rect::new(10., 20., 40., 10.);
        let s = Shape::rect(r)
            .fill(hex(0xFF0000))
            .gradient(
                LinearGradient::horizontal()
                    .stop(1., hex(0xFFFFFF))
                    .stop(0., hex(0))
                    .opacity(0.5),
            )
            .stroke(2., hex(0), StrokeAlign::Outside);
        let l = layers(&s);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].ramp, l[0].color), (None, hex(0xFF0000)));
        let ramp = l[1].ramp.unwrap();
        // Position 0 at the box's left edge and 1 at its right, not the stroke's.
        let t = |x: f32, y: f32| x * ramp.axis[0] + y * ramp.axis[1] + ramp.axis[2];
        assert_eq!((t(10., 0.), t(50., 99.)), (0., 1.));
        assert_eq!((l[1].color[3], l[1].a), (0.5, l[0].a));
        // Stops come out sorted, and the spare places repeat the last one.
        assert_eq!((ramp.stops[0].color, ramp.stops[1].offset), (hex(0), 1.));
        assert_eq!(ramp.stops[7], ramp.stops[1]);

        // Handles: top left to bottom right of a 40x10 box, square to the line.
        let g = LinearGradient::between([0., 0.], [1., 1.]).stop(0., hex(0));
        let ramp = layers(&Shape::rect(r).gradient(g))[0].ramp.unwrap();
        let t = |x: f32, y: f32| x * ramp.axis[0] + y * ramp.axis[1] + ramp.axis[2];
        assert!(t(10., 20.).abs() < 1e-5 && (t(50., 30.) - 1.).abs() < 1e-5);
        // A point square to the line from its start is still at 0.
        assert!(t(10. - 10., 20. + 40.).abs() < 1e-5);
    }

    #[test]
    fn bad_gradients_are_problems() {
        let r = Rect::new(0., 0., 10., 10.);
        let none = Shape::rect(r).gradient(LinearGradient::vertical());
        assert!(none.problem().is_some() && layers(&none).is_empty());
        let mut many = LinearGradient::vertical();
        for i in 0..=MAX_STOPS {
            many = many.stop(i as f32 / 8., hex(0));
        }
        assert!(Shape::rect(r).gradient(many).problem().is_some());
        let flat = LinearGradient::between([0.5, 0.5], [0.5, 0.5]).stop(0., hex(0));
        assert!(Shape::rect(r).gradient(flat).problem().is_some());
    }

    #[test]
    fn a_line_is_a_segment_with_its_shadow_behind() {
        let s = Shape::line([7., 0.], [0., 7.], 1.5)
            .fill(hex(0xFFFFFF))
            .shadow(Shadow::drop(hex(0), [0., 1.], 2.));
        let l = layers(&s);
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|l| l.line));
        assert_eq!(
            (l[1].a.rect, l[1].a.radii[0]),
            (Rect::new(7., 0., -7., 7.), 0.75)
        );
        assert_eq!(l[1].extent(), Rect::new(-0.75, -0.75, 8.5, 8.5));
        // The shadow is the segment moved down, hidden under the line itself.
        assert_eq!(l[0].a.rect, Rect::new(7., 1., -7., 7.));
        assert_eq!((l[0].mode, l[0].b), (Mode::Drop, Some(l[1].a)));
        assert_eq!(l[0].extent(), Rect::new(-3.75, -2.75, 14.5, 14.5));
        let stroked = s.stroke(1., hex(0), StrokeAlign::Center);
        assert!(stroked.problem().is_some() && layers(&stroked).is_empty());
    }

    #[test]
    fn invisible_and_invalid_shapes_draw_nothing() {
        let r = Rect::new(0., 0., 10., 10.);
        assert!(layers(&Shape::rect(r)).is_empty());
        assert!(layers(&Shape::rect(r).fill(hex(0)).opacity(0.)).is_empty());
        let bad = Shape::rect(r).fill(hex(0)).radius(f32::NAN);
        assert!(bad.problem().is_some() && layers(&bad).is_empty());
    }

    #[test]
    fn resolved_stroke_geometry_rejects_conflicting_styles() {
        use crate::{StrokeCap, WindingRule};
        let rect = Rect::new(0., 0., 20., 20.);
        let p = VectorPath::new(WindingRule::NonZero)
            .move_to([0., 0.])
            .line_to([20., 0.])
            .line_to([0., 20.])
            .close();
        let s = Shape::vector(rect, vec![p.clone()])
            .stroke(2., hex(0), StrokeAlign::Center)
            .stroke_geometry(vec![p.clone()]);
        assert_eq!(s.problem(), None);
        assert_eq!(layers(&s).len(), 1);
        for bad in [
            Shape::rect(rect)
                .stroke(2., hex(0), StrokeAlign::Center)
                .stroke_geometry(vec![p.clone()]),
            Shape::vector(rect, vec![p.clone()]).stroke_geometry(vec![p.clone()]),
            s.clone().stroke_cap(StrokeCap::Round),
            s.clone().dash_pattern(vec![2., 2.]),
            s.clone().stroke_paths(vec![p.clone()]),
            s.clone()
                .stroke_geometry(vec![VectorPath::new(WindingRule::None)]),
            s.clone()
                .stroke_geometry(vec![p.start_cap(StrokeCap::Round)]),
        ] {
            assert!(bad.problem().is_some());
            assert!(layers(&bad).is_empty());
        }
    }

    #[test]
    fn text_sits_in_its_box() {
        let t = Text::new("On", Rect::new(10., 10., 100., 40.)).size(13., 16.);
        assert_eq!(t.origin(20.), [10., 10.]);
        let t = t.align(Align::Center, Align::Center);
        assert_eq!(t.origin(20.), [50., 22.]);
        assert_eq!(
            t.clone().align(Align::End, Align::End).origin(20.),
            [90., 34.]
        );
        // Two lines of 16 are centred as a block of 32 in the 40-tall box.
        assert_eq!(t.line_origin(20., 1, 2), [50., 30.]);
    }
}

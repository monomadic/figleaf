//! Figma auto layout solver: fixed, hug and fill sizing, horizontal and vertical
//! stacks, wrapping, padding, gaps, alignment, absolute children and min/max
//! limits.
//!
//! Build a [`Tree`] from the values in Figma's auto layout panel, solve it for
//! a size, and draw each thing at the rectangle it gets. Borrowed from figui,
//! where it was checked against positions Figma recorded.
//!
//! Not covered: baseline alignment, rotated children, vertical wrapping, grid
//! layout, and constraints other than "keep the authored
//! offset". Intrinsic leaves can use [`Tree::solve_measured`] with a host-owned
//! measurement callback (for example [`crate::Text::measure`]).
//!
//! Widths are solved first and heights second. The only thing heights take
//! from widths is where wrapping stacks break and the measured height of leaves.
use crate::{Error, Rect, StrokeAlign};
use std::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    Fixed,
    /// Shrink to content. Auto-layout frames hug their children; leaves use
    /// authored sizes unless a `solve_measured` callback supplies their size.
    Hug,
    /// Take the space the parent auto layout frame gives.
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Min,
    Center,
    Max,
    /// Primary axis only: first and last children touch the padding and the
    /// rest are spread evenly. The item spacing is ignored.
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

/// Figma's distribution of wrapped tracks along the counter axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    Auto,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoLayout {
    pub direction: Direction,
    /// Left, top, right, bottom.
    pub padding: [f32; 4],
    pub spacing: f32,
    pub primary_align: Align,
    pub counter_align: Align,
    /// Figma's "wrap": children that don't fit start a new line. Horizontal
    /// stacks only.
    pub wrap: bool,
    /// The gap between wrapped lines.
    pub counter_spacing: f32,
    pub counter_align_content: AlignContent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Index of the parent, which must come earlier. `None` only for the root.
    pub parent: Option<usize>,
    /// Authored size: used by fixed sizing, and by nodes that can't hug.
    pub size: [f32; 2],
    /// Authored position in the parent, used for nodes outside a stack's flow.
    pub offset: [f32; 2],
    /// Horizontal, vertical.
    pub sizing: [Sizing; 2],
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Takes part in its parent's stack: visible and not absolutely positioned.
    pub in_flow: bool,
    pub layout: Option<AutoLayout>,
    /// Visible stroke widths: top, right, bottom, left. Use zero when all
    /// stroke paints are hidden or absent; visible zero-opacity paints count.
    pub stroke_weights: [f32; 4],
    pub stroke_align: StrokeAlign,
    /// Reserve the frame's inside strokes in addition to padding.
    pub strokes_included_in_layout: bool,
}

impl Node {
    pub fn at(parent: Option<usize>, offset: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            parent,
            size,
            offset,
            sizing: [Sizing::Fixed; 2],
            min: [0.; 2],
            max: [f32::INFINITY; 2],
            in_flow: true,
            layout: None,
            stroke_weights: [0.; 4],
            stroke_align: StrokeAlign::Inside,
            strokes_included_in_layout: false,
        }
    }
}

struct Solver<'a> {
    nodes: &'a [Node],
    children: Vec<Vec<usize>>,
    measured: Vec<Option<f32>>,
    axis: usize,
    /// The lines of each wrapping stack, set while widths are arranged and
    /// read while heights are.
    lines: Vec<Option<Vec<Vec<usize>>>>,
}

impl<'a> Solver<'a> {
    fn new(
        nodes: &'a [Node],
        children: &[Vec<usize>],
        axis: usize,
        lines: Vec<Option<Vec<Vec<usize>>>>,
    ) -> Self {
        Self {
            nodes,
            children: children.to_vec(),
            measured: vec![None; nodes.len()],
            axis,
            lines,
        }
    }

    fn clamp(&self, i: usize, v: f32) -> f32 {
        let n = &self.nodes[i];
        v.min(n.max[self.axis]).max(n.min[self.axis])
    }

    /// Size along the axis before the parent stretches or fills it.
    fn measure(&mut self, i: usize) -> f32 {
        if let Some(v) = self.measured[i] {
            return v;
        }
        let n = &self.nodes[i];
        let v = match (n.sizing[self.axis], n.layout) {
            (Sizing::Hug, Some(l)) => self.hug(i, l),
            _ => n.size[self.axis],
        };
        let v = self.clamp(i, v);
        self.measured[i] = Some(v);
        v
    }

    fn hug(&mut self, i: usize, l: AutoLayout) -> f32 {
        let (start, end) = pads(&self.nodes[i], l, self.axis);
        let flow: Vec<usize> = self.flow(i);
        // Figma output: a hugging frame with nothing in its flow keeps its size.
        if flow.is_empty() {
            return self.nodes[i].size[self.axis];
        }
        let content = if primary(l) == self.axis {
            let sum: f32 = flow.iter().map(|&c| self.measure(c)).sum();
            sum + l.spacing * flow.len().saturating_sub(1) as f32
        } else {
            let lines = self.lines_of(i, l, &flow);
            let heights: f32 = lines.iter().map(|line| self.track_thickness(line, l)).sum();
            heights
                + if l.wrap && l.counter_align_content == AlignContent::SpaceBetween {
                    0.
                } else {
                    l.counter_spacing
                } * lines.len().saturating_sub(1) as f32
        };
        content + start + end
    }

    /// The lines of a stack across its direction: one, unless it wrapped.
    fn lines_of(&self, i: usize, l: AutoLayout, flow: &[usize]) -> Vec<Vec<usize>> {
        match &self.lines[i] {
            Some(lines) if l.wrap => lines.clone(),
            _ => vec![flow.to_vec()],
        }
    }

    /// How much room a line needs across the stack's direction.
    fn thickness(&mut self, line: &[usize]) -> f32 {
        // Children that fill the counter axis follow the line, so they
        // don't set its size unless nothing else does.
        let fixed: Vec<usize> = line
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].sizing[self.axis] != Sizing::Fill)
            .collect();
        let from = if fixed.is_empty() { line } else { &fixed };
        let measured = from.iter().map(|&c| self.measure(c)).fold(0., f32::max);
        // A fill child does not contribute its authored size beside fixed/hug
        // siblings, but its minimum still constrains the hugging parent.
        line.iter()
            .map(|&c| self.nodes[c].min[self.axis])
            .fold(measured, f32::max)
    }

    fn track_thickness(&mut self, line: &[usize], l: AutoLayout) -> f32 {
        if l.wrap
            && line
                .iter()
                .all(|&c| self.nodes[c].sizing[self.axis] == Sizing::Fill)
        {
            // Figma ignores authored fill heights when sizing a wrapped track.
            line.iter()
                .map(|&c| self.nodes[c].min[self.axis])
                .fold(1., f32::max)
        } else {
            self.thickness(line)
        }
    }

    /// Breaks a wrapping stack's children into lines that fit `inner`. A
    /// child that fills counts as its minimum size here and then shares what
    /// its line has left.
    fn wrapped(&mut self, flow: &[usize], inner: f32, l: AutoLayout) -> Vec<Vec<usize>> {
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut used = 0.;
        for &c in flow {
            let size = if self.nodes[c].sizing[self.axis] == Sizing::Fill {
                self.clamp(c, 0.)
            } else {
                self.measure(c)
            };
            match lines.last_mut() {
                Some(line) if used + l.spacing + size <= inner + 0.001 => {
                    line.push(c);
                    used += l.spacing + size;
                }
                _ => {
                    lines.push(vec![c]);
                    used = size;
                }
            }
        }
        lines
    }

    fn flow(&self, i: usize) -> Vec<usize> {
        self.children[i]
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].in_flow)
            .collect()
    }

    /// Places the children of `i`, whose size on this axis is `size`.
    fn arrange(&mut self, i: usize, size: f32, out: &mut [[f32; 2]]) {
        let Some(l) = self.nodes[i].layout else {
            for c in self.children[i].clone() {
                let s = self.measure(c);
                out[c] = [self.nodes[c].offset[self.axis], s];
                self.arrange(c, s, out);
            }
            return;
        };
        let (start, end) = pads(&self.nodes[i], l, self.axis);
        let inner = size - start - end;
        let flow = self.flow(i);
        if primary(l) == self.axis {
            let lines = if l.wrap {
                self.wrapped(&flow, inner, l)
            } else {
                vec![flow]
            };
            for flow in &lines {
                let sizes = self.distribute(flow, inner, l);
                let sum: f32 = sizes.iter().sum();
                let k = flow.len();
                let gaps = l.spacing * k.saturating_sub(1) as f32;
                let (mut pos, gap) = match l.primary_align {
                    Align::Min => (start, l.spacing),
                    Align::Center => (start + (inner - sum - gaps) / 2., l.spacing),
                    Align::Max => (start + inner - sum - gaps, l.spacing),
                    Align::SpaceBetween if k > 1 => (start, (inner - sum) / (k - 1) as f32),
                    Align::SpaceBetween => (start, 0.),
                };
                for (&c, &s) in flow.iter().zip(&sizes) {
                    out[c] = [pos, s];
                    pos += s + gap;
                }
            }
            if l.wrap {
                self.lines[i] = Some(lines);
            }
        } else {
            let lines = self.lines_of(i, l, &flow);
            // A stack that did not wrap has one line as thick as itself.
            // Wrapped lines are as thick as their content and sit together,
            // placed as a block by the counter alignment.
            let mut bands = vec![(start, inner)];
            if l.wrap {
                let mut thick: Vec<f32> = lines
                    .iter()
                    .map(|line| self.track_thickness(line, l))
                    .collect();
                let total: f32 = thick.iter().sum();
                let between = l.counter_align_content == AlignContent::SpaceBetween;
                let gap = if between && lines.len() > 1 {
                    ((inner - total) / (lines.len() - 1) as f32).max(0.)
                } else if between {
                    0.
                } else {
                    l.counter_spacing
                };
                let spare = inner - total - gap * lines.len().saturating_sub(1) as f32;
                let stretch = !between
                    && !flow.is_empty()
                    && flow
                        .iter()
                        .all(|&c| self.nodes[c].sizing[self.axis] == Sizing::Fill);
                if stretch {
                    let extra = spare.max(0.) / thick.len() as f32;
                    for t in &mut thick {
                        *t += extra;
                    }
                }
                let mut pos = if stretch || (between && lines.len() > 1) {
                    start
                } else {
                    match l.counter_align {
                        Align::Center => start + spare / 2.,
                        Align::Max => start + spare,
                        _ => start,
                    }
                };
                bands.clear();
                for t in thick {
                    bands.push((pos, t));
                    pos += t + gap;
                }
            }
            for (line, (start, inner)) in lines.iter().zip(bands) {
                for &c in line {
                    let s = if self.nodes[c].sizing[self.axis] == Sizing::Fill {
                        self.clamp(c, inner)
                    } else {
                        self.measure(c)
                    };
                    let pos = match l.counter_align {
                        Align::Center => start + (inner - s) / 2.,
                        Align::Max => start + inner - s,
                        _ => start,
                    };
                    out[c] = [pos, s];
                }
            }
        }
        for c in self.children[i].clone() {
            if !self.nodes[c].in_flow {
                out[c] = [self.nodes[c].offset[self.axis], self.measure(c)];
            }
            let s = out[c][1];
            self.arrange(c, s, out);
        }
    }

    /// Sizes of flow children on the primary axis. Fill children share what is
    /// left equally; one that hits a limit keeps it and the rest share again.
    fn distribute(&mut self, flow: &[usize], inner: f32, l: AutoLayout) -> Vec<f32> {
        let mut sizes: Vec<f32> = flow
            .iter()
            .map(|&c| {
                if self.nodes[c].sizing[self.axis] == Sizing::Fill {
                    0.
                } else {
                    self.measure(c)
                }
            })
            .collect();
        let open: Vec<usize> = (0..flow.len())
            .filter(|&k| self.nodes[flow[k]].sizing[self.axis] == Sizing::Fill)
            .collect();
        let gaps = l.spacing * flow.len().saturating_sub(1) as f32;
        let available = f64::from(inner - gaps) - sizes.iter().map(|&v| f64::from(v)).sum::<f64>();
        // Solve sum(clamp(share, min, max)) = available. Freezing both lower
        // and upper violations at once can over-allocate (min 80 / max 40
        // in 100 pixels must yield 80 / 20, not 80 / 40).
        let total = |share: f64| {
            open.iter()
                .map(|&k| {
                    let n = &self.nodes[flow[k]];
                    share
                        .max(f64::from(n.min[self.axis]))
                        .min(f64::from(n.max[self.axis]))
                })
                .sum::<f64>()
        };
        let mut lo = 0.;
        let mut hi = available.max(0.);
        if total(0.) < available {
            for _ in 0..64 {
                let mid = (lo + hi) / 2.;
                if total(mid) < available {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
        } else {
            hi = 0.;
        }
        for k in open {
            sizes[k] = self.clamp(flow[k], hi as f32);
        }
        sizes
    }
}

fn primary(l: AutoLayout) -> usize {
    match l.direction {
        Direction::Horizontal => 0,
        Direction::Vertical => 1,
    }
}

fn pads(n: &Node, l: AutoLayout, axis: usize) -> (f32, f32) {
    let weights = if n.strokes_included_in_layout && n.stroke_align == StrokeAlign::Inside {
        n.stroke_weights
    } else {
        [0.; 4]
    };
    let (start, end) = if axis == 0 { (3, 1) } else { (0, 2) };
    (
        l.padding[axis] + weights[start],
        l.padding[axis + 2] + weights[end],
    )
}

/// Checks the input and lists each node's children.
fn children_of(nodes: &[Node]) -> Result<Vec<Vec<usize>>, Error> {
    let mut children = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        match n.parent {
            None if i == 0 => {}
            Some(p) if p < i => children[p].push(i),
            _ => return Err(Error("layout parents must precede children")),
        }
        let finite = n
            .size
            .iter()
            .chain(&n.offset)
            .chain(&n.min)
            .all(|v| v.is_finite());
        if n.stroke_weights.iter().any(|v| !v.is_finite() || *v < 0.) {
            return Err(Error("invalid layout stroke widths"));
        }
        if !finite
            || n.size.iter().chain(&n.min).any(|&v| v < 0.)
            || (0..2).any(|axis| n.max[axis].is_nan() || n.max[axis] < n.min[axis])
        {
            return Err(Error("invalid layout input"));
        }
        if let Some(l) = n.layout {
            let gaps = [l.spacing, l.counter_spacing];
            if !l.padding.iter().chain(&gaps).all(|v| v.is_finite())
                || l.padding.iter().any(|&v| v < 0.)
            {
                return Err(Error("invalid layout input"));
            }
            if l.wrap && l.direction == Direction::Vertical {
                return Err(Error("only horizontal stacks wrap"));
            }
            if l.counter_align == Align::SpaceBetween {
                return Err(Error("space between applies to the primary axis only"));
            }
        }
    }
    if nodes.is_empty() {
        return Err(Error("empty layout"));
    }
    Ok(children)
}

/// The size the root takes from its own sizing: hugging axes wrap the content.
pub fn measure(nodes: &[Node]) -> Result<[f32; 2], Error> {
    let children = children_of(nodes)?;
    let mut s = Solver::new(nodes, &children, 0, vec![None; nodes.len()]);
    let width = s.measure(0);
    // Heights depend on where wrapping stacks break at that width.
    s.arrange(0, width, &mut vec![[0.; 2]; nodes.len()]);
    let height = Solver::new(nodes, &children, 1, s.lines).measure(0);
    Rect::new(0., 0., width, height).validate()?;
    Ok([width, height])
}

/// The rectangle of every node relative to its parent. The supplied root size
/// overrides authored/hug sizing and is clamped to the root's min/max limits.
pub fn solve(nodes: &[Node], root_size: [f32; 2]) -> Result<Vec<Rect>, Error> {
    let children = children_of(nodes)?;
    if !root_size.iter().all(|v| v.is_finite() && *v >= 0.) {
        return Err(Error("invalid layout input"));
    }
    let mut out = vec![Rect::default(); nodes.len()];
    let mut lines = vec![None; nodes.len()];
    for (axis, &root) in root_size.iter().enumerate() {
        let mut s = Solver::new(nodes, &children, axis, lines);
        let mut axis_out = vec![[0f32; 2]; nodes.len()];
        let root = s.clamp(0, root);
        axis_out[0] = [0., root];
        s.arrange(0, root, &mut axis_out);
        for (b, [pos, size]) in out.iter_mut().zip(axis_out) {
            if axis == 0 {
                (b.x, b.w) = (pos, size);
            } else {
                (b.y, b.h) = (pos, size);
            }
        }
        lines = s.lines;
    }
    for (n, rect) in nodes.iter().zip(&out) {
        validate_bounds(n, rect)?;
    }
    Ok(out)
}

fn validate_bounds(n: &Node, rect: &Rect) -> Result<(), Error> {
    rect.validate()?;
    if n.layout.is_some()
        && n.strokes_included_in_layout
        && n.stroke_align == StrokeAlign::Inside
        && (rect.w < n.stroke_weights[1] + n.stroke_weights[3]
            || rect.h < n.stroke_weights[0] + n.stroke_weights[2])
    {
        return Err(Error(
            "layout frame is smaller than its included inside strokes",
        ));
    }
    Ok(())
}

/// Builders named after Figma's auto layout panel.
impl Node {
    /// A leaf, or a plain frame whose children keep their offsets.
    pub fn fixed(size: [f32; 2]) -> Self {
        Self::at(None, [0.; 2], size)
    }
    /// A leaf that takes the space its parent stack gives on both axes.
    pub fn fill() -> Self {
        Self::fixed([0.; 2])
            .width(Sizing::Fill)
            .height(Sizing::Fill)
    }
    /// A horizontal auto layout frame that hugs its content.
    pub fn row() -> Self {
        Self::stack(Direction::Horizontal)
    }
    /// A vertical auto layout frame that hugs its content.
    pub fn column() -> Self {
        Self::stack(Direction::Vertical)
    }
    fn stack(direction: Direction) -> Self {
        let mut n = Self::fixed([0.; 2]).width(Sizing::Hug).height(Sizing::Hug);
        n.layout = Some(AutoLayout {
            direction,
            padding: [0.; 4],
            spacing: 0.,
            primary_align: Align::Min,
            counter_align: Align::Min,
            wrap: false,
            counter_spacing: 0.,
            counter_align_content: AlignContent::Auto,
        });
        n
    }
    /// The fixed size; also switches both axes to fixed sizing.
    pub fn size(mut self, size: [f32; 2]) -> Self {
        self.size = size;
        self.sizing = [Sizing::Fixed; 2];
        self
    }
    pub fn width(mut self, sizing: Sizing) -> Self {
        self.sizing[0] = sizing;
        self
    }
    pub fn height(mut self, sizing: Sizing) -> Self {
        self.sizing[1] = sizing;
        self
    }
    pub fn min(mut self, min: [f32; 2]) -> Self {
        self.min = min;
        self
    }
    pub fn max(mut self, max: [f32; 2]) -> Self {
        self.max = max;
        self
    }
    /// Takes the node out of its parent's flow and pins it at `offset`.
    pub fn absolute(mut self, offset: [f32; 2]) -> Self {
        self.offset = offset;
        self.in_flow = false;
        self
    }
    /// The same padding on every side. Stack settings on a node that is not a
    /// stack make it a horizontal one.
    pub fn padding(self, all: f32) -> Self {
        self.paddings([all; 4])
    }
    /// Horizontal then vertical padding.
    pub fn padding_xy(self, x: f32, y: f32) -> Self {
        self.paddings([x, y, x, y])
    }
    /// Left, top, right, bottom.
    pub fn paddings(mut self, padding: [f32; 4]) -> Self {
        self.stack_mut().padding = padding;
        self
    }
    /// Uniform visible stroke width. See [`Node::stroke_weights`].
    pub fn stroke_weight(self, weight: f32) -> Self {
        self.stroke_weights([weight; 4])
    }
    /// Top, right, bottom, left; counted once regardless of paint count.
    pub fn stroke_weights(mut self, weights: [f32; 4]) -> Self {
        self.stroke_weights = weights;
        self
    }
    pub fn stroke_align(mut self, align: StrokeAlign) -> Self {
        self.stroke_align = align;
        self
    }
    /// Only inside strokes of an auto-layout frame reserve content space.
    /// Center/outside strokes and absolute-child offsets are unaffected.
    pub fn strokes_included_in_layout(mut self, included: bool) -> Self {
        self.strokes_included_in_layout = included;
        self
    }
    /// Figma's "gap between items".
    pub fn gap(mut self, spacing: f32) -> Self {
        self.stack_mut().spacing = spacing;
        self
    }
    /// Alignment along the stack's direction, then across it.
    pub fn align(mut self, primary: Align, counter: Align) -> Self {
        let l = self.stack_mut();
        (l.primary_align, l.counter_align) = (primary, counter);
        self
    }
    /// Figma's "wrap", with the gap between lines. Horizontal stacks only.
    pub fn wrap(mut self, gap_between_lines: f32) -> Self {
        let l = self.stack_mut();
        (l.wrap, l.counter_spacing) = (true, gap_between_lines);
        self
    }
    /// Distribution between wrapped tracks; independent of item alignment.
    pub fn counter_axis_align_content(mut self, align: AlignContent) -> Self {
        self.stack_mut().counter_align_content = align;
        self
    }
    fn stack_mut(&mut self) -> &mut AutoLayout {
        self.layout.get_or_insert(AutoLayout {
            direction: Direction::Horizontal,
            padding: [0.; 4],
            spacing: 0.,
            primary_align: Align::Min,
            counter_align: Align::Min,
            wrap: false,
            counter_spacing: 0.,
            counter_align_content: AlignContent::Auto,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

/// A layout tree built parent first.
#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    pub fn new(mut root: Node) -> Self {
        root.parent = None;
        Self { nodes: vec![root] }
    }
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }
    /// Appends `node` as the last child of `parent`.
    pub fn add(&mut self, parent: NodeId, mut node: Node) -> NodeId {
        assert!(parent.0 < self.nodes.len(), "parent is from another tree");
        node.parent = Some(parent.0);
        self.nodes.push(node);
        NodeId(self.nodes.len() - 1)
    }
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }
    /// For values that change at runtime, such as a measured text width.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }
    /// Places every node with the root at `size`.
    pub fn solve(&self, size: [f32; 2]) -> Result<Solved, Error> {
        let mut rects = solve(&self.nodes, size)?;
        // Parents come first, so one pass turns parent-relative into absolute.
        for i in 1..rects.len() {
            let p = rects[self.nodes[i].parent.unwrap()];
            rects[i] = rects[i].translate([p.x, p.y]);
            validate_bounds(&self.nodes[i], &rects[i])?;
        }
        Ok(Solved { rects })
    }
    /// Solve intrinsic leaf sizes without retaining content or a text backend.
    /// Each root axis can be supplied or left natural. For hug-width leaves the
    /// callback first receives `None` (intrinsic width); for hug-height leaves it
    /// then receives the final width. Return `None` for ordinary authored leaves.
    /// Only the relevant returned dimension is used; both must be finite/nonnegative.
    pub fn solve_measured(
        &self,
        size: [Option<f32>; 2],
        mut measure: impl FnMut(NodeId, Option<f32>) -> Result<Option<[f32; 2]>, Error>,
    ) -> Result<Solved, Error> {
        if size.iter().flatten().any(|v| !v.is_finite() || *v < 0.) {
            return Err(Error("invalid layout root size"));
        }
        let mut nodes = self.nodes.clone();
        let children = children_of(&nodes)?;
        let check = |v: [f32; 2]| -> Result<[f32; 2], Error> {
            if v.iter().any(|v| !v.is_finite() || *v < 0.) {
                Err(Error("invalid measured layout size"))
            } else {
                Ok(v)
            }
        };
        for (i, node) in nodes.iter_mut().enumerate() {
            if node.layout.is_none() && node.sizing[0] == Sizing::Hug {
                if !children[i].is_empty() {
                    return Err(Error("measured layout requires a leaf"));
                }
                if let Some(measured) = measure(NodeId(i), None)? {
                    node.size[0] = check(measured)?[0];
                }
            }
        }
        let mut widths = Solver::new(&nodes, &children, 0, vec![None; nodes.len()]);
        let width = match size[0] {
            Some(v) => widths.clamp(0, v),
            None => widths.measure(0),
        };
        let mut x = vec![[0.; 2]; nodes.len()];
        x[0] = [0., width];
        widths.arrange(0, width, &mut x);
        let wrapped_lines = widths.lines;
        for (i, node) in nodes.iter_mut().enumerate() {
            if node.layout.is_none() && node.sizing[1] == Sizing::Hug {
                if !children[i].is_empty() {
                    return Err(Error("measured layout requires a leaf"));
                }
                if let Some(measured) = measure(NodeId(i), Some(x[i][1]))? {
                    node.size[1] = check(measured)?[1];
                }
            }
        }
        let mut heights = Solver::new(&nodes, &children, 1, wrapped_lines);
        let height = match size[1] {
            Some(v) => heights.clamp(0, v),
            None => heights.measure(0),
        };
        let mut y = vec![[0.; 2]; nodes.len()];
        y[0] = [0., height];
        heights.arrange(0, height, &mut y);
        let mut rects: Vec<Rect> = x
            .iter()
            .zip(&y)
            .map(|(x, y)| Rect::new(x[0], y[0], x[1], y[1]))
            .collect();
        for i in 0..rects.len() {
            if let Some(p) = nodes[i].parent {
                rects[i] = rects[i].translate([rects[p].x, rects[p].y]);
            }
            validate_bounds(&nodes[i], &rects[i])?;
        }
        Ok(Solved { rects })
    }

    /// Places every node with the root at the size its own sizing gives:
    /// hugging axes wrap the content, other axes keep the root's size.
    pub fn solve_natural(&self) -> Result<Solved, Error> {
        self.solve(measure(&self.nodes)?)
    }
}

/// Root-relative rectangles, indexed by [`NodeId`].
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    rects: Vec<Rect>,
}

impl Solved {
    pub fn size(&self) -> [f32; 2] {
        [self.rects[0].w, self.rects[0].h]
    }
}

impl Index<NodeId> for Solved {
    type Output = Rect;
    fn index(&self, id: NodeId) -> &Rect {
        &self.rects[id.0]
    }
}

#[cfg(test)]
mod tests {
    //! Synthetic cases from Figma's documented behaviour, not Figma output.
    use super::*;

    fn stack(direction: Direction, primary_align: Align, counter_align: Align) -> Node {
        let mut n = Node::at(None, [0., 0.], [200., 100.]);
        n.layout = Some(AutoLayout {
            direction,
            padding: [10., 5., 20., 15.],
            spacing: 4.,
            primary_align,
            counter_align,
            wrap: false,
            counter_spacing: 0.,
            counter_align_content: AlignContent::Auto,
        });
        n
    }

    fn b(v: &Rect) -> [f32; 4] {
        [v.x, v.y, v.w, v.h]
    }

    #[test]
    fn horizontal_stack_places_fixed_children_with_padding_and_gaps() {
        let nodes = [
            stack(Direction::Horizontal, Align::Min, Align::Min),
            Node::at(Some(0), [99., 99.], [30., 20.]),
            Node::at(Some(0), [99., 99.], [40., 10.]),
        ];
        let r = solve(&nodes, [200., 100.]).unwrap();
        assert_eq!(b(&r[1]), [10., 5., 30., 20.]);
        assert_eq!(b(&r[2]), [44., 5., 40., 10.]);
    }

    #[test]
    fn alignment_centres_and_ends() {
        let nodes = [
            stack(Direction::Vertical, Align::Center, Align::Max),
            Node::at(Some(0), [0., 0.], [30., 20.]),
            Node::at(Some(0), [0., 0.], [40., 10.]),
        ];
        let r = solve(&nodes, [200., 100.]).unwrap();
        // Inner height 80, content 34: starts at 5 + 23.
        assert_eq!(b(&r[1]), [150., 28., 30., 20.]);
        assert_eq!(b(&r[2]), [140., 52., 40., 10.]);
    }

    #[test]
    fn fill_children_share_space_and_respect_limits() {
        let mut a = Node::at(Some(0), [0., 0.], [1., 1.]);
        a.sizing = [Sizing::Fill, Sizing::Fill];
        a.max[0] = 30.;
        let mut c = a.clone();
        c.max[0] = f32::INFINITY;
        let nodes = [
            stack(Direction::Horizontal, Align::Min, Align::Min),
            a,
            Node::at(Some(0), [0., 0.], [50., 10.]),
            c,
        ];
        let r = solve(&nodes, [200., 100.]).unwrap();
        // Inner 170, gaps 8, fixed 50 → 112 to share; the first stops at 30.
        assert_eq!(b(&r[1]), [10., 5., 30., 80.]);
        assert_eq!(b(&r[2]), [44., 5., 50., 10.]);
        assert_eq!(b(&r[3]), [98., 5., 82., 80.]);
    }

    #[test]
    fn hug_frames_wrap_their_content_and_skip_absolute_children() {
        let mut inner = stack(Direction::Vertical, Align::Min, Align::Min);
        inner.parent = Some(0);
        inner.sizing = [Sizing::Hug, Sizing::Hug];
        let mut absolute = Node::at(Some(1), [3., 4.], [500., 500.]);
        absolute.in_flow = false;
        let nodes = [
            stack(Direction::Horizontal, Align::Min, Align::Center),
            inner,
            Node::at(Some(1), [0., 0.], [30., 20.]),
            Node::at(Some(1), [0., 0.], [40., 10.]),
            absolute,
        ];
        let r = solve(&nodes, [200., 100.]).unwrap();
        // 40 + 10 + 20 wide; 20 + 4 + 10 + 5 + 15 tall, centred in inner 80.
        assert_eq!(b(&r[1]), [10., 5. + (80. - 54.) / 2., 70., 54.]);
        assert_eq!(b(&r[4]), [3., 4., 500., 500.]);
    }

    #[test]
    fn space_between_spreads_children() {
        let nodes = [
            stack(Direction::Horizontal, Align::SpaceBetween, Align::Min),
            Node::at(Some(0), [0., 0.], [30., 20.]),
            Node::at(Some(0), [0., 0.], [40., 10.]),
            Node::at(Some(0), [0., 0.], [20., 10.]),
        ];
        let r = solve(&nodes, [200., 100.]).unwrap();
        assert_eq!([r[1].x, r[2].x, r[3].x], [10., 80., 160.]);
    }

    #[test]
    fn rejects_children_before_parents() {
        let nodes = [
            Node::at(None, [0., 0.], [1., 1.]),
            Node::at(Some(2), [0., 0.], [1., 1.]),
        ];
        assert!(solve(&nodes, [1., 1.]).is_err());
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;

    #[test]
    fn a_settings_row_resizes_from_one_tree() {
        // Label on the left, a 44x24 switch on the right, 12 padding.
        let mut t = Tree::new(
            Node::row()
                .padding(12.)
                .gap(8.)
                .align(Align::SpaceBetween, Align::Center),
        );
        let label = t.add(t.root(), Node::fixed([80., 16.]));
        let switch = t.add(t.root(), Node::fixed([44., 24.]));
        let knob = t.add(switch, Node::fixed([20., 20.]).absolute([2., 2.]));

        let s = t.solve([320., 48.]).unwrap();
        assert_eq!(s[label], Rect::new(12., 16., 80., 16.));
        assert_eq!(s[switch], Rect::new(264., 12., 44., 24.));
        // Children are root-relative, not parent-relative.
        assert_eq!(s[knob], Rect::new(266., 14., 20., 20.));

        let wide = t.solve([400., 48.]).unwrap();
        assert_eq!(wide[switch].x, 344.);

        // Hugging: 12 + 80 + 8 + 44 + 12 wide, 12 + 24 + 12 tall.
        assert_eq!(t.solve_natural().unwrap().size(), [156., 48.]);
    }

    #[test]
    fn filter_segments_wrap_where_figma_wraps_them() {
        // "Filter Segments" (node 145:3699 in the Abner design system file):
        // 254 wide, hugging height, gap 9, 4 between lines, centred across.
        // Expected rectangles are the ones Figma reports.
        let mut t = Tree::new(
            Node::row()
                .gap(9.)
                .wrap(4.)
                .align(Align::Min, Align::Center)
                .width(Sizing::Fixed),
        );
        t.node_mut(t.root()).size[0] = 254.;
        let sizes = [[62., 18.], [85., 18.], [80., 23.], [76., 23.], [67., 23.]];
        let ids = sizes.map(|s| t.add(t.root(), Node::fixed(s)));
        let s = t.solve_natural().unwrap();
        assert_eq!(s.size(), [254., 50.]);
        let figma = [[0., 2.5], [71., 2.5], [165., 0.], [0., 27.], [85., 27.]];
        for ((id, at), size) in ids.iter().zip(figma).zip(sizes) {
            assert_eq!(s[*id], Rect::new(at[0], at[1], size[0], size[1]));
        }
        // Narrower, the third segment drops to the second line.
        t.node_mut(t.root()).size[0] = 200.;
        let s = t.solve_natural().unwrap();
        assert_eq!(s.size(), [200., 18. + 4. + 23. + 4. + 23.]);
        assert_eq!(s[ids[2]], Rect::new(0., 22., 80., 23.));
        assert_eq!(s[ids[4]], Rect::new(0., 49., 67., 23.));
    }

    #[test]
    fn a_palette_wraps_in_a_fixed_frame() {
        // "Application palette" (node 26:3791): 1492x470, twenty 286x82
        // swatches, 12 between them both ways. Figma puts five on a line.
        let mut t = Tree::new(Node::row().gap(12.).wrap(12.).size([1492., 470.]));
        let ids: Vec<NodeId> = (0..20)
            .map(|_| t.add(t.root(), Node::fixed([286., 82.])))
            .collect();
        let s = t.solve_natural().unwrap();
        assert_eq!(s.size(), [1492., 470.]);
        assert_eq!(s[ids[4]], Rect::new(1192., 0., 286., 82.));
        assert_eq!(s[ids[5]], Rect::new(0., 94., 286., 82.));
        assert_eq!(s[ids[19]], Rect::new(1192., 282., 286., 82.));
    }

    #[test]
    fn wrapped_fill_children_share_their_own_line() {
        let mut t = Tree::new(Node::row().gap(10.).wrap(5.).size([100., 100.]));
        let a = t.add(t.root(), Node::fixed([60., 10.]));
        let b = t.add(
            t.root(),
            Node::fixed([0., 20.]).width(Sizing::Fill).min([50., 0.]),
        );
        let c = t.add(t.root(), Node::fixed([20., 10.]).height(Sizing::Fill));
        let s = t.solve([100., 100.]).unwrap();
        assert_eq!(s[a], Rect::new(0., 0., 60., 10.));
        // 50 at least does not fit beside 60, so it starts a line and
        // shares it with the 20 that follows.
        assert_eq!(s[b], Rect::new(0., 15., 70., 20.));
        assert_eq!(s[c], Rect::new(80., 15., 20., 20.));
        let column = Tree::new(Node::column().wrap(4.));
        assert!(column.solve([10., 10.]).is_err());
    }

    #[test]
    fn fill_children_and_runtime_size_changes() {
        let mut t = Tree::new(Node::column().size([200., 100.]).gap(4.));
        let header = t.add(t.root(), Node::fixed([0., 20.]).width(Sizing::Fill));
        let body = t.add(t.root(), Node::fill());
        let s = t.solve([200., 100.]).unwrap();
        assert_eq!(s[header], Rect::new(0., 0., 200., 20.));
        assert_eq!(s[body], Rect::new(0., 24., 200., 76.));
        t.node_mut(header).size[1] = 30.;
        assert_eq!(t.solve([200., 100.]).unwrap()[body].h, 66.);
        // A fixed root keeps its size when solved naturally.
        assert_eq!(t.solve_natural().unwrap().size(), [200., 100.]);
    }
}

#[cfg(test)]
mod responsive_tests {
    use super::*;
    #[test]
    fn competing_fill_bounds_redistribute_without_freezing_the_wrong_limit() {
        let mut tree = Tree::new(Node::row());
        let a = tree.add(tree.root(), Node::fill().min([80., 0.]));
        let b = tree.add(tree.root(), Node::fill().max([40., f32::INFINITY]));
        for (width, expected) in [(100., [80., 20.]), (160., [120., 40.]), (40., [80., 0.])] {
            let out = tree.solve([width, 20.]).unwrap();
            assert_eq!([out[a].w, out[b].w], expected);
        }
        // Lower/upper limits must not depend on child order or axis.
        let mut tree = Tree::new(Node::column());
        let a = tree.add(tree.root(), Node::fill().max([f32::INFINITY, 40.]));
        let b = tree.add(tree.root(), Node::fill().min([0., 80.]));
        let out = tree.solve([20., 100.]).unwrap();
        assert_eq!([out[a].h, out[b].h], [20., 80.]);
    }
    #[test]
    fn hug_counter_axis_includes_fill_minimum_and_excludes_absolute_children() {
        // Confirmed by a live Figma probe: fixed width 30 beside fill min 100
        // makes a vertical hugging frame 100 wide, not 30.
        let mut tree = Tree::new(Node::column());
        tree.add(tree.root(), Node::fixed([30., 20.]));
        let fill = tree.add(
            tree.root(),
            Node::fixed([200., 20.]).width(Sizing::Fill).min([100., 0.]),
        );
        tree.add(tree.root(), Node::fixed([900., 900.]).absolute([4., 7.]));
        let out = tree.solve_natural().unwrap();
        assert_eq!(out.size(), [100., 40.]);
        assert_eq!(out[fill].w, 100.);
    }
    #[test]
    fn invalid_dimensions_bounds_padding_and_root_sizes_fail() {
        for node in [
            Node::fixed([-1., 20.]),
            Node::fill().min([-1., 0.]),
            Node::fill().min([20., 0.]).max([10., 30.]),
            Node::row().padding(-1.),
            Node::row().max([f32::NEG_INFINITY, 20.]),
        ] {
            assert!(Tree::new(node).solve([100., 100.]).is_err());
        }
        let tree = Tree::new(Node::row().min([50., 20.]).max([100., 60.]));
        assert_eq!(tree.solve([10., 90.]).unwrap().size(), [50., 60.]);
        for size in [[-1., 20.], [f32::INFINITY, 20.], [f32::NAN, 20.]] {
            assert!(tree.solve(size).is_err());
        }
    }
}

#[cfg(test)]
mod measured_tests {
    use super::*;
    #[test]
    fn width_then_height_measurement_propagates_through_nested_hug_frames() {
        let mut tree = Tree::new(
            Node::column()
                .size([200., 1.])
                .height(Sizing::Hug)
                .padding(10.)
                .gap(5.),
        );
        let row = tree.add(tree.root(), Node::row().width(Sizing::Fill).gap(4.));
        let title = tree.add(
            row,
            Node::fixed([999., 10.])
                .width(Sizing::Hug)
                .max([50., f32::INFINITY]),
        );
        let body = tree.add(
            row,
            Node::fixed([999., 999.])
                .width(Sizing::Fill)
                .height(Sizing::Hug),
        );
        let footer = tree.add(tree.root(), Node::fixed([30., 12.]));
        for width in [200., 120.] {
            let mut calls = Vec::new();
            let solved = tree
                .solve_measured([Some(width), None], |id, width| {
                    calls.push((id, width));
                    if id == title {
                        assert!(width.is_none());
                        return Ok(Some([70., 10.]));
                    }
                    if id == body {
                        let w = width.unwrap();
                        return Ok(Some([w, (200. / w).ceil() * 20.]));
                    }
                    Ok(None)
                })
                .unwrap();
            let body_width = width - 20. - 50. - 4.;
            let height = (200. / body_width).ceil() * 20.;
            assert_eq!(solved[body].w, body_width);
            assert_eq!(solved[row].h, height);
            assert_eq!(solved[footer].y, 10. + height + 5.);
            assert_eq!(solved.size(), [width, 20. + height + 5. + 12.]);
            assert_eq!(calls, [(title, None), (body, Some(body_width))]);
        }
        assert_eq!(
            tree.node(body).size,
            [999., 999.],
            "measurement must not mutate the tree"
        );
        assert!(
            tree.solve_measured([Some(200.), None], |_, _| Err(Error("missing font")))
                .is_err()
        );
        assert!(
            tree.solve_measured([Some(200.), None], |_, _| Ok(Some([f32::NAN, 1.])))
                .is_err()
        );
    }
}

#[cfg(test)]
mod stroke_tests {
    use super::*;

    #[test]
    fn inside_borders_reserve_space_once_and_leave_absolute_offsets() {
        for align in [
            StrokeAlign::Inside,
            StrokeAlign::Center,
            StrokeAlign::Outside,
        ] {
            for included in [false, true] {
                let mut tree = Tree::new(
                    Node::row()
                        .paddings([3., 5., 7., 9.])
                        .stroke_weights([2., 4., 6., 8.])
                        .stroke_align(align)
                        .strokes_included_in_layout(included),
                );
                let child = tree.add(tree.root(), Node::fill().stroke_weight(100.));
                let absolute = tree.add(tree.root(), Node::fixed([5., 5.]).absolute([1., 2.]));
                let solved = tree.solve([120., 80.]).unwrap();
                let expected = if included && align == StrokeAlign::Inside {
                    Rect::new(11., 7., 98., 58.)
                } else {
                    Rect::new(3., 5., 110., 66.)
                };
                assert_eq!(solved[child], expected);
                assert_eq!(solved[absolute], Rect::new(1., 2., 5., 5.));
            }
        }
    }

    #[test]
    fn borders_participate_in_hug_and_width_dependent_measurement() {
        let mut tree = Tree::new(
            Node::column()
                .padding(3.)
                .stroke_weight(10.)
                .strokes_included_in_layout(true),
        );
        let child = tree.add(tree.root(), Node::fixed([30., 20.]));
        assert_eq!(
            tree.solve_measured([None, None], |_, _| Ok(None))
                .unwrap()
                .size(),
            [56., 46.]
        );
        tree.node_mut(child).sizing = [Sizing::Fill, Sizing::Hug];
        let solved = tree
            .solve_measured([Some(100.), None], |_, width| {
                assert_eq!(width, Some(74.));
                Ok(Some([74., 40.]))
            })
            .unwrap();
        assert_eq!(solved.size(), [100., 66.]);
        assert_eq!(solved[child], Rect::new(13., 13., 74., 40.));
        assert!(tree.solve([12., 12.]).is_err());
        for weight in [-1., f32::NAN, f32::INFINITY] {
            tree.node_mut(tree.root()).stroke_weights = [weight; 4];
            assert!(tree.solve([100., 100.]).is_err());
        }
    }
}

#[cfg(test)]
mod wrapped_measurement_tests {
    use super::*;

    #[test]
    fn nested_wrapped_measured_leaves_set_track_and_parent_heights() {
        let mut tree = Tree::new(Node::column().padding(5.).gap(7.));
        let wrap = tree.add(
            tree.root(),
            Node::row().width(Sizing::Fill).gap(10.).wrap(6.),
        );
        let text = tree.add(
            wrap,
            Node::fixed([999., 999.])
                .width(Sizing::Fill)
                .height(Sizing::Hug)
                .min([60., 0.])
                .max([100., f32::INFINITY]),
        );
        let fixed = tree.add(wrap, Node::fixed([70., 20.]));
        let absolute = tree.add(wrap, Node::fixed([1., 1.]).absolute([2., 3.]));
        let footer = tree.add(tree.root(), Node::fixed([10., 10.]));
        for (width, text_width, fixed_y, wrap_height) in
            [(130., 100., 46., 66.), (200., 100., 0., 40.)]
        {
            let solved = tree
                .solve_measured([Some(width), None], |id, width| {
                    assert_eq!(id, text);
                    assert_eq!(width, Some(text_width));
                    Ok(Some([text_width, 40.]))
                })
                .unwrap();
            assert_eq!(solved[wrap].h, wrap_height);
            assert_eq!(solved[fixed].y, 5. + fixed_y);
            assert_eq!(solved[footer].y, 5. + wrap_height + 7.);
            assert_eq!(solved[absolute], Rect::new(7., 8., 1., 1.));
            assert_eq!(solved.size(), [width, 10. + wrap_height + 7. + 10.]);
        }
    }

    #[test]
    fn wrapped_fill_tracks_ignore_authored_heights_and_distribute_spare_space() {
        let mut tree = Tree::new(
            Node::row()
                .gap(10.)
                .wrap(12.)
                .padding(10.)
                .align(Align::Min, Align::Center),
        );
        let mut ids = Vec::new();
        for i in 0..4 {
            ids.push(
                tree.add(
                    tree.root(),
                    Node::fixed([50., 999.])
                        .height(Sizing::Fill)
                        .min([0., if i == 0 { 90. } else { 10. }]),
                ),
            );
        }
        let solved = tree.solve([140., 160.]).unwrap();
        assert_eq!(solved[ids[0]], Rect::new(10., 10., 50., 104.));
        assert_eq!(solved[ids[2]], Rect::new(10., 126., 50., 24.));
        tree.node_mut(tree.root())
            .layout
            .as_mut()
            .unwrap()
            .counter_align_content = AlignContent::SpaceBetween;
        let solved = tree.solve([140., 160.]).unwrap();
        assert_eq!(solved[ids[0]], Rect::new(10., 10., 50., 90.));
        assert_eq!(solved[ids[2]], Rect::new(10., 140., 50., 10.));
        let overflow = tree.solve([140., 80.]).unwrap();
        assert_eq!(
            overflow[ids[2]].y, 100.,
            "overflowing tracks have zero distributed gap"
        );
    }
}

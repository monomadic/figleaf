//! Figma auto layout solver: fixed, hug and fill sizing, horizontal and vertical
//! stacks, grids, fixed-size rotations, wrapping, padding, gaps, alignment, absolute children and min/max
//! limits.
//!
//! Build a [`Tree`] from the values in Figma's auto layout panel, solve it for
//! a size, and draw each thing at the rectangle it gets. Borrowed from figui,
//! where it was checked against positions Figma recorded.
//!
//! Not covered: fill-height baseline cycles, dynamic rotated dimensions, spans
//! mixing hug and fractional tracks, and constraint-driven flipped geometry. Intrinsic leaves can use [`Tree::solve_measured`] with a host-owned
//! measurement callback (for example [`crate::Text::measure`]).
//!
//! Widths are solved first and heights second. Vertical wrapping repeats these
//! passes until track membership and boxes settle, remeasuring text at each width.
//! Cyclic dependencies or dependencies requiring more than 64 passes return errors.
use crate::{Error, Rect, StrokeAlign, Transform};
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
    /// First text baseline (horizontal stacks, counter axis only).
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

/// Intrinsic dimensions and an optional first text baseline from the box top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub size: [f32; 2],
    pub first_baseline: Option<f32>,
}

/// Figma resize constraints for plain-frame and absolute children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    Min,
    Center,
    Max,
    Stretch,
    Scale,
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
    /// or vertical stacks.
    pub wrap: bool,
    /// The gap between wrapped lines.
    pub counter_spacing: f32,
    pub counter_align_content: AlignContent,
}

/// A grid track in pixels, fractions of remaining space, or content size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    Fixed(f32),
    Flex(f32),
    Hug,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// Horizontal columns, then vertical rows.
    pub tracks: [Vec<GridTrack>; 2],
    /// Column gap, row gap.
    pub gaps: [f32; 2],
}

/// Explicit grid placement; indices are zero based, spans are positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCell {
    pub column: usize,
    pub row: usize,
    pub column_span: usize,
    pub row_span: usize,
    /// Horizontal, vertical. Min corresponds to Figma's AUTO default.
    pub align: [Align; 2],
}

impl Default for GridCell {
    fn default() -> Self {
        Self {
            column: 0,
            row: 0,
            column_span: 1,
            row_span: 1,
            align: [Align::Min; 2],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Index of the parent, which must come earlier. `None` only for the root.
    pub parent: Option<usize>,
    /// Authored size: used by fixed sizing, and by nodes that can't hug.
    pub size: [f32; 2],
    /// Authored position in the parent, used for nodes outside a stack's flow.
    pub offset: [f32; 2],
    /// Horizontal, vertical; ignored for in-flow auto-layout children.
    pub constraints: [Constraint; 2],
    pub first_baseline: Option<f32>,
    /// Ask the measurement callback for a baseline even on fixed-size text.
    pub measure_baseline: bool,
    /// Horizontal, vertical.
    pub sizing: [Sizing; 2],
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Takes part in its parent's stack: visible and not absolutely positioned.
    pub in_flow: bool,
    pub layout: Option<AutoLayout>,
    pub grid: Option<Grid>,
    pub grid_cell: GridCell,
    /// Figma degrees, positive counter-clockwise. Rotated nodes currently
    /// require fixed dimensions; their descendants can still be responsive.
    pub rotation: f32,
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
            constraints: [Constraint::Min; 2],
            first_baseline: None,
            measure_baseline: false,
            sizing: [Sizing::Fixed; 2],
            min: [0.; 2],
            max: [f32::INFINITY; 2],
            in_flow: true,
            layout: None,
            grid: None,
            grid_cell: GridCell::default(),
            rotation: 0.,
            stroke_weights: [0.; 4],
            stroke_align: StrokeAlign::Inside,
            strokes_included_in_layout: false,
        }
    }
}

struct Solver<'a> {
    nodes: &'a [Node],
    reference: &'a [Node],
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
        reference: &'a [Node],
        children: &[Vec<usize>],
        axis: usize,
        lines: Vec<Option<Vec<Vec<usize>>>>,
    ) -> Self {
        Self {
            nodes,
            reference,
            children: children.to_vec(),
            measured: vec![None; nodes.len()],
            axis,
            lines,
        }
    }

    fn clamp(&self, i: usize, v: f32) -> f32 {
        let n = &self.nodes[i];
        v.min(n.max[self.axis])
            .max(n.min[self.axis])
            .max(implicit_min(n, self.axis))
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
        let v = if self.rotated_flow(i) {
            let b = rotation_bounds(n);
            [b.w, b.h][self.axis]
        } else {
            v
        };
        self.measured[i] = Some(v);
        v
    }

    fn rotated_flow(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        n.rotation != 0. && n.in_flow && n.parent.is_some_and(|p| self.nodes[p].layout.is_some())
    }

    fn grid_tracks(&mut self, i: usize, available: Option<f32>) -> Vec<f32> {
        let g = self.nodes[i].grid.as_ref().unwrap();
        let tracks = g.tracks[self.axis].clone();
        let gap = g.gaps[self.axis];
        let mut sizes: Vec<f32> = tracks
            .iter()
            .map(|t| match t {
                GridTrack::Fixed(v) => *v,
                _ => 0.,
            })
            .collect();
        for c in self.flow(i) {
            let n = &self.nodes[c];
            let cell = n.grid_cell;
            let anchor = [cell.column, cell.row][self.axis];
            let span = [cell.column_span, cell.row_span][self.axis];
            if span == 1 && tracks[anchor] == GridTrack::Hug {
                let contribution = if n.sizing[self.axis] == Sizing::Fill {
                    self.clamp(c, 0.)
                } else {
                    self.measure(c)
                };
                sizes[anchor] = sizes[anchor].max(contribution);
            }
        }
        let mut spanning = self.flow(i);
        spanning.sort_by_key(|&c| {
            [
                self.nodes[c].grid_cell.column_span,
                self.nodes[c].grid_cell.row_span,
            ][self.axis]
        });
        for c in spanning {
            let n = &self.nodes[c];
            let cell = n.grid_cell;
            let anchor = [cell.column, cell.row][self.axis];
            let span = [cell.column_span, cell.row_span][self.axis];
            let hugs = tracks[anchor..anchor + span]
                .iter()
                .filter(|&&t| t == GridTrack::Hug)
                .count();
            if span > 1 && hugs > 0 {
                let contribution = if n.sizing[self.axis] == Sizing::Fill {
                    self.clamp(c, 0.)
                } else {
                    self.measure(c)
                };
                let existing =
                    sizes[anchor..anchor + span].iter().sum::<f32>() + gap * (span - 1) as f32;
                let extra = (contribution - existing).max(0.) / hugs as f32;
                for k in anchor..anchor + span {
                    if tracks[k] == GridTrack::Hug {
                        sizes[k] += extra;
                    }
                }
            }
        }
        if let Some(available) = available {
            let remaining = (available
                - sizes.iter().sum::<f32>()
                - gap * sizes.len().saturating_sub(1) as f32)
                .max(0.);
            let weight: f64 = tracks
                .iter()
                .map(|t| {
                    if let GridTrack::Flex(v) = t {
                        f64::from(*v)
                    } else {
                        0.
                    }
                })
                .sum();
            for (size, track) in sizes.iter_mut().zip(tracks) {
                if let GridTrack::Flex(v) = track {
                    *size = (f64::from(remaining) * f64::from(v) / weight) as f32;
                }
            }
        }
        sizes
    }

    fn hug(&mut self, i: usize, l: AutoLayout) -> f32 {
        let (start, end) = pads(&self.nodes[i], l, self.axis);
        if let Some(g) = &self.nodes[i].grid {
            let gap = g.gaps[self.axis];
            let sizes = self.grid_tracks(i, None);
            return start
                + end
                + sizes.iter().sum::<f32>()
                + gap * sizes.len().saturating_sub(1) as f32;
        }
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
            .map(|&c| self.nodes[c].min[self.axis].max(implicit_min(&self.nodes[c], self.axis)))
            .fold(measured, f32::max)
    }

    fn has_baseline(&self, i: usize) -> bool {
        self.nodes[i].first_baseline.is_some()
            || self.children[i].iter().any(|&c| self.has_baseline(c))
    }

    fn baseline(&mut self, i: usize, height: f32) -> f32 {
        if let Some(baseline) = self.nodes[i].first_baseline {
            return baseline;
        }
        if let Some(child) = self.children[i]
            .iter()
            .copied()
            .find(|&c| self.has_baseline(c))
        {
            let mut positions = vec![[0.; 2]; self.nodes.len()];
            self.arrange(i, height, &mut positions);
            return positions[child][0] + self.baseline(child, positions[child][1]);
        }
        height
    }

    fn baseline_extent(&mut self, line: &[usize]) -> (f32, f32) {
        if line.is_empty() {
            return (0., 0.);
        }
        let mut above = f32::NEG_INFINITY;
        let mut below = f32::NEG_INFINITY;
        for &c in line {
            let height = self.measure(c);
            let baseline = self.baseline(c, height);
            above = above.max(baseline);
            below = below.max(height - baseline);
        }
        (above, below)
    }

    fn track_thickness(&mut self, line: &[usize], l: AutoLayout) -> f32 {
        if self.axis == 1 && l.counter_align == Align::Baseline {
            let (above, below) = self.baseline_extent(line);
            return above + below;
        }
        if l.wrap
            && line
                .iter()
                .all(|&c| self.nodes[c].sizing[self.axis] == Sizing::Fill)
        {
            // Figma ignores authored fill heights when sizing a wrapped track.
            line.iter()
                .map(|&c| self.nodes[c].min[self.axis].max(implicit_min(&self.nodes[c], self.axis)))
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

    fn constrained(&mut self, child: usize, parent: usize, size: f32) -> [f32; 2] {
        let axis = self.axis;
        let n = &self.reference[child];
        let before = self.reference[parent].size[axis];
        let delta = size - before;
        let offset = n.offset[axis];
        let authored = n.size[axis];
        let measured = self.measure(child);
        match n.constraints[axis] {
            Constraint::Min => [offset, measured],
            Constraint::Max => [offset + delta + authored - measured, measured],
            Constraint::Center => [offset + (delta + authored - measured) / 2., measured],
            Constraint::Stretch => {
                let span = authored + delta;
                // A negative span implies flipped geometry, which this solver
                // does not represent. Preserve the error for bounds validation.
                [
                    offset,
                    if span < 0. {
                        span
                    } else {
                        self.clamp(child, span)
                    },
                ]
            }
            Constraint::Scale => [
                offset * size / before,
                self.clamp(child, authored * size / before),
            ],
        }
    }

    /// Places the children of `i`, whose size on this axis is `size`.
    fn arrange(&mut self, i: usize, size: f32, out: &mut [[f32; 2]]) {
        let size = if self.rotated_flow(i) {
            local_size(&self.nodes[i])[self.axis]
        } else {
            size
        };
        if let Some(grid) = self.nodes[i].grid.clone() {
            let (start, end) = pads(&self.nodes[i], self.nodes[i].layout.unwrap(), self.axis);
            let tracks = self.grid_tracks(i, Some(size - start - end));
            for c in self.children[i].clone() {
                if self.nodes[c].in_flow {
                    let cell = self.nodes[c].grid_cell;
                    let anchor = [cell.column, cell.row][self.axis];
                    let span = [cell.column_span, cell.row_span][self.axis];
                    let gap = grid.gaps[self.axis];
                    let pos = start + tracks[..anchor].iter().sum::<f32>() + gap * anchor as f32;
                    let extent =
                        tracks[anchor..anchor + span].iter().sum::<f32>() + gap * (span - 1) as f32;
                    let extent_child = if self.nodes[c].sizing[self.axis] == Sizing::Fill {
                        self.clamp(c, extent)
                    } else {
                        self.measure(c)
                    };
                    let align = match cell.align[self.axis] {
                        Align::Center => 0.5,
                        Align::Max => 1.,
                        _ => 0.,
                    };
                    out[c] = [pos + (extent - extent_child) * align, extent_child];
                } else {
                    out[c] = self.constrained(c, i, size);
                }
                self.arrange(c, out[c][1], out);
            }
            return;
        }
        let Some(l) = self.nodes[i].layout else {
            for c in self.children[i].clone() {
                out[c] = self.constrained(c, i, size);
                self.arrange(c, out[c][1], out);
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
                    Align::Min | Align::Baseline => (start, l.spacing),
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
                let baseline = if l.counter_align == Align::Baseline {
                    self.baseline_extent(line).0
                } else {
                    0.
                };
                for &c in line {
                    let s = if self.nodes[c].sizing[self.axis] == Sizing::Fill {
                        self.clamp(c, inner)
                    } else {
                        self.measure(c)
                    };
                    let pos = match l.counter_align {
                        Align::Baseline => start + baseline - self.baseline(c, s),
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
                out[c] = self.constrained(c, i, size);
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
                    (share + f64::from(implicit_min(n, self.axis)))
                        .min(f64::from(n.max[self.axis]))
                        .max(f64::from(n.min[self.axis]))
                        .max(f64::from(implicit_min(n, self.axis)))
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
            sizes[k] = self.clamp(
                flow[k],
                hi as f32 + implicit_min(&self.nodes[flow[k]], self.axis),
            );
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
        if !n.rotation.is_finite() {
            return Err(Error("invalid layout rotation"));
        }
        if n.rotation != 0.
            && (n.sizing != [Sizing::Fixed; 2]
                || n.constraints != [Constraint::Min; 2]
                || n.measure_baseline
                || n.first_baseline.is_some())
        {
            return Err(Error(
                "rotated layout nodes require fixed sizing, min constraints and no baseline",
            ));
        }
        if let Some(g) = &n.grid {
            if n.layout.is_none()
                || g.gaps.iter().any(|v| !v.is_finite() || *v < 0.)
                || g.tracks.iter().any(|tracks| {
                    tracks.is_empty()
                        || tracks.len() > 1024
                        || tracks.iter().any(|t| match t {
                            GridTrack::Fixed(v) => !v.is_finite() || *v < 0.,
                            GridTrack::Flex(v) => !v.is_finite() || *v <= 0.,
                            GridTrack::Hug => false,
                        })
                })
            {
                return Err(Error("invalid grid tracks or gaps"));
            }
            if (0..2).any(|a| {
                n.sizing[a] == Sizing::Hug
                    && g.tracks[a].iter().any(|t| matches!(t, GridTrack::Flex(_)))
            }) {
                return Err(Error("fractional grid tracks require a non-hug axis"));
            }
            if n.layout.is_some_and(|l| {
                l.wrap || l.primary_align != Align::Min || l.counter_align != Align::Min
            }) {
                return Err(Error("grid requires cell alignment and no wrapping"));
            }
        }
        if let Some(g) = n
            .parent
            .and_then(|p| nodes[p].grid.as_ref())
            .filter(|_| n.in_flow)
        {
            let c = n.grid_cell;
            for a in 0..2 {
                let anchor = [c.column, c.row][a];
                let span = [c.column_span, c.row_span][a];
                if span == 0
                    || anchor
                        .checked_add(span)
                        .is_none_or(|end| end > g.tracks[a].len())
                    || !matches!(c.align[a], Align::Min | Align::Center | Align::Max)
                {
                    return Err(Error("invalid grid cell"));
                }
                if span > 1
                    && g.tracks[a][anchor..anchor + span].contains(&GridTrack::Hug)
                    && g.tracks[a][anchor..anchor + span]
                        .iter()
                        .any(|t| matches!(t, GridTrack::Flex(_)))
                {
                    return Err(Error(
                        "spans mixing hug and fractional grid tracks are unsupported",
                    ));
                }
            }
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
        if let Some(parent) = n.parent
            && (nodes[parent].layout.is_none() || !n.in_flow)
        {
            if n.measure_baseline
                && matches!(n.constraints[1], Constraint::Stretch | Constraint::Scale)
            {
                return Err(Error(
                    "baseline text with vertically resizing constraints is unsupported",
                ));
            }
            for axis in 0..2 {
                if matches!(n.constraints[axis], Constraint::Stretch | Constraint::Scale)
                    && n.sizing[axis] != Sizing::Fixed
                {
                    return Err(Error(
                        "stretch/scale constraints require fixed sizing on that axis",
                    ));
                }
                if n.constraints[axis] == Constraint::Scale && nodes[parent].size[axis] == 0. {
                    return Err(Error(
                        "scale constraint requires a nonzero authored parent size",
                    ));
                }
            }
        }
        if n.first_baseline.is_some_and(|b| !b.is_finite()) {
            return Err(Error("invalid first baseline"));
        }
        if let Some(l) = n.layout {
            let gaps = [l.spacing, l.counter_spacing];
            if !l.padding.iter().chain(&gaps).all(|v| v.is_finite())
                || l.padding.iter().any(|&v| v < 0.)
            {
                return Err(Error("invalid layout input"));
            }
            if l.primary_align == Align::Baseline
                || (l.counter_align == Align::Baseline && l.direction != Direction::Horizontal)
            {
                return Err(Error(
                    "baseline alignment requires a horizontal counter axis",
                ));
            }
            if l.counter_align == Align::SpaceBetween {
                return Err(Error("space between applies to the primary axis only"));
            }
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        if n.layout.is_some_and(|l| l.counter_align == Align::Baseline) {
            let mut pending: Vec<usize> = children[i]
                .iter()
                .copied()
                .filter(|&c| nodes[c].in_flow)
                .collect();
            while let Some(c) = pending.pop() {
                if nodes[c].sizing[1] == Sizing::Fill {
                    return Err(Error("fill-height baseline dependencies are unsupported"));
                }
                if nodes[c].rotation != 0. {
                    return Err(Error("rotated baseline descendants are unsupported"));
                }
                pending.extend(&children[c]);
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
    if nodes
        .iter()
        .any(|n| n.measure_baseline && n.first_baseline.is_none())
    {
        return Err(Error(
            "baseline measurement requires solve_measured_with_baselines",
        ));
    }
    if has_vertical_wrap(nodes) {
        let rects = solve_coupled(
            &mut nodes.to_vec(),
            nodes,
            &children,
            [None, None],
            |_, _| Ok(()),
        )?;
        return Ok([rects[0].w, rects[0].h]);
    }
    let mut s = Solver::new(nodes, nodes, &children, 0, vec![None; nodes.len()]);
    let width = s.measure(0);
    // Heights depend on where wrapping stacks break at that width.
    s.arrange(0, width, &mut vec![[0.; 2]; nodes.len()]);
    let height = Solver::new(nodes, nodes, &children, 1, s.lines).measure(0);
    Rect::new(0., 0., width, height).validate()?;
    Ok([width, height])
}

/// The rectangle of every node relative to its parent. The supplied root size
/// overrides authored/hug sizing and is clamped to the root's min/max limits.
pub fn solve(nodes: &[Node], root_size: [f32; 2]) -> Result<Vec<Rect>, Error> {
    let children = children_of(nodes)?;
    if nodes
        .iter()
        .any(|n| n.measure_baseline && n.first_baseline.is_none())
    {
        return Err(Error(
            "baseline measurement requires solve_measured_with_baselines",
        ));
    }
    if !root_size.iter().all(|v| v.is_finite() && *v >= 0.) {
        return Err(Error("invalid layout input"));
    }
    if has_vertical_wrap(nodes) {
        return solve_coupled(
            &mut nodes.to_vec(),
            nodes,
            &children,
            root_size.map(Some),
            |_, _| Ok(()),
        );
    }
    let mut out = vec![Rect::default(); nodes.len()];
    let mut lines = vec![None; nodes.len()];
    for (axis, &root) in root_size.iter().enumerate() {
        let mut s = Solver::new(nodes, nodes, &children, axis, lines);
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
    local_rects(nodes, &mut out);
    for (n, rect) in nodes.iter().zip(&out) {
        validate_bounds(n, rect)?;
    }
    Ok(out)
}

fn has_vertical_wrap(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| {
        n.layout
            .is_some_and(|l| l.wrap && l.direction == Direction::Vertical)
    })
}

/// Resolve cross-axis wrapping dependencies without retaining layout state.
/// A full pass places widths, remeasures leaves at those widths, then places
/// heights. Repeat until both boxes and track membership are stable. Cyclic
/// or excessively deep dependencies fail rather than returning stale boxes.
fn solve_coupled(
    nodes: &mut [Node],
    reference: &[Node],
    children: &[Vec<usize>],
    root_size: [Option<f32>; 2],
    mut measure_heights: impl FnMut(&mut [Node], &[[f32; 2]]) -> Result<(), Error>,
) -> Result<Vec<Rect>, Error> {
    let mut lines = vec![None; nodes.len()];
    let mut previous = None;
    for _ in 0..64 {
        let old_lines = lines.clone();
        let mut axes = [vec![[0.; 2]; nodes.len()], vec![[0.; 2]; nodes.len()]];
        for axis in 0..2 {
            let mut solver = Solver::new(nodes, reference, children, axis, lines);
            let root = match root_size[axis] {
                Some(size) => solver.clamp(0, size),
                None => solver.measure(0),
            };
            axes[axis][0] = [0., root];
            solver.arrange(0, root, &mut axes[axis]);
            lines = solver.lines;
            if axis == 0 {
                if axes[0].iter().any(|v| !v[1].is_finite() || v[1] < 0.) {
                    return Err(Error("invalid coupled layout width"));
                }
                measure_heights(nodes, &axes[0])?;
            }
        }
        let rects: Vec<Rect> = axes[0]
            .iter()
            .zip(&axes[1])
            .map(|(x, y)| Rect::new(x[0], y[0], x[1], y[1]))
            .collect();
        if previous.as_ref() == Some(&rects) && old_lines == lines {
            let mut rects = rects;
            local_rects(nodes, &mut rects);
            for (n, rect) in nodes.iter().zip(&rects) {
                validate_bounds(n, rect)?;
            }
            return Ok(rects);
        }
        previous = Some(rects);
    }
    Err(Error(
        "wrapped layout dependencies did not converge within 64 passes",
    ))
}

fn local_size(n: &Node) -> [f32; 2] {
    [0, 1].map(|a| {
        n.size[a]
            .min(n.max[a])
            .max(n.min[a])
            .max(implicit_min(n, a))
    })
}

fn rotation(n: &Node) -> Transform {
    let (sin, cos) = (f64::from(n.rotation).to_radians()).sin_cos();
    Transform::new([[cos as f32, sin as f32, 0.], [-sin as f32, cos as f32, 0.]])
}

fn rotation_bounds(n: &Node) -> Rect {
    let [w, h] = local_size(n);
    rotation(n).bounds(Rect::new(0., 0., w, h))
}

// Flow placement uses the rotated envelope; painting and descendant layout
// use the original local box. Absolute offsets already name the local origin.
fn local_rects(nodes: &[Node], rects: &mut [Rect]) {
    for (i, n) in nodes.iter().enumerate() {
        if n.rotation != 0. && n.in_flow && n.parent.is_some_and(|p| nodes[p].layout.is_some()) {
            let bounds = rotation_bounds(n);
            let [w, h] = local_size(n);
            rects[i] = Rect::new(rects[i].x - bounds.x, rects[i].y - bounds.y, w, h);
        }
    }
}

fn root_relative(nodes: &[Node], mut rects: Vec<Rect>) -> Result<Solved, Error> {
    let mut transforms = vec![Transform::IDENTITY; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        let mut transform = rotation(n);
        transform.0[0][2] = rects[i].x;
        transform.0[1][2] = rects[i].y;
        if let Some(parent) = n.parent {
            transform = transforms[parent].compose(transform);
        }
        transforms[i] = transform;
        rects[i].x = transform.0[0][2];
        rects[i].y = transform.0[1][2];
        validate_bounds(n, &rects[i])?;
    }
    Ok(Solved { rects, transforms })
}

/// Auto-layout padding and included inside borders cannot be compressed.
fn implicit_min(n: &Node, axis: usize) -> f32 {
    n.layout.map_or(0., |layout| {
        let (start, end) = pads(n, layout, axis);
        start + end
    })
}

fn validate_bounds(n: &Node, rect: &Rect) -> Result<(), Error> {
    rect.validate()?;
    if rect.w < implicit_min(n, 0) || rect.h < implicit_min(n, 1) {
        return Err(Error(
            "layout frame is smaller than its padding and inside strokes",
        ));
    }
    Ok(())
}

/// Builders named after Figma's auto layout panel.
impl Node {
    /// Figma rotation in degrees; fixed local dimensions, no baseline coupling.
    pub fn rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }
    /// Grid with explicit columns and rows. Children specify their grid cell.
    pub fn grid(columns: Vec<GridTrack>, rows: Vec<GridTrack>) -> Self {
        let mut node = Self::row();
        node.grid = Some(Grid {
            tracks: [columns, rows],
            gaps: [0.; 2],
        });
        node
    }
    /// Column and row spacing, in pixels.
    pub fn grid_gaps(mut self, column: f32, row: f32) -> Self {
        self.grid
            .get_or_insert(Grid {
                tracks: [vec![], vec![]],
                gaps: [0.; 2],
            })
            .gaps = [column, row];
        self
    }
    /// Zero-based column/row anchors and positive column/row spans.
    pub fn grid_cell(
        mut self,
        column: usize,
        row: usize,
        column_span: usize,
        row_span: usize,
    ) -> Self {
        self.grid_cell = GridCell {
            column,
            row,
            column_span,
            row_span,
            ..self.grid_cell
        };
        self
    }
    /// Cell alignment for each axis: Min (including AUTO), Center, or Max.
    pub fn grid_align(mut self, horizontal: Align, vertical: Align) -> Self {
        self.grid_cell.align = [horizontal, vertical];
        self
    }
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
    /// Horizontal and vertical constraints, relative to the parent's authored
    /// size. Used only outside auto-layout flow; scale/stretch resize fixed axes.
    pub fn constraints(mut self, horizontal: Constraint, vertical: Constraint) -> Self {
        self.constraints = [horizontal, vertical];
        self
    }
    /// Baseline from the top of this box, typically supplied by text measurement.
    pub fn first_baseline(mut self, baseline: f32) -> Self {
        self.first_baseline = Some(baseline);
        self
    }
    pub fn measure_baseline(mut self) -> Self {
        self.measure_baseline = true;
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
    /// Figma's "wrap", with the gap between lines. Horizontal or vertical stacks.
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
        root_relative(&self.nodes, solve(&self.nodes, size)?)
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
        self.solve_measured_with_baselines(size, |id, width| {
            measure(id, width).map(|value| {
                value.map(|size| Measurement {
                    size,
                    first_baseline: None,
                })
            })
        })
    }

    /// Width-first text measurement including first baselines. Nodes marked
    /// `measure_baseline` must return one at their final solved width.
    pub fn solve_measured_with_baselines(
        &self,
        size: [Option<f32>; 2],
        mut measure: impl FnMut(NodeId, Option<f32>) -> Result<Option<Measurement>, Error>,
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
                    node.size[0] = check(measured.size)?[0];
                }
            }
        }
        let mut measure_heights = |nodes: &mut [Node], x: &[[f32; 2]]| -> Result<(), Error> {
            for (i, node) in nodes.iter_mut().enumerate() {
                if node.layout.is_none() && (node.sizing[1] == Sizing::Hug || node.measure_baseline)
                {
                    if !children[i].is_empty() {
                        return Err(Error("measured layout requires a leaf"));
                    }
                    let measured = measure(NodeId(i), Some(x[i][1]))?;
                    if node.measure_baseline && measured.is_none_or(|m| m.first_baseline.is_none())
                    {
                        return Err(Error("text measurement did not supply a first baseline"));
                    }
                    if let Some(measured) = measured {
                        let size = check(measured.size)?;
                        if node.sizing[1] == Sizing::Hug {
                            node.size[1] = size[1];
                        }
                        if measured.first_baseline.is_some_and(|b| !b.is_finite()) {
                            return Err(Error("invalid measured first baseline"));
                        }
                        if node.measure_baseline {
                            node.first_baseline = measured.first_baseline;
                        }
                    }
                }
            }
            Ok(())
        };
        if has_vertical_wrap(&nodes) {
            let rects = solve_coupled(
                &mut nodes,
                &self.nodes,
                &children,
                size,
                &mut measure_heights,
            )?;
            return root_relative(&nodes, rects);
        }
        let mut widths = Solver::new(&nodes, &self.nodes, &children, 0, vec![None; nodes.len()]);
        let width = match size[0] {
            Some(v) => widths.clamp(0, v),
            None => widths.measure(0),
        };
        let mut x = vec![[0.; 2]; nodes.len()];
        x[0] = [0., width];
        widths.arrange(0, width, &mut x);
        let wrapped_lines = widths.lines;
        measure_heights(&mut nodes, &x)?;
        let mut heights = Solver::new(&nodes, &self.nodes, &children, 1, wrapped_lines);
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
        local_rects(&nodes, &mut rects);
        root_relative(&nodes, rects)
    }

    /// Places every node with the root at the size its own sizing gives:
    /// hugging axes wrap the content, other axes keep the root's size.
    pub fn solve_natural(&self) -> Result<Solved, Error> {
        self.solve(measure(&self.nodes)?)
    }
}

/// Root-relative local origins and local dimensions, indexed by [`NodeId`].
/// Use [`Solved::transform`] to paint rotated boxes and their descendants.
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    rects: Vec<Rect>,
    transforms: Vec<Transform>,
}

impl Solved {
    /// Local-to-root transform, including all ancestor rotations and placement.
    pub fn transform(&self, id: NodeId) -> Transform {
        self.transforms[id.0]
    }

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
        assert!(column.solve([10., 10.]).is_ok());
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
        assert_eq!(tree.solve([12., 12.]).unwrap().size(), [26., 26.]);
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

#[cfg(test)]
mod constraint_tests {
    use super::*;

    #[test]
    fn constraints_use_authored_parent_bounds_and_ignore_flow() {
        let mut tree = Tree::new(Node::fixed([200., 100.]));
        let modes = [
            Constraint::Min,
            Constraint::Center,
            Constraint::Max,
            Constraint::Stretch,
            Constraint::Scale,
        ];
        let ids = modes.map(|mode| {
            tree.add(
                tree.root(),
                Node::fixed([60., 20.])
                    .absolute([20., 10.])
                    .constraints(mode, mode),
            )
        });
        let solved = tree.solve([300., 150.]).unwrap();
        for (id, expected) in ids.into_iter().zip([
            [20., 10., 60., 20.],
            [70., 35., 60., 20.],
            [120., 60., 60., 20.],
            [20., 10., 160., 70.],
            [30., 15., 90., 30.],
        ]) {
            assert_eq!(
                solved[id],
                Rect::new(expected[0], expected[1], expected[2], expected[3])
            );
        }
        assert!(
            tree.solve([100., 50.]).is_err(),
            "negative stretch spans require unsupported flipped geometry"
        );
        tree.node_mut(tree.root()).size = [0., 100.];
        assert!(tree.solve([100., 100.]).is_err());
        let mut flow = Tree::new(Node::row().size([200., 100.]).padding(10.));
        let child = flow.add(
            flow.root(),
            Node::fixed([60., 20.]).constraints(Constraint::Max, Constraint::Scale),
        );
        assert_eq!(
            flow.solve([300., 150.]).unwrap()[child],
            Rect::new(10., 10., 60., 20.)
        );
    }

    #[test]
    fn constrained_text_measures_final_width_and_preserves_bottom_anchor() {
        let mut tree = Tree::new(Node::fixed([200., 100.]));
        let text = tree.add(
            tree.root(),
            Node::fixed([100., 20.])
                .height(Sizing::Hug)
                .absolute([10., 50.])
                .constraints(Constraint::Stretch, Constraint::Max),
        );
        let solved = tree
            .solve_measured([Some(300.), Some(150.)], |id, width| {
                assert_eq!(id, text);
                assert_eq!(width, Some(200.));
                Ok(Some([200., 40.]))
            })
            .unwrap();
        assert_eq!(solved[text], Rect::new(10., 80., 200., 40.));
        assert_eq!(tree.node(text).size, [100., 20.]);
        tree.node_mut(text).constraints[1] = Constraint::Stretch;
        assert!(tree.solve([300., 150.]).is_err());
    }

    #[test]
    fn scale_and_stretch_obey_size_limits_without_moving_start_anchor() {
        let mut tree = Tree::new(Node::fixed([200., 100.]));
        let stretch = tree.add(
            tree.root(),
            Node::fixed([60., 20.])
                .absolute([20., 10.])
                .constraints(Constraint::Stretch, Constraint::Min)
                .max([80., 100.]),
        );
        let scale = tree.add(
            tree.root(),
            Node::fixed([60., 20.])
                .absolute([20., 10.])
                .constraints(Constraint::Scale, Constraint::Min)
                .max([80., 100.]),
        );
        let solved = tree.solve([300., 150.]).unwrap();
        assert_eq!(solved[stretch], Rect::new(20., 10., 80., 20.));
        assert_eq!(solved[scale], Rect::new(30., 10., 80., 20.));
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;
    #[test]
    fn baselines_expand_hug_height_and_inherit_through_containers() {
        let mut tree = Tree::new(Node::row().align(Align::Min, Align::Baseline).padding(10.));
        let small = tree.add(tree.root(), Node::fixed([40., 24.]).first_baseline(18.));
        let large = tree.add(tree.root(), Node::fixed([60., 40.]).first_baseline(32.));
        tree.add(tree.root(), Node::fixed([20., 50.]));
        let nested = tree.add(tree.root(), Node::column().padding(5.));
        let inner = tree.add(nested, Node::fixed([20., 24.]).first_baseline(18.));
        tree.add(nested, Node::fixed([20., 24.]).first_baseline(18.));
        let solved = tree.solve_natural().unwrap();
        assert_eq!(solved.size()[1], 105.);
        assert_eq!(solved[small].y, 42.);
        assert_eq!(solved[large].y, 28.);
        assert_eq!(solved[nested].y, 37.);
        assert_eq!(solved[inner].y, 42.);
        tree.node_mut(small).sizing[1] = Sizing::Fill;
        assert!(tree.solve_natural().is_err());
    }
    #[test]
    fn fixed_text_requires_baseline_measurement_and_rejects_bad_metrics() {
        let mut tree = Tree::new(Node::row().align(Align::Min, Align::Baseline));
        let text = tree.add(tree.root(), Node::fixed([40., 24.]).measure_baseline());
        tree.add(tree.root(), Node::fixed([20., 50.]));
        assert!(tree.solve([100., 100.]).is_err());
        assert!(
            tree.solve_measured([Some(100.), None], |_, _| Ok(Some([40., 24.])))
                .is_err()
        );
        let solved = tree
            .solve_measured_with_baselines([Some(100.), None], |id, width| {
                assert_eq!(id, text);
                assert_eq!(width, Some(40.));
                Ok(Some(Measurement {
                    size: [40., 24.],
                    first_baseline: Some(18.),
                }))
            })
            .unwrap();
        assert_eq!(solved[text].y, 32.);
        assert_eq!(solved.size(), [100., 56.]);
        assert!(
            tree.solve_measured_with_baselines([Some(100.), None], |_, _| Ok(Some(Measurement {
                size: [40., 24.],
                first_baseline: Some(f32::NAN)
            })))
            .is_err()
        );
        assert!(
            Tree::new(Node::column().align(Align::Min, Align::Baseline))
                .solve([100., 100.])
                .is_err()
        );
    }
}

#[cfg(test)]
mod vertical_wrap_tests {
    use super::*;

    #[test]
    fn vertical_wrap_recomputes_hug_width_and_nested_parent_size() {
        let mut tree = Tree::new(Node::row().size([1., 140.]).width(Sizing::Hug).gap(7.));
        let column = tree.add(
            tree.root(),
            Node::column().height(Sizing::Fill).wrap(12.).gap(10.),
        );
        let ids = [30., 40., 50., 60.].map(|w| tree.add(column, Node::fixed([w, 50.])));
        let sibling = tree.add(tree.root(), Node::fixed([20., 30.]));
        let result = tree.solve_natural().unwrap();
        assert_eq!(result.size(), [139., 140.]);
        assert_eq!(result[ids[2]], Rect::new(52., 0., 50., 50.));
        assert_eq!(result[sibling].x, 119.);
        tree.node_mut(tree.root()).size[1] = 240.;
        let result = tree.solve_natural().unwrap();
        assert_eq!(result.size(), [87., 240.]);
        assert_eq!(result[ids[3]], Rect::new(0., 180., 60., 50.));
        assert_eq!(result[sibling].x, 67.);
    }

    #[test]
    fn vertical_wrap_remeasures_text_at_final_column_width() {
        let mut tree = Tree::new(Node::column().size([100., 100.]).wrap(10.).gap(5.));
        let a = tree.add(
            tree.root(),
            Node::fixed([1., 1.])
                .width(Sizing::Fill)
                .height(Sizing::Hug),
        );
        let b = tree.add(
            tree.root(),
            Node::fixed([1., 1.])
                .width(Sizing::Fill)
                .height(Sizing::Hug),
        );
        let result = tree
            .solve_measured([Some(100.), Some(100.)], |_, width| {
                let width = width.unwrap();
                Ok(Some([width, if width < 60. { 80. } else { 55. }]))
            })
            .unwrap();
        assert_eq!(result[a], Rect::new(0., 0., 45., 80.));
        assert_eq!(result[b], Rect::new(55., 0., 45., 80.));
        assert_eq!(tree.node(a).size, [1., 1.]);
        let error = tree
            .solve_measured([Some(100.), Some(100.)], |_, width| {
                let width = width.unwrap();
                Ok(Some([width, if width < 60. { 20. } else { 60. }]))
            })
            .unwrap_err();
        assert!(error.0.contains("did not converge"));
    }
}

#[cfg(test)]
mod implicit_min_tests {
    use super::*;

    #[test]
    fn padding_and_inside_borders_expand_fixed_roots_even_past_maximum() {
        for align in [
            StrokeAlign::Inside,
            StrokeAlign::Center,
            StrokeAlign::Outside,
        ] {
            for included in [false, true] {
                let tree = Tree::new(
                    Node::row()
                        .size([50., 50.])
                        .paddings([3., 5., 7., 9.])
                        .stroke_weights([2., 4., 6., 8.])
                        .stroke_align(align)
                        .strokes_included_in_layout(included)
                        .max([12., 12.]),
                );
                let expected = if included && align == StrokeAlign::Inside {
                    [22., 22.]
                } else {
                    [10., 14.]
                };
                assert_eq!(tree.solve([2., 3.]).unwrap().size(), expected);
                assert_eq!(
                    tree.solve_measured([Some(2.), Some(3.)], |_, _| Ok(None))
                        .unwrap()
                        .size(),
                    expected
                );
            }
        }
    }

    #[test]
    fn fill_reserves_implicit_space_before_sharing_the_remainder() {
        let mut tree = Tree::new(Node::row().size([80., 50.]));
        let padded = tree.add(
            tree.root(),
            Node::row()
                .size([30., 30.])
                .width(Sizing::Fill)
                .padding_xy(10., 0.),
        );
        let other = tree.add(tree.root(), Node::fixed([20., 10.]).width(Sizing::Fill));
        let result = tree.solve_natural().unwrap();
        assert_eq!(result[padded].w, 50.);
        assert_eq!(result[other].w, 30.);
        tree.node_mut(padded).max[0] = 25.;
        let result = tree.solve_natural().unwrap();
        assert_eq!(result[padded].w, 25.);
        assert_eq!(result[other].w, 55.);
        tree.node_mut(padded).max[0] = 12.;
        let result = tree.solve_natural().unwrap();
        assert_eq!(result[padded].w, 20.);
        assert_eq!(result[other].w, 60.);
    }

    #[test]
    fn wrapped_frames_break_at_their_implicit_minimum() {
        let mut tree = Tree::new(Node::row().size([35., 100.]).wrap(4.).gap(2.));
        let frame = Node::row()
            .size([0., 10.])
            .width(Sizing::Fill)
            .stroke_weight(10.)
            .strokes_included_in_layout(true);
        let a = tree.add(tree.root(), frame.clone());
        let b = tree.add(tree.root(), frame);
        let result = tree.solve_natural().unwrap();
        assert_eq!(result[a], Rect::new(0., 0., 35., 20.));
        assert_eq!(result[b], Rect::new(0., 24., 35., 20.));
    }
}

#[cfg(test)]
mod grid_rotation_tests {
    use super::*;
    #[test]
    fn grid_tracks_spans_alignment_and_absolute_children() {
        let mut tree = Tree::new(
            Node::grid(
                vec![
                    GridTrack::Fixed(50.),
                    GridTrack::Flex(1.),
                    GridTrack::Flex(2.),
                ],
                vec![GridTrack::Fixed(40.), GridTrack::Flex(1.)],
            )
            .size([320., 140.])
            .padding(10.)
            .grid_gaps(10., 10.),
        );
        let span = tree.add(tree.root(), Node::fill().grid_cell(1, 0, 2, 1));
        let end = tree.add(
            tree.root(),
            Node::fixed([20., 10.])
                .grid_cell(2, 1, 1, 1)
                .grid_align(Align::Max, Align::Max),
        );
        let absolute = tree.add(tree.root(), Node::fixed([5., 5.]).absolute([3., 4.]));
        let solved = tree.solve([320., 140.]).unwrap();
        assert_eq!(solved[span], Rect::new(70., 10., 240., 40.));
        assert_eq!(solved[end], Rect::new(290., 120., 20., 10.));
        assert_eq!(solved[absolute], Rect::new(3., 4., 5., 5.));
        let smaller = tree.solve([230., 110.]).unwrap();
        assert_eq!(smaller[span].w, 150.);
    }
    #[test]
    fn hug_grid_spans_distribute_only_missing_content_and_measure_text() {
        let mut tree = Tree::new(
            Node::grid(
                vec![GridTrack::Hug, GridTrack::Hug],
                vec![GridTrack::Hug, GridTrack::Hug],
            )
            .width(Sizing::Hug)
            .height(Sizing::Hug)
            .grid_gaps(10., 5.),
        );
        tree.add(tree.root(), Node::fixed([100., 30.]).grid_cell(0, 0, 2, 1));
        let a = tree.add(tree.root(), Node::fixed([20., 20.]).grid_cell(0, 1, 1, 1));
        let b = tree.add(tree.root(), Node::fixed([10., 20.]).grid_cell(1, 1, 1, 1));
        let solved = tree.solve_natural().unwrap();
        assert_eq!(solved.size(), [100., 55.]);
        assert_eq!(solved[b].x, 60.);
        tree.node_mut(a).sizing[0] = Sizing::Fill;
        let solved = tree.solve_natural().unwrap();
        assert_eq!(solved[a].w, 40.);
        assert_eq!(solved[b].x, 50.);
        let mut tree = Tree::new(
            Node::grid(vec![GridTrack::Flex(1.)], vec![GridTrack::Hug])
                .size([100., 1.])
                .height(Sizing::Hug)
                .padding(10.),
        );
        let text = tree.add(
            tree.root(),
            Node::fixed([1., 1.])
                .width(Sizing::Fill)
                .height(Sizing::Hug),
        );
        let solved = tree
            .solve_measured([Some(100.), None], |id, width| {
                Ok((id == text).then(|| [width.unwrap(), (200. / width.unwrap()).ceil() * 10.]))
            })
            .unwrap();
        assert_eq!(solved.size(), [100., 50.]);
        assert_eq!(solved[text], Rect::new(10., 10., 80., 30.));
    }
    #[test]
    fn rotation_uses_envelopes_and_composes_descendant_placement() {
        let mut tree = Tree::new(
            Node::row()
                .width(Sizing::Hug)
                .height(Sizing::Hug)
                .padding(10.)
                .gap(5.),
        );
        let rotated = tree.add(
            tree.root(),
            Node::row().size([60., 20.]).rotation(90.).padding(2.),
        );
        let nested = tree.add(rotated, Node::fill());
        let sibling = tree.add(tree.root(), Node::fixed([30., 10.]));
        let solved = tree.solve_natural().unwrap();
        assert_eq!(solved.size(), [75., 80.]);
        assert_eq!(solved[rotated], Rect::new(10., 70., 60., 20.));
        assert_eq!(solved[sibling].x, 35.);
        assert_eq!(solved[nested], Rect::new(12., 68., 56., 16.));
        let p = solved.transform(nested).point([10., 0.]);
        assert!((p[0] - 12.).abs() < 0.0001 && (p[1] - 58.).abs() < 0.0001);
        let measured = tree.solve_measured([None, None], |_, _| Ok(None)).unwrap();
        assert_eq!(solved, measured);
    }
    #[test]
    fn invalid_grid_and_rotation_semantics_fail_closed() {
        for track in [
            GridTrack::Flex(0.),
            GridTrack::Fixed(-1.),
            GridTrack::Flex(f32::NAN),
        ] {
            assert!(
                Tree::new(Node::grid(vec![track], vec![GridTrack::Hug]))
                    .solve([100., 100.])
                    .is_err()
            );
        }
        let mut tree = Tree::new(Node::grid(
            vec![GridTrack::Hug, GridTrack::Flex(1.)],
            vec![GridTrack::Hug],
        ));
        let child = tree.add(tree.root(), Node::fixed([10., 10.]).grid_cell(0, 0, 2, 1));
        assert!(tree.solve([100., 100.]).is_err());
        tree.node_mut(child).grid_cell.column_span = 0;
        assert!(tree.solve([100., 100.]).is_err());
        let mut tree = Tree::new(Node::row());
        let child = tree.add(tree.root(), Node::fill().rotation(30.));
        assert!(tree.solve([100., 100.]).is_err());
        tree.node_mut(child).sizing = [Sizing::Fixed; 2];
        tree.node_mut(child).rotation = f32::NAN;
        assert!(tree.solve([100., 100.]).is_err());
    }
}

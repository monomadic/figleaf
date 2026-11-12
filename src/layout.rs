//! Figma auto layout solver: fixed, hug and fill sizing, horizontal and vertical
//! stacks, wrapping, padding, gaps, alignment, absolute children and min/max
//! limits.
//!
//! Build a [`Tree`] from the values in Figma's auto layout panel, solve it for
//! a size, and draw each thing at the rectangle it gets. Borrowed from figui,
//! where it was checked against positions Figma recorded.
//!
//! Not covered: baseline alignment, rotated children, grid layout, "space
//! between" for wrapped lines, and constraints other than "keep the authored
//! offset". Text does not measure itself: measure it with a text backend and
//! set the node's size.
//!
//! Widths are solved first and heights second. The only thing heights take
//! from widths is where a wrapping stack broke its lines; text reflow would
//! need more.
use crate::{Error, Rect};
use std::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    Fixed,
    /// Shrink to content. Only auto layout frames hug; other nodes keep their
    /// authored size (text measurement is not implemented).
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
        let (start, end) = pads(l, self.axis);
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
            let heights: f32 = lines.iter().map(|line| self.thickness(line)).sum();
            heights + l.counter_spacing * lines.len().saturating_sub(1) as f32
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
        from.iter().map(|&c| self.measure(c)).fold(0., f32::max)
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
        let (start, end) = pads(l, self.axis);
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
                let thick: Vec<f32> = lines.iter().map(|line| self.thickness(line)).collect();
                let gaps = l.counter_spacing * lines.len().saturating_sub(1) as f32;
                let spare = inner - thick.iter().sum::<f32>() - gaps;
                let mut pos = match l.counter_align {
                    Align::Center => start + spare / 2.,
                    Align::Max => start + spare,
                    _ => start,
                };
                bands.clear();
                for t in thick {
                    bands.push((pos, t));
                    pos += t + l.counter_spacing;
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
        let fill = |s: &Self, c: usize| s.nodes[c].sizing[s.axis] == Sizing::Fill;
        let mut sizes: Vec<Option<f32>> = flow
            .iter()
            .map(|&c| (!fill(self, c)).then(|| self.measure(c)))
            .collect();
        let gaps = l.spacing * flow.len().saturating_sub(1) as f32;
        loop {
            let open: Vec<usize> = (0..flow.len()).filter(|&k| sizes[k].is_none()).collect();
            if open.is_empty() {
                break;
            }
            let used: f32 = sizes.iter().flatten().sum();
            let share = ((inner - gaps - used) / open.len() as f32).max(0.);
            let mut frozen = false;
            for &k in &open {
                let v = self.clamp(flow[k], share);
                if v != share {
                    sizes[k] = Some(v);
                    frozen = true;
                }
            }
            if !frozen {
                for &k in &open {
                    sizes[k] = Some(share);
                }
            }
        }
        sizes.into_iter().map(Option::unwrap).collect()
    }
}

fn primary(l: AutoLayout) -> usize {
    match l.direction {
        Direction::Horizontal => 0,
        Direction::Vertical => 1,
    }
}

fn pads(l: AutoLayout, axis: usize) -> (f32, f32) {
    (l.padding[axis], l.padding[axis + 2])
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
        if !finite || n.max.iter().any(|v| v.is_nan()) {
            return Err(Error("invalid layout input"));
        }
        if let Some(l) = n.layout {
            let gaps = [l.spacing, l.counter_spacing];
            if !l.padding.iter().chain(&gaps).all(|v| v.is_finite()) {
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
    Ok([width, height])
}

/// The rectangle of every node relative to its parent, with the root at `root_size`.
pub fn solve(nodes: &[Node], root_size: [f32; 2]) -> Result<Vec<Rect>, Error> {
    let children = children_of(nodes)?;
    if !root_size.iter().all(|v| v.is_finite()) {
        return Err(Error("invalid layout input"));
    }
    let mut out = vec![Rect::default(); nodes.len()];
    let mut lines = vec![None; nodes.len()];
    for (axis, &root) in root_size.iter().enumerate() {
        let mut s = Solver::new(nodes, &children, axis, lines);
        let mut axis_out = vec![[0f32; 2]; nodes.len()];
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
    Ok(out)
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
    fn stack_mut(&mut self) -> &mut AutoLayout {
        self.layout.get_or_insert(AutoLayout {
            direction: Direction::Horizontal,
            padding: [0.; 4],
            spacing: 0.,
            primary_align: Align::Min,
            counter_align: Align::Min,
            wrap: false,
            counter_spacing: 0.,
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

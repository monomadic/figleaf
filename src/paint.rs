//! Shapes and text described with the values Figma's inspector shows, collected
//! in a [`Canvas`] in paint order.
//!
//! A [`Shape`] lowers to [`Layer`]s: plain coverage problems a renderer can draw
//! without knowing what a stroke alignment or a spread is. The wgpu renderer
//! draws them; a host with another renderer can draw them itself.
use crate::{Color, Rect, text};

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

/// A rectangle or ellipse with a fill, a stroke and effects.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub rect: Rect,
    pub ellipse: bool,
    /// Top-left, top-right, bottom-right, bottom-left. Ignored by ellipses.
    pub radii: [f32; 4],
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
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
            ellipse: false,
            radii: [0.; 4],
            fill: None,
            stroke: None,
            shadows: Vec::new(),
            opacity: 1.,
            clip: None,
        }
    }
    /// The ellipse inscribed in `rect`.
    pub fn ellipse(rect: Rect) -> Self {
        Self {
            ellipse: true,
            ..Self::rect(rect)
        }
    }
    pub fn radius(self, radius: f32) -> Self {
        self.radii([radius; 4])
    }
    pub fn radii(mut self, radii: [f32; 4]) -> Self {
        self.radii = radii;
        self
    }
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }
    pub fn stroke(mut self, width: f32, color: Color, align: StrokeAlign) -> Self {
        self.stroke = Some(Stroke {
            width,
            color,
            align,
        });
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
        self.clip = Some(Outline {
            rect,
            radii: [radius; 4],
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
        Outline { rect, radii }
    }

    /// Why the shape can't be drawn, if it can't.
    pub fn problem(&self) -> Option<&'static str> {
        let mut numbers = self.radii.to_vec();
        numbers.extend(self.clip.map_or([0.; 4], |c| c.radii));
        numbers.push(self.opacity);
        numbers.extend(self.fill.unwrap_or_default());
        if let Some(s) = self.stroke {
            numbers.push(s.width);
            numbers.extend(s.color);
        }
        for s in &self.shadows {
            numbers.extend([s.blur, s.spread, s.offset[0], s.offset[1]]);
            numbers.extend(s.color);
        }
        if self.rect.validate().is_err() || self.clip.is_some_and(|c| c.rect.validate().is_err()) {
            Some("invalid rectangle")
        } else if !numbers.iter().all(|v| v.is_finite()) {
            Some("non-finite shape value")
        } else if self.radii.iter().any(|r| *r < 0.)
            || self.clip.is_some_and(|c| c.radii.iter().any(|r| *r < 0.))
            || self.stroke.is_some_and(|s| s.width < 0.)
            || self.shadows.iter().any(|s| s.blur < 0.)
        {
            Some("negative radius, stroke width or blur")
        } else {
            None
        }
    }

    /// The shape as coverage layers, back to front: drop shadows, fill,
    /// stroke, inner shadows. Nothing for a shape with a [`problem`](Self::problem).
    pub fn layers(&self, out: &mut Vec<Layer>) {
        if self.problem().is_some() {
            return;
        }
        // How far the stroke reaches outside and inside the shape's edge.
        let (outside, inside) = match self.stroke {
            Some(s) if s.width > 0. => match s.align {
                StrokeAlign::Inside => (0., s.width),
                StrokeAlign::Center => (s.width / 2., s.width / 2.),
                StrokeAlign::Outside => (s.width, 0.),
            },
            _ => (0., 0.),
        };
        // Effects follow the fill and the stroke together.
        let silhouette = self.outline(outside);
        let start = out.len();
        let clip = self.clip.map(|c| {
            let limit = c.rect.w.min(c.rect.h) / 2.;
            Outline {
                rect: c.rect,
                radii: c.radii.map(|r| r.min(limit)),
            }
        });
        let layer = |mode, a, b, color: Color, sigma| Layer {
            mode,
            a,
            b,
            ellipse: self.ellipse,
            color: [
                color[0],
                color[1],
                color[2],
                color[3] * self.opacity.clamp(0., 1.),
            ],
            sigma,
            clip,
        };
        for s in self.shadows.iter().filter(|s| !s.inner) {
            let mut a = self.outline(outside + s.spread);
            a.rect = a.rect.translate(s.offset);
            let knockout = (!s.behind).then_some(silhouette);
            out.push(layer(Mode::Drop, a, knockout, s.color, s.blur / 2.));
        }
        if let Some(color) = self.fill {
            // Two antialiased edges that meet let the background through.
            // Under an opaque stroke the fill can run on to the middle of
            // the stroke's outer part, where it is hidden, and close the seam.
            let opaque = self.stroke.is_some_and(|s| s.color[3] >= 1.);
            let under = if opaque { outside / 2. } else { 0. };
            out.push(layer(Mode::Band, self.outline(under), None, color, 0.));
        }
        if let Some(s) = self.stroke.filter(|s| s.width > 0.) {
            let hole = self.outline(-inside);
            let hole = (hole.rect.w > 0. && hole.rect.h > 0.).then_some(hole);
            out.push(layer(Mode::Band, silhouette, hole, s.color, 0.));
        }
        for s in self.shadows.iter().filter(|s| s.inner) {
            let mut a = self.outline(outside - s.spread);
            a.rect = a.rect.translate(s.offset);
            out.push(layer(
                Mode::Inner,
                a,
                Some(silhouette),
                s.color,
                s.blur / 2.,
            ));
        }
        let mut i = 0;
        out.retain(|l| {
            i += 1;
            i <= start || l.color[3] > 0.
        });
    }
}

/// A rounded rectangle, or the ellipse inscribed in `rect`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub rect: Rect,
    pub radii: [f32; 4],
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

/// One tinted coverage layer, in logical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    pub mode: Mode,
    pub a: Outline,
    pub b: Option<Outline>,
    pub ellipse: bool,
    pub color: Color,
    /// Gaussian standard deviation: half of Figma's blur value.
    pub sigma: f32,
    pub clip: Option<Outline>,
}

impl Layer {
    /// Everything the layer can touch, before clipping.
    pub fn extent(&self) -> Rect {
        match (self.mode, self.b) {
            (Mode::Inner, Some(b)) => b.rect,
            (Mode::Drop, _) => self.a.rect.inflate(3. * self.sigma),
            _ => self.a.rect,
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

/// One line of text in a box: Figma's text layer with a fixed line height.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub string: String,
    /// The line is `line_height` tall and placed in this box by the alignments.
    pub rect: Rect,
    /// Index into the fonts the text backend was built with.
    pub font: usize,
    pub size: f32,
    pub line_height: f32,
    /// Figma's letter spacing in pixels.
    pub letter_spacing: f32,
    pub color: Color,
    pub align: Align,
    pub vertical_align: Align,
    /// Figma's inner shadow effect on a text layer: colour, X and Y, blur.
    /// Spread is not supported on text.
    pub inner_shadow: Option<(Color, [f32; 2], f32)>,
    pub clip: Option<Rect>,
}

impl Text {
    /// Black 12/16 text in font 0, top left in `rect`.
    pub fn new(string: impl Into<String>, rect: Rect) -> Self {
        Self {
            string: string.into(),
            rect,
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
            color: [0., 0., 0., 1.],
            align: Align::Start,
            vertical_align: Align::Start,
            inner_shadow: None,
            clip: None,
        }
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
    pub fn inner_shadow(mut self, color: Color, offset: [f32; 2], blur: f32) -> Self {
        self.inner_shadow = Some((color, offset, blur));
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
    pub fn clip(mut self, clip: Rect) -> Self {
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
    pub fn origin(&self, advance: f32) -> [f32; 2] {
        let place = |align, spare: f32| match align {
            Align::Start => 0.,
            Align::Center => spare / 2.,
            Align::End => spare,
        };
        [
            self.rect.x + place(self.align, self.rect.w - advance),
            self.rect.y + place(self.vertical_align, self.rect.h - self.line_height),
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
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
    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    fn layers(s: &Shape) -> Vec<Layer> {
        let mut out = Vec::new();
        s.layers(&mut out);
        out
    }

    #[test]
    fn stroke_alignment_sets_the_ring() {
        let base = Shape::rect(Rect::new(10., 10., 40., 20.)).radius(6.);
        let ring = |align| {
            let l = layers(&base.clone().stroke(2., hex(0), align));
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
        let l = layers(&s.clone().stroke(2., hex(0), StrokeAlign::Outside));
        assert_eq!(l[0].a.radii, [0.; 4]);
        let l = layers(&s.stroke(5., hex(0), StrokeAlign::Inside));
        assert_eq!(l[0].b, None);
    }

    #[test]
    fn oversized_radii_make_a_pill() {
        let s = Shape::rect(Rect::new(0., 0., 44., 24.)).radius(999.);
        let l = layers(&s.fill(hex(0)));
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
    fn invisible_and_invalid_shapes_draw_nothing() {
        let r = Rect::new(0., 0., 10., 10.);
        assert!(layers(&Shape::rect(r)).is_empty());
        assert!(layers(&Shape::rect(r).fill(hex(0)).opacity(0.)).is_empty());
        let bad = Shape::rect(r).fill(hex(0)).radius(f32::NAN);
        assert!(bad.problem().is_some() && layers(&bad).is_empty());
    }

    #[test]
    fn text_sits_in_its_box() {
        let t = Text::new("On", Rect::new(10., 10., 100., 40.)).size(13., 16.);
        assert_eq!(t.origin(20.), [10., 10.]);
        let t = t.align(Align::Center, Align::Center);
        assert_eq!(t.origin(20.), [50., 22.]);
        assert_eq!(t.align(Align::End, Align::End).origin(20.), [90., 34.]);
    }
}

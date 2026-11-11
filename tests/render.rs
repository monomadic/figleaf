//! Renders through a real GPU device and checks pixels. Synthetic cases from
//! Figma's documented behaviour, not comparisons against Figma output.
#[path = "../examples/support/mod.rs"]
mod support;

use figleaf::{
    Canvas, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa, render::Renderer, text::NoText,
};
use support::{Gpu, Image};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn draw(canvas: &Canvas) -> Image {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    gpu.render(&mut renderer, canvas, [100, 60], 1., [0.; 4], &mut NoText)
}

#[test]
fn fills_are_crisp_on_pixel_edges_and_clip() {
    let mut c = Canvas::new();
    c.shape(Shape::rect(Rect::new(10., 10., 20., 20.)).fill(hex(0xFF0000)));
    c.shape(
        Shape::rect(Rect::new(50., 10., 40., 40.))
            .fill(hex(0x0000FF))
            .clip(Rect::new(50., 10., 20., 40.)),
    );
    let i = draw(&c);
    assert_eq!(i.pixel(10, 10), RED);
    assert_eq!(i.pixel(29, 29), RED);
    assert_eq!(i.pixel(9, 10), CLEAR);
    assert_eq!(i.pixel(30, 29), CLEAR);
    assert_eq!(i.pixel(69, 30), BLUE);
    assert_eq!(i.pixel(70, 30), CLEAR);

    // A rounded clip cuts the corners of what it holds.
    let mut c = Canvas::new();
    let frame = Rect::new(10., 10., 40., 40.);
    c.shape(
        Shape::rect(Rect::new(0., 0., 100., 60.))
            .fill(hex(0xFF0000))
            .clip_rounded(frame, 12.),
    );
    let i = draw(&c);
    assert_eq!(i.pixel(30, 30), RED);
    assert_eq!(i.pixel(30, 10), RED);
    assert_eq!(i.pixel(11, 11), CLEAR);
    assert_eq!(i.pixel(48, 48), CLEAR);
    assert_eq!(i.pixel(9, 30), CLEAR);
    assert_eq!(i.pixel(50, 30), CLEAR);
}

#[test]
fn round_shapes_cut_their_corners() {
    let mut c = Canvas::new();
    c.shape(Shape::ellipse(Rect::new(10., 10., 40., 40.)).fill(hex(0xFF0000)));
    c.shape(
        Shape::rect(Rect::new(55., 10., 40., 40.))
            .radii([12., 0., 0., 0.])
            .fill(hex(0x0000FF)),
    );
    let i = draw(&c);
    assert_eq!(i.pixel(30, 30), RED);
    assert_eq!(i.pixel(11, 11), CLEAR);
    assert_eq!(i.pixel(48, 48), CLEAR);
    // Antialiased somewhere along the left edge of the circle.
    let edge = i.pixel(10, 30)[3];
    assert!(edge > 0 && edge < 255, "{edge}");
    // Only the top-left corner of the rectangle is round.
    assert_eq!(i.pixel(56, 11), CLEAR);
    assert_eq!(i.pixel(93, 11), BLUE);
    assert_eq!(i.pixel(56, 48), BLUE);
}

#[test]
fn strokes_follow_their_alignment() {
    let at = |align| {
        let mut c = Canvas::new();
        c.shape(
            Shape::rect(Rect::new(20., 20., 40., 20.))
                .fill(hex(0x0000FF))
                .stroke(2., hex(0xFF0000), align),
        );
        let i = draw(&c);
        // Just outside the edge, just inside it, and well inside.
        [i.pixel(19, 30), i.pixel(20, 30), i.pixel(23, 30)]
    };
    assert_eq!(at(StrokeAlign::Inside), [CLEAR, RED, BLUE]);
    assert_eq!(at(StrokeAlign::Outside), [RED, BLUE, BLUE]);
    assert_eq!(at(StrokeAlign::Center), [RED, RED, BLUE]);

    // Where a fill meets an opaque outside stroke on a curve, nothing of the
    // background shows through: every pixel well inside the outer edge is
    // fully covered.
    let mut c = Canvas::new();
    c.shape(
        Shape::ellipse(Rect::new(30., 10., 40., 40.))
            .fill(hex(0x0000FF))
            .stroke(3., hex(0xFF0000), StrokeAlign::Outside),
    );
    let i = draw(&c);
    for y in 0..60 {
        for x in 0..100 {
            let (dx, dy) = (x as f32 + 0.5 - 50., y as f32 + 0.5 - 30.);
            if (dx * dx + dy * dy).sqrt() < 21.5 {
                assert_eq!(i.pixel(x, y)[3], 255, "{x} {y}");
            }
        }
    }
}

#[test]
fn shadows_fall_off_outside_and_darken_inside() {
    let mut c = Canvas::new();
    let rect = Rect::new(30., 10., 40., 20.);
    c.shape(
        Shape::rect(rect)
            .radius(4.)
            .fill(hex(0xFFFFFF))
            .shadow(Shadow::drop(hexa(0x000000FF), [0., 4.], 8.))
            .shadow(Shadow::inner(hexa(0xFF0000FF), [0., 3.], 0.)),
    );
    let i = draw(&c);
    // Below the shape the drop shadow fades with distance.
    let (near, far) = (i.pixel(50, 32)[3], i.pixel(50, 40)[3]);
    assert!(near > far && far > 0 && near < 255, "{near} {far}");
    assert_eq!(i.pixel(50, 58)[3], 0);
    // Above the shape the shadow is hidden: it sits 4 lower.
    assert!(i.pixel(50, 7)[3] < 16);
    // The inner shadow paints the top 3 rows red and leaves the rest white.
    assert_eq!(i.pixel(50, 11), RED);
    assert_eq!(i.pixel(50, 20), [255, 255, 255, 255]);
}

#[test]
fn an_unchanged_canvas_uploads_nothing_and_problems_are_reported() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut c = Canvas::new();
    c.shape(Shape::rect(Rect::new(0., 0., 10., 10.)).fill(hex(0)));
    c.shape(Shape::rect(Rect::new(0., 0., f32::NAN, 10.)).fill(hex(0)));
    c.text(Text::new("label", Rect::new(0., 0., 50., 16.)));
    let mut prepare =
        |c: &Canvas| renderer.prepare(&gpu.device, &gpu.queue, c, [64, 64], 2., &mut NoText);
    let first = prepare(&c).unwrap();
    assert!(first.changed && first.uploaded_bytes > 0);
    let second = prepare(&c).unwrap();
    assert_eq!((second.changed, second.uploaded_bytes), (false, 0));
    c.shape(Shape::ellipse(Rect::new(0., 0., 4., 4.)).fill(hex(0)));
    assert!(prepare(&c).unwrap().changed);
    c.clear();
    assert!(prepare(&c).unwrap().changed);
    assert!(!prepare(&c).unwrap().changed);

    let mut c = Canvas::new();
    c.shape(Shape::rect(Rect::new(0., 0., f32::NAN, 10.)).fill(hex(0)));
    c.text(Text::new("label", Rect::new(0., 0., 50., 16.)));
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [64, 64], 2., &mut NoText)
        .unwrap();
    let problems: Vec<_> = renderer
        .problems()
        .iter()
        .map(|p| (p.item, p.reason))
        .collect();
    assert_eq!(problems, [(0, "invalid rectangle"), (1, "no text backend")]);
}

#[cfg(feature = "text-coretext")]
#[test]
fn text_lands_in_its_box_in_its_colour() {
    use figleaf::{
        paint::Align,
        text::{FontSource, coretext::CoreText},
    };
    let font = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: font,
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut c = Canvas::new();
    c.text(
        Text::new("HHHH", Rect::new(20., 10., 60., 40.))
            .size(14., 20.)
            .color(hex(0xFF0000))
            .align(Align::Center, Align::Center),
    );
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert!(renderer.problems().is_empty());
    let inked: Vec<(u32, u32)> = (0..60)
        .flat_map(|y| (0..100).map(move |x| (x, y)))
        .filter(|&(x, y)| i.pixel(x, y)[3] > 128)
        .collect();
    assert!(!inked.is_empty());
    // Over a clear target the result is premultiplied red.
    for &(x, y) in &inked {
        let [r, g, b, a] = i.pixel(x, y);
        assert!(r.abs_diff(a) <= 1 && g == 0 && b == 0);
    }
    // Four 8.4-wide glyphs centred in x 20..80; the 20-tall line centred in y 10..50.
    let xs = inked.iter().map(|p| p.0);
    let ys = inked.iter().map(|p| p.1);
    let (x0, x1) = (xs.clone().min().unwrap(), xs.max().unwrap());
    let (y0, y1) = (ys.clone().min().unwrap(), ys.max().unwrap());
    assert!(
        x0 >= 32 && x1 <= 68 && (x0 + x1).abs_diff(100) <= 3,
        "{x0} {x1}"
    );
    assert!(y0 >= 20 && y1 < 40, "{y0} {y1}");
}

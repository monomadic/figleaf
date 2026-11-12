//! Renders through a real GPU device and checks pixels. Synthetic cases from
//! Figma's documented behaviour, not comparisons against Figma output.
#[path = "../examples/support/mod.rs"]
mod support;

use figleaf::{
    Canvas, LinearGradient, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa, render::Renderer,
    text::NoText,
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

#[test]
fn black_darkens_an_srgb_target_as_it_does_in_figma() {
    // Black at 50% over white is 128 in Figma, which blends encoded values.
    // Blended naively in linear light it would read back as 188.
    let mut gpu = Gpu::new();
    gpu.format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.shape(Shape::rect(Rect::new(10., 10., 20., 20.)).fill(hexa(0x00000080)));
    c.shape(Shape::rect(Rect::new(50., 10., 20., 20.)).fill(hexa(0xFF000080)));
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [1.; 4], &mut NoText);
    let grey = i.pixel(20, 20)[0];
    assert!(grey.abs_diff(128) <= 3, "{grey}");
    // Opaque colours are converted, so they read back as authored.
    c.clear();
    c.shape(Shape::rect(Rect::new(10., 10., 20., 20.)).fill(hex(0x34C759)));
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [1.; 4], &mut NoText);
    let [r, g, b, _] = i.pixel(20, 20);
    assert!(r.abs_diff(0x34) <= 1 && g.abs_diff(0xC7) <= 1 && b.abs_diff(0x59) <= 1);
}

#[test]
fn gradients_run_along_their_axis_and_stack_over_fills() {
    let near = |got: [u8; 4], want: [u8; 4]| {
        assert!(
            got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2),
            "{got:?}"
        );
    };
    let mut c = Canvas::new();
    // Red to blue across 100 pixels.
    c.shape(
        Shape::rect(Rect::new(0., 0., 100., 10.)).gradient(
            LinearGradient::horizontal()
                .stop(0., hex(0xFF0000))
                .stop(1., hex(0x0000FF)),
        ),
    );
    // Opaque white to transparent black: colour and alpha blend separately,
    // so half way is grey at 50% (64 once laid over nothing), not white at 50%.
    c.shape(
        Shape::rect(Rect::new(0., 10., 100., 10.)).gradient(
            LinearGradient::between([0., 0.], [1., 0.])
                .stop(0., hex(0xFFFFFF))
                .stop(1., hexa(0x00000000)),
        ),
    );
    // A hard stop on a pixel edge is crisp; inside a pixel it is shared.
    let hard = LinearGradient::horizontal()
        .stop(0.5, hex(0xFF0000))
        .stop(0.5, hex(0x0000FF));
    c.shape(Shape::rect(Rect::new(10., 20., 20., 10.)).gradient(hard.clone()));
    c.shape(Shape::rect(Rect::new(50., 20., 21., 10.)).gradient(hard));
    // Top to bottom, held at the end stops' colours outside their offsets.
    c.shape(
        Shape::rect(Rect::new(0., 30., 10., 20.)).gradient(
            LinearGradient::vertical()
                .stop(0.25, hex(0xFF0000))
                .stop(0.75, hex(0x0000FF)),
        ),
    );
    // A gradient at 50% over a solid fill of the same shape.
    c.shape(
        Shape::rect(Rect::new(20., 30., 10., 10.))
            .fill(hex(0xFF0000))
            .gradient(
                LinearGradient::vertical()
                    .stop(0., hex(0x0000FF))
                    .opacity(0.5),
            ),
    );
    let i = draw(&c);
    near(i.pixel(0, 5), [254, 0, 1, 255]);
    near(i.pixel(49, 5), [129, 0, 126, 255]);
    near(i.pixel(99, 5), [1, 0, 254, 255]);
    near(i.pixel(49, 15), [64, 64, 64, 129]);
    assert_eq!((i.pixel(19, 25), i.pixel(20, 25)), (RED, BLUE));
    assert_eq!((i.pixel(59, 25), i.pixel(61, 25)), (RED, BLUE));
    near(i.pixel(60, 25), [128, 0, 128, 255]);
    assert_eq!((i.pixel(5, 31), i.pixel(5, 48)), (RED, BLUE));
    near(i.pixel(5, 40), [115, 0, 140, 255]);
    near(i.pixel(25, 35), [128, 0, 128, 255]);

    // On an sRGB target the stops still blend as encoded values.
    let mut gpu = Gpu::new();
    gpu.format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.shape(
        Shape::rect(Rect::new(0., 0., 100., 10.)).gradient(
            LinearGradient::horizontal()
                .stop(0., hex(0x000000))
                .stop(1., hex(0xFFFFFF)),
        ),
    );
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [1.; 4], &mut NoText);
    near(i.pixel(49, 5), [126, 126, 126, 255]);
}

#[test]
fn a_host_can_draw_a_range_of_items() {
    // Three squares; only the middle one is asked for.
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut c = Canvas::new();
    for x in [10., 40., 70.] {
        c.shape(Shape::rect(Rect::new(x, 10., 20., 20.)).fill(hex(0xFF0000)));
    }
    let i = gpu.render_items(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText, 1..2);
    assert_eq!(i.pixel(20, 20), CLEAR);
    assert_eq!(i.pixel(50, 20), RED);
    assert_eq!(i.pixel(80, 20), CLEAR);
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

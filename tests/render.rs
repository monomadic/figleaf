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

#[cfg(feature = "text-coretext")]
#[test]
fn wrapped_text_fills_its_lines_and_is_cached() {
    use figleaf::text::{FontSource, coretext::CoreText};
    let font = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: font,
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    // Glyphs are 6 wide at size 10: "HHHH HHHH" is 54 and does not fit 40.
    let text = Text::new("HHHH HHHH\nHH", Rect::new(10., 0., 40., 60.))
        .size(10., 20.)
        .color(hex(0xFF0000))
        .wrap();
    let mut c = Canvas::new();
    c.text(text.clone());
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert!(renderer.problems().is_empty());
    let ink = |x0: u32, x1: u32, line: u32| {
        (line * 20..line * 20 + 20)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .any(|(x, y)| i.pixel(x, y)[3] > 128)
    };
    // Three lines: four glyphs, four glyphs, two glyphs; nothing past them.
    assert!(ink(10, 34, 0) && ink(10, 34, 1) && ink(10, 22, 2));
    assert!(!ink(35, 100, 0) && !ink(35, 100, 1) && !ink(23, 100, 2));

    // Unchanged, nothing is measured, rasterized or uploaded again.
    let again = renderer
        .prepare(
            &gpu.device,
            &gpu.queue,
            &c,
            [100, 60],
            1.,
            &mut figleaf::text::NoText,
        )
        .unwrap();
    assert_eq!((again.changed, again.uploaded_bytes), (false, 0));

    // Without wrapping, a line break is reported, not dropped quietly.
    let mut single = text;
    single.wrap = false;
    c.clear();
    c.text(single);
    gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert_eq!(renderer.problems().len(), 1);
}

#[test]
fn a_line_has_round_ends_and_runs_either_way() {
    let mut c = Canvas::new();
    // Level, 10 thick: covers y 15..25 between the ends, and rounds past them.
    c.shape(Shape::line([20., 20.], [60., 20.], 10.).fill(hex(0xFF0000)));
    // A diagonal that runs up and to the left.
    c.shape(Shape::line([90., 50.], [70., 30.], 4.).fill(hex(0x0000FF)));
    let i = draw(&c);
    assert_eq!(i.pixel(40, 15), RED);
    assert_eq!(i.pixel(40, 24), RED);
    assert_eq!(i.pixel(40, 14), CLEAR);
    assert_eq!(i.pixel(40, 25), CLEAR);
    // The round end reaches 5 past the end point on the axis, not at the corner.
    assert_eq!(i.pixel(16, 20), RED);
    assert_eq!(i.pixel(63, 19), RED);
    assert_eq!(i.pixel(16, 15), CLEAR);
    assert_eq!(i.pixel(65, 20), CLEAR);
    assert_eq!(i.pixel(80, 40), BLUE);
    assert_eq!(i.pixel(75, 35), BLUE);
    assert_eq!(i.pixel(84, 36), CLEAR);
    assert_eq!(i.pixel(66, 26), CLEAR);
}

#[cfg(feature = "text-coretext")]
#[test]
fn a_text_drop_shadow_falls_below_the_glyphs() {
    use figleaf::text::{FontSource, coretext::CoreText};
    let font = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: font,
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let text = Text::new("HHHH", Rect::new(20., 10., 60., 20.))
        .size(14., 20.)
        .color(hex(0xFF0000));
    let render = |renderer: &mut Renderer, backend: &mut CoreText, text: Text| {
        let mut c = Canvas::new();
        c.text(text);
        let i = gpu.render(renderer, &c, [100, 60], 1., [0.; 4], backend);
        assert!(renderer.problems().is_empty());
        i
    };
    let plain = render(&mut renderer, &mut backend, text.clone());
    let shadowed = render(
        &mut renderer,
        &mut backend,
        text.drop_shadow(hex(0x0000FF), [0., 12.], 0.),
    );
    // Every glyph pixel is as it was; the same shape shows in blue 12 lower.
    let mut below = 0;
    for y in 0..48 {
        for x in 0..100 {
            let ink = plain.pixel(x, y);
            if ink[3] == 255 {
                assert_eq!(shadowed.pixel(x, y), ink);
                if plain.pixel(x, y + 12)[3] == 0 {
                    assert_eq!(shadowed.pixel(x, y + 12), BLUE);
                    below += 1;
                }
            }
        }
    }
    assert!(below > 4, "{below}");
}

#[test]
fn individual_frame_borders_join_without_seams_or_phantom_sides() {
    let mut c = Canvas::new();
    c.shape(
        Shape::rect(Rect::new(10., 10., 30., 30.))
            .radius(10.)
            .strokes([0., 0., 4., 0.], hex(0xFF0000), StrokeAlign::Inside),
    );
    c.shape(Shape::rect(Rect::new(55., 10., 30., 30.)).strokes(
        [2., 4., 6., 0.],
        hex(0x0000FF),
        StrokeAlign::Outside,
    ));
    let i = draw(&c);
    assert_eq!(i.pixel(25, 38), RED);
    assert_eq!(i.pixel(25, 35), CLEAR);
    assert_eq!(i.pixel(10, 25), CLEAR);
    assert_eq!(i.pixel(25, 10), CLEAR);
    // Rounded top corners must not leave an antialias fringe when their
    // border widths are zero: outer and inner coverage cancel exactly.
    for y in 8..20 {
        for x in 8..42 {
            assert_eq!(i.pixel(x, y), CLEAR, "{x},{y}");
        }
    }
    assert_eq!(i.pixel(70, 9), BLUE);
    assert_eq!(i.pixel(87, 25), BLUE);
    assert_eq!(i.pixel(70, 44), BLUE);
    assert_eq!(i.pixel(54, 25), CLEAR);
    assert_eq!(i.pixel(70, 25), CLEAR);
}

#[test]
fn individual_border_changes_upload_once_and_invalid_widths_are_reported() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut c = Canvas::new();
    let shape = Shape::rect(Rect::new(10., 10., 30., 30.))
        .radius(8.)
        .strokes([2., 4., 6., 8.], hex(0xFF0000), StrokeAlign::Center);
    c.shape(shape.clone());
    let a = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(a.changed);
    let b = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!b.changed);
    assert_eq!(b.uploaded_bytes, 0);
    c.clear();
    c.shape(shape.strokes([2., 4., -1., 8.], hex(0xFF0000), StrokeAlign::Center));
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(renderer.problems().len(), 1);
}

fn vector_square(
    path: figleaf::VectorPath,
    lo: f32,
    hi: f32,
    reverse: bool,
) -> figleaf::VectorPath {
    let points = if reverse {
        [[lo, lo], [lo, hi], [hi, hi], [hi, lo]]
    } else {
        [[lo, lo], [hi, lo], [hi, hi], [lo, hi]]
    };
    path.move_to(points[0])
        .line_to(points[1])
        .line_to(points[2])
        .line_to(points[3])
        .close()
}

#[test]
fn vector_winding_holes_and_regions_do_not_drop_or_double_paint() {
    use figleaf::{VectorPath, WindingRule};
    for (rule, reverse, hole) in [
        (WindingRule::EvenOdd, false, true),
        (WindingRule::NonZero, false, false),
        (WindingRule::NonZero, true, true),
    ] {
        let path = vector_square(
            vector_square(VectorPath::new(rule), 0., 40., false),
            10.,
            30.,
            reverse,
        );
        let mut c = Canvas::new();
        c.shape(Shape::vector(Rect::new(10., 10., 40., 40.), vec![path]).fill(hex(0xFF0000)));
        let i = draw(&c);
        assert_eq!(i.pixel(15, 15), RED);
        assert_eq!(i.pixel(30, 30), if hole { CLEAR } else { RED });
    }
    let a = vector_square(VectorPath::new(WindingRule::NonZero), 0., 30., false);
    let b = vector_square(VectorPath::new(WindingRule::EvenOdd), 10., 40., false);
    let mut c = Canvas::new();
    c.shape(Shape::vector(Rect::new(10., 10., 40., 40.), vec![a, b]).fill(hexa(0xFF000080)));
    let i = draw(&c);
    assert_eq!(i.pixel(15, 15)[3], 128);
    assert_eq!(i.pixel(30, 30)[3], 128); // overlap is a union, not 75% alpha
    assert_eq!(i.pixel(45, 45)[3], 128);
}

#[test]
fn vector_curves_open_contours_clips_and_gradients_render() {
    use figleaf::{VectorPath, WindingRule};
    let lens = VectorPath::new(WindingRule::NonZero)
        .move_to([0., 20.])
        .quadratic_to([20., -20.], [40., 20.])
        .cubic_to([30., 40.], [10., 40.], [0., 20.]);
    let mut c = Canvas::new();
    c.shape(
        Shape::vector(Rect::new(10., 10., 40., 40.), vec![lens])
            .gradient(
                LinearGradient::horizontal()
                    .stop(0., hex(0xFF0000))
                    .stop(1., hex(0x0000FF)),
            )
            .clip(Rect::new(10., 10., 25., 40.)),
    );
    let i = draw(&c);
    assert_eq!(i.pixel(10, 10), CLEAR);
    let p = i.pixel(25, 30);
    assert_eq!(p[3], 255);
    assert!(p[0] > p[2] && p[2] > 0);
    assert_eq!(i.pixel(40, 30), CLEAR);
    // Open triangle closes for fill; a line with no area stays invisible.
    c.clear();
    c.shape(
        Shape::vector(
            Rect::new(0., 0., 40., 40.),
            vec![
                VectorPath::new(WindingRule::EvenOdd)
                    .move_to([10., 10.])
                    .line_to([40., 10.])
                    .line_to([10., 40.]),
            ],
        )
        .fill(hex(0xFF0000)),
    );
    let i = draw(&c);
    assert_eq!(i.pixel(15, 15), RED);
    assert_eq!(i.pixel(35, 35), CLEAR);
}

#[test]
fn vector_cache_tracks_geometry_scale_and_problems() {
    use figleaf::{VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut c = Canvas::new();
    let path = vector_square(VectorPath::new(WindingRule::NonZero), 0., 20., false);
    let shape = Shape::vector(Rect::new(10., 10., 20., 20.), vec![path]).fill(hex(0xFF0000));
    c.shape(shape.clone());
    let first = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(first.changed);
    let same = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!same.changed);
    assert_eq!(same.uploaded_bytes, 0);
    assert_eq!(same.textures_created, 0);
    let scaled = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 2., &mut NoText)
        .unwrap();
    assert!(scaled.changed);
    // Same bounds, different winding: edge-buffer edits must trigger changed.
    let mut changed = shape.clone();
    changed.vector_paths.as_mut().unwrap()[0].winding_rule = WindingRule::EvenOdd;
    c.clear();
    c.shape(changed);
    assert!(
        renderer
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 2., &mut NoText)
            .unwrap()
            .changed
    );
    c.clear();
    c.shape(shape.stroke(1., hex(0), StrokeAlign::Inside));
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(renderer.problems().is_empty());
}

#[test]
fn vectors_fail_closed_on_excessive_work_and_degenerate_gradients() {
    use figleaf::{VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut p = VectorPath::new(WindingRule::NonZero).move_to([0., 0.]);
    for i in 0..1000 {
        p = p.line_to([if i % 2 == 0 { 0. } else { 1000. }, i as f32]);
    }
    let mut c = Canvas::new();
    c.shape(Shape::vector(Rect::new(0., 0., 1000., 1000.), vec![p]).fill(hex(0)));
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [1000, 1000], 1., &mut NoText)
        .unwrap();
    assert_eq!(
        renderer.problems()[0].reason,
        "vector coverage work limit exceeded"
    );
    c.clear();
    let p = vector_square(VectorPath::new(WindingRule::NonZero), 0., 20., false);
    c.shape(
        Shape::vector(Rect::default(), vec![p])
            .gradient(LinearGradient::horizontal().stop(0., hex(0))),
    );
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(renderer.problems().len(), 1);
}

#[test]
fn self_intersections_and_unfilled_network_branches_keep_their_meaning() {
    use figleaf::{VectorPath, WindingRule};
    let bow = VectorPath::new(WindingRule::EvenOdd)
        .move_to([0., 0.])
        .line_to([40., 40.])
        .line_to([0., 40.])
        .line_to([40., 0.])
        .close();
    let branch = vector_square(VectorPath::new(WindingRule::None), 0., 40., false);
    let mut c = Canvas::new();
    c.shape(Shape::vector(Rect::new(10., 10., 40., 40.), vec![bow, branch]).fill(hex(0xFF0000)));
    let i = draw(&c);
    assert_eq!(i.pixel(30, 15), RED);
    assert_eq!(i.pixel(30, 45), RED);
    assert_eq!(i.pixel(15, 30), CLEAR);
}

#[test]
fn affine_geometry_clips_gradients_and_cache() {
    use figleaf::Transform;
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    // Rotates the local horizontal ramp down the screen, stretching it 2x.
    let m = Transform::new([[0., -1., 40.], [2., 0., 10.]]);
    c.shape(
        Shape::rect(Rect::new(0., 0., 20., 20.))
            .gradient(
                LinearGradient::horizontal()
                    .stop(0., hex(0xff0000))
                    .stop(1., hex(0x0000ff)),
            )
            .transform(m)
            .clip_transformed(Rect::new(0., 0., 10., 20.), 0., m),
    );
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert!(i.pixel(30, 12)[0] > 220);
    assert!(i.pixel(30, 28)[2] > 100);
    assert_eq!(i.pixel(30, 31), CLEAR);
    assert_eq!(i.pixel(41, 12), CLEAR);
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    c.clear();
    c.shape(
        Shape::rect(Rect::new(0., 0., 10., 10.))
            .fill(hex(0xff0000))
            .transform(Transform::new([[-2., 1., 40.], [0., 2., 10.]])),
    );
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(35, 20), RED);
    assert_eq!(i.pixel(20, 20), CLEAR);
    c.shape(
        Shape::rect(Rect::new(0., 0., 10., 10.))
            .fill(hex(0))
            .transform(Transform::new([[0.; 3], [0.; 3]])),
    );
    renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(renderer.problems().len(), 1);
    assert_eq!(renderer.problems()[0].item, 1);
}

#[cfg(feature = "text-coretext")]
#[test]
fn rotation_carries_strokes_shadows_vectors_and_text() {
    use figleaf::{
        Transform, VectorPath, WindingRule,
        text::{FontSource, coretext::CoreText},
    };
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    let mut images = Vec::new();
    for transform in [
        Transform::IDENTITY,
        Transform::new([[0., -1., 100.], [1., 0., 0.]]),
    ] {
        c.clear();
        c.shape(
            Shape::rect(Rect::new(10., 10., 30., 20.))
                .radius(5.)
                .fill(hex(0xff0000))
                .stroke(2., hex(0x0000ff), StrokeAlign::Outside)
                .shadow(Shadow::drop(hexa(0x00000080), [3., 5.], 4.))
                .transform(transform),
        );
        c.shape(
            Shape::vector(
                Rect::new(50., 10., 30., 30.),
                vec![
                    VectorPath::new(WindingRule::NonZero)
                        .move_to([0., 0.])
                        .quadratic_to([30., 0.], [30., 30.])
                        .line_to([0., 30.])
                        .close(),
                ],
            )
            .fill(hex(0x00ff00))
            .transform(transform),
        );
        c.text(
            Text::new("Test", Rect::new(10., 55., 60., 25.))
                .size(14., 20.)
                .color(hex(0x0000ff))
                .drop_shadow(hexa(0x00000080), [2., 3.], 2.)
                .transform(transform)
                .clip_transformed(Rect::new(10., 55., 40., 25.), transform),
        );
        images.push(gpu.render(&mut renderer, &c, [100, 100], 1., [0.; 4], &mut backend));
        assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    }
    let mut total = 0u64;
    for y in 0..100 {
        for x in 0..100 {
            for (a, b) in images[0]
                .pixel(x, y)
                .into_iter()
                .zip(images[1].pixel(99 - y, x))
            {
                total += a.abs_diff(b) as u64;
            }
        }
    }
    assert!(total < 5000, "rotation changed pixels by {total}");
}

#[test]
fn path_caps_joins_alignment_holes_and_cache() {
    use figleaf::{StrokeCap, StrokeJoin, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let line = VectorPath::new(WindingRule::None)
        .move_to([20., 20.])
        .line_to([60., 20.]);
    for cap in [StrokeCap::Butt, StrokeCap::Square, StrokeCap::Round] {
        let mut c = Canvas::new();
        c.shape(
            Shape::vector(Rect::new(0., 0., 100., 60.), vec![line.clone()])
                .stroke(10., hex(0xff0000), StrokeAlign::Center)
                .stroke_cap(cap),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert!(r.problems().is_empty());
        assert_eq!(i.pixel(30, 20), RED);
        assert_eq!(
            i.pixel(16, 20),
            if cap == StrokeCap::Butt { CLEAR } else { RED }
        );
        assert_eq!(
            i.pixel(15, 15),
            if cap == StrokeCap::Square { RED } else { CLEAR }
        );
        let report = r
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0);
    }
    let square = |p: VectorPath, a: f32, b: f32| {
        p.move_to([a, a])
            .line_to([b, a])
            .line_to([b, b])
            .line_to([a, b])
            .close()
    };
    let path = square(
        square(VectorPath::new(WindingRule::EvenOdd), 10., 50.),
        20.,
        40.,
    );
    for align in [
        StrokeAlign::Inside,
        StrokeAlign::Center,
        StrokeAlign::Outside,
    ] {
        let mut c = Canvas::new();
        c.shape(
            Shape::vector(Rect::new(0., 0., 60., 60.), vec![path.clone()]).stroke(
                6.,
                hex(0xff0000),
                align,
            ),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(
            i.pixel(8, 30),
            if align == StrokeAlign::Inside {
                CLEAR
            } else {
                RED
            }
        );
        assert_eq!(
            i.pixel(12, 30),
            if align == StrokeAlign::Outside {
                CLEAR
            } else {
                RED
            }
        );
        assert_eq!(
            i.pixel(22, 30),
            if align == StrokeAlign::Inside {
                CLEAR
            } else {
                RED
            }
        );
        assert_eq!(i.pixel(30, 30), CLEAR);
    }
    let corner = VectorPath::new(WindingRule::None)
        .move_to([20., 40.])
        .line_to([20., 20.])
        .line_to([60., 20.]);
    for join in [StrokeJoin::Miter, StrokeJoin::Bevel, StrokeJoin::Round] {
        let mut c = Canvas::new();
        c.shape(
            Shape::vector(Rect::new(0., 0., 100., 60.), vec![corner.clone()])
                .stroke(12., hex(0xff0000), StrokeAlign::Center)
                .stroke_join(join),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(
            i.pixel(14, 14),
            if join == StrokeJoin::Miter {
                RED
            } else {
                CLEAR
            }
        );
    }
}

#[test]
fn path_strokes_transform_clip_and_reject_invalid_geometry() {
    use figleaf::{StrokeCap, Transform, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let path = VectorPath::new(WindingRule::None)
        .move_to([5., 5.])
        .cubic_to([20., 0.], [30., 30.], [40., 20.]);
    let shape = Shape::vector(Rect::new(0., 0., 50., 40.), vec![path])
        .stroke(8., hexa(0xff000080), StrokeAlign::Center)
        .stroke_cap(StrokeCap::Round)
        .transform(Transform::new([[-1.5, 0.2, 80.], [0., 1., 10.]]))
        .clip(Rect::new(20., 10., 40., 40.));
    let mut c = Canvas::new();
    c.shape(shape.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert!(i.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 100));
    assert!(i.rgba.as_chunks::<4>().0.iter().all(|p| p[3] <= 128));
    assert_eq!(i.pixel(65, 15), CLEAR);
    c.clear();
    c.shape(shape.stroke_miter_limit(0.));
    r.prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(r.problems().len(), 1);
}

#[test]
fn path_miter_limit_degenerate_caps_and_thick_strokes() {
    use figleaf::{StrokeCap, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    for (limit, expected) in [(1., CLEAR), (4., RED)] {
        let mut c = Canvas::new();
        let p = VectorPath::new(WindingRule::None)
            .move_to([20., 40.])
            .line_to([20., 20.])
            .line_to([60., 20.]);
        c.shape(
            Shape::vector(Rect::new(0., 0., 100., 60.), vec![p])
                .stroke(12., hex(0xff0000), StrokeAlign::Center)
                .stroke_miter_limit(limit),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(i.pixel(14, 14), expected);
    }
    for cap in [StrokeCap::Butt, StrokeCap::Round, StrokeCap::Square] {
        let mut c = Canvas::new();
        let p = VectorPath::new(WindingRule::None)
            .move_to([30., 30.])
            .line_to([30., 30.]);
        c.shape(
            Shape::vector(Rect::new(0., 0., 60., 60.), vec![p])
                .stroke(10., hex(0xff0000), StrokeAlign::Center)
                .stroke_cap(cap),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert!(r.problems().is_empty(), "{:?}", r.problems());
        assert_eq!(
            i.pixel(30, 30),
            if cap == StrokeCap::Butt { CLEAR } else { RED }
        );
    }
    let mut c = Canvas::new();
    let p = VectorPath::new(WindingRule::NonZero)
        .move_to([20., 20.])
        .line_to([40., 20.])
        .line_to([40., 40.])
        .line_to([20., 40.])
        .close();
    c.shape(Shape::vector(Rect::new(0., 0., 60., 60.), vec![p]).stroke(
        40.,
        hexa(0xff000080),
        StrokeAlign::Inside,
    ));
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(30, 30), [128, 0, 0, 128]);
    assert_eq!(i.pixel(19, 30), CLEAR);
}

#[test]
fn fitted_dashes_preserve_caps_corners_alpha_and_cache() {
    use figleaf::{StrokeCap, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let base = Shape::vector(
        Rect::new(0., 0., 100., 60.),
        vec![
            VectorPath::new(WindingRule::None)
                .move_to([20., 20.])
                .line_to([80., 20.]),
        ],
    )
    .stroke(4., hexa(0xff000080), StrokeAlign::Center)
    .dash_pattern(vec![12., 8.]);
    let mut c = Canvas::new();
    c.shape(base.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    for x in [20, 25, 34, 45, 74, 79] {
        assert_eq!(i.pixel(x, 20), [128, 0, 0, 128]);
    }
    for x in [19, 26, 33, 46, 73, 80] {
        assert_eq!(i.pixel(x, 20), CLEAR);
    }
    assert!(
        !r.prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap()
            .changed
    );
    c.clear();
    c.shape(base.clone().stroke_cap(StrokeCap::Round));
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(report.changed && report.uploaded_bytes > 0);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(i.pixel(26, 20)[3] > 100);
    assert!(i.rgba.as_chunks::<4>().0.iter().all(|p| p[3] <= 128));
    c.clear();
    c.shape(
        Shape::rect(Rect::new(20., 15., 60., 30.))
            .strokes([4.; 4], hexa(0xff000080), StrokeAlign::Center)
            .dash_pattern(vec![12., 8.]),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(19, 14), [128, 0, 0, 128]);
    assert_eq!(i.pixel(29, 15), CLEAR);
    c.clear();
    c.shape(
        base.dash_pattern(vec![0.00001, 0.00001])
            .fill(hex(0xff0000)),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(r.problems().len(), 1);
    assert!(i.rgba.iter().all(|v| *v == 0));
}

#[test]
fn endpoints_are_independent_and_triangle_shaft_is_trimmed() {
    use figleaf::{StrokeCap, Transform, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let p = VectorPath::new(WindingRule::None)
        .move_to([20., 30.])
        .line_to([80., 30.])
        .start_cap(StrokeCap::CircleFilled)
        .end_cap(StrokeCap::ArrowEquilateral);
    let shape = Shape::vector(Rect::new(0., 0., 100., 60.), vec![p]).stroke(
        4.,
        hex(0xff0000),
        StrokeAlign::Center,
    );
    let mut c = Canvas::new();
    c.shape(shape.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(20, 21), RED);
    assert_eq!(i.pixel(62, 23), RED);
    assert_eq!(i.pixel(79, 31), CLEAR);
    assert_eq!(i.pixel(82, 30), CLEAR);
    c.clear();
    c.shape(shape.transform(Transform::new([[-1., 0., 100.], [0., 1., 0.]])));
    let reflected = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    for y in 0..60 {
        for x in 0..100 {
            assert_eq!(i.pixel(x, y), reflected.pixel(99 - x, y));
        }
    }
    c.clear();
    c.shape(
        Shape::vector(
            Rect::new(0., 0., 20., 20.),
            vec![
                VectorPath::new(WindingRule::None)
                    .move_to([10., 10.])
                    .line_to([10., 10.]),
            ],
        )
        .stroke(2., hex(0), StrokeAlign::Center)
        .stroke_cap(StrokeCap::ArrowLines),
    );
    r.prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(r.problems().len(), 1);
}

#[test]
fn primitive_holes_alignment_errors_and_cache() {
    use figleaf::{ArcData, StrokeCap};
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    let rect = Rect::new(10., 10., 80., 60.);
    c.shape(
        Shape::ellipse_arc(rect, ArcData::new(0., std::f32::consts::TAU, 0.5)).fill(hex(0xff0000)),
    );
    let i = gpu.render(&mut r, &c, [100, 80], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(50, 40), CLEAR);
    assert_eq!(i.pixel(50, 15), RED);
    assert_eq!(i.pixel(5, 40), CLEAR);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 80], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    for (align, red_x, clear_x) in [
        (StrokeAlign::Inside, 88, 92),
        (StrokeAlign::Center, 90, 94),
        (StrokeAlign::Outside, 92, 88),
    ] {
        c.clear();
        c.shape(
            Shape::ellipse_arc(rect, ArcData::new(0., std::f32::consts::PI, 1.))
                .fill(hex(0xff0000))
                .stroke(4., hex(0xff0000), align)
                .stroke_cap(StrokeCap::Round),
        );
        let i = gpu.render(&mut r, &c, [100, 80], 1., [0.; 4], &mut NoText);
        assert!(r.problems().is_empty());
        assert_eq!(i.pixel(red_x, 40), RED);
        assert_eq!(i.pixel(clear_x, 40), CLEAR);
        assert_eq!(i.pixel(50, 50), CLEAR);
    }
    for shape in [
        Shape::polygon(rect, 2),
        Shape::star(rect, 5, 2.),
        Shape::ellipse_arc(rect, ArcData::new(0., 1., 0.)).radius(2.),
        Shape::ellipse_arc(rect, ArcData::new(0., 1., 1.)).stroke(60., hex(0), StrokeAlign::Inside),
    ] {
        c.clear();
        c.shape(shape.fill(hex(0xff0000)));
        let i = gpu.render(&mut r, &c, [100, 80], 1., [0.; 4], &mut NoText);
        assert_eq!(r.problems().len(), 1);
        assert!(i.rgba.iter().all(|v| *v == 0));
    }
}

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

#[test]
fn resolved_stroke_geometry_masks_holes_unions_alpha_and_tracks_changes() {
    use figleaf::{Transform, VectorPath, WindingRule, path::Command};
    let rectangle = |a: f32, b: f32, c: f32, d: f32| {
        VectorPath::new(WindingRule::NonZero)
            .move_to([a, b])
            .line_to([c, b])
            .line_to([c, d])
            .line_to([a, d])
            .close()
    };
    // Resolved boolean domain: square with a square hole.
    let mut domain = rectangle(10., 10., 50., 50.);
    domain.winding_rule = WindingRule::EvenOdd;
    domain
        .commands
        .extend(rectangle(20., 20., 40., 40.).commands);
    // A supplied outline deliberately spans both sides of the hole boundary.
    let outline = rectangle(18., 16., 22., 44.);
    let shape = Shape::vector(Rect::new(0., 0., 60., 60.), vec![domain])
        .stroke(2., [1., 0., 0., 0.5], StrokeAlign::Inside)
        .stroke_geometry(vec![outline.clone(), outline.clone()]);
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.shape(shape.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(19, 30)[3], 128); // Overlapping outlines paint once.
    assert_eq!(i.pixel(21, 30), CLEAR); // Hole excluded by inside alignment.
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    let mut outside = shape.clone();
    outside.stroke.as_mut().unwrap().align = StrokeAlign::Outside;
    c.clear();
    c.shape(outside);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(19, 30), CLEAR);
    assert_eq!(i.pixel(21, 30)[3], 128);
    let mut changed = shape.clone();
    changed.stroke_geometry = Some(vec![rectangle(16., 16., 20., 44.)]);
    c.clear();
    c.shape(changed);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(report.changed && report.uploaded_bytes > 0);
    c.clear();
    c.shape(
        shape
            .clone()
            .transform(Transform::new([[-1., 0., 60.], [0., 1., 0.]])),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(40, 30)[3], 128);
    assert_eq!(i.pixel(38, 30), CLEAR);
    let mut malformed = shape.fill(hex(0xff0000));
    malformed.stroke_geometry.as_mut().unwrap()[0]
        .commands
        .push(Command::LineTo([f32::NAN, 0.]));
    c.clear();
    c.shape(malformed);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(r.problems().len(), 1);
    assert!(i.rgba.iter().all(|v| *v == 0));
}

#[test]
fn clip_stacks_intersect_transforms_and_do_not_double_antialias() {
    use figleaf::{Clip, Transform};
    let a = Clip::rounded_rect(Rect::new(10., 10., 50., 40.), [12., 0., 0., 0.]);
    let b = Clip::rect(Rect::new(0., 0., 35., 50.))
        .transform(Transform::new([[1., 0.4, 20.], [0., 1., 5.]]));
    let shape = Shape::rect(Rect::new(0., 0., 100., 60.))
        .fill(hex(0xff0000))
        .intersect_clip(a.clone())
        .intersect_clip(b.clone());
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.shape(shape.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    for p in [[10, 10], [20, 20], [30, 40], [60, 30]] {
        assert_eq!(i.pixel(p[0], p[1]), CLEAR);
    }
    for p in [[30, 20], [58, 20], [40, 40]] {
        assert_eq!(i.pixel(p[0], p[1]), RED);
    }
    c.clear();
    c.shape(shape.clone().intersect_clip(a.clone()).intersect_clip(b));
    let twice = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.rgba, twice.rgba);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    let mut changed = shape;
    changed.clips[0] = Clip::rounded_rect(Rect::new(10., 10., 50., 40.), [0., 20., 0., 0.]);
    c.clear();
    c.shape(changed);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(report.changed && report.uploaded_bytes > 0);
    // A path and an identical clip share coverage samples at their boundary.
    let p = figleaf::Primitive::Arc(figleaf::ArcData::new(0., std::f32::consts::TAU, 0.))
        .paths(Rect::new(10., 10., 40., 40.), 0.)
        .unwrap();
    c.clear();
    c.shape(Shape::vector(Rect::default(), p.clone()).fill(hex(0xff0000)));
    let unclipped = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    c.clear();
    c.shape(
        Shape::vector(Rect::default(), p.clone())
            .fill(hex(0xff0000))
            .intersect_clip(Clip::paths(p)),
    );
    let clipped = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(unclipped.rgba, clipped.rgba);
}

#[test]
fn arbitrary_clips_preserve_holes_union_regions_and_clip_text_and_shadows() {
    use figleaf::{
        Clip, Error, Transform, VectorPath, WindingRule,
        text::{Backend, Mask, Metrics, Style},
    };
    struct SolidText;
    impl Backend for SolidText {
        fn measure(&mut self, _: &str, _: Style) -> Result<Metrics, Error> {
            Ok(Metrics {
                advance: 100.,
                lines: 1,
                ..Default::default()
            })
        }
        fn raster(
            &mut self,
            text: &str,
            style: Style,
            width: f32,
            scale: f32,
        ) -> Result<Mask, Error> {
            let [width, height] = figleaf::text::validate(text, style, width, scale)?;
            Ok(Mask {
                width,
                height,
                alpha: vec![255; (width * height) as usize],
                metrics: self.measure(text, style)?,
            })
        }
    }
    let rect = |x, y, w, h| {
        VectorPath::new(WindingRule::EvenOdd)
            .move_to([x, y])
            .line_to([x + w, y])
            .line_to([x + w, y + h])
            .line_to([x, y + h])
            .close()
    };
    let mut hole = rect(10., 10., 50., 40.);
    hole.commands.extend(rect(25., 20., 20., 20.).commands);
    let clip = Clip::paths(vec![hole, rect(30., 25., 10., 10.)])
        .transform(Transform::new([[-1., 0., 100.], [0., 1., 0.]]));
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.text(
        Text::new("test", Rect::new(0., 0., 100., 60.))
            .size(20., 60.)
            .color(hex(0xff0000))
            .drop_shadow(hex(0xff0000), [5., 5.], 4.)
            .intersect_clip(clip.clone()),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut SolidText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(45, 30), RED);
    assert_eq!(i.pixel(57, 30), CLEAR);
    assert_eq!(i.pixel(65, 30), RED);
    assert_eq!(i.pixel(95, 30), CLEAR);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut SolidText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    // Empty and malformed clips suppress the whole item, including its shadow.
    for clip in [
        Clip::paths(vec![]),
        Clip::rounded_rect(Rect::new(0., 0., 100., 60.), [f32::NAN; 4]),
    ] {
        c.clear();
        c.text(
            Text::new("test", Rect::new(0., 0., 100., 60.))
                .size(20., 60.)
                .color(hex(0xff0000))
                .intersect_clip(clip.clone()),
        );
        c.shape(
            Shape::rect(Rect::new(0., 0., 100., 60.))
                .fill(hex(0xff0000))
                .shadow(Shadow::drop(hex(0), [2., 2.], 4.))
                .intersect_clip(clip),
        );
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut SolidText);
        assert!(i.rgba.iter().all(|v| *v == 0));
    }
    assert_eq!(r.problems().len(), 2);
}

#[test]
fn vector_shadows_preserve_holes_stroke_silhouettes_and_cache() {
    use figleaf::{Transform, VectorPath, WindingRule};
    let rectangle = |x, y, w, h| {
        VectorPath::new(WindingRule::EvenOdd)
            .move_to([x, y])
            .line_to([x + w, y])
            .line_to([x + w, y + h])
            .line_to([x, y + h])
            .close()
    };
    let mut path = rectangle(10., 10., 30., 30.);
    path.commands.extend(rectangle(20., 20., 10., 10.).commands);
    let base = Shape::vector(Rect::new(0., 0., 50., 50.), vec![path]).fill(hex(0xff0000));
    let mut c = Canvas::new();
    c.shape(
        base.clone()
            .shadow(Shadow::drop(hex(0x0000ff), [40., 0.], 0.).show_behind()),
    );
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(15, 15), RED);
    assert_eq!(i.pixel(55, 15), BLUE);
    assert_eq!(i.pixel(65, 25), CLEAR);
    assert_eq!(i.pixel(25, 25), CLEAR);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    c.clear();
    c.shape(
        base.clone()
            .shadow(Shadow::inner(hex(0x0000ff), [0., 4.], 0.)),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(15, 11), BLUE);
    assert_eq!(i.pixel(15, 16), RED);
    assert_eq!(i.pixel(25, 25), CLEAR);
    let stroke = Shape::vector(
        Rect::new(0., 0., 50., 50.),
        vec![rectangle(10., 10., 30., 30.)],
    )
    .strokes([0., 4., 0., 0.], hex(0xff0000), StrokeAlign::Center)
    .stroke_geometry(vec![rectangle(38., 10., 4., 30.)])
    .shadow(Shadow::drop(hex(0x0000ff), [10., 0.], 0.).show_behind())
    .transform(Transform::new([[-1., 0., 90.], [0., 1., 0.]]));
    c.clear();
    c.shape(stroke);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(50, 25), RED);
    assert_eq!(i.pixel(40, 25), BLUE);
    assert_eq!(i.pixel(65, 25), CLEAR);
    c.clear();
    c.shape(base.shadow(Shadow::drop(hex(0), [0., 0.], f32::NAN)));
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(r.problems().len(), 1);
    assert!(i.rgba.iter().all(|v| *v == 0));
}

#[test]
fn masks_composite_siblings_once_nest_and_cache() {
    use figleaf::{Mask, MaskType};
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut source = Canvas::new();
    source.shape(Shape::rect(Rect::new(0., 0., 80., 50.)).fill([1., 0., 0., 0.5]));
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 40., 40.)).fill(hex(0xff0000)));
    content.shape(Shape::rect(Rect::new(20., 0., 40., 40.)).fill(hex(0x0000ff)));
    let mut c = Canvas::new();
    c.masked(Mask::new(MaskType::Alpha, source.clone()), content.clone());
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert_eq!(i.pixel(10, 20), [128, 0, 0, 128]);
    assert_eq!(i.pixel(30, 20), [0, 0, 128, 128]);
    let r = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(r, figleaf::render::Preparation::default());
    let mut nested = Canvas::new();
    nested.masked(Mask::new(MaskType::Alpha, source.clone()), c);
    let i = gpu.render(&mut renderer, &nested, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(30, 20), [0, 0, 64, 64]);
    let mut vector = Canvas::new();
    vector.masked(Mask::new(MaskType::Vector, source.clone()), content.clone());
    let i = gpu.render(&mut renderer, &vector, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(30, 20), BLUE);
    let mut luminance = Canvas::new();
    luminance.masked(Mask::new(MaskType::Luminance, source).opacity(0.5), content);
    let i = gpu.render(
        &mut renderer,
        &luminance,
        [100, 60],
        1.,
        [0.; 4],
        &mut NoText,
    );
    assert!((i.pixel(30, 20)[3] as i32 - 35).abs() <= 1);
    assert_eq!(i.pixel(90, 20), CLEAR);
    let r = renderer
        .prepare(
            &gpu.device,
            &gpu.queue,
            &luminance,
            [100, 60],
            1.,
            &mut NoText,
        )
        .unwrap();
    assert!(!r.changed && r.uploaded_bytes == 0 && r.textures_created == 0);
    let r = renderer
        .prepare(
            &gpu.device,
            &gpu.queue,
            &luminance,
            [100, 60],
            2.,
            &mut NoText,
        )
        .unwrap();
    assert!(r.changed && r.textures_created == 2);
}

#[test]
fn masks_fail_closed_and_vector_masks_keep_holes_and_strokes() {
    use figleaf::{Mask, MaskType, Transform, VectorPath, WindingRule};
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut source = Canvas::new();
    let path = VectorPath::new(WindingRule::EvenOdd)
        .move_to([0., 0.])
        .line_to([40., 0.])
        .line_to([40., 40.])
        .line_to([0., 40.])
        .close()
        .move_to([10., 10.])
        .line_to([30., 10.])
        .line_to([30., 30.])
        .line_to([10., 30.])
        .close();
    source.shape(
        Shape::vector(Rect::new(0., 0., 40., 40.), vec![path])
            .fill([1., 0., 0., 0.1])
            .transform(Transform::new([[-1., 0., 50.], [0., 1., 10.]])),
    );
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let mut c = Canvas::new();
    c.masked(Mask::new(MaskType::Vector, source.clone()), content.clone());
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(15, 15), RED);
    assert_eq!(i.pixel(30, 30), CLEAR);
    source.clear();
    source.shape(
        Shape::rect(Rect::new(10., 10., 30., 30.))
            .fill([1., 0., 0., 0.])
            .stroke(4., [0., 0., 1., 0.1], StrokeAlign::Inside),
    );
    c.clear();
    c.masked(Mask::new(MaskType::Vector, source.clone()), content.clone());
    let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.pixel(11, 20), RED);
    assert_eq!(i.pixel(20, 20), CLEAR);
    for bad in [
        Mask::new(MaskType::Alpha, source.clone()).opacity(f32::NAN),
        {
            let mut invalid = Canvas::new();
            invalid.shape(Shape::rect(Rect::new(0., 0., f32::NAN, 1.)));
            Mask::new(MaskType::Alpha, invalid)
        },
        {
            let mut text = Canvas::new();
            text.text(Text::new("mask", Rect::new(0., 0., 20., 20.)));
            Mask::new(MaskType::Vector, text)
        },
    ] {
        c.clear();
        c.masked(bad, content.clone());
        let i = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(renderer.problems().len(), 1);
        assert!(i.rgba.iter().all(|v| *v == 0));
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    }
    let mut nested = content;
    for _ in 0..9 {
        let mut c = Canvas::new();
        c.masked(Mask::new(MaskType::Alpha, source.clone()), nested);
        nested = c;
    }
    let r = renderer
        .prepare(&gpu.device, &gpu.queue, &nested, [100, 60], 1., &mut NoText)
        .unwrap();
    assert_eq!(r.textures_created, 0);
    assert_eq!(
        renderer.problems()[0].reason,
        "mask nesting exceeds eight levels"
    );
}

#[cfg(feature = "text-coretext")]
#[test]
fn alpha_text_masks_retain_glyph_coverage_and_clip() {
    use figleaf::{
        Clip, Mask, MaskType,
        text::{FontSource, coretext::CoreText},
    };
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut source = Canvas::new();
    source.text(
        Text::new("HHHH", Rect::new(10., 10., 80., 30.))
            .size(20., 25.)
            .color([1.; 4])
            .intersect_clip(Clip::rect(Rect::new(0., 0., 40., 60.))),
    );
    let expected = gpu.render(&mut r, &source, [100, 60], 1., [0.; 4], &mut backend);
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let mut c = Canvas::new();
    c.masked(Mask::new(MaskType::Alpha, source), content);
    let actual = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert!(r.problems().is_empty());
    assert!(actual.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    for y in 0..60 {
        for x in 0..100 {
            assert_eq!(actual.pixel(x, y)[3], expected.pixel(x, y)[3]);
        }
    }
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
}

#[test]
fn resolved_variable_width_and_brush_outlines_keep_tips_holes_and_cache() {
    use figleaf::{Clip, Transform, VectorPath, WindingRule};
    let rectangle = |x, y, w, h| {
        VectorPath::new(WindingRule::EvenOdd)
            .move_to([x, y])
            .line_to([x + w, y])
            .line_to([x + w, y + h])
            .line_to([x, y + h])
            .close()
    };
    let taper = VectorPath::new(WindingRule::NonZero)
        .move_to([10., 10.])
        .line_to([70., 25.])
        .line_to([10., 40.])
        .close();
    let mut brush = rectangle(15., 15., 25., 20.);
    brush
        .commands
        .extend(rectangle(20., 20., 10., 10.).commands);
    let source = vec![rectangle(0., 0., 512., 512.)];
    let shape = Shape::vector(Rect::new(0., 0., 512., 512.), source.clone())
        .stroke(1., [1., 0., 0., 0.5], StrokeAlign::Center)
        .stroke_geometry(vec![taper]);
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let mut c = Canvas::new();
    c.shape(shape.clone());
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(15, 15)[3], 128);
    assert_eq!(i.pixel(60, 15), CLEAR);
    assert_eq!(i.pixel(65, 25)[3], 128);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    // Nominal width is metadata, not a second widening pass.
    let mut wide = shape;
    wide.stroke.as_mut().unwrap().width = 100.;
    c.clear();
    c.shape(wide);
    let j = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(i.rgba, j.rgba);
    c.clear();
    let brush_shape = Shape::vector(Rect::new(0., 0., 512., 512.), source)
        .stroke(10., [1., 0., 0., 0.5], StrokeAlign::Center)
        .stroke_geometry(vec![brush; 100])
        .transform(Transform::new([[-1., 0., 80.], [0., 1., 0.]]));
    c.shape(brush_shape.clone());
    // The huge unpainted fill domain must not inflate centered outline work.
    let uncut = gpu.render(&mut r, &c, [512, 512], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(uncut.pixel(64, 18)[3], 128);
    c.clear();
    c.shape(brush_shape.intersect_clip(Clip::rect(Rect::new(0., 0., 62., 60.))));
    let i = gpu.render(&mut r, &c, [512, 512], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert_eq!(i.pixel(45, 18)[3], 128);
    assert_eq!(i.pixel(55, 25), CLEAR);
    assert_eq!(i.pixel(64, 18), CLEAR);
    // Invalid outlines never fall back to a constant-width stroke.
    c.clear();
    c.shape(
        Shape::vector(Rect::new(0., 0., 40., 40.), vec![])
            .stroke(4., hex(0), StrokeAlign::Center)
            .stroke_geometry(vec![
                VectorPath::new(WindingRule::None)
                    .move_to([0., 0.])
                    .line_to([30., 30.]),
            ]),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(r.problems().len(), 1);
    assert!(i.rgba.iter().all(|v| *v == 0));
}

#[test]
fn stroke_paints_preserve_order_gradient_coordinates_and_cache() {
    use figleaf::{Paint, Transform, VectorPath, WindingRule};
    let gradient = LinearGradient::horizontal()
        .stop(0., hex(0xff0000))
        .stop(1., hex(0x0000ff));
    let base = Shape::rect(Rect::new(10., 10., 80., 40.)).stroke(10., hex(0), StrokeAlign::Inside);
    let mut c = Canvas::new();
    c.shape(base.clone().stroke_paints(vec![
        Paint::Solid(hex(0xff0000)),
        Paint::Solid([0., 0., 1., 0.5]),
    ]));
    let gpu = Gpu::new();
    let mut r = Renderer::new(&gpu.device, gpu.format);
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    let p = i.pixel(50, 12);
    assert!((127..=128).contains(&p[0]) && (127..=128).contains(&p[2]));
    assert_eq!(p[3], 255);
    assert_eq!(i.pixel(50, 30), CLEAR);
    c.clear();
    c.shape(
        base.clone()
            .stroke_paints(vec![Paint::Linear(gradient.clone())]),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(i.pixel(12, 12)[0] > 240);
    assert!(i.pixel(87, 12)[2] > 240);
    assert_eq!(i.pixel(50, 30), CLEAR);
    let report = r
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
    c.clear();
    c.shape(
        base.stroke_paints(vec![Paint::Linear(gradient.clone().opacity(0.5))])
            .transform(Transform::new([[-1., 0., 100.], [0., 1., 0.]])),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(i.pixel(12, 12)[2] > 120);
    assert_eq!(i.pixel(12, 12)[3], 128);
    c.clear();
    c.shape(
        Shape::vector(
            Rect::new(10., 30., 80., 0.),
            vec![
                VectorPath::new(WindingRule::None)
                    .move_to([0., 0.])
                    .line_to([80., 0.]),
            ],
        )
        .stroke_gradient(10., gradient, StrokeAlign::Center),
    );
    let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(r.problems().is_empty());
    assert!(i.pixel(12, 30)[0] > 240);
    assert!(i.pixel(87, 30)[2] > 240);
    for bad in [
        Shape::rect(Rect::new(0., 0., 20., 20.)).stroke_paints(vec![Paint::Solid(hex(0))]),
        Shape::rect(Rect::new(0., 0., 20., 0.)).stroke_gradient(
            2.,
            LinearGradient::vertical()
                .stop(0., hex(0))
                .stop(1., hex(0xffffff)),
            StrokeAlign::Center,
        ),
        Shape::rect(Rect::new(0., 0., 20., 20.)).stroke_gradient(
            2.,
            LinearGradient::horizontal(),
            StrokeAlign::Inside,
        ),
        Shape::rect(Rect::new(0., 0., 20., 20.))
            .stroke(2., hex(0), StrokeAlign::Inside)
            .stroke_paints(vec![Paint::Solid(hex(0)); 33]),
    ] {
        c.clear();
        c.shape(bad);
        let i = gpu.render(&mut r, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(r.problems().len(), 1);
        assert!(i.rgba.iter().all(|v| *v == 0));
    }
}

#[test]
fn vector_masks_use_stroke_paint_visibility_not_the_legacy_color() {
    use figleaf::{Mask, MaskType, Paint};
    let mut source = Canvas::new();
    source.shape(
        Shape::rect(Rect::new(10., 10., 30., 30.))
            .stroke(5., [0.; 4], StrokeAlign::Inside)
            .stroke_paints(vec![Paint::Linear(
                LinearGradient::horizontal()
                    .stop(0., [1., 0., 0., 0.])
                    .stop(1., [1., 0., 0., 0.5]),
            )]),
    );
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let mut c = Canvas::new();
    c.masked(Mask::new(MaskType::Vector, source), content);
    let i = draw(&c);
    assert_eq!(i.pixel(11, 20), RED);
    assert_eq!(i.pixel(25, 25), CLEAR);
}

#[test]
fn all_gradient_types_and_large_stop_buffers() {
    use figleaf::Gradient;
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let identity = [[1., 0., 0.], [0., 1., 0.]];
    for gradient in [
        Gradient::radial(identity),
        Gradient::angular(identity),
        Gradient::diamond(identity),
        Gradient::horizontal(),
    ] {
        let mut gradient = gradient;
        for i in 0..64 {
            gradient = gradient.stop(
                i as f32 / 63.,
                [i as f32 / 63., 0., 1. - i as f32 / 63., 1.],
            );
        }
        let mut c = Canvas::new();
        c.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).gradient(gradient.clone()));
        let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert!(renderer.problems().is_empty());
        assert_eq!(image.pixel(50, 30)[3], 255);
        assert_ne!(image.pixel(15, 15), image.pixel(75, 40));
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0);
        c.clear();
        c.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).gradient(gradient.stop(1., [1.; 4])));
        let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(renderer.problems().len(), 1);
        assert!(image.rgba.iter().all(|v| *v == 0));
    }
}

#[test]
fn image_placement_transparency_strokes_and_upload_cache() {
    use figleaf::{ImagePaint, ImageScaleMode, Paint, Transform};
    let image = figleaf::Image::new([2, 1], vec![255, 0, 0, 255, 0, 0, 255, 255]);
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    let paint = ImagePaint::new(image.clone()).scale_mode(ImageScaleMode::Fit);
    canvas.shape(Shape::rect(Rect::new(10., 10., 80., 40.)).paint(Paint::Image(paint)));
    let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(actual.pixel(15, 30), RED);
    assert_eq!(actual.pixel(85, 30), BLUE);
    assert_eq!(actual.pixel(5, 30), CLEAR);
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &canvas, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    canvas.clear();
    canvas.shape(
        Shape::rect(Rect::new(10., 10., 80., 40.)).paint(Paint::Image(
            ImagePaint::new(image.clone()).scale_mode(ImageScaleMode::Crop(Transform::new([
                [2., 0., -0.5],
                [0., 2., -0.5],
            ]))),
        )),
    );
    let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(actual.pixel(15, 30), CLEAR);
    assert_eq!(actual.pixel(35, 30), RED);
    canvas.clear();
    canvas.shape(
        Shape::rect(Rect::new(10., 10., 80., 40.))
            .stroke(6., [1.; 4], StrokeAlign::Inside)
            .stroke_paints(vec![Paint::Image(ImagePaint::new(image.clone()))]),
    );
    let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(actual.pixel(12, 30), RED);
    assert_eq!(actual.pixel(87, 30), BLUE);
    assert_eq!(actual.pixel(50, 30), CLEAR);
    for invalid in [
        ImagePaint::new(figleaf::Image::new([2, 2], vec![0; 3])),
        ImagePaint::new(image.clone()).rotation(45.),
        ImagePaint::new(image).scale_mode(ImageScaleMode::Tile(0.)),
    ] {
        canvas.clear();
        canvas.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).paint(Paint::Image(invalid)));
        let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(renderer.problems().len(), 1);
        assert!(actual.rgba.iter().all(|p| *p == 0));
    }
}

#[test]
fn pattern_tiles_stagger_spacing_and_cache_native_source() {
    use figleaf::{Paint, Pattern, TileType};
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut source = Canvas::new();
    source.shape(Shape::rect(Rect::new(0., 0., 8., 8.)).fill(hex(0xff0000)));
    for tile_type in [
        TileType::Rectangular,
        TileType::HorizontalHexagonal,
        TileType::VerticalHexagonal,
    ] {
        let mut canvas = Canvas::new();
        canvas.shape(
            Shape::rect(Rect::new(0., 0., 100., 60.)).paint(Paint::Pattern(
                Pattern::new(source.clone(), [8., 8.])
                    .spacing([1., 1.])
                    .tile_type(tile_type),
            )),
        );
        let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
        assert!(renderer.problems().is_empty());
        assert_eq!(actual.pixel(3, 3), RED);
        assert_eq!(actual.pixel(11, 11), CLEAR);
        match tile_type {
            TileType::Rectangular => assert_eq!(actual.pixel(3, 19), RED),
            TileType::HorizontalHexagonal => {
                assert_eq!(actual.pixel(3, 19), CLEAR);
                assert_eq!(actual.pixel(11, 19), RED);
            }
            TileType::VerticalHexagonal => {
                assert_eq!(actual.pixel(19, 3), CLEAR);
                assert_eq!(actual.pixel(19, 11), RED);
            }
        }
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &canvas, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    }
    let mut nested = source;
    for _ in 0..10 {
        let mut outer = Canvas::new();
        outer.shape(
            Shape::rect(Rect::new(0., 0., 10., 10.))
                .paint(Paint::Pattern(Pattern::new(nested, [10., 10.]))),
        );
        nested = outer;
    }
    let actual = gpu.render(&mut renderer, &nested, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(!renderer.problems().is_empty());
    assert!(actual.rgba.iter().all(|p| *p == 0));
}

#[test]
fn invalid_pattern_sources_cache_the_failure_without_partial_paint() {
    use figleaf::{Paint, Pattern};
    let mut source = Canvas::new();
    source.shape(Shape::rect(Rect::new(0., 0., 8., 8.)).gradient(LinearGradient::horizontal()));
    let mut canvas = Canvas::new();
    canvas.shape(
        Shape::rect(Rect::new(0., 0., 100., 60.))
            .fill(hex(0xff0000))
            .paint(Paint::Pattern(Pattern::new(source, [8., 8.]))),
    );
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let actual = gpu.render(&mut renderer, &canvas, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(renderer.problems().len(), 1);
    assert!(actual.rgba.iter().all(|p| *p == 0));
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &canvas, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
}

#[test]
fn composite_opacity_flattens_overlap_and_caches() {
    use figleaf::Composite;
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(10., 10., 50., 40.)).fill(hex(0xff0000)));
    content.shape(Shape::rect(Rect::new(40., 10., 50., 40.)).fill(hex(0x0000ff)));
    let mut c = Canvas::new();
    c.composite(Composite::new(content).opacity(0.5));
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert_eq!(image.pixel(20, 20), [128, 0, 0, 128]);
    assert_eq!(image.pixel(50, 20), [0, 0, 128, 128]);
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    c.clear();
    c.shape(
        Shape::rect(Rect::new(10., 10., 80., 40.))
            .fill(hex(0xff0000))
            .fill(hex(0x0000ff))
            .opacity(0.5),
    );
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(image.pixel(20, 20), [0, 0, 128, 128]);
}
#[test]
fn isolated_and_pass_through_blends_and_masks_use_the_enclosing_backdrop() {
    use figleaf::{BlendMode, Composite, Mask, MaskType};
    let mut root = Canvas::new();
    root.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    for (x, mode) in [(0., BlendMode::PassThrough), (50., BlendMode::Normal)] {
        let mut child = Canvas::new();
        child.shape(Shape::rect(Rect::new(x, 0., 50., 60.)).fill(hex(0x0000ff)));
        let mut group = Canvas::new();
        group.composite(Composite::new(child).blend_mode(BlendMode::Multiply));
        root.composite(Composite::new(group).blend_mode(mode));
    }
    let mut c = Canvas::new();
    c.composite(Composite::new(root));
    let image = draw(&c);
    assert_eq!(image.pixel(20, 20), [0, 0, 0, 255]);
    assert_eq!(image.pixel(70, 20), BLUE);
    let mut root = Canvas::new();
    root.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let mut mask = Canvas::new();
    mask.shape(Shape::rect(Rect::new(10., 10., 30., 30.)).fill([1.; 4]));
    let mut content = Canvas::new();
    let mut blue = Canvas::new();
    blue.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0x0000ff)));
    content.composite(Composite::new(blue).blend_mode(BlendMode::Multiply));
    root.masked(Mask::new(MaskType::Alpha, mask).opacity(0.5), content);
    let mut c = Canvas::new();
    c.composite(Composite::new(root));
    let image = draw(&c);
    assert_eq!(image.pixel(20, 20), [128, 0, 0, 255]);
    assert_eq!(image.pixel(70, 20), RED);
}
#[test]
fn composite_shadows_preserve_holes_clips_and_failed_input_cache() {
    use figleaf::{BlendMode, Clip, Composite};
    let mut source = Canvas::new();
    for x in [10., 40.] {
        source.shape(Shape::rect(Rect::new(x, 10., 10., 10.)).fill(hex(0xff0000)));
    }
    let mut c = Canvas::new();
    c.composite(
        Composite::new(source.clone())
            .shadow(Shadow::drop(hex(0), [0., 12.], 0.).show_behind())
            .intersect_clip(Clip::rect(Rect::new(0., 0., 45., 60.))),
    );
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert_eq!(image.pixel(15, 25), [0, 0, 0, 255]);
    assert_eq!(image.pixel(30, 25), CLEAR);
    assert_eq!(image.pixel(47, 25), CLEAR);
    for bad in [
        Composite::new(source.clone()).opacity(f32::NAN),
        Composite::new(source.clone()).shadow(Shadow::drop(hex(0), [0.; 2], 1000.)),
        Composite::new(source).blend_mode(BlendMode::Multiply),
    ] {
        c.clear();
        c.composite(bad);
        let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(renderer.problems().len(), 1);
        assert!(image.rgba.iter().all(|v| *v == 0));
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    }
}
#[test]
fn compositing_fixture_blend_swatches_match_the_saved_figma_export() {
    mod generated {
        include!("../examples/generated/compositing.rs");
    }
    let mut c = Canvas::new();
    generated::draw_selection(&mut c);
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let actual = gpu.render(&mut renderer, &c, [560, 410], 1., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    let reference = support::load("docs/figma/compositing.png").unwrap();
    for i in 0..18 {
        let x = 40 + (i % 6) * 92;
        let y = 30 + (i / 6) * 65;
        let a = actual.pixel(x, y);
        let b = reference.pixel(x, y);
        assert!(
            a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
            "blend {i}: {a:?} != {b:?}"
        );
    }
}

#[test]
fn fill_and_stroke_blends_share_the_enclosing_canvas() {
    use figleaf::{BlendMode, Composite};
    let mut root = Canvas::new();
    root.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    root.shape(
        Shape::rect(Rect::new(10., 10., 80., 40.))
            .fill(hex(0x0000ff))
            .fill_blend(BlendMode::Multiply)
            .stroke(6., hex(0x00ff00), StrokeAlign::Outside)
            .stroke_blend_modes(vec![BlendMode::Screen]),
    );
    let mut c = Canvas::new();
    c.composite(Composite::new(root));
    let image = draw(&c);
    assert_eq!(image.pixel(20, 20), [0, 0, 0, 255]);
    assert_eq!(image.pixel(6, 20), [255, 255, 0, 255]);
}

#[test]
fn composite_scope_budget_is_shared_across_root_items() {
    let mut c = Canvas::new();
    for _ in 0..129 {
        c.composite(figleaf::Composite::new(Canvas::new()));
    }
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert_eq!(renderer.problems().len(), 129);
    assert!(image.rgba.iter().all(|v| *v == 0));
}

#[test]
fn layer_blur_filters_premultiplied_content_before_opacity_and_clipping() {
    use figleaf::{Clip, Composite, Transform};
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(20., 20., 20., 20.)).fill(hex(0xff0000)));
    content.shape(Shape::rect(Rect::new(30., 20., 20., 20.)).fill(hex(0xff0000)));
    let mut c = Canvas::new();
    c.composite(
        Composite::new(content)
            .layer_blur(4.)
            .opacity(0.5)
            .effect_transform(Transform::new([[2., 0., 0.], [0., 1., 0.]]))
            .intersect_clip(Clip::rect(Rect::new(0., 0., 45., 60.))),
    );
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert!((126..=128).contains(&image.pixel(35, 30)[3]));
    let fringe = image.pixel(17, 30);
    assert!(fringe[3] > 0 && fringe[3] < 64);
    assert_eq!(fringe[0], fringe[3]);
    assert_eq!(fringe[1], 0);
    assert_eq!(fringe[2], 0);
    assert_eq!(image.pixel(45, 30), CLEAR);
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
}

#[test]
fn background_blur_uses_prior_content_and_preserves_foreground_and_boundary() {
    use figleaf::{Clip, Composite};
    let make = |color| {
        let mut content = Canvas::new();
        content.shape(Shape::rect(Rect::new(0., 0., 50., 60.)).fill(color));
        content.shape(Shape::rect(Rect::new(50., 0., 50., 60.)).fill(hex(0x0000ff)));
        let mut foreground = Canvas::new();
        foreground.shape(Shape::rect(Rect::new(46., 25., 8., 10.)).fill(hex(0x00ff00)));
        content.composite(Composite::new(foreground).background_blur(
            12.,
            Clip::rounded_rect(Rect::new(25., 10., 50., 40.), [10.; 4]),
        ));
        let mut c = Canvas::new();
        c.composite(Composite::new(content));
        c
    };
    let c = make(hex(0xff0000));
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    assert_eq!(image.pixel(49, 30), [0, 255, 0, 255]);
    let mixed = image.pixel(49, 20);
    assert!(mixed[0] > 80 && mixed[2] > 80);
    assert_eq!(image.pixel(49, 5), RED);
    assert_eq!(image.pixel(50, 5), BLUE);
    assert_eq!(image.pixel(25, 10), RED);
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    let changed = gpu.render(
        &mut renderer,
        &make(hex(0xffffff)),
        [100, 60],
        1.,
        [0.; 4],
        &mut NoText,
    );
    assert_ne!(image.pixel(49, 20), changed.pixel(49, 20));
}

#[test]
fn blur_errors_fail_closed_and_remain_cached() {
    use figleaf::{Clip, Composite};
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    for scope in [
        Composite::new(content.clone()).layer_blur(f32::NAN),
        Composite::new(content.clone()).layer_blur(-1.),
        Composite::new(content.clone()).layer_blur(1000.),
        Composite::new(content.clone())
            .background_blur(4., Clip::rect(Rect::new(0., 0., 40., 40.))),
    ] {
        let mut c = Canvas::new();
        c.composite(scope);
        let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut NoText);
        assert_eq!(renderer.problems().len(), 1);
        assert!(image.rgba.iter().all(|v| *v == 0));
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut NoText)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    }
    let mut c = Canvas::new();
    c.composite(Composite::new(content.clone()).layer_blur(0.));
    assert_eq!(draw(&c).rgba, draw(&content).rgba);
}

#[test]
fn blurred_mask_sources_and_background_holes_keep_their_domains() {
    use figleaf::{Clip, Composite, Mask, MaskType, VectorPath, WindingRule};
    let mut mask = Canvas::new();
    mask.shape(Shape::rect(Rect::new(20., 10., 40., 40.)).fill([1.; 4]));
    let mut blurred = Canvas::new();
    blurred.composite(Composite::new(mask).layer_blur(8.));
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 100., 60.)).fill(hex(0xff0000)));
    let mut c = Canvas::new();
    c.masked(Mask::new(MaskType::Alpha, blurred), content);
    let image = draw(&c);
    assert!(image.pixel(18, 30)[3] > 0);
    assert_eq!(image.pixel(90, 30), CLEAR);
    let boundary = VectorPath::new(WindingRule::EvenOdd)
        .move_to([10., 5.])
        .line_to([90., 5.])
        .line_to([90., 55.])
        .line_to([10., 55.])
        .close()
        .move_to([40., 20.])
        .line_to([60., 20.])
        .line_to([60., 40.])
        .line_to([40., 40.])
        .close();
    let mut content = Canvas::new();
    content.shape(Shape::rect(Rect::new(0., 0., 50., 60.)).fill(hex(0xff0000)));
    content.shape(Shape::rect(Rect::new(50., 0., 50., 60.)).fill(hex(0x0000ff)));
    content
        .composite(Composite::new(Canvas::new()).background_blur(12., Clip::paths(vec![boundary])));
    let mut c = Canvas::new();
    c.composite(Composite::new(content));
    let image = draw(&c);
    assert_eq!(image.pixel(49, 30), RED);
    assert_eq!(image.pixel(50, 30), BLUE);
    assert!(image.pixel(49, 10)[2] > 80);
}

#[cfg(feature = "text-coretext")]
#[test]
fn styled_text_shapes_whole_lines_preserves_colors_and_caches() {
    use figleaf::text::{self, Axis, Backend, FontSource, TextRun, coretext::CoreText};
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let regular = [Axis {
        tag: *b"wght",
        value: 400.,
    }];
    let bold = [Axis {
        tag: *b"wght",
        value: 700.,
    }];
    let mut backend = CoreText::new(&[
        Some(FontSource {
            bytes,
            axes: &regular,
        }),
        Some(FontSource { bytes, axes: &bold }),
        None,
    ])
    .unwrap();
    let runs = vec![
        TextRun::new("ab").size(16., 24.).color(hex(0xff0000)),
        TextRun::new("cd")
            .size(16., 24.)
            .font(1)
            .color(hex(0x0000ff)),
    ];
    let measured = backend.measure_runs(&runs).unwrap();
    let raster = backend.raster_runs(&runs, 100., 2.).unwrap();
    assert_eq!(raster.metrics.advance, measured.advance);
    assert_eq!(raster.metrics.baseline, measured.baseline);
    let same = vec![
        TextRun::new("a").size(16., 24.),
        TextRun::new("b").size(16., 24.).color(hex(0xff0000)),
    ];
    let whole = backend.measure("ab", same[0].style).unwrap();
    assert!((whole.advance - backend.measure_runs(&same).unwrap().advance).abs() < 0.001);
    let mut c = Canvas::new();
    c.text(Text::rich(runs, Rect::new(10., 10., 80., 45.)).wrap());
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert!(renderer.problems().is_empty());
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 100 && p[2] == 0)
    );
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 100 && p[0] == 0)
    );
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    let figleaf::paint::Item::Text(original) = &c.items()[0] else {
        unreachable!()
    };
    let mut edited = original.clone();
    edited.runs[0].color = hex(0x00ff00);
    let mut changed = Canvas::new();
    changed.text(edited);
    let updated = gpu.render(
        &mut renderer,
        &changed,
        [100, 60],
        1.,
        [0.; 4],
        &mut backend,
    );
    assert!(
        updated
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[1] > 100 && p[0] == 0 && p[2] == 0)
    );
    assert_ne!(updated.rgba, image.rgba);
    for font in [2, 63] {
        let mut bad = Canvas::new();
        bad.text(Text::rich(
            vec![TextRun::new("valid "), TextRun::new("missing").font(font)],
            Rect::new(0., 0., 100., 40.),
        ));
        let result = gpu.render(&mut renderer, &bad, [100, 60], 1., [0.; 4], &mut backend);
        assert_eq!(renderer.problems().len(), 1);
        assert!(result.rgba.iter().all(|v| *v == 0));
        let report = renderer
            .prepare(&gpu.device, &gpu.queue, &bad, [100, 60], 1., &mut backend)
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    }
    let mut bad = Canvas::new();
    bad.text(Text::rich(
        vec![TextRun::new("no backend")],
        Rect::new(0., 0., 100., 30.),
    ));
    let _ = gpu.render(
        &mut renderer,
        &bad,
        [100, 60],
        1.,
        [0.; 4],
        &mut text::NoText,
    );
    assert_eq!(renderer.problems().len(), 1);
}

#[cfg(feature = "text-coretext")]
#[test]
fn automatic_line_height_uses_supplied_font_metrics_and_caches() {
    use figleaf::text::{self, Backend, FontSource, TextRun, coretext::CoreText};
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut backend = CoreText::new(&[Some(FontSource { bytes, axes: &[] }), None]).unwrap();
    let run = TextRun::new("Auto\n\nheight")
        .size(16., 16.)
        .auto_line_height();
    let height = backend.natural_line_height(run.style).unwrap();
    assert!((height - 21.12).abs() < 0.01);
    let lines = text::layout_runs(&mut backend, std::slice::from_ref(&run), 80., true).unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| (l.height - height).abs() < 0.001));
    let style = text::resolve_style(&mut backend, run.style).unwrap();
    let plain = backend.measure("Auto", run.style).unwrap();
    let resolved = backend.measure("Auto", style).unwrap();
    assert_eq!(plain.baseline, resolved.baseline);
    let mask = backend.raster("Auto", run.style, 80., 2.).unwrap();
    assert_eq!(mask.height, (height * 2.).ceil() as u32);
    let mut c = Canvas::new();
    c.text(
        Text::new("Auto\nheight", Rect::new(5., 0., 90., 60.))
            .size(16., 16.)
            .auto_line_height()
            .wrap(),
    );
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let image = gpu.render(&mut renderer, &c, [100, 60], 1., [0.; 4], &mut backend);
    assert!(renderer.problems().is_empty());
    assert!(image.rgba.iter().any(|v| *v > 0));
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 60], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    let mut bad = Canvas::new();
    bad.text(
        Text::new("Missing", Rect::new(0., 0., 90., 30.))
            .font(1)
            .auto_line_height(),
    );
    let image = gpu.render(&mut renderer, &bad, [100, 60], 1., [0.; 4], &mut backend);
    assert_eq!(renderer.problems().len(), 1);
    assert!(image.rgba.iter().all(|v| *v == 0));
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &bad, [100, 60], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0);
}

#[cfg(feature = "text-coretext")]
#[test]
fn paragraphs_align_with_total_spacing_and_invalidate_cached_layout() {
    use figleaf::{
        paint::Align,
        text::{FontSource, coretext::CoreText},
    };
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &[],
    })])
    .unwrap();
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let make = |spacing, indent| {
        let mut c = Canvas::new();
        c.text(
            Text::new("A\nB", Rect::new(0., 0., 100., 100.))
                .size(16., 24.)
                .paragraph_spacing(spacing)
                .paragraph_indent(indent)
                .align(Align::End, Align::End)
                .wrap(),
        );
        c
    };
    let c = make(8., 20.);
    let image = gpu.render(&mut renderer, &c, [100, 100], 1., [0.; 4], &mut backend);
    assert!(renderer.problems().is_empty());
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &c, [100, 100], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
    let no_indent = gpu.render(
        &mut renderer,
        &make(8., 0.),
        [100, 100],
        1.,
        [0.; 4],
        &mut backend,
    );
    assert_eq!(
        image.rgba, no_indent.rgba,
        "right alignment keeps the same right edge"
    );
    let wider_gap = gpu.render(
        &mut renderer,
        &make(20., 20.),
        [100, 100],
        1.,
        [0.; 4],
        &mut backend,
    );
    assert_ne!(wider_gap.rgba, image.rgba);
    for y in 76..100 {
        for x in 0..100 {
            assert_eq!(
                image.pixel(x, y),
                wider_gap.pixel(x, y),
                "final line has no trailing paragraph gap"
            );
        }
    }
    let bad = make(f32::NAN, 20.);
    let image = gpu.render(&mut renderer, &bad, [100, 100], 1., [0.; 4], &mut backend);
    assert_eq!(renderer.problems().len(), 1);
    assert!(image.rgba.iter().all(|v| *v == 0));
    let report = renderer
        .prepare(&gpu.device, &gpu.queue, &bad, [100, 100], 1., &mut backend)
        .unwrap();
    assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
}

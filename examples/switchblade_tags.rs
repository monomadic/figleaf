//! Switchblade's tag pill: the "Tag pill" component set (node 178:105 in the
//! Switchblade Design System Figma file), all five tones in all four forms,
//! written from the values Figma reports and rendered beside Figma's exports.
//!
//!     cargo run --example switchblade_tags --features wgpu,text-coretext
//!
//! The design is set in SF Pro, which is Apple's and not ours to bundle: the
//! example reads it from `/Library/Fonts/SF-Pro.ttf`, or from the path given
//! as its first argument.
//!
//! Writes `out/switchblade-tags-compare.png` (the whole set at 1x, Figma on
//! the left), two single pills at 2x, and `out/switchblade-tags@2x.png`.
mod support;

use figleaf::{
    Canvas, Color, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa,
    layout::{Align, Node, Tree},
    paint,
    render::Renderer,
    text::{self, Axis, Backend, FontSource, coretext::CoreText},
};

#[derive(Clone, Copy, PartialEq)]
enum Form {
    Plain,
    Count,
    Applied,
    Addable,
}

/// One row of the component set. Alphas are of black: a well's outside stroke
/// and its inner shadow.
struct Tone {
    fill: Color,
    ink: Color,
    /// Icons and the count; the person tone greys them.
    well_ink: Color,
    /// Count disc and remove well: stroke, inner shadow.
    well: [f32; 2],
    /// The count disc's inner shadow in the addable form, where it differs.
    addable_count_shadow: f32,
    /// Add well: stroke, inner shadow, and whether its fill is shown.
    add: [f32; 2],
    add_filled: bool,
    add_ink: Color,
    /// Icons carry a drop shadow: black 25%, Y 1, blur 1.9.
    icon_shadow: bool,
    /// The count carries one (blur 1) in the count form, the addable form.
    count_shadow: [bool; 2],
    /// Right padding in the count and applied forms.
    pad_right: [f32; 2],
}

const COLD: Color = hex(0xDFF4FF);
const WHITE: Color = hex(0xFFFFFF);
const GREY: Color = hex(0x595959);

const TONES: [Tone; 5] = [
    // Heat 1
    Tone {
        fill: [160. / 255., 3. / 255., 1., 0.957],
        ink: COLD,
        well_ink: COLD,
        well: [0.2, 0.35],
        addable_count_shadow: 0.39,
        add: [0.12, 0.25],
        add_filled: false,
        add_ink: COLD,
        icon_shadow: false,
        count_shadow: [false, false],
        pad_right: [5., 7.],
    },
    // Heat 2
    Tone {
        fill: hex(0x00C352),
        ink: COLD,
        well_ink: COLD,
        well: [0.12, 0.25],
        addable_count_shadow: 0.35,
        add: [0.04, 0.32],
        add_filled: false,
        add_ink: WHITE,
        icon_shadow: false,
        count_shadow: [false, false],
        pad_right: [5., 7.],
    },
    // Heat 3
    Tone {
        fill: [0., 40. / 255., 219. / 255., 0.878],
        ink: WHITE,
        well_ink: WHITE,
        well: [0.13, 0.63],
        addable_count_shadow: 0.63,
        add: [0.32, 0.25],
        add_filled: false,
        add_ink: WHITE,
        icon_shadow: false,
        count_shadow: [false, false],
        pad_right: [5., 7.],
    },
    // Heat 4
    Tone {
        fill: hex(0xFF3CCD),
        ink: WHITE,
        well_ink: WHITE,
        well: [0.2, 0.25],
        addable_count_shadow: 0.25,
        add: [0.14, 0.25],
        add_filled: false,
        add_ink: WHITE,
        icon_shadow: true,
        count_shadow: [false, true],
        pad_right: [5., 6.],
    },
    // Person
    Tone {
        fill: WHITE,
        ink: hex(0x000000),
        well_ink: GREY,
        well: [0.2, 0.25],
        addable_count_shadow: 0.25,
        add: [0.2, 0.25],
        add_filled: true,
        add_ink: GREY,
        icon_shadow: true,
        count_shadow: [true, true],
        pad_right: [6., 6.],
    },
];

fn black(alpha: f32) -> Color {
    [0., 0., 0., alpha]
}

/// A 16-tall well: fill 141414 at 19%, a 1 wide outside stroke, radius 8 and
/// an inner shadow (Y 1, blur 2.9).
fn well(rect: Rect, filled: bool, [stroke, shadow]: [f32; 2]) -> Shape {
    let shape = Shape::rect(rect)
        .radius(8.)
        .stroke(1., black(stroke), StrokeAlign::Outside)
        .shadow(Shadow::inner(black(shadow), [0., 1.], 2.9));
    if filled {
        shape.fill(hexa(0x14141430))
    } else {
        shape
    }
}

/// An icon's strokes: 1.5 wide with round ends, inside the icon's box.
fn icon(c: &mut Canvas, at: Rect, lines: &[[f32; 4]], ink: Color, shadow: bool) {
    for [x0, y0, x1, y1] in lines {
        let mut line = Shape::line([at.x + x0, at.y + y0], [at.x + x1, at.y + y1], 1.5).fill(ink);
        if shadow {
            line = line.shadow(Shadow::drop(black(0.25), [0., 1.], 1.9));
        }
        c.shape(line);
    }
}

const ADD: [[f32; 4]; 2] = [[4.5, 0., 4.5, 9.], [0., 4.5, 9., 4.5]];
const CLOSE: [[f32; 4]; 2] = [[0., 0., 7., 7.], [7., 0., 0., 7.]];

/// Draws one pill with its top left at `at` and returns its size.
fn pill(
    c: &mut Canvas,
    backend: &mut dyn Backend,
    at: [f32; 2],
    tone: &Tone,
    form: Form,
    label: &str,
    count: &str,
) -> [f32; 2] {
    // Label: SF Pro Medium 12, a 14 tall line. Count: SF Pro Semibold 11, 13 tall.
    let label_style = text::Style {
        font: 0,
        size: 12.,
        line_height: 14.,
        letter_spacing: 0.,
    };
    let count_style = text::Style {
        font: 1,
        size: 11.,
        line_height: 13.,
        ..label_style
    };
    // Figma hugs text to whole units.
    let mut wide = |s: &str, style| backend.measure(s, style).unwrap().advance.ceil() as f32;
    let (label_w, count_w) = (wide(label, label_style), wide(count, count_style));

    // The pill: a row 26 tall, gap 7, children centred across.
    let (left, right) = match form {
        Form::Plain => (10., 10.),
        Form::Count => (10., tone.pad_right[0]),
        Form::Applied => (10., tone.pad_right[1]),
        Form::Addable => (6., 6.),
    };
    let mut t = Tree::new(
        Node::row()
            .size([0., 26.])
            .width(figleaf::layout::Sizing::Hug)
            .paddings([left, 0., right, 0.])
            .gap(7.)
            .align(Align::Min, Align::Center),
    );
    let root = t.root();
    let add = (form == Form::Addable).then(|| t.add(root, Node::fixed([16., 16.])));
    let label_box = t.add(root, Node::fixed([label_w, 14.]));
    // Count disc: hugs the count with 5 either side, at least 18.67 wide.
    let disc = matches!(form, Form::Count | Form::Addable).then(|| {
        let disc = t.add(
            root,
            Node::row()
                .size([0., 16.])
                .width(figleaf::layout::Sizing::Hug)
                .min([18.67, 0.])
                .padding_xy(5., 0.)
                .align(Align::Center, Align::Center),
        );
        (disc, t.add(disc, Node::fixed([count_w, 13.])))
    });
    let remove = (form == Form::Applied).then(|| t.add(root, Node::fixed([17., 16.])));
    let solved = t.solve_natural().unwrap();
    let place = |id| solved[id].translate(at);

    c.shape(Shape::rect(place(root)).radius(13.).fill(tone.fill));
    if let Some(add) = add {
        let r = place(add);
        c.shape(well(r, tone.add_filled, tone.add));
        let glyph = Rect::new(r.x + 3.5, r.y + 3.5, 9., 9.);
        icon(c, glyph, &ADD, tone.add_ink, tone.icon_shadow);
    }
    c.text(
        Text::new(label, place(label_box))
            .size(12., 14.)
            .color(tone.ink),
    );
    if let Some((disc, number)) = disc {
        let addable = form == Form::Addable;
        let shadow = if addable {
            tone.addable_count_shadow
        } else {
            tone.well[1]
        };
        c.shape(well(place(disc), true, [tone.well[0], shadow]));
        let mut text = Text::new(count, place(number))
            .font(1)
            .size(11., 13.)
            .color(tone.well_ink)
            .align(paint::Align::Center, paint::Align::Start);
        if tone.count_shadow[usize::from(addable)] {
            text = text.drop_shadow(black(0.25), [0., 1.], 1.);
        }
        c.text(text);
    }
    if let Some(remove) = remove {
        let r = place(remove);
        c.shape(well(r, true, tone.well));
        let glyph = Rect::new(r.x + 5., r.y + 4.5, 7., 7.);
        icon(c, glyph, &CLOSE, tone.well_ink, tone.icon_shadow);
    }
    solved.size()
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Library/Fonts/SF-Pro.ttf".into());
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("no font at {path}: pass the path to SF Pro as the first argument");
        return;
    };
    // Figma's Medium is weight 510 and its Semibold 590.
    let weight = |value| {
        [Axis {
            tag: *b"wght",
            value,
        }]
    };
    let (medium, semibold) = (weight(510.), weight(590.));
    let font = |axes| {
        Some(FontSource {
            bytes: &bytes,
            axes,
        })
    };
    let mut backend = CoreText::new(&[font(&medium), font(&semibold)]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    std::fs::create_dir_all("out").unwrap();
    let report = |renderer: &Renderer| {
        for p in renderer.problems() {
            eprintln!("item {} left out: {}", p.item, p.reason);
        }
    };
    let forms = [Form::Plain, Form::Count, Form::Applied, Form::Addable];

    // The component set: 476x290, fill 080A0D, radius 14; a row per tone at
    // y 32 + 50n, the forms at x 32, 114, 244 and 346.
    let mut c = Canvas::new();
    c.shape(
        Shape::rect(Rect::new(0., 0., 476., 290.))
            .radius(14.)
            .fill(hex(0x080A0D)),
    );
    for (row, tone) in TONES.iter().enumerate() {
        for (form, x) in forms.into_iter().zip([32., 114., 244., 346.]) {
            let at = [x, 32. + 50. * row as f32];
            let size = pill(&mut c, &mut backend, at, tone, form, "night", "18");
            if row == 0 {
                println!("pill {size:?}");
            }
        }
    }
    let clear = [0.; 4];
    let ours = gpu.render(&mut renderer, &c, [476, 290], 1., clear, &mut backend);
    report(&renderer);
    match support::load("docs/figma/switchblade-tags-figma@1x.png") {
        Some(figma) => {
            // Both are transparent outside the set's rounded corners.
            let flat = support::Image {
                rgba: ours
                    .rgba
                    .chunks(4)
                    .flat_map(|p| [p[0], p[1], p[2], 255])
                    .collect(),
                ..ours
            };
            support::compare(&figma, &flat, [0; 3], 2, "out/switchblade-tags-compare.png")
        }
        None => println!("no Figma export to compare against"),
    }
    let big = gpu.render(&mut renderer, &c, [952, 580], 2., clear, &mut backend);
    support::save("out/switchblade-tags@2x.png", &big);

    // Two pills alone at 2x, over the set's background colour.
    let background = [8u8, 10, 13];
    let clear = [8. / 255., 10. / 255., 13. / 255., 1.];
    for (name, tone, form) in [
        ("person-addable", &TONES[4], Form::Addable),
        ("heat1-applied", &TONES[0], Form::Applied),
    ] {
        let mut c = Canvas::new();
        let [w, h] = pill(&mut c, &mut backend, [0.; 2], tone, form, "night", "18");
        let size = [(w * 2.) as u32, (h * 2.) as u32];
        let ours = gpu.render(&mut renderer, &c, size, 2., clear, &mut backend);
        report(&renderer);
        let out = format!("out/switchblade-tag-{name}-compare.png");
        match support::load(&format!("docs/figma/switchblade-tag-{name}-figma@2x.png")) {
            Some(figma) => support::compare(&figma, &ours, background, 4, &out),
            None => println!("no Figma export to compare against"),
        }
    }
}

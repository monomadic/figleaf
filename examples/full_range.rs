//! A real control: "Abner / Processing / Full Range" (node 214:399 in the
//! Abner Timeline Edit Figma file), written from the values Figma reports and
//! rendered beside Figma's own export of it.
//!
//!     cargo run --example full_range --features wgpu,text-coretext
//!
//! Writes `out/full-range.png` (figleaf alone) and, when the Figma export is
//! at `docs/figma/full-range-figma.png`, `out/full-range-compare.png`: Figma
//! on the left, figleaf on the right, both enlarged 3x.
mod support;

use figleaf::{
    Canvas, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa,
    layout::{Align, Node, Tree},
    render::Renderer,
    text::{Axis, Backend, FontSource, coretext::CoreText},
};

const MEDIUM: usize = 0;
const BOLD: usize = 1;
/// Figma's export starts at the node's corner and runs 14 past its right and
/// bottom edges, where the glow spills out.
const MARGIN: f32 = 0.;
const SIZE: [u32; 2] = [224, 125];

fn main() {
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let weight = |value| {
        [Axis {
            tag: *b"wght",
            value,
        }]
    };
    let (medium, bold) = (weight(500.), weight(700.));
    let source = |axes| Some(FontSource { bytes, axes });
    let mut backend = CoreText::new(&[source(&medium), source(&bold)]).unwrap();

    // Text layers: JetBrains Mono 10, line heights 29 and 28, tracked.
    let title = Text::new("FULL RANGE", Rect::default())
        .font(MEDIUM)
        .size(10., 29.)
        .letter_spacing(1.4)
        .color(hex(0x9CA9B5));
    let segment = |string: &str, color| {
        Text::new(string, Rect::default())
            .font(BOLD)
            .size(10., 28.)
            .letter_spacing(0.7)
            .color(hex(color))
    };
    let off = segment("16-235", 0x7E8B99);
    let on = segment("0-255", 0x070B0D);
    let mut hug = |t: &Text| {
        let advance = backend.measure(&t.string, t.style()).unwrap().advance as f32;
        Node::fixed([advance, t.line_height])
    };

    // Auto layout, as in Figma's panel: a column with gap 8 holding the title
    // and a 210x74 row with padding 6; each 99x62 segment centres its label.
    let mut t = Tree::new(Node::column().gap(8.));
    let title_box = t.add(t.root(), hug(&title));
    let track = Node::row()
        .size([210., 74.])
        .padding(6.)
        .align(Align::Center, Align::Center);
    let track = t.add(t.root(), track);
    let cell = || {
        Node::row()
            .size([99., 62.])
            .align(Align::Center, Align::Center)
    };
    let off_cell = t.add(track, cell());
    let off_box = t.add(off_cell, hug(&off));
    let on_cell = t.add(track, cell());
    let on_box = t.add(on_cell, hug(&on));
    let solved = t.solve_natural().unwrap();
    let at = |id| solved[id].translate([MARGIN; 2]);
    let place = |mut text: Text, id| {
        text.rect = at(id);
        text
    };

    let mut c = Canvas::new();
    // The track's stroke lies under its children: the glow tints it.
    c.shape(
        Shape::rect(at(track))
            .radius(17.)
            .fill(hex(0x050608))
            .stroke(4., hex(0x606B79), StrokeAlign::Inside),
    );
    c.shape(
        Shape::rect(at(on_cell))
            .radius(11.)
            .fill(hex(0x00AEEE))
            .shadow(Shadow::drop(hexa(0x00AEEE47), [0., 0.], 20.).show_behind()),
    );
    c.text(place(title, title_box));
    c.text(place(off, off_box));
    c.text(place(on, on_box));

    let figma = support::load("docs/figma/full-range-figma.png");
    // The export is opaque: its corner is Figma's canvas colour.
    let canvas = figma.as_ref().map_or([30, 30, 30, 255], |f| f.pixel(0, 0));
    let background = canvas.map(|v| f64::from(v) / 255.);

    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let ours = gpu.render(&mut renderer, &c, SIZE, 1., background, &mut backend);
    for p in renderer.problems() {
        eprintln!("item {} left out: {}", p.item, p.reason);
    }
    std::fs::create_dir_all("out").unwrap();
    support::save("out/full-range.png", &ours);
    println!("layout size {:?}", solved.size());
    match figma {
        Some(figma) => {
            let background = [canvas[0], canvas[1], canvas[2]];
            support::compare(&figma, &ours, background, 3, "out/full-range-compare.png");
        }
        None => println!("no Figma export to compare against"),
    }
}

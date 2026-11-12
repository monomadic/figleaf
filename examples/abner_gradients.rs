//! Real gradients: four small pieces of Abner's timeline (nodes 14:147,
//! 14:148, 14:124 and 14:128 in the Abner design system Figma file), written
//! from the values Figma reports and rendered beside Figma's own 2x exports.
//!
//!     cargo run --example abner_gradients --features wgpu,text-coretext
//!
//! Writes `out/abner-{trim-area,trim-grip,marker-4,marker-5}-compare.png`:
//! Figma on the left, figleaf on the right, both enlarged 6x.
mod support;

use figleaf::{
    Canvas, LinearGradient, Rect, Shape, Text, hex, hexa, paint,
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};

/// "Start trim area": 8x56, white at 9% fading to 777777 at 0%, left to right.
fn trim_area(c: &mut Canvas) {
    c.shape(
        Shape::rect(Rect::new(0., 0., 8., 56.)).gradient(
            LinearGradient::transform([1., 0., 0.])
                .stop(0., hexa(0xFFFFFF17))
                .stop(1., hexa(0x77777700)),
        ),
    );
}

/// "Start trim grip": 7x38, two hard-edged white bars made from eight stops,
/// at 50% in a paint at 56%. Figma also lists a solid white fill at 0%.
fn trim_grip(c: &mut Canvas) {
    let (clear, bar) = (hexa(0x00000000), hexa(0xFFFFFF80));
    let mut g = LinearGradient::transform([1., 0., 0.]).opacity(0.56);
    for (offset, color) in [
        (0.2222, clear),
        (0.2222, bar),
        (0.3333, bar),
        (0.3333, clear),
        (0.5556, clear),
        (0.5556, bar),
        (0.6667, bar),
        (0.6667, clear),
    ] {
        g = g.stop(offset, color);
    }
    c.shape(
        Shape::rect(Rect::new(0., 0., 7., 38.))
            .fill(hexa(0xFFFFFF00))
            .gradient(g),
    );
}

/// The numbered tab both markers share: 20x18, corners 3, 9, 9, 3.
fn number(c: &mut Canvas, fill: u32, digit: &str, font: usize, size: f32, ink: u32) {
    let tab = Rect::new(0., 0., 20., 18.);
    c.shape(Shape::rect(tab).radii([3., 9., 9., 3.]).fill(hex(fill)));
    c.text(
        Text::new(digit, tab)
            .font(font)
            .size(size, 18.)
            .color(hex(ink))
            .align(paint::Align::Center, paint::Align::Start),
    );
}

/// "Marker / 4": a 3x12 flag with a three-stop gradient, top to bottom,
/// under the tab.
fn marker_4(c: &mut Canvas) {
    c.shape(
        Shape::rect(Rect::new(0., 15., 3., 12.)).gradient(
            LinearGradient::transform([0., 1., 0.])
                .stop(0., hex(0x393A3A))
                .stop(0.5096, hex(0x3F3F3F))
                .stop(1., hex(0x333333)),
        ),
    );
    number(c, 0x393A3A, "4", 0, 10.5, 0xEDEDEC);
}

/// "Marker / 5": the flag is over the tab and has two fills, a solid and a
/// gradient at 66% that darkens it from 45% of the way down.
fn marker_5(c: &mut Canvas) {
    number(c, 0xE9AC00, "5", 0, 12., 0x1A1408);
    c.shape(
        Shape::rect(Rect::new(0., 15., 3., 12.))
            .fill(hex(0xE9AC00))
            .gradient(
                LinearGradient::transform([0., 1., 0.])
                    .opacity(0.66)
                    .stop(0.4519, hexa(0x66666636))
                    .stop(1., hexa(0x0000007D)),
            ),
    );
}

fn main() {
    // Figma calls the style Bold but sets the weight axis to 600.
    let weight = [Axis {
        tag: *b"wght",
        value: 600.,
    }];
    let source = FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &weight,
    };
    let mut backend = CoreText::new(&[Some(source)]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    std::fs::create_dir_all("out").unwrap();

    type Piece = (&'static str, fn(&mut Canvas), [u32; 2]);
    let pieces: [Piece; 4] = [
        ("trim-area", trim_area, [16, 112]),
        ("trim-grip", trim_grip, [14, 76]),
        ("marker-4", marker_4, [40, 54]),
        ("marker-5", marker_5, [40, 54]),
    ];
    for (name, draw, size) in pieces {
        let mut c = Canvas::new();
        draw(&mut c);
        // The exports are transparent; both sides go over the same dark blue.
        let background = [24u8, 30, 40];
        let clear = [24. / 255., 30. / 255., 40. / 255., 1.];
        let ours = gpu.render(&mut renderer, &c, size, 2., clear, &mut backend);
        for p in renderer.problems() {
            eprintln!("item {} left out: {}", p.item, p.reason);
        }
        let out = format!("out/abner-{name}-compare.png");
        match support::load(&format!("docs/figma/abner-{name}-figma@2x.png")) {
            Some(figma) => support::compare(&figma, &ours, background, 6, &out),
            None => println!("no Figma export to compare against"),
        }
    }
}

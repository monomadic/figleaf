//! A real switch with a label: "Abner / Process / Switch" (node 103:2651 in
//! the Abner design system Figma file), both states, written from the values
//! Figma reports and rendered beside Figma's own 2x exports.
//!
//!     cargo run --example abner_onoff --features wgpu,text-coretext
//!
//! Writes `out/abner-onoff-{on,off}-compare.png`: Figma on the left, figleaf
//! on the right, both enlarged 4x.
mod support;

use figleaf::{
    Canvas, Rect, Shape, StrokeAlign, Text, hex, hexa, paint,
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};

fn main() {
    let bold = [Axis {
        tag: *b"wght",
        value: 700.,
    }];
    let source = FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &bold,
    };
    let mut backend = CoreText::new(&[Some(source)]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    std::fs::create_dir_all("out").unwrap();

    for (name, on) in [("on", true), ("off", false)] {
        let mut c = Canvas::new();
        // Frame: 61x27, fill 0A0A0A, stroke A4A7AD 1.5 inside, radius 8.
        c.shape(
            Shape::rect(Rect::new(0., 0., 61., 27.))
                .radius(8.)
                .fill(hex(0x0A0A0A))
                .stroke(1.5, hex(0xA4A7AD), StrokeAlign::Inside),
        );
        // Thumb: 28x21, radius 5. On: 0074E1 at x 30. Off: 535353 at x 3
        // with a 0.5 inside stroke of white at 20%.
        let thumb = Rect::new(if on { 30. } else { 3. }, 3., 28., 21.);
        let shape = Shape::rect(thumb).radius(5.);
        c.shape(if on {
            shape.fill(hex(0x0074E1))
        } else {
            shape
                .fill(hex(0x535353))
                .stroke(0.5, hexa(0xFFFFFF33), StrokeAlign::Inside)
        });
        // Label: JetBrains Mono Bold 12, line height 12.5, centred in the thumb.
        c.text(
            Text::new(if on { "ON" } else { "OFF" }, thumb)
                .size(12., 12.5)
                .color(hex(0xEDEDED))
                // Inner shadow: 000000 at 41%, X 0, Y 1, blur 2.8.
                .inner_shadow(hexa(0x00000069), [0., 1.], 2.8)
                .align(paint::Align::Center, paint::Align::Center),
        );

        let background = [3u8; 3];
        let clear = [3. / 255., 3. / 255., 3. / 255., 1.];
        let ours = gpu.render(&mut renderer, &c, [122, 54], 2., clear, &mut backend);
        for p in renderer.problems() {
            eprintln!("item {} left out: {}", p.item, p.reason);
        }
        let out = format!("out/abner-onoff-{name}-compare.png");
        match support::load(&format!("docs/figma/abner-onoff-{name}-figma@2x.png")) {
            Some(figma) => support::compare(&figma, &ours, background, 4, &out),
            None => println!("no Figma export to compare against"),
        }
    }
}

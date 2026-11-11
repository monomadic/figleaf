//! A real switch: the orange pill in "Abner / Process / Processor card ·
//! toggle" (node 143:5040 in the Abner design system Figma file), written
//! from the values Figma reports and rendered beside Figma's own 2x export.
//!
//!     cargo run --example abner_switch --features wgpu
//!
//! Writes `out/abner-switch.png` and `out/abner-switch-compare.png`: Figma on
//! the left, figleaf on the right, both enlarged 6x.
mod support;

use figleaf::{
    Canvas, Shadow, Shape, StrokeAlign, hex, hexa,
    layout::{Align, Node, Tree},
    render::Renderer,
    text::NoText,
};

fn main() {
    // Frame: 32x16, horizontal auto layout, padding 2, clip content.
    // The thumb is absolutely positioned at 18, 2.
    let mut t = Tree::new(
        Node::row()
            .size([32., 16.])
            .padding(2.)
            .align(Align::Max, Align::Center),
    );
    let thumb = t.add(t.root(), Node::fixed([12., 12.]).absolute([18., 2.]));
    let solved = t.solve_natural().unwrap();
    // Figma's export starts 1 outside the frame, where the stroke is.
    let at = |id| solved[id].translate([1., 1.]);
    let frame = at(t.root());

    let mut c = Canvas::new();
    // Fill FF9D00, stroke FFFFFF 1 outside, corner radius 17.
    c.shape(Shape::rect(frame).radius(17.).fill(hex(0xFF9D00)).stroke(
        1.,
        hex(0xFFFFFF),
        StrokeAlign::Outside,
    ));
    // Thumb: white, radius 6, drop shadow X0 Y4 blur 4 at 000000 25%,
    // clipped by the frame.
    c.shape(
        Shape::rect(at(thumb))
            .radius(6.)
            .fill(hex(0xFFFFFF))
            .shadow(Shadow::drop(hexa(0x00000040), [0., 4.], 4.))
            .clip_rounded(frame, 17.),
    );

    // The card behind the switch is 030303.
    let background = [3u8; 3];
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let clear = [3. / 255., 3. / 255., 3. / 255., 1.];
    let ours = gpu.render(&mut renderer, &c, [68, 36], 2., clear, &mut NoText);
    for p in renderer.problems() {
        eprintln!("item {} left out: {}", p.item, p.reason);
    }
    std::fs::create_dir_all("out").unwrap();
    support::save("out/abner-switch.png", &ours);
    match support::load("docs/figma/abner-switch-figma@2x.png") {
        Some(figma) => {
            support::compare(&figma, &ours, background, 6, "out/abner-switch-compare.png")
        }
        None => println!("no Figma export to compare against"),
    }
}

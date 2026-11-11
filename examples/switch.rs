//! A settings card with switches and buttons, laid out by the auto layout
//! solver and drawn offscreen at 2x into `out/switch.png`.
//!
//!     cargo run --example switch --features wgpu,text-coretext
//!
//! Without `text-coretext` the shapes still draw and the labels are listed as
//! problems. The design is made up for this example; it is not a Figma export.
mod support;

use figleaf::{
    Canvas, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa,
    layout::{Align, Node, Sizing, Tree},
    paint,
    render::Renderer,
};

fn switch(canvas: &mut Canvas, r: Rect, on: bool) {
    let track = Shape::rect(r).radius(r.h / 2.);
    canvas.shape(if on {
        track.fill(hex(0x34C759))
    } else {
        track
            .fill(hex(0xE9E9EB))
            .shadow(Shadow::inner(hexa(0x00000026), [0., 1.], 2.))
    });
    let d = r.h - 4.;
    let x = if on { r.x + r.w - d - 2. } else { r.x + 2. };
    canvas.shape(
        Shape::ellipse(Rect::new(x, r.y + 2., d, d))
            .fill(hex(0xFFFFFF))
            .stroke(0.5, hexa(0x0000000A), StrokeAlign::Outside)
            .shadow(Shadow::drop(hexa(0x00000033), [0., 2.], 4.)),
    );
}

fn label(canvas: &mut Canvas, string: &str, r: Rect, color: u32, align: paint::Align) {
    canvas.text(
        Text::new(string, r)
            .size(13., 16.)
            .color(hex(color))
            .align(align, paint::Align::Center),
    );
}

fn main() {
    // The card's auto layout, as it would read in Figma's panel.
    let mut t = Tree::new(Node::column().size([320., 200.]).padding(16.).gap(12.));
    let root = t.root();
    let title = t.add(root, Node::fixed([0., 20.]).width(Sizing::Fill));
    let mut rows = Vec::new();
    for (name, on) in [("Sync to grid", true), ("Quantize", false)] {
        let row = Node::row()
            .width(Sizing::Fill)
            .align(Align::SpaceBetween, Align::Center);
        let row = t.add(root, row);
        let text = t.add(row, Node::fixed([160., 16.]));
        let control = t.add(row, Node::fixed([44., 24.]));
        rows.push((name, on, text, control));
    }
    t.add(root, Node::fill());
    let footer = Node::row()
        .width(Sizing::Fill)
        .gap(8.)
        .align(Align::Max, Align::Center);
    let footer = t.add(root, footer);
    let status = t.add(footer, Node::fixed([8., 8.]));
    let status_text = t.add(footer, Node::fixed([0., 16.]).width(Sizing::Fill));
    let cancel = t.add(footer, Node::fixed([72., 28.]));
    let apply = t.add(footer, Node::fixed([72., 28.]));

    let card = Rect::new(20., 20., 320., 200.);
    let solved = t.solve([card.w, card.h]).unwrap();
    let at = |id| solved[id].translate([card.x, card.y]);

    let mut c = Canvas::new();
    c.shape(
        Shape::rect(card)
            .radius(12.)
            .fill(hex(0xFFFFFF))
            .stroke(1., hexa(0x00000014), StrokeAlign::Inside)
            .shadow(Shadow::drop(hexa(0x0000001F), [0., 8.], 24.))
            .shadow(Shadow::drop(hexa(0x00000014), [0., 1.], 2.)),
    );
    c.text(
        Text::new("Playback", at(title))
            .size(15., 20.)
            .color(hex(0x1C1C1E)),
    );
    for (name, on, text, control) in rows {
        label(&mut c, name, at(text), 0x3A3A3C, paint::Align::Start);
        switch(&mut c, at(control), on);
    }
    c.shape(Shape::ellipse(at(status)).fill(hex(0x34C759)));
    label(
        &mut c,
        "Saved",
        at(status_text),
        0x8E8E93,
        paint::Align::Start,
    );
    c.shape(
        Shape::rect(at(cancel))
            .radius(6.)
            .fill(hex(0xFFFFFF))
            .stroke(1., hex(0xD1D1D6), StrokeAlign::Inside),
    );
    label(&mut c, "Cancel", at(cancel), 0x1C1C1E, paint::Align::Center);
    c.shape(
        Shape::rect(at(apply))
            .radius(6.)
            .fill(hex(0x0A84FF))
            .shadow(Shadow::drop(hexa(0x0A84FF55), [0., 2.], 6.))
            .shadow(Shadow::inner(hexa(0xFFFFFF59), [0., 1.], 0.)),
    );
    label(&mut c, "Apply", at(apply), 0xFFFFFF, paint::Align::Center);

    #[cfg(feature = "text-coretext")]
    let mut text = {
        use figleaf::text::{Axis, FontSource, coretext::CoreText};
        let source = FontSource {
            bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
            axes: &[Axis {
                tag: *b"wght",
                value: 500.,
            }],
        };
        CoreText::new(&[Some(source)]).unwrap()
    };
    #[cfg(not(feature = "text-coretext"))]
    let mut text = figleaf::text::NoText;

    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let background = [242. / 255., 242. / 255., 247. / 255., 1.];
    let image = gpu.render(&mut renderer, &c, [720, 480], 2., background, &mut text);
    for p in renderer.problems() {
        eprintln!("item {} left out: {}", p.item, p.reason);
    }

    std::fs::create_dir_all("out").unwrap();
    let file = std::io::BufWriter::new(std::fs::File::create("out/switch.png").unwrap());
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&image.rgba).unwrap();
    println!("wrote out/switch.png");
}

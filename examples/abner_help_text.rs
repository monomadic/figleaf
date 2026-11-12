//! Real paragraphs: two help texts from the Abner Timeline Edit Figma file
//! (frames 88:183 and 88:275), rendered beside Figma's own 2x exports. The
//! first paragraph wraps to fit its 304-wide box; the second has a line
//! break in it.
//!
//!     cargo run --example abner_help_text --features wgpu,text-coretext
//!
//! Writes `out/abner-help-{card,lines}-compare.png`: Figma on the left,
//! figleaf on the right, both enlarged 2x.
mod support;

use figleaf::{
    Canvas, Rect, Shape, Text, hex,
    render::Renderer,
    text::{self, Axis, FontSource, coretext::CoreText},
};

/// "One file. More possibilities.": a 328x81 card, radius 8, fill 0D1B2B,
/// with a heading and a paragraph in JetBrains Mono 11 / 17.
fn card(c: &mut Canvas) -> Text {
    c.shape(
        Shape::rect(Rect::new(0., 0., 328., 81.))
            .radius(8.)
            .fill(hex(0x0D1B2B)),
    );
    c.text(
        Text::new(
            "One file. More possibilities.",
            Rect::new(12., 12., 304., 17.),
        )
        .size(11., 17.)
        .color(hex(0x61B5EE)),
    );
    Text::new(
        "Duplicate this source to try different edits while keeping the original at hand.",
        Rect::new(12., 35., 304., 34.),
    )
    .size(11., 17.)
    .color(hex(0xB3BDC9))
    .wrap()
}

/// "Selection help": two texts in JetBrains Mono 10 / 15 with no frame fill.
fn lines(c: &mut Canvas) -> Text {
    c.text(
        Text::new(
            "Click a card to view. Check to edit.",
            Rect::new(0., 0., 296., 15.),
        )
        .size(10., 15.)
        .color(hex(0xB3BDC9)),
    );
    Text::new(
        "Linked copies take no extra disk space\nuntil an output is created.",
        Rect::new(0., 21., 296., 30.),
    )
    .size(10., 15.)
    .color(hex(0x8D9AAB))
    .wrap()
}

fn main() {
    let regular = [Axis {
        tag: *b"wght",
        value: 400.,
    }];
    let source = FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &regular,
    };
    let mut backend = CoreText::new(&[Some(source)]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    std::fs::create_dir_all("out").unwrap();

    type Piece = (&'static str, fn(&mut Canvas) -> Text, [u32; 2]);
    let pieces: [Piece; 2] = [("card", card, [656, 162]), ("lines", lines, [592, 102])];
    for (name, draw, size) in pieces {
        let mut c = Canvas::new();
        let paragraph = draw(&mut c);
        // What a host would do to learn the paragraph's height.
        let broken = text::wrap(
            &mut backend,
            &paragraph.string,
            paragraph.style(),
            paragraph.rect.w,
        )
        .unwrap();
        for line in &broken {
            println!("{:>7.2}  {}", line.advance, line.text);
        }
        c.text(paragraph);

        // The exports are transparent outside the card; both sides go over
        // the same dark blue.
        let background = [20u8, 24, 30];
        let clear = [20. / 255., 24. / 255., 30. / 255., 1.];
        let ours = gpu.render(&mut renderer, &c, size, 2., clear, &mut backend);
        for problem in renderer.problems() {
            eprintln!("item {} left out: {}", problem.item, problem.reason);
        }
        let out = format!("out/abner-help-{name}-compare.png");
        match support::load(&format!("docs/figma/abner-help-{name}-figma@2x.png")) {
            Some(figma) => support::compare(&figma, &ours, background, 2, &out),
            None => println!("no Figma export to compare against"),
        }
    }
}

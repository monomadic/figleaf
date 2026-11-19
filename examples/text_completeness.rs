//! Generated Unicode, decorations, text blends and strokes beside Figma.
mod support;
mod generated {
    include!("generated/text_completeness.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{FontSource, coretext::CoreText},
};
fn main() {
    let mut text = CoreText::new(&[
        Some(FontSource {
            bytes: include_bytes!("../fonts/NotoSansMath.ttf"),
            axes: &[],
        }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/NotoMusic.ttf"),
            axes: &[],
        }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/NotoEmoji.ttf"),
            axes: &[figleaf::text::Axis {
                tag: *b"wght",
                value: 400.,
            }],
        }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
            axes: &[figleaf::text::Axis {
                tag: *b"wght",
                value: 400.,
            }],
        }),
    ])
    .unwrap()
    .with_fallback_fonts(&[3])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0, 1, 2, 3]);
    // Independent opaque cells can be rendered separately without changing
    // their backdrop, keeping the existing intermediate-pixel budget bounded.
    let figleaf::paint::Item::Composite(root) = &canvas.items()[0] else {
        panic!("expected isolated selection")
    };
    let mut actual = support::Image {
        width: 2360,
        height: 2340,
        rgba: vec![0; 2360 * 2340 * 4],
    };
    for row in 0..6 {
        for col in 0..3 {
            let mut content = Canvas::new();
            let figleaf::paint::Item::Shape(background) = &root.content.items()[0] else {
                panic!("expected background")
            };
            content.shape(background.clone());
            let figleaf::paint::Item::Text(label) = &root.content.items()[1 + row * 3 + col] else {
                panic!("expected text")
            };
            let mut label = label.clone();
            label.transform.0[0][2] -= col as f32 * 390.;
            label.transform.0[1][2] -= row as f32 * 190.;
            content.text(label);
            let mut tile = Canvas::new();
            tile.composite(figleaf::Composite::new(content));
            let image = gpu.render(
                &mut renderer,
                &tile,
                [
                    if col == 2 { 800 } else { 780 },
                    if row == 5 { 440 } else { 380 },
                ],
                2.,
                [1.; 4],
                &mut text,
            );
            assert!(
                renderer.problems().is_empty(),
                "cell {row},{col}: {:?}",
                renderer.problems()
            );
            for y in 0..image.height as usize {
                let start = ((row * 380 + y) * 2360 + col * 780) * 4;
                let width = image.width as usize * 4;
                actual.rgba[start..start + width]
                    .copy_from_slice(&image.rgba[y * width..(y + 1) * width]);
            }
        }
    }
    std::fs::create_dir_all("out").unwrap();
    support::save("out/text-completeness.png", &actual);
    let reference = support::load("docs/figma/text-completeness.png").unwrap();
    support::compare(
        &reference,
        &actual,
        [255; 3],
        1,
        "out/text-completeness-compare.png",
    );
}

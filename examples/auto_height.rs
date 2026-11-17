//! Plugin-generated mixed styles, explicit fonts and wrapping beside Figma.
mod support;
mod generated {
    include!("generated/auto_height.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};
fn main() {
    let regular = [Axis {
        tag: *b"wght",
        value: 400.,
    }];
    let bold = [Axis {
        tag: *b"wght",
        value: 700.,
    }];
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut text = CoreText::new(&[
        Some(FontSource {
            bytes,
            axes: &regular,
        }),
        Some(FontSource { bytes, axes: &bold }),
    ])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0, 1]);
    let actual = gpu.render(&mut renderer, &canvas, [920, 660], 2., [1.; 4], &mut text);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/auto-height.png", &actual);
    let reference = support::load("docs/figma/auto-height.png").unwrap();
    support::compare(
        &reference,
        &actual,
        [255; 3],
        1,
        "out/auto-height-compare.png",
    );
}

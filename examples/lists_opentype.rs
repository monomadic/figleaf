//! Generated lists and small caps beside Figma.
mod support;
mod generated {
    include!("generated/lists_opentype.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{FontSource, coretext::CoreText},
};
fn main() {
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut text = CoreText::new(&[
        Some(FontSource { bytes, axes: &[] }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/SourceSerif4.ttf"),
            axes: &[],
        }),
    ])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0, 1]);
    let actual = gpu.render(&mut renderer, &canvas, [1840, 2040], 2., [1.; 4], &mut text);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/lists-opentype.png", &actual);
    let reference = support::load("docs/figma/lists-opentype.png").unwrap();
    support::compare(
        &reference,
        &actual,
        [255; 3],
        1,
        "out/lists-opentype-compare.png",
    );
}

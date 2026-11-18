//! Generated script shaping and explicit fonts beside Figma.
mod support;
mod generated {
    include!("generated/scripts.rs");
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
            bytes: include_bytes!("../fonts/NotoSansDevanagari.ttf"),
            axes: &[],
        }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/NotoSansArabic.ttf"),
            axes: &[],
        }),
    ])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0, 1, 2]);
    let actual = gpu.render(&mut renderer, &canvas, [1280, 1080], 2., [1.; 4], &mut text);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/scripts.png", &actual);
    let reference = support::load("docs/figma/scripts.png").unwrap();
    support::compare(&reference, &actual, [255; 3], 1, "out/scripts-compare.png");
}

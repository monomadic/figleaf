//! Generated text fills and strokes beside Figma.
mod support;
mod generated {
    include!("generated/text_paints.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{FontSource, coretext::CoreText},
};
fn main() {
    let bytes = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    let mut text = CoreText::new(&[Some(FontSource { bytes, axes: &[] })]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0]);
    let actual = gpu.render(&mut renderer, &canvas, [2000, 1060], 2., [1.; 4], &mut text);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/text-paints.png", &actual);
    let reference = support::load("docs/figma/text-paints.png").unwrap();
    support::compare(
        &reference,
        &actual,
        [255; 3],
        1,
        "out/text-paints-compare.png",
    );
}

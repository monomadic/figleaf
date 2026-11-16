//! Generated Rust from the temporary Figma complex_strokes fixture.
mod support;
#[allow(clippy::approx_constant)]
mod generated {
    include!("generated/complex_strokes.rs");
}
use figleaf::{Canvas, render::Renderer, text::NoText};
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas);
    let image = gpu.render(&mut renderer, &canvas, [600, 430], 1., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/complex_strokes.png", &image);
    let reference = support::load("docs/figma/complex-strokes@1x.png").unwrap();
    support::compare(
        &reference,
        &image,
        [255; 3],
        1,
        "out/complex_strokes-compare.png",
    );
}

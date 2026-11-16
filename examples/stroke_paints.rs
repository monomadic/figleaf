//! Generated Rust from the temporary Figma stroke_paints fixture.
mod support;
#[allow(clippy::approx_constant)]
mod generated {
    include!("generated/stroke_paints.rs");
}
use figleaf::{Canvas, render::Renderer, text::NoText};
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas);
    let image = gpu.render(
        &mut renderer,
        &canvas,
        [1000, 770],
        2.,
        [1.; 4],
        &mut NoText,
    );
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/stroke_paints.png", &image);
    let reference = support::load("docs/figma/stroke-paints@2x.png").unwrap();
    support::compare(
        &reference,
        &image,
        [255; 3],
        1,
        "out/stroke_paints-compare.png",
    );
}

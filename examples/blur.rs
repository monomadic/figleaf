//! Generated Rust from the temporary Figma blur fixture.
mod support;
#[allow(clippy::approx_constant)]
mod generated {
    include!("generated/blur.rs");
}
use figleaf::{Canvas, render::Renderer, text::NoText};
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas);
    let image = gpu.render(&mut renderer, &canvas, [480, 300], 1., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/blur.png", &image);
    let reference = support::load("docs/figma/blur.png").unwrap();
    support::compare(&reference, &image, [255; 3], 1, "out/blur-compare.png");
}

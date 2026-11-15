//! Generated Rust from the temporary Figma dash/endpoint fixture.
mod support;
mod generated {
    include!("generated/dashed_endpoints.rs");
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
        [1020, 880],
        2.,
        [1.; 4],
        &mut NoText,
    );
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/dashed-endpoints.png", &image);
    let reference = support::load("docs/figma/dashed-endpoints@2x.png").unwrap();
    support::compare(
        &reference,
        &image,
        [255; 3],
        1,
        "out/dashed-endpoints-compare.png",
    );
}

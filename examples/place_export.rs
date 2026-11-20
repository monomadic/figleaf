//! Places one exported piece twice, at different sizes and offsets, in one
//! canvas: the Figma plugin's output always draws at the origin, and
//! `Canvas::append_at` moves it.
//!
//!     cargo run --example place_export --features wgpu
//!
//! Writes `out/place-export.png`.
mod support;
mod generated {
    include!("generated/responsive_layout.rs");
}
use figleaf::{Canvas, Rect, Shape, hex, render::Renderer, text::NoText};

fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    std::fs::create_dir_all("out").unwrap();

    let mut canvas = Canvas::new();
    // The host's own background, then the exported piece twice.
    canvas.shape(Shape::rect(Rect::new(0., 0., 820., 400.)).fill(hex(0xE9E9EC)));
    for ([w, h], at) in [([280., 300.], [20., 20.]), ([480., 360.], [320., 20.])] {
        let mut piece = Canvas::new();
        generated::draw_selection(&mut piece, Some([w, h])).unwrap();
        canvas.append_at(piece, at);
    }
    let image = gpu.render(
        &mut renderer,
        &canvas,
        [1640, 800],
        2.,
        [1.; 4],
        &mut NoText,
    );
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    support::save("out/place-export.png", &image);
    println!("wrote out/place-export.png");
}

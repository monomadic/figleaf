//! Synthetic individual-border cases. Not a Figma comparison.
mod support;
use figleaf::{Canvas, Rect, Shape, StrokeAlign, hex, render::Renderer, text::NoText};
fn main() {
    let mut canvas = Canvas::new();
    let widths = [
        [0., 0., 4., 0.],
        [2., 6., 10., 4.],
        [0., 6., 0., 6.],
        [20., 4., 6., 0.],
    ];
    for (row, align) in [
        StrokeAlign::Inside,
        StrokeAlign::Center,
        StrokeAlign::Outside,
    ]
    .into_iter()
    .enumerate()
    {
        for (col, w) in widths.iter().enumerate() {
            canvas.shape(
                Shape::rect(Rect::new(
                    24. + col as f32 * 104.,
                    24. + row as f32 * 84.,
                    72.,
                    52.,
                ))
                .radius(16.)
                .fill(hex(0xE7EDF5))
                .strokes(*w, hex(0x1769E0), align),
            );
        }
    }
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let image = gpu.render(&mut renderer, &canvas, [880, 552], 2., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/frame-borders.png", &image);
}

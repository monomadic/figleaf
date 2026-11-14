//! Person icon from Figma's Switchblade Design System, node 336:1230.
//! The source has a variable-bound solid fill. This fixture uses the explicit
//! resolved color, without changing the design. The plugin still rejects bindings.
//! Also renders synthetic even-odd/nonzero holes and overlapping regions.
mod support;
mod generated {
    include!("generated/vector_person.rs");
}
use figleaf::{
    Canvas, Rect, Shape, VectorPath, WindingRule, hex, hexa, render::Renderer, text::NoText,
};
fn square(p: VectorPath, a: f32, b: f32) -> VectorPath {
    p.move_to([a, a])
        .line_to([b, a])
        .line_to([b, b])
        .line_to([a, b])
        .close()
}
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas);
    let image = gpu.render(&mut renderer, &canvas, [60, 60], 4., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/vector-person.png", &image);
    let reference = support::load("docs/figma/vector-person@4x.png").expect("Figma reference");
    support::compare(
        &reference,
        &image,
        [255; 3],
        6,
        "out/vector-person-compare.png",
    );
    canvas.clear();
    for (i, rule) in [WindingRule::EvenOdd, WindingRule::NonZero]
        .into_iter()
        .enumerate()
    {
        canvas.shape(
            Shape::vector(
                Rect::new(12. + i as f32 * 80., 12., 60., 60.),
                vec![square(square(VectorPath::new(rule), 0., 60.), 15., 45.)],
            )
            .fill(hex(0x1769E0)),
        );
    }
    canvas.shape(
        Shape::vector(
            Rect::new(172., 12., 80., 60.),
            vec![
                square(VectorPath::new(WindingRule::NonZero), 0., 40.),
                square(VectorPath::new(WindingRule::NonZero), 20., 60.),
            ],
        )
        .fill(hexa(0x1769E080)),
    );
    let image = gpu.render(&mut renderer, &canvas, [504, 168], 2., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    support::save("out/vector-rules.png", &image);
}

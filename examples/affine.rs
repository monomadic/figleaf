//! Rotation, reflection and skew beside an exported temporary Figma fixture.
mod support;
use figleaf::{Canvas, Rect, Shape, StrokeAlign, Transform, render::Renderer, text::NoText};
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    canvas.shape(
        Shape::rect(Rect::new(0., 0., 45., 40.))
            .radius(8.)
            .fill([0.09, 0.41, 0.88, 1.])
            .stroke(2., [0.05, 0.15, 0.3, 1.], StrokeAlign::Inside)
            .transform(Transform::new([
                [0.86601907, -0.500011, 35.],
                [0.500011, 0.86601907, 15.],
            ])),
    );
    canvas.shape(
        Shape::ellipse(Rect::new(0., 0., 45., 40.))
            .fill([0.09, 0.41, 0.88, 1.])
            .stroke(2., [0.05, 0.15, 0.3, 1.], StrokeAlign::Inside)
            .transform(Transform::new([
                [-1., 0.3713907, 150.],
                [0., 0.92847675, 20.],
            ])),
    );
    canvas.shape(
        Shape::rect(Rect::new(0., 0., 45., 40.))
            .radius(8.)
            .fill([0.09, 0.41, 0.88, 1.])
            .stroke(2., [0.05, 0.15, 0.3, 1.], StrokeAlign::Inside)
            .transform(Transform::new([
                [1., 0.4472136, 165.],
                [0., 0.8944272, 50.],
            ])),
    );
    let image = gpu.render(&mut renderer, &canvas, [480, 240], 2., [1.; 4], &mut NoText);
    assert!(renderer.problems().is_empty());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/affine.png", &image);
    let reference = support::load("docs/figma/affine@2x.png").unwrap();
    support::compare(&reference, &image, [255; 3], 2, "out/affine-compare.png");
}

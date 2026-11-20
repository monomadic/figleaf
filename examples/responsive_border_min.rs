//! One generated layout at three host-supplied sizes, beside Figma.
mod support;
mod generated {
    include!("generated/responsive_border_min.rs");
}
use figleaf::{Canvas, render::Renderer, text::NoText};
fn main() {
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    std::fs::create_dir_all("out").unwrap();
    for [width, height] in [[160, 80], [70, 50], [2, 3]] {
        let mut canvas = Canvas::new();
        generated::draw_selection(&mut canvas, Some([width as f32, height as f32])).unwrap();
        let root = generated::selection_layout(Some([width as f32, height as f32])).unwrap()[0];
        let actual_width = root.w as u32;
        let actual_height = root.h as u32;
        let actual = gpu.render(
            &mut renderer,
            &canvas,
            [actual_width * 2, actual_height * 2],
            2.,
            [1.; 4],
            &mut NoText,
        );
        assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
        let report = renderer
            .prepare(
                &gpu.device,
                &gpu.queue,
                &canvas,
                [actual_width * 2, actual_height * 2],
                2.,
                &mut NoText,
            )
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
        support::save(&format!("out/responsive-border-min-{width}.png"), &actual);
        let reference =
            support::load(&format!("docs/figma/responsive-border-min-{width}.png")).unwrap();
        support::compare(
            &reference,
            &actual,
            [255; 3],
            1,
            &format!("out/responsive-border-min-{width}-compare.png"),
        );
    }
}

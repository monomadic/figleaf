//! One responsive text export at two widths, beside live Figma captures.
mod support;
mod generated {
    include!("generated/responsive_baseline.rs");
}
mod reference {
    include!("generated/responsive_baseline_expected.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};
fn main() {
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &[Axis {
            tag: *b"wght",
            value: 400.,
        }],
    })])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    std::fs::create_dir_all("out").unwrap();
    for &(width, expected_height, expected) in reference::CASES {
        let size = [Some(width as f32), None];
        let boxes = generated::selection_layout(size, [0], &mut backend).unwrap();
        for (actual, expected) in boxes.iter().zip(expected) {
            assert_eq!([actual.x, actual.y, actual.w, actual.h], *expected);
        }
        let mut canvas = Canvas::new();
        generated::draw_selection(&mut canvas, size, [0], &mut backend).unwrap();
        let actual = gpu.render(
            &mut renderer,
            &canvas,
            [width * 2, expected_height * 2],
            2.,
            [1.; 4],
            &mut backend,
        );
        assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
        let report = renderer
            .prepare(
                &gpu.device,
                &gpu.queue,
                &canvas,
                [width * 2, expected_height * 2],
                2.,
                &mut backend,
            )
            .unwrap();
        assert!(!report.changed && report.uploaded_bytes == 0 && report.textures_created == 0);
        support::save(&format!("out/responsive-baseline-{width}.png"), &actual);
        let reference =
            support::load(&format!("docs/figma/responsive-baseline-{width}.png")).unwrap();
        support::compare(
            &reference,
            &actual,
            [255; 3],
            1,
            &format!("out/responsive-baseline-{width}-compare.png"),
        );
    }
}

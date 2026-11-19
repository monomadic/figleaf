//! Generated Arabic/Hebrew lists, decorations and tracking beside Figma.
mod support;
mod generated {
    include!("generated/rtl_text.rs");
}
use figleaf::{
    Canvas,
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};
fn main() {
    let bytes = include_bytes!("../fonts/NotoSansArabic.ttf");
    let mut text = CoreText::new(&[
        Some(FontSource {
            bytes,
            axes: &[
                Axis {
                    tag: *b"wght",
                    value: 400.,
                },
                Axis {
                    tag: *b"wdth",
                    value: 100.,
                },
            ],
        }),
        Some(FontSource {
            bytes: include_bytes!("../fonts/NotoSansHebrew.ttf"),
            axes: &[
                Axis {
                    tag: *b"wght",
                    value: 400.,
                },
                Axis {
                    tag: *b"wdth",
                    value: 100.,
                },
            ],
        }),
    ])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0, 1]);
    let actual = gpu.render(&mut renderer, &canvas, [2360, 2340], 2., [1.; 4], &mut text);
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
    std::fs::create_dir_all("out").unwrap();
    support::save("out/rtl-text.png", &actual);
    let reference = support::load("docs/figma/rtl-text.png").unwrap();
    support::compare(&reference, &actual, [255; 3], 1, "out/rtl-text-compare.png");
}

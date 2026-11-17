//! Plugin-generated ancestor clips beside Figma, also exercising path clips.
mod support;
mod generated {
    include!("generated/clipping.rs");
}
use figleaf::{
    ArcData, Canvas, Clip, ClipGeometry, Primitive,
    paint::Item,
    render::Renderer,
    text::{FontSource, coretext::CoreText},
};
fn main() {
    let source = FontSource {
        bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
        axes: &[],
    };
    let mut text = CoreText::new(&[Some(source)]).unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, gpu.format);
    let mut canvas = Canvas::new();
    generated::draw_selection(&mut canvas, [0]);
    // The circle frame's clipping boundary is equivalent to this arbitrary
    // path. Exercise it for both its shape and text without using a mask runtime.
    let mut paths = Canvas::new();
    for item in canvas.items() {
        let mut item = item.clone();
        let clips = match &mut item {
            Item::Shape(s) => &mut s.clips,
            Item::Text(t) => &mut t.clips,
            Item::Masked(_) | Item::Composite(_) => unreachable!("fixture has no masks"),
        };
        for clip in clips {
            if let ClipGeometry::RoundedRect { rect, radii } = &clip.geometry
                && rect.w == 80.
                && rect.h == 80.
                && *radii == [40.; 4]
            {
                *clip = Clip::paths(
                    Primitive::Arc(ArcData::new(0., std::f32::consts::TAU, 0.))
                        .paths(*rect, 0.)
                        .unwrap(),
                )
                .transform(clip.transform);
            }
        }
        match item {
            Item::Shape(s) => paths.shape(s),
            Item::Text(t) => paths.text(t),
            Item::Masked(m) => paths.masked(m.mask, m.content),
            Item::Composite(c) => paths.composite(c),
        };
    }
    std::fs::create_dir_all("out").unwrap();
    let reference = support::load("docs/figma/clipping@2x.png").unwrap();
    for (name, c) in [("clipping", canvas), ("clipping-path", paths)] {
        let image = gpu.render(&mut renderer, &c, [920, 300], 2., [1.; 4], &mut text);
        assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
        support::save(&format!("out/{name}.png"), &image);
        support::compare(
            &reference,
            &image,
            [255; 3],
            1,
            &format!("out/{name}-compare.png"),
        );
    }
}

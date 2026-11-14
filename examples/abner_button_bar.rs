//! Abner's button bar: the "Button Bar" component (node 463:6046 in the
//! "Abner — Signal / 01 · Design System" Figma file), written from the values
//! Figma reports and rendered beside Figma's 2x export. Its labels are
//! placeholders.
//!
//!     cargo run --example abner_button_bar --features wgpu,text-coretext
//!
//! The design is set in SF Pro, which is Apple's and not ours to bundle: the
//! example reads it from `/Library/Fonts/SF-Pro.ttf`, or from the path given
//! as its first argument.
//!
//! Writes `out/abner-button-bar-compare.png`: Figma on the left, figleaf on
//! the right.
mod support;

use figleaf::{
    Canvas, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa,
    layout::{Node, Sizing, Tree},
    render::Renderer,
    text::{Axis, FontSource, coretext::CoreText},
};

/// Draws the bar with its top left at the origin, `width` wide. The first
/// button is the pressed one.
fn button_bar(c: &mut Canvas, width: f32, labels: &[&str], pressed: usize) {
    // Bar: 32 tall, padding 14 / 4, gap 3, a 1 wide stroke on the bottom
    // edge that takes room in the layout.
    let mut t = Tree::new(
        Node::row()
            .size([width, 32.])
            .paddings([14., 4., 14., 5.])
            .gap(3.),
    );
    let root = t.root();
    // Buttons: 116 wide, filling the bar's height; the label hugs.
    let buttons: Vec<_> = labels
        .iter()
        .map(|_| t.add(root, Node::fixed([116., 0.]).height(Sizing::Fill)))
        .collect();
    let solved = t.solve([width, 32.]).unwrap();
    let bar = solved[root];

    // Fill 6B6B6A at 53%, top corners 4, inner shadow black 29% Y 2 blur 2.9.
    c.shape(
        Shape::rect(bar)
            .radii([4., 4., 0., 0.])
            .fill(hexa(0x6B6B6A87))
            .strokes([0., 0., 1., 0.], hex(0x585454), StrokeAlign::Inside)
            .shadow(Shadow::inner(hexa(0x0000004A), [0., 2.], 2.9)),
    );

    for (n, (label, id)) in labels.iter().zip(buttons).enumerate() {
        let r = solved[id];
        let down = n == pressed;
        let shape = Shape::rect(r).radius(6.);
        c.shape(if down {
            // Pressed: 3F3D3B at 72%, inner shadow black 22% Y 2 blur 2.2.
            shape
                .fill(hexa(0x3F3D3BB8))
                .shadow(Shadow::inner(hexa(0x00000038), [0., 2.], 2.2))
        } else {
            // Rest: C1BEBA at 33%, a 1 wide inside stroke of 565353 at 52%.
            shape
                .fill(hexa(0xC1BEBA54))
                .stroke(1., hexa(0x56535385), StrokeAlign::Inside)
        });
        // Label: SF Pro Medium 12 / 16, 10 in from the button's edge (and
        // from its stroke), centred across.
        let inset = if down { 10. } else { 11. };
        c.text(
            Text::new(*label, Rect::new(r.x + inset, r.y, r.w - 2. * inset, r.h))
                .size(12., 16.)
                .color(if down {
                    hex(0xF4F3ED)
                } else {
                    hexa(0x211D1AAD)
                })
                .align(figleaf::paint::Align::Start, figleaf::paint::Align::Center),
        );
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Library/Fonts/SF-Pro.ttf".into());
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("no font at {path}: pass the path to SF Pro as the first argument");
        return;
    };
    // Figma's Medium is weight 510.
    let medium = [Axis {
        tag: *b"wght",
        value: 510.,
    }];
    let mut backend = CoreText::new(&[Some(FontSource {
        bytes: &bytes,
        axes: &medium,
    })])
    .unwrap();
    let gpu = support::Gpu::new();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    std::fs::create_dir_all("out").unwrap();

    let mut c = Canvas::new();
    button_bar(&mut c, 690., &["16:9", "1080p", "Reset", "Save ProRes"], 0);
    // The bar is translucent; both are laid over the same dark ground.
    let background = [28u8, 27, 26];
    let clear = [28. / 255., 27. / 255., 26. / 255., 1.];
    let ours = gpu.render(&mut renderer, &c, [1380, 64], 2., clear, &mut backend);
    for p in renderer.problems() {
        eprintln!("item {} left out: {}", p.item, p.reason);
    }
    match support::load("docs/figma/abner-button-bar-figma@2x.png") {
        Some(figma) => support::compare(
            &figma,
            &ours,
            background,
            1,
            "out/abner-button-bar-compare.png",
        ),
        None => println!("no Figma export to compare against"),
    }
}

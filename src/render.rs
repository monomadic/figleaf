//! Draws a [`Canvas`] with the host's wgpu device, inside the host's render pass.
//!
//! ```ignore
//! let mut renderer = Renderer::new(&device, surface_format);
//! // each frame, or whenever the UI changes:
//! renderer.prepare(&device, &queue, &canvas, [width, height], scale, &mut text)?;
//! renderer.draw(&mut pass);
//! ```
//!
//! Colours blend in the target's own space. On a plain `Rgba8Unorm` or
//! `Bgra8Unorm` target that is encoded sRGB, which is how Figma blends. On an
//! `*Srgb` target colours are converted first and blending is linear:
//! translucent black (most shadows) is corrected to darken as Figma shows,
//! other translucent colours come out slightly different from Figma.
use crate::{
    Canvas, Color, Error, Rect,
    paint::{Item, Layer, Mode, Outline},
    text::{self, Backend},
};
use std::{collections::HashMap, ops::Range};

const SHAPE_STRIDE: usize = 224;
const TEXT_STRIDE: usize = 48;

/// What a `prepare` call did. An unchanged canvas uploads nothing and reports
/// `changed: false`, so the host can skip the redraw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preparation {
    pub changed: bool,
    pub uploaded_bytes: u64,
    pub textures_created: u32,
}

/// A canvas item that was left out, by index, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Problem {
    pub item: usize,
    pub reason: &'static str,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MaskKey {
    string: String,
    style: [u32; 4],
    scale: u32,
    /// Inner shadow offset and blur; the colour is applied when drawing.
    shadow: Option<[u32; 3]>,
}

/// A wrapped paragraph: its string, style and box width.
#[derive(Clone, PartialEq, Eq, Hash)]
struct WrapKey {
    string: String,
    style: [u32; 4],
    width: u32,
}

fn style_bits(s: text::Style) -> [u32; 4] {
    [
        s.font as u32,
        s.size.to_bits(),
        s.line_height.to_bits(),
        s.letter_spacing.to_bits(),
    ]
}

struct Mask {
    id: u64,
    group: wgpu::BindGroup,
    /// Where the inner shadow falls on the glyphs, when the text has one.
    shadow: Option<wgpu::BindGroup>,
    size: [u32; 2],
    advance: f32,
    /// From the top of the line to the baseline, in logical units.
    baseline: f32,
}

struct MaskSlot {
    mask: Result<Mask, &'static str>,
    used: bool,
}

/// One thing a canvas item draws: a run of shape instances, or a line of text
/// as one instance with its mask (and the mask's identity, for change
/// detection).
enum Draw {
    Shapes(Range<u32>),
    Text(u32, wgpu::BindGroup, u64),
    /// Text followed by its inner shadow, which is the next instance.
    ShadowedText(u32, wgpu::BindGroup, u64, wgpu::BindGroup),
}

struct Instances {
    buffer: wgpu::Buffer,
    capacity: usize,
    bytes: Vec<u8>,
    uploaded: Vec<u8>,
    label: &'static str,
}

impl Instances {
    fn buffer(device: &wgpu::Device, label: &'static str, size: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
    fn new(device: &wgpu::Device, label: &'static str, stride: usize) -> Self {
        Self {
            buffer: Self::buffer(device, label, stride),
            capacity: stride,
            bytes: Vec::new(),
            uploaded: Vec::new(),
            label,
        }
    }
    fn push(&mut self, values: &[f32]) {
        for v in values {
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    /// Uploads if the bytes differ from the last upload. Returns the bytes
    /// written, or `None` when nothing changed.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<u64> {
        if self.bytes == self.uploaded {
            return None;
        }
        if self.bytes.len() > self.capacity {
            self.capacity = self.bytes.len().next_power_of_two();
            self.buffer = Self::buffer(device, self.label, self.capacity);
        }
        if !self.bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, &self.bytes);
        }
        std::mem::swap(&mut self.bytes, &mut self.uploaded);
        Some(self.uploaded.len() as u64)
    }
}

pub struct Renderer {
    linear: bool,
    shape_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    viewport: [u32; 2],
    shapes: Instances,
    texts: Instances,
    masks: HashMap<MaskKey, MaskSlot>,
    next_mask: u64,
    /// One entry per canvas item: empty for items that draw nothing, and one
    /// draw per line for text.
    draws: Vec<Vec<Draw>>,
    /// Line breaks of wrapped text, kept while the text stays on the canvas.
    wraps: HashMap<WrapKey, (Result<Vec<String>, &'static str>, bool)>,
    order: Vec<u64>,
    problems: Vec<Problem>,
    layers: Vec<Layer>,
}

fn pipeline(
    device: &wgpu::Device,
    label: &'static str,
    source: &str,
    format: wgpu::TextureFormat,
    stride: usize,
    attributes: &[wgpu::VertexAttribute],
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: stride as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn edges(r: Rect, scale: f32) -> [f32; 4] {
    [
        r.x * scale,
        r.y * scale,
        (r.x + r.w) * scale,
        (r.y + r.h) * scale,
    ]
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Renderer {
    /// `format` is the format of the view the host's pass draws into. The pass
    /// must be single-sampled and need no depth test from these pipelines.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shape_pipeline = pipeline(
            device,
            "figleaf shapes",
            include_str!("render/shapes.wgsl"),
            format,
            SHAPE_STRIDE,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4,10=>Float32x4,11=>Float32x4,12=>Uint32x4,13=>Uint32x4],
        );
        let text_pipeline = pipeline(
            device,
            "figleaf text",
            include_str!("render/text.wgsl"),
            format,
            TEXT_STRIDE,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4],
        );
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("figleaf viewport"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &shape_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        Self {
            linear: format.is_srgb(),
            shape_pipeline,
            text_pipeline,
            view,
            view_group,
            viewport: [0; 2],
            shapes: Instances::new(device, "figleaf shape instances", SHAPE_STRIDE),
            texts: Instances::new(device, "figleaf text instances", TEXT_STRIDE),
            masks: HashMap::new(),
            next_mask: 0,
            draws: Vec::new(),
            wraps: HashMap::new(),
            order: Vec::new(),
            problems: Vec::new(),
            layers: Vec::new(),
        }
    }

    /// Items the last `prepare` left out. Nothing is dropped without a line here.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    fn color(&self, c: Color) -> [f32; 4] {
        if self.linear {
            [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3]]
        } else {
            c
        }
    }

    /// Gets the canvas ready to draw into a target of `target` physical pixels,
    /// with `scale` physical pixels per logical unit. Text is rasterized by
    /// `text` the first time a string is seen in a style and kept while it
    /// stays on the canvas.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        target: [u32; 2],
        scale: f32,
        text: &mut dyn Backend,
    ) -> Result<Preparation, Error> {
        if !scale.is_finite() || scale <= 0. || target.contains(&0) {
            return Err(Error("invalid scale or target size"));
        }
        let mut report = Preparation::default();
        if self.viewport != target {
            let linear = f32::from(u8::from(self.linear));
            let size = [target[0] as f32, target[1] as f32, linear, 0.];
            queue.write_buffer(&self.view, 0, &size.map(f32::to_le_bytes).concat());
            self.viewport = target;
            report.changed = true;
            report.uploaded_bytes += 16;
        }
        let screen = Rect::new(0., 0., target[0] as f32, target[1] as f32);
        let clip_px = |clip: Option<Rect>| {
            let e = clip.map_or([0., 0., screen.w, screen.h], |c| edges(c, scale));
            Rect::new(e[0], e[1], e[2] - e[0], e[3] - e[1]).intersect(screen)
        };
        let outline = |o: Outline| (edges(o.rect, scale), o.radii.map(|r| r * scale));

        self.shapes.bytes.clear();
        self.texts.bytes.clear();
        self.draws.clear();
        self.problems.clear();
        for slot in self.masks.values_mut() {
            slot.used = false;
        }
        for slot in self.wraps.values_mut() {
            slot.1 = false;
        }
        for (index, item) in canvas.items().iter().enumerate() {
            self.draws.push(Vec::new());
            match item {
                Item::Shape(shape) => {
                    if let Some(reason) = shape.problem() {
                        self.problems.push(Problem {
                            item: index,
                            reason,
                        });
                        continue;
                    }
                    let first = (self.shapes.bytes.len() / SHAPE_STRIDE) as u32;
                    self.layers.clear();
                    shape.layers(&mut self.layers);
                    for i in 0..self.layers.len() {
                        let l = self.layers[i];
                        let clip = clip_px(l.clip.map(|c| c.rect));
                        // The clip's own outline, for its antialiased edge.
                        const OPEN: [f32; 4] = [-1e6, -1e6, 1e6, 1e6];
                        let (clip_rect, clip_radii) = l.clip.map_or((OPEN, [0.; 4]), outline);
                        let e = edges(l.extent(), scale);
                        // One spare pixel for the antialiased edge.
                        let quad = Rect::new(
                            e[0].floor() - 1.,
                            e[1].floor() - 1.,
                            e[2].ceil() - e[0].floor() + 2.,
                            e[3].ceil() - e[1].floor() + 2.,
                        )
                        .intersect(clip);
                        if quad.w <= 0. || quad.h <= 0. {
                            continue;
                        }
                        let (a_rect, a_radii) = outline(l.a);
                        let (b_rect, b_radii) = l.b.map_or(([0.; 4], [0.; 4]), outline);
                        let mode = match l.mode {
                            Mode::Band => 0.,
                            Mode::Drop => 1.,
                            Mode::Inner => 2.,
                        };
                        // A ramp keeps its stops as Figma has them; the
                        // shader blends them and then converts.
                        let color = match l.ramp {
                            Some(_) => l.color,
                            None => self.color(l.color),
                        };
                        self.shapes.push(&a_rect);
                        self.shapes.push(&a_radii);
                        self.shapes.push(&b_rect);
                        self.shapes.push(&b_radii);
                        self.shapes.push(&color);
                        self.shapes.push(&[
                            l.sigma * scale,
                            mode,
                            f32::from(u8::from(l.ellipse)),
                            f32::from(u8::from(l.ramp.is_some())),
                        ]);
                        self.shapes.push(&clip_rect);
                        self.shapes.push(&edges(quad, 1.));
                        self.shapes.push(&clip_radii);
                        match l.ramp {
                            Some(ramp) => {
                                let [x, y, c] = ramp.axis;
                                self.shapes.push(&[x / scale, y / scale, c, 0.]);
                                self.shapes.push(&ramp.stops.map(|s| s.offset));
                                for stop in ramp.stops {
                                    let bytes = stop.color.map(|v| (v * 255.).round() as u8);
                                    self.shapes.bytes.extend_from_slice(&bytes);
                                }
                            }
                            None => self.shapes.bytes.extend_from_slice(&[0; 80]),
                        }
                    }
                    let end = (self.shapes.bytes.len() / SHAPE_STRIDE) as u32;
                    if end > first {
                        self.draws[index].push(Draw::Shapes(first..end));
                    }
                }
                Item::Text(t) => {
                    if t.string.is_empty() || t.color[3] <= 0. {
                        continue;
                    }
                    let mut problem = None;
                    if t.rect.validate().is_err() || t.clip.is_some_and(|c| c.validate().is_err()) {
                        problem = Some("invalid rectangle");
                    } else if t.inner_shadow.is_some_and(|(c, o, blur)| {
                        !c.iter().chain(&o).all(|v| v.is_finite()) || !(0. ..=64.).contains(&blur)
                    }) {
                        problem = Some("invalid text shadow");
                    }
                    let lines = match problem {
                        Some(_) => Vec::new(),
                        None if !t.wrap => vec![t.string.clone()],
                        None => {
                            let key = WrapKey {
                                string: t.string.clone(),
                                style: style_bits(t.style()),
                                width: t.rect.w.to_bits(),
                            };
                            let slot = self.wraps.entry(key).or_insert_with(|| {
                                let lines = text::wrap(text, &t.string, t.style(), t.rect.w)
                                    .map(|lines| lines.into_iter().map(|l| l.text).collect())
                                    .map_err(|e| e.0);
                                (lines, true)
                            });
                            slot.1 = true;
                            match &slot.0 {
                                Ok(lines) => lines.clone(),
                                Err(reason) => {
                                    problem = Some(reason);
                                    Vec::new()
                                }
                            }
                        }
                    };
                    for (n, line) in lines.iter().enumerate() {
                        if line.is_empty() {
                            continue;
                        }
                        let key = MaskKey {
                            string: line.clone(),
                            style: style_bits(t.style()),
                            scale: scale.to_bits(),
                            shadow: t
                                .inner_shadow
                                .map(|(_, o, blur)| [o[0], o[1], blur].map(f32::to_bits)),
                        };
                        if !self.masks.contains_key(&key) {
                            let mask = match raster(text, line, t.style(), scale) {
                                Ok(m) => {
                                    report.textures_created += 1;
                                    report.uploaded_bytes += m.alpha.len() as u64;
                                    self.next_mask += 1;
                                    let size = [m.width, m.height];
                                    let shadow = t.inner_shadow.map(|(_, offset, blur)| {
                                        let offset = offset.map(|v| v * scale);
                                        let alpha =
                                            inner_shadow(&m.alpha, size, offset, blur / 2. * scale);
                                        report.uploaded_bytes += alpha.len() as u64;
                                        self.upload_mask(device, queue, size, &alpha)
                                    });
                                    Ok(Mask {
                                        id: self.next_mask,
                                        shadow,
                                        group: self.upload_mask(device, queue, size, &m.alpha),
                                        size: [m.width, m.height],
                                        advance: m.metrics.advance as f32,
                                        baseline: m.metrics.baseline as f32,
                                    })
                                }
                                Err(e) => Err(e.0),
                            };
                            self.masks
                                .insert(key.clone(), MaskSlot { mask, used: true });
                        }
                        let slot = self.masks.get_mut(&key).unwrap();
                        slot.used = true;
                        let mask = match &slot.mask {
                            Ok(mask) => mask,
                            Err(reason) => {
                                problem = Some(reason);
                                break;
                            }
                        };
                        let mut origin = t.line_origin(mask.advance, n, lines.len());
                        // Figma sets the baseline on a whole logical pixel.
                        let baseline = origin[1] + mask.baseline;
                        origin[1] += baseline.round() - baseline;
                        let origin = origin.map(|v| (v * scale).round());
                        let rect = [
                            origin[0],
                            origin[1],
                            origin[0] + mask.size[0] as f32,
                            origin[1] + mask.size[1] as f32,
                        ];
                        let (id, group, shadow) =
                            (mask.id, mask.group.clone(), mask.shadow.clone());
                        let instance = (self.texts.bytes.len() / TEXT_STRIDE) as u32;
                        let color = self.color(t.color);
                        self.texts.push(&rect);
                        self.texts.push(&edges(clip_px(t.clip), 1.));
                        self.texts.push(&color);
                        let draw = match (shadow, t.inner_shadow) {
                            (Some(shadow), Some((shadow_color, ..))) => {
                                let color = self.color(shadow_color);
                                self.texts.push(&rect);
                                self.texts.push(&edges(clip_px(t.clip), 1.));
                                self.texts.push(&color);
                                Draw::ShadowedText(instance, group, id, shadow)
                            }
                            _ => Draw::Text(instance, group, id),
                        };
                        self.draws[index].push(draw);
                    }
                    // A text with a line that can't be drawn is left out whole.
                    if let Some(reason) = problem {
                        self.draws[index].clear();
                        self.problems.push(Problem {
                            item: index,
                            reason,
                        });
                    }
                }
            }
        }
        let order: Vec<u64> = self
            .draws
            .iter()
            .enumerate()
            .flat_map(|(item, draws)| {
                draws.iter().map(move |d| match d {
                    Draw::Shapes(r) => [item as u64, 1, u64::from(r.start), u64::from(r.end)],
                    Draw::Text(instance, _, id) => [item as u64, 2, u64::from(*instance), *id],
                    Draw::ShadowedText(instance, _, id, _) => {
                        [item as u64, 3, u64::from(*instance), *id]
                    }
                })
            })
            .flatten()
            .collect();
        self.masks.retain(|_, slot| slot.used);
        self.wraps.retain(|_, slot| slot.1);

        for uploaded in [
            self.shapes.upload(device, queue),
            self.texts.upload(device, queue),
        ]
        .into_iter()
        .flatten()
        {
            report.uploaded_bytes += uploaded;
            report.changed = true;
        }
        report.changed |= report.textures_created > 0 || order != self.order;
        self.order = order;
        Ok(report)
    }

    fn upload_mask(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        [width, height]: [u32; 2],
        alpha: &[u8],
    ) -> wgpu::BindGroup {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("figleaf text mask"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            alpha,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            size,
        );
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.text_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.view.as_entire_binding(),
                },
            ],
        })
    }

    /// Draws what the last `prepare` saw, over whatever the pass already holds.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_items(pass, 0..self.draws.len());
    }

    /// Draws only the canvas items in `items`, by index. A host with its own
    /// ordered display list uses this to put its drawing between ours. This
    /// sets the pipeline, bind group 0 and vertex buffer 0; the host restores
    /// its own afterwards. The scissor rectangle is left as the host set it.
    pub fn draw_items(&self, pass: &mut wgpu::RenderPass<'_>, items: Range<usize>) {
        let end = items.end.min(self.draws.len());
        let mut run: Option<Range<u32>> = None;
        let flush = |pass: &mut wgpu::RenderPass<'_>, run: &mut Option<Range<u32>>| {
            if let Some(range) = run.take() {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &self.view_group, &[]);
                pass.set_vertex_buffer(0, self.shapes.buffer.slice(..));
                pass.draw(0..6, range);
            }
        };
        for draw in self.draws[items.start.min(end)..end].iter().flatten() {
            match draw {
                // Instances of consecutive shapes are contiguous.
                Draw::Shapes(range) => match &mut run {
                    Some(open) if open.end == range.start => open.end = range.end,
                    _ => {
                        flush(pass, &mut run);
                        run = Some(range.clone());
                    }
                },
                Draw::Text(instance, group, _) | Draw::ShadowedText(instance, group, ..) => {
                    flush(pass, &mut run);
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, group, &[]);
                    pass.set_vertex_buffer(0, self.texts.buffer.slice(..));
                    pass.draw(0..6, *instance..*instance + 1);
                    if let Draw::ShadowedText(.., shadow) = draw {
                        pass.set_bind_group(0, shadow, &[]);
                        pass.draw(0..6, *instance + 1..*instance + 2);
                    }
                }
            }
        }
        flush(pass, &mut run);
    }
}

/// One line as a mask just wide enough for it.
fn raster(
    backend: &mut dyn Backend,
    string: &str,
    style: text::Style,
    scale: f32,
) -> Result<text::Mask, Error> {
    let measured = backend.measure(string, style)?;
    if !measured.advance.is_finite() || measured.advance < 0. {
        return Err(Error("text backend violated mask or metric limits"));
    }
    // One spare unit keeps the last glyph's antialiased edge.
    let width = (measured.advance.ceil() as f32 + 1.).clamp(1., text::MAX_WIDTH);
    let mask = backend.raster(string, style, width, scale)?;
    let expected = text::validate(string, style, width, scale)?;
    if [mask.width, mask.height] != expected
        || mask.alpha.len() != expected[0] as usize * expected[1] as usize
        || !mask.metrics.advance.is_finite()
        || mask.metrics.lines != 1
    {
        return Err(Error("text backend violated mask or metric limits"));
    }
    Ok(mask)
}

/// Coverage of an inner shadow on glyphs: inside the glyph, wherever the
/// blurred, shifted outside of the glyph reaches. `sigma` and `offset` are in
/// mask pixels; beyond the mask counts as outside.
fn inner_shadow(alpha: &[u8], [w, h]: [u32; 2], offset: [f32; 2], sigma: f32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    // How much of each pixel is outside the glyph.
    let mut outside: Vec<f32> = alpha.iter().map(|a| 1. - f32::from(*a) / 255.).collect();
    if sigma >= 0.25 {
        let radius = (3. * sigma).ceil() as isize;
        let kernel: Vec<f32> = (-radius..=radius)
            .map(|i| (-((i * i) as f32) / (2. * sigma * sigma)).exp())
            .collect();
        let total: f32 = kernel.iter().sum();
        let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
            let mut out = vec![0.; src.len()];
            for y in 0..h as isize {
                for x in 0..w as isize {
                    let mut sum = 0.;
                    for (k, weight) in kernel.iter().enumerate() {
                        let d = k as isize - radius;
                        let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                        let inside = sx >= 0 && sy >= 0 && sx < w as isize && sy < h as isize;
                        let v = if inside {
                            src[sy as usize * w + sx as usize]
                        } else {
                            1.
                        };
                        sum += weight * v;
                    }
                    out[y as usize * w + x as usize] = sum / total;
                }
            }
            out
        };
        let rows = pass(&outside, true);
        outside = pass(&rows, false);
    }
    let sample = |x: isize, y: isize| {
        if x >= 0 && y >= 0 && x < w as isize && y < h as isize {
            outside[y as usize * w + x as usize]
        } else {
            1.
        }
    };
    let mut out = vec![0u8; alpha.len()];
    for y in 0..h {
        for x in 0..w {
            // The shadow at a pixel is the outside as it was `offset` back.
            let (fx, fy) = (x as f32 - offset[0], y as f32 - offset[1]);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let (x0, y0) = (x0 as isize, y0 as isize);
            let top = sample(x0, y0) * (1. - tx) + sample(x0 + 1, y0) * tx;
            let bottom = sample(x0, y0 + 1) * (1. - tx) + sample(x0 + 1, y0 + 1) * tx;
            let shade = top * (1. - ty) + bottom * ty;
            let glyph = f32::from(alpha[y * w + x]) / 255.;
            out[y * w + x] = (glyph * shade * 255.).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_inner_shadow_stays_inside_the_glyph_and_follows_its_offset() {
        // A 6x6 solid block in a 10x10 mask, shadow pushed 2 down, no blur.
        let mut alpha = vec![0u8; 100];
        for y in 2..8 {
            for x in 2..8 {
                alpha[y * 10 + x] = 255;
            }
        }
        let shadow = inner_shadow(&alpha, [10, 10], [0., 2.], 0.);
        let at = |x: usize, y: usize| shadow[y * 10 + x];
        assert_eq!((at(4, 2), at(4, 3), at(4, 4), at(4, 7)), (255, 255, 0, 0));
        assert_eq!((at(4, 1), at(1, 4), at(4, 8)), (0, 0, 0));
        // Blurred, it fades into the block instead of stopping.
        let soft = inner_shadow(&alpha, [10, 10], [0., 2.], 1.);
        let at = |x: usize, y: usize| soft[y * 10 + x];
        assert!(at(4, 2) > at(4, 4) && at(4, 4) > at(4, 6) && at(4, 1) == 0);
    }
}

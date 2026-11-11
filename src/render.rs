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
//! `Bgra8Unorm` target that is encoded sRGB, which is how Figma blends; on an
//! `*Srgb` target colours are converted first and blending is linear, so
//! translucent edges and shadows come out slightly different from Figma.
use crate::{
    Canvas, Color, Error, Rect,
    paint::{Item, Layer, Mode, Outline},
    text::{self, Backend},
};
use std::{collections::HashMap, ops::Range};

const SHAPE_STRIDE: usize = 144;
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
    font: usize,
    size: u32,
    line_height: u32,
    letter_spacing: u32,
    scale: u32,
}

struct Mask {
    id: u64,
    group: wgpu::BindGroup,
    size: [u32; 2],
    advance: f32,
}

struct MaskSlot {
    mask: Result<Mask, &'static str>,
    used: bool,
}

/// What one canvas item draws: a run of shape instances, or one text
/// instance with its mask (and the mask's identity, for change detection).
enum Draw {
    Shapes(Range<u32>),
    Text(u32, wgpu::BindGroup, u64),
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
    /// One entry per canvas item; `None` for items that draw nothing.
    draws: Vec<Option<Draw>>,
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
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4],
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
            let size = [target[0] as f32, target[1] as f32, 0., 0.];
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
        for (index, item) in canvas.items().iter().enumerate() {
            self.draws.push(None);
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
                        let color = self.color(l.color);
                        self.shapes.push(&a_rect);
                        self.shapes.push(&a_radii);
                        self.shapes.push(&b_rect);
                        self.shapes.push(&b_radii);
                        self.shapes.push(&color);
                        self.shapes.push(&[
                            l.sigma * scale,
                            mode,
                            f32::from(u8::from(l.ellipse)),
                            f32::from(u8::from(l.b.is_some())),
                        ]);
                        self.shapes.push(&clip_rect);
                        self.shapes.push(&edges(quad, 1.));
                        self.shapes.push(&clip_radii);
                    }
                    let end = (self.shapes.bytes.len() / SHAPE_STRIDE) as u32;
                    if end > first {
                        self.draws[index] = Some(Draw::Shapes(first..end));
                    }
                }
                Item::Text(t) => {
                    if t.string.is_empty() || t.color[3] <= 0. {
                        continue;
                    }
                    if t.rect.validate().is_err() || t.clip.is_some_and(|c| c.validate().is_err()) {
                        self.problems.push(Problem {
                            item: index,
                            reason: "invalid rectangle",
                        });
                        continue;
                    }
                    let key = MaskKey {
                        string: t.string.clone(),
                        font: t.font,
                        size: t.size.to_bits(),
                        line_height: t.line_height.to_bits(),
                        letter_spacing: t.letter_spacing.to_bits(),
                        scale: scale.to_bits(),
                    };
                    if !self.masks.contains_key(&key) {
                        let mask = match raster(text, &t.string, t.style(), scale) {
                            Ok(m) => {
                                report.textures_created += 1;
                                report.uploaded_bytes += m.alpha.len() as u64;
                                self.next_mask += 1;
                                Ok(Mask {
                                    id: self.next_mask,
                                    group: self.upload_mask(device, queue, &m),
                                    size: [m.width, m.height],
                                    advance: m.metrics.advance as f32,
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
                            self.problems.push(Problem {
                                item: index,
                                reason,
                            });
                            continue;
                        }
                    };
                    let origin = t.origin(mask.advance).map(|v| (v * scale).round());
                    let rect = [
                        origin[0],
                        origin[1],
                        origin[0] + mask.size[0] as f32,
                        origin[1] + mask.size[1] as f32,
                    ];
                    let (id, group) = (mask.id, mask.group.clone());
                    let instance = (self.texts.bytes.len() / TEXT_STRIDE) as u32;
                    let color = self.color(t.color);
                    self.texts.push(&rect);
                    self.texts.push(&edges(clip_px(t.clip), 1.));
                    self.texts.push(&color);
                    self.draws[index] = Some(Draw::Text(instance, group, id));
                }
            }
        }
        let order: Vec<u64> = self
            .draws
            .iter()
            .flat_map(|d| match d {
                None => [0, 0, 0],
                Some(Draw::Shapes(r)) => [1, u64::from(r.start), u64::from(r.end)],
                Some(Draw::Text(instance, _, id)) => [2, u64::from(*instance), *id],
            })
            .collect();
        self.masks.retain(|_, slot| slot.used);

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
        mask: &text::Mask,
    ) -> wgpu::BindGroup {
        let size = wgpu::Extent3d {
            width: mask.width,
            height: mask.height,
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
            &mask.alpha,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mask.width),
                rows_per_image: Some(mask.height),
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
                Draw::Text(instance, group, _) => {
                    flush(pass, &mut run);
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, group, &[]);
                    pass.set_vertex_buffer(0, self.texts.buffer.slice(..));
                    pass.draw(0..6, *instance..*instance + 1);
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

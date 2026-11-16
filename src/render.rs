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
    paint::{Item, Mode, Outline},
    path,
    text::{self, Backend},
};
use std::{collections::HashMap, ops::Range};

mod masked;

const SHAPE_STRIDE: usize = 256;
const TEXT_STRIDE: usize = 112;

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
    /// Drop shadow blur; the offset and colour are applied when drawing.
    drop: Option<u32>,
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
    /// The blurred glyphs for a drop shadow: mask, its size, and how many
    /// pixels it reaches beyond the text's own mask on every side.
    drop: Option<(wgpu::BindGroup, [u32; 2], u32)>,
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
    Composite(wgpu::BindGroup, u64),
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
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE,
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
    format: wgpu::TextureFormat,
    composite_pipeline: wgpu::RenderPipeline,
    composites: HashMap<usize, masked::Composite>,
    failed_composites: HashMap<usize, masked::Failure>,
    linear: bool,
    shape_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    viewport: [u32; 2],
    shapes: Instances,
    texts: Instances,
    path_edges: Instances,
    clip_data: Instances,
    text_clip_group: wgpu::BindGroup,
    transforms: Instances,
    path_group: wgpu::BindGroup,
    masks: HashMap<MaskKey, MaskSlot>,
    next_mask: u64,
    /// One entry per canvas item: empty for items that draw nothing, and one
    /// draw per line for text.
    draws: Vec<Vec<Draw>>,
    /// Line breaks of wrapped text, kept while the text stays on the canvas.
    wraps: HashMap<WrapKey, (Result<Vec<String>, &'static str>, bool)>,
    order: Vec<u64>,
    problems: Vec<Problem>,
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
            &format!(
                "{}\n{}",
                include_str!("render/clips.wgsl"),
                include_str!("render/shapes.wgsl")
            ),
            format,
            SHAPE_STRIDE,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4,10=>Float32x4,11=>Float32x4,12=>Uint32x4,13=>Uint32x4,14=>Float32x4,15=>Float32x4],
        );
        let text_pipeline = pipeline(
            device,
            "figleaf text",
            &format!(
                "{}\n{}",
                include_str!("render/clips.wgsl"),
                include_str!("render/text.wgsl")
            ),
            format,
            TEXT_STRIDE,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4],
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
        let path_edges = Instances::new(device, "figleaf vector edges", 32);
        let transforms = Instances::new(device, "figleaf transforms", 80);
        let clip_data = Instances::new(device, "figleaf clips", 16);
        let text_clip_group = Self::text_clip_group(device, &text_pipeline, &clip_data.buffer);
        let path_group = Self::path_group(
            device,
            &shape_pipeline,
            &path_edges.buffer,
            &transforms.buffer,
            &clip_data.buffer,
        );
        Self {
            format,
            composite_pipeline: masked::pipeline(device, format),
            composites: HashMap::new(),
            failed_composites: HashMap::new(),
            clip_data,
            text_clip_group,
            path_edges,
            transforms,
            path_group,
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
    /// stays on the canvas. Mask scopes render to cached offscreen targets here;
    /// this submits their GPU work before the host encodes its drawing pass.
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
        self.path_edges.bytes.clear();
        self.clip_data.bytes.clear();
        self.transforms.bytes.clear();
        self.draws.clear();
        self.problems.clear();
        for slot in self.masks.values_mut() {
            slot.used = false;
        }
        for slot in self.wraps.values_mut() {
            slot.1 = false;
        }
        let mut mask_count = 0;
        let mask_problem = masked::validate(canvas, 0, &mut mask_count)
            .err()
            .or_else(|| {
                (mask_count > 0
                    && (target
                        .iter()
                        .any(|d| *d > device.limits().max_texture_dimension_2d)
                        || u64::from(target[0]) * u64::from(target[1]) * mask_count as u64
                            > 32_000_000))
                    .then_some(Error("mask target pixel budget exceeded"))
            });
        self.failed_composites
            .retain(|index, _| matches!(canvas.items().get(*index), Some(Item::Masked(_))));
        self.composites
            .retain(|index, _| matches!(canvas.items().get(*index), Some(Item::Masked(_))));
        for (index, item) in canvas.items().iter().enumerate() {
            self.draws.push(Vec::new());
            if let Item::Masked(masked) = item {
                let result = if let Some(e) = &mask_problem {
                    Err(Error(e.0))
                } else {
                    self.prepare_masked(
                        device,
                        queue,
                        masked,
                        index,
                        target,
                        scale,
                        text,
                        &mut report,
                    )
                };
                if let Err(e) = result {
                    self.composites.remove(&index);
                    self.problems.push(Problem {
                        item: index,
                        reason: e.0,
                    });
                }
                continue;
            }
            let clips = match item {
                Item::Masked(_) => unreachable!(),
                Item::Shape(s) => &s.clips,
                Item::Text(t) => &t.clips,
            };
            let (clip_range, stack_bounds) = match self.prepare_clips(device, clips, scale, screen)
            {
                Ok(value) => value,
                Err(e) => {
                    self.problems.push(Problem {
                        item: index,
                        reason: e.0,
                    });
                    continue;
                }
            };
            match item {
                Item::Masked(_) => unreachable!(),
                Item::Shape(shape) => {
                    if let Some(reason) = shape.problem() {
                        self.problems.push(Problem {
                            item: index,
                            reason,
                        });
                        continue;
                    }
                    let first = (self.shapes.bytes.len() / SHAPE_STRIDE) as u32;
                    let mut layers = Vec::new();
                    shape.layers(&mut layers);
                    let mut prepared = Vec::new();
                    let mut pending_edges = Vec::new();
                    let mut path_error = None;
                    let mut path_work = 0.;
                    let edge_start = self.path_edges.bytes.len() / 32;
                    for l in &layers {
                        let mut ranges = [[0.; 4]; 3];
                        let mut bounds = None;
                        let primitive = if let Some(geometry) = l.primitive {
                            match geometry.render_paths(
                                shape.rect,
                                shape.radii[0],
                                l.vector_stroke.and(shape.stroke),
                            ) {
                                Ok(paths) => Some(paths),
                                Err(e) => {
                                    path_error = Some(e.0);
                                    break;
                                }
                            }
                        } else if l.vector_stroke.is_some() && l.vector_paths.is_none() {
                            Some(vec![path::primitive_path(
                                shape.rect,
                                shape.radii,
                                shape.ellipse,
                            )])
                        } else {
                            None
                        };
                        if let Some(paths) = l.vector_paths.or(primitive.as_deref()) {
                            let groups = if l.path_shadow.is_some() {
                                let stroked = if let Some(outlines) = &shape.stroke_geometry {
                                    Ok([outlines.clone(), Vec::new(), Vec::new()])
                                } else if let Some(stroke) = shape.stroke {
                                    path::stroke_outlines(
                                        shape.stroke_paths.as_deref().unwrap_or(paths),
                                        stroke.width,
                                        shape.path_stroke,
                                        stroke.align,
                                        scale * shape.transform.max_scale(),
                                        &shape.dash_pattern,
                                    )
                                } else {
                                    Ok(Default::default())
                                };
                                stroked.map(|[closed, open, _]| [paths.to_vec(), closed, open])
                            } else if let Some(outlines) = l.stroke_geometry {
                                Ok([outlines.to_vec(), Vec::new(), paths.to_vec()])
                            } else if let Some((stroke, style)) = l.vector_stroke {
                                path::stroke_outlines(
                                    paths,
                                    stroke.width,
                                    style,
                                    stroke.align,
                                    scale * shape.transform.max_scale(),
                                    l.dash_pattern,
                                )
                            } else {
                                Ok([paths.to_vec(), Vec::new(), Vec::new()])
                            };
                            let result = groups.and_then(|mut groups| {
                                // Resolved fill regions supply holes/orientation when the
                                // caller provides separate network centerlines.
                                if l.vector_stroke.is_some() && shape.stroke_paths.is_some() {
                                    groups[2] = shape.vector_paths.clone().unwrap_or_default();
                                }
                                let mut all = Vec::new();
                                for (i, group) in groups.iter().enumerate() {
                                    let edges = path::flatten_transformed(
                                        group,
                                        if primitive.is_some() {
                                            [0.; 2]
                                        } else {
                                            l.vector_origin
                                        },
                                        scale,
                                        if l.path_shadow.is_some() {
                                            crate::Transform::IDENTITY
                                        } else {
                                            l.transform
                                        },
                                    )?;
                                    ranges[i] = [
                                        (edge_start + pending_edges.len() + all.len()) as f32,
                                        edges.len() as f32,
                                        0.,
                                        0.,
                                    ];
                                    all.extend(edges);
                                }
                                Ok(all)
                            });
                            match result {
                                Err(e) => {
                                    path_error = Some(e.0);
                                    break;
                                }
                                Ok(edges) => {
                                    // Actual outline bounds include every cap and miter tip.
                                    let mut lo = [f32::INFINITY; 2];
                                    let mut hi = [f32::NEG_INFINITY; 2];
                                    for e in &edges {
                                        for p in
                                            [[e.points[0], e.points[1]], [e.points[2], e.points[3]]]
                                        {
                                            for i in 0..2 {
                                                lo[i] = lo[i].min(p[i]);
                                                hi[i] = hi[i].max(p[i]);
                                            }
                                        }
                                    }
                                    let r = if edges.is_empty() {
                                        Rect::default()
                                    } else {
                                        Rect::new(lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1])
                                    };
                                    let extent = if l.path_shadow.is_some() {
                                        {
                                            let e = l.extent();
                                            Rect::new(
                                                e.x * scale,
                                                e.y * scale,
                                                e.w * scale,
                                                e.h * scale,
                                            )
                                        }
                                    } else {
                                        r
                                    };
                                    let visible = extent
                                        .inflate(1.)
                                        .intersect(clip_px(
                                            l.clip.map(|c| l.clip_transform.bounds(c.rect)),
                                        ))
                                        .intersect(stack_bounds);
                                    let work = f64::from(visible.w.ceil())
                                        * f64::from(visible.h.ceil())
                                        * edges.len() as f64
                                        * if l.path_shadow.is_some() { 4. } else { 16. };
                                    path_work += work;
                                    if path_work > 512_000_000. {
                                        path_error = Some("vector coverage work limit exceeded");
                                        break;
                                    }
                                    bounds = if l.path_shadow.is_some() {
                                        None
                                    } else {
                                        Some(r)
                                    };
                                    if pending_edges.len() + edges.len() > 8192 {
                                        path_error = Some("vector exceeds 8192 flattened edges");
                                        break;
                                    }
                                    pending_edges.extend(edges);
                                }
                            }
                        }
                        prepared.push((ranges, bounds));
                    }
                    if pending_edges.len() > 8192 {
                        path_error = Some("vector exceeds 8192 flattened edges");
                    }
                    let limit = (device.limits().max_storage_buffer_binding_size as usize)
                        .min(32 * 1024 * 1024);
                    if ((edge_start + pending_edges.len()) * 32)
                        .max(32)
                        .next_power_of_two()
                        > limit
                    {
                        path_error = Some("vector edge buffer limit exceeded");
                    }
                    if let Some(reason) = path_error {
                        self.problems.push(Problem {
                            item: index,
                            reason,
                        });
                        continue;
                    }
                    for edge in pending_edges {
                        self.path_edges.push(&edge.points);
                        self.path_edges.push(&[
                            f32::from(edge.even_odd),
                            f32::from(edge.end_region),
                            0.,
                            0.,
                        ]);
                    }
                    let inverse = shape.transform.pixels(scale, scale).inverse();
                    let clip_inverse = shape.clip_transform.pixels(scale, scale).inverse();
                    let (inverse, clip_inverse) = match (inverse, clip_inverse) {
                        (Ok(a), Ok(b))
                            if layers.iter().all(|l| {
                                l.extent().validate().is_ok()
                                    && l.clip.is_none_or(|c| {
                                        l.clip_transform.bounds(c.rect).validate().is_ok()
                                    })
                            }) =>
                        {
                            (a, b)
                        }
                        _ => {
                            self.problems.push(Problem {
                                item: index,
                                reason: "invalid transformed geometry",
                            });
                            continue;
                        }
                    };
                    let transform_index = self.transforms.bytes.len() / 80;
                    if (self.transforms.bytes.len() + 80).next_power_of_two()
                        > device.limits().max_storage_buffer_binding_size as usize
                    {
                        self.problems.push(Problem {
                            item: index,
                            reason: "transform buffer limit exceeded",
                        });
                        continue;
                    }
                    for m in [inverse, clip_inverse] {
                        let linear_identity = m.0[0][0] == 1.
                            && m.0[0][1] == 0.
                            && m.0[1][0] == 0.
                            && m.0[1][1] == 1.;
                        self.transforms.push(&[
                            m.0[0][0],
                            m.0[0][1],
                            m.0[0][2],
                            f32::from(!linear_identity),
                        ]);
                        self.transforms.push(&[m.0[1][0], m.0[1][1], m.0[1][2], 0.]);
                    }
                    self.transforms.push(&clip_range);
                    for (l, (ranges, path_bounds)) in layers.into_iter().zip(prepared) {
                        let path_range = (l.vector_paths.is_some()
                            || l.primitive.is_some()
                            || l.vector_stroke.is_some())
                        .then_some(ranges[0]);
                        let clip = clip_px(l.clip.map(|c| l.clip_transform.bounds(c.rect)))
                            .intersect(stack_bounds);
                        // The clip's own outline, for its antialiased edge.
                        const OPEN: [f32; 4] = [-1e6, -1e6, 1e6, 1e6];
                        let (clip_rect, clip_radii) = l.clip.map_or((OPEN, [0.; 4]), outline);
                        let e =
                            path_bounds.map_or_else(|| edges(l.extent(), scale), |r| edges(r, 1.));
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
                        let (b_rect, b_radii) = l.b.map_or(([0.; 4], [-1.; 4]), outline);
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
                        self.shapes.push(&l.path_shadow.map_or(a_rect, |s| {
                            [
                                s.spread * scale,
                                s.offset[0] * scale,
                                s.offset[1] * scale,
                                f32::from(s.behind),
                            ]
                        }));
                        self.shapes.push(&if path_range.is_some() {
                            ranges[1]
                        } else {
                            a_radii
                        });
                        self.shapes.push(&path_range.unwrap_or(b_rect));
                        self.shapes.push(&if path_range.is_some() {
                            ranges[2]
                        } else {
                            b_radii
                        });
                        self.shapes.push(&color);
                        self.shapes.push(&[
                            l.sigma * scale,
                            l.vector_stroke.map_or(mode, |(s, _)| match s.align {
                                crate::StrokeAlign::Center => 0.,
                                crate::StrokeAlign::Inside => 1.,
                                crate::StrokeAlign::Outside => 2.,
                            }),
                            if l.path_shadow.is_some() {
                                5.
                            } else if l.vector_stroke.is_some() {
                                4.
                            } else if path_range.is_some() {
                                3.
                            } else if l.line {
                                2.
                            } else {
                                f32::from(u8::from(l.ellipse))
                            },
                            if l.path_shadow.is_some() {
                                let align = l.shadow_stroke.map_or(0., |s| match s.align {
                                    crate::StrokeAlign::Center => 0.,
                                    crate::StrokeAlign::Inside => 1.,
                                    crate::StrokeAlign::Outside => 2.,
                                });
                                align + 4. * f32::from(l.shadow_has_fill)
                            } else {
                                f32::from(u8::from(l.ramp.is_some()))
                            },
                        ]);
                        self.shapes.push(&clip_rect);
                        self.shapes.push(&edges(quad, 1.));
                        self.shapes.push(&clip_radii);
                        match l.ramp {
                            Some(ramp) => {
                                let [x, y, c] = ramp.axis;
                                let a = x / scale;
                                let b = y / scale;
                                self.shapes.push(&[
                                    a * inverse.0[0][0] + b * inverse.0[1][0],
                                    a * inverse.0[0][1] + b * inverse.0[1][1],
                                    c + a * inverse.0[0][2] + b * inverse.0[1][2],
                                    transform_index as f32,
                                ]);
                                self.shapes.push(&ramp.stops.map(|s| s.offset));
                                for stop in ramp.stops {
                                    let bytes = stop.color.map(|v| (v * 255.).round() as u8);
                                    self.shapes.bytes.extend_from_slice(&bytes);
                                }
                            }
                            None => {
                                self.shapes.push(&[0., 0., 0., transform_index as f32]);
                                self.shapes.bytes.extend_from_slice(&[0; 64]);
                            }
                        }
                        self.shapes.push(&l.a.radii_y.map(|v| v * scale));
                        self.shapes
                            .push(&l.b.map_or([0.; 4], |b| b.radii_y.map(|v| v * scale)));
                    }
                    let end = (self.shapes.bytes.len() / SHAPE_STRIDE) as u32;
                    if end > first {
                        self.draws[index].push(Draw::Shapes(first..end));
                    }
                }
                Item::Text(t) => {
                    if let Err(e) = t
                        .transform
                        .inverse()
                        .and_then(|_| t.clip_transform.inverse())
                    {
                        self.problems.push(Problem {
                            item: index,
                            reason: e.0,
                        });
                        continue;
                    }
                    let raster_scale = (scale * t.transform.max_scale()).max(scale).max(0.5);
                    let forward = t.transform.pixels(raster_scale, scale);
                    let clip_inverse = match t.clip_transform.pixels(raster_scale, scale).inverse()
                    {
                        Ok(m) => m,
                        Err(e) => {
                            self.problems.push(Problem {
                                item: index,
                                reason: e.0,
                            });
                            continue;
                        }
                    };
                    let mut text_transforms =
                        [forward, clip_inverse].map(|m| m.0.map(|r| [r[0], r[1], r[2], 0.]));
                    text_transforms[0][0][3] = clip_range[0];
                    text_transforms[0][1][3] = clip_range[1];
                    let text_clip = t
                        .clip
                        .map_or([-1e7, -1e7, 1e7, 1e7], |r| edges(r, raster_scale));
                    if t.string.is_empty() || t.color[3] <= 0. {
                        continue;
                    }
                    let mut problem = None;
                    if t.rect.validate().is_err()
                        || t.transform.bounds(t.rect).validate().is_err()
                        || t.clip.is_some_and(|c| {
                            c.validate().is_err() || t.clip_transform.bounds(c).validate().is_err()
                        })
                    {
                        problem = Some("invalid rectangle");
                    } else if [t.inner_shadow, t.drop_shadow].iter().flatten().any(
                        |(c, o, blur)| {
                            !c.iter().chain(o).all(|v| v.is_finite()) || !(0. ..=64.).contains(blur)
                        },
                    ) {
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
                            scale: raster_scale.to_bits(),
                            shadow: t
                                .inner_shadow
                                .map(|(_, o, blur)| [o[0], o[1], blur].map(f32::to_bits)),
                            drop: t.drop_shadow.map(|(.., blur)| blur.to_bits()),
                        };
                        if !self.masks.contains_key(&key) {
                            let mask = match raster(text, line, t.style(), raster_scale) {
                                Ok(m) => {
                                    report.textures_created += 1;
                                    report.uploaded_bytes += m.alpha.len() as u64;
                                    self.next_mask += 1;
                                    let size = [m.width, m.height];
                                    let shadow = t.inner_shadow.map(|(_, offset, blur)| {
                                        let offset = offset.map(|v| v * raster_scale);
                                        let alpha = inner_shadow(
                                            &m.alpha,
                                            size,
                                            offset,
                                            blur / 2. * raster_scale,
                                        );
                                        report.uploaded_bytes += alpha.len() as u64;
                                        self.upload_mask(device, queue, size, &alpha)
                                    });
                                    let drop = t.drop_shadow.map(|(.., blur)| {
                                        let (alpha, size, pad) =
                                            drop_shadow(&m.alpha, size, blur / 2. * raster_scale);
                                        report.uploaded_bytes += alpha.len() as u64;
                                        (self.upload_mask(device, queue, size, &alpha), size, pad)
                                    });
                                    Ok(Mask {
                                        id: self.next_mask,
                                        shadow,
                                        drop,
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
                        let origin = origin.map(|v| (v * raster_scale).round());
                        let rect = [
                            origin[0],
                            origin[1],
                            origin[0] + mask.size[0] as f32,
                            origin[1] + mask.size[1] as f32,
                        ];
                        let (id, group, shadow, drop) = (
                            mask.id,
                            mask.group.clone(),
                            mask.shadow.clone(),
                            mask.drop.clone(),
                        );
                        // The drop shadow is its own mask, drawn first.
                        if let (Some((group, size, pad)), Some((color, offset, _))) =
                            (drop, t.drop_shadow)
                        {
                            let at = [0, 1].map(|i| {
                                origin[i] + (offset[i] * raster_scale).round() - pad as f32
                            });
                            let instance = (self.texts.bytes.len() / TEXT_STRIDE) as u32;
                            let color = self.color(color);
                            self.texts.push(&[
                                at[0],
                                at[1],
                                at[0] + size[0] as f32,
                                at[1] + size[1] as f32,
                            ]);
                            self.texts.push(&text_clip);
                            self.texts.push(&color);
                            for rows in text_transforms {
                                for row in rows {
                                    self.texts.push(&row);
                                }
                            }
                            self.draws[index].push(Draw::Text(instance, group, id));
                        }
                        let instance = (self.texts.bytes.len() / TEXT_STRIDE) as u32;
                        let color = self.color(t.color);
                        self.texts.push(&rect);
                        self.texts.push(&text_clip);
                        self.texts.push(&color);
                        for rows in text_transforms {
                            for row in rows {
                                self.texts.push(&row);
                            }
                        }
                        let draw = match (shadow, t.inner_shadow) {
                            (Some(shadow), Some((shadow_color, ..))) => {
                                let color = self.color(shadow_color);
                                self.texts.push(&rect);
                                self.texts.push(&text_clip);
                                self.texts.push(&color);
                                for rows in text_transforms {
                                    for row in rows {
                                        self.texts.push(&row);
                                    }
                                }
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
                    Draw::Composite(_, id) => [item as u64, 4, *id, 0],
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

        let transform_capacity = self.transforms.capacity;
        let transforms_uploaded = self.transforms.upload(device, queue);
        let clip_capacity = self.clip_data.capacity;
        let clips_uploaded = self.clip_data.upload(device, queue);
        if clip_capacity != self.clip_data.capacity {
            self.text_clip_group =
                Self::text_clip_group(device, &self.text_pipeline, &self.clip_data.buffer);
        }
        let capacity = self.path_edges.capacity;
        let paths_uploaded = self.path_edges.upload(device, queue);
        if capacity != self.path_edges.capacity
            || transform_capacity != self.transforms.capacity
            || clip_capacity != self.clip_data.capacity
        {
            self.path_group = Self::path_group(
                device,
                &self.shape_pipeline,
                &self.path_edges.buffer,
                &self.transforms.buffer,
                &self.clip_data.buffer,
            );
        }
        for uploaded in [
            clips_uploaded,
            paths_uploaded,
            transforms_uploaded,
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

    fn text_clip_group(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("figleaf text clips"),
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    fn prepare_clips(
        &mut self,
        device: &wgpu::Device,
        clips: &[crate::Clip],
        scale: f32,
        screen: Rect,
    ) -> Result<([f32; 4], Rect), Error> {
        if clips.len() > 128 {
            return Err(Error("clip stack exceeds 128 clips"));
        }
        let mut data = Vec::new();
        let mut visible = screen;
        let mut total = 0;
        for clip in clips {
            let edges = clip.edges(scale)?;
            total += edges.len();
            if total > 8192 {
                return Err(Error("clip stack exceeds 8192 flattened edges"));
            }
            let mut lo = [f32::INFINITY; 2];
            let mut hi = [f32::NEG_INFINITY; 2];
            for edge in &edges {
                for p in [
                    [edge.points[0], edge.points[1]],
                    [edge.points[2], edge.points[3]],
                ] {
                    for i in 0..2 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
            }
            let bounds = if edges.is_empty() {
                Rect::default()
            } else {
                Rect::new(lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1])
            };
            bounds.validate()?;
            visible = visible.intersect(bounds.inflate(1.));
            data.push([edges.len() as f32, 0., 0., 0.]);
            data.push([bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h]);
            for edge in edges {
                data.push(edge.points);
                data.push([f32::from(edge.even_odd), f32::from(edge.end_region), 0., 0.]);
            }
        }
        if f64::from(visible.w.ceil()) * f64::from(visible.h.ceil()) * total as f64 * 16.
            > 512_000_000.
        {
            return Err(Error("clip coverage work limit exceeded"));
        }
        if (self.clip_data.bytes.len() + data.len() * 16)
            .max(16)
            .next_power_of_two()
            > device.limits().max_storage_buffer_binding_size as usize
        {
            return Err(Error("clip buffer limit exceeded"));
        }
        let range = [
            (self.clip_data.bytes.len() / 16) as f32,
            clips.len() as f32,
            0.,
            0.,
        ];
        for row in data {
            self.clip_data.push(&row);
        }
        Ok((range, visible))
    }

    fn path_group(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        buffer: &wgpu::Buffer,
        transforms: &wgpu::Buffer,
        clips: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("figleaf vector edges"),
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: transforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: clips.as_entire_binding(),
                },
            ],
        })
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
    /// sets the pipeline, bind groups 0/1 and vertex buffer 0; the host restores
    /// its own afterwards. The scissor rectangle is left as the host set it.
    pub fn draw_items(&self, pass: &mut wgpu::RenderPass<'_>, items: Range<usize>) {
        let end = items.end.min(self.draws.len());
        let mut run: Option<Range<u32>> = None;
        let flush = |pass: &mut wgpu::RenderPass<'_>, run: &mut Option<Range<u32>>| {
            if let Some(range) = run.take() {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &self.view_group, &[]);
                pass.set_bind_group(1, &self.path_group, &[]);
                pass.set_vertex_buffer(0, self.shapes.buffer.slice(..));
                pass.draw(0..6, range);
            }
        };
        for draw in self.draws[items.start.min(end)..end].iter().flatten() {
            match draw {
                Draw::Composite(group, _) => {
                    flush(pass, &mut run);
                    pass.set_pipeline(&self.composite_pipeline);
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(0..3, 0..1);
                }
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
                    pass.set_bind_group(1, &self.text_clip_group, &[]);
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

/// Gaussian blur of a `w` by `h` field; beyond it counts as `edge`.
fn blur(src: &[f32], w: usize, h: usize, sigma: f32, edge: f32) -> Vec<f32> {
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
                        edge
                    };
                    sum += weight * v;
                }
                out[y as usize * w + x as usize] = sum / total;
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

/// The glyphs blurred for a drop shadow, in a mask `pad` pixels larger on
/// every side so the blur has room. Returns the mask, its size and `pad`.
fn drop_shadow(alpha: &[u8], [w, h]: [u32; 2], sigma: f32) -> (Vec<u8>, [u32; 2], u32) {
    let (w, h) = (w as usize, h as usize);
    let pad = if sigma >= 0.25 {
        (3. * sigma).ceil() as usize
    } else {
        0
    };
    let (pw, ph) = (w + 2 * pad, h + 2 * pad);
    let mut field = vec![0f32; pw * ph];
    for y in 0..h {
        for x in 0..w {
            field[(y + pad) * pw + x + pad] = f32::from(alpha[y * w + x]) / 255.;
        }
    }
    if sigma >= 0.25 {
        field = blur(&field, pw, ph, sigma, 0.);
    }
    let out = field.iter().map(|v| (v * 255.).round() as u8).collect();
    (out, [pw as u32, ph as u32], pad as u32)
}

/// Coverage of an inner shadow on glyphs: inside the glyph, wherever the
/// blurred, shifted outside of the glyph reaches. `sigma` and `offset` are in
/// mask pixels; beyond the mask counts as outside.
fn inner_shadow(alpha: &[u8], [w, h]: [u32; 2], offset: [f32; 2], sigma: f32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    // How much of each pixel is outside the glyph.
    let mut outside: Vec<f32> = alpha.iter().map(|a| 1. - f32::from(*a) / 255.).collect();
    if sigma >= 0.25 {
        outside = blur(&outside, w, h, sigma, 1.);
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

    #[test]
    fn a_drop_shadow_mask_grows_by_its_blur_and_keeps_the_ink() {
        let alpha = vec![255u8; 16];
        let (sharp, size, pad) = drop_shadow(&alpha, [4, 4], 0.);
        assert_eq!((sharp, size, pad), (alpha.clone(), [4, 4], 0));
        let (soft, size, pad) = drop_shadow(&alpha, [4, 4], 1.);
        assert_eq!((size, pad), ([10, 10], 3));
        let at = |x: usize, y: usize| soft[y * 10 + x];
        assert!(at(5, 5) > at(3, 5) && at(3, 5) > at(1, 5) && at(1, 5) > 0);
        // Blurring spreads the ink without losing much of it.
        let total: u32 = soft.iter().map(|v| u32::from(*v)).sum();
        assert!(
            (total as f32 - 16. * 255.).abs() < 16. * 255. * 0.05,
            "{total}"
        );
    }
}

use super::*;
use crate::{BlendMode, Composite, Paint, Shape};

pub(super) struct Cached {
    pub snapshot: Composite,
    pub target: [u32; 2],
    pub scale: f32,
    pub group: wgpu::BindGroup,
    pub id: u64,
}
pub(super) fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    present: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("figleaf compositing"),
        source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("figleaf compositing"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: present.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
#[derive(Clone)]
struct Surface {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
struct Engine<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    text: &'a mut dyn Backend,
    report: &'a mut Preparation,
    pipeline: wgpu::RenderPipeline,
    target: [u32; 2],
    scale: f32,
    pixels: u64,
    sample_work: u64,
}
impl Engine<'_> {
    fn surface(&mut self) -> Result<Surface, Error> {
        self.pixels += u64::from(self.target[0]) * u64::from(self.target[1]);
        if self.pixels > 64_000_000 {
            return Err(Error("compositing exceeds 64 million intermediate pixels"));
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("figleaf composite canvas"),
            size: wgpu::Extent3d {
                width: self.target[0],
                height: self.target[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.report.textures_created += 1;
        self.report.changed = true;
        Ok(Surface { texture, view })
    }
    fn group(
        &mut self,
        pipeline: &wgpu::RenderPipeline,
        s: &Surface,
        b: &Surface,
        mask: &Surface,
        values: [f32; 16],
    ) -> wgpu::BindGroup {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("figleaf composite parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&buffer, 0, &values.map(f32::to_le_bytes).concat());
        self.report.uploaded_bytes += 64;
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&s.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&b.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&mask.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: buffer.as_entire_binding(),
                },
            ],
        })
    }
    fn pass(
        &mut self,
        s: &Surface,
        b: &Surface,
        mask: &Surface,
        values: [f32; 16],
    ) -> Result<Surface, Error> {
        let taps = match values[0] as u32 {
            2 => (2 * values[2].abs().ceil() as u64 + 1).pow(2),
            3 => 2 * (values[2] * 3.).ceil() as u64 + 1,
            _ => 1,
        };
        self.sample_work += u64::from(self.target[0]) * u64::from(self.target[1]) * taps;
        if self.sample_work > 256_000_000 {
            return Err(Error("compositing filter work budget exceeded"));
        }
        let out = self.surface()?;
        let pipeline = self.pipeline.clone();
        let group = self.group(&pipeline, s, b, mask, values);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("figleaf composite operation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        Ok(out)
    }
    fn start(&mut self, initial: Option<&Surface>) -> Result<Surface, Error> {
        let out = self.surface()?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(src) = initial {
            encoder.copy_texture_to_texture(
                src.texture.as_image_copy(),
                out.texture.as_image_copy(),
                wgpu::Extent3d {
                    width: self.target[0],
                    height: self.target[1],
                    depth_or_array_layers: 1,
                },
            );
        } else {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit([encoder.finish()]);
        Ok(out)
    }
    fn ordinary(&mut self, canvas: &Canvas, out: &Surface) -> Result<(), Error> {
        if canvas.items().is_empty() {
            return Ok(());
        }
        let mut renderer = Renderer::new(self.device, wgpu::TextureFormat::Rgba8Unorm);
        let report = renderer.prepare(
            self.device,
            self.queue,
            canvas,
            self.target,
            self.scale,
            self.text,
        )?;
        self.report.uploaded_bytes += report.uploaded_bytes;
        self.report.textures_created += report.textures_created;
        if let Some(problem) = renderer.problems().first() {
            return Err(Error(problem.reason));
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.draw(&mut pass);
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
    fn run(
        &mut self,
        canvas: &Canvas,
        initial: Option<&Surface>,
        depth: usize,
    ) -> Result<Surface, Error> {
        if depth > 8 {
            return Err(Error("composite nesting exceeds eight levels"));
        }
        let mut out = self.start(initial)?;
        let mut ordinary = Canvas::new();
        for item in canvas.items() {
            if let Item::Masked(m) = item {
                self.ordinary(&ordinary, &out)?;
                ordinary.clear();
                let mask = self.run(&m.mask.source()?, None, depth + 1)?;
                let content = self.run(&m.content, Some(&out), depth + 1)?;
                let mut p = [0.; 16];
                p[0] = 7.;
                p[1] = f32::from(m.mask.kind == crate::MaskType::Luminance);
                p[2] = m.mask.opacity;
                out = self.pass(&content, &out, &mask, p)?;
                continue;
            }
            let composite = match item {
                Item::Composite(c) => Some(c.clone()),
                Item::Shape(s) if needs_scope(s) => Some(shape_scope(s)?),
                _ => None,
            };
            if let Some(c) = composite {
                self.ordinary(&ordinary, &out)?;
                ordinary.clear();
                out = self.compose(&c, &out, depth + 1)?;
            } else {
                match item {
                    Item::Shape(s) => {
                        ordinary.shape(s.clone());
                    }
                    Item::Text(t) => {
                        ordinary.text(t.clone());
                    }
                    Item::Masked(m) => {
                        ordinary.masked(m.mask.clone(), m.content.clone());
                    }
                    Item::Composite(_) => unreachable!(),
                }
            }
        }
        self.ordinary(&ordinary, &out)?;
        Ok(out)
    }
    fn shadow(
        &mut self,
        source: &Surface,
        backdrop: &Surface,
        c: &Composite,
        index: usize,
    ) -> Result<Surface, Error> {
        let s = c.shadows[index];
        let m = c.effect_transform.0;
        let unit = c.effect_transform.max_scale();
        let scale = self.scale * unit;
        let spread = if s.inner { -s.spread } else { s.spread };
        let mut filtered = source.clone();
        let x = [m[0][0] / unit, m[1][0] / unit];
        let y = [m[0][1] / unit, m[1][1] / unit];
        if spread != 0. {
            let mut p = [0.; 16];
            p[0] = 2.;
            p[2] = spread * scale;
            p[4..8].copy_from_slice(&[x[0], x[1], y[0], y[1]]);
            filtered = self.pass(&filtered, source, source, p)?;
        }
        if s.blur > 0. {
            for direction in [x, y] {
                let mut p = [0.; 16];
                p[0] = 3.;
                p[2] = s.blur * scale / 2.;
                p[4] = direction[0];
                p[5] = direction[1];
                filtered = self.pass(&filtered, source, source, p)?;
            }
        }
        let mut p = [0.; 16];
        p[0] = 4.;
        p[1] = c.shadow_blend_modes.get(index).copied().unwrap_or_default() as u32 as f32;
        p[4] = (m[0][0] * s.offset[0] + m[0][1] * s.offset[1]) * self.scale;
        p[5] = (m[1][0] * s.offset[0] + m[1][1] * s.offset[1]) * self.scale;
        p[8..12].copy_from_slice(&s.color);
        p[12] = f32::from(s.inner);
        p[13] = f32::from(s.behind);
        self.pass(&filtered, backdrop, source, p)
    }
    fn compose(&mut self, c: &Composite, b: &Surface, depth: usize) -> Result<Surface, Error> {
        validate_scope(c, self.scale)?;
        let pass_through = c.blend_mode == BlendMode::PassThrough;
        let source = if !pass_through || !c.shadows.is_empty() {
            Some(self.run(&c.content, None, depth)?)
        } else {
            None
        };
        let mut out = if pass_through {
            b.clone()
        } else {
            self.start(None)?
        };
        for (i, s) in c.shadows.iter().enumerate() {
            if !s.inner {
                out = self.shadow(source.as_ref().expect("shadow source"), &out, c, i)?;
            }
        }
        out = if pass_through {
            self.run(&c.content, Some(&out), depth)?
        } else {
            let src = source.as_ref().expect("isolated source");
            self.pass(
                src,
                &out,
                src,
                [
                    0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,
                ],
            )?
        };
        for (i, s) in c.shadows.iter().enumerate() {
            if s.inner {
                out = self.shadow(source.as_ref().expect("shadow source"), &out, c, i)?;
            }
        }
        if !c.clips.is_empty() {
            let mut shape = Shape::rect(Rect::new(
                0.,
                0.,
                self.target[0] as f32 / self.scale,
                self.target[1] as f32 / self.scale,
            ))
            .fill([1.; 4]);
            shape.clips = c.clips.clone();
            let mut canvas = Canvas::new();
            canvas.shape(shape);
            let mask = self.run(&canvas, None, depth)?;
            let mut p = [0.; 16];
            p[0] = 6.;
            p[1] = f32::from(pass_through);
            out = self.pass(&out, b, &mask, p)?;
        }
        let mut p = [0.; 16];
        p[0] = f32::from(pass_through);
        p[1] = c.blend_mode as u32 as f32;
        p[2] = c.opacity;
        self.pass(&out, b, &out, p)
    }
}
pub(super) fn needs_scope(s: &Shape) -> bool {
    s.opacity != 1.
        || s.fill_blend_modes
            .iter()
            .chain(&s.stroke_blend_modes)
            .any(|m| *m != BlendMode::Normal)
}
pub(super) fn shape_scope(s: &Shape) -> Result<Composite, Error> {
    if let Some(reason) = s.problem() {
        return Err(Error(reason));
    }
    let mut base = s.clone();
    base.opacity = 1.;
    base.fill_blend_modes.clear();
    base.stroke_blend_modes.clear();
    let mut content = Canvas::new();
    if s.fill_blend_modes
        .iter()
        .chain(&s.stroke_blend_modes)
        .all(|m| *m == BlendMode::Normal)
    {
        content.shape(base);
        return Ok(Composite::new(content).opacity(s.opacity));
    }
    base.shadows.clear();
    for (i, paint) in s.fills.iter().enumerate() {
        let mut shape = base.clone();
        shape.fills = vec![paint.clone()];
        shape.stroke = None;
        shape.stroke_paints.clear();
        shape.stroke_weights = None;
        shape.stroke_geometry = None;
        shape.stroke_paths = None;
        shape.dash_pattern.clear();
        shape.path_stroke = Default::default();
        let mut one = Canvas::new();
        one.shape(shape);
        content.composite(
            Composite::new(one).blend_mode(s.fill_blend_modes.get(i).copied().unwrap_or_default()),
        );
    }
    let strokes = if s.stroke_paints.is_empty() {
        s.stroke
            .map(|s| vec![Paint::Solid(s.color)])
            .unwrap_or_default()
    } else {
        s.stroke_paints.clone()
    };
    for (i, paint) in strokes.into_iter().enumerate() {
        let mut shape = base.clone();
        shape.fills.clear();
        shape.stroke_paints = vec![paint];
        let mut one = Canvas::new();
        one.shape(shape);
        content.composite(
            Composite::new(one)
                .blend_mode(s.stroke_blend_modes.get(i).copied().unwrap_or_default()),
        );
    }
    let mut result = Composite::new(content)
        .blend_mode(BlendMode::PassThrough)
        .opacity(s.opacity)
        .effect_transform(s.transform);
    result.shadows = s.shadows.clone();
    result.clips = s.clips.clone();
    if let Some(clip) = s.clip {
        result
            .clips
            .push(crate::Clip::rounded_rect(clip.rect, clip.radii).transform(s.clip_transform));
    }
    Ok(result)
}
fn validate_scope(c: &Composite, scale: f32) -> Result<(), Error> {
    if !c.opacity.is_finite()
        || !(0. ..=1.).contains(&c.opacity)
        || c.shadows.len() > 16
        || c.shadow_blend_modes.len() > c.shadows.len()
        || c.shadow_blend_modes.contains(&BlendMode::PassThrough)
    {
        return Err(Error("invalid composite opacity or effects"));
    }
    if c.clips.len() > 128 {
        return Err(Error("composite clip stack exceeds 128 clips"));
    }
    for clip in &c.clips {
        clip.validate()?;
    }
    c.effect_transform.inverse()?;
    let scale = scale * c.effect_transform.max_scale();
    for s in &c.shadows {
        if s.color
            .iter()
            .chain(&s.offset)
            .chain([&s.blur, &s.spread])
            .any(|v| !v.is_finite())
            || s.blur < 0.
            || s.blur * scale * 1.5 > 128.
            || s.spread.abs() * scale > 32.
            || s.offset.iter().any(|v| v.abs() * scale > 1e7)
        {
            return Err(Error("container shadow exceeds filter limits"));
        }
    }
    Ok(())
}
pub(super) fn validate(
    canvas: &Canvas,
    depth: usize,
    count: &mut usize,
    scale: f32,
) -> Result<(), Error> {
    if depth > 8 {
        return Err(Error("composite nesting exceeds eight levels"));
    }
    for item in canvas.items() {
        match item {
            Item::Composite(c) => {
                *count += 1;
                if *count > 128 {
                    return Err(Error("canvas exceeds 128 composite scopes"));
                }
                validate_scope(c, scale)?;
                validate(&c.content, depth + 1, count, scale)?;
            }
            Item::Shape(s) => {
                if needs_scope(s) {
                    *count += 1;
                    if s.fill_blend_modes
                        .iter()
                        .chain(&s.stroke_blend_modes)
                        .any(|m| *m != BlendMode::Normal)
                    {
                        *count += s.fills.len()
                            + s.stroke_paints.len().max(usize::from(s.stroke.is_some()));
                    }
                    if *count > 128 {
                        return Err(Error("canvas exceeds 128 composite scopes"));
                    }
                }
                for paint in s.fills.iter().chain(&s.stroke_paints) {
                    if let Paint::Pattern(p) = paint {
                        validate(&p.source, depth + 1, count, scale)?;
                    }
                }
            }
            Item::Masked(m) => {
                validate(&m.mask.canvas, depth + 1, count, scale)?;
                validate(&m.content, depth + 1, count, scale)?;
            }
            _ => {}
        }
    }
    Ok(())
}
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_scope(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        c: &Composite,
        index: usize,
        target: [u32; 2],
        scale: f32,
        text: &mut dyn Backend,
        report: &mut Preparation,
    ) -> Result<(), Error> {
        if c.blend_mode != BlendMode::Normal {
            return Err(Error(
                "root blend requires an enclosing isolated composite canvas",
            ));
        }
        if let Some(cached) = self.scope_cache.get(&index)
            && cached.snapshot == *c
            && cached.target == target
            && cached.scale == scale
        {
            self.draws[index].push(Draw::Scope(cached.group.clone(), cached.id));
            return Ok(());
        }
        let signature = format!("{c:?}:{target:?}:{scale}");
        if let Some((old, reason)) = self.failed_scopes.get(&index)
            && old == &signature
        {
            return Err(Error(reason));
        }
        let result = (|| {
            validate_scope(c, scale)?;
            validate(&c.content, 0, &mut 0, scale)?;
            masked::validate(&c.content, 0, &mut 0)?;
            if target
                .iter()
                .any(|v| *v > device.limits().max_texture_dimension_2d)
            {
                return Err(Error("composite target exceeds texture limits"));
            }
            let mut engine = Engine {
                device,
                queue,
                text,
                report,
                pipeline: pipeline(device, wgpu::TextureFormat::Rgba8Unorm, false),
                target,
                scale,
                pixels: 0,
                sample_work: 0,
            };
            let clear = engine.start(None)?;
            let out = engine.compose(c, &clear, 0)?;
            let mut p = [0.; 16];
            p[0] = 5.;
            p[1] = f32::from(self.linear);
            Ok(engine.group(&self.scope_pipeline, &out, &out, &out, p))
        })();
        match result {
            Ok(group) => {
                self.next_mask += 1;
                self.failed_scopes.remove(&index);
                self.scope_cache.insert(
                    index,
                    Cached {
                        snapshot: c.clone(),
                        target,
                        scale,
                        group: group.clone(),
                        id: self.next_mask,
                    },
                );
                self.draws[index].push(Draw::Scope(group, self.next_mask));
                Ok(())
            }
            Err(e) => {
                self.failed_scopes.insert(index, (signature, e.0));
                Err(e)
            }
        }
    }
}

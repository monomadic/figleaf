use super::*;
use crate::{MaskType, Masked};

// Failed inputs can contain NaNs, whose PartialEq is never reflexive.
// Retain their diagnostic representation so repeated invalid canvases do no work.
pub(super) struct Failure {
    signature: String,
    target: [u32; 2],
    scale: f32,
    reason: &'static str,
}

pub(super) struct Composite {
    snapshot: Masked,
    target: [u32; 2],
    scale: f32,
    pub group: wgpu::BindGroup,
    pub id: u64,
}

pub(super) fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("figleaf mask composite"),
        source: wgpu::ShaderSource::Wgsl(include_str!("masked.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("figleaf mask composite"),
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
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

// Bound the entire recursive operation before allocating any targets.
pub(super) fn validate(canvas: &Canvas, depth: usize, count: &mut usize) -> Result<(), Error> {
    if depth > 8 {
        return Err(Error("mask nesting exceeds eight levels"));
    }
    for item in canvas.items() {
        if let Item::Shape(s) = item {
            for paint in s.fills.iter().chain(&s.stroke_paints) {
                if let crate::Paint::Pattern(p) = paint {
                    *count += 1;
                    if *count > 32 {
                        return Err(Error("canvas exceeds 32 mask/pattern scopes"));
                    }
                    validate(&p.source, depth + 1, count)?;
                }
            }
        }
        if let Item::Masked(m) = item {
            *count += 1;
            if *count > 32 {
                return Err(Error("canvas exceeds 32 mask scopes"));
            }
            validate(&m.mask.canvas, depth + 1, count)?;
            validate(&m.content, depth + 1, count)?;
        }
    }
    Ok(())
}
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_masked(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        item: &Masked,
        index: usize,
        target: [u32; 2],
        scale: f32,
        text: &mut dyn Backend,
        report: &mut Preparation,
    ) -> Result<(), Error> {
        if self
            .composites
            .get(&index)
            .is_some_and(|c| c.snapshot == *item && c.target == target && c.scale == scale)
        {
            let c = &self.composites[&index];
            self.draws[index].push(Draw::Composite(c.group.clone(), c.id));
            return Ok(());
        }
        if let Some(f) = self.failed_composites.get(&index)
            && f.signature == format!("{item:?}")
            && f.target == target
            && f.scale == scale
        {
            return Err(Error(f.reason));
        }
        self.failed_composites.remove(&index);
        let source = item.mask.source()?;
        let mut content_renderer = Renderer::new(device, self.format);
        let mut mask_renderer = Renderer::new(device, wgpu::TextureFormat::Rgba8Unorm);
        for (renderer, canvas) in [
            (&mut content_renderer, &item.content),
            (&mut mask_renderer, &source),
        ] {
            let r = renderer.prepare(device, queue, canvas, target, scale, text)?;
            report.changed |= r.changed;
            report.uploaded_bytes += r.uploaded_bytes;
            report.textures_created += r.textures_created;
            if let Some(problem) = renderer.problems().first() {
                self.failed_composites.insert(
                    index,
                    Failure {
                        signature: format!("{item:?}"),
                        target,
                        scale,
                        reason: problem.reason,
                    },
                );
                return Err(Error(problem.reason));
            }
        }
        let views = [self.format, wgpu::TextureFormat::Rgba8Unorm].map(|format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("figleaf mask scope"),
                    size: wgpu::Extent3d {
                        width: target[0],
                        height: target[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        for (renderer, view) in [(&content_renderer, &views[0]), (&mask_renderer, &views[1])] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("figleaf masked paint"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.draw(&mut pass);
        }
        queue.submit([encoder.finish()]);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("figleaf mask parameters"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let values = [
            f32::from(item.mask.kind == MaskType::Luminance),
            item.mask.opacity,
            0.,
            0.,
        ];
        queue.write_buffer(&params, 0, &values.map(f32::to_le_bytes).concat());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("figleaf mask composite"),
            layout: &self.composite_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        self.next_mask += 1;
        self.draws[index].push(Draw::Composite(group.clone(), self.next_mask));
        self.composites.insert(
            index,
            Composite {
                snapshot: item.clone(),
                target,
                scale,
                group,
                id: self.next_mask,
            },
        );
        report.changed = true;
        report.textures_created += 2;
        report.uploaded_bytes += 16;
        Ok(())
    }
}

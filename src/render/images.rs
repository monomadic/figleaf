use super::*;
use crate::image::TexturePaint;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Source {
    Pixels(crate::Image),
    Pattern(Canvas, [u32; 2], f32),
}
pub(super) struct Slot {
    pub source: Source,
    pub group: wgpu::BindGroup,
    pub id: u64,
    pub used: bool,
}
pub(super) fn group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("figleaf image"),
        layout: &pipeline.get_bind_group_layout(2),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}
pub(super) fn texture(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("figleaf image paint"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        paint: TexturePaint<'_>,
        scale: f32,
        text: &mut dyn Backend,
        report: &mut Preparation,
    ) -> Result<(wgpu::BindGroup, u64), Error> {
        let (source, size) = match paint {
            TexturePaint::Image(p) => (Source::Pixels(p.image.clone()), p.image.size),
            TexturePaint::Pattern(p) => {
                let scale = scale * p.scaling_factor;
                let size = p.size.map(|v| (v * scale).ceil() as u32);
                (Source::Pattern(p.source.clone(), size, scale), size)
            }
        };
        if size.contains(&0)
            || size
                .iter()
                .any(|v| *v > device.limits().max_texture_dimension_2d)
            || u64::from(size[0]) * u64::from(size[1]) > 16_000_000
        {
            return Err(Error("image/pattern texture size limit exceeded"));
        }
        if let Some(slot) = self.images.iter_mut().find(|s| s.source == source) {
            slot.used = true;
            return Ok((slot.group.clone(), slot.id));
        }
        let signature = matches!(source, Source::Pattern(..)).then(|| format!("{source:?}"));
        if let Some(failure) = signature
            .as_ref()
            .and_then(|key| self.failed_images.get_mut(key))
        {
            failure.1 = true;
            return Err(Error(failure.0));
        }
        let used_pixels: u64 = self
            .images
            .iter()
            .filter(|s| s.used)
            .map(|s| match &s.source {
                Source::Pixels(i) => i.size,
                Source::Pattern(_, size, _) => *size,
            })
            .map(|s| u64::from(s[0]) * u64::from(s[1]))
            .sum();
        if used_pixels + u64::from(size[0]) * u64::from(size[1]) > 32_000_000 {
            return Err(Error("image/pattern pixel budget exceeded"));
        }
        let texture = images::texture(device, size);
        let view = texture.create_view(&Default::default());
        report.textures_created += 1;
        report.changed = true;
        match &source {
            Source::Pixels(image) => {
                let mut pixels = image.rgba.to_vec();
                for p in pixels.as_chunks_mut::<4>().0 {
                    let a = u16::from(p[3]);
                    for c in &mut p[..3] {
                        *c = ((u16::from(*c) * a + 127) / 255) as u8;
                    }
                }
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size[0] * 4),
                        rows_per_image: Some(size[1]),
                    },
                    wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                );
                report.uploaded_bytes += pixels.len() as u64;
            }
            Source::Pattern(canvas, _, scale) => {
                let mut renderer = Renderer::new(device, wgpu::TextureFormat::Rgba8Unorm);
                let prepared = renderer.prepare(device, queue, canvas, size, *scale, text)?;
                report.uploaded_bytes += prepared.uploaded_bytes;
                report.textures_created += prepared.textures_created;
                if let Some(problem) = renderer.problems().first() {
                    self.failed_images.insert(
                        signature.expect("pattern signature"),
                        (problem.reason, true),
                    );
                    return Err(Error(problem.reason));
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("figleaf pattern tile"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
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
            }
        }
        let group = images::group(device, &self.shape_pipeline, &view);
        self.next_mask += 1;
        let id = self.next_mask;
        self.images.push(Slot {
            source,
            group: group.clone(),
            id,
            used: true,
        });
        Ok((group, id))
    }
}

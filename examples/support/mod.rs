//! Headless rendering for the example and the tests: draw a canvas into an
//! offscreen `Rgba8Unorm` texture and read the pixels back.
#![allow(dead_code)]
use figleaf::{Canvas, render::Renderer, text::Backend};
use std::{sync::mpsc, time::Duration};

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// Format of the offscreen target; `Rgba8Unorm` unless changed.
    pub format: wgpu::TextureFormat,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba[i..i + 4].try_into().unwrap()
    }
}

impl Gpu {
    pub fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        Self {
            device,
            queue,
            format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }

    /// Prepares and draws `canvas` over `clear` (straight alpha, encoded sRGB).
    pub fn render(
        &self,
        renderer: &mut Renderer,
        canvas: &Canvas,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
        text: &mut dyn Backend,
    ) -> Image {
        let all = 0..canvas.items().len();
        self.render_items(renderer, canvas, size, scale, clear, text, all)
    }

    /// `render`, drawing only the canvas items in `items`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_items(
        &self,
        renderer: &mut Renderer,
        canvas: &Canvas,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
        text: &mut dyn Backend,
        items: std::ops::Range<usize>,
    ) -> Image {
        let [width, height] = size;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        renderer
            .prepare(&self.device, &self.queue, canvas, size, scale, text)
            .unwrap();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0] * clear[3],
                            g: clear[1] * clear[3],
                            b: clear[2] * clear[3],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.draw_items(&mut pass, items);
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(stride * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let rgba = mapped
            .chunks_exact(stride as usize)
            .flat_map(|r| r[..width as usize * 4].iter().copied())
            .collect();
        drop(mapped);
        readback.unmap();
        Image {
            width,
            height,
            rgba,
        }
    }
}

pub fn load(path: &str) -> Option<Image> {
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    Some(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

pub fn save(path: &str, image: &Image) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&image.rgba).unwrap();
}

/// Writes a Figma export (left) beside our render (right), both enlarged
/// `zoom` times, and prints how far apart they are. The export may be
/// transparent; it is laid over `background` the way Figma's canvas would.
pub fn compare(figma: &Image, ours: &Image, background: [u8; 3], zoom: u32, path: &str) {
    assert_eq!(
        [figma.width, figma.height],
        [ours.width, ours.height],
        "the render must be the size of the Figma export"
    );
    let (w, h, gap) = (figma.width, figma.height, 8);
    let width = (w * 2 + gap) * zoom;
    let mut rgba = vec![255u8; (width * h * zoom * 4) as usize];
    let (mut worst, mut worst_at, mut total) = (0u32, [0u32; 2], 0u64);
    for y in 0..h {
        for x in 0..w {
            let over = |p: [u8; 4]| {
                let a = f32::from(p[3]) / 255.;
                let mix = |i: usize| {
                    (f32::from(p[i]) * a + f32::from(background[i]) * (1. - a)).round() as u8
                };
                [mix(0), mix(1), mix(2), 255]
            };
            let (a, b) = (over(figma.pixel(x, y)), ours.pixel(x, y));
            let d = (0..3)
                .map(|i| u32::from(a[i].abs_diff(b[i])))
                .max()
                .unwrap();
            if d > worst {
                (worst, worst_at) = (d, [x, y]);
            }
            total += u64::from(d);
            for (image, pixel) in [a, b].into_iter().enumerate() {
                let x0 = (image as u32 * (w + gap) + x) * zoom;
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let i = (((y * zoom + dy) * width + x0 + dx) * 4) as usize;
                        rgba[i..i + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
    }
    let image = Image {
        width,
        height: h * zoom,
        rgba,
    };
    save(path, &image);
    let mean = total as f64 / f64::from(w * h);
    println!("{path}: largest channel difference {worst}/255 at {worst_at:?}, mean {mean:.2}/255");
}

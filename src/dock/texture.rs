use crate::shared::{SharedFrames, Source};
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;

// Keep two texels per design pixel so high-DPI displays can show extra detail.
const WIDTH: u32 = 600;
const HEIGHT: u32 = 192;
const FADE: Duration = Duration::from_millis(320);
const SEED_DELAY: Duration = Duration::from_millis(200);

const SIZE: wgpu::Extent3d = wgpu::Extent3d {
    width: WIDTH,
    height: HEIGHT,
    depth_or_array_layers: 1,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Paused,
    Main,
    Remote,
}

#[derive(Clone, Copy)]
pub enum Sample {
    Main([f32; 4]),
    Remote(u64),
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub sample: Option<Sample>,
    pub previous_weight: f32,
    start_fade: bool,
    fade_elapsed: Option<Duration>,
    playing: bool,
    now: Instant,
}

pub struct Preview {
    pub view: wgpu::TextureView,
    pub previous_view: wgpu::TextureView,
    pub presented_remote: bool,
    current: wgpu::Texture,
    previous: wgpu::Texture,
    size: wgpu::Extent3d,
    first_main: Option<Instant>,
    fade_elapsed: Option<Duration>,
    fade_tick: Option<Instant>,
    crossfade: bool,
    pipeline: wgpu::RenderPipeline,
    crop: wgpu::Buffer,
    main: wgpu::BindGroup,
    remote: Option<(wgpu::BindGroup, SharedFrames, [u32; 2])>,
    last: Option<Sample>,
}

impl Preview {
    pub fn new(device: &wgpu::Device, sampler: &wgpu::Sampler, source: &wgpu::TextureView) -> Self {
        Self::with_size(device, sampler, source, SIZE, None)
    }

    pub fn catalog(&self, device: &wgpu::Device, sampler: &wgpu::Sampler, source: &wgpu::TextureView) -> Self {
        let size = wgpu::Extent3d { width: 464, height: 320, depth_or_array_layers: 1 };
        Self::with_size(device, sampler, source, size, Some(&self.pipeline))
    }

    fn with_size(device: &wgpu::Device, sampler: &wgpu::Sampler, source: &wgpu::TextureView, size: wgpu::Extent3d, pipeline: Option<&wgpu::RenderPipeline>) -> Self {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = |label, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | usage,
                view_formats: &[],
            })
        };
        let current = texture(
            "Retained Wallpapers card",
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let previous = texture("Outgoing Wallpapers card", wgpu::TextureUsages::COPY_DST);
        let view = current.create_view(&Default::default());
        let previous_view = previous.create_view(&Default::default());
        let pipeline = pipeline.cloned().unwrap_or_else(|| {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wallpapers thumbnail crop"),
            source: wgpu::ShaderSource::Wgsl(include_str!("preview.wgsl").into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wallpapers thumbnail crop"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("snapshot"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })});
        let crop = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Wallpapers thumbnail UVs"),
            contents: &[0; 32],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let main = Self::binding(device, &pipeline, &crop, sampler, source);
        Self {
            view,
            previous_view,
            current,
            previous,
            size,
            first_main: None,
            fade_elapsed: None,
            fade_tick: None,
            crossfade: false,
            presented_remote: false,
            pipeline,
            crop,
            main,
            remote: None,
            last: None,
        }
    }

    fn binding(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        crop: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wallpapers thumbnail source"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: crop.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn bind_main(
        &mut self,
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        source: &wgpu::TextureView,
    ) {
        self.main = Self::binding(device, &self.pipeline, &self.crop, sampler, source);
    }

    pub fn attach(
        &mut self,
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        source: Source,
        fade: bool,
    ) -> super::Result<()> {
        let (texture, frames) = source.open(device)?;
        let binding = Self::binding(
            device,
            &self.pipeline,
            &self.crop,
            sampler,
            &texture.create_view(&Default::default()),
        );
        self.remote = Some((binding, frames, source.size));
        self.presented_remote = false;
        self.crossfade = fade;
        Ok(())
    }

    pub fn remote_ready(&self) -> bool {
        self.remote
            .as_ref()
            .is_some_and(|(_, frames, _)| frames.has_new_frame().unwrap_or(false))
    }

    pub fn wake_for_frame(&self, wake: &crate::bridge_signal::FrameWake) {
        if let Some((_, frames, _)) = &self.remote {
            frames.wake_for_next(wake.handle());
        }
    }

    pub fn clear_remote(&mut self) {
        self.remote = None;
        self.presented_remote = false;
        self.crossfade = false;
    }

    pub fn seed_due(&self, now: Instant) -> bool {
        self.seed_deadline().is_some_and(|at| now >= at)
    }

    pub fn has_thumbnail(&self) -> bool {
        self.last.is_some()
    }

    pub fn seed_deadline(&self) -> Option<Instant> {
        self.first_main.filter(|_| self.last.is_none()).map(|at| at + SEED_DELAY)
    }

    pub fn animating(&self) -> bool {
        self.fade_elapsed.is_some()
    }

    pub fn pause(&mut self) {
        // Keep the last presented blend amount even if the pointer moves
        // away between two UI refreshes.
        self.fade_tick = None;
    }

    pub fn frame(
        &self,
        now: Instant,
        playback: Playback,
        main_new: bool,
        rect: [f32; 4],
        can_seed: bool,
    ) -> Frame {
        let sample = self.sample(playback, main_new, rect, can_seed);
        let playing = playback == Playback::Remote;
        let start_fade = self.crossfade
            && self.last.is_some()
            && !self.presented_remote
            && matches!(sample, Some(Sample::Remote(_)));
        let fade_elapsed = if start_fade {
            Some(Duration::ZERO)
        } else {
            self.fade_elapsed.map(|elapsed| {
                elapsed
                    + self
                        .fade_tick
                        .filter(|_| playing)
                        .map_or(Duration::ZERO, |tick| now - tick)
            })
        };
        let previous_weight = if self.last.is_none() && sample.is_none() {
            -1.0 // No ready frame: leave the dock's own plate visible.
        } else {
            let t = fade_elapsed.map_or(1.0, |elapsed| {
                (elapsed.as_secs_f32() / FADE.as_secs_f32()).min(1.0)
            });
            1.0 - t * t * (3.0 - 2.0 * t)
        };
        Frame {
            sample,
            previous_weight,
            start_fade,
            fade_elapsed,
            playing,
            now,
        }
    }

    fn sample(
        &self,
        playback: Playback,
        main_new: bool,
        rect: [f32; 4],
        can_seed: bool,
    ) -> Option<Sample> {
        if playback == Playback::Remote {
            let (_, frames, _) = self.remote.as_ref()?;
            return frames
                .frame()
                .ok()
                .flatten()
                .filter(|&serial| frames.is_new(serial))
                .map(Sample::Remote);
        }
        match self.last {
            // The first decoded frames can be a clip's black lead-in. Seed
            // once after the main source has played through the loading cover.
            None if can_seed => Some(Sample::Main(rect)),
            Some(last)
                if playback == Playback::Main
                    && (main_new
                        || !matches!(last, Sample::Main(previous) if previous == rect)) =>
            {
                Some(Sample::Main(rect))
            }
            _ => None,
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        frame: Frame,
        size: [u32; 2],
        linear: bool,
    ) {
        let Some(sample) = frame.sample else { return };
        if frame.start_fade {
            encoder.copy_texture_to_texture(
                self.current.as_image_copy(),
                self.previous.as_image_copy(),
                self.size,
            );
        }
        let (binding, rect, size, linear) = match sample {
            Sample::Main(rect) => (&self.main, rect, size, linear),
            Sample::Remote(_) => {
                let (binding, _, size) = self.remote.as_ref().unwrap();
                (
                    binding,
                    [0.0, 0.0, size[0] as f32, size[1] as f32],
                    *size,
                    false,
                )
            }
        };
        let [x, y, w, h] = rect;
        let (tw, th) = (size[0] as f32, size[1] as f32);
        let (width, height) = (self.size.width as f32, self.size.height as f32);
        let ratio = width / height * h / w;
        let (cw, ch) = (w * ratio.min(1.0), h * ratio.recip().min(1.0));
        let params = [
            (x + (w - cw) * 0.5) / tw,
            (y + (h - ch) * 0.5) / th,
            cw / (tw * width),
            ch / (th * height),
            f32::from(linear),
            0.0,
            0.0,
            0.0,
        ];
        queue.write_buffer(&self.crop, 0, bytemuck::cast_slice(&params));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Save Wallpapers thumbnail"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, binding, &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn presented(&mut self, frame: Frame) {
        let now = Instant::now();
        self.first_main.get_or_insert(now);
        self.fade_elapsed = frame.fade_elapsed.filter(|&elapsed| elapsed < FADE);
        self.fade_tick = (frame.playing && self.fade_elapsed.is_some())
            .then_some(if frame.start_fade { now } else { frame.now });
        if let Some(sample) = frame.sample {
            self.last = Some(sample);
            if let Sample::Remote(serial) = sample
                && let Some((_, frames, _)) = &self.remote
            {
                self.presented_remote = true;
                let _ = frames.release(serial);
            }
        }
    }
}

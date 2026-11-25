use super::tube;
use crate::{
    bridge_signal::FrameWake,
    shared::{SharedFrames, Source},
};
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct CoverTexture {
    source: wgpu::Texture,
    cached: wgpu::Texture,
    frames: SharedFrames,
    pub(super) id: egui::TextureId,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub struct PaintGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    renderer: Option<egui_wgpu::Renderer>,
    jobs: Vec<egui::ClippedPrimitive>,
    model: Option<egui::TextureId>,
    model_bounds: egui::Rect,
    dragging_model: bool,
    local_jobs: Vec<egui::ClippedPrimitive>,
    uploaded_region: Option<[u32; 4]>,
    pixels_per_point: f32,
    stamp: u64,
    ui_stamp: u64,
    crt_atlas: wgpu::TextureView,
    crt_sampler: wgpu::Sampler,
    tube_light: Option<tube::Paint>,
    tube_painter: Option<tube::Painter>,
    pub(super) cover: Option<CoverTexture>,
    pub(super) collection_art: Option<crate::home::CollectionArt>,
    frame_to_release: Option<u64>,
}
impl PaintGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        crt_atlas: &wgpu::TextureView,
        crt_sampler: &wgpu::Sampler,
    ) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            renderer: None,
            jobs: Vec::new(),
            model: None,
            model_bounds: egui::Rect::NOTHING,
            dragging_model: false,
            local_jobs: Vec::new(),
            uploaded_region: None,
            pixels_per_point: 1.,
            stamp: 0,
            ui_stamp: 0,
            crt_atlas: crt_atlas.clone(),
            crt_sampler: crt_sampler.clone(),
            tube_light: None,
            tube_painter: None,
            cover: None,
            collection_art: None,
            frame_to_release: None,
        }
    }
    pub fn stamp(&self) -> u64 {
        self.stamp
    }
    pub fn ui_stamp(&self) -> u64 {
        self.ui_stamp
    }
    pub fn dragging_model(&self) -> bool {
        self.dragging_model
    }
    pub(super) fn set_dragging_model(&mut self, dragging: bool) {
        self.dragging_model = dragging;
    }
    fn renderer(&mut self) -> &mut egui_wgpu::Renderer {
        self.renderer.get_or_insert_with(|| {
            egui_wgpu::Renderer::new(&self.device, self.format, Default::default())
        })
    }

    pub fn register_collection_art(&mut self, view: &wgpu::TextureView, size: [u32; 2]) {
        if let Some(old) = self.collection_art.take() {
            self.renderer().free_texture(&old.id);
        }
        let device = self.device.clone();
        let id = self.renderer().register_native_texture(&device, view, wgpu::FilterMode::Nearest);
        self.collection_art = Some(crate::home::CollectionArt { id, size });
    }
    pub(super) fn attach_cover(&mut self, source: Source) -> Result<()> {
        self.clear_cover();
        let [width, height] = source.size;
        let (texture, frames) = source.open(&self.device)?;
        let cached = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cached profile cover frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: source.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = cached.create_view(&Default::default());
        let device = self.device.clone();
        let id = self
            .renderer()
            .register_native_texture(&device, &view, wgpu::FilterMode::Linear);
        self.cover = Some(CoverTexture {
            source: texture,
            cached,
            frames,
            id,
            width,
            height,
        });
        Ok(())
    }
    pub fn cover_ready(&self) -> bool {
        self.cover
            .as_ref()
            .is_some_and(|c| c.frames.has_new_frame().unwrap_or(false))
    }
    pub fn wake_for_cover(&self, wake: &FrameWake) {
        if let Some(c) = &self.cover {
            c.frames.wake_for_next(wake.handle());
        }
    }
    pub(super) fn clear_cover(&mut self) {
        if let Some(c) = self.cover.take() {
            self.renderer().free_texture(&c.id);
            self.frame_to_release = None;
        }
    }
    pub(super) fn update(
        &mut self,
        jobs: Vec<egui::ClippedPrimitive>,
        textures: egui::TexturesDelta,
        scale: f32,
        model: Option<egui::TextureId>,
    ) {
        use egui::epaint::Primitive::Mesh;
        let mesh_changed = model != self.model
            || scale != self.pixels_per_point
            || self.jobs.len() != jobs.len()
            || self.jobs.iter().zip(&jobs).any(|(old, new)| {
                old.clip_rect != new.clip_rect
                    || !matches!((&old.primitive, &new.primitive), (Mesh(a), Mesh(b)) if a == b)
            });
        if !mesh_changed && textures.is_empty() {
            return;
        }
        if mesh_changed
            || textures.set.iter().any(|(id, _)| Some(*id) != model)
            || !textures.free.is_empty()
        {
            self.ui_stamp = self.ui_stamp.wrapping_add(1);
        }
        let (device, queue) = (self.device.clone(), self.queue.clone());
        let renderer = self.renderer();
        for (id, data) in textures.set {
            renderer.update_texture(&device, &queue, id, &data);
        }
        for id in textures.free {
            renderer.free_texture(&id);
        }
        if mesh_changed {
            self.model = model;
            self.model_bounds = jobs.iter().fold(egui::Rect::NOTHING, |bounds, job| {
                if let Mesh(mesh) = &job.primitive
                    && Some(mesh.texture_id) == model
                {
                    let clipped = mesh.calc_bounds().intersect(job.clip_rect);
                    if clipped.is_positive() {
                        return bounds.union(clipped);
                    }
                }
                bounds
            });
            self.jobs = jobs;
            self.pixels_per_point = scale;
            self.uploaded_region = None;
        }
        self.stamp = self.stamp.wrapping_add(1);
    }
    pub(super) fn set_tube(&mut self, paint: Option<tube::Paint>) {
        if paint.is_some() {
            // Prepare before the first present starts the power-on clock.
            self.tube_painter.get_or_insert_with(|| {
                tube::Painter::new(&self.device, self.format, &self.crt_atlas, &self.crt_sampler)
            });
        }
        let paint = paint.filter(|p| p.has_light());
        if self.tube_light.is_some() || paint.is_some() {
            self.stamp = self.stamp.wrapping_add(1);
            self.ui_stamp = self.ui_stamp.wrapping_add(1);
        }
        self.tube_light = paint;
    }

    pub(super) fn clear(&mut self) {
        self.set_tube(None);
        self.dragging_model = false;
        if !self.jobs.is_empty() {
            self.jobs.clear();
            self.model = None;
            self.model_bounds = egui::Rect::NOTHING;
            self.local_jobs.clear();
            self.uploaded_region = None;
            self.stamp = self.stamp.wrapping_add(1);
            self.ui_stamp = self.ui_stamp.wrapping_add(1);
        }
    }
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture) {
        self.encode_region(encoder, target, [0, 0], wgpu::LoadOp::Load);
        if let (Some(light), Some(painter)) = (self.tube_light, &self.tube_painter) {
            painter.encode(encoder, &self.queue, target, light);
        }
    }

    pub fn model_region(&self, width: u32, height: u32) -> Option<[u32; 4]> {
        let bounds = self.model_bounds;
        if !bounds.is_positive() {
            return None;
        }
        // The image sits inside the opaque preview gradient. Match egui's
        // pixel scissor so this visual only replaces pixels of that preview.
        let px = |n: f32| (n * self.pixels_per_point).round() as u32;
        let (x, y) = (px(bounds.left()), px(bounds.top()));
        let w = px(bounds.right()).min(width).saturating_sub(x);
        let h = px(bounds.bottom()).min(height).saturating_sub(y);
        (w > 0 && h > 0).then_some([x, y, w, h])
    }

    pub fn encode_region(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        origin: [u32; 2],
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        if self.jobs.is_empty() {
            return;
        }
        if let Some(c) = &self.cover
            && let Ok(Some(frame)) = c.frames.frame()
            && c.frames.is_new(frame)
        {
            encoder.copy_texture_to_texture(
                c.source.as_image_copy(),
                c.cached.as_image_copy(),
                c.cached.size(),
            );
            self.frame_to_release = Some(frame);
        }
        let renderer = self.renderer.as_mut().unwrap();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [target.width(), target.height()],
            pixels_per_point: self.pixels_per_point,
        };
        let region = [origin[0], origin[1], target.width(), target.height()];
        let upload = self.uploaded_region != Some(region);
        let jobs = if origin == [0, 0] {
            &self.jobs
        } else {
            if upload {
                let shift = egui::vec2(-(origin[0] as f32), -(origin[1] as f32))
                    / self.pixels_per_point;
                let visible = egui::Rect::from_min_size(
                    egui::pos2(-shift.x, -shift.y),
                    egui::vec2(target.width() as f32, target.height() as f32)
                        / self.pixels_per_point,
                );
                self.local_jobs.clear();
                self.local_jobs.extend(self.jobs.iter().filter(|job| {
                    let clip = job.clip_rect.intersect(visible);
                    clip.is_positive()
                        && match &job.primitive {
                            egui::epaint::Primitive::Mesh(mesh) => mesh.calc_bounds().intersects(clip),
                            egui::epaint::Primitive::Callback(_) => false,
                        }
                }).cloned());
                for job in &mut self.local_jobs {
                    job.clip_rect = job.clip_rect.translate(shift);
                    if let egui::epaint::Primitive::Mesh(mesh) = &mut job.primitive {
                        mesh.translate(shift);
                    }
                }
            }
            &self.local_jobs
        };
        if upload {
            let callbacks =
                renderer.update_buffers(&self.device, &self.queue, encoder, jobs, &screen);
            debug_assert!(callbacks.is_empty(), "profile shapes contain no paint callbacks");
            self.uploaded_region = Some(region);
        }
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.format),
            ..Default::default()
        });
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Ika profile"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, jobs, &screen);
    }
    pub fn presented(&mut self) -> Result<()> {
        if let Some(serial) = self.frame_to_release.take()
            && let Some(c) = &self.cover
        {
            c.frames.release(serial)?;
        }
        Ok(())
    }
}

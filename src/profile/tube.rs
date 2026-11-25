use crate::crt::Layers;
use bytemuck::{Pod, Zeroable};

pub(super) fn transform(panel: &mut [egui::ClippedPrimitive], center: egui::Pos2, state: [f32; 4]) {
    let scale = egui::vec2(state[0], state[1]);
    let transform = |p: egui::Pos2| center + (p - center) * scale;
    for job in panel {
        job.clip_rect =
            egui::Rect::from_min_max(transform(job.clip_rect.min), transform(job.clip_rect.max));
        if let egui::epaint::Primitive::Mesh(mesh) = &mut job.primitive {
            for vertex in &mut mesh.vertices {
                vertex.pos = transform(vertex.pos);
                if state[2] < 1.0 {
                    vertex.color = vertex.color.gamma_multiply(state[2]);
                }
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Paint {
    area: [f32; 4],
    screen: [f32; 4],
    layers: Layers,
}

impl Paint {
    pub fn new(area: egui::Rect, dpi: f32, layers: Layers) -> Self {
        Self {
            area: [area.left(), area.top(), area.width(), area.height()],
            screen: [dpi, 0., 0., 0.],
            layers,
        }
    }
    pub fn has_light(self) -> bool {
        let l = self.layers;
        l.flash[2] > 0.0 || l.flash[3] > 0.0 || l.beam[2] > 0.0 || l.dot[2] > 0.0
    }
}

pub(super) struct Painter {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bindings: wgpu::BindGroup,
}

impl Painter {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        atlas: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Preferences CRT"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("tube.wgsl"), "\n", include_str!("../crt.wgsl")).into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Preferences CRT"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Preferences CRT frame"),
            size: std::mem::size_of::<Paint>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Preferences CRT resources"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(atlas),
                },
            ],
        });
        Self {
            format,
            pipeline,
            uniform,
            bindings,
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        paint: Paint,
    ) {
        let [x, y, w, h] = paint.area;
        let dpi = paint.screen[0];
        // Match the same glow margin used by the Wallpapers compositor.
        let left = ((x - 110.) * dpi).floor() as u32;
        let top = ((y - 110.) * dpi).floor() as u32;
        let right = (((x + w + 110.) * dpi).ceil() as u32).min(target.width());
        let bottom = (((y + h + 110.) * dpi).ceil() as u32).min(target.height());
        if left >= right || top >= bottom {
            return;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&paint));
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.format),
            ..Default::default()
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Preferences CRT light"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings, &[]);
        pass.set_scissor_rect(left, top, right - left, bottom - top);
        pass.draw(0..3, 0..1);
    }
}

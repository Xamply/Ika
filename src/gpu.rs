use crate::crt::Layers;
use crate::frame_cache::CachedFrame;
use crate::scene::Scene;
use crate::shared::{SharedFrames, Source};
use crate::startup::Paint;
use crate::wallpapers::{PosterKey, ROWS};
use bytemuck::{Pod, Zeroable};
use image::ImageDecoder;
use std::{
    cell::Cell,
    error::Error,
    fs::File,
    hash::{DefaultHasher, Hasher},
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu::util::{DeviceExt, TextureDataOrder};
use winit::{dpi::PhysicalSize, window::Window};

#[path = "shader_source.rs"]
mod shader_source;
#[path = "video_planes.rs"]
mod video_planes;
#[path = "dock/texture.rs"]
pub mod dock_preview;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Params {
    pub screen: [f32; 4],
    pub frame: [f32; 4],
    pub button: [f32; 4],
    pub glass: [f32; 4],
    pub state: [f32; 4],
    pub glyphs: [[u32; 4]; 8],
    pub motion: [[f32; 4]; 8],
    pub next_button: [f32; 4],
    pub next_state: [f32; 4],
    pub next_glyphs: [[u32; 4]; 8],
    pub title_phase: [f32; 4],
    pub title_nav: [f32; 4],
    pub title_tabs: [[f32; 4]; 5],
    pub title_tints: [[f32; 4]; 2],
    pub title_glows: [[f32; 4]; 2],
    pub title_opacity: [[f32; 4]; 2],
    pub title_controls: [f32; 4],
    pub monitor_rect: [f32; 4],
    pub monitor_view: [f32; 4],
    pub monitor_crt: Layers,
    pub monitor_ui: [f32; 4],
    pub monitor_rows: [[u32; 4]; crate::titlebar::monitor::VISIBLE_ROWS],
    pub dock_view: [f32; 4],
    pub dock_phase: [f32; 4],
    pub dock_play: [f32; 4],
    pub dock_tip: [f32; 4],
    pub dock_hot: [[f32; 4]; 4],
    pub dock_scale: [[f32; 4]; 2],
    pub dock_fx: [f32; 4],
    pub dock_pulse: [f32; 4],
    pub play: crate::dock::play::Frame,
    pub wall_rect: [f32; 4],
    pub wall_crt: Layers,
    pub wall_fade: [f32; 4],
    pub wall_state: [f32; 4],
    pub wall_grid: [f32; 4],
    pub wall_ui: [u32; 4],
    pub wall_catalog: [u32; 4],
    pub wall_details: [u32; 4],
    pub wall_glows: [u32; 4],
    pub wall_sources: [u32; 4],
    pub wall_scrollbar: [f32; 4],
    pub home_view: [f32; 4],
    pub home_state: [f32; 4],
    pub home_toolbar: [f32; 4],
    pub home_toolbar_view: [f32; 4],
    pub home_toolbar_rects: [[f32; 4]; 6],
    pub home_phase: [f32; 4],
    pub home_rects: [[f32; 4]; 5],
    pub home_images: [[u32; 4]; 5],
    pub home_controls: [[f32; 4]; 3],
    pub home_text: [crate::home::Text; 18],
    pub wall_rows: [[u32; 4]; crate::wallpapers::LINES],
    pub wall_words: [[u32; 4]; 2 * crate::wallpapers::LINES],
    pub wall_advances: [[u32; 4]; 2 * crate::wallpapers::LINES],
    pub wall_sizes: [[u32; 4]; crate::wallpapers::ROWS.div_ceil(4)],
}

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn load_ui_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    name: &str,
) -> Result<wgpu::TextureView> {
    let data = std::io::Cursor::new(bytes);
    let mut reader = png::Decoder::new(data).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("Atlas inválido")?];
    let info = reader.next_frame(&mut pixels)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("El atlas de interfaz debe ser RGBA de 8 bits".into());
    }
    Ok(device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(name),
                size: wgpu::Extent3d {
                    width: info.width,
                    height: info.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::LayerMajor,
            &pixels[..info.buffer_size()],
        )
        .create_view(&Default::default()))
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    home_pipeline: wgpu::RenderPipeline,
    wallpaper_pipeline: wgpu::RenderPipeline,
    glass_capture_pipeline: wgpu::RenderPipeline,
    glass_blur: Option<GlassBlur>,
    video_pipeline: Option<wgpu::RenderPipeline>,
    overlay_pipeline: Option<wgpu::RenderPipeline>,
    background_only: bool,
    present_copy: bool,
    present_copy_supported: bool,
    video_planes: Option<video_planes::VideoPlanes>,
    video_planes_failed: bool,
    last_ui_present: Option<(Instant, u64)>,
    pub dock_preview: dock_preview::Preview,
    pub catalog_preview: dock_preview::Preview,
    pub profile: crate::profile::PaintGpu,
    source_size: [u32; 2],
    source_format: wgpu::TextureFormat,
    timings: Option<PassTimings>,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    bindings: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    mask: wgpu::TextureView,
    title_atlas: wgpu::TextureView,
    dock_atlas: wgpu::TextureView,
    crt_atlas: wgpu::TextureView,
    home_atlas: wgpu::TextureView,
    source: wgpu::TextureView,
    source_texture: wgpu::Texture,
    video_import: Option<wgpu::Texture>,
    empty_posters: wgpu::TextureView,
    posters: Option<Posters>,
    frames: Option<SharedFrames>,
    outgoing: Option<wgpu::TextureView>,
    startup: Option<StartupPainter>,
}

struct StartupPainter {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bindings: wgpu::BindGroup,
}

struct GlassBlur {
    source: wgpu::TextureView,
    horizontal: wgpu::TextureView,
    filtered: wgpu::TextureView,
    passes: [(wgpu::RenderPipeline, wgpu::BindGroup); 2],
}

impl GlassBlur {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    fn new(device: &wgpu::Device, sampler: &wgpu::Sampler) -> Self {
        let surface = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                // The 544×158 panel plus 32 logical pixels of padding on each side,
                // rendered at half resolution. See glass_capture_rect in demo.wgsl.
                size: wgpu::Extent3d {
                    width: 304,
                    height: 111,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: Self::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }).create_view(&Default::default())
        };
        let source = surface("Panel backdrop capture");
        let horizontal = surface("Panel backdrop horizontal blur");
        let filtered = surface("Panel backdrop finished blur");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Panel backdrop Gaussian blur"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glass_blur.wgsl").into()),
        });
        let make_pass = |entry, input| {
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Panel backdrop Gaussian blur"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(Self::FORMAT.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Panel backdrop source and sampler"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            (pipeline, bindings)
        };
        let passes = [make_pass("horizontal", &source), make_pass("vertical", &horizontal)];
        Self { source, horizontal, filtered, passes }
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        capture: &wgpu::RenderPipeline,
        wallpaper: &wgpu::BindGroup,
    ) {
        let [(horizontal, h_source), (vertical, v_source)] = &self.passes;
        for (target, pipeline, bindings) in [
            (&self.source, capture, wallpaper),
            (&self.horizontal, horizontal, h_source),
            (&self.filtered, vertical, v_source),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Panel backdrop"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bindings, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

// Benchmark adjacent real frames on the same GPU clock, alternating which
// path goes first. Normal playback allocates no queries or readback buffers.
struct PassTimings {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    count: Cell<u32>,
}

impl StartupPainter {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Startup transition"),
            source: wgpu::ShaderSource::Wgsl(include_str!("startup.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Startup transition"),
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
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Startup geometry"),
            size: std::mem::size_of::<Paint>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            format,
            pipeline,
            buffer,
            bindings,
        }
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        paint: &Paint,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(paint));
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.format),
            ..Default::default()
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Startup transition"),
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
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings, &[]);
        pass.draw(0..3, 0..1);
    }
}

struct Posters {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    // Each layer keeps its poster ID independently of the card's screen position.
    keys: [PosterKey; ROWS],
}

const POSTER_WIDTH: u32 = 464;
const POSTER_HEIGHT: u32 = 360;

fn poster_texture(device: &wgpu::Device, full: bool) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Wallpapers visible posters"),
        size: wgpu::Extent3d {
            width: if full { POSTER_WIDTH } else { 1 },
            height: if full { POSTER_HEIGHT } else { 1 },
            depth_or_array_layers: if full {
                crate::wallpapers::ROWS as u32
            } else {
                1
            },
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (texture, view)
}

impl Gpu {
    pub async fn new(
        window: Arc<Window>,
        scene: &Scene,
        proof: bool,
        source: Option<Source>,
    ) -> Result<Self> {
        let mut options = wgpu::InstanceDescriptor::new_without_display_handle();
        options.backends = wgpu::Backends::DX12;
        // On the RTX 4070 SUPER, FXC executes this WGSL composition much
        // faster than the SDK's DXC with the same pixels and frame rate.
        let compiler = wgpu::Dx12Compiler::Fxc.with_env();
        options.backend_options.dx12.shader_compiler = compiler.clone();
        let instance = wgpu::Instance::new(options);
        let present_copy = std::env::var("IKA_PRESENT_COPY").as_deref() == Ok("1");
        let prefer_video_planes = present_copy
            && !proof
            && std::env::var("IKA_DCOMP_VIDEO").as_deref() != Ok("0")
            && std::env::var_os("IKA_COMPARE_HOME_GPU").is_none()
            && std::env::var_os("IKA_GPU_TEST_MODE").is_none();
        let (mut video_planes, surface) = if prefer_video_planes {
            match video_planes::VideoPlanes::new(&instance, &window) {
                Ok((planes, overlay)) => (Some(planes), overlay),
                Err(err) => {
                    eprintln!("Video composition unavailable: {err}");
                    (None, instance.create_surface(window.clone())?)
                }
            }
        } else {
            (None, instance.create_surface(window.clone())?)
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let info = adapter.get_info();
        // Video producers publish BGRA8 shared textures.  Pick that surface
        // format before the source arrives (`--await-source`) so attaching a
        // video later can use a raw texture copy instead of a fullscreen
        // sampling pass.  The copy path keeps native resolution/cadence and
        // leaves only the small UI overlay on the 3D engine.
        let format = if std::env::var("IKA_PRESENT_COPY").as_deref() == Ok("1") {
            wgpu::TextureFormat::Bgra8UnormSrgb
        } else if scene.motion.is_some() {
            wgpu::TextureFormat::Bc3RgbaUnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        };
        let background_only =
            source.is_some() && std::env::var("IKA_GPU_TEST_MODE").as_deref() == Ok("background");
        // Keep the copy-capable surface even while the initial source is
        // pending. A later MPV source can then use the direct path without
        // rebuilding the winit surface.
        let source_size = source.map_or(scene.size, |source| source.size);
        let source_format = source.map_or(format, |source| source.format);
        let precompiled = matches!(compiler, wgpu::Dx12Compiler::Fxc)
            && !background_only
            && !cfg!(debug_assertions)
            && adapter
                .features()
                .contains(wgpu::Features::PASSTHROUGH_SHADERS);
        let mut required_features = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let measure = std::env::var_os("IKA_COMPARE_HOME_GPU").is_some();
        if measure {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        if precompiled {
            required_features |= wgpu::Features::PASSTHROUGH_SHADERS;
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await?;
        let size = window.inner_size();
        let timings = measure.then(|| {
            let buffer = |usage| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Frame benchmark"),
                    size: 16,
                    usage,
                    mapped_at_creation: false,
                })
            };
            PassTimings {
                query: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: None,
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
                read: buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
                count: Cell::new(0),
            }
        });
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("La GPU no tiene un formato de presentación compatible")?;
        // DX12 uses latency + 1 swapchain buffers. Two are sufficient for
        // this single presented pass and avoid a third full-size 1440p image.
        config.desired_maximum_frame_latency = 1;
        if video_planes.is_some() {
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        }
        let preferred_format = match source_format {
            wgpu::TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8UnormSrgb,
            _ => wgpu::TextureFormat::Bgra8UnormSrgb,
        };
        config.format = if present_copy {
            caps.formats
                .iter()
                .copied()
                .find(|format| *format == preferred_format)
                .or_else(|| {
                    caps.formats.iter().copied().find(|format| {
                        matches!(
                            format,
                            wgpu::TextureFormat::Bgra8UnormSrgb
                                | wgpu::TextureFormat::Rgba8UnormSrgb
                        )
                    })
                })
        } else {
            caps.formats.iter().copied().find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
                )
            })
        }
        .ok_or("La GPU no ofrece una superficie sRGB de 8 bits")?;
        let present_copy_supported = caps.usages.contains(wgpu::TextureUsages::COPY_DST);
        if present_copy && present_copy_supported {
            config.usage |= wgpu::TextureUsages::COPY_DST;
        }
        if proof {
            if !caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
                return Err("La GPU no permite leer la superficie en modo de prueba".into());
            }
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let encoded = config.format.remove_srgb_suffix();
        config.view_formats.push(encoded);
        surface.configure(&device, &config);
        if let Some(planes) = &mut video_planes {
            planes.resize([config.width, config.height])?;
        }
        let startup = (!proof).then(|| StartupPainter::new(&device, config.view_formats[0]));
        let texture_binding = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let d2 = wgpu::TextureViewDimension::D2;
        let array = wgpu::TextureViewDimension::D2Array;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Frame and UI resources"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Params>() as u64
                        ),
                    },
                    count: None,
                },
                texture_binding(1, d2),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_binding(3, array),
                texture_binding(4, d2),
                texture_binding(5, d2),
                texture_binding(6, array),
                texture_binding(7, d2),
                texture_binding(8, d2),
                texture_binding(9, d2),
                texture_binding(10, d2),
                texture_binding(11, d2),
                texture_binding(12, d2),
                texture_binding(13, d2),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shared wallpaper and UI layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make_pipeline = |vertex: &wgpu::ShaderModule,
                             pixel: &wgpu::ShaderModule,
                             entry,
                             format: wgpu::TextureFormat,
                             blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Wallpaper and effects"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: vertex,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: pixel,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let shared_overlay = present_copy || source.is_some();
        let overlay_blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let pipeline = if precompiled {
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let load = |bytes: &'static [u8]| {
                // SAFETY: The build validates our WGSL and compiles it with
                // FXC using the register assignments for the layout above.
                unsafe {
                    device.create_shader_module_passthrough(
                        wgpu::ShaderModuleDescriptorPassthrough {
                            dxil: Some(bytes.into()),
                            ..Default::default()
                        },
                    )
                }
            };
            let vertex = load(include_bytes!(concat!(env!("OUT_DIR"), "/vertex.cso")));
            let pixel = load(include_bytes!(concat!(env!("OUT_DIR"), "/fragment.cso")));
            let home = load(include_bytes!(concat!(env!("OUT_DIR"), "/home.cso")));
            let wallpaper = load(include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper.cso")));
            let glass = load(include_bytes!(concat!(env!("OUT_DIR"), "/glass_capture.cso")));
            let pipeline = (
                make_pipeline(&vertex, &pixel, "fragment", config.format, None),
                make_pipeline(&vertex, &home, "shared_home", encoded, None),
                make_pipeline(&vertex, &wallpaper, "wallpaper_only", encoded, None),
                make_pipeline(&vertex, &glass, "glass_capture", GlassBlur::FORMAT, None),
                shared_overlay.then(|| {
                    let video = load(include_bytes!(concat!(env!("OUT_DIR"), "/video.cso")));
                    make_pipeline(&vertex, &video, "shared_video_fragment", encoded, None)
                }),
                shared_overlay.then(|| {
                    let overlay = load(include_bytes!(concat!(env!("OUT_DIR"), "/overlay.cso")));
                    make_pipeline(&vertex, &overlay, "overlay", encoded, overlay_blend)
                }),
            );
            match validation.pop().await {
                None => Some(pipeline),
                Some(error) => {
                    eprintln!("Precompiled shaders rejected; compiling WGSL: {error}");
                    None
                }
            }
        } else {
            None
        };
        let precompiled = pipeline.is_some();
        let (pipeline, home_pipeline, wallpaper_pipeline, glass_capture_pipeline, video_pipeline, overlay_pipeline) =
            pipeline.unwrap_or_else(|| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("scene and titlebar"),
                    source: wgpu::ShaderSource::Wgsl(shader_source::WGSL.into()),
                });
                let entry = if background_only {
                    "shared_wallpaper_fragment"
                } else {
                    "fragment"
                };
                (
                    make_pipeline(&shader, &shader, entry, config.format, None),
                    make_pipeline(&shader, &shader, "shared_home", encoded, None),
                    make_pipeline(&shader, &shader, "wallpaper_only", encoded, None),
                    make_pipeline(&shader, &shader, "glass_capture", GlassBlur::FORMAT, None),
                    shared_overlay.then(|| {
                        make_pipeline(&shader, &shader, "shared_video_fragment", encoded, None)
                    }),
                    shared_overlay.then(|| {
                        make_pipeline(&shader, &shader, "overlay", encoded, overlay_blend)
                    }),
                )
            });
        let (texture, frames, video_import) = if let Some(source) = source {
            let (texture, frames) = source.open(&device)?;
            let (texture, video_import) = Self::video_frame_storage(&device, source, texture);
            (texture, Some(frames), video_import)
        } else {
            (
                device.create_texture_with_data(
                    &queue,
                    &wgpu::TextureDescriptor {
                        label: Some("original Wallpaper Engine image"),
                        size: wgpu::Extent3d {
                            width: scene.size[0],
                            height: scene.size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: scene.mip_count,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    TextureDataOrder::LayerMajor,
                    &scene.texture,
                ),
                None,
                None,
            )
        };
        let texture_view = texture.create_view(&Default::default());
        let (mask_size, layers, masks) = scene
            .motion
            .as_ref()
            .map_or(([1, 1], 9, &[0; 18][..]), |effects| {
                (effects.mask_size, 9, effects.masks.as_slice())
            });
        let mask_texture = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("Original Chisa effect masks"),
                size: wgpu::Extent3d {
                    width: mask_size[0],
                    height: mask_size[1],
                    depth_or_array_layers: layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::LayerMajor,
            masks,
        );
        let mask_view = mask_texture.create_view(&Default::default());
        let title_atlas = load_ui_atlas(
            &device,
            &queue,
            include_bytes!("titlebar/atlas.png"),
            "Titlebar atlas",
        )?;
        let dock_atlas = load_ui_atlas(
            &device,
            &queue,
            include_bytes!("dock/atlas.png"),
            "Dock atlas",
        )?;
        let crt_atlas = load_ui_atlas(
            &device,
            &queue,
            include_bytes!("crt-atlas.png"),
            "CRT phosphor sprites",
        )?;
        let home_atlas = load_ui_atlas(&device, &queue, crate::home::TOOLBAR_LABELS, "Home toolbar labels")?;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("one frame and button state"),
            contents: bytemuck::bytes_of(&Params::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let (_, empty_posters) = poster_texture(&device, false);
        let dock_preview = dock_preview::Preview::new(&device, &sampler, &texture_view);
        let catalog_preview = dock_preview.catalog(&device, &sampler, &texture_view);
        let bindings = Self::binding(
            &device,
            &layout,
            &uniforms,
            &sampler,
            [
                &mask_view,
                &title_atlas,
                &dock_atlas,
                &empty_posters,
                &texture_view,
                &crt_atlas,
                &home_atlas,
                &dock_preview.view,
                &dock_preview.previous_view,
                &catalog_preview.view,
                &crt_atlas,
            ],
            &texture_view,
        );
        if present_copy {
            println!(
                "GPU: {} ({:?}); compiler={compiler:?}; precompiled={precompiled}; present_copy={present_copy}/{present_copy_supported}; DirectComposition={}; source={source_size:?}",
                info.name,
                info.backend,
                video_planes.is_some(),
            );
        } else {
            println!(
                "GPU: {} ({:?}); compiler={compiler:?}; precompiled={precompiled}; 1 winit window, 1 surface",
                info.name, info.backend,
            );
        }
        let profile = crate::profile::PaintGpu::new(
            &device,
            &queue,
            config.format.remove_srgb_suffix(),
            &crt_atlas,
            &sampler,
        );
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            home_pipeline,
            wallpaper_pipeline,
            glass_capture_pipeline,
            glass_blur: None,
            video_pipeline,
            overlay_pipeline,
            background_only,
            present_copy,
            present_copy_supported,
            video_planes,
            video_planes_failed: false,
            last_ui_present: None,
            dock_preview,
            catalog_preview,
            profile,
            source_size,
            source_format,
            timings,
            layout,
            uniforms,
            bindings: Some(bindings),
            sampler,
            mask: mask_view,
            title_atlas,
            dock_atlas,
            crt_atlas,
            home_atlas,
            source: texture_view,
            source_texture: texture,
            video_import,
            empty_posters,
            posters: None,
            frames,
            outgoing: None,
            startup,
        })
    }

    fn binding(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniforms: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        [
            mask,
            title_atlas,
            dock_atlas,
            posters,
            outgoing,
            crt_atlas,
            home_atlas,
            dock_preview,
            dock_previous,
            catalog_preview,
            glass_blurred,
        ]: [&wgpu::TextureView; 11],
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mask),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(title_atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(dock_atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(posters),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(outgoing),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(crt_atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(home_atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(dock_preview),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(dock_previous),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(catalog_preview),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::TextureView(glass_blurred),
                },
            ],
        })
    }

    pub fn detach(&mut self) {
        self.bindings = None;
        self.frames = None;
        self.video_import = None;
    }

    pub fn finish(&mut self) -> Result<()> {
        self.wait_for_draw()?;
        self.detach();
        self.device.poll(wgpu::PollType::Poll)?;
        Ok(())
    }

    pub fn wait_for_draw(&self) -> Result<()> {
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(1)),
        })?;
        Ok(())
    }

    /// Returns whether a live shared producer has published a new frame.
    /// `None` means the current source is a cached still image.
    pub fn shared_frame_ready(&self) -> Result<Option<bool>> {
        self.frames
            .as_ref()
            .map_or(Ok(None), |frames| frames.has_new_frame().map(Some))
    }

    pub fn wake_for_shared_frame(&self, wake: &crate::bridge_signal::FrameWake) -> bool {
        self.frames.as_ref().is_some_and(|frames| frames.wake_for_next(wake.handle()))
    }

    pub fn attach(&mut self, source: Source) -> Result<()> {
        self.attach_with_fade(source, false)
    }

    pub fn attach_with_fade(&mut self, source: Source, fade: bool) -> Result<()> {
        let (texture, frames) = source.open(&self.device)?;
        // Switch::Waiting guarantees the old texture is pinned and its
        // producer has exited. Keep that allocation for the fade.
        self.outgoing = fade.then(|| self.source.clone());
        self.source_size = source.size;
        self.source_format = source.format;
        (self.source_texture, self.video_import) =
            Self::video_frame_storage(&self.device, source, texture);
        self.source = self.source_texture.create_view(&Default::default());
        self.bind_source();
        self.frames = Some(frames);
        Ok(())
    }

    pub fn show_cached_frame(&mut self, frame: &CachedFrame, fade: bool) {
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("Cached first wallpaper frame"),
                size: frame.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: frame.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::LayerMajor,
            &frame.pixels,
        );
        self.outgoing = fade.then(|| self.source.clone());
        self.source_size = [frame.size.width, frame.size.height];
        self.source_format = frame.format;
        self.source_texture = texture;
        self.source = self.source_texture.create_view(&Default::default());
        self.frames = None;
        self.video_import = None;
        self.bind_source();
    }

    pub fn resume_live_frame(&mut self, source: Source) -> Result<()> {
        let (texture, frames) = source.open(&self.device)?;
        self.source_size = source.size;
        self.source_format = source.format;
        (self.source_texture, self.video_import) =
            Self::video_frame_storage(&self.device, source, texture);
        self.source = self.source_texture.create_view(&Default::default());
        self.frames = Some(frames);
        self.bind_source();
        Ok(())
    }

    pub fn finish_wallpaper_fade(&mut self) {
        if self.outgoing.take().is_some() {
            self.bind_source();
        }
    }

    fn video_frame_storage(
        device: &wgpu::Device,
        source: Source,
        imported: wgpu::Texture,
    ) -> (wgpu::Texture, Option<wgpu::Texture>) {
        if !source.video {
            return (imported, None);
        }
        let stable = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Last complete wallpaper video frame"),
            size: imported.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: source.format,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        (stable, Some(imported))
    }

    fn copy_fresh_video_frame(&self, encoder: &mut wgpu::CommandEncoder, fresh: bool) {
        // Once we signal used, libmpv may be halfway through its next draw.
        // UI-only presents, previews and fades must sample our completed copy.
        if fresh && let Some(imported) = &self.video_import {
            encoder.copy_texture_to_texture(
                imported.as_image_copy(),
                self.source_texture.as_image_copy(),
                imported.size(),
            );
        }
    }

    fn bind_source(&mut self) {
        self.last_ui_present = None;
        self.dock_preview.bind_main(&self.device, &self.sampler, &self.source);
        self.bindings = Some(Self::binding(
            &self.device,
            &self.layout,
            &self.uniforms,
            &self.sampler,
            [
                &self.mask,
                &self.title_atlas,
                &self.dock_atlas,
                self.posters
                    .as_ref()
                    .map_or(&self.empty_posters, |p| &p.view),
                self.outgoing.as_ref().unwrap_or(&self.source),
                &self.crt_atlas,
                &self.home_atlas,
                &self.dock_preview.view,
                &self.dock_preview.previous_view,
                &self.catalog_preview.view,
                self.glass_blur.as_ref().map_or(&self.crt_atlas, |blur| &blur.filtered),
            ],
            &self.source,
        ));
    }

    pub fn attach_dock_preview(&mut self, source: Source, fade: bool) -> Result<()> {
        self.dock_preview.attach(&self.device, &self.sampler, source, fade)
    }

    pub fn attach_catalog_preview(&mut self, source: Source) -> Result<()> {
        self.catalog_preview.attach(&self.device, &self.sampler, source, false)
    }

    pub fn load_home_atlas(&mut self, atlas: image::RgbaImage) {
        self.home_atlas =
            self.device
                .create_texture_with_data(
                    &self.queue,
                    &wgpu::TextureDescriptor {
                        label: Some("Home covers, icons and toolbar labels"),
                        size: wgpu::Extent3d {
                            width: atlas.width(),
                            height: atlas.height(),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    TextureDataOrder::LayerMajor,
                    atlas.as_raw(),
                )
                .create_view(&Default::default());
        self.profile.register_collection_art(&self.home_atlas, [atlas.width(), atlas.height()]);
        if self.bindings.is_some() {
            self.bind_source();
        }
    }

    pub fn release_posters(&mut self) {
        if self.posters.take().is_some() {
            self.bind_source();
        }
    }

    pub fn load_posters(&mut self, cache: &Path, keys: &[PosterKey; ROWS]) {
        const ROW_BYTES: u32 = POSTER_WIDTH * 4;
        // Protect every poster still on screen. Keep the rest until its layer is needed.
        let mut reserved = self.posters.as_ref().map_or([false; ROWS], |posters| {
            posters
                .keys
                .map(|key| key != PosterKey::EMPTY && keys.contains(&key))
        });
        let mut path = None::<PathBuf>;
        let mut count = 0;
        for &key in keys {
            let Some(id) = key.as_str() else { continue };
            if self.poster_layer(key).is_some() {
                continue;
            }
            let path = path.get_or_insert_with(|| cache.to_path_buf());
            path.push(format!("{id}.png"));
            let file = File::open(&*path);
            path.pop();
            let Ok(file) = file else { continue };
            let Ok(png) = image::codecs::png::PngDecoder::new(BufReader::new(file)) else {
                continue;
            };
            if png.dimensions() != (POSTER_WIDTH, POSTER_HEIGHT) || png.color_type() != image::ColorType::Rgba8 {
                continue;
            }
            let mut bytes = vec![0; (ROW_BYTES * POSTER_HEIGHT) as usize];
            if png.read_image(&mut bytes).is_err() {
                continue;
            }
            if self.posters.is_none() {
                let (texture, view) = poster_texture(&self.device, true);
                self.posters = Some(Posters {
                    texture,
                    view,
                    keys: [PosterKey::EMPTY; ROWS],
                });
                self.bind_source();
            }
            let posters = self.posters.as_mut().unwrap();
            let Some(layer) = posters
                .keys
                .iter()
                .position(|&id| id == PosterKey::EMPTY)
                .or_else(|| reserved.iter().position(|&held| !held))
            else {
                continue;
            };
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &posters.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_BYTES),
                    rows_per_image: Some(POSTER_HEIGHT),
                },
                wgpu::Extent3d {
                    width: POSTER_WIDTH,
                    height: POSTER_HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
            posters.keys[layer] = key;
            reserved[layer] = true;
            count += 1;
            // Limit PNG decoding and upload work per redraw.
            if count == 3 {
                break;
            }
        }
        if count > 0 {
            self.last_ui_present = None;
        }
    }

    pub fn poster_layer(&self, key: PosterKey) -> Option<u32> {
        if key == PosterKey::EMPTY {
            return None;
        }
        self.posters
            .as_ref()?
            .keys
            .iter()
            .position(|&id| id == key)
            .map(|i| i as u32)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0
            && size.height > 0
            && (size.width, size.height) != (self.config.width, self.config.height)
        {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            self.last_ui_present = None;
            if let Some(planes) = &mut self.video_planes
                && let Err(error) = planes.resize([size.width, size.height])
            {
                eprintln!("Video composition resize: {error}");
            }
        }
    }

    fn acquire(&self) -> Result<Option<wgpu::SurfaceTexture>> {
        Ok(match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(swap)
            | wgpu::CurrentSurfaceTexture::Suboptimal(swap) => Some(swap),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            _ => return Err("Se perdió la superficie wgpu".into()),
        })
    }

    pub fn draw_loading(&self, window: &Window, paint: &Paint) -> Result<bool> {
        let (Some(startup), Some(swap)) = (&self.startup, self.acquire()?) else {
            return Ok(false);
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        startup.encode(
            &mut encoder,
            &self.queue,
            &swap.texture,
            paint,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
        self.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        swap.present();
        Ok(true)
    }

    pub fn finish_startup(&mut self) {
        self.startup = None;
    }

    pub fn startup_dock_ready(&self) -> bool {
        self.background_only || self.dock_preview.has_thumbnail()
    }

    fn draw_video_planes(
        &mut self,
        window: &Window,
        params: &Params,
        frame: u64,
        active_ui: bool,
        thumbnail: dock_preview::Frame,
        catalog_frame: dock_preview::Frame,
    ) -> Result<bool> {
        let now = Instant::now();
        let end = if params.wall_rect[2] > 0.0 {
            std::mem::size_of::<Params>()
        } else {
            std::mem::offset_of!(Params, wall_rows)
        };
        // Ambient clocks don't force every decoded frame to repaint the UI.
        // Changes in controls and animations still redraw without waiting.
        let mut ui = *params;
        ui.title_phase[..2].fill(0.0);
        ui.dock_phase = [0.0; 4];
        ui.dock_play[..3].fill(0.0);
        ui.dock_pulse[..3].fill(0.0);
        ui.play = crate::dock::play::Frame::zeroed();
        ui.home_phase = [0.0; 4];
        let mut hash = DefaultHasher::new();
        hash.write(&bytemuck::bytes_of(&ui)[..end]);
        hash.write_u64(self.profile.ui_stamp());
        let stamp = hash.finish();
        let catalog_open = params.wall_rect[2] > 0.0;
        let frames = self.frames.as_ref().expect("video frames");
        let new_video = frames.is_new(frame);
        let update_ui = active_ui || thumbnail.sample.is_some() || catalog_frame.sample.is_some()
            || self.profile.cover_ready()
            || self.last_ui_present.is_none_or(|(at, previous)| {
                stamp != previous
                    // Do not run unrelated ambient effects while dragging the
                    // model. Video frames and actual UI changes remain independent.
                    || (new_video && !catalog_open && !self.profile.dragging_model()
                        && now - at >= Duration::from_millis(62))
            });
        let ui_swap = if update_ui { self.acquire()? } else { None };
        let planes = self.video_planes.as_mut().expect("video planes");
        planes.configure(&self.device, &self.config, self.source_size)?;
        let video_swap = if new_video || self.last_ui_present.is_none() {
            match planes.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(swap)
                | wgpu::CurrentSurfaceTexture::Suboptimal(swap) => Some(swap),
                wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
                _ => return Err("Se perdió la capa de vídeo".into()),
            }
        } else {
            None
        };
        // Present only the preview itself on a drag. The rest of the profile
        // remains in the window overlay, which does not depend on the pose.
        let profile_region = self.profile.model_region(self.config.width, self.config.height);
        let profile_stamp = self.profile.stamp();
        let profile_swap = if let Some(region) = profile_region {
            planes.acquire_profile(
                &self.device, &self.config, region, profile_stamp,
            )?
        } else {
            planes.hide_profile()?;
            None
        };
        if ui_swap.is_none() && video_swap.is_none() && profile_swap.is_none() {
            return Ok(false);
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.copy_fresh_video_frame(&mut encoder, new_video);
        if let Some(swap) = &video_swap {
            encoder.copy_texture_to_texture(
                self.source_texture.as_image_copy(),
                swap.texture.as_image_copy(),
                wgpu::Extent3d {
                    width: self.source_size[0],
                    height: self.source_size[1],
                    depth_or_array_layers: 1,
                },
            );
        }
        if let Some(swap) = &ui_swap {
            self.dock_preview.encode(&mut encoder, &self.queue, thumbnail, self.source_size, false);
            self.catalog_preview.encode(&mut encoder, &self.queue, catalog_frame, self.source_size, false);
            self.queue
                .write_buffer(&self.uniforms, 0, &bytemuck::bytes_of(params)[..end]);
            if params.next_state[3] > 0.5 {
                self.glass_blur.as_ref().expect("console glass").encode(
                    &mut encoder, &self.glass_capture_pipeline, self.bindings.as_ref().expect("render bindings"),
                );
            }
            self.draw_overlay(
                &mut encoder,
                &swap.texture,
                params,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
            self.profile.encode(&mut encoder, &swap.texture);
        }
        if let (Some(swap), Some([x, y, _, _])) = (&profile_swap, profile_region) {
            if ui_swap.is_some() {
                // The base and cropped passes use different egui projections.
                // Submit before changing the renderer's shared vertex buffers.
                self.queue.submit([encoder.finish()]);
                encoder = self.device.create_command_encoder(&Default::default());
            }
            self.profile.encode_region(
                &mut encoder, &swap.texture, [x, y], wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
        }
        self.queue.submit([encoder.finish()]);
        self.profile.presented()?;
        if video_swap.is_some() {
            frames.release(frame)?;
        }
        window.pre_present_notify();
        if let Some(swap) = video_swap {
            swap.present();
        }
        if let Some(swap) = ui_swap {
            swap.present();
            self.last_ui_present = Some((now, stamp));
            self.dock_preview.presented(thumbnail);
            self.catalog_preview.presented(catalog_frame);
        }
        if let (Some(swap), Some(region)) = (profile_swap, profile_region) {
            self.video_planes
                .as_mut()
                .expect("video planes")
                .present_profile(swap, region, profile_stamp)?;
        }
        Ok(true)
    }

    fn draw_overlay(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        params: &Params,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let [w, h] = [self.config.width, self.config.height];
        let dpi = params.screen[3];
        let dock_top = h.saturating_sub((251.0 * params.dock_view[2]).ceil() as u32);
        let center = w as f32 * 0.5;
        let mut regions = vec![
            [0, 0, w, (66.0 * dpi).ceil() as u32],
            [
                (center - 484.0 * dpi).floor() as u32,
                0,
                (center + 484.0 * dpi).ceil() as u32,
                (116.0 * dpi).ceil() as u32,
            ],
            [0, dock_top, w, h],
        ];
        if params.title_phase[3] > 0.0 {
            regions.push([
                w.saturating_sub((480.0 * dpi).ceil() as u32),
                0,
                w,
                (256.0 * dpi).ceil() as u32,
            ]);
        }
        if params.wall_rect[2] > 0.0 {
            let [x, y, scale, height] = params.wall_rect;
            // Include the falloff outside the panel and the tube's glow.
            let (mx, my) = (
                (1260.0 * 0.02 + 125.0) * scale,
                (height * 0.02 + 150.0) * scale,
            );
            regions.push([
                (x - mx).floor() as u32,
                (y - my).floor() as u32,
                (x + 1260.0 * scale + mx).ceil() as u32,
                (y + height * scale + my).ceil() as u32,
            ]);
        }
        for [x, y, width, height] in [params.glass, params.button, params.next_button] {
            if width > 0.0 {
                regions.push([
                    x.floor() as u32,
                    y.floor() as u32,
                    (x + width).ceil() as u32,
                    (y + height).ceil() as u32,
                ]);
            }
        }
        if params.home_view[2] > 0.0 {
            let [x, y, scale, _] = params.home_view;
            let half_w = match params.home_state[0] as u32 {
                0 => 170.0,
                1..=4 => 565.0,
                _ => 810.0,
            };
            regions.push([
                (x - half_w * scale).floor() as u32,
                (y - 134.0 * scale).floor() as u32,
                (x + half_w * scale).ceil() as u32,
                (y + 134.0 * scale).ceil() as u32,
            ]);
        }
        if params.home_toolbar_view[2] > 0.0 {
            let [x0, y0, x1, y1] = crate::home::toolbar_bounds(params.home_toolbar_view);
            regions.push([
                x0.floor() as u32,
                y0.floor() as u32,
                x1.ceil() as u32,
                y1.ceil() as u32,
            ]);
        }
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.remove_srgb_suffix()),
            ..Default::default()
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wallpaper UI overlay"),
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
        });
        pass.set_pipeline(
            self.overlay_pipeline
                .as_ref()
                .expect("wallpaper overlay pipeline"),
        );
        pass.set_bind_group(0, self.bindings.as_ref().unwrap(), &[]);
        // Merge overlapping UI regions in each horizontal band so
        // translucent pixels are blended exactly once.
        let mut edges: Vec<_> = regions
            .iter()
            .flat_map(|r| [r[1].min(h), r[3].min(h)])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        regions.sort_unstable_by_key(|r| r[0]);
        for band in edges.windows(2) {
            let (top, bottom) = (band[0], band[1]);
            let mut span = None;
            for &[left, y, right, end] in &regions {
                let right = right.min(w);
                if y > top || end < bottom || left >= right {
                    continue;
                }
                match span {
                    Some((l, r)) if left <= r => span = Some((l, r.max(right))),
                    old => {
                        if let Some((l, r)) = old {
                            pass.set_scissor_rect(l, top, r - l, bottom - top);
                            pass.draw(0..3, 0..1);
                        }
                        span = Some((left, right));
                    }
                }
            }
            if let Some((l, r)) = span {
                pass.set_scissor_rect(l, top, r - l, bottom - top);
                pass.draw(0..3, 0..1);
            }
        }
    }

    pub fn draw(
        &mut self,
        window: &Window,
        params: &mut Params,
        snapshot: Option<&Path>,
        hold_shared_frame: bool,
        paint: Option<&Paint>,
        active_ui: bool,
        dock_playback: dock_preview::Playback,
        catalog_playing: bool,
    ) -> Result<bool> {
        if self.bindings.is_none() {
            return Ok(false);
        }
        let blur_active = !self.background_only && params.glass[2] > 0.0 && params.state[0] > 0.5;
        if blur_active && self.glass_blur.is_none() {
            self.glass_blur = Some(GlassBlur::new(&self.device, &self.sampler));
            self.bind_source();
        }
        let shared_frame = if let Some(frames) = &self.frames {
            let Some(frame) = frames.frame()? else {
                return Ok(false);
            };
            Some(frame)
        } else {
            None
        };
        let shared_color = params.motion[7][3] > 0.5;
        let source_size = [self.source_texture.width(), self.source_texture.height()];
        let dock_rect = if shared_color {
            [0.0, 0.0, source_size[0] as f32, source_size[1] as f32]
        } else {
            params.frame
        };
        let main_new = self.frames.as_ref().zip(shared_frame).is_some_and(|(frames, frame)| frames.is_new(frame));
        let now = Instant::now();
        // Seed stills immediately. Give shared video a short preroll under
        // the loading cover before freezing its first dock thumbnail.
        let can_seed = !self.background_only
            && (self.frames.is_none() || self.dock_preview.seed_due(now));
        let thumbnail = self.dock_preview.frame(now, dock_playback, main_new, dock_rect, can_seed);
        params.dock_view[3] = thumbnail.previous_weight;
        let catalog_playback = if catalog_playing { dock_preview::Playback::Remote } else { dock_preview::Playback::Paused };
        let catalog_frame = self.catalog_preview.frame(Instant::now(), catalog_playback, false, dock_rect, false);
        let fast_copy = self.present_copy
            && self.present_copy_supported
            && snapshot.is_none()
            && paint.is_none()
            && self.frames.is_some()
            && params.motion[7][0] <= 0.0001
            && params.wall_state[3] <= 0.0001
            && params.state[0] <= 0.5
            && params.monitor_ui[3] <= 0.0
            && params.wall_rect[2] <= 0.0
            && self.source_size[0] == self.config.width
            && self.source_size[1] == self.config.height
            && self.source_format == self.config.format.remove_srgb_suffix();
        // MP4s normally have a 2560x1440 shared texture while the maximized
        // client area is a few pixels shorter. A simple encoded-color sample
        // avoids the expensive linear/sRGB conversion and branch-heavy scene
        // shader while keeping the native video resolution and UI overlay.
        let video_compatible = self.video_pipeline.is_some()
            && self.frames.is_some()
            && self.source_format == wgpu::TextureFormat::Bgra8Unorm
            && snapshot.is_none()
            && paint.is_none()
            && params.motion[7][0] <= 0.0001
            && params.monitor_ui[3] <= 0.0;
        let fast_video = video_compatible && !blur_active;
        if video_compatible
            && (fast_video || params.next_state[3] > 0.5)
            && !hold_shared_frame
            && !self.background_only
            && params.wall_state[3] == 0.0
            && self.video_planes.is_some()
            && !self.video_planes_failed
            && let Some(frame) = shared_frame
        {
            match self.draw_video_planes(window, params, frame, active_ui, thumbnail, catalog_frame) {
                Ok(presented) => return Ok(presented),
                Err(error) => {
                    self.video_planes_failed = true;
                    eprintln!("Native video surface failed; using the wgpu compositor: {error}");
                }
            }
        }
        let bindings = self.bindings.as_ref().expect("render bindings");
        self.last_ui_present = None;
        let Some(swap) = self.acquire()? else {
            return Ok(false);
        };
        if !fast_copy || !self.background_only {
            let bytes = bytemuck::bytes_of(params);
            let end = if params.wall_rect[2] > 0.0 {
                bytes.len()
            } else if params.home_view[2] > 0.0 {
                std::mem::offset_of!(Params, wall_rows)
            } else {
                std::mem::offset_of!(Params, home_rects)
            };
            self.queue.write_buffer(&self.uniforms, 0, &bytes[..end]);
        }
        // Crossfades, dimming, blur tools and modal panels keep their original
        // linear-space math. No intermediate image or extra pass for Home.
        let home = !self.background_only
            && !blur_active
            && params.home_view[2] > 0.0
            && params.motion[7][3] > 0.5
            && params.motion[7][0] == 0.0
            && params.wall_state[3] == 0.0
            && params.monitor_ui[3] == 0.0
            && params.wall_rect[2] == 0.0;
        let sample = self
            .timings
            .as_ref()
            .filter(|_| home && paint.is_none())
            .and_then(|t| {
                let count = t.count.get().wrapping_add(1);
                t.count.set(count);
                (count % 32 < 2).then_some((count % 2 == count / 32 % 2, t))
            });
        let home = home && sample.as_ref().is_none_or(|(optimized, _)| *optimized);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.copy_fresh_video_frame(&mut encoder, main_new);
        self.dock_preview.encode(&mut encoder, &self.queue, thumbnail, source_size, !shared_color);
        self.catalog_preview.encode(&mut encoder, &self.queue, catalog_frame, source_size, false);
        if blur_active {
            self.glass_blur.as_ref().unwrap().encode(&mut encoder, &self.glass_capture_pipeline, bindings);
        }
        if fast_copy || fast_video {
            if fast_copy {
                let source_aspect = self.source_size[0] as f32 / self.source_size[1] as f32;
                let target_aspect = self.config.width as f32 / self.config.height as f32;
                let mut origin = wgpu::Origin3d::ZERO;
                if source_aspect > target_aspect {
                    let visible = (self.source_size[1] as f32 * target_aspect)
                        .round()
                        .clamp(1.0, self.source_size[0] as f32)
                        as u32;
                    origin.x = (self.source_size[0] - visible) / 2;
                } else if source_aspect < target_aspect {
                    let visible = (self.source_size[0] as f32 / target_aspect)
                        .round()
                        .clamp(1.0, self.source_size[1] as f32)
                        as u32;
                    origin.y = (self.source_size[1] - visible) / 2;
                }
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.source_texture,
                        mip_level: 0,
                        origin,
                        aspect: wgpu::TextureAspect::All,
                    },
                    swap.texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: self.config.width,
                        height: self.config.height,
                        depth_or_array_layers: 1,
                    },
                );
            } else {
                let view = swap.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(self.config.format.remove_srgb_suffix()),
                    ..Default::default()
                });
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("native-rate shared video"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::DontCare(unsafe {
                                wgpu::LoadOpDontCare::enabled()
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(self.video_pipeline.as_ref().expect("video pipeline"));
                pass.set_bind_group(0, bindings, &[]);
                pass.draw(0..3, 0..1);
            }
            if !self.background_only {
                self.draw_overlay(&mut encoder, &swap.texture, params, wgpu::LoadOp::Load);
            }
        } else {
            let view = swap.texture.create_view(&wgpu::TextureViewDescriptor {
                format: home.then(|| self.config.format.remove_srgb_suffix()),
                ..Default::default()
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("one swapchain target"),
                timestamp_writes: sample
                    .as_ref()
                    .map(|(_, t)| wgpu::RenderPassTimestampWrites {
                        query_set: &t.query,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // All paths cover every pixel with opaque, unblended
                        // writes before storing or drawing the loading overlay.
                        load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, bindings, &[]);
            if home {
                let (w, h) = (self.config.width, self.config.height);
                let dpi = params.screen[3];
                let [_, center, scale, _] = params.home_view;
                let title_end = if params.title_phase[3] > 0.0 {
                    256.0
                } else {
                    116.0
                } * dpi;
                // Outside the title, home controls and dock this fragment costs a
                // texture sample. Disjoint scissors write every pixel once.
                let top = (title_end.ceil() as u32).min(h);
                let toolbar_top = crate::home::toolbar_bounds(params.home_toolbar_view)[1];
                let bottom = ((center - 134.0 * scale)
                    .min(toolbar_top)
                    .min(h as f32 - 251.0 * params.dock_view[2])
                    .floor() as u32)
                    .clamp(top, h);
                for (start, end, pipeline) in [
                    (0, top, &self.home_pipeline),
                    (top, bottom, &self.wallpaper_pipeline),
                    (bottom, h, &self.home_pipeline),
                ] {
                    if start < end {
                        pass.set_pipeline(pipeline);
                        pass.set_scissor_rect(0, start, w, end - start);
                        pass.draw(0..3, 0..1);
                    }
                }
            } else {
                pass.set_pipeline(&self.pipeline);
                pass.draw(0..3, 0..1);
            }
        }
        if !self.background_only {
            self.profile.encode(&mut encoder, &swap.texture);
        }
        if let (Some(startup), Some(paint)) = (&self.startup, paint) {
            startup.encode(
                &mut encoder,
                &self.queue,
                &swap.texture,
                paint,
                wgpu::LoadOp::Load,
            );
        }
        // Verification alone copies the finished swapchain image to a CPU buffer.
        let stride = (self.config.width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = snapshot.map(|_| {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("test screenshot only"),
                size: u64::from(stride) * u64::from(self.config.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                swap.texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: self.config.width,
                    height: self.config.height,
                    depth_or_array_layers: 1,
                },
            );
            buffer
        });
        if let Some((_, t)) = &sample {
            encoder.resolve_query_set(&t.query, 0..2, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, 16);
        }
        let submission = self.queue.submit([encoder.finish()]);
        self.profile.presented()?;
        if let (Some(frames), Some(frame)) = (&self.frames, shared_frame) {
            if hold_shared_frame {
                frames.hold(frame);
            } else {
                frames.release(frame)?;
            }
        }
        window.pre_present_notify();
        swap.present();
        if let Some(planes) = &mut self.video_planes {
            planes.hide_profile()?;
        }
        self.dock_preview.presented(thumbnail);
        self.catalog_preview.presented(catalog_frame);
        if let Some((optimized, t)) = sample {
            let (tx, rx) = std::sync::mpsc::channel();
            t.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission.clone()),
                timeout: Some(Duration::from_secs(2)),
            })?;
            rx.recv_timeout(Duration::from_secs(2))??;
            let bytes = t.read.slice(..).get_mapped_range();
            let ticks: &[u64] = bytemuck::cast_slice(&bytes);
            let us = ticks[1].saturating_sub(ticks[0]) as f64
                * f64::from(self.queue.get_timestamp_period())
                / 1000.0;
            drop(bytes);
            t.read.unmap();
            println!(
                "GPU pass us {} {us:.2}",
                if optimized { "home" } else { "reference" }
            );
        }
        if let (Some(buffer), Some(file)) = (readback, snapshot) {
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
            self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })?;
            rx.recv_timeout(Duration::from_secs(1))??;
            let pixels = buffer.slice(..).get_mapped_range();
            let width = self.config.width * 4;
            let mut rgba = Vec::with_capacity((width * self.config.height) as usize);
            for row in pixels.chunks_exact(stride as usize) {
                rgba.extend_from_slice(&row[..width as usize]);
            }
            if self.config.format == wgpu::TextureFormat::Bgra8UnormSrgb {
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            let mut png =
                png::Encoder::new(File::create(file)?, self.config.width, self.config.height);
            png.set_color(png::ColorType::Rgba);
            png.set_depth(png::BitDepth::Eight);
            png.write_header()?.write_image_data(&rgba)?;
        }
        Ok(true)
    }
}

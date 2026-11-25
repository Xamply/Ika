//! DX12 view of the frame transferred by the isolated D3D11 worker.

use std::{cell::Cell, path::Path};
use windows::Win32::{
    Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
    Graphics::Direct3D12::{
        D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
        ID3D12CommandQueue, ID3D12Fence, ID3D12Resource,
    },
    System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy)]
pub struct Source {
    pid: u32,
    pub size: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub video: bool,
    frame: usize,
    ready: usize,
    used: usize,
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub struct SharedFrames {
    ready: ID3D12Fence,
    used: ID3D12Fence,
    queue: ID3D12CommandQueue,
    last: Cell<u64>,
    held: Cell<Option<u64>>,
    armed: Cell<u64>,
}

impl Source {
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut fields = text.split_whitespace();
        let [pid, width, height, format, frame, ready, used]: [usize; 7] = fields
            .by_ref()
            .take(7)
            .map(str::parse)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| "Metadatos incompletos de la textura compartida")?;
        let video = match (fields.next(), fields.next()) {
            (None, None) => false,
            (Some("video"), None) => true,
            _ => return Err("Tipo de textura compartida desconocido".into()),
        };
        let format = match format {
            28 => wgpu::TextureFormat::Rgba8Unorm,
            87 => wgpu::TextureFormat::Bgra8Unorm,
            _ => return Err("El productor no entregó una textura SDR de 8 bits".into()),
        };
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || frame == 0
            || ready == 0
            || used == 0
        {
            return Err("La textura compartida tiene valores fuera del contrato".into());
        }
        Ok(Self {
            pid: pid.try_into()?,
            size: [width as u32, height as u32],
            format,
            video,
            frame,
            ready,
            used,
        })
    }

    pub fn open(self, device: &wgpu::Device) -> Result<(wgpu::Texture, SharedFrames)> {
        // The producer owns the texture and two ordinary NT fence handles.
        // The DXGI legacy resource handle is already valid in both processes.
        unsafe {
            let producer = OwnedHandle(OpenProcess(PROCESS_DUP_HANDLE, false, self.pid)?);
            let duplicate = |handle| -> Result<OwnedHandle> {
                let mut local = HANDLE::default();
                DuplicateHandle(
                    producer.0,
                    HANDLE(handle as _),
                    GetCurrentProcess(),
                    &mut local,
                    0,
                    false,
                    DUPLICATE_SAME_ACCESS,
                )?;
                Ok(OwnedHandle(local))
            };
            let ready_handle = duplicate(self.ready)?;
            let used_handle = duplicate(self.used)?;
            let (resource, ready, used, queue) = {
                let hal = device
                    .as_hal::<wgpu::hal::api::Dx12>()
                    .ok_or("La GPU no es DX12")?;
                let dx = hal.raw_device();
                let mut resource = None;
                let mut ready = None;
                let mut used = None;
                dx.OpenSharedHandle::<ID3D12Resource>(HANDLE(self.frame as _), &mut resource)?;
                dx.OpenSharedHandle::<ID3D12Fence>(ready_handle.0, &mut ready)?;
                dx.OpenSharedHandle::<ID3D12Fence>(used_handle.0, &mut used)?;
                (
                    resource.ok_or("Falta el recurso DXGI")?,
                    ready.ok_or("Falta la señal de escritura")?,
                    used.ok_or("Falta la señal de lectura")?,
                    hal.raw_queue().clone(),
                )
            };
            let raw = resource.GetDesc();
            let direct = match std::env::var("IKA_TRANSFER_ARCH").as_deref() {
                Err(_) | Ok("") | Ok("direct") | Ok("direct_alias") | Ok("copy_nt")
                | Ok("copy_read") => true,
                _ => false,
            };
            if raw.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
                || raw.Width != u64::from(self.size[0])
                || raw.Height != self.size[1]
                || raw.DepthOrArraySize != 1
                || raw.MipLevels != 1
                || raw.SampleDesc.Count != 1
                || (!direct
                    && !raw
                        .Flags
                        .contains(D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS))
                || raw.Format.0
                    != if self.format == wgpu::TextureFormat::Rgba8Unorm {
                        28
                    } else {
                        87
                    }
            {
                return Err(format!(
                    "Descripción DX12 incompatible con la textura compartida: {raw:?}"
                )
                .into());
            }
            let size = wgpu::Extent3d {
                width: self.size[0],
                height: self.size[1],
                depth_or_array_layers: 1,
            };
            let mut usage = wgpu::TextureUsages::TEXTURE_BINDING;
            if self.video || std::env::var("IKA_PRESENT_COPY").as_deref() == Ok("1") {
                usage |= wgpu::TextureUsages::COPY_SRC;
            }
            let desc = wgpu::TextureDescriptor {
                label: Some("Official Wallpaper Engine GPU frame"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage,
                view_formats: &[],
            };
            // OpenSharedHandle opened it on this precise device. The producer
            // completed its write and won't reuse it until the read fence.
            // Simultaneous-access textures decay back to COMMON after execute.
            let hal = wgpu::hal::dx12::Device::texture_from_raw(
                resource,
                self.format,
                desc.dimension,
                size,
                1,
                1,
            );
            let texture = device.create_texture_from_hal::<wgpu::hal::api::Dx12>(hal, &desc);
            Ok((
                texture,
                SharedFrames {
                    ready,
                    used,
                    queue,
                    last: Cell::new(0),
                    held: Cell::new(None),
                    armed: Cell::new(0),
                },
            ))
        }
    }
}

impl SharedFrames {
    pub fn wake_for_next(&self, event: HANDLE) -> bool {
        if self.held.get().is_some() {
            return false;
        }
        let next = self.last.get() + 1;
        if self.armed.get() == next {
            return true;
        }
        if unsafe { self.ready.SetEventOnCompletion(next, event) }.is_err() {
            return false;
        }
        self.armed.set(next);
        true
    }

    pub fn is_new(&self, serial: u64) -> bool {
        serial > self.last.get()
    }

    pub fn has_new_frame(&self) -> Result<bool> {
        if self.held.get().is_some() {
            return Ok(false);
        }
        let value = unsafe { self.ready.GetCompletedValue() };
        if value == u64::MAX {
            return Err("El dispositivo DXGI del productor se perdió".into());
        }
        Ok(value > self.last.get())
    }

    pub fn frame(&self) -> Result<Option<u64>> {
        if let Some(frame) = self.held.get() {
            // The copy already completed. Its producer cannot reuse this
            // texture while we withhold the used fence, even as it exits.
            return Ok(Some(frame));
        }
        let value = unsafe { self.ready.GetCompletedValue() };
        if value == u64::MAX {
            return Err("El dispositivo DXGI del productor se perdió".into());
        }
        // UI animations must not inherit the producer's cadence. Once the first
        // frame has been presented, keep sampling that valid texture until the
        // producer publishes a newer fence value. This lets CRT, titlebar and
        // Home carousel animations continue at the window's own cadence even
        // when a wallpaper is paused, throttled or produces frames irregularly.
        let last = self.last.get();
        Ok(if value > last {
            Some(value)
        } else if last != 0 {
            Some(last)
        } else {
            None
        })
    }

    pub fn hold(&self, frame: u64) {
        self.held.set(Some(frame));
    }

    pub fn release(&self, frame: u64) -> Result<()> {
        self.held.set(None);
        if self.last.get() == frame {
            // The producer already has this acknowledgement. A 120 Hz UI
            // can reuse a still image indefinitely without signalling again.
            return Ok(());
        }
        // Signal on the same queue, strictly after submitting the draw that
        // sampled the texture, so the producer never overwrites a GPU read.
        unsafe {
            self.queue.Signal(&self.used, frame)?;
        }
        self.last.set(frame);
        if frame == 1 || frame.is_multiple_of(60) {
            println!("Shared frames rendered: {frame}");
        }
        Ok(())
    }
}

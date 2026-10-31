// Capture complete DWM frames rather than asking the DirectX window to paint itself.
// Readback runs on one MTA thread; the WebView only copies the last completed frame.
use super::wallpaper_glass::{HEIGHT, WIDTH};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible};
use windows_core::{factory, Interface, Result};

struct Capture {
    source_hwnd: isize,
    layout_notification_pending: bool,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    winrt_device: IDirect3DDevice,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    size: SizeInt32,
    textures: Option<Textures>,
    arrived: std::sync::mpsc::Receiver<()>,
    arrived_token: i64,
}

struct Textures {
    mipmap: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
    level: u32,
    width: usize,
    height: usize,
}

impl Capture {
    pub fn new(hwnd: isize) -> Result<Self> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(HWND(hwnd as *mut _))? };
        let size = item.Size()?;
        let mut device = None;
        let mut context = None;
        let hardware = unsafe {
            D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION,
                Some(&mut device), None, Some(&mut context))
        };
        if hardware.is_err() {
            unsafe {
                D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION,
                    Some(&mut device), None, Some(&mut context))?;
            }
        }
        let device = device.ok_or_else(windows_core::Error::empty)?;
        let context = context.ok_or_else(windows_core::Error::empty)?;
        let dxgi: IDXGIDevice = device.cast()?;
        let winrt_device: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        // Wake on completed GPU frames instead of polling with thread::sleep.
        // Windows timer rounding and two independent 60 Hz clocks caused repeats
        // and uneven updates even when the scene itself was rendering smoothly.
        let (sender, arrived) = std::sync::mpsc::sync_channel(1);
        let arrived_token = pool.FrameArrived(&TypedEventHandler::new(move |_, _| {
            let _ = sender.try_send(());
            Ok(())
        }))?;
        let capture = Self { source_hwnd: hwnd, layout_notification_pending: true,
            pool, session, winrt_device, device, context, size, textures: None,
            arrived, arrived_token };
        capture.session.SetIsCursorCaptureEnabled(false)?;
        capture.session.SetIsBorderRequired(false)?;
        let _ = capture.session.SetIncludeSecondaryWindows(false);
        let _ = capture.session.SetMinUpdateInterval(TimeSpan { Duration: 166_667 });
        capture.session.StartCapture()?;
        Ok(capture)
    }

    pub fn dimensions(&self) -> (i32, i32) { (self.size.Width, self.size.Height) }

    pub fn next(&mut self) -> Result<Option<Vec<u8>>> {
        let Ok(mut frame) = self.pool.TryGetNextFrame() else { return Ok(None) };
        for _ in 0..2 {
            if let Ok(newer) = self.pool.TryGetNextFrame() {
                let _ = frame.Close();
                frame = newer;
            } else { break; }
        }
        let size = frame.ContentSize();
        let size = match size {
            Ok(size) if size.Width > 0 && size.Height > 0 => size,
            Ok(_) => { let _ = frame.Close(); return Ok(None); }
            Err(error) => { let _ = frame.Close(); return Err(error); }
        };
        if size.Width != self.size.Width || size.Height != self.size.Height {
            // Release the old frame before recreating its pool. Keep the published
            // frame until a complete frame at the new window size arrives.
            let _ = frame.Close();
            self.pool.Recreate(&self.winrt_device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
            self.size = size;
            self.textures = None;
            self.layout_notification_pending = true;
            return Ok(None);
        }
        let result = self.read_frame(&frame);
        let _ = frame.Close();
        if matches!(&result, Ok(Some(_))) && self.layout_notification_pending {
            self.layout_notification_pending = false;
            // The resize request and the external window's location event can both
            // precede its new GPU surface. Refresh DWM once that surface is real.
            super::we_source_layout_changed(self.source_hwnd);
        }
        result
    }

    fn read_frame(&mut self, frame: &Direct3D11CaptureFrame) -> Result<Option<Vec<u8>>> {
        let size = self.size;
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        let source: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { source.GetDesc(&mut desc); }
        if desc.Width != size.Width as u32 || desc.Height != size.Height as u32 { return Ok(None); }
        if self.textures.is_none() { self.textures = Some(self.create_textures(&desc)?); }
        let textures = self.textures.as_ref().unwrap();
        unsafe {
            // Downscale on the GPU, then read only a small mip level. This avoids
            // allocating or transferring a full-resolution bitmap on every tick.
            self.context.CopySubresourceRegion(&textures.mipmap, 0, 0, 0, 0, &source, 0, None);
            self.context.GenerateMips(&textures.view);
            self.context.CopySubresourceRegion(&textures.staging, 0, 0, 0, 0,
                &textures.mipmap, textures.level, None);
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { self.context.Map(&textures.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?; }
        let output = if mapped.pData.is_null() { None } else {
            let bytes = unsafe { std::slice::from_raw_parts(mapped.pData as *const u8,
                mapped.RowPitch as usize * textures.height) };
            Some(resample(bytes, mapped.RowPitch as usize, textures.width, textures.height))
        };
        unsafe { self.context.Unmap(&textures.staging, 0); }
        Ok(output)
    }

    fn create_textures(&self, source: &D3D11_TEXTURE2D_DESC) -> Result<Textures> {
        let mut desc = D3D11_TEXTURE2D_DESC {
            Width: source.Width, Height: source.Height, MipLevels: 0, ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0, MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let mut mipmap = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut mipmap))?; }
        let mipmap = mipmap.ok_or_else(windows_core::Error::empty)?;
        let mut view = None;
        unsafe { self.device.CreateShaderResourceView(&mipmap, None, Some(&mut view))?; }
        let view = view.ok_or_else(windows_core::Error::empty)?;
        let mut level = 0;
        while (source.Width >> (level + 1)) >= WIDTH as u32
            && (source.Height >> (level + 1)) >= HEIGHT as u32 { level += 1; }
        desc.Width = source.Width >> level;
        desc.Height = source.Height >> level;
        desc.MipLevels = 1;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut staging))?; }
        Ok(Textures { mipmap, view, staging: staging.ok_or_else(windows_core::Error::empty)?, level,
            width: desc.Width as usize, height: desc.Height as usize })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.arrived_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

fn resample(bytes: &[u8], pitch: usize, width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; 8 + WIDTH * HEIGHT * 4];
    output[..4].copy_from_slice(&(WIDTH as u32).to_le_bytes());
    output[4..8].copy_from_slice(&(HEIGHT as u32).to_le_bytes());
    for y in 0..HEIGHT {
        let sy = ((y as f32 + 0.5) * height as f32 / HEIGHT as f32 - 0.5).clamp(0.0, (height - 1) as f32);
        let y0 = sy as usize; let y1 = (y0 + 1).min(height - 1); let fy = sy - y0 as f32;
        for x in 0..WIDTH {
            let sx = ((x as f32 + 0.5) * width as f32 / WIDTH as f32 - 0.5).clamp(0.0, (width - 1) as f32);
            let x0 = sx as usize; let x1 = (x0 + 1).min(width - 1); let fx = sx - x0 as f32;
            let dst = 8 + (y * WIDTH + x) * 4;
            for (dc, sc) in [(0, 2), (1, 1), (2, 0)] {
                let top = bytes[y0 * pitch + x0 * 4 + sc] as f32 * (1.0 - fx)
                    + bytes[y0 * pitch + x1 * 4 + sc] as f32 * fx;
                let bottom = bytes[y1 * pitch + x0 * 4 + sc] as f32 * (1.0 - fx)
                    + bytes[y1 * pitch + x1 * 4 + sc] as f32 * fx;
                output[dst + dc] = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
            output[dst + 3] = 255;
        }
    }
    output
}

type Source = (u64, isize);

pub enum FrameStatus {
    Ready(Vec<u8>),
    Pending,
    Unavailable,
}

#[derive(Default)]
struct SharedState {
    requested: Option<Source>,
    /// When the page last asked for a frame: with nobody asking (no glass on screen), the
    /// capture session is closed, since while it is open DWM renders the window for it.
    last_request: Option<Instant>,
    frame: Option<(Source, Arc<[u8]>)>,
    unavailable: Option<Source>,
    failed: bool,
}

struct Worker {
    state: Arc<Mutex<SharedState>>,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

pub fn frame(source: Source) -> FrameStatus {
    let worker = WORKER.get_or_init(|| {
        let state = Arc::new(Mutex::new(SharedState::default()));
        let thread_state = Arc::clone(&state);
        if std::thread::Builder::new()
            .name("wallpaper-capture".into())
            .spawn(move || run(thread_state))
            .is_err()
        {
            state.lock().unwrap_or_else(|e| e.into_inner()).failed = true;
        }
        Worker { state }
    });
    let (bytes, unavailable) = {
        let mut state = worker.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.requested != Some(source) {
            state.requested = Some(source);
            state.frame = None;
            state.unavailable = None;
        }
        state.last_request = Some(Instant::now());
        let bytes = state.frame.as_ref()
            .filter(|(key, _)| *key == source)
            .map(|(_, bytes)| Arc::clone(bytes));
        (bytes, state.failed || state.unavailable == Some(source))
    };
    // A temporary capture/device error must not replace a completed WGC frame
    // with the older PrintWindow path while the worker recovers.
    if let Some(bytes) = bytes {
        FrameStatus::Ready(bytes.to_vec())
    } else if unavailable {
        FrameStatus::Unavailable
    } else {
        FrameStatus::Pending
    }
}

pub fn pause() {
    if let Some(worker) = WORKER.get() {
        let mut state = worker.state.lock().unwrap_or_else(|e| e.into_inner());
        state.requested = None;
        state.frame = None;
        state.unavailable = None;
    }
}

struct Runtime;

impl Runtime {
    fn new() -> Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)?; }
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) { unsafe { RoUninitialize(); } }
}

fn run(state: Arc<Mutex<SharedState>>) {
    let _runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            log::warn!("No se pudo iniciar la captura gráfica del fondo: {error}");
            state.lock().unwrap_or_else(|e| e.into_inner()).failed = true;
            return;
        }
    };
    let mut source = None;
    let mut capture: Option<Capture> = None;
    let mut retry_at = Instant::now();
    loop {
        let tick = Instant::now();
        let requested = {
            let shared = state.lock().unwrap_or_else(|e| e.into_inner());
            let idle = shared.last_request.map_or(true, |t| tick.duration_since(t) > Duration::from_secs(1));
            if idle { None } else { shared.requested }
        };
        if requested != source {
            capture = None;
            source = requested;
            retry_at = tick;
        }
        if let Some(key) = source {
            let hwnd = HWND(key.1 as *mut _);
            let visible = unsafe { IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() };
            if !visible {
                capture = None;
                retry_at = tick;
            } else {
                if capture.is_none() && tick >= retry_at {
                    match Capture::new(key.1) {
                        Ok(new_capture) => {
                            log::debug!("Captura gráfica del fondo iniciada: {:?}", new_capture.dimensions());
                            capture = Some(new_capture);
                        }
                        Err(error) => {
                            mark_unavailable(&state, key, &error);
                            retry_at = tick + Duration::from_secs(2);
                        }
                    }
                }
                if let Some(active) = capture.as_mut() {
                    match active.next() {
                        Ok(Some(bytes)) => {
                            let bytes: Arc<[u8]> = bytes.into();
                            let mut shared = state.lock().unwrap_or_else(|e| e.into_inner());
                            if shared.requested == Some(key) {
                                shared.frame = Some((key, bytes));
                                shared.unavailable = None;
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            mark_unavailable(&state, key, &error);
                            capture = None;
                            retry_at = tick + Duration::from_secs(2);
                        }
                    }
                }
            }
        }
        if let Some(active) = capture.as_ref() {
            // A single notification is retained while readback is in progress;
            // next() drains to the newest frame. Timeout keeps source changes,
            // minimize and idle cleanup responsive even if rendering stops.
            let _ = active.arrived.recv_timeout(Duration::from_millis(100));
        } else {
            std::thread::sleep(Duration::from_millis(100).saturating_sub(tick.elapsed()));
        }
    }
}

fn mark_unavailable(state: &Mutex<SharedState>, source: Source, error: &windows_core::Error) {
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    if state.requested == Some(source) {
        if state.unavailable != Some(source) {
            log::warn!("Captura gráfica del fondo no disponible; se conserva la última imagen: {error}");
        }
        state.unavailable = Some(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_bgra_rows_become_opaque_rgba_without_sampling_padding() {
        let source = [
            10, 20, 90, 0, 30, 40, 110, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            50, 60, 130, 0, 70, 80, 150, 0, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let output = resample(&source, 16, 2, 2);
        assert_eq!(&output[..4], &(WIDTH as u32).to_le_bytes());
        assert_eq!(&output[4..8], &(HEIGHT as u32).to_le_bytes());
        assert_eq!(&output[8..12], &[90, 20, 10, 255]);
        assert_eq!(&output[output.len() - 4..], &[150, 80, 70, 255]);
        assert!(output[8..].chunks_exact(4).all(|pixel| pixel[3] == 255 && pixel[..3].iter().all(|v| *v < 200)));
    }

    #[test]
    fn single_pixel_surface_fills_every_output_pixel() {
        let output = resample(&[9, 31, 117, 0], 4, 1, 1);
        assert_eq!(output.len(), 8 + WIDTH * HEIGHT * 4);
        assert!(output[8..].chunks_exact(4).all(|pixel| pixel == [117, 31, 9, 255]));
    }
}

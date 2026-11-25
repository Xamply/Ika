use windows::{
    Win32::Graphics::DirectComposition::{
        DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR, DCompositionCreateDevice,
        IDCompositionDevice, IDCompositionRectangleClip, IDCompositionTarget, IDCompositionVisual,
    },
    core::Interface,
};
use windows_numerics::Matrix3x2;
use winit::window::Window;

pub struct VideoPlanes {
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    root: IDCompositionVisual,
    clip: IDCompositionRectangleClip,
    video: IDCompositionVisual,
    ui: IDCompositionVisual,
    profile: IDCompositionVisual,
    pub surface: wgpu::Surface<'static>,
    profile_surface: wgpu::Surface<'static>,
    profile_size: [u32; 2],
    profile_presented: Option<([u32; 4], u64)>,
    video_size: [u32; 2],
    view_size: [u32; 2],
}

impl VideoPlanes {
    pub fn new(
        instance: &wgpu::Instance,
        window: &Window,
    ) -> crate::Result<(Self, wgpu::Surface<'static>)> {
        let hwnd = crate::titlebar::window_handle(window)?;
        // The visual targets retain both composition swapchains; the window
        // itself is owned by the winit session.
        unsafe {
            let device: IDCompositionDevice = DCompositionCreateDevice(None)?;
            let target = device.CreateTargetForHwnd(hwnd, true)?;
            let root = device.CreateVisual()?;
            let clip = device.CreateRectangleClip()?;
            let video = device.CreateVisual()?;
            let ui = device.CreateVisual()?;
            let profile = device.CreateVisual()?;
            root.SetClip(&clip)?;
            root.AddVisual(&video, false, None)?;
            root.AddVisual(&ui, true, &video)?;
            target.SetRoot(&root)?;
            video.SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)?;
            let surface = instance.create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::CompositionVisual(video.as_raw()),
            )?;
            let overlay = instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(ui.as_raw()))?;
            let profile_surface = instance.create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::CompositionVisual(profile.as_raw()),
            )?;
            Ok((
                Self {
                    device,
                    _target: target,
                    root,
                    clip,
                    video,
                    ui,
                    profile,
                    surface,
                    profile_surface,
                    profile_size: [0; 2],
                    profile_presented: None,
                    video_size: [0; 2],
                    view_size: [0; 2],
                },
                overlay,
            ))
        }
    }

    pub fn resize(&mut self, size: [u32; 2]) -> crate::Result<()> {
        self.view_size = size;
        unsafe {
            self.clip.SetRight2(size[0] as f32)?;
            self.clip.SetBottom2(size[1] as f32)?;
        }
        self.commit()
    }

    pub fn configure(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        source_size: [u32; 2],
    ) -> crate::Result<()> {
        if self.video_size != source_size {
            self.surface.configure(
                device,
                &wgpu::SurfaceConfiguration {
                    width: source_size[0],
                    height: source_size[1],
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                    view_formats: vec![],
                    ..config.clone()
                },
            );
            self.video_size = source_size;
            self.commit()?;
            println!(
                "DirectComposition video: {}x{}",
                source_size[0], source_size[1]
            );
        }
        Ok(())
    }

    pub fn commit(&self) -> crate::Result<()> {
        let [w, h] = self.view_size.map(|n| n as f32);
        let [vw, vh] = self.video_size.map(|n| n as f32);
        unsafe {
            if vw > 0.0 && vh > 0.0 {
                let s = (w / vw).max(h / vh);
                self.video.SetTransform2(&Matrix3x2 {
                    M11: s,
                    M12: 0.0,
                    M21: 0.0,
                    M22: s,
                    M31: (w - vw * s) * 0.5,
                    M32: (h - vh * s) * 0.5,
                })?;
            }
            self.device.Commit()?;
        }
        Ok(())
    }

    pub fn acquire_profile(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        region: [u32; 4],
        stamp: u64,
    ) -> crate::Result<Option<wgpu::SurfaceTexture>> {
        if self.profile_presented == Some((region, stamp)) {
            return Ok(None);
        }
        let [_, _, width, height] = region;
        let setup = |surface: &wgpu::Surface<'_>| {
            surface.configure(
                device,
                &wgpu::SurfaceConfiguration {
                    width,
                    height,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    ..config.clone()
                },
            );
        };
        if self.profile_size != [width, height] {
            setup(&self.profile_surface);
            self.profile_size = [width, height];
        }
        let mut swap = self.profile_surface.get_current_texture();
        if matches!(swap, wgpu::CurrentSurfaceTexture::Outdated) {
            setup(&self.profile_surface);
            swap = self.profile_surface.get_current_texture();
        }
        match swap {
            wgpu::CurrentSurfaceTexture::Success(swap)
            | wgpu::CurrentSurfaceTexture::Suboptimal(swap) => Ok(Some(swap)),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => Ok(None),
            _ => Err("Se perdió la capa del perfil".into()),
        }
    }

    pub fn present_profile(
        &mut self,
        swap: wgpu::SurfaceTexture,
        region: [u32; 4],
        stamp: u64,
    ) -> crate::Result<()> {
        swap.present();
        if self.profile_presented.is_none_or(|(old, _)| old != region) {
            unsafe {
                self.profile.SetOffsetX2(region[0] as f32)?;
                self.profile.SetOffsetY2(region[1] as f32)?;
                if self.profile_presented.is_none() {
                    self.root.AddVisual(&self.profile, true, &self.ui)?;
                }
                self.device.Commit()?;
            }
        }
        self.profile_presented = Some((region, stamp));
        Ok(())
    }

    pub fn hide_profile(&mut self) -> crate::Result<()> {
        if self.profile_presented.is_some() {
            unsafe {
                self.root.RemoveVisual(&self.profile)?;
                self.device.Commit()?;
            }
            self.profile_presented = None;
        }
        Ok(())
    }
}

mod bridge_signal;
mod crt;
mod dock;
#[path = "dock/preview.rs"]
mod dock_preview;
mod frame_cache;
mod gpu;
mod home;
mod home_widgets;
mod minecraft;
mod profile;
mod resident_memory;
mod scene;
mod shared;
mod startup;
mod titlebar;
mod wallpapers;

use bytemuck::Zeroable;
use gpu::{Gpu, Params};
use scene::Scene;
use std::{
    error::Error,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::ScreenToClient,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyState, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
        },
        WindowsAndMessaging::GetCursorPos,
    },
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{CursorIcon, Fullscreen, Window, WindowId},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const COMPOSITOR_FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

fn control_is_down(session: &Session) -> bool {
    session.ctrl
        || unsafe {
            GetKeyState(VK_CONTROL.0 as i32) < 0
                || GetKeyState(VK_LCONTROL.0 as i32) < 0
                || GetKeyState(VK_RCONTROL.0 as i32) < 0
                || GetAsyncKeyState(VK_CONTROL.0 as i32) < 0
                || GetAsyncKeyState(VK_LCONTROL.0 as i32) < 0
                || GetAsyncKeyState(VK_RCONTROL.0 as i32) < 0
        }
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut package = None;
    let mut proof = None;
    let mut home_proof = false;
    let mut source = None;
    let mut source_folder = None;
    let mut pending_source = None;
    let mut switch = None;
    let mut title = None;
    let mut trim = None;
    let mut fullscreen = false;
    while let Some(arg) = args.next() {
        if (arg == "--proof-dir" || arg == "--home-proof") && proof.is_none() {
            home_proof = arg == "--home-proof";
            proof = Some(PathBuf::from(
                args.next().ok_or("Falta la carpeta de verificación")?,
            ));
        } else if arg == "--shared-source" && source.is_none() && pending_source.is_none() {
            let path = PathBuf::from(args.next().ok_or("Faltan los metadatos de la fuente")?);
            source = Some(shared::Source::read(&path)?);
            source_folder = path.parent().map(PathBuf::from);
        } else if arg == "--await-source" && source.is_none() && pending_source.is_none() {
            pending_source = Some(PathBuf::from(
                args.next().ok_or("Faltan los metadatos de la fuente")?,
            ));
            source_folder = pending_source
                .as_deref()
                .and_then(std::path::Path::parent)
                .map(PathBuf::from);
        } else if arg == "--switch-dir" && switch.is_none() {
            switch = Some(PathBuf::from(
                args.next().ok_or("Falta carpeta de control")?,
            ));
        } else if arg == "--title" && title.is_none() {
            title = Some(
                args.next()
                    .ok_or("Falta el nombre del fondo")?
                    .to_string_lossy()
                    .into_owned(),
            );
        } else if arg == "--trim-viewer-once" {
            trim = Some(resident_memory::WorkingSet::new());
        } else if arg == "--fullscreen" {
            fullscreen = true;
        } else if package.is_none() {
            package = Some(PathBuf::from(arg));
        } else {
            return Err("Uso: ika-wgpu-wallpaper-bridge [archivo.pkg | --shared-source archivo | --await-source archivo] [--switch-dir carpeta] [--proof-dir carpeta] [--fullscreen]".into());
        }
    }
    let shared = source.is_some() || pending_source.is_some();
    if shared && package.is_some() {
        return Err("Elige una sola fuente de Wallpaper Engine".into());
    }
    if switch.is_some() && !shared {
        return Err("El selector requiere una textura compartida".into());
    }
    if pending_source.is_some() && proof.is_some() {
        return Err("--proof-dir requiere que la fuente ya esté lista".into());
    }
    let path = package.unwrap_or_else(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("media/3608457742/scene.pkg")
    });
    if let Some(dir) = &proof {
        std::fs::create_dir_all(dir)?;
    }
    let mut scene = if let Some(source) = source {
        Scene::shared(source.size)
    } else if pending_source.is_some() {
        let mut placeholder = Scene::shared([1, 1]);
        placeholder.texture = vec![0, 0, 0, 255];
        placeholder
    } else if home_proof {
        let mut placeholder = Scene::shared([1, 1]);
        placeholder.texture = vec![31, 23, 55, 255];
        placeholder
    } else {
        Scene::open(&path)?
    };
    if shared {
        if let Some(title) = title {
            scene.title = title;
        }
        if source.is_some() {
            println!("Fuente activa: {}", scene.title);
        }
    } else if scene.motion.is_some() {
        println!(
            "{}: textura y máscaras originales; efectos portados en WGSL",
            scene.title
        );
    } else {
        println!(
            "{}: {} cuadros del paquete, bucle de {:.2} s",
            scene.title,
            scene.frames.len(),
            scene.period
        );
    }
    let proof = proof.map(|dir| {
        if home_proof {
            (dir, Proof::Home(0))
        } else if switch.is_some() {
            (dir.join("0"), Proof::Warm(Instant::now()))
        } else {
            (dir, Proof::Before)
        }
    });
    let event_loop = EventLoop::<()>::with_user_event().build()?;
    let bridge_folder = switch.clone().or(source_folder);
    let wake = bridge_folder
        .as_deref()
        .and_then(|folder| bridge_signal::Wake::connect(folder, event_loop.create_proxy()));
    let mut app = App {
        epoch: Instant::now(),
        scene,
        source,
        pending_source,
        bridge_folder,
        wake,
        selector: switch.map(|folder| Selector {
            folder,
            generation: 0,
            state: Switch::Idle,
            active: 0,
            choice: None,
            queued: None,
            preview: None,
            preview_serial: 0,
            allow_cached_preview: false,
        }),
        session: None,
        frame_wake: bridge_signal::FrameWake::new(event_loop.create_proxy()).ok(),
        event_proxy: event_loop.create_proxy(),
        proof,
        trim,
        fullscreen,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

fn clipboard_text() -> Option<String> {
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        },
    };
    // CF_UNICODETEXT is a null-terminated UTF-16 string in a movable HGLOBAL.
    unsafe {
        OpenClipboard(None).ok()?;
        let text = GetClipboardData(13).ok().and_then(|data| {
            let handle = HGLOBAL(data.0);
            let len = (GlobalSize(handle) / 2).min(2048);
            let ptr = GlobalLock(handle).cast::<u16>();
            if ptr.is_null() {
                return None;
            }
            let units = std::slice::from_raw_parts(ptr, len);
            let end = units.iter().position(|&unit| unit == 0).unwrap_or(len);
            let text = String::from_utf16_lossy(&units[..end]);
            let _ = GlobalUnlock(handle);
            Some(text)
        });
        let _ = CloseClipboard();
        text
    }
}

enum Proof {
    Home(u8),
    HomeDone,
    Warm(Instant),
    Before,
    WaitingForClick,
    After,
    Moving(Instant),
    Done,
}

struct Selector {
    folder: PathBuf,
    generation: u32,
    state: Switch,
    active: usize,
    choice: Option<usize>,
    queued: Option<(Option<usize>, bool)>,
    preview: Option<i32>,
    preview_serial: u32,
    allow_cached_preview: bool,
}

impl Selector {
    fn prepare(&mut self, preview: Option<i32>, wake: &bridge_signal::Wake) {
        if self.preview == preview {
            return;
        }
        self.preview = preview;
        self.preview_serial += 1;
        let temp = self.folder.join("prepare.tmp");
        if std::fs::write(
            &temp,
            format!("{} {}", self.preview_serial, preview.unwrap_or(-2)),
        )
        .and_then(|()| std::fs::rename(temp, self.folder.join("prepare.txt")))
        .is_ok()
        {
            wake.request();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Switch {
    Idle,
    PinFrame,
    Waiting,
}

const WALLPAPER_FADE: Duration = Duration::from_millis(110);

struct WallpaperFade {
    aspect: f32,
    first_present: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq)]
enum Button {
    Blur,
    Next,
    Chrome(titlebar::Hit),
    Dock(usize),
    Home(home::Hit),
    Wallpapers(wallpapers::Hit),
}

struct App {
    scene: Scene,
    source: Option<shared::Source>,
    pending_source: Option<PathBuf>,
    bridge_folder: Option<PathBuf>,
    wake: Option<bridge_signal::Wake>,
    selector: Option<Selector>,
    epoch: Instant,
    session: Option<Session>,
    frame_wake: Option<bridge_signal::FrameWake>,
    event_proxy: winit::event_loop::EventLoopProxy<()>,
    proof: Option<(PathBuf, Proof)>,
    trim: Option<resident_memory::WorkingSet>,
    fullscreen: bool,
    error: Option<Box<dyn Error>>,
}

struct Session {
    window: Arc<Window>,
    gpu: Gpu,
    cursor: Option<PhysicalPosition<f64>>,
    down: Option<Button>,
    has_selector: bool,
    fullscreen: bool,
    titlebar: titlebar::Titlebar,
    dock: dock::Dock,
    dock_preview: dock_preview::Preview,
    home: Option<home::Home>,
    widgets: Option<home_widgets::Widgets>,
    profile: profile::Profile,
    wallpapers: wallpapers::Wallpapers,
    blur: bool,
    running_instance: Option<String>,
    game_minimized: bool,
    minimize_on_play: bool,
    refresh_instances: bool,
    occluded: bool,
    shift: bool,
    ctrl: bool,
    last_frame: Option<usize>,
    // Enabled by the GPU benchmark only. Count successful presents, not source frames.
    present_stats: Option<(Instant, u32)>,
    shown: bool,
    reduced_motion: bool,
    startup: Option<startup::Startup>,
    wallpaper_fade: Option<WallpaperFade>,
    cached_aspect: Option<f32>,
    audio: Option<bridge_signal::Audio>,
    last_click: Option<(Instant, PhysicalPosition<f64>)>,
}

impl Session {
    fn show(&mut self) -> Result<()> {
        if !self.shown {
            self.gpu.wait_for_draw()?;
            titlebar::reveal_startup(&self.window)?;
            self.shown = true;
            self.occluded = false;
        }
        Ok(())
    }

    fn tool_scale(&self) -> f32 {
        let size = self.window.inner_size();
        (self.window.scale_factor() as f32)
            .min(size.width as f32 / 620.0)
            .min(size.height as f32 / 560.0)
            .max(f32::EPSILON)
    }

    fn glass(&self) -> [f32; 4] {
        let size = self.window.inner_size();
        let scale = self.tool_scale();
        [
            (44.0 * scale).round(),
            ((size.height as f32 - 235.0 * scale) * 0.5).round(),
            544.0 * scale,
            158.0 * scale,
        ]
    }

    fn button(&self, button: Button) -> [f32; 4] {
        if button == Button::Next && !self.has_selector {
            return [0.0; 4];
        }
        let [x, y, panel_width, _] = self.glass();
        let scale = self.tool_scale();
        let width = if button == Button::Blur { 178.0 } else { 222.0 } * scale;
        let left = if self.has_selector {
            x + (panel_width - 412.0 * scale) * 0.5
                + if button == Button::Next {
                    190.0 * scale
                } else {
                    0.0
                }
        } else {
            x + (panel_width - width) * 0.5
        };
        [
            left.round(),
            (y + 186.0 * scale).round(),
            width,
            49.0 * scale,
        ]
    }

    fn home_active(&self) -> bool {
        self.titlebar.on_home() && self.home.is_some()
    }

    fn console_view(&self) -> profile::ConsoleView {
        if self.home.is_none() || self.wallpapers.visible() || self.titlebar.picker.is_some()
            || self.titlebar.notification_open || self.startup.as_ref().is_some_and(startup::Startup::loading)
        {
            return profile::ConsoleView::Hidden;
        }
        if self.titlebar.on_home() {
            profile::ConsoleView::Home
        } else if self.titlebar.on_console() {
            profile::ConsoleView::Detail
        } else {
            profile::ConsoleView::Hidden
        }
    }

    fn widgets_interactive(&self) -> bool {
        self.home_active() && self.widgets.is_some() && self.profile.panel.is_none()
            && !self.wallpapers.visible() && self.titlebar.picker.is_none()
            && !self.titlebar.notification_open
    }

    fn cursor_position(&self) -> Option<PhysicalPosition<f64>> {
        self.cursor.or_else(|| {
            let mut point = POINT::default();
            unsafe {
                GetCursorPos(&mut point).ok()?;
                ScreenToClient(titlebar::window_handle(&self.window).ok()?, &mut point)
                    .as_bool()
                    .then(|| PhysicalPosition::new(point.x as f64, point.y as f64))
            }
        })
    }

    fn unobstructed_chrome(&self, p: PhysicalPosition<f64>) -> Option<Button> {
        if !self.profile.exposes_chrome_at(&self.window,p) {
            return None;
        }
        self.titlebar.hit(p,&self.window,!self.fullscreen,Instant::now()).map(Button::Chrome)
            .or_else(||self.home_active().then(||dock::hit(&self.window,p).map(Button::Dock)).flatten())
    }

    fn hovered(&self) -> Option<Button> {
        let p = self.cursor?;
        if self.profile.panel.is_some() {
            return self.unobstructed_chrome(p);
        }
        if self.wallpapers.visible() {
            return Some(Button::Wallpapers(self.wallpapers.hit(
                &self.window,
                p,
                Instant::now(),
                self.audio.is_some(),
            )));
        }
        if let Some(hit) = self
            .titlebar
            .hit(p, &self.window, !self.fullscreen, Instant::now())
        {
            return Some(Button::Chrome(hit));
        }
        if self.home_active() {
            if self.profile.console_covers(&self.window, p) || self.profile.oneko_covers(&self.window, p) {
                return None;
            }
            if self.widgets_interactive()
                && let (Some(widgets), Some(home)) = (&self.widgets, &self.home)
                && widgets.blocks(&self.window, p, Instant::now(), home)
            {
                return None;
            }
            return self
                .home
                .as_ref()
                .and_then(|h| h.hit(&self.window, p, Instant::now()).map(Button::Home))
                .or_else(|| dock::hit(&self.window, p).map(Button::Dock));
        }
        if self.titlebar.on_console() {
            return dock::hit(&self.window, p).map(Button::Dock);
        }
        [Button::Blur, Button::Next]
            .into_iter()
            .find(|&button| {
                let [left, top, w, h] = self.button(button).map(f64::from);
                p.x >= left && p.x < left + w && p.y >= top && p.y < top + h
            })
            .or_else(|| dock::hit(&self.window, p).map(Button::Dock))
    }
}

impl App {
    fn open(&mut self, events: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            events.create_window(
                Window::default_attributes()
                    .with_title(format!("Ika wgpu - {}", self.scene.title))
                    .with_decorations(false)
                    .with_inner_size(LogicalSize::new(1152, 648))
                    .with_min_inner_size(LogicalSize::new(
                        if self.selector.is_some() { 440 } else { 300 },
                        360,
                    ))
                    .with_transparent(true)
                    .with_visible(false),
            )?,
        );
        let hwnd = titlebar::window_handle(&window)?;
        // On Windows winit maximizes with ShowWindow(SW_MAXIMIZE), which
        // exposes even a window created with visible=false. Cloak it first.
        titlebar::stage_startup(&window)?;
        if let Some(folder) = &self.bridge_folder {
            let _ = std::fs::write(folder.join("viewer-hwnd.txt"), format!("{}", hwnd.0 as usize));
        }
        if self.fullscreen {
            window.set_fullscreen(Some(Fullscreen::Borderless(events.primary_monitor())));
        } else {
            titlebar::install(&window)?;
            window.set_maximized(true);
        }
        let gpu = pollster::block_on(Gpu::new(
            window.clone(),
            &self.scene,
            self.proof.is_some(),
            self.source,
        ))?;
        window.set_visible(true);
        let now = Instant::now();
        let reduced_motion = titlebar::reduced_motion();
        let home = (self.proof.is_none() || matches!(self.proof, Some((_, Proof::Home(_)))))
            .then(|| home::Home::new(self.event_proxy.clone(), reduced_motion));
        let mut profile = profile::Profile::new(self.event_proxy.clone(), self.proof.is_none(), reduced_motion);
        profile.init_cursor(events);
        self.session = Some(Session {
            window: window.clone(),
            gpu,
            cursor: None,
            down: None,
            has_selector: self.selector.is_some(),
            fullscreen: self.fullscreen,
            titlebar: titlebar::Titlebar::new(now),
            dock: dock::Dock::new(now),
            dock_preview: dock_preview::Preview::default(),
            home,
            widgets: self.proof.is_none().then(|| home_widgets::Widgets::new(now, reduced_motion)),
            profile,
            wallpapers: wallpapers::Wallpapers::new(now),
            blur: false,
            running_instance: None,
            game_minimized: false,
            minimize_on_play: profile::game_window_mode()==1,
            refresh_instances: false,
            occluded: false,
            shift: false,
            ctrl: false,
            last_frame: None,
            present_stats: std::env::var_os("IKA_PRESENT_STATS").map(|_| (now, 0)),
            shown: false,
            reduced_motion,
            startup: self
                .proof
                .is_none()
                .then(|| startup::Startup::new(now, reduced_motion)),
            wallpaper_fade: None,
            cached_aspect: None,
            audio: self
                .bridge_folder
                .as_deref()
                .and_then(|dir| bridge_signal::Audio::open(dir, "0")),
            last_click: None,
        });
        self.epoch = Instant::now();
        // The original atlas has been uploaded; the process does not stream new frames.
        self.scene.texture = Vec::new();
        if let Some(effects) = &mut self.scene.motion {
            effects.masks = Vec::new();
        }
        window.request_redraw();
        Ok(())
    }

    fn toggle(&mut self) {
        let session = self.session.as_mut().unwrap();
        session.blur = !session.blur;
        println!("Blur: {}", if session.blur { "ON" } else { "OFF" });
        if let Some((_, stage @ Proof::WaitingForClick)) = &mut self.proof {
            *stage = Proof::After;
        }
        session.window.request_redraw();
    }

    fn next(&mut self) {
        self.switch_to(None);
    }

    fn switch_to(&mut self, choice: Option<usize>) {
        self.switch_to_source(choice, false);
    }

    fn switch_to_source(&mut self, choice: Option<usize>, force: bool) {
        let Some(selector) = self.selector.as_mut() else {
            return;
        };
        if selector.state != Switch::Idle {
            selector.queued = Some((choice, force));
            return;
        }
        if !force && choice == Some(selector.active) {
            selector.queued = None;
            return;
        }
        let session = self.session.as_mut().unwrap();
        if session.wallpaper_fade.take().is_some() {
            session.gpu.finish_wallpaper_fade();
        }
        if let Some(audio) = &mut session.audio {
            audio.set(false);
        }
        selector.generation += 1;
        selector.state = Switch::PinFrame;
        selector.choice = choice;
        selector.allow_cached_preview = !force && self.proof.is_none();
        session.window.set_title("Ika wgpu - Cambiando fondo...");
        session.window.request_redraw();
    }

    fn open_wallpapers(&mut self) -> Result<()> {
        let session = self.session.as_mut().unwrap();
        if !session.fullscreen {
            titlebar::set_client_mode(&session.window, true)?;
        }
        session.titlebar.notification_open = false;
        let folder = self.selector.as_ref().map(|s| s.folder.as_path());
        session.wallpapers.open(folder, &self.scene.title, Instant::now())?;
        session.window.request_redraw();
        Ok(())
    }

    fn activate_wallpapers(&mut self, hit: wallpapers::Hit) {
        let active = self.selector.as_ref().map_or(0, |s| s.active);
        let session = self.session.as_mut().unwrap();
        let action = session.wallpapers.activate(hit, active, Instant::now());
        match action {
            wallpapers::Action::None => (),
            wallpapers::Action::Audio => {
                if let Some(audio) = session.audio.as_mut().filter(|_| {
                    self.selector
                        .as_ref()
                        .is_none_or(|s| s.state == Switch::Idle)
                }) {
                    audio.set(!audio.playing);
                }
                session.window.request_redraw();
            }
            wallpapers::Action::Apply(index) => self.switch_to(Some(index)),
            wallpapers::Action::Open(url) => {
                if url.starts_with("https://steamcommunity.com/sharedfiles/filedetails/?id=") {
                    let wide = windows::core::HSTRING::from(url);
                    dock::open_url(windows::core::PCWSTR(wide.as_ptr()));
                }
            }
        }
    }

    fn prepare_selector(
        selector: &mut Option<Selector>,
        wake: &Option<bridge_signal::Wake>,
        preview: Option<i32>,
    ) {
        if let (Some(selector), Some(wake)) = (selector, wake) {
            selector.prepare(preview, wake);
        }
    }

    fn hover_preview(hovered: Option<Button>) -> Option<i32> {
        match hovered {
            Some(Button::Next) => Some(-1),
            _ => None,
        }
    }

    fn poll_switch(&mut self) -> Result<()> {
        let Some(selector) = self
            .selector
            .as_mut()
            .filter(|s| s.state == Switch::Waiting)
        else {
            return Ok(());
        };
        let id = selector.generation;
        let title = match std::fs::read_to_string(selector.folder.join(format!("ready-{id}.txt"))) {
            Ok(title) => title,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                if selector.allow_cached_preview
                    && let Ok(file) =
                        std::fs::read_to_string(selector.folder.join(format!("preview-{id}.txt")))
                {
                    selector.allow_cached_preview = false;
                    let path = std::path::Path::new(&file);
                    match frame_cache::CachedFrame::read(path) {
                        Ok(frame) => {
                            let session = self.session.as_mut().unwrap();
                            let fade = !session.reduced_motion;
                            session.gpu.show_cached_frame(&frame, fade);
                            session.cached_aspect =
                                Some(frame.size.width as f32 / frame.size.height as f32);
                            session.wallpaper_fade = fade.then_some(WallpaperFade {
                                aspect: self.scene.frame_aspect,
                                first_present: None,
                            });
                            session.window.request_redraw();
                            println!("Fotograma en caché: {id}");
                        }
                        Err(error) => eprintln!("No se pudo leer el fotograma en caché: {error}"),
                    }
                }
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let source = shared::Source::read(&selector.folder.join(format!("surface-{id}.txt")))?;
        let session = self.session.as_mut().unwrap();
        let recovered = title.trim_end().ends_with("(no se pudo cambiar)");
        let cached = session.cached_aspect.take().is_some();
        let fade = self.proof.is_none() && !session.reduced_motion && !recovered;
        if cached && !recovered {
            session.gpu.resume_live_frame(source)?;
        } else {
            session.gpu.attach_with_fade(source, fade)?;
            session.wallpaper_fade = fade.then_some(WallpaperFade {
                aspect: self.scene.frame_aspect,
                first_present: None,
            });
        }
        self.source = Some(source);
        self.scene = Scene::shared(source.size);
        self.scene.title = title.trim().to_owned();
        if !recovered && let Some(widgets) = &mut session.widgets {
            widgets.record(format!("Fondo aplicado: {}", self.scene.title));
        }
        selector.active =
            std::fs::read_to_string(selector.folder.join(format!("active-{id}.txt")))?
                .trim()
                .parse()?;
        let audio_id =
            match std::fs::read_to_string(selector.folder.join(format!("audio-{id}.txt"))) {
                Ok(audio_id) => audio_id,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => id.to_string(),
                Err(error) => return Err(error.into()),
            };
        session.audio = bridge_signal::Audio::open(&selector.folder, &audio_id);
        selector.state = Switch::Idle;
        session.wallpapers.applied(Instant::now());
        if title.ends_with("(no se pudo cambiar)") {
            session.wallpapers.playback_error();
        }
        session
            .window
            .set_title(&format!("Ika wgpu - {}", self.scene.title));
        println!(
            "Fuente activa: {} | blur={}",
            self.scene.title, session.blur
        );
        if let Some((dir, stage)) = self.proof.as_mut() {
            *dir = dir.with_file_name(id.to_string());
            std::fs::create_dir_all(&dir)?;
            *stage = Proof::Warm(Instant::now());
        } else if session.hovered() == Some(Button::Next)
            && let Some(wake) = &self.wake
        {
            // Keeping the cursor on Next prepares another step in the cycle.
            selector.preview = None;
            selector.prepare(Some(-1), wake);
        }
        session.window.request_redraw();
        Ok(())
    }

    fn animation_elapsed(&self) -> Duration {
        if matches!(
            self.proof,
            Some((_, Proof::Before | Proof::WaitingForClick | Proof::After))
        ) {
            self.proof_frame_time()
        } else {
            self.epoch.elapsed()
        }
    }

    fn proof_frame_time(&self) -> Duration {
        Duration::from_secs_f64(if self.scene.motion.is_some() {
            1.65
        } else {
            self.scene.period / 3.0
        })
    }

    fn poll_initial_source(&mut self) -> Result<()> {
        let (Some(path), Some(session)) = (&self.pending_source, &mut self.session) else {
            return Ok(());
        };
        let source = match shared::Source::read(path) {
            Ok(source) => source,
            Err(error)
                if error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                    )
                }) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        session.gpu.attach(source)?;
        let mut scene = Scene::shared(source.size);
        std::mem::swap(&mut scene.title, &mut self.scene.title);
        self.scene = scene;
        self.source = Some(source);
        self.pending_source = None;
        session.audio = self
            .bridge_folder
            .as_deref()
            .and_then(|dir| bridge_signal::Audio::open(dir, "0"));
        println!("Fuente activa: {}", self.scene.title);
        session.window.request_redraw();
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        let elapsed = self.animation_elapsed();
        let (i, _) = self.scene.frame_at(elapsed);
        let proof_time = self.proof_frame_time();
        let session = self.session.as_mut().unwrap();
        let size = session.window.inner_size();
        if (session.occluded && session.shown)
            || session.window.is_minimized() == Some(true)
            || size.width == 0
            || size.height == 0
        {
            session.titlebar.pause();
            return Ok(());
        }
        let now = Instant::now();
        let paint = session.startup.as_mut().and_then(|startup| {
            if let Some(source) = &self.pending_source {
                startup.update_progress(now, source);
            }
            startup.tick(now);
            startup.paint(
                now,
                [size.width, size.height],
                session.window.scale_factor() as f32,
                self.pending_source.is_none(),
            )
        });
        if self.pending_source.is_some() {
            if let Some(paint) = &paint
                && session.gpu.draw_loading(&session.window, paint)?
            {
                session.show()?;
            }
            return Ok(());
        }
        if let Some((dir, stage @ Proof::Warm(_))) = &mut self.proof
            && matches!(stage, Proof::Warm(t) if t.elapsed() >= Duration::from_millis(1800))
        {
            std::fs::create_dir_all(dir)?;
            *stage = Proof::Before;
        }
        let candidate = match &self.proof {
            Some((dir, Proof::Home(stage)))
                if session
                    .home
                    .as_ref()
                    .is_some_and(|h| h.ready() && !h.animated(now)) =>
            {
                Some(dir.join(if *stage == 0 {
                    "home.png"
                } else {
                    "home-next.png"
                }))
            }
            Some((dir, Proof::Before)) => Some(dir.join("before.png")),
            Some((dir, Proof::After)) => Some(dir.join("after.png")),
            Some((dir, Proof::Moving(t))) if t.elapsed() >= Duration::from_millis(650) => {
                Some(dir.join("animated.png"))
            }
            _ => None,
        };
        session.titlebar.sync_picker(&session.window, now)?;
        if session.wallpapers.sync(now) {
            session.gpu.release_posters();
            if !session.fullscreen {
                titlebar::set_client_mode(&session.window, false)?;
            }
        }
        let hovered = session.hovered();
        let checking_blur = matches!(
            self.proof,
            Some((
                _,
                Proof::Warm(_) | Proof::Before | Proof::WaitingForClick | Proof::After
            ))
        );
        session.titlebar.set_hover(
            match (hovered, checking_blur) {
                (Some(Button::Chrome(hit)), false) => Some(hit),
                _ => None,
            },
            now,
        );
        let mut params = Params {
            screen: [
                size.width as f32,
                size.height as f32,
                session.cached_aspect.unwrap_or(self.scene.frame_aspect),
                session.window.scale_factor() as f32,
            ],
            frame: self
                .scene
                .frames
                .get(i)
                .unwrap_or(&self.scene.frames[0])
                .rect,
            button: session.button(Button::Blur),
            glass: session.glass(),
            state: [
                session.blur,
                hovered == Some(Button::Blur),
                session.down == Some(Button::Blur),
                self.scene.nearest,
            ]
            .map(|s| u8::from(s) as f32),
            glyphs: glyphs(if session.blur { "BLUR ON " } else { "BLUR OFF" }),
            motion: self.scene.motion.as_ref().map_or([[0.0; 4]; 8], |effects| {
                effects.parameters(elapsed.as_secs_f32())
            }),
            next_button: session.button(Button::Next),
            next_state: [
                u8::from(hovered == Some(Button::Next)) as f32,
                u8::from(session.down == Some(Button::Next)) as f32,
                session.tool_scale(),
                0.0,
            ],
            next_glyphs: glyphs("CAMBIAR FONDO"),
            ..Params::zeroed()
        };
        if session.home_active() {
            params.button = [0.0; 4];
            params.next_button = [0.0; 4];
            params.glass = [0.0; 4];
            session.home.as_mut().unwrap().uniforms(
                &mut params,
                &session.window,
                now,
                match hovered {
                    Some(Button::Home(hit)) => Some(hit),
                    _ => None,
                },
            );
        }
        if session.titlebar.on_console() {
            params.button = [0.0; 4];
            params.next_button = [0.0; 4];
            params.glass = [0.0; 4];
            params.state[0] = 0.0;
        }
        let console_glass = session.profile.console_glass(&session.window, session.console_view());
        if let Some(r) = console_glass {
            let dpi = params.screen[3];
            params.glass = [r.left() * dpi, r.top() * dpi, r.width() * dpi, r.height() * dpi];
            params.state[0] = 1.;
            params.next_state[3] = 1.;
        }
        if self.source.is_some() {
            params.motion[7][3] = 1.0;
            let ratio = (params.screen[0] / params.screen[1]) / params.screen[2];
            let (x, y) = (ratio.min(1.0), ratio.recip().min(1.0));
            params.motion[6] = [
                x / params.screen[0],
                y / params.screen[1],
                (1.0 - x) * 0.5,
                (1.0 - y) * 0.5,
            ];
        }
        if let Some(fade) = &session.wallpaper_fade {
            let progress = fade.first_present.map_or(0.0, |start| {
                ((now - start).as_secs_f32() / WALLPAPER_FADE.as_secs_f32()).min(1.0)
            });
            // Show the new wallpaper on its first present, then let it settle quickly.
            let remaining = 1.0 - progress;
            params.motion[7][0] = 0.68 * remaining * remaining;
            params.motion[7][1] = fade.aspect;
        }
        let hold_for_proof = matches!(
            self.proof,
            Some((_, Proof::Before | Proof::WaitingForClick))
        );
        let switching = self
            .selector
            .as_ref()
            .is_some_and(|s| s.state != Switch::Idle);
        let hold_titlebar = hold_for_proof || matches!(self.proof, Some((_, Proof::After)));
        session.titlebar.uniforms(
            &mut params,
            &session.window,
            now,
            hold_titlebar,
            !session.fullscreen,
        );
        let dock_hit = |button| match button {
            Some(Button::Dock(i)) if !hold_titlebar => Some(i),
            _ => None,
        };
        let play_state = session
            .home
            .as_ref()
            .map_or(dock::play::State::Idle, home::Home::play_state);
        session.dock.uniforms(
            &mut params,
            &session.window,
            now,
            session.titlebar.elapsed(),
            dock_hit(hovered),
            dock_hit(session.down),
            hold_titlebar,
            play_state,
        );
        // The last scale lane belongs to a tray button whose sprite has no
        // vertex scale. Use its sentinel to suppress the baked guest card.
        let profile_ready = session.profile.dock_ready();
        let show_profile = session.startup.is_none() || profile_ready;
        if show_profile && session.profile.has_account() {
            params.dock_scale[1][3] = 2.0;
        }
        if let Some((cache, keys)) = session.wallpapers.posters(&session.window, now) {
            session.gpu.load_posters(cache, keys);
        }
        let catalog_hover = match hovered {
            Some(Button::Wallpapers(hit)) if !switching && paint.is_none() => hit,
            _ => wallpapers::Hit::Inside,
        };
        let active = self.selector.as_ref().map_or(0, |s| s.active);
        session.wallpapers.uniforms(
            &mut params,
            &session.window,
            now,
            catalog_hover,
            active,
            switching,
            session.audio.as_ref().map(|audio| audio.playing),
            &session.gpu,
        );
        let active_ui = session.titlebar.animated(now)
            || session.dock.animated(now)
            || session.wallpapers.animating(now)
            || session.wallpapers.card_uses_main(catalog_hover, active)
            || session.home_active() && session.home.as_ref().is_some_and(|home| home.animated(now));
        let show_clock = session.home_active();
        let show_panels = session.widgets_interactive();
        if let Some(widgets) = &mut session.widgets {
            if let Some((style, clock_24h)) = session.profile.webview_clock() {
                widgets.apply_webview(style, clock_24h);
            }
            if let Some(favorites) = session.profile.webview_favorites() {
                widgets.import_favorites(favorites);
            }
            if widgets.tick(show_clock, show_panels, now) {
                session.profile.invalidate();
            }
        }
        let widgets = show_clock.then(|| session.widgets.as_ref().zip(session.home.as_ref())).flatten();
        session.profile.paint(
            &session.window, &mut session.gpu.profile, show_profile, session.wallpapers.dim,
            session.titlebar.elapsed(), widgets, show_panels,
            if widgets.is_some() { home_widgets::ClockOcclusion::from_scene(&params) } else { Default::default() },
            console_glass.is_some(),
        );
        if let Some(action) = session.profile.take_console_action() {
            match action {
                profile::ConsoleAction::Detail => session.titlebar.select(1, now),
                profile::ConsoleAction::Select(direction) => {
                    if let Some(home) = &mut session.home {
                        let count = home.collection().len() as i32;
                        if count > 0 {
                            let next = (home.selected() as i32 + direction).rem_euclid(count);
                            home.select_instance(next as usize, now);
                        }
                    }
                }
                profile::ConsoleAction::Launch => {
                    if let Some(home) = &mut session.home {
                        home.play_as(session.profile.account());
                    }
                }
            }
            session.window.request_redraw();
        }
        if session.gpu.draw(
            &session.window,
            &mut params,
            candidate.as_deref(),
            hold_for_proof || switching,
            paint.as_ref(),
            active_ui,
            session.dock_preview.playback(
                hovered == Some(Button::Dock(dock::WALLPAPERS)) && paint.is_none(),
            ),
            session.wallpapers.card_playing(catalog_hover, active),
        )? {
            session.show()?;
            if paint.is_none() && session.startup.take().is_some() {
                session.gpu.finish_startup();
                println!("Pantalla de carga cerrada");
            }
            let presented = Instant::now();
            session.dock_preview.presented(presented, &session.gpu);
            if let Some((start, frames)) = &mut session.present_stats {
                *frames += 1;
                let elapsed = presented - *start;
                if elapsed >= Duration::from_secs(1) {
                    println!(
                        "Viewer presents/s: {:.2}",
                        f64::from(*frames) / elapsed.as_secs_f64()
                    );
                    *start = presented;
                    *frames = 0;
                }
            }
            if let Some(picker) = &mut session.titlebar.picker {
                picker.presented(presented);
            }
            session.wallpapers.presented(presented);
            session.profile.presented(presented);
            if let Some(fade) = &mut session.wallpaper_fade {
                if params.motion[7][0] == 0.0 {
                    session.wallpaper_fade = None;
                    session.gpu.finish_wallpaper_fade();
                } else {
                    fade.first_present.get_or_insert(presented);
                }
            }
            if let Some(startup) = &mut session.startup {
                // The cover stays opaque until both dock cards have been
                // submitted in the same frame as the wallpaper.
                startup.presented(presented, profile_ready && session.gpu.startup_dock_ready());
            }
            session.last_frame = Some(i);
            if let Some(selector) = self
                .selector
                .as_mut()
                .filter(|s| s.state == Switch::PinFrame)
            {
                // Keep the imported texture readable by withholding its used fence.
                // Its owner can exit; the viewer reuses that allocation until attach.
                let file = selector
                    .folder
                    .join(format!("request-{}", selector.generation));
                let temp = file.with_extension("tmp");
                std::fs::write(
                    &temp,
                    selector.choice.map_or_else(String::new, |i| i.to_string()),
                )?;
                std::fs::rename(temp, file)?;
                selector.state = Switch::Waiting;
                if let Some(wake) = &self.wake {
                    wake.request();
                }
            }
            if let (Some(file), Some((_, stage))) = (candidate, &mut self.proof) {
                println!("{} frame={} blur={}", file.display(), i, session.blur);
                *stage = match stage {
                    Proof::Home(0) => {
                        session
                            .home
                            .as_mut()
                            .unwrap()
                            .activate(home::Hit::Next, Instant::now());
                        Proof::Home(1)
                    }
                    Proof::Home(_) => Proof::HomeDone,
                    Proof::Before => Proof::WaitingForClick,
                    Proof::After => {
                        let now = Instant::now();
                        self.epoch = now - proof_time;
                        Proof::Moving(now)
                    }
                    _ => Proof::Done,
                };
            }
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn exiting(&mut self, _: &ActiveEventLoop) {
        // Drain submitted frames before releasing the surface and shared source.
        if let Some(mut session) = self.session.take() {
            if self.proof.is_none()
                && let Some(home) = &session.home
                && let Err(error) = home.save()
            {
                eprintln!("Perfil Minecraft: {error}");
            }
            if let Err(error) = session.gpu.finish() {
                self.error.get_or_insert(error);
            }
        }
    }

    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.session.is_none()
            && let Err(error) = self.open(events)
        {
            self.error = Some(error);
            events.exit();
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(session) = self.session.as_mut().filter(|s| s.window.id() == id) else {
            return;
        };
        if (self.pending_source.is_some()
            || session
                .startup
                .as_ref()
                .is_some_and(startup::Startup::loading))
            && matches!(
                &event,
                WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::KeyboardInput { .. }
            )
        {
            // Match the WebView: the cover blocks input until Home is ready,
            // then the leaves are decorative and do not intercept it.
            return;
        }
        // Menu input is modal over Home. The titlebar and dock retain their own hit targets.
        if session.profile.panel.is_some() {
            match &event {
                WindowEvent::MouseInput { state:ElementState::Pressed, button:MouseButton::Left,.. } => {
                    if let Some(hit)=session.cursor.and_then(|p|session.unobstructed_chrome(p)) {
                        let already_open=matches!((hit,session.profile.panel),
                            (Button::Dock(dock::PREFERENCES),Some(profile::Panel::Preferences))
                            | (Button::Dock(dock::FRIENDS),Some(profile::Panel::Friends)));
                        if already_open {
                            session.profile.dismiss(Instant::now());
                            session.down=None;
                            session.window.request_redraw();
                            return;
                        }
                        session.profile.close();
                    }
                }
                _ => {}
            }
        }
        if session.profile.event(&session.window, &event)
            && !matches!(&event,WindowEvent::CursorMoved{..}|WindowEvent::CursorLeft{..}) {
            return;
        }
        let cursor = if let WindowEvent::CursorMoved { position, .. } = &event {
            Some(*position)
        } else {
            session.cursor_position()
        };
        if session.profile.oneko_event(&session.window, cursor, &event) {
            if let WindowEvent::CursorMoved { position, .. } = event {
                session.cursor = Some(position);
            } else if matches!(event, WindowEvent::MouseInput { .. }) {
                session.down = None;
            }
            return;
        }
        let widgets_blocks = !session.profile.console_captured()
            && cursor.is_some_and(|p| {
                session.widgets_interactive()
                    && session.widgets.as_ref().zip(session.home.as_ref()).is_some_and(|(widgets, home)| {
                        widgets.blocks(&session.window, p, Instant::now(), home)
                    })
            });
        if !widgets_blocks && session.profile.panel.is_none() && session.console_view() != profile::ConsoleView::Hidden {
            if session.profile.console_event(&session.window, cursor, &event) {
                if let WindowEvent::CursorMoved { position, .. } = event {
                    session.cursor = Some(position);
                } else if matches!(event, WindowEvent::MouseInput { .. }) {
                    session.down = None;
                }
                return;
            }
        }
        let now = Instant::now();
        if session.widgets_interactive() && self.pending_source.is_none()
            && session.startup.as_ref().is_none_or(|startup| !startup.loading())
        {
            let ctrl = matches!(event, WindowEvent::MouseWheel { .. }) && control_is_down(session);
            let pointer = matches!(event, WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. })
                .then(|| session.cursor_position()).flatten();
            let home = session.home.as_mut().unwrap();
            let widgets = session.widgets.as_mut().unwrap();
            match &event {
                WindowEvent::CursorMoved { position, .. } => {
                    let icon = widgets.moved(&session.window, *position, now, home);
                    // Edge entry may start a slide even before it covers the pointer.
                    session.window.request_redraw();
                    if let Some(icon) = icon {
                        session.cursor = Some(*position);
                        session.window.set_cursor(icon);
                        return;
                    }
                    if let Some(previous) = session.cursor
                        && widgets.blocks(&session.window, previous, now, home)
                    {
                        session.window.set_cursor(CursorIcon::Default);
                    }
                }
                WindowEvent::MouseInput { state, button: button @ (MouseButton::Left | MouseButton::Right), .. }
                    if let Some(p) = pointer =>
                {
                    let (handled, action) = widgets.button(
                        &session.window, p, now, state.is_pressed(), *button == MouseButton::Right, home,
                    );
                    if handled {
                        session.down = None;
                        session.cursor = Some(p);
                        session.window.request_redraw();
                        match action {
                            Some(home_widgets::Action::Instance(index)) => home.select_instance(index, now),
                            Some(home_widgets::Action::InstanceFolder(index)) => {
                                if let Some(instance) = home.collection().nth(index) {
                                    if let Err(error) = std::process::Command::new("explorer.exe").arg(&instance.path).spawn() {
                                        eprintln!("Carpeta de instancia: {error}");
                                    }
                                }
                            }
                            Some(home_widgets::Action::Folder) => {
                                if let Some(instance) = home.collection().nth(home.selected()) {
                                    match std::process::Command::new("explorer.exe").arg(minecraft::game_folder(instance)).spawn() {
                                        Ok(_) => widgets.record(format!("Carpeta abierta: {}", instance.name)),
                                        Err(error) => eprintln!("Carpeta de Minecraft: {error}"),
                                    }
                                }
                            }
                            Some(home_widgets::Action::Create) => session.profile.open_instance(profile::InstanceMode::Create, None),
                            Some(home_widgets::Action::Shortcut(1)) => {
                                if let Some(instance) = home.active_instance() {
                                    let game = minecraft::game_folder(instance);
                                    let mods = game.join("mods");
                                    let folder = if mods.is_dir() { mods } else { game };
                                    if let Err(error) = std::process::Command::new("explorer.exe").arg(folder).spawn() {
                                        eprintln!("Carpeta de mods: {error}");
                                    }
                                }
                            }
                            Some(home_widgets::Action::Shortcut(2 | 3)) => {
                                let mode = if matches!(action, Some(home_widgets::Action::Shortcut(2))) {
                                    profile::InstanceMode::Edit
                                } else { profile::InstanceMode::Copy };
                                session.profile.open_instance(mode, home.active_instance().cloned());
                            }
                            _ => {}
                        }
                        return;
                    }
                }
                WindowEvent::MouseWheel { delta, .. } if let Some(p) = pointer => {
                    let delta = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y,
                        MouseScrollDelta::PixelDelta(p) => (p.y / 120.) as f32,
                    };
                    if widgets.wheel(&session.window, p, now, ctrl, delta, home) {
                        session.window.request_redraw();
                        return;
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if widgets.collection_editing() && event.state.is_pressed()
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight)) =>
                {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape | NamedKey::Enter) => widgets.dismiss_collection_input(&session.window, now),
                        Key::Named(NamedKey::Backspace) => widgets.collection_backspace(),
                        Key::Character(key) if session.ctrl && key.eq_ignore_ascii_case("v") => {
                            if let Some(text) = clipboard_text() {
                                widgets.collection_input(&text);
                            }
                        }
                        _ if !session.ctrl => {
                            if let Some(text) = &event.text {
                                widgets.collection_input(text);
                            }
                        }
                        _ => {}
                    }
                    session.window.request_redraw();
                    return;
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state.is_pressed() && event.logical_key == Key::Named(NamedKey::Escape)
                        && !home.editing() && widgets.close(now) =>
                {
                    session.window.request_redraw();
                    return;
                }
                _ => {}
            }
        }
        if let Some(widgets) = &mut session.widgets {
            match event {
                WindowEvent::CursorLeft { .. } => widgets.leave(now, false),
                WindowEvent::Focused(false) => widgets.leave(now, true),
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => session.profile.invalidate(),
                _ => {}
            }
        }
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Resized(size) => {
                if session.window.is_minimized() != Some(true) {
                    session.gpu.resize(size);
                }
                session.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => session.window.request_redraw(),
            WindowEvent::Occluded(occluded) => {
                session.occluded = occluded;
                if occluded && session.shown {
                    session.titlebar.pause();
                } else {
                    session.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                session.shift = modifiers.state().shift_key();
                session.ctrl = modifiers.state().control_key();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let cursor = session.cursor.or_else(|| {
                    let mut point = POINT::default();
                    unsafe {
                        GetCursorPos(&mut point).ok()?;
                        ScreenToClient(titlebar::window_handle(&session.window).ok()?, &mut point)
                            .as_bool()
                            .then(|| PhysicalPosition::new(point.x as f64, point.y as f64))
                    }
                });
                if let Some(pos) = cursor {
                    session.cursor = Some(pos);
                }
                if session.home_active()
                    && control_is_down(session)
                    && session.titlebar.picker.is_none()
                    && !session.wallpapers.visible()
                    && session.profile.panel.is_none()
                    && let (Some(home), Some(cursor)) = (&mut session.home, session.cursor)
                {
                    let amount = match delta {
                        MouseScrollDelta::LineDelta(_, lines) => lines,
                        MouseScrollDelta::PixelDelta(position) => {
                            (position.y as f32 / 120.0).clamp(-4.0, 4.0)
                        }
                    };
                    if home.zoom(&session.window, cursor, amount, Instant::now()) {
                        session.window.request_redraw();
                    }
                } else if session.wallpapers.visible() {
                    let pixels = match delta {
                        MouseScrollDelta::LineDelta(_, rows) => {
                            rows * 42.0 * session.window.scale_factor() as f32
                        }
                        MouseScrollDelta::PixelDelta(position) => position.y as f32,
                    };
                    session.wallpapers.scroll(&session.window, -pixels);
                    session.window.request_redraw();
                } else if let Some(picker) = &mut session.titlebar.picker {
                    let delta = match delta {
                        MouseScrollDelta::LineDelta(_, lines) => lines * 36.0,
                        MouseScrollDelta::PixelDelta(p) => {
                            (p.y / session.window.scale_factor()) as f32
                        }
                    };
                    picker.scroll(&session.window, -delta);
                    session.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let was = session.hovered();
                session.cursor = Some(position);
                if session.wallpapers.drag_scroll(&session.window, position.y) {
                    session.window.request_redraw();
                    return;
                }
                if session.down == Some(Button::Wallpapers(wallpapers::Hit::Dim)) {
                    session.wallpapers.dim_at(&session.window, position.x);
                    session.window.request_redraw();
                }
                let hover = session.hovered();
                if hover != Some(Button::Dock(dock::WALLPAPERS)) {
                    session.dock_preview.leave(Instant::now(), &mut session.gpu);
                }
                if was != hover {
                    if self.proof.is_none() {
                        Self::prepare_selector(
                            &mut self.selector,
                            &self.wake,
                            Self::hover_preview(hover),
                        );
                    }
                    session.window.set_cursor(
                        if matches!(
                            hover,
                            Some(
                                Button::Blur
                                    | Button::Next
                                    | Button::Dock(_)
                                    | Button::Home(_)
                                    | Button::Wallpapers(
                                        wallpapers::Hit::Close
                                            | wallpapers::Hit::Settings
                                            | wallpapers::Hit::Dim
                                            | wallpapers::Hit::Tab(_)
                                            | wallpapers::Hit::Online
                                            | wallpapers::Hit::Local
                                            | wallpapers::Hit::Submit
                                            | wallpapers::Hit::Filter(_)
                                            | wallpapers::Hit::Sort
                                            | wallpapers::Hit::Page(_)
                                            | wallpapers::Hit::Import
                                            | wallpapers::Hit::Link
                                            | wallpapers::Hit::Folder
                                            | wallpapers::Hit::Refresh
                                            | wallpapers::Hit::Scrollbar(true)
                                            | wallpapers::Hit::Details(_)
                                            | wallpapers::Hit::CloseDetails
                                            | wallpapers::Hit::Property(_, _)
                                            | wallpapers::Hit::PropertyPage(_)
                                            | wallpapers::Hit::ApplyProperties
                                            | wallpapers::Hit::ResetProperties
                                            | wallpapers::Hit::Apply(_)
                                            | wallpapers::Hit::Audio
                                            | wallpapers::Hit::Remove(_)
                                            | wallpapers::Hit::Power
                                            | wallpapers::Hit::Interval
                                            | wallpapers::Hit::AddCurrent
                                            | wallpapers::Hit::Clear
                                    )
                                    | Button::Chrome(
                                        titlebar::Hit::Tab(_)
                                            | titlebar::Hit::Window(_)
                                            | titlebar::Hit::Picker(
                                                titlebar::monitor::Target::Close
                                                    | titlebar::monitor::Target::Monitor(_)
                                            )
                                    )
                            )
                        ) {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        },
                    );
                    session.window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } if session.wallpapers.dragging_scroll() => (),
            WindowEvent::CursorLeft { .. } => {
                session.dock_preview.leave(Instant::now(), &mut session.gpu);
                if let (Some(selector), Some(wake)) = (&mut self.selector, &self.wake) {
                    selector.prepare(None, wake);
                }
                session.wallpapers.end_scroll();
                session.cursor = None;
                session.down = None;
                session.window.set_cursor(CursorIcon::Default);
                session.window.request_redraw();
            }
            WindowEvent::Focused(false) => {
                session.dock_preview.leave(Instant::now(), &mut session.gpu);
                if let (Some(selector), Some(wake)) = (&mut self.selector, &self.wake) {
                    selector.prepare(None, wake);
                }
                session.wallpapers.end_scroll();
                session.cursor = None;
                session.down = None;
                session.ctrl = false;
                session.shift = false;
                session.last_click = None;
                session.window.set_cursor(CursorIcon::Default);
                session.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let hovered = session.hovered();
                if state == ElementState::Released
                    && matches!(
                        session.down,
                        Some(Button::Wallpapers(wallpapers::Hit::Scrollbar(_)))
                    )
                {
                    session.down = None;
                    session.wallpapers.end_scroll();
                    session.window.set_cursor(CursorIcon::Default);
                    session.window.request_redraw();
                    return;
                }
                match state {
                    ElementState::Pressed => {
                        let now = Instant::now();
                        let cursor_pos = session.cursor.or_else(|| {
                            let mut point = POINT::default();
                            unsafe {
                                GetCursorPos(&mut point).ok()?;
                                ScreenToClient(
                                    titlebar::window_handle(&session.window).ok()?,
                                    &mut point,
                                )
                                .as_bool()
                                .then(|| PhysicalPosition::new(point.x as f64, point.y as f64))
                            }
                        });
                        if let Some(pos) = cursor_pos {
                            session.cursor = Some(pos);
                        }
                        if session.home_active()
                            && control_is_down(session)
                            && session.titlebar.picker.is_none()
                            && let (Some(home), Some(pos)) = (&mut session.home, session.cursor)
                            && !home.is_over_toolbar(&session.window, pos, now)
                            && home.is_over_carousel(&session.window, pos)
                        {
                            let is_double_click =
                                session.last_click.map_or(false, |(prev_time, prev_pos)| {
                                    now.saturating_duration_since(prev_time)
                                        <= Duration::from_millis(400)
                                        && (prev_pos.x - pos.x).hypot(prev_pos.y - pos.y) <= 10.0
                                });
                            if is_double_click {
                                session.last_click = None;
                                session.down = None;
                                if home.reset_zoom(now) {
                                    session.window.request_redraw();
                                }
                                return;
                            } else {
                                session.last_click = Some((now, pos));
                            }
                        } else if let Some(pos) = session.cursor {
                            session.last_click = Some((now, pos));
                        }
                        if self.proof.is_none() {
                            Self::prepare_selector(
                                &mut self.selector,
                                &self.wake,
                                Self::hover_preview(hovered),
                            );
                        }
                        session.down = hovered;
                        if hovered == Some(Button::Dock(dock::PLAY)) {
                            session.dock.play_pointer_down(now);
                        }
                        if matches!(
                            hovered,
                            Some(Button::Wallpapers(wallpapers::Hit::Scrollbar(_)))
                        ) && let Some(p) = session.cursor
                        {
                            session.wallpapers.start_scroll(&session.window, p.y);
                        }
                        if hovered == Some(Button::Wallpapers(wallpapers::Hit::Dim))
                            && let Some(p) = session.cursor
                        {
                            session.wallpapers.dim_at(&session.window, p.x);
                        }
                    }
                    ElementState::Released => {
                        let pressed = session.down.take();
                        if let Some(button) = pressed.filter(|_| pressed == hovered) {
                            match button {
                                Button::Blur => self.toggle(),
                                Button::Next => self.next(),
                                Button::Dock(dock::WALLPAPERS) => {
                                    if let Err(error) = self.open_wallpapers() {
                                        self.error = Some(error);
                                        events.exit();
                                    }
                                }
                                Button::Dock(dock::PLAY) => {
                                    if let Some(home) = &mut session.home && !session.profile.loading() {
                                        home.play_as(session.profile.account());
                                        if !session.titlebar.on_home() {
                                            session.titlebar.select(0, Instant::now());
                                        }
                                    }
                                }
                                Button::Dock(dock::PROFILE) => {
                                    session.profile.open(profile::Panel::Profile);
                                }
                                Button::Dock(dock::PREFERENCES) => session.profile.open(profile::Panel::Preferences),
                                Button::Dock(dock::FRIENDS) => session.profile.open(profile::Panel::Friends),
                                Button::Dock(dock::EXPORT | dock::EDIT) => {
                                    let mode = if button == Button::Dock(dock::EXPORT) {
                                        profile::InstanceMode::Export
                                    } else { profile::InstanceMode::Edit };
                                    session.profile.open_instance(mode, session.home.as_ref().and_then(home::Home::active_instance).cloned());
                                }
                                Button::Dock(dock::FOLDER) => {
                                    if let Some(instance) = session.home.as_ref().and_then(home::Home::active_instance)
                                        && let Err(error) = std::process::Command::new("explorer.exe").arg(minecraft::game_folder(instance)).spawn()
                                    {
                                        eprintln!("Carpeta de Minecraft: {error}");
                                    }
                                }
                                Button::Dock(i) => dock::open_link(i),
                                Button::Home(home::Hit::Create) => {
                                    session.profile.open_instance(profile::InstanceMode::Create, None);
                                }
                                Button::Home(hit) => {
                                    if let Some(home) = &mut session.home {
                                        home.activate(hit, Instant::now());
                                    }
                                }
                                Button::Wallpapers(hit) => self.activate_wallpapers(hit),
                                Button::Chrome(titlebar::Hit::Tab(index)) => {
                                    if index != 0
                                        && let Some(home) = &mut session.home
                                        && home.editing()
                                    {
                                        home.cancel();
                                    }
                                    session.titlebar.select(index, Instant::now());
                                    session.window.request_redraw();
                                }
                                Button::Chrome(titlebar::Hit::Window(
                                    titlebar::Control::Notify,
                                )) => {
                                    session.titlebar.toggle_notification(Instant::now());
                                    session.window.request_redraw();
                                }
                                Button::Chrome(titlebar::Hit::NotificationPanel) => (),
                                Button::Chrome(titlebar::Hit::Window(
                                    titlebar::Control::Minimize,
                                )) => {
                                    session.window.set_minimized(true);
                                }
                                Button::Chrome(titlebar::Hit::Window(
                                    titlebar::Control::Monitor,
                                )) => {
                                    if let Err(error) =
                                        session.titlebar.open_picker(&session.window)
                                    {
                                        self.error = Some(error);
                                        events.exit();
                                    }
                                    session.window.request_redraw();
                                }
                                Button::Chrome(titlebar::Hit::Picker(hit)) => {
                                    if let Some(picker) = &mut session.titlebar.picker {
                                        picker.activate(&session.window, hit, Instant::now());
                                        session.window.request_redraw();
                                    }
                                }
                                Button::Chrome(titlebar::Hit::Window(titlebar::Control::Close)) => {
                                    events.exit()
                                }
                            }
                            if let Some(s) = &self.session {
                                s.window.request_redraw();
                            }
                            return;
                        }
                    }
                }
                if state == ElementState::Pressed
                    && !matches!(
                        hovered,
                        Some(Button::Chrome(
                            titlebar::Hit::Window(titlebar::Control::Notify)
                                | titlebar::Hit::NotificationPanel
                        ))
                    )
                {
                    session.titlebar.notification_open = false;
                }
                session.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if matches!(event.logical_key, Key::Named(NamedKey::Control)) =>
            {
                // Keep a local fallback for wheel events. Some Windows pointer
                // drivers deliver the wheel before the corresponding
                // ModifiersChanged notification.
                session.ctrl = event.state.is_pressed();
            }
            WindowEvent::KeyboardInput { event, .. }
                if matches!(
                    event.physical_key,
                    PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight)
                ) =>
            {
                session.ctrl = event.state.is_pressed();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                if session.wallpapers.visible() {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => session.wallpapers.close(Instant::now()),
                        Key::Named(NamedKey::Backspace) => session.wallpapers.backspace(),
                        Key::Named(NamedKey::Enter) if session.wallpapers.editing() => {
                            session.wallpapers.submit();
                        }
                        Key::Character(ref key)
                            if session.ctrl
                                && key.eq_ignore_ascii_case("v")
                                && session.wallpapers.editing() =>
                        {
                            if let Some(text) = clipboard_text() {
                                session.wallpapers.key_text(text.trim());
                            }
                        }
                        _ => {
                            if let Some(text) = &event.text {
                                session.wallpapers.key_text(text);
                            }
                        }
                    }
                    session.window.request_redraw();
                    return;
                }
                if session.home_active()
                    && session.titlebar.picker.is_none()
                    && let Some(home) = &mut session.home
                    && home.editing()
                {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => home.cancel(),
                        Key::Named(NamedKey::Backspace) => home.backspace(),
                        Key::Named(NamedKey::Enter) if !event.repeat => home.submit(),
                        Key::Character(ref key)
                            if session.ctrl && key.eq_ignore_ascii_case("v") =>
                        {
                            if let Some(text) = clipboard_text() {
                                home.input(text.trim());
                            }
                        }
                        _ if !session.ctrl => {
                            if let Some(text) = &event.text {
                                home.input(text);
                            }
                        }
                        _ => (),
                    }
                    session.window.request_redraw();
                    return;
                }
                if let Key::Named(arrow @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) =
                    event.logical_key
                    && session.home_active()
                    && session.titlebar.picker.is_none()
                    && !session.wallpapers.visible()
                    && session.profile.panel.is_none()
                    && control_is_down(session)
                    && let (Some(home), Some(cursor)) = (&mut session.home, session.cursor)
                {
                    let direction = if arrow == NamedKey::ArrowUp { -1 } else { 1 };
                    if home.nudge_layout(&session.window, cursor, direction, Instant::now(), event.repeat) {
                        session.window.request_redraw();
                    }
                    return;
                }
                if event.repeat {
                    return;
                }
                if let Some(picker) = &mut session.titlebar.picker {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => picker.activate(
                            &session.window,
                            titlebar::monitor::Target::Close,
                            Instant::now(),
                        ),
                        Key::Named(NamedKey::Tab) => {
                            picker.focus_next(&session.window, session.shift)
                        }
                        Key::Named(NamedKey::Enter | NamedKey::Space) => {
                            picker.activate_focus(&session.window, Instant::now())
                        }
                        _ => (),
                    }
                    session.window.request_redraw();
                    return;
                }
                match event.logical_key {
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::Enter)
                        if session.home_active() =>
                    {
                        let home = session.home.as_mut().unwrap();
                        match event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                home.activate(home::Hit::Prev, Instant::now())
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                home.activate(home::Hit::Next, Instant::now())
                            }
                            _ if !session.profile.loading() => home.play_as(session.profile.account()),
                            _ => (),
                        }
                        session.window.request_redraw();
                    }
                    Key::Named(NamedKey::Enter | NamedKey::Space) if !session.home_active() => {
                        self.toggle()
                    }
                    Key::Character(ref key) if key.eq_ignore_ascii_case("m") && !session.ctrl => {
                        self.activate_wallpapers(wallpapers::Hit::Audio);
                    }
                    Key::Named(NamedKey::Escape) => {
                        if session.titlebar.notification_open {
                            session.titlebar.notification_open = false;
                            session.window.request_redraw();
                        } else {
                            events.exit();
                        }
                    }
                    Key::Character(ref key) if key.eq_ignore_ascii_case("n") => self.next(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw() {
                    self.error = Some(error);
                    events.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        if let Err(error) = self.poll_initial_source().and_then(|()| self.poll_switch()) {
            self.error = Some(error);
            events.exit();
            return;
        }
        if let Some(action) = self.session.as_mut().and_then(|s|s.profile.take_app_action()) {
            match action {
                profile::AppAction::Wallpapers => {
                    self.session.as_mut().unwrap().profile.close();
                    if let Err(error) = self.open_wallpapers() { eprintln!("Wallpapers: {error}"); }
                }
                profile::AppAction::WallpaperDim(dim) => {
                    let s = self.session.as_mut().unwrap();
                    s.wallpapers.dim = dim;
                    s.window.request_redraw();
                }
                profile::AppAction::GameWindow(hide) => self.session.as_mut().unwrap().minimize_on_play = hide,
                profile::AppAction::InstanceFolderChanged => self.session.as_mut().unwrap().refresh_instances = true,
            }
        }
        let now = Instant::now();
        if self.session.as_ref().is_some()
            && let Some((choice, force)) = self
                .selector
                .as_mut()
                .filter(|s| s.state == Switch::Idle)
                .and_then(|s| s.queued.take())
        {
            self.switch_to_source(choice, force);
        }
        let idle = self
            .selector
            .as_ref()
            .is_some_and(|s| s.state == Switch::Idle);
        if let Some(s) = self.session.as_mut()
            && let Some((index, force)) = s.wallpapers.poll(now, idle)
        {
            self.switch_to_source(Some(index), force);
        }
        if let Some(sel) = self.selector.as_ref().filter(|s| s.state == Switch::Idle)
            && let Some(s) = self.session.as_mut()
            && let Some(index) = s.wallpapers.rotate(sel.active, Instant::now())
        {
            self.switch_to(Some(index));
        }
        if let Some(s) = self.session.as_mut()
            && let Some(home) = &mut s.home
            && home.poll(&mut s.gpu)
        {
            s.profile.invalidate();
            s.window.request_redraw();
        }
        if let Some(s) = self.session.as_mut() && let Some(home) = &mut s.home {
            if s.refresh_instances && !home.busy() {
                s.refresh_instances = false;
                home.refresh_catalog();
            }
            let instance = home.running_instance();
            let running = instance.is_some();
            if let Some(instance) = instance
                && s.running_instance.as_deref() != Some(instance.path.as_str())
                && let Some(widgets) = &mut s.widgets
            {
                widgets.record(format!("Juego iniciado: {}", instance.name));
            }
            if s.running_instance.as_deref() != instance.map(|instance| instance.path.as_str()) {
                s.running_instance = instance.map(|instance| instance.path.clone());
            }
            if running && !s.game_minimized && s.window.is_minimized() != Some(true) && s.minimize_on_play {
                s.game_minimized = true;
                s.window.set_minimized(true);
            }
            if !running && std::mem::take(&mut s.game_minimized) {
                s.window.set_minimized(false);
            }
        }
        if matches!(self.proof, Some((_, Proof::HomeDone))) {
            events.exit();
            return;
        }
        let mut preview_hover = None;
        if let Some(s) = self.session.as_mut() {
            let visible = self.proof.is_none() && s.shown && !s.occluded
                && s.window.is_minimized() != Some(true)
                && s.window.inner_size().width > 0 && s.window.inner_size().height > 0;
            if s.profile.poll(now, visible, self.bridge_folder.as_deref(), self.wake.as_ref(), &mut s.gpu.profile) {
                s.window.request_redraw();
            }
            let console_view = if visible { s.console_view() } else { profile::ConsoleView::Hidden };
            if s.profile.poll_console(console_view, s.home.as_ref(), now) {
                s.window.request_redraw();
            }
            if visible && s.profile.poll_oneko(&s.window, now) {
                s.window.request_redraw();
            }
            if let Some((path, message)) = s.profile.take_instance_saved() {
                if let Some(home) = s.home.as_mut() {
                    home.select_after_refresh(&path);
                }
                if let Some(widgets) = s.widgets.as_mut() {
                    widgets.record(message);
                }
            }
            preview_hover = if visible && s.startup.is_none() {
                s.hovered()
            } else {
                None
            };
            let active = self.selector.as_ref().map_or(0, |sel| sel.active);
            let bridge = self.bridge_folder.as_deref().map(|folder| dock_preview::Bridge {
                folder,
                active,
                wake: self.wake.as_ref(),
                fade: !s.reduced_motion,
                prepare: visible && idle && !s.wallpapers.visible()
                    && s.startup.as_ref().is_none_or(|startup| !startup.loading()),
            });
            if s.dock_preview.poll(now, preview_hover == Some(Button::Dock(dock::WALLPAPERS)), bridge, &mut s.gpu) {
                s.window.request_redraw();
            }
            let hit = match preview_hover {
                Some(Button::Wallpapers(hit)) if idle => hit,
                _ => wallpapers::Hit::Inside,
            };
            if s.wallpapers.poll_card_preview(now, hit, active, self.wake.as_ref(), &mut s.gpu) {
                s.window.request_redraw();
            }
        }
        let Some(s) = self.session.as_ref() else {
            return;
        };
        let waiting = self
            .selector
            .as_ref()
            .is_some_and(|s| s.state == Switch::Waiting);
        let size = s.window.inner_size();
        let hidden = (s.occluded && s.shown)
            || s.window.is_minimized() == Some(true)
            || size.width == 0
            || size.height == 0;
        let trim_at = self.trim.as_mut().and_then(|trim| {
            let loading = !s.shown
                || s.startup.is_some()
                || self.pending_source.is_some()
                || self.selector.as_ref().is_some_and(|sel| sel.state != Switch::Idle)
                || s.profile.loading()
                || s.home.as_ref().is_some_and(home::Home::catalog_loading);
            let interactive = !hidden
                && (s.wallpaper_fade.is_some()
                    || s.wallpapers.visible()
                    || s.profile.panel.is_some()
                    || s.titlebar.picker.is_some()
                    || s.titlebar.notification_open
                    || s.widgets.as_ref().is_some_and(|widgets| widgets.transient_ui(now)));
            trim.update(now, loading || interactive, s.window.has_focus())
        });
        if hidden
            || (matches!(self.proof, Some((_, Proof::WaitingForClick))) && !s.wallpapers.visible())
        {
            let wake = if waiting {
                Some(now + Duration::from_millis(32))
            } else if let Some(at) = s.wallpapers.deadline() {
                Some(at)
            } else if s.dock_preview.needs_poll() {
                Some(now + Duration::from_millis(16))
            } else {
                None
            };
            events.set_control_flow(
                wake.into_iter().chain(trim_at).min()
                    .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
            );
            return;
        }
        if let Some(startup) = &s.startup
            && !waiting
        {
            s.window.request_redraw();
            let interval = if !s.shown || startup.animating() {
                COMPOSITOR_FRAME_INTERVAL
            } else if self.pending_source.is_some() {
                Duration::from_millis(125)
            } else {
                // Reduced motion still needs draws to seed the video thumbnail.
                Duration::from_millis(32)
            };
            events.set_control_flow(ControlFlow::WaitUntil(now + interval));
            return;
        }
        let (index, delay) = self.scene.frame_at(self.animation_elapsed());
        let now = Instant::now();
        let mut next_title = s.titlebar.next_tick(now);
        if s.dock.animated(now)
            || s.dock_preview.animating(&s.gpu)
            || s.wallpapers.animating(now)
            || s.wallpaper_fade.is_some()
            || s.home_active() && s.home.as_ref().is_some_and(|home| home.animated(now))
        {
            next_title = Some(next_title.map_or(s.titlebar.interaction_tick(), |t| t.min(s.titlebar.interaction_tick())));
        }
        if waiting {
            if next_title.is_some_and(|t| now >= t) {
                s.window.request_redraw();
            }
            events.set_control_flow(if let Some(t) = next_title {
                ControlFlow::WaitUntil(
                    t.max(now)
                        .min(now + Duration::from_millis(if self.wake.is_some() { 32 } else { 16 })),
                )
            } else {
                ControlFlow::WaitUntil(
                    now + Duration::from_millis(if self.wake.is_some() { 32 } else { 16 }),
                )
            });
            return;
        }

        let has_source = self.source.is_some();
        let source_has_frame = has_source && s.gpu.shared_frame_ready().ok().flatten().unwrap_or(false);
        let frame_wake = self.frame_wake.as_ref().is_some_and(|wake| {
            s.gpu.profile.wake_for_cover(wake);
            if s.dock_preview.playback(preview_hover == Some(Button::Dock(dock::WALLPAPERS)))
                == gpu::dock_preview::Playback::Remote
            {
                s.gpu.dock_preview.wake_for_frame(wake);
            }
            let active = self.selector.as_ref().map_or(0, |sel| sel.active);
            if let Some(Button::Wallpapers(hit)) = preview_hover
                && s.wallpapers.card_playing(hit, active)
            {
                s.gpu.catalog_preview.wake_for_frame(wake);
            }
            has_source && s.gpu.wake_for_shared_frame(wake)
        });
        let anim_ready = next_title.is_some_and(|t| now >= t);
        let scene_frame_changed = s.last_frame != Some(index);
        let widget_tick = s.widgets.as_ref().and_then(|widgets| widgets.deadline(now));

        if source_has_frame || anim_ready || scene_frame_changed || s.gpu.dock_preview.seed_due(now)
            || widget_tick.is_some_and(|t| t <= now) {
            s.window.request_redraw();
        }

        let next_wake = match (has_source, next_title) {
            (_, Some(t)) => t.max(now).min(now + Duration::from_millis(16)),
            (true, None) => now + Duration::from_millis(if frame_wake { 50 } else { 16 }),
            (false, None) => now + delay,
        };
        let next_wake = s.dock_preview.deadline(now).map_or(next_wake, |at| next_wake.min(at));
        let next_wake = s.gpu.dock_preview.seed_deadline()
            .filter(|&at| at > now).map_or(next_wake, |at| next_wake.min(at));
        let next_wake = if s.dock_preview.needs_poll() {
            next_wake.min(now + Duration::from_millis(16))
        } else { next_wake };
        let next_wake = s.wallpapers.card_preview_deadline(now)
            .map_or(next_wake, |at| next_wake.min(at));
        let next_wake = s.profile.deadline().map_or(next_wake, |at| next_wake.min(at.max(now)));
        let next_wake = widget_tick.map_or(next_wake, |at| next_wake.min(at));
        let next_wake = trim_at.map_or(next_wake, |at| next_wake.min(at));
        events.set_control_flow(ControlFlow::WaitUntil(next_wake));
    }
}

const fn glyphs(text: &str) -> [[u32; 4]; 8] {
    let mut rows = [[0; 4]; 8];
    let chars = text.as_bytes();
    let mut i = 0;
    while i < chars.len() {
        let glyph = font8x8::legacy::BASIC_LEGACY[chars[i] as usize];
        let mut y = 0;
        while y < 8 {
            rows[y][i / 4] |= (glyph[y] as u32) << ((i % 4) * 8);
            y += 1;
        }
        i += 1;
    }
    rows
}

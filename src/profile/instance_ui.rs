use super::{load_image, storage, windows_ui};
use crate::minecraft::{
    Instance,
    instance_catalog::{self, JAVA_VENDORS, JavaBuild, LoaderVersion},
    instances::{self, Draft, FALLBACK_VERSIONS, LOADERS},
    modpack_catalog::{self, Catalog, PAGE_SIZE},
    modpacks::{self, DescriptionBlock, Details, Project, Provider},
};
use egui::{
    Align2, Color32 as C, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextureHandle, Ui, pos2, vec2,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};
use winit::{event_loop::EventLoopProxy, window::Window};

#[path = "instance_art.rs"]
mod instance_art;
#[path = "instance_gallery.rs"]
mod instance_gallery;
use instance_art::Kind as ArtKind;
use instance_gallery::Gallery;

const BG: C = C::from_rgb(4, 4, 22);
const CARD: C = C::from_rgb(12, 12, 31);
const FIELD: C = C::from_rgb(15, 13, 34);
const LINE: C = C::from_rgb(54, 37, 92);
const WHITE: C = C::from_rgb(241, 245, 249);
const MUTED: C = C::from_rgb(156, 174, 200);
const DIM: C = C::from_rgb(109, 127, 154);
const VIOLET: C = C::from_rgb(143, 94, 255);
const LAVENDER: C = C::from_rgb(196, 181, 253);
const CYAN: C = C::from_rgb(6, 243, 232);
const AMBER: C = C::from_rgb(251, 191, 36);
const NONE: C = C::TRANSPARENT;
const UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1., 1.));
const CREATE_COVER: &str = "shader_bsl_shaders_42";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Create,
    Edit,
    Export,
    Copy,
}

enum ResultMessage {
    Versions(Vec<String>),
    Java(Vec<(u32, String)>),
    Majors(usize, Result<Vec<u32>, String>),
    Builds((usize, u32), Result<Vec<JavaBuild>, String>),
    Loaders(String, LoaderSet),
    Packs(PackSearch, Result<Catalog, String>),
    PackDetails(String, Details),
    PackImage(String, Option<image::RgbaImage>),
    PackMedia(String, Vec<(String, image::RgbaImage)>),
    Thumbnails((PackSearch, usize), Vec<(String, image::RgbaImage)>),
    Progress(String),
    Saved(Result<(PathBuf, String), String>),
}

type LoaderSet = [Result<Vec<LoaderVersion>, String>; 4];

#[derive(Clone, PartialEq, Eq)]
struct PackSearch {
    provider: Provider,
    query: String,
    minecraft: String,
}

struct PackState {
    query: String,
    filter: String,
    provider: Provider,
    filter_open: bool,
    page: usize,
    selected: usize,
    next_selection: Option<(usize, usize)>,
    release: usize,
    gallery: Gallery,
    release_open: bool,
    search_key: Option<PackSearch>,
    running: bool,
    search_after: Option<Instant>,
    catalog: Option<Catalog>,
    search_error: Option<(PackSearch, String)>,
    automatic_name: String,
    detail_key: Option<String>,
    detail_running: bool,
    detail: Option<Details>,
    snapshot_hero: Option<(String, TextureHandle)>,
    media_key: Option<String>,
    media_running: bool,
    media_images: Vec<(String, image::RgbaImage)>,
    media_textures: HashMap<String, TextureHandle>,
    thumbnail_key: Option<(PackSearch, usize)>,
    thumbnail_running: bool,
    thumbnail_attempted: bool,
    thumbnail_images: Vec<(String, image::RgbaImage)>,
    thumbnails: HashMap<String, TextureHandle>,
}

impl Default for PackState {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: String::new(),
            provider: Provider::CurseForge,
            filter_open: false,
            page: 0,
            selected: 0,
            next_selection: None,
            release: 0,
            gallery: Gallery::default(),
            release_open: false,
            search_key: None,
            running: false,
            search_after: None,
            catalog: None,
            search_error: None,
            automatic_name: "Mi instancia".into(),
            detail_key: None,
            detail_running: false,
            detail: None,
            snapshot_hero: None,
            media_key: None,
            media_running: false,
            media_images: Vec::new(),
            media_textures: HashMap::new(),
            thumbnail_key: None,
            thumbnail_running: false,
            thumbnail_attempted: false,
            thumbnail_images: Vec::new(),
            thumbnails: HashMap::new(),
        }
    }
}

impl PackState {
    fn key(&self) -> PackSearch {
        PackSearch {
            provider: self.provider,
            query: self.query.trim().into(),
            minecraft: self.filter.clone(),
        }
    }
    fn projects(&self) -> &[Project] {
        self.catalog
            .as_ref()
            .map_or(&[], |catalog| catalog.projects.as_slice())
    }
    fn project(&self) -> Option<&Project> {
        self.projects().get(self.selected)
    }
    fn release(&self) -> Option<&modpacks::Release> {
        self.detail
            .as_ref()?
            .versions
            .as_ref()
            .ok()?
            .get(self.release)
    }
    fn reset_detail(&mut self) {
        self.detail_key = None;
        self.detail = None;
        self.release = 0;
        self.release_open = false;
        self.gallery.reset();
        self.media_key = None;
        self.media_images.clear();
        self.media_textures.clear();
    }

    fn activate(&mut self, key: PackSearch, catalog: Catalog) {
        self.page = 0;
        self.selected = 0;
        self.next_selection = None;
        self.snapshot_hero = None;
        self.reset_detail();
        self.search_error = None;
        if let Some(detail) = catalog.first_details()
            && let Some(project) = catalog.projects.first()
        {
            if !detail.body.is_empty() {
                self.detail_key = Some(pack_detail_key(project, &key.minecraft));
            }
            self.detail = Some(detail);
        }
        self.catalog = Some(catalog);
        self.search_key = Some(key);
        self.thumbnail_key = None;
    }
}

fn pack_detail_key(project: &Project, minecraft: &str) -> String {
    format!("{}:{}:{minecraft}", project.provider.label(), project.id)
}

pub(super) struct Dialog {
    pub mode: Mode,
    source: Option<Instance>,
    original: Draft,
    draft: Draft,
    custom: bool,
    query: String,
    packs: PackState,
    version_drag_origin: Option<f32>,
    version_drag_position: f32,
    version_wheel: f32,
    custom_name: String,
    pack_name: String,
    recipient: String,
    error: String,
    is_error: bool,
    busy: bool,
    java_recommended: bool,
    java: Option<Vec<(u32, String)>>,
    java_refreshing: bool,
    java_vendor: usize,
    java_major: Option<u32>,
    java_pick_latest: bool,
    java_major_anchor: Option<(usize, u32, usize)>,
    java_majors: HashMap<usize, Option<Result<Vec<u32>, String>>>,
    java_builds: HashMap<(usize, u32), Option<Result<Vec<JavaBuild>, String>>>,
    java_icons: Option<Vec<TextureHandle>>,
    versions: Vec<String>,
    loader_versions: HashMap<String, LoaderSet>,
    loader_request: Option<String>,
    art: Option<Art>,
    art_picker: Option<instance_art::Picker>,
    art_tube: Option<crate::crt::Tube>,
    art_files: Option<std::sync::Arc<tempfile::TempDir>>,
    tx: mpsc::Sender<ResultMessage>,
    rx: mpsc::Receiver<ResultMessage>,
    wake: EventLoopProxy<()>,
}

struct Art {
    cover: TextureHandle,
    icon: TextureHandle,
    icon_plate: TextureHandle,
    symbols: TextureHandle,
    controls: TextureHandle,
    platforms: [TextureHandle; 2],
}

impl Art {
    fn new(ctx: &egui::Context, draft: &Draft, source: Option<&Instance>) -> Self {
        let cover = draft
            .cover
            .as_ref()
            .and_then(|path| read_image(path, 1280))
            .or_else(|| source.and_then(|inst| read_image(&instances::cover_path(inst), 1280)))
            .or_else(|| {
                if source.is_some() {
                    return None;
                }
                let (_, bytes) = ArtKind::Cover.file_bytes(ArtKind::Cover.index(CREATE_COVER)?)?;
                Some(image::load_from_memory(&bytes).ok()?.to_rgba8())
            })
            .unwrap_or_else(|| {
                image::load_from_memory(include_bytes!("../home/instance-cover.jpg"))
                    .unwrap()
                    .to_rgba8()
            });
        let icon = draft
            .icon
            .as_ref()
            .and_then(|path| read_image(path, 192))
            .or_else(|| {
                source
                    .and_then(|inst| inst.icon_path.as_ref())
                    .and_then(|p| read_image(Path::new(p), 192))
            })
            .unwrap_or_else(|| {
                image::load_from_memory(include_bytes!("../home/instance-grass.png"))
                    .unwrap()
                    .to_rgba8()
            });
        let symbols = image::load_from_memory(include_bytes!("../home/instance-icons.png"))
            .unwrap()
            .to_rgba8();
        let controls = image::load_from_memory(include_bytes!("../home/instance-controls.png"))
            .unwrap()
            .to_rgba8();
        let platforms = [
            include_bytes!("../home/modpack-catalog/curseforge.png").as_slice(),
            include_bytes!("../home/modpack-catalog/modrinth.png").as_slice(),
        ]
        .map(|bytes| {
            let image = image::load_from_memory(bytes)
                .unwrap()
                .thumbnail(40, 40)
                .to_rgba8();
            load_image(
                ctx,
                "modpack-platform",
                &image,
                egui::TextureOptions::LINEAR,
            )
        });
        Self {
            cover: load_image(ctx, "instance-cover", &cover, egui::TextureOptions::LINEAR),
            icon: load_image(ctx, "instance-icon", &icon, egui::TextureOptions::NEAREST),
            icon_plate: load_image(
                ctx,
                "instance-icon-plate",
                &image::load_from_memory(include_bytes!("instance-icon-plate.png")).unwrap().to_rgba8(),
                egui::TextureOptions::LINEAR,
            ),
            symbols: load_image(
                ctx,
                "instance-symbols",
                &symbols,
                egui::TextureOptions::LINEAR,
            ),
            controls: load_image(
                ctx,
                "instance-controls",
                &controls,
                egui::TextureOptions::LINEAR,
            ),
            platforms,
        }
    }
    fn symbol(&self, p: &Painter, atlas: usize, center: Pos2, width: f32, tint: C) {
        Self::atlas_symbol(p, &self.symbols, atlas, center, width, tint);
    }
    fn control(&self, p: &Painter, atlas: usize, center: Pos2, width: f32, tint: C) {
        Self::atlas_symbol(p, &self.controls, atlas, center, width, tint);
    }
    fn atlas_symbol(
        p: &Painter,
        texture: &TextureHandle,
        atlas: usize,
        center: Pos2,
        width: f32,
        tint: C,
    ) {
        let cell = 1. / (texture.size()[0] as f32 / 72.);
        let src = Rect::from_min_max(
            pos2((atlas as f32 + 1. / 12.) * cell, 1. / 12.),
            pos2((atlas as f32 + 11. / 12.) * cell, 11. / 12.),
        );
        p.image(
            texture.id(),
            Rect::from_center_size(center, vec2(width, width)),
            src,
            tint,
        );
    }
}
fn read_image(path: &Path, max: u32) -> Option<image::RgbaImage> {
    if path.metadata().ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    storage::rgba(&std::fs::read(path).ok()?, max)
}

impl Dialog {
    pub fn new(mode: Mode, source: Option<Instance>, wake: EventLoopProxy<()>) -> Self {
        let draft = Draft::new(source.as_ref());
        let java_vendor = draft
            .java_runtime
            .as_ref()
            .and_then(|rt| JAVA_VENDORS.iter().position(|v| v.id == rt.vendor))
            .unwrap_or(0);
        let java_major = draft.java_runtime.as_ref().map(|rt| rt.major);
        let java_recommended =
            java_major.is_none_or(|major| major == instances::recommended_java(&draft.version));
        let (tx, rx) = mpsc::channel();
        let mut result = Self {
            mode,
            source,
            original: draft.clone(),
            custom: matches!(mode, Mode::Copy | Mode::Edit),
            query: String::new(),
            packs: PackState::default(),
            version_drag_origin: None,
            version_drag_position: 0.,
            version_wheel: 0.,
            custom_name: draft.name.clone(),
            pack_name: draft.name.clone(),
            draft,
            recipient: String::new(),
            error: String::new(),
            is_error: false,
            busy: false,
            java_recommended,
            java: None,
            java_refreshing: false,
            java_vendor,
            java_major,
            java_pick_latest: mode == Mode::Create,
            java_major_anchor: None,
            java_majors: HashMap::new(),
            java_builds: HashMap::new(),
            java_icons: None,
            versions: FALLBACK_VERSIONS.map(str::to_owned).to_vec(),
            loader_versions: HashMap::new(),
            loader_request: None,
            art: None,
            art_picker: None,
            art_tube: None,
            art_files: None,
            tx,
            rx,
            wake,
        };
        if mode == Mode::Copy {
            result.draft.name = format!(
                "{} (Copia)",
                result.draft.name.chars().take(42).collect::<String>()
            );
            if let Some(source) = result.source.as_ref() {
                result.draft.cover = Some(instances::cover_path(source)).filter(|p| p.is_file());
                result.draft.icon = source
                    .icon_path
                    .as_deref()
                    .map(PathBuf::from)
                    .filter(|p| p.is_file());
            }
        }
        if mode == Mode::Create {
            result.open_pack_catalog();
            result.sync_pack_name();
        }
        if mode != Mode::Export {
            result.background(|| ResultMessage::Versions(instances::versions()));
            if result.custom {
                result.refresh_java();
            }
        }
        result
    }

    fn background(&self, f: impl FnOnce() -> ResultMessage + Send + 'static) {
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            if tx.send(f()).is_ok() {
                let _ = wake.send_event(());
            }
        });
    }
    fn refresh_java(&mut self) {
        if self.java_refreshing {
            return;
        }
        self.java_refreshing = true;
        self.background(|| ResultMessage::Java(instances::java_installations()));
    }
    pub fn busy(&self) -> bool {
        self.busy
    }

    pub(super) fn picker_layer() -> egui::LayerId {
        instance_art::Picker::layer()
    }

    pub(super) fn close_art_picker(&mut self) -> bool {
        if let Some(tube) = &mut self.art_tube {
            tube.close(Instant::now());
            return true;
        }
        self.art_picker.take().is_some()
    }

    pub(super) fn picker_frame(&self, now: Instant) -> Option<crate::crt::Frame> {
        self.art_tube.as_ref().map(|tube| tube.frame(now))
    }

    pub(super) fn picker_presented(&mut self, now: Instant) {
        if let Some(tube) = &mut self.art_tube {
            tube.presented(now);
        }
    }

    pub(super) fn tick_picker(&mut self, now: Instant) -> bool {
        // Keep the resting tube until the picker closes. Discarding it at
        // the end of the power-on effect would skip power-off on settled pickers.
        if !self.art_tube.as_ref().is_some_and(|tube| tube.finished(now)) { return false; }
        self.art_picker = None;
        self.art_tube = None;
        true
    }

    pub(super) fn picker_blocks_input(&self, now: Instant) -> bool {
        self.art_tube.as_ref().is_some_and(|tube| tube.closing() || tube.opening_before(now, 0.42))
    }

    fn open_art_picker(&mut self, kind: ArtKind, reduced_motion: bool) {
        let preset = match kind {
            ArtKind::Cover => self.draft.cover_preset.as_deref(),
            ArtKind::Icon => self.draft.icon_preset.as_deref().or_else(|| {
                (self.custom
                    && self.draft.icon.is_none()
                    && self.source.as_ref().is_none_or(|s| s.icon_path.is_none()))
                .then_some("grass_block_side")
            }),
        };
        self.art_picker = Some(instance_art::Picker::new(kind, preset));
        self.art_tube = (!reduced_motion).then(|| crate::crt::Tube::new(false));
    }

    fn prepare_art_preset(&mut self, kind: ArtKind, index: usize) -> Option<(PathBuf, String)> {
        let preset = kind.name(index)?;
        let (ext, bytes) = kind.file_bytes(index)?;
        if self.art_files.is_none() {
            self.art_files = Some(std::sync::Arc::new(tempfile::tempdir().ok()?));
        }
        let path = self
            .art_files
            .as_ref()?
            .path()
            .join(format!("{preset}.{ext}"));
        std::fs::write(&path, bytes).ok()?;
        Some((path, preset.into()))
    }

    fn apply_art_picker(&mut self, choice: instance_art::Result, window: &Window) {
        let (kind, path, preset) = match choice {
            instance_art::Result::Close => {
                self.close_art_picker();
                return;
            }
            instance_art::Result::Import(kind) => {
                let Some(path) = pick_file(
                    window,
                    match kind {
                        ArtKind::Cover => Pick::Cover,
                        ArtKind::Icon => Pick::Icon,
                    },
                ) else {
                    return;
                };
                if read_image(&path, if kind == ArtKind::Cover { 1280 } else { 192 }).is_none() {
                    self.notice("No se pudo leer la imagen seleccionada.".into(), true);
                    self.close_art_picker();
                    return;
                }
                (kind, path, None)
            }
            instance_art::Result::Select(kind, index) => {
                let current = match kind {
                    ArtKind::Cover => self.draft.cover_preset.as_deref(),
                    ArtKind::Icon => self.draft.icon_preset.as_deref(),
                };
                if current.is_some() && current == kind.name(index) {
                    self.close_art_picker();
                    return;
                }
                let Some((path, preset)) = self.prepare_art_preset(kind, index) else {
                    self.notice("No se pudo preparar la imagen seleccionada.".into(), true);
                    self.close_art_picker();
                    return;
                };
                (kind, path, Some(preset))
            }
        };
        match kind {
            ArtKind::Cover => {
                self.draft.cover = Some(path);
                self.draft.cover_preset = preset;
            }
            ArtKind::Icon => {
                self.draft.icon = Some(path);
                self.draft.icon_preset = preset;
            }
        }
        self.art = None;
        self.close_art_picker();
    }

    pub fn poll(&mut self) -> (bool, Option<(PathBuf, String)>) {
        let mut changed = false;
        let mut saved = None;
        while let Ok(message) = self.rx.try_recv() {
            changed = true;
            match message {
                ResultMessage::Versions(list) => self.versions = list,
                ResultMessage::Java(list) => {
                    self.java = Some(list);
                    self.java_refreshing = false;
                }
                ResultMessage::Majors(vendor, list) => {
                    self.java_majors.insert(vendor, Some(list));
                }
                ResultMessage::Builds(key, list) => {
                    if self.java_builds.contains_key(&key) {
                        self.java_builds.insert(key, Some(list));
                    }
                }
                ResultMessage::Loaders(mc, set) => {
                    self.loader_request = None;
                    if self.loader_versions.len() >= 12 {
                        self.loader_versions.clear();
                    }
                    self.loader_versions.insert(mc, set);
                }
                ResultMessage::Packs(key, list) => {
                    self.packs.running = false;
                    if self.packs.key() == key {
                        match list {
                            Ok(catalog) => self.packs.activate(key, catalog),
                            Err(error) => self.packs.search_error = Some((key, error)),
                        }
                    }
                }
                ResultMessage::PackDetails(key, detail) => {
                    self.packs.detail_running = false;
                    if self.packs.detail_key.as_ref() == Some(&key) {
                        if let Some(current) = self.packs.detail.as_mut() {
                            current.description = detail.description;
                            current.body = detail.body;
                            current.gallery = detail.gallery;
                            current.gallery_thumbnails = detail.gallery_thumbnails;
                        } else {
                            self.packs.detail = Some(detail);
                        }
                    }
                }
                ResultMessage::PackImage(url, image) => {
                    self.packs.gallery.receive(url, image);
                }
                ResultMessage::PackMedia(key, images) => {
                    self.packs.media_running = false;
                    if self.packs.media_key.as_ref() == Some(&key) {
                        self.packs.media_images = images;
                    }
                }
                ResultMessage::Thumbnails(key, images) => {
                    self.packs.thumbnail_running = false;
                    if self.packs.thumbnail_key.as_ref() == Some(&key) {
                        self.packs.thumbnail_images = images;
                    }
                }
                ResultMessage::Progress(message) => self.notice(message, false),
                ResultMessage::Saved(result) => {
                    self.busy = false;
                    match result {
                        Ok(value) => saved = Some(value),
                        Err(e) => self.notice(e, true),
                    }
                }
            }
        }
        (changed, saved)
    }
    fn notice(&mut self, s: String, error: bool) {
        self.error = s;
        self.is_error = error;
    }

    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        window: &Window,
        phase: f32,
        elapsed: Duration,
        reduced_motion: bool,
        backdrop: f32,
    ) -> (bool, Rect, Option<Rect>) {
        if self.art.is_none() {
            self.art = Some(Art::new(ctx, &self.draft, self.source.as_ref()));
        }
        let screen = ctx.content_rect();
        let scale = if self.mode == Mode::Export {
            (screen.width() / 610.).min(screen.height() / 518.).min(1.)
        } else {
            (screen.width() / 800.).min(screen.height() / 725.).min(1.)
        }
        .max(0.2);
        let view = screen.size() / scale;
        let (w, h, center_y) = if self.mode == Mode::Export {
            let h = if self.error.is_empty() { 466. } else { 496. };
            (580., h, ((view.y - 148.) / 2.).max(h / 2. + 8.))
        } else {
            let h = (view.y - 259.).clamp(540., 840.);
            (
                (view.x * 0.94).clamp(780., 1280.).min(view.x - 12.),
                h,
                (view.y - 51.) / 2.,
            )
        };
        let frame = Rect::from_min_size(pos2((view.x - w) / 2., center_y - h / 2.), vec2(w, h));
        let layer = windows_ui::preferences_layer();
        let transform = egui::emath::TSTransform::from_scaling(scale);
        ctx.set_transform_layer(layer, transform);
        ui.painter().rect_filled(
            screen,
            0.,
            C::from_black_alpha((146. * backdrop).round() as u8),
        );
        let mut close = false;
        let picker_open = self.art_picker.is_some();
        egui::Area::new(layer.id)
            .order(layer.order)
            .fixed_pos(frame.min)
            .constrain(false)
            .movable(false)
            .show(ctx, |ui| {
                let r = Rect::from_min_size(ui.min_rect().min, frame.size());
                ui.set_min_size(frame.size());
                ui.set_clip_rect(r.expand(26.));
                let radius = if self.mode == Mode::Export { 14. } else { 20. };
                let stroke = if self.mode == Mode::Export { 1.6 } else { 1.8 };
                windows_ui::shadow(ui.painter(), r, radius, 24);
                windows_ui::dialog_frame(ui.painter(), r, radius, BG, stroke, phase);
                if self.mode == Mode::Export {
                    close = self.paint_export(ui, r);
                } else if picker_open {
                    ui.scope_builder(egui::UiBuilder::new().disabled(), |ui| {
                        self.paint_form(ui, r, window, elapsed, reduced_motion);
                    });
                } else {
                    close = self.paint_form(ui, r, window, elapsed, reduced_motion);
                }
            });
        let mut picker_surface = None;
        if picker_open && let Some(picker) = &mut self.art_picker {
            let view = Rect::from_min_size(Pos2::ZERO, view);
            let r = picker.area(view);
            let layer = Self::picker_layer();
            ctx.set_transform_layer(layer, transform);
            let choice = egui::Area::new(layer.id)
                .order(layer.order)
                .fixed_pos(view.min)
                .default_size(view.size())
                .fade_in(false)
                .movable(false)
                .constrain(false)
                .show(ctx, |ui| {
                    ui.set_min_size(view.size());
                    picker.paint(ui, view, r, phase, reduced_motion)
                })
                .inner;
            if let Some(choice) = choice {
                self.apply_art_picker(choice, window);
                ctx.request_repaint();
            }
            picker_surface = Some(transform * r);
        }
        (close, transform * frame, picker_surface)
    }

    fn paint_export(&mut self, ui: &mut Ui, r: Rect) -> bool {
        let Some(source) = self.source.as_ref() else {
            return true;
        };
        let art = self.art.as_ref().unwrap();
        let p = ui.painter().clone();
        let at = r.min;
        let title = |p: &Painter, x, y, s: &str, size, ink| {
            label(p, at + vec2(x, y), Align2::LEFT_CENTER, s, size, true, ink)
        };
        let mut close = close_button(ui, rect(at, 528., 9., 46., 35.));
        neon_pill(
            &p,
            rect(at, 28., 20., 91., 29.),
            C::from_rgb(160, 152, 255),
            "PC A PC",
            Some((art, 0)),
            12.,
        );
        neon_pill(
            &p,
            rect(at, 131., 20., 109., 29.),
            CYAN,
            "EXPORTAR",
            Some((art, 1)),
            12.,
        );
        art.symbol(&p, 2, at + vec2(47., 96.), 42., C::WHITE);
        title(&p, 84., 85., "Instancia Seleccionada", 20., WHITE);
        title(
            &p,
            84.,
            110.,
            "Esta instancia será enviada a tu amigo para que la instale.",
            13.5,
            MUTED,
        );
        let card = rect(at, 28., 135., 524., 78.);
        surface(&p, card, 14., FIELD, LINE);
        image_fill(&p, &art.cover, rect(at, 43., 148., 84., 52.), 9.);
        let info = p.with_clip_rect(card.shrink(10.));
        title(&info, 139., 164., &source.name, 14., WHITE);
        label(
            &info,
            at + vec2(139., 186.),
            Align2::LEFT_CENTER,
            &format!(
                "Minecraft {} · {} · {} mods",
                source.mc_version, source.loader, source.mod_count
            ),
            13.5,
            false,
            MUTED,
        );
        art.symbol(&p, 3, at + vec2(39., 238.), 22., LAVENDER);
        title(&p, 59., 238., "Usuario de destino", 15.5, WHITE);
        let response = input(
            ui,
            rect(at, 28., 257., 524., 46.),
            "recipient",
            &mut self.recipient,
            "Escribe el usuario de tu amigo...",
            15.,
            24,
            true,
            !self.busy,
        );
        let banner = rect(at, 28., 319., 524., 67.);
        surface(&p, banner, 14., FIELD, LINE);
        art.symbol(&p, 4, at + vec2(64., 351.), 34., C::WHITE);
        title(
            &p,
            94.,
            343.,
            "Escribe el nombre de usuario de tu amigo y pulsa",
            13.,
            WHITE,
        );
        title(&p, 399., 343., "Enviar invitación.", 13., LAVENDER);
        label(
            &p,
            at + vec2(94., 365.),
            Align2::LEFT_CENTER,
            "A tu amigo le llegará una notificación para aceptarla e instalarla al instante.",
            12.5,
            false,
            WHITE,
        );
        if !self.error.is_empty() {
            label(
                &p,
                at + vec2(28., 404.),
                Align2::LEFT_CENTER,
                &self.error,
                12.,
                false,
                if self.is_error {
                    C::from_rgb(253, 172, 172)
                } else {
                    C::from_rgb(110, 231, 183)
                },
            );
        }
        let y = r.height() - 60.;
        if button(
            ui,
            rect(at, 387., y, 165., 40.),
            "send",
            "Enviar invitación",
            true,
            !self.busy,
        )
        .clicked()
            || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            let name = self.recipient.trim().trim_start_matches('@');
            if !crate::minecraft::valid_username(name) {
                self.notice("Escribe un nombre de usuario válido.".into(), true);
            } else {
                self.notice("El envío PC a PC requiere WebView2.".into(), true);
            }
        }
        if button(
            ui,
            rect(at, 269., y, 114., 40.),
            "cancel",
            "Cancelar",
            false,
            !self.busy,
        )
        .clicked()
        {
            close = true;
        }
        close && !self.busy
    }

    fn paint_form(
        &mut self, ui: &mut Ui, r: Rect, window: &Window, elapsed: Duration, reduced_motion: bool,
    ) -> bool {
        if !self.custom {
            self.update_pack_catalog(ui.ctx());
            self.sync_pack_name();
            self.prepare_pack_textures(ui.ctx());
        }
        let p = ui.painter().clone();
        let at = r.min;
        let edit = self.mode == Mode::Edit;
        let title_w = if edit { 179. } else { 174. };
        neon_pill(
            &p,
            rect(at, 26., 18., title_w, 34.),
            CYAN,
            if edit {
                "EDITAR INSTANCIA"
            } else {
                "CREAR INSTANCIA"
            },
            None,
            16.,
        );
        if edit {
            let header = p.with_clip_rect(rect(at, 215., 5., r.width() - 245., 58.));
            label(
                &header,
                at + vec2(215., 35.),
                Align2::LEFT_CENTER,
                &self.original.name,
                20.,
                true,
                WHITE,
            );
        }
        let bottom = r.bottom() - 80.;
        let column = (r.width() - 68.) / 2.;
        let left = rect(at, 24., 78., column, bottom - at.y - 78.);
        let right = Rect::from_min_size(left.min + vec2(column + 20., 0.), left.size());
        let mut pick = None;
        let cover_h = (left.height() * 0.37 - 73.).clamp(62., 144.);
        let cover = rect(left.min, 0., 0., column, cover_h);
        image_fill(&p, &self.art.as_ref().unwrap().cover, cover, 12.);
        p.rect_stroke(
            cover,
            12.,
            Stroke::new(1., C::from_white_alpha(27)),
            StrokeKind::Inside,
        );
        let cover_button = interact(ui, cover, "cover").on_hover_text("Cambiar portada");
        if cover_button.hovered() {
            p.rect_filled(cover, 12., C::from_black_alpha(100));
            self.art
                .as_ref()
                .unwrap()
                .control(&p, 9, cover.center(), 22., WHITE);
        }
        if cover_button.clicked() && !self.busy {
            self.open_art_picker(ArtKind::Cover, reduced_motion);
            ui.ctx().request_repaint();
        }
        let name_at = cover.left_bottom() + vec2(0., 7.);
        let icon = rect(name_at, 8., 4., 50., 50.);
        let icon_click = interact(ui, icon, "icon").on_hover_text("Cambiar icono");
        if icon_click.clicked() && !self.busy {
            self.open_art_picker(ArtKind::Icon, reduced_motion);
            ui.ctx().request_repaint();
        }
        let project_icon = (!self.custom && self.draft.icon.is_none())
            .then(|| {
                self.packs
                    .project()
                    .and_then(|p| self.packs.thumbnails.get(&p.id))
            })
            .flatten();
        let art = self.art.as_ref().unwrap();
        let texture = project_icon.or_else(|| (self.custom || self.draft.icon.is_some()).then_some(&art.icon));
        let hovered = icon_click.hovered() && !self.busy;
        let hover = if reduced_motion { f32::from(hovered) } else {
            ui.ctx().animate_bool_with_time(icon_click.id.with("icon-hover"), hovered, 0.18)
        };
        instance_icon(&p, icon, texture, art, elapsed, hover, reduced_motion);
        label(
            &p,
            name_at + vec2(72., 15.),
            Align2::LEFT_CENTER,
            "Nombre de la instancia",
            12.5,
            true,
            WHITE,
        );
        let name_field = rect(name_at, 72., 27., column - 74., 30.);
        let name_input = input(
            ui,
            name_field,
            "name",
            &mut self.draft.name,
            "Mi instancia",
            13.5,
            50,
            false,
            !self.busy,
        );
        let name_changed = edit && self.original.name != self.draft.name;
        p.hline(
            name_field.x_range(),
            name_field.bottom(),
            if name_changed {
                Stroke::new(1.4, AMBER)
            } else if name_input.has_focus() {
                Stroke::new(2., C::from_rgb(117, 56, 255))
            } else {
                Stroke::new(1., C::from_white_alpha(48))
            },
        );
        if edit {
            diff_chip(
                &p,
                name_field.right_top() - vec2(0., 12.),
                true,
                (column * 0.5).max(68.),
                &self.original.name,
                Some(&self.draft.name),
            );
        }

        let content_y = name_at.y + 68.;
        let mut details_top = content_y;
        if !edit {
            label(
                &p,
                pos2(left.left(), content_y + 8.),
                Align2::LEFT_CENTER,
                "Tipo de instancia",
                12.5,
                true,
                WHITE,
            );
            let option_w = (column - 12.) / 2.;
            let art = self.art.as_ref().unwrap();
            for (i, (name, custom, idx)) in [("Modpack", false, 6), ("Custom", true, 7)]
                .into_iter()
                .enumerate()
            {
                let button = rect(
                    pos2(left.left(), content_y),
                    i as f32 * (option_w + 12.),
                    22.,
                    option_w,
                    50.,
                );
                let active = self.custom == custom;
                let response = interact(ui, button, ("type", i));
                surface(
                    &p,
                    button,
                    12.,
                    if active {
                        C::from_rgb(25, 16, 58)
                    } else {
                        CARD
                    },
                    if active {
                        VIOLET
                    } else if response.hovered() {
                        LINE
                    } else {
                        NONE
                    },
                );
                art.symbol(&p, idx, button.left_center() + vec2(30., 0.), 25., VIOLET);
                label(
                    &p,
                    button.left_center() + vec2(57., 0.),
                    Align2::LEFT_CENTER,
                    name,
                    16.,
                    true,
                    WHITE,
                );
                if response.clicked() && !self.busy && self.custom != custom {
                    if custom {
                        self.pack_name =
                            std::mem::replace(&mut self.draft.name, self.custom_name.clone());
                    } else {
                        self.custom_name =
                            std::mem::replace(&mut self.draft.name, self.pack_name.clone());
                    }
                    self.custom = custom;
                    ui.ctx().request_repaint();
                }
            }
            if self.custom && self.java.is_none() && !self.java_refreshing {
                self.refresh_java();
            }
            details_top += 79.;
        }
        if self.custom {
            self.packs.gallery.pause();
            self.paint_versions(
                ui,
                Rect::from_min_max(pos2(left.left(), details_top), left.max),
                reduced_motion,
            );
            if self.paint_java(ui, right) {
                pick = Some(Pick::Java);
            }
        } else {
            self.paint_modpacks(
                ui,
                rect(
                    pos2(left.left(), details_top),
                    0.,
                    0.,
                    column,
                    (left.bottom() - details_top).max(82.),
                ),
                right,
                !reduced_motion && ui.is_enabled() && !self.busy,
            );
        }

        if let Some(pick) = pick
            && let Some(path) = pick_file(window, pick)
        {
            match pick {
                Pick::Java => {
                    self.draft.java = path.to_string_lossy().into_owned();
                    self.draft.java_runtime = None;
                    self.java_pick_latest = false;
                }
                _ => {}
            }
        }

        let caption = if self.busy && !self.custom {
            "Instalando…"
        } else if self.busy {
            "Guardando…"
        } else if edit {
            "+   Guardar cambios"
        } else if self.custom {
            "+   Crear instancia"
        } else {
            "+   Instalar modpack"
        };
        let save = rect(r.right_bottom(), -207., -52., 183., 36.);
        let cancel = rect(save.left_center(), -110., -18., 98., 36.);
        let changed = self.draft != self.original;
        let click = form_submit(
            ui,
            save,
            caption,
            edit,
            !self.busy
                && if self.custom {
                    (!edit || changed)
                        && (self.draft.loader == "Vanilla"
                            || !self.draft.loader_version.is_empty()
                            || edit
                                && self.original.version == self.draft.version
                                && self.original.loader == self.draft.loader)
                } else {
                    self.packs.search_key.as_ref() == Some(&self.packs.key())
                        && self.packs.project().is_some()
                        && self.packs.release().is_some()
                },
        )
        .clicked();
        let close = button(ui, cancel, "cancel", "Cancelar", false, !self.busy).clicked();
        let message = p.with_clip_rect(Rect::from_min_max(
            pos2(r.left() + 23., bottom),
            pos2(cancel.left(), r.bottom()),
        ));
        if !self.error.is_empty() {
            label(
                &message,
                pos2(r.left() + 24., save.center().y),
                Align2::LEFT_CENTER,
                &self.error,
                12.,
                true,
                if self.is_error {
                    C::from_rgb(253, 172, 172)
                } else {
                    LAVENDER
                },
            );
        } else if edit {
            let changes = [
                (self.draft.name != self.original.name, "Nombre"),
                (self.draft.version != self.original.version, "Minecraft"),
                (
                    (&self.draft.loader, &self.draft.loader_version)
                        != (&self.original.loader, &self.original.loader_version),
                    "Loader",
                ),
                (
                    (self.draft.min_ram, self.draft.max_ram)
                        != (self.original.min_ram, self.original.max_ram),
                    "RAM",
                ),
                (self.draft.jvm != self.original.jvm, "JVM"),
                (
                    self.draft.java != self.original.java
                        || self.draft.java_runtime != self.original.java_runtime,
                    "Java",
                ),
                (self.draft.cover.is_some(), "Portada"),
                (self.draft.icon.is_some(), "Icono"),
            ]
            .into_iter()
            .filter_map(|(changed, label)| changed.then_some(label))
            .collect::<Vec<_>>();
            let at = pos2(r.left() + 24., save.center().y);
            if changes.is_empty() {
                label(
                    &message,
                    at,
                    Align2::LEFT_CENTER,
                    "Sin cambios todavía: lo que elijas distinto aparecerá aquí.",
                    12.,
                    false,
                    DIM,
                );
            } else {
                let count = format!(
                    "{} {}",
                    changes.len(),
                    if changes.len() == 1 {
                        "cambio"
                    } else {
                        "cambios"
                    }
                );
                label(&message, at, Align2::LEFT_CENTER, &count, 12., true, AMBER);
                let mut x = at.x + text_width(&message, &count, 12., true) + 9.;
                for change in changes {
                    let w = text_width(&message, change, 11., true) + 18.;
                    let tag = rect(pos2(x, at.y), 0., -10., w, 20.);
                    surface(
                        &message,
                        tag,
                        10.,
                        C::from_rgb(40, 31, 31),
                        C::from_rgb(110, 80, 34),
                    );
                    label(
                        &message,
                        tag.center(),
                        Align2::CENTER_CENTER,
                        change,
                        11.,
                        true,
                        C::from_rgb(253, 230, 138),
                    );
                    x += w + 6.;
                }
                if let Some(source) = &self.source
                    && source.mod_count > 0
                    && (self.original.version != self.draft.version
                        || self.original.loader != self.draft.loader
                        || self.original.loader_version != self.draft.loader_version)
                {
                    label(
                        &message,
                        at + vec2(0., 16.),
                        Align2::LEFT_CENTER,
                        &format!(
                            "Tienes {} mods: revisa que sean compatibles",
                            source.mod_count
                        ),
                        11.,
                        false,
                        C::from_rgb(252, 165, 165),
                    );
                }
            }
        }
        if click {
            if self.mode == Mode::Create
                && self.draft.cover.is_none()
                && let Some(index) = ArtKind::Cover.index(CREATE_COVER)
                && let Some((path, preset)) = self.prepare_art_preset(ArtKind::Cover, index)
            {
                self.draft.cover = Some(path);
                self.draft.cover_preset = Some(preset);
            }
            if self.custom {
                let draft = self.draft.clone();
                let original = self.original.clone();
                let source = self.source.clone().filter(|_| edit);
                self.busy = true;
                self.error.clear();
                let art_files = self.art_files.clone();
                self.background(move || {
                    let _art_files = art_files;
                    let saved = instances::save(&draft, source.as_ref().map(|s| (s, &original)))
                        .map(|path| {
                            (
                                path,
                                format!(
                                    "{}: {}",
                                    if edit {
                                        "Instancia editada"
                                    } else {
                                        "Instancia creada"
                                    },
                                    draft.name
                                ),
                            )
                        });
                    ResultMessage::Saved(saved)
                });
            } else if let (Some(project), Some(release)) =
                (self.packs.project(), self.packs.release())
            {
                let project = project.clone();
                let release = release.clone();
                let mut draft = self.draft.clone();
                draft.java.clear();
                draft.java_runtime = None;
                if draft.name == "Mi instancia" {
                    draft.name = project
                        .title
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(50)
                        .collect();
                }
                self.busy = true;
                self.notice("Preparando instalación…".into(), false);
                let tx = self.tx.clone();
                let wake = self.wake.clone();
                let art_files = self.art_files.clone();
                self.background(move || {
                    let _art_files = art_files;
                    let result = instances::install_pack(&draft, &project, &release, |message| {
                        if tx.send(ResultMessage::Progress(message)).is_ok() {
                            let _ = wake.send_event(());
                        }
                    });
                    ResultMessage::Saved(
                        result.map(|path| (path, format!("Modpack instalado: {}", draft.name))),
                    )
                });
            }
        }
        close
    }

    fn update_pack_catalog(&mut self, ctx: &egui::Context) {
        let key = self.packs.key();
        let failed = self
            .packs
            .search_error
            .as_ref()
            .is_some_and(|(failed, _)| failed == &key);
        let default = key.query.is_empty() && key.minecraft.is_empty();
        if (self.packs.search_after.is_some()
            || self.packs.search_key.as_ref() != Some(&key) && !failed)
            && (!self.packs.running || default)
        {
            let remaining = self.packs.search_after.map_or(Duration::ZERO, |when| {
                when.saturating_duration_since(Instant::now())
            });
            if remaining.is_zero() {
                self.query_packs();
            } else {
                ctx.request_repaint_after(remaining);
            }
        }
        if let Some((page, index)) = self.packs.next_selection.take() {
            self.packs.page = page;
            self.select_pack(index);
        }
    }

    fn query_packs(&mut self) {
        let key = self.packs.key();
        if key.query.is_empty() && key.minecraft.is_empty() {
            self.open_pack_catalog();
            return;
        }
        self.packs.search_error = None;
        self.packs.running = true;
        self.packs.search_after = None;
        self.background(move || {
            let results = modpack_catalog::search(key.provider, &key.query, &key.minecraft);
            ResultMessage::Packs(key, results)
        });
    }

    fn open_pack_catalog(&mut self) {
        let key = self.packs.key();
        let provider = key.provider;
        self.packs.activate(key, modpack_catalog::open(provider));
        self.packs.search_after = None;
        std::thread::spawn(move || modpack_catalog::refresh_next_open(provider));
    }

    fn sync_pack_name(&mut self) {
        if let Some(project) = self.packs.project() {
            let title: String = project
                .title
                .chars()
                .filter(|c| !c.is_control())
                .take(50)
                .collect();
            if self.draft.name == self.packs.automatic_name {
                self.draft.name.clone_from(&title);
            }
            self.packs.automatic_name = title;
        }
    }

    fn select_pack(&mut self, index: usize) {
        self.packs.selected = index;
        self.packs.reset_detail();
        if index == 0
            && let Some(catalog) = &self.packs.catalog
            && let Some(details) = catalog.first_details()
            && let Some(project) = catalog.projects.first()
            && let Some(key) = &self.packs.search_key
        {
            self.packs.detail = Some(details);
            if self
                .packs
                .detail
                .as_ref()
                .is_some_and(|detail| !detail.body.is_empty())
            {
                self.packs.detail_key = Some(pack_detail_key(project, &key.minecraft));
            }
        }
    }

    fn prepare_pack_textures(&mut self, ctx: &egui::Context) {
        let Some(key) = self
            .packs
            .search_key
            .clone()
            .map(|key| (key, self.packs.page))
        else {
            return;
        };
        if self.packs.thumbnail_key.as_ref() != Some(&key) {
            if self.packs.thumbnail_key.is_none() {
                self.packs.snapshot_hero = self
                    .packs
                    .catalog
                    .as_ref()
                    .and_then(Catalog::first_hero)
                    .map(|(url, image)| {
                        (
                            url,
                            load_image(
                                ctx,
                                "modpack-featured",
                                &image,
                                egui::TextureOptions::LINEAR,
                            ),
                        )
                    });
            }
            self.packs.thumbnail_key = Some(key);
            self.packs.thumbnails.clear();
            self.packs.thumbnail_attempted = false;
            self.packs.thumbnail_images = self
                .packs
                .catalog
                .as_ref()
                .map(|catalog| catalog.page_images(self.packs.page))
                .unwrap_or_default();
        }
        for (id, image) in self.packs.thumbnail_images.drain(..) {
            let texture = load_image(
                ctx,
                "modpack-thumbnail",
                &image,
                egui::TextureOptions::LINEAR,
            );
            self.packs.thumbnails.insert(id, texture);
        }
    }

    fn paint_modpacks(&mut self, ui: &mut Ui, left: Rect, right: Rect, animate_gallery: bool) {
        let p = ui.painter().clone();
        let at = left.min;
        let w = left.width();
        let filter_w = (w * 0.14).clamp(67., 85.);
        let curse_w = (w * 0.177).clamp(83., 107.);
        let modrinth_w = (w * 0.15).clamp(77., 91.);
        let search_w = w - filter_w - curse_w - modrinth_w - 21.;
        label(
            &p,
            at + vec2(0., 6.),
            Align2::LEFT_CENTER,
            "Buscar modpack",
            12.5,
            true,
            WHITE,
        );
        let field = rect(at, 0., 21., search_w, 30.);
        surface(&p, field, 10., FIELD, LINE);
        magnifier(&p, field.left_center() + vec2(18., 0.));
        let search = input(
            ui,
            Rect::from_min_max(field.min + vec2(33., 0.), field.max - vec2(8., 0.)),
            "modpack-query",
            &mut self.packs.query,
            "",
            12.,
            100,
            false,
            !self.busy,
        );
        if search.changed() {
            self.packs.search_after = Some(Instant::now() + Duration::from_millis(350));
            ui.ctx().request_repaint_after(Duration::from_millis(350));
        }
        if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.packs.search_after = Some(Instant::now());
            ui.ctx().request_repaint();
        }
        let filter = if self.packs.filter.is_empty() {
            "Todas"
        } else {
            &self.packs.filter
        };
        let filter_button = rect(at, search_w + 8., 21., filter_w, 30.);
        surface(&p, filter_button, 8., CARD, C::from_white_alpha(27));
        if small_button(
            ui,
            filter_button,
            "pack-filter",
            filter,
            self.packs.filter_open,
        )
        .clicked()
            && !self.busy
        {
            self.packs.filter_open = !self.packs.filter_open;
        }
        chevron(
            &p,
            filter_button.right_center() - vec2(10., 0.),
            self.packs.filter_open,
        );
        self.art.as_ref().unwrap().symbol(
            &p,
            9,
            filter_button.left_center() + vec2(15., 0.),
            13.,
            MUTED,
        );
        for (i, provider) in [Provider::CurseForge, Provider::Modrinth]
            .into_iter()
            .enumerate()
        {
            let button = rect(
                at,
                search_w + filter_w + 15. + if i == 0 { 0. } else { curse_w + 6. },
                21.,
                if i == 0 { curse_w } else { modrinth_w },
                30.,
            );
            let active = self.packs.provider == provider;
            let response = interact(ui, button, ("provider", i));
            surface(
                &p,
                button,
                8.,
                if active {
                    C::from_rgb(32, 23, 59)
                } else {
                    CARD
                },
                if active {
                    C::from_rgb(168, 85, 247)
                } else {
                    C::from_white_alpha(28)
                },
            );
            p.image(
                self.art.as_ref().unwrap().platforms[i].id(),
                Rect::from_center_size(button.left_center() + vec2(16., 0.), vec2(15., 15.)),
                UV,
                WHITE,
            );
            label(
                &p.with_clip_rect(button),
                button.left_center() + vec2(29., 0.),
                Align2::LEFT_CENTER,
                provider.label(),
                11.,
                true,
                if active { WHITE } else { MUTED },
            );
            if response.clicked() && !self.busy && !active {
                self.packs.provider = provider;
                self.packs.filter_open = false;
                self.packs.search_after = Some(Instant::now());
                ui.ctx().request_repaint();
            }
        }

        let key = self.packs.key();
        let compact_grid = left.height() < 180.;
        let panel = Rect::from_min_max(
            at + vec2(1., if compact_grid { 78. } else { 93. }),
            left.max - vec2(1., 1.),
        );
        let list = self.packs.projects();
        let pages = list.len().div_ceil(PAGE_SIZE);
        label(
            &p,
            at + vec2(0., if compact_grid { 63. } else { 74. }),
            Align2::LEFT_CENTER,
            if self.packs.filter_open {
                "Versión de Minecraft"
            } else {
                "Resultados"
            },
            12.5,
            true,
            WHITE,
        );
        if !self.packs.filter_open && !list.is_empty() {
            let count = format!("{} modpacks", list.len());
            tag(
                &p,
                at + vec2(69., if compact_grid { 53. } else { 64. }),
                &count,
                10.,
            );
        }
        if !self.packs.filter_open && pages > 1 {
            let page = self.packs.page;
            let tab_width = 27.;
            let width = tab_width * pages as f32 + 48.;
            let tabs = rect(
                left.right_top(),
                -width,
                if compact_grid { 51. } else { 59. },
                width,
                28.,
            );
            surface(&p, tabs, 8., FIELD, LINE);
            for i in 0..pages + 2 {
                let offset = match i {
                    0 => 2.,
                    i if i == pages + 1 => width - 25.,
                    i => 24. + tab_width * (i - 1) as f32,
                };
                let tab = rect(tabs.min, offset, 4., 23., 20.);
                let (go, caption) = match i {
                    0 => (page.checked_sub(1), "‹".to_owned()),
                    i if i == pages + 1 => ((page + 1 < pages).then_some(page + 1), "›".to_owned()),
                    i => (Some(i - 1), i.to_string()),
                };
                let active = i == page + 1;
                let response = interact(ui, tab, ("pack-page", i));
                if active || (go.is_some() && response.hovered()) {
                    surface(
                        &p,
                        tab,
                        5.,
                        C::from_rgb(37, 22, 70),
                        if active {
                            C::from_rgb(168, 85, 247)
                        } else {
                            LINE
                        },
                    );
                }
                label(
                    &p,
                    tab.center(),
                    Align2::CENTER_CENTER,
                    &caption,
                    11.,
                    true,
                    if active {
                        WHITE
                    } else if go.is_some() {
                        MUTED
                    } else {
                        DIM.gamma_multiply(0.45)
                    },
                );
                if response.clicked()
                    && !self.busy
                    && let Some(go) = go.filter(|p| *p != page)
                {
                    self.packs.next_selection = Some((go, go * PAGE_SIZE));
                    ui.ctx().request_repaint();
                }
            }
        }

        if self.packs.filter_open {
            let mut version = None;
            rows(ui, panel, "pack-mc-versions", |ui| {
                for mc in std::iter::once("").chain(self.versions.iter().map(String::as_str)) {
                    if list_row(
                        ui,
                        panel.width(),
                        27.,
                        ("pack-mc", mc),
                        if mc.is_empty() {
                            "Todas las versiones"
                        } else {
                            mc
                        },
                        "",
                        mc == self.packs.filter,
                    )
                    .clicked()
                        && !self.busy
                    {
                        version = Some(mc.to_owned());
                    }
                }
            });
            if let Some(v) = version {
                self.packs.filter_open = false;
                if self.packs.filter != v {
                    self.packs.filter = v;
                    self.packs.search_after = Some(Instant::now());
                    ui.ctx().request_repaint();
                }
            }
        } else {
            let mut selected = None;
            if !self.packs.projects().is_empty() {
                selected = self.paint_pack_grid(ui, panel);
            } else if self.packs.search_key.as_ref() != Some(&key)
                || self.packs.running
                || self.packs.search_after.is_some()
            {
                center_notice(ui, panel, "Buscando modpacks…");
            } else {
                center_notice(ui, panel, "No se encontraron modpacks.");
            }
            if let Some(i) = selected
                && self.packs.selected != i
            {
                self.packs.next_selection = Some((self.packs.page, i));
                ui.ctx().request_repaint();
            }
        }
        if !self.packs.filter_open && self.packs.search_key.as_ref() != Some(&self.packs.key()) {
            let status = rect(
                at,
                155.,
                if compact_grid { 53. } else { 64. },
                (w - 325.).max(62.),
                21.,
            );
            if let Some((_, error)) = self
                .packs
                .search_error
                .as_ref()
                .filter(|(failed, _)| failed == &key)
            {
                if small_button(ui, status, "retry-packs", "Reintentar", false)
                    .on_hover_text(error)
                    .clicked()
                {
                    self.query_packs();
                }
            } else {
                center_notice(ui, status, "Buscando…");
            }
        }

        let Some(project) = self.packs.project().cloned() else {
            surface(
                &p,
                right,
                14.,
                C::from_rgb(10, 10, 29),
                C::from_rgb(42, 26, 73),
            );
            let center = right.center();
            p.rect_filled(
                Rect::from_center_size(center - vec2(0., 44.), vec2(68., 68.)),
                16.,
                C::from_rgb(24, 18, 50),
            );
            self.art
                .as_ref()
                .unwrap()
                .symbol(&p, 8, center - vec2(0., 44.), 44., WHITE);
            label(
                &p,
                center + vec2(0., 10.),
                Align2::CENTER_CENTER,
                "Selecciona un modpack",
                15.,
                true,
                WHITE,
            );
            return;
        };
        let mc = self
            .packs
            .search_key
            .as_ref()
            .map(|key| key.minecraft.as_str())
            .unwrap_or_default();
        let detail_key = pack_detail_key(&project, mc);
        if self.packs.detail_key.as_ref() != Some(&detail_key) && !self.packs.detail_running {
            self.packs.detail_key = Some(detail_key.clone());
            self.packs.detail_running = true;
            let (project, mc) = (project.clone(), mc.to_owned());
            self.background(move || {
                ResultMessage::PackDetails(detail_key, modpacks::details(&project, &mc))
            });
        }
        self.paint_pack_details(ui, right, &project, animate_gallery);
    }

    fn paint_pack_grid(&mut self, ui: &mut Ui, panel: Rect) -> Option<usize> {
        self.prepare_pack_textures(ui.ctx());
        let page = self.packs.page;
        let start = page * PAGE_SIZE;
        if !self.packs.thumbnail_running
            && !self.packs.thumbnail_attempted
            && let Some(key) = self.packs.thumbnail_key.clone()
            && let Some(catalog) = self.packs.catalog.as_ref()
        {
            self.packs.thumbnail_attempted = true;
            let entries = catalog
                .page(page)
                .iter()
                .filter(|project| !self.packs.thumbnails.contains_key(&project.id))
                .map(|project| (project.id.clone(), project.icon.clone()))
                .collect::<Vec<_>>();
            if !entries.is_empty() {
                self.packs.thumbnail_running = true;
                self.background(move || {
                    ResultMessage::Thumbnails(key, modpacks::thumbnails(&entries))
                });
            }
        }

        let gap = 7.;
        let cell_w = (panel.width() - gap * 4.) / 5.;
        let cell_h = (panel.height() - gap * 2.) / 3.;
        let p = ui.painter().with_clip_rect(panel);
        let tiles = &self.packs.thumbnails;
        let mut selected = None;
        for (offset, project) in self
            .packs
            .projects()
            .iter()
            .skip(start)
            .take(PAGE_SIZE)
            .enumerate()
        {
            let i = start + offset;
            let cell = rect(
                panel.min,
                (offset % 5) as f32 * (cell_w + gap),
                (offset / 5) as f32 * (cell_h + gap),
                cell_w,
                cell_h,
            );
            let response = interact(ui, cell, ("pack-card", i));
            let active = self.packs.selected == i;
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    !self.busy,
                    active,
                    &project.title,
                )
            });
            let hover = response.hovered();
            surface(
                &p,
                cell,
                9.,
                if active {
                    C::from_rgb(28, 17, 55)
                } else if hover {
                    C::from_rgb(25, 18, 46)
                } else {
                    CARD
                },
                if active {
                    C::from_rgb(168, 85, 247)
                } else if hover {
                    LINE
                } else {
                    C::from_white_alpha(19)
                },
            );
            let small = cell_h < 49.;
            let thumb = if small {
                Rect::from_min_size(
                    cell.min + vec2(5., 5.),
                    vec2((cell_h - 10.).min(24.), cell_h - 10.),
                )
            } else {
                Rect::from_min_max(cell.min + vec2(6., 6.), cell.max - vec2(6., 25.))
            };
            let texture = tiles.get(&project.id);
            if let Some(texture) = texture.filter(|_| thumb.height() > 2.) {
                image_fill(&p, texture, thumb, 6.);
            }
            let at = if texture.is_none() {
                cell.left_center() + vec2(7., 0.)
            } else if small {
                pos2(thumb.right() + 4., cell.center().y)
            } else {
                pos2(cell.left() + 7., cell.bottom() - 11.)
            };
            let title_room = cell.right() - at.x - 6.;
            let title = ellipsize(&p, &project.title, 11., true, title_room);
            label(
                &p.with_clip_rect(cell.shrink(4.)),
                at,
                Align2::LEFT_CENTER,
                &title,
                11.,
                true,
                WHITE,
            );
            if response
                .on_hover_text(format!(
                    "{} · {} descargas",
                    project.title, project.downloads
                ))
                .clicked()
                && !self.busy
            {
                selected = Some(i);
            }
        }
        selected
    }

    fn paint_pack_details(
        &mut self,
        ui: &mut Ui,
        right: Rect,
        project: &Project,
        animate_gallery: bool,
    ) {
        let p = ui.painter().with_clip_rect(right);
        let width = right.width();
        let tight = right.height() < 460.;
        let gap = if tight { 7. } else { 10. };
        let line = C::from_white_alpha(20);
        let cover_height = (right.height() * if tight { 0.30 } else { 0.34 }).clamp(80., 190.);
        let featured = (self.packs.selected == 0)
            .then_some(self.packs.snapshot_hero.as_ref())
            .flatten();
        let shots: Vec<_> = self
            .packs
            .detail
            .as_ref()
            .map(|d| d.gallery.as_slice())
            .unwrap_or(&project.gallery)
            .iter()
            .filter(|url| **url != project.icon)
            .cloned()
            .collect();
        let count = shots.len().max(1);
        if let Some(detail) = self.packs.detail.as_ref()
            && let Some(key) = self.packs.detail_key.as_ref()
            && !self.packs.detail_running
            && !self.packs.media_running
            && self.packs.media_key.as_ref() != Some(key)
        {
            let key = key.clone();
            let entries = detail
                .body
                .iter()
                .filter_map(|block| match block {
                    DescriptionBlock::Image { url, .. } => Some((url.clone(), url.clone(), 560)),
                    _ => None,
                })
                .take(8)
                .chain(shots.iter().map(|url| {
                    (
                        url.clone(),
                        detail.gallery_thumbnails.get(url).unwrap_or(url).clone(),
                        116,
                    )
                }))
                .collect::<Vec<_>>();
            self.packs.media_key = Some(key.clone());
            if !entries.is_empty() {
                self.packs.media_running = true;
                self.background(move || {
                    ResultMessage::PackMedia(key, modpacks::detail_images(&entries))
                });
            }
        }
        for (key, image) in self.packs.media_images.drain(..) {
            let texture = load_image(
                ui.ctx(),
                "modpack-media",
                &image,
                egui::TextureOptions::LINEAR,
            );
            self.packs.media_textures.insert(key, texture);
        }

        let frame = self.packs.gallery.frame(
            ui.ctx(),
            &shots,
            featured,
            &self.packs.media_textures,
            animate_gallery,
        );
        if let Some(url) = frame.request {
            self.background(move || {
                let image = modpacks::artwork(&url, 720);
                ResultMessage::PackImage(url, image)
            });
        }
        let artwork = self.packs.thumbnails.get(&project.id);
        let gallery_h = cover_height
            + if count > 1 {
                if tight { 38. } else { 48. }
            } else {
                13.
            };
        let gallery = rect(right.min, 0., 0., width - 2., gallery_h);
        let hero = rect(right.min, 0., 0., gallery.width(), cover_height);
        surface(&p, gallery, 14., CARD, line);
        if let Some(texture) = frame
            .texture
            .as_ref()
            .or_else(|| featured.map(|(_, texture)| texture))
            .or(artwork)
        {
            if let Some((previous, direction, t)) = &frame.transition {
                gallery_image(
                    &p,
                    previous,
                    hero,
                    -direction * 0.28 * t,
                    1.04 + 0.01 * t,
                    1. - t,
                );
                gallery_image(
                    &p,
                    texture,
                    hero,
                    direction * 0.28 * (1. - t),
                    1.01 + 0.07 * (1. - t),
                    *t,
                );
            } else {
                gallery_image(&p, texture, hero, 0., 1.04, 1.);
            }
        }
        let shot_text = format!("Captura {}/{count}", frame.index + 1);
        let shot_badge = rect(hero.right_bottom(), -86., -27., 76., 22.);
        surface(
            &p,
            shot_badge,
            6.,
            C::from_black_alpha(200),
            C::from_white_alpha(48),
        );
        label(
            &p,
            shot_badge.center(),
            Align2::CENTER_CENTER,
            &shot_text,
            10.5,
            true,
            WHITE,
        );
        if let Some(progress) = frame.progress {
            let track = rect(hero.left_bottom(), 0., -2.5, hero.width(), 2.5);
            p.rect_filled(track, 0., C::from_white_alpha(23));
            if progress > 0. {
                let fill = rect(track.min, 0., 0., track.width() * progress, track.height());
                horizontal_gradient(
                    &p,
                    fill,
                    C::from_rgb(124, 58, 237),
                    C::from_rgb(192, 132, 252),
                );
            }
        }
        if count > 1 {
            let strip = Rect::from_min_max(
                hero.left_bottom() + vec2(9., 6.),
                gallery.right_bottom() - vec2(5., 5.),
            );
            let mut pick = None;
            ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
                ui.set_clip_rect(strip);
                ui.style_mut().always_scroll_the_only_direction = true;
                egui::ScrollArea::horizontal()
                    .id_salt(("pack-gallery", &project.id))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 6.;
                        ui.horizontal(|ui| {
                            for (i, url) in shots.iter().enumerate() {
                                let (tile, button) = ui.allocate_exact_size(
                                    vec2(54., (strip.height() - 2.).min(35.)),
                                    Sense::click(),
                                );
                                let selected = frame.index == i;
                                if selected && frame.changed {
                                    button.scroll_to_me(Some(egui::Align::Center));
                                }
                                let painter = ui.painter();
                                let texture = self
                                    .packs
                                    .media_textures
                                    .get(url)
                                    .or_else(|| {
                                        featured
                                            .filter(|(ready, _)| ready == url)
                                            .map(|(_, texture)| texture)
                                    })
                                    .or_else(|| {
                                        selected.then_some(frame.texture.as_ref()).flatten()
                                    });
                                painter.rect_filled(tile, 5., FIELD);
                                if let Some(texture) = texture {
                                    image_fill(painter, texture, tile, 5.);
                                }
                                if !selected && !button.hovered() {
                                    painter.rect_filled(tile, 5., C::from_black_alpha(82));
                                }
                                painter.rect_stroke(
                                    tile,
                                    5.,
                                    Stroke::new(
                                        1.,
                                        if selected {
                                            C::from_rgb(177, 101, 255)
                                        } else {
                                            line
                                        },
                                    ),
                                    StrokeKind::Inside,
                                );
                                if button
                                    .on_hover_text(format!("Captura {} de {count}", i + 1))
                                    .clicked()
                                {
                                    pick = Some(i);
                                }
                            }
                        });
                    });
            });
            if let Some(i) = pick {
                self.packs.gallery.pick(i);
                ui.ctx().request_repaint();
            }
        }

        let meta_h = if tight { 53. } else { 66. };
        let meta = rect(gallery.left_bottom(), 0., gap, gallery.width(), meta_h);
        surface(&p, meta, 13., CARD, line);
        let pic = Rect::from_center_size(meta.left_center() + vec2(36., 0.), vec2(42., 42.));
        if let Some(texture) = artwork {
            image_fill(&p, texture, pic, 9.);
            p.rect_stroke(pic, 9., Stroke::new(1., VIOLET), StrokeKind::Inside);
        }
        let title_pos = meta.left_center() + vec2(70., -8.);
        let site = rect(meta.right_center(), -133., -14., 118., 27.);
        let title_clip =
            Rect::from_min_max(title_pos - vec2(1., 19.), site.left_bottom() - vec2(7., 0.));
        let provider = project.provider.label();
        let platform_w = if project.provider == Provider::CurseForge {
            74.
        } else {
            65.
        };
        let available = (title_clip.right() - title_pos.x - platform_w - 10.).max(30.);
        let title = ellipsize(&p, &project.title, 15., true, available);
        let title_w = text_width(&p, &title, 15., true);
        label(
            &p.with_clip_rect(title_clip),
            title_pos,
            Align2::LEFT_CENTER,
            &title,
            15.,
            true,
            WHITE,
        );
        let stamp = rect(title_pos, title_w + 8., -9., platform_w, 18.);
        let platform_color = if project.provider == Provider::CurseForge {
            C::from_rgb(249, 133, 49)
        } else {
            C::from_rgb(65, 214, 128)
        };
        surface(
            &p,
            stamp,
            9.,
            platform_color.gamma_multiply(0.10),
            platform_color.gamma_multiply(0.52),
        );
        label(
            &p,
            stamp.center(),
            Align2::CENTER_CENTER,
            provider,
            10.5,
            true,
            platform_color,
        );
        label(
            &p.with_clip_rect(meta),
            title_pos + vec2(0., 19.),
            Align2::LEFT_CENTER,
            &format!(
                "Por {}  ·  {} descargas",
                project.author,
                compact_count(project.downloads)
            ),
            11.,
            false,
            MUTED,
        );
        let site_click = interact(ui, site, "pack-web").on_hover_text(&project.web_url);
        surface(
            &p,
            site,
            8.,
            C::from_white_alpha(10),
            if site_click.hovered() {
                LINE
            } else {
                C::from_white_alpha(30)
            },
        );
        self.art
            .as_ref()
            .unwrap()
            .control(&p, 8, site.left_center() + vec2(18., 0.), 12., MUTED);
        label(
            &p,
            site.left_center() + vec2(31., 0.),
            Align2::LEFT_CENTER,
            "Página oficial",
            11.5,
            true,
            MUTED,
        );
        if site_click.clicked() {
            open_pack_url(&project.web_url);
        }

        let version_h = if tight { 61. } else { 74. };
        let version_card = rect(
            right.left_bottom(),
            0.,
            -version_h,
            gallery.width(),
            version_h,
        );
        let desc_card = Rect::from_min_max(
            meta.left_bottom() + vec2(0., gap),
            version_card.right_top() - vec2(0., gap),
        );
        surface(&p, version_card, 13., CARD, line);
        if desc_card.height() > 20. {
            surface(&p, desc_card, 13., CARD, line);
            label(
                &p.with_clip_rect(desc_card),
                desc_card.min + vec2(14., 19.),
                Align2::LEFT_CENTER,
                if self.packs.release_open {
                    "VERSIONES DEL MODPACK"
                } else {
                    "DESCRIPCIÓN OFICIAL"
                },
                11.5,
                true,
                LAVENDER,
            );
        }
        let content = Rect::from_min_max(
            desc_card.min + vec2(14., 38.),
            desc_card.max - vec2(12., 8.),
        );
        let mut picked = None;
        if self.packs.release_open {
            if let Some(Ok(versions)) = self.packs.detail.as_ref().map(|d| &d.versions) {
                rows(ui, content, ("pack-releases", &project.id), |ui| {
                    for (i, version) in versions.iter().enumerate() {
                        if list_row(
                            ui,
                            content.width(),
                            32.,
                            ("pack-release", i),
                            &version.title,
                            &format!("Minecraft {} · {}", version.minecraft, version.loader),
                            i == self.packs.release,
                        )
                        .on_hover_text(&version.title)
                        .clicked()
                            && !self.busy
                        {
                            picked = Some(i);
                        }
                    }
                });
            }
        } else {
            let detail = self.packs.detail.as_ref();
            rows(ui, content, ("pack-description", &project.id), |ui| {
                ui.set_width(content.width() - 5.);
                if let Some(detail) = detail.filter(|d| !d.body.is_empty()) {
                    paint_pack_description(ui, &detail.body, &self.packs.media_textures);
                } else {
                    ui.label(
                        egui::RichText::new(
                            detail.map_or(&project.description, |d| &d.description),
                        )
                        .size(13.)
                        .color(C::from_rgb(209, 216, 229)),
                    );
                }
            });
        }
        if let Some(i) = picked {
            self.packs.release = i;
            self.packs.release_open = false;
        }
        label(
            &p,
            version_card.min + vec2(14., if tight { 14. } else { 18. }),
            Align2::LEFT_CENTER,
            "Versión específica del modpack:",
            11.5,
            true,
            WHITE,
        );
        let mut footer = rect(
            version_card.left_bottom(),
            14.,
            if tight { -36. } else { -44. },
            version_card.width() - 29.,
            if tight { 27. } else { 34. },
        );
        let mut retry = false;
        if let Some(release) = self.packs.release() {
            let minecraft = format!("Minecraft {}", release.minecraft);
            let loader = release.loader.clone();
            let title = format!("{} • {} ({})", release.title, minecraft, loader);
            let reserve = if width > 460. { 174. } else { 132. };
            footer.max.x -= reserve;
            surface(&p, footer, 8., FIELD, LINE);
            let response = interact(ui, footer, "pack-version-picker").on_hover_text(&title);
            label(
                &p.with_clip_rect(footer.shrink2(vec2(12., 1.))),
                footer.left_center() + vec2(13., 0.),
                Align2::LEFT_CENTER,
                &ellipsize(&p, &title, 11.5, false, footer.width() - 35.),
                11.5,
                false,
                WHITE,
            );
            chevron(
                &p,
                footer.right_center() - vec2(12., 0.),
                self.packs.release_open,
            );
            let mc_badge = rect(footer.right_top(), 10., 0., reserve - 69., footer.height());
            let ldr_badge = rect(mc_badge.right_top(), 6., 0., 53., footer.height());
            for (r, text) in [(mc_badge, minecraft), (ldr_badge, loader)] {
                surface(&p, r, 8., C::from_rgb(27, 23, 54), LINE);
                label(
                    &p.with_clip_rect(r),
                    r.center(),
                    Align2::CENTER_CENTER,
                    &text,
                    11.,
                    true,
                    LAVENDER,
                );
            }
            if response.clicked() && !self.busy {
                self.packs.release_open = !self.packs.release_open;
            }
        } else if let Some(Err(error)) = self.packs.detail.as_ref().map(|d| &d.versions) {
            retry = small_button(
                ui,
                footer,
                "retry-pack-details",
                "No se pudieron cargar las versiones · Reintentar",
                false,
            )
            .on_hover_text(error)
            .clicked();
        } else {
            surface(&p, footer, 8., FIELD, LINE);
            center_notice(
                ui,
                footer,
                if self.packs.detail.is_none() {
                    "Consultando versiones…"
                } else {
                    "No hay versiones para este filtro."
                },
            );
        }
        if retry {
            self.packs.reset_detail();
            ui.ctx().request_repaint();
        }
    }

    fn paint_versions(&mut self, ui: &mut Ui, area: Rect, reduced_motion: bool) {
        if area.height() < 60. {
            return;
        }
        let p = ui.painter().clone();
        let w = (area.width() - 12.) / 2.;
        let left = rect(area.min, 0., 0., w, area.height());
        let right = left.translate(vec2(w + 12., 0.));
        let art = self.art.as_ref().unwrap();
        let edit = self.mode == Mode::Edit;
        surface(&p, right, 14., CARD, NONE);
        art.symbol(&p, 10, right.min + vec2(30., 29.), 25., VIOLET);
        label(
            &p.with_clip_rect(right),
            right.min + vec2(57., if edit { 23. } else { 29. }),
            Align2::LEFT_CENTER,
            "Loader",
            16.,
            true,
            WHITE,
        );

        surface(&p, left, 14., C::from_rgb(12, 12, 29), NONE);
        art.symbol(&p, 9, left.min + vec2(28., 28.), 26., VIOLET);
        let heading = p.with_clip_rect(left);
        label(
            &heading,
            left.min + vec2(54., 21.),
            Align2::LEFT_CENTER,
            "Versión de Minecraft",
            16.,
            true,
            WHITE,
        );
        if edit {
            changed_outline(&p, left, 14., self.draft.version != self.original.version);
            diff_chip(
                &p,
                left.min + vec2(54., 40.),
                false,
                w - 66.,
                &self.original.version,
                Some(&self.draft.version),
            );
        } else {
            label(
                &heading,
                left.min + vec2(54., 39.),
                Align2::LEFT_CENTER,
                "Selecciona la versión que quieres usar.",
                11.5,
                false,
                MUTED,
            );
        }
        let field = rect(left.min, 12., 53., w - 26., 28.);
        magnifier(&p, field.left_center() + vec2(6., 0.));
        let search = input(
            ui,
            Rect::from_min_max(field.min + vec2(21., 0.), field.max),
            "version-search",
            &mut self.query,
            "",
            12.5,
            30,
            false,
            !self.busy,
        )
        .on_hover_text("Buscar versión…");
        p.hline(
            field.x_range(),
            field.bottom(),
            Stroke::new(1., C::from_white_alpha(48)),
        );
        let drum = rect(left.min, 12., 86., w - 26., (left.height() - 92.).max(0.));
        if drum.height() >= 36. {
            let query = self.query.trim();
            let current = &self.draft.version;
            let source = self.versions.iter().filter(|v| v.contains(query));
            let mut choices = source.collect::<Vec<_>>();
            if !current.is_empty() && current.contains(query) && !choices.contains(&current) {
                choices.insert(0, current);
            }
            let active = choices.iter().position(|v| **v == *current);
            let i = active.unwrap_or(0);
            let painter = p.with_clip_rect(drum);
            if choices.is_empty() {
                center_notice(ui, drum, "No se encontraron versiones coincidentes.");
            } else {
                const ROW: f32 = 44.;
                let center = Rect::from_center_size(drum.center(), vec2(w - 52., ROW));
                let hit = Rect::from_min_max(
                    pos2(field.left(), field.bottom() + 1.),
                    pos2(field.right(), drum.bottom()),
                );
                let response = ui.interact(
                    hit,
                    ui.make_persistent_id("minecraft-version-drum"),
                    Sense::click_and_drag(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        !self.busy,
                        true,
                        choices[i],
                    )
                });
                let limit = (choices.len() - 1) as f32;
                let mut target = i as f32;
                let animation = response.id.with(("scroll", query));
                let mut dragging = None;
                if !self.busy && response.enabled() {
                    if response.drag_started() {
                        self.version_drag_origin = Some(i as f32);
                        self.version_drag_position = i as f32;
                        response.request_focus();
                    }
                    if let Some(origin) = self.version_drag_origin {
                        if let Some(delta) = response.total_drag_delta() {
                            self.version_drag_position = (origin - delta.y / ROW).clamp(0., limit);
                        }
                        target = self.version_drag_position;
                        dragging = response.dragged().then_some(target);
                    }
                    if !response.dragged() {
                        self.version_drag_origin = None;
                    }
                    if response.clicked() {
                        response.request_focus();
                        if let Some(pointer) = response.interact_pointer_pos() {
                            let shown = ui.ctx().animate_value_with_time(
                                animation,
                                i as f32,
                                if reduced_motion { 0. } else { 0.13 },
                            );
                            target = (shown + (pointer.y - center.center().y) / ROW).round();
                        }
                    } else if dragging.is_none() && !response.drag_stopped() {
                        let mut steps = 0.;
                        if response.hovered() {
                            let wheel = ui.input(|inp| {
                                inp.events
                                    .iter()
                                    .filter_map(|e| match e {
                                        egui::Event::MouseWheel { delta, unit, .. } => {
                                            Some(match unit {
                                                egui::MouseWheelUnit::Point => delta.y / ROW,
                                                egui::MouseWheelUnit::Line => delta.y,
                                                egui::MouseWheelUnit::Page => delta.y * 7.,
                                            })
                                        }
                                        _ => None,
                                    })
                                    .sum::<f32>()
                            });
                            self.version_wheel += wheel;
                            let whole = self.version_wheel.trunc();
                            self.version_wheel -= whole;
                            steps -= whole;
                        } else {
                            self.version_wheel = 0.;
                        }
                        if response.has_focus() || search.has_focus() {
                            steps += ui.input_mut(|inp| {
                                (inp.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                                    as i32
                                    - inp.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                                        as i32) as f32
                            });
                        }
                        target += steps;
                    }
                }
                let selected = target.round().clamp(0., limit) as usize;
                let shown = if let Some(dragging) = dragging {
                    ui.ctx().animate_value_with_time(animation, dragging, 0.);
                    dragging
                } else {
                    ui.ctx().animate_value_with_time(
                        animation,
                        selected as f32,
                        if reduced_motion || search.changed() {
                            0.
                        } else {
                            0.13
                        },
                    )
                };
                paint_version_lens(&painter, drum, center);
                for index in (shown.floor() as usize).saturating_sub(3)
                    ..=(shown.ceil() as usize + 3).min(choices.len() - 1)
                {
                    let offset = index as f32 - shown;
                    let distance = offset.abs();
                    let neighbor = C::from_rgba_unmultiplied(148, 163, 184, 107);
                    let far = C::from_rgba_unmultiplied(100, 116, 139, 37);
                    let (size, ink) = if distance < 1. {
                        (
                            24. - 8. * distance,
                            C::WHITE.lerp_to_gamma(neighbor, distance),
                        )
                    } else if distance < 2. {
                        (
                            16. - 3. * (distance - 1.),
                            neighbor.lerp_to_gamma(far, distance - 1.),
                        )
                    } else {
                        (13., far.gamma_multiply((3. - distance).clamp(0., 1.)))
                    };
                    let y = center.center().y + ROW * offset;
                    let at = pos2(center.center().x, y + 1.);
                    let fade = ((1. - (y - drum.center().y).abs() / (drum.height() * 0.5)) / 0.26)
                        .clamp(0., 1.);
                    if distance < 0.7 {
                        paint_version_glow(
                            &painter,
                            at,
                            choices[index],
                            size,
                            (1. - distance / 0.7).max(0.),
                        );
                    }
                    label(
                        &painter,
                        at,
                        Align2::CENTER_CENTER,
                        choices[index],
                        size,
                        true,
                        ink.gamma_multiply(fade),
                    );
                }
                if !self.busy
                    && response.enabled()
                    && dragging.is_none()
                    && (active.is_some() || search.changed() || target != i as f32)
                    && choices[selected] != current
                {
                    self.draft.version = choices[selected].to_string();
                    self.draft.loader_version.clear();
                    self.java_major_anchor = None;
                }
            }
        }
        self.paint_loaders(ui, right);
    }

    fn paint_loaders(&mut self, ui: &mut Ui, panel: Rect) {
        let p = ui.painter().with_clip_rect(panel);
        let mc = self.draft.version.clone();
        if self.loader_request.is_none() && !self.loader_versions.contains_key(&mc) {
            self.loader_request = Some(mc.clone());
            let request = mc.clone();
            self.background(move || {
                let client = instance_catalog::loader_agent();
                let set = std::thread::scope(|scope| {
                    let fetch =
                        |loader| instance_catalog::loader_versions(&client, loader, &request);
                    let workers = LOADERS[1..]
                        .iter()
                        .map(|loader| scope.spawn(move || fetch(loader)))
                        .collect::<Vec<_>>();
                    let mut versions = workers.into_iter().map(|w| {
                        w.join()
                            .unwrap_or_else(|_| Err("No se pudo consultar el loader.".into()))
                    });
                    std::array::from_fn(|_| versions.next().unwrap())
                });
                ResultMessage::Loaders(request, set)
            });
        }
        let set = self.loader_versions.get(&mc);
        let edit = self.mode == Mode::Edit;
        let compact = panel.height() < 210.;
        let is_current = self.original.version == mc
            && self.original.loader == self.draft.loader
            && self.mode == Mode::Edit;
        if is_current && self.draft.loader_version.is_empty() {
            self.draft
                .loader_version
                .clone_from(&self.original.loader_version);
        }
        // Keep the installed loader on Edit, including a version the server has since removed.
        if let Some(set) = set
            && let Some(i) = LOADERS.iter().position(|name| *name == self.draft.loader)
            && i != 0
            && !is_current
            && self.draft.loader_version.is_empty()
        {
            if set[i - 1].as_ref().is_ok_and(Vec::is_empty)
                && let Some(replacement) = set
                    .iter()
                    .position(|v| v.as_ref().is_ok_and(|v| !v.is_empty()))
            {
                self.draft.loader = LOADERS[replacement + 1].into();
            }
            if let Some(index) = LOADERS.iter().position(|name| *name == self.draft.loader)
                && index != 0
                && let Ok(versions) = &set[index - 1]
                && let Some(v) = versions
                    .iter()
                    .find(|v| v.recommended)
                    .or_else(|| versions.first())
            {
                self.draft.loader_version.clone_from(&v.version);
            }
        }

        let size = (panel.width() - 30. - 12.) / 3.;
        let first_row = match (edit, compact) {
            (true, true) => 57.,
            (true, false) => 64.,
            (false, true) => 45.,
            (false, false) => 53.,
        };
        let tab_h = if compact { 24. } else { 29. };
        let tab_gap = if compact { 3. } else { 6. };
        let colors = [
            C::from_rgb(74, 222, 128),
            LAVENDER,
            MUTED,
            C::from_rgb(251, 146, 60),
            C::from_rgb(192, 132, 252),
        ];
        let art = self.art.as_ref().unwrap();
        if edit {
            let orig = loader_label(&self.original.loader, &self.original.loader_version);
            let next = loader_label(&self.draft.loader, &self.draft.loader_version);
            diff_chip(
                &p,
                panel.min + vec2(57., 45.),
                false,
                panel.width() - 66.,
                &orig,
                Some(&next),
            );
            changed_outline(&p, panel, 14., orig != next);
        }
        for (i, loader) in LOADERS.into_iter().enumerate() {
            let (col, row) = ((i % 3) as f32, (i / 3) as f32);
            let cell = rect(
                panel.min,
                15. + col * (size + 6.),
                first_row + row * (tab_h + tab_gap),
                size,
                tab_h,
            );
            let active = self.draft.loader == loader;
            let versions = set.and_then(|s| i.checked_sub(1).map(|i| &s[i]));
            let installed = self.mode == Mode::Edit
                && self.original.version == mc
                && self.original.loader == loader;
            let enabled =
                !versions.is_some_and(|r| r.as_ref().is_ok_and(Vec::is_empty)) || installed;
            let hint = match versions {
                Some(Ok(v)) if !v.is_empty() => format!("{} versiones", v.len()),
                Some(Ok(_)) if !installed => format!("{loader} no disponible para Minecraft {mc}"),
                Some(Err(e)) => e.clone(),
                _ => loader.into(),
            };
            let click = interact(ui, cell, ("loader", i)).on_hover_text(hint);
            let tint = if enabled || active {
                colors[i]
            } else {
                colors[i].gamma_multiply(0.34)
            };
            let ink = if active {
                WHITE
            } else if enabled {
                MUTED
            } else {
                DIM.gamma_multiply(0.5)
            };
            surface(
                &p,
                cell,
                8.,
                if active {
                    C::from_rgb(34, 22, 67)
                } else {
                    C::from_white_alpha(7)
                },
                if active {
                    C::from_rgb(117, 56, 255)
                } else if click.hovered() && enabled {
                    LINE
                } else {
                    NONE
                },
            );
            let font = if size > 66. { 12. } else { 10.5 };
            let use_icon = size >= 58. && text_width(&p, loader, font, true) + 18. < size;
            if use_icon {
                let x = cell.center().x - (text_width(&p, loader, font, true) + 15.) / 2.;
                art.control(&p, i, pos2(x + 6., cell.center().y), 13., tint);
                label(
                    &p.with_clip_rect(cell),
                    pos2(x + 19., cell.center().y),
                    Align2::LEFT_CENTER,
                    loader,
                    font,
                    true,
                    ink,
                );
            } else {
                label(
                    &p.with_clip_rect(cell),
                    cell.center(),
                    Align2::CENTER_CENTER,
                    loader,
                    font,
                    true,
                    ink,
                );
            }
            let clicked = enabled && click.clicked();
            if clicked && !self.busy && !active {
                self.draft.loader = loader.into();
                self.draft.loader_version = if installed {
                    self.original.loader_version.clone()
                } else {
                    versions
                        .and_then(|v| v.as_ref().ok())
                        .and_then(|v| v.iter().find(|r| r.recommended).or_else(|| v.first()))
                        .map(|v| v.version.clone())
                        .unwrap_or_default()
                };
            }
        }

        let list_y = first_row + 2. * tab_h + tab_gap + if compact { 26. } else { 46. };
        let list = Rect::from_min_max(panel.min + vec2(23., list_y), panel.max - vec2(23., 9.));
        let loader = self.draft.loader.as_str();
        let header = if loader == "Vanilla" {
            format!("Minecraft {mc} sin loader")
        } else {
            format!("Versión de {loader} ({mc})")
        };
        let selected_version = if loader == "Vanilla" {
            "Sin mods"
        } else {
            &self.draft.loader_version
        };
        let badge_w = if selected_version.is_empty() {
            0.
        } else {
            text_width(&p, selected_version, 10., true) + 15.
        };
        let header_rect = rect(panel.min, 24., list_y - 30., panel.width() - 48., 23.);
        label(
            &p.with_clip_rect(header_rect),
            header_rect.left_center(),
            Align2::LEFT_CENTER,
            &ellipsize(&p, &header, 11.5, true, header_rect.width() - badge_w - 5.),
            11.5,
            true,
            MUTED,
        );
        if badge_w > 0. {
            status_badge(
                &p,
                header_rect.right_center(),
                selected_version,
                LAVENDER,
                false,
            );
        }
        if loader == "Vanilla" {
            label(
                &p.with_clip_rect(panel),
                list.center_top() + vec2(0., 17.),
                Align2::CENTER_CENTER,
                "Minecraft puro",
                12.,
                true,
                C::from_rgb(134, 239, 172),
            );
            return;
        }
        let versions = LOADERS[1..]
            .iter()
            .position(|s| *s == loader)
            .and_then(|i| set.map(|set| &set[i]));
        let releases = versions
            .and_then(|v| v.as_ref().ok())
            .map_or(&[][..], Vec::as_slice);
        let current = (self.mode == Mode::Edit
            && self.original.version == mc
            && self.original.loader == loader
            && !self.original.loader_version.is_empty())
        .then_some(self.original.loader_version.as_str());
        let mut picked = None;
        let mut retry = false;
        rows(ui, list, ("loader-list", &mc, loader), |ui| {
            if let Some(original) = current
                && !releases.iter().any(|v| v.version == original)
            {
                let r = ui.allocate_space(vec2(list.width() - 4., 32.)).1;
                if loader_row(
                    ui,
                    r,
                    "loader-current",
                    original,
                    true,
                    false,
                    self.draft.loader_version == original,
                )
                .clicked()
                {
                    picked = Some(original.to_owned());
                }
            }
            for release in releases {
                let r = ui.allocate_space(vec2(list.width() - 4., 32.)).1;
                if loader_row(
                    ui,
                    r,
                    ("loader-release", &release.version),
                    &release.version,
                    Some(release.version.as_str()) == current,
                    release.recommended,
                    self.draft.loader_version == release.version,
                )
                .on_hover_text(release.kind)
                .clicked()
                {
                    picked = Some(release.version.clone());
                }
            }
            if releases.is_empty() {
                match versions {
                    Some(Err(e)) => {
                        let row = ui.allocate_space(vec2(list.width() - 9., 27.)).1;
                        retry =
                            small_button(ui, row, "retry-loaders", "Reintentar consulta", false)
                                .on_hover_text(e)
                                .clicked();
                    }
                    Some(Ok(_)) if current.is_none() => {
                        ui.label(
                            egui::RichText::new("No hay versiones compatibles.")
                                .size(12.)
                                .color(MUTED),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("Cargando versiones…")
                                .size(12.)
                                .color(MUTED),
                        );
                    }
                    _ => {}
                }
            }
        });
        if !self.busy {
            if let Some(v) = picked {
                self.draft.loader_version = v;
            }
            if retry {
                self.loader_versions.remove(&mc);
                ui.ctx().request_repaint();
            }
        }
    }

    fn paint_java(&mut self, ui: &mut Ui, area: Rect) -> bool {
        let p = ui.painter().clone();
        let art = self.art.as_ref().unwrap();
        let compact = area.height() < 460.;
        let h = (220. + (area.height() - 502.) * 0.575).clamp(172., 375.);
        let java = rect(area.min, 0., 0., area.width(), h);
        let jvm = rect(
            area.min,
            0.,
            h + 12.,
            area.width(),
            (area.height() - h - 12.).max(0.),
        );
        surface(&p, java, 14., CARD, NONE);
        surface(&p, jvm, 14., CARD, NONE);
        art.symbol(&p, 11, java.min + vec2(30., 27.), 25., VIOLET);
        label(
            &p,
            java.min + vec2(54., 27.),
            Align2::LEFT_CENTER,
            "Java Runtime",
            16.,
            true,
            WHITE,
        );
        let rec = instances::recommended_java(&self.draft.version);
        let wide = java.width() >= 454.;
        let filter = rect(
            java.right_top(),
            if wide { -238. } else { -177. },
            14.,
            if wide { 146. } else { 87. },
            26.,
        );
        let x = 54. + text_width(&p, "Java Runtime", 16., true) + 10.;
        for title in [format!("Recomendado: Java {rec}"), format!("Java {rec}")] {
            let w = text_width(&p, &title, 11.5, true) + 19.;
            if java.left() + x + w > filter.left() + 2. {
                continue;
            }
            let badge = rect(java.min, x, 17., w, 22.);
            surface(&p, badge, 11., C::from_rgb(35, 27, 66), NONE);
            label(
                &p,
                badge.center(),
                Align2::CENTER_CENTER,
                &title,
                11.5,
                true,
                LAVENDER,
            );
            break;
        }
        let filter_click = interact(ui, filter, "java-filter")
            .on_hover_text(format!("Solo recomendado: Java {rec}"));
        let check = Rect::from_center_size(filter.left_center() + vec2(14., 0.), vec2(15., 15.));
        surface(
            &p,
            check,
            4.,
            if self.java_recommended {
                C::from_rgb(143, 84, 255)
            } else {
                C::from_white_alpha(15)
            },
            if self.java_recommended {
                VIOLET
            } else {
                C::from_white_alpha(if filter_click.hovered() { 110 } else { 71 })
            },
        );
        if self.java_recommended {
            let at = check.center();
            p.line(
                [at + vec2(-4., 0.), at + vec2(-1., 3.), at + vec2(4., -3.)].to_vec(),
                Stroke::new(1.4, WHITE),
            );
        }
        label(
            &p,
            filter.left_center() + vec2(28., 0.),
            Align2::LEFT_CENTER,
            if wide {
                "Solo Recomendado"
            } else {
                "Solo recom."
            },
            12.,
            true,
            C::from_rgb(203, 213, 225),
        );
        if filter_click.clicked() && !self.busy {
            self.java_recommended = !self.java_recommended;
        }
        let refresh = icon_button(
            ui,
            rect(java.right_top(), -80., 14., 28., 26.),
            "java-refresh",
            "Detectar instalaciones de Java",
            art,
            5,
        )
        .clicked()
            && !self.busy;
        let folder = icon_button(
            ui,
            rect(java.right_top(), -43., 14., 28., 26.),
            "java-file",
            "Examinar ejecutable de Java · Clic derecho para restablecer",
            art,
            6,
        );
        let browse = folder.clicked() && !self.busy;
        folder.context_menu(|ui| {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Usar Java automáticamente"))
                .clicked()
            {
                self.draft.java.clear();
                self.draft.java_runtime = None;
                self.java_pick_latest = false;
                ui.close();
            }
        });
        if refresh {
            self.refresh_java();
        }
        let edit = self.mode == Mode::Edit;
        self.paint_java_picker(ui, java, rec, compact);
        if jvm.height() < 72. {
            return browse;
        }
        let row_h = (jvm.height() * 0.25).clamp(48., 75.);
        let jvm_w = jvm.width();
        let min_at = jvm.min + vec2(19., if compact { 8. } else { 15. });
        let max_at = min_at + vec2(0., row_h);
        let min_cap = self.draft.max_ram.max(512);
        let system_ram = system_memory();
        let max_end = self
            .draft
            .max_ram
            .max(self.original.max_ram)
            .max(system_ram.unwrap_or(16384));
        let ram_changed = (self.draft.min_ram, self.draft.max_ram)
            != (self.original.min_ram, self.original.max_ram);
        let jvm_changed = self.draft.jvm != self.original.jvm;
        if edit {
            let pair = |draft: &Draft| {
                format!(
                    "{} / {} (Xms / Xmx)",
                    ram_label(draft.min_ram),
                    ram_label(draft.max_ram)
                )
            };
            diff_chip(
                &p,
                min_at + vec2(jvm_w - 111., 11.),
                true,
                (jvm_w - 293.).max(80.),
                &pair(&self.original),
                Some(&pair(&self.draft)),
            );
            changed_outline(&p, jvm, 14., ram_changed || jvm_changed);
        }
        let art = self.art.as_ref().unwrap();
        for at in [min_at, max_at] {
            art.control(&p, 7, at + vec2(8., 11.), 15., LAVENDER);
        }
        paint_slider(
            ui,
            min_at,
            jvm_w - 38.,
            "RAM mínima al iniciar  -Xms",
            "ram-min",
            &mut self.draft.min_ram,
            min_cap,
            self.busy,
            &format!("Hasta: {}", ram_label(min_cap)),
            "",
            row_h < 58.,
        );
        paint_slider(
            ui,
            max_at,
            jvm_w - 38.,
            "RAM máxima  -Xmx",
            "ram-max",
            &mut self.draft.max_ram,
            max_end,
            self.busy,
            &ram_label(max_end),
            &system_ram
                .map(|mb| format!("RAM del sistema: {}", ram_label(mb)))
                .unwrap_or_default(),
            row_h < 58.,
        );
        if self.draft.max_ram < self.draft.min_ram {
            self.draft.min_ram = self.draft.max_ram;
        }
        let args_y = max_at.y + row_h;
        if args_y + 23. >= jvm.bottom() {
            return browse;
        }
        label(
            &p,
            pos2(min_at.x, args_y + 8.),
            Align2::LEFT_CENTER,
            "›_   Argumentos JVM",
            12.5,
            true,
            WHITE,
        );
        if edit {
            let preview = |value: &str| {
                if value.is_empty() {
                    "ninguno".to_owned()
                } else {
                    value.replace(['\r', '\n'], " ")
                }
            };
            diff_chip(
                &p,
                pos2(jvm.right() - 26., args_y + 8.),
                true,
                (jvm_w * 0.52).max(80.),
                &preview(&self.original.jvm),
                Some(&preview(&self.draft.jvm)),
            );
        }
        let args = Rect::from_min_max(pos2(min_at.x, args_y + 24.), jvm.max - vec2(19., 15.));
        if args.height() > 16. {
            surface(
                &p,
                args,
                9.,
                FIELD,
                if edit && jvm_changed {
                    AMBER.gamma_multiply(0.75)
                } else {
                    LINE
                },
            );
            let widget = egui::TextEdit::multiline(&mut self.draft.jvm)
                .id_salt("jvm-args")
                .font(FontId::monospace(11.))
                .text_color(WHITE)
                .frame(egui::Frame::NONE)
                .hint_text("-XX:+UseG1GC -XX:+ParallelRefProcEnabled")
                .interactive(!self.busy)
                .desired_rows(2)
                .desired_width(args.width() - 20.);
            ui.scope_builder(egui::UiBuilder::new().max_rect(args), |ui| {
                ui.set_clip_rect(args.shrink(3.));
                ui.put(args.shrink(5.), widget);
            });
        }
        browse
    }
    fn paint_java_picker(&mut self, ui: &mut Ui, card: Rect, recommended: u32, compact: bool) {
        let p = ui.painter().with_clip_rect(card);
        let pad = if compact { 17. } else { 25. };
        let table = Rect::from_min_max(card.min + vec2(pad, 58.), card.max - vec2(pad + 3., 20.));
        let vendor = self.java_vendor;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.java_majors.entry(vendor) {
            entry.insert(None);
            self.background(move || {
                ResultMessage::Majors(vendor, instance_catalog::java_majors(vendor))
            });
        }
        let mut majors = self
            .java_majors
            .get(&vendor)
            .and_then(Option::as_ref)
            .and_then(|r| r.as_ref().ok())
            .filter(|list| !list.is_empty())
            .cloned()
            .unwrap_or_else(|| JAVA_VENDORS[vendor].defaults.to_vec());
        if let Some(rt) = &self.draft.java_runtime
            && rt.vendor == JAVA_VENDORS[vendor].id
            && !majors.contains(&rt.major)
        {
            majors.insert(0, rt.major);
        }
        let rec = if majors.contains(&recommended) {
            recommended
        } else {
            majors[0]
        };
        let major = if self.java_recommended {
            rec
        } else {
            self.java_major
                .filter(|m| majors.contains(m))
                .unwrap_or(rec)
        };
        self.java_major = Some(major);
        let key = (vendor, major);
        if !self.java_builds.contains_key(&key) {
            if self.java_builds.len() >= 12 {
                self.java_builds.retain(|_, value| value.is_none());
            }
            // Requests are only created when their column is visible. Cap in-flight work if switching rapidly.
            if self.java_builds.values().filter(|v| v.is_none()).count() < 3 {
                self.java_builds.insert(key, None);
                self.background(move || {
                    ResultMessage::Builds(key, instance_catalog::java_builds(vendor, major))
                });
            }
        }
        let builds = self.java_builds.get(&key).and_then(Option::as_ref);
        if self.java_pick_latest && !self.busy {
            self.draft.java.clear();
            self.draft.java_runtime = builds
                .and_then(|r| r.as_ref().ok())
                .and_then(|b| b.first())
                .map(|build| build.runtime.clone());
        }

        let vendor_w = (table.width() * 0.33).min(183.);
        let major_w = (table.width() * 0.215).min(118.);
        let edges = [0., vendor_w, vendor_w + major_w, table.width()];
        let pad = if compact { 9. } else { 25. };
        let mut columns = [table; 3];
        for (i, title) in ["DISTRIBUIDOR", "VERSIÓN", "VERSIONES DISPONIBLES"]
            .into_iter()
            .enumerate()
        {
            let x = table.left() + edges[i];
            let end = table.left() + edges[i + 1] - if i == 2 { 0. } else { pad };
            let title_rect = Rect::from_min_max(pos2(x, table.top()), pos2(end, table.top() + 23.));
            label(
                &p.with_clip_rect(title_rect),
                title_rect.left_center() + vec2(3., 0.),
                Align2::LEFT_CENTER,
                title,
                11.,
                true,
                MUTED,
            );
            columns[i] = Rect::from_min_max(
                title_rect.left_bottom() + vec2(0., 10.),
                pos2(end, table.bottom()),
            );
            if i == 2 && title_rect.width() > 233. {
                let count = match builds {
                    Some(Ok(list)) => format!(
                        "{} disponible{}",
                        list.len(),
                        if list.len() == 1 { "" } else { "s" }
                    ),
                    Some(Err(_)) => "Sin conexión".into(),
                    None => "Buscando…".into(),
                };
                label(
                    &p,
                    title_rect.right_center() - vec2(4., 0.),
                    Align2::RIGHT_CENTER,
                    &count,
                    9.5,
                    false,
                    DIM,
                );
            }
        }

        let icons = self.java_icons.get_or_insert_with(|| {
            [
                include_bytes!("../home/java-vendors/mojang.png").as_slice(),
                include_bytes!("../home/java-vendors/adoptium.png").as_slice(),
                include_bytes!("../home/java-vendors/zulu.png").as_slice(),
                include_bytes!("../home/java-vendors/semeru.png").as_slice(),
            ]
            .into_iter()
            .map(|png| {
                let rgba = image::load_from_memory(png).unwrap().to_rgba8();
                load_image(
                    ui.ctx(),
                    "java-vendor-icon",
                    &rgba,
                    egui::TextureOptions::LINEAR,
                )
            })
            .collect()
        });
        let mut pick_vendor = None;
        rows(ui, columns[0], "java-vendors", |ui| {
            for (i, entry) in JAVA_VENDORS.iter().enumerate() {
                let row = ui
                    .allocate_space(vec2(
                        columns[0].width() - 3.,
                        if compact { 34. } else { 50. },
                    ))
                    .1;
                let response = interact(ui, row, ("java-vendor", entry.id));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        !self.busy,
                        vendor == i,
                        entry.label,
                    )
                });
                let paint = ui.painter().with_clip_rect(ui.clip_rect().intersect(row));
                surface(
                    &paint,
                    row,
                    9.,
                    if i == vendor || response.hovered() {
                        C::from_rgb(32, 20, 73)
                    } else {
                        C::from_rgb(20, 19, 38)
                    },
                    if i == vendor { VIOLET } else { NONE },
                );
                let inset = if compact { 17. } else { 28. };
                let icon_size = if compact { 20. } else { 28. };
                let icon = Rect::from_center_size(
                    row.left_center() + vec2(inset, 0.),
                    vec2(icon_size, icon_size),
                );
                paint.image(icons[i].id(), icon, UV, WHITE);
                let title_at = pos2(
                    icon.right() + if compact { 7. } else { 13. },
                    row.center().y - 7.,
                );
                let name = ellipsize(
                    &paint,
                    entry.label,
                    13.5,
                    true,
                    row.right() - title_at.x - 8.,
                );
                label(
                    &paint,
                    title_at,
                    Align2::LEFT_CENTER,
                    &name,
                    13.5,
                    true,
                    WHITE,
                );
                label(
                    &paint,
                    title_at + vec2(0., 15.),
                    Align2::LEFT_CENTER,
                    entry.sub,
                    10.5,
                    false,
                    MUTED,
                );
                if response.on_hover_text(entry.label).clicked() && vendor != i {
                    pick_vendor = Some(i);
                }
            }
        });
        let mut pick_major = None;
        let anchor = (
            vendor,
            major,
            if self.java_recommended {
                0
            } else {
                majors.iter().position(|m| *m == major).unwrap_or(0)
            },
        );
        let reveal_major = self.java_major_anchor.replace(anchor) != Some(anchor);
        rows(
            ui,
            columns[1],
            ("java-majors", vendor, self.java_recommended),
            |ui| {
                for m in majors
                    .into_iter()
                    .filter(|m| !self.java_recommended || *m == major)
                {
                    let row = ui
                        .allocate_space(vec2(
                            columns[1].width() - 6.,
                            if compact { 28. } else { 35. },
                        ))
                        .1;
                    if m == major && reveal_major && !ui.clip_rect().contains_rect(row) {
                        ui.scroll_to_rect(row, Some(egui::Align::Center));
                    }
                    if small_button(ui, row, ("java-major", m), &format!("Java {m}"), m == major)
                        .clicked()
                        && m != major
                    {
                        pick_major = Some(m);
                    }
                }
            },
        );

        let original = self.original.java_runtime.as_ref();
        let runtime = self.draft.java_runtime.as_ref();
        let matching = |rt: &instance_catalog::JavaRuntime| {
            rt.vendor == JAVA_VENDORS[vendor].id && rt.major == major
        };
        let exact = |a: &instance_catalog::JavaRuntime, b: &instance_catalog::JavaRuntime| {
            a.vendor == b.vendor && a.major == b.major && a.name == b.name
        };
        let mut picked = None;
        let mut retry = false;
        let available = builds
            .and_then(|r| r.as_ref().ok())
            .map_or(&[][..], Vec::as_slice);
        rows(ui, columns[2], ("java-builds", key), |ui| {
            let width = columns[2].width();
            let height = if compact { 33. } else { 42. };
            let slot = |ui: &mut Ui| ui.allocate_space(vec2((width - 1.).max(1.), height)).1;
            let local_path = &self.draft.java;
            if !local_path.is_empty()
                && !available
                    .iter()
                    .any(|b| b.runtime.is_installed_at(local_path))
            {
                let r = slot(ui);
                java_build_row(ui, r, "java-manual", "Java local", local_path, true, true);
            }
            if let Some(rt) = original.filter(|rt| matching(rt))
                && !available.iter().any(|b| exact(&b.runtime, rt))
            {
                let r = slot(ui);
                let installed = self
                    .java
                    .as_ref()
                    .is_some_and(|list| list.iter().any(|(_, path)| rt.is_installed_at(path)));
                if java_build_row(
                    ui,
                    r,
                    "java-build-current",
                    &format!("Java {}", rt.major),
                    &rt.name,
                    installed,
                    runtime.is_some_and(|r| exact(r, rt)),
                )
                .clicked()
                {
                    picked = Some(rt.clone());
                }
            }
            for b in available {
                let installed = self.java.as_ref().is_some_and(|list| {
                    list.iter().any(|(major, path)| {
                        *major == b.runtime.major && b.runtime.is_installed_at(path)
                    })
                });
                let active = runtime.is_some_and(|r| exact(r, &b.runtime))
                    || (!local_path.is_empty() && b.runtime.is_installed_at(local_path));
                let day = b.date.get(..10).unwrap_or(&b.date);
                let info = if day.is_empty() {
                    b.runtime.name.clone()
                } else {
                    format!("{} · {day}", b.runtime.name)
                };
                let r = slot(ui);
                if java_build_row(
                    ui,
                    r,
                    ("java-build", &b.runtime.name),
                    &b.version,
                    &info,
                    installed,
                    active,
                )
                .on_hover_text(format!("{} · {} · {}", b.version, info, b.package))
                .clicked()
                    && !active
                {
                    picked = Some(b.runtime.clone());
                }
            }
            match builds {
                None => {
                    ui.label(
                        egui::RichText::new("Buscando builds…")
                            .size(12.)
                            .color(MUTED),
                    );
                }
                Some(Ok(list)) if list.is_empty() => {
                    ui.label(
                        egui::RichText::new(format!(
                            "No hay versiones de Java {major} para Windows x64"
                        ))
                        .size(12.)
                        .color(MUTED),
                    );
                }
                Some(Err(e)) => {
                    let row = ui.allocate_space(vec2(columns[2].width() - 8., 27.)).1;
                    retry = small_button(
                        ui,
                        row,
                        "java-build-retry",
                        "Sin conexión · Reintentar",
                        false,
                    )
                    .on_hover_text(e)
                    .clicked();
                }
                _ => {}
            }
        });
        if !self.busy {
            if let Some(i) = pick_vendor {
                self.java_vendor = i;
                self.java_major = None;
                self.java_pick_latest = true;
                self.draft.java.clear();
                self.draft.java_runtime = None;
                ui.ctx().request_repaint();
            }
            if let Some(m) = pick_major {
                self.java_major = Some(m);
                self.java_pick_latest = true;
                self.draft.java.clear();
                self.draft.java_runtime = None;
                ui.ctx().request_repaint();
            }
            if let Some(build) = picked {
                self.draft.java.clear();
                self.draft.java_runtime = Some(build);
                self.java_pick_latest = false;
            }
            if retry {
                self.java_builds.remove(&key);
                if self
                    .java_majors
                    .get(&vendor)
                    .is_some_and(|m| m.as_ref().is_some_and(Result::is_err))
                {
                    self.java_majors.remove(&vendor);
                }
                ui.ctx().request_repaint();
            }
        }
    }
}

fn rect(origin: Pos2, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(origin + vec2(x, y), vec2(w.max(0.), h.max(0.)))
}

fn rows(ui: &mut Ui, area: Rect, id: impl std::hash::Hash, show: impl FnOnce(&mut Ui)) {
    if area.height() < 12. || area.width() < 20. {
        return;
    }
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.set_clip_rect(area);
        ui.spacing_mut().item_spacing.y = 3.;
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(area.height())
            .auto_shrink([false, true])
            .show(ui, show);
    });
}

fn paint_pack_description(
    ui: &mut Ui,
    body: &[DescriptionBlock],
    textures: &HashMap<String, TextureHandle>,
) {
    let width = ui.available_width().max(1.);
    for block in body {
        match block {
            DescriptionBlock::Image {
                url,
                alt,
                link,
                heading,
            } => {
                if let Some(texture) = textures.get(url) {
                    let native = texture.size_vec2();
                    let scale = (width / native.x).min(1.);
                    let scale = if *heading > 0 && native.x > native.y * 3. {
                        scale.min(34. / native.y)
                    } else {
                        scale
                    };
                    let size = native * scale;
                    let (r, response) = ui.allocate_exact_size(
                        size,
                        if link.is_some() {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    ui.painter().image(texture.id(), r, UV, WHITE);
                    if *heading > 0 {
                        ui.add_space(2.);
                    }
                    if response.clicked()
                        && let Some(link) = link
                    {
                        open_pack_url(link);
                    }
                } else if !alt.is_empty() {
                    ui.label(egui::RichText::new(alt).color(MUTED).size(12.));
                }
            }
            DescriptionBlock::Line { heading, fragments } => {
                let size = match heading {
                    1 => 16.,
                    2 => 14.5,
                    3 => 13.5,
                    6 => 11.5,
                    _ => 12.5,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0., 2.);
                    for part in fragments {
                        let mut text = part.text.as_str();
                        if let Some(rest) = text.strip_prefix('✔') {
                            let (mark, _) =
                                ui.allocate_exact_size(vec2(19., size + 3.), Sense::hover());
                            let c = mark.center();
                            ui.painter().add(egui::Shape::line(
                                vec![c + vec2(-6., 0.), c + vec2(-2., 3.5), c + vec2(4., -3.5)],
                                Stroke::new(2.2, C::from_rgb(139, 102, 218)),
                            ));
                            text = rest.trim_start_matches('\u{fe0f}').trim_start();
                        }
                        if text.is_empty() {
                            continue;
                        }
                        let mut rich = egui::RichText::new(text).size(size).color(WHITE);
                        if part.bold || *heading > 0 {
                            rich = rich.family(FontFamily::Name("ika-semibold".into()));
                        }
                        if let Some(link) = &part.link {
                            if ui
                                .add(
                                    egui::Label::new(rich.color(LAVENDER).underline())
                                        .sense(Sense::click()),
                                )
                                .on_hover_text(link)
                                .clicked()
                            {
                                open_pack_url(link);
                            }
                        } else {
                            ui.label(rich);
                        }
                    }
                });
            }
        }
    }
}

fn open_pack_url(url: &str) {
    if let Some(url) = modpacks::description_url(url, false) {
        let _ = crate::dock::open_target(&url);
    }
}

fn chevron(p: &Painter, center: Pos2, open: bool) {
    let up = if open { -1. } else { 1. };
    p.add(egui::Shape::line(
        vec![
            center + vec2(-3., -2. * up),
            center + vec2(0., up),
            center + vec2(3., -2. * up),
        ],
        Stroke::new(1.2, LAVENDER),
    ));
}

fn center_notice(ui: &Ui, area: Rect, text: &str) {
    label(
        &ui.painter().with_clip_rect(area),
        area.center(),
        Align2::CENTER_CENTER,
        text,
        12.,
        false,
        MUTED,
    );
}

fn list_row(
    ui: &mut Ui,
    width: f32,
    height: f32,
    id: impl std::hash::Hash,
    title: &str,
    info: &str,
    active: bool,
) -> Response {
    let r = ui.allocate_space(vec2((width - 9.).max(12.), height)).1;
    list_row_at(ui, r, id, title, info, active)
}

fn java_build_row(
    ui: &Ui,
    r: Rect,
    id: impl std::hash::Hash,
    version: &str,
    info: &str,
    installed: bool,
    active: bool,
) -> Response {
    let click = interact(ui, r, id);
    let p = ui.painter().with_clip_rect(ui.clip_rect().intersect(r));
    surface(
        &p,
        r,
        8.,
        if active {
            C::from_rgb(32, 20, 73)
        } else {
            C::from_rgb(20, 19, 38)
        },
        if active {
            C::from_rgb(129, 58, 255)
        } else if click.hovered() {
            LINE
        } else {
            NONE
        },
    );
    let at = r.left_center() + vec2(12., 0.);
    let room = r.width() - 22.;
    let shown = ellipsize(&p, version, 13.5, true, room.min(90.));
    label(&p, at, Align2::LEFT_CENTER, &shown, 13.5, true, WHITE);
    let meta = at + vec2(text_width(&p, &shown, 13.5, true) + 10., 0.);
    let check = r.right_center() - vec2(17., 0.);
    if active {
        p.line(
            [
                check + vec2(-4., 0.),
                check + vec2(-1., 3.),
                check + vec2(4., -3.),
            ]
            .to_vec(),
            Stroke::new(1.45, WHITE),
        );
    }
    let tag = if installed { "Instalado" } else { "Descargar" };
    let w = text_width(&p, tag, 10.5, true) + 18.;
    let end = if active {
        check.x - 11.
    } else {
        r.right() - 10.
    };
    let tint = if installed {
        C::from_rgb(52, 211, 153)
    } else {
        C::from_rgb(196, 181, 253)
    };
    let tag_rect = rect(pos2(end, r.center().y), -w, -8.5, w, 17.);
    if tag_rect.left() > meta.x - 4. {
        surface(
            &p,
            tag_rect,
            8.,
            tint.gamma_multiply(0.12),
            tint.gamma_multiply(0.31),
        );
        label(
            &p,
            tag_rect.center(),
            Align2::CENTER_CENTER,
            tag,
            10.5,
            true,
            tint,
        );
        let rest = tag_rect.left() - meta.x - 9.;
        if rest > 12. {
            label(
                &p,
                meta,
                Align2::LEFT_CENTER,
                &ellipsize(&p, info, 11.5, true, rest),
                11.5,
                true,
                C::from_rgb(135, 130, 162),
            );
        }
    }
    click
}

fn loader_label(loader: &str, version: &str) -> String {
    if version.is_empty() || loader == "Vanilla" {
        loader.to_owned()
    } else {
        format!("{loader} {version}")
    }
}

fn status_badge(p: &Painter, at: Pos2, text: &str, tint: C, left: bool) -> f32 {
    let w = text_width(p, text, 10., true) + 15.;
    let r = rect(at, if left { 0. } else { -w }, -8.5, w, 17.);
    surface(p, r, 8., tint.gamma_multiply(0.12), NONE);
    label(p, r.center(), Align2::CENTER_CENTER, text, 10., true, tint);
    w
}

fn loader_row(
    ui: &Ui,
    r: Rect,
    id: impl std::hash::Hash,
    version: &str,
    current: bool,
    recommended: bool,
    active: bool,
) -> Response {
    let response = interact(ui, r, id);
    let p = ui.painter().with_clip_rect(ui.clip_rect().intersect(r));
    surface(
        &p,
        r,
        7.,
        if active {
            C::from_rgb(32, 21, 72)
        } else {
            C::from_white_alpha(6)
        },
        if active {
            VIOLET
        } else if response.hovered() {
            LINE
        } else {
            NONE
        },
    );
    let mut end = r.right() - 10.;
    if active {
        let at = pos2(end - 5., r.center().y);
        p.line(
            [at + vec2(-4., 0.), at + vec2(-1., 3.), at + vec2(5., -4.)].to_vec(),
            Stroke::new(1.5, LAVENDER),
        );
        end -= 21.;
    }
    let start = r.left_center() + vec2(10., 0.);
    let reserve = if recommended { 83. } else { 0. } + if current { 53. } else { 0. };
    let shown = ellipsize(&p, version, 12., true, (end - start.x - reserve).max(28.));
    label(&p, start, Align2::LEFT_CENTER, &shown, 12., true, WHITE);
    let mut at = start + vec2(text_width(&p, &shown, 12., true) + 6., 0.);
    if recommended && at.x + 78. <= end {
        at.x += status_badge(&p, at, "Recomendado", C::from_rgb(74, 222, 128), true) + 4.;
    }
    if current && at.x + 48. <= end {
        status_badge(&p, at, "Actual", AMBER, true);
    }
    response
}

fn list_row_at(
    ui: &Ui,
    r: Rect,
    id: impl std::hash::Hash,
    title: &str,
    info: &str,
    active: bool,
) -> Response {
    let response = interact(ui, r, id);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            title,
        )
    });
    let p = ui.painter().with_clip_rect(ui.clip_rect().intersect(r));
    if active || response.hovered() {
        surface(
            &p,
            r,
            7.,
            C::from_rgb(27, 18, 63),
            if active { VIOLET } else { LINE },
        );
    } else {
        p.rect_filled(r, 7., C::from_white_alpha(8));
    }
    let text = p.with_clip_rect(Rect::from_min_max(
        r.min + vec2(7., 0.),
        r.max - vec2(if active { 20. } else { 7. }, 0.),
    ));
    let y = if info.is_empty() {
        r.center().y
    } else {
        r.top() + r.height() * 0.29
    };
    label(
        &text,
        pos2(r.left() + 8., y),
        Align2::LEFT_CENTER,
        title,
        12.,
        true,
        WHITE,
    );
    if !info.is_empty() {
        label(
            &text,
            pos2(r.left() + 8., r.top() + r.height() * 0.74),
            Align2::LEFT_CENTER,
            info,
            10.5,
            false,
            MUTED,
        );
    }
    if active {
        let center = r.right_center() - vec2(11., 0.);
        p.line_segment(
            [center + vec2(-4., 0.), center + vec2(-1., 3.)],
            Stroke::new(1.7, LAVENDER),
        );
        p.line_segment(
            [center + vec2(-1., 3.), center + vec2(4., -3.)],
            Stroke::new(1.7, LAVENDER),
        );
    }
    response
}
fn label(p: &Painter, at: Pos2, align: Align2, s: &str, size: f32, bold: bool, color: C) {
    p.text(
        at,
        align,
        s,
        FontId::new(
            size,
            if bold {
                FontFamily::Name("ika-semibold".into())
            } else {
                FontFamily::Proportional
            },
        ),
        color,
    );
}

fn paint_version_lens(p: &Painter, drum: Rect, lens: Rect) {
    let edge = C::from_rgb(12, 12, 29);
    let center = C::from_rgb(12, 10, 30);
    let upper = Rect::from_min_max(drum.min, pos2(drum.right(), lens.top()));
    let lower = Rect::from_min_max(pos2(drum.left(), lens.bottom()), drum.max);
    gradient_rect(p, upper, [edge, edge, center, center]);
    gradient_rect(p, lower, [center, center, edge, edge]);
    p.rect_filled(
        Rect::from_x_y_ranges(drum.x_range(), lens.y_range()),
        0.,
        center,
    );
    p.add(
        egui::epaint::RectShape::filled(
            lens.expand(4.),
            12.,
            C::from_rgba_unmultiplied(168, 85, 247, 20),
        )
        .with_blur_width(17.),
    );
    p.rect_filled(lens, 9., C::from_rgb(20, 15, 37));
    windows_ui::ring(p, lens, 9., 1., |pos| {
        let center = 1. - ((pos.x - lens.center().x) / (lens.width() * 0.5)).abs();
        let edge = ((pos.x - lens.left()).min(lens.right() - pos.x) / 8.).clamp(0., 1.);
        C::from_rgba_unmultiplied(168, 85, 247, ((72. + 31. * center) * edge) as u8)
    });
}

fn paint_version_glow(p: &Painter, at: Pos2, text: &str, size: f32, strength: f32) {
    let font = FontId::new(size, FontFamily::Name("ika-semibold".into()));
    let galley = p.layout_no_wrap(text.to_owned(), font, C::WHITE);
    let at = Align2::CENTER_CENTER.anchor_size(at, galley.size()).min;
    let halo = Rect::from_center_size(at + galley.size() * 0.5, vec2(galley.size().x + 10., 14.));
    p.add(
        egui::epaint::RectShape::filled(
            halo,
            7.,
            C::from_rgba_unmultiplied(158, 72, 248, (59. * strength) as u8),
        )
        .with_blur_width(24.),
    );
    for n in 0..8 {
        let angle = n as f32 * std::f32::consts::TAU / 8.;
        let offset = vec2(angle.cos(), angle.sin());
        for (radius, alpha) in [(1.6, 36.), (3.3, 14.)] {
            let color = C::from_rgba_unmultiplied(183, 103, 255, (alpha * strength) as u8);
            p.galley_with_override_text_color(at + offset * radius, galley.clone(), color);
        }
    }
}

fn gradient_rect(p: &Painter, r: Rect, colors: [C; 4]) {
    if !r.is_positive() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ]
    .into_iter()
    .zip(colors)
    {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

fn horizontal_gradient(p: &Painter, rect: Rect, left: C, right: C) {
    gradient_rect(p, rect, [left, right, right, left]);
}

fn text_width(p: &Painter, s: &str, size: f32, bold: bool) -> f32 {
    let family = if bold {
        FontFamily::Name("ika-semibold".into())
    } else {
        FontFamily::Proportional
    };
    p.layout_no_wrap(s.into(), FontId::new(size, family), WHITE)
        .size()
        .x
}

fn ellipsize(p: &Painter, text: &str, size: f32, bold: bool, width: f32) -> String {
    if text_width(p, text, size, bold) <= width {
        return text.into();
    }
    let mut ends = text.char_indices().map(|(i, _)| i).collect::<Vec<_>>();
    let i =
        ends.partition_point(|&i| text_width(p, &format!("{}…", &text[..i]), size, bold) <= width);
    let end = ends.get(i.saturating_sub(1)).copied().unwrap_or(0);
    ends.clear();
    format!("{}…", &text[..end])
}

fn compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.)
    } else {
        n.to_string()
    }
}

fn diff_chip(p: &Painter, at: Pos2, right: bool, width: f32, current: &str, new: Option<&str>) {
    let changed = new.is_some_and(|new| new != current);
    let text = if changed {
        format!("{current}  →  {}", new.unwrap())
    } else {
        format!("Actual · {current}")
    };
    let text = ellipsize(p, &text, 10.5, true, width - 16.);
    let w = text_width(p, &text, 10.5, true) + 16.;
    let r = rect(at, if right { -w } else { 0. }, -9.5, w, 19.);
    surface(
        p,
        r,
        9.,
        if changed {
            AMBER
        } else {
            C::from_rgb(25, 27, 45)
        },
        if changed {
            AMBER
        } else {
            C::from_rgb(47, 51, 71)
        },
    );
    label(
        p,
        r.center(),
        Align2::CENTER_CENTER,
        &text,
        10.5,
        true,
        if changed {
            C::from_rgb(28, 20, 5)
        } else {
            MUTED
        },
    );
}

fn changed_outline(p: &Painter, r: Rect, radius: f32, active: bool) {
    if active {
        p.rect_stroke(
            r,
            radius,
            Stroke::new(1., AMBER.gamma_multiply(0.75)),
            StrokeKind::Inside,
        );
    }
}

fn magnifier(p: &Painter, center: Pos2) {
    let at = center - vec2(1., 1.);
    let stroke = Stroke::new(1.1, DIM);
    p.circle_stroke(at, 4.3, stroke);
    p.line_segment([at + vec2(3., 3.), at + vec2(6., 6.)], stroke);
}

fn form_submit(ui: &Ui, r: Rect, text: &str, edit: bool, enabled: bool) -> Response {
    let response = ui
        .interact(
            r,
            ui.id().with("save"),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        )
        .on_hover_cursor(if enabled {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::NotAllowed
        });
    let fill = match (edit, enabled, response.hovered()) {
        (_, false, _) => C::from_rgb(38, 26, 75),
        (true, true, true) => C::from_rgb(88, 32, 255),
        (true, true, false) => C::from_rgb(75, 21, 242),
        (false, true, true) => C::from_rgb(117, 56, 255),
        (false, true, false) => C::from_rgb(101, 37, 255),
    };
    ui.painter().rect_filled(r, 14., fill);
    label(
        ui.painter(),
        r.center(),
        Align2::CENTER_CENTER,
        text,
        13.5,
        true,
        if enabled { WHITE } else { MUTED },
    );
    response
}

fn surface(p: &Painter, r: Rect, radius: f32, fill: C, line: C) {
    if r.is_positive() {
        p.rect(r, radius, fill, Stroke::new(1., line), StrokeKind::Inside);
    }
}
fn interact(ui: &Ui, r: Rect, id: impl std::hash::Hash) -> Response {
    ui.interact(r, ui.id().with(id), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn input(
    ui: &mut Ui,
    r: Rect,
    id: &'static str,
    text: &mut String,
    hint: &str,
    size: f32,
    limit: usize,
    frame: bool,
    enabled: bool,
) -> Response {
    if frame {
        surface(ui.painter(), r, 10., FIELD, LINE);
    }
    ui.put(
        r.shrink2(vec2(if frame { 13. } else { 0. }, 3.)),
        egui::TextEdit::singleline(text)
            .id_salt(id)
            .font(FontId::proportional(size))
            .hint_text(hint)
            .text_color(WHITE)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .char_limit(limit)
            .interactive(enabled),
    )
}
fn close_button(ui: &Ui, r: Rect) -> bool {
    let a = interact(ui, r, "close").on_hover_text("Cerrar ventana");
    let p = ui.painter();
    if a.hovered() {
        p.rect_filled(r.shrink(4.), 7., C::from_rgba_unmultiplied(225, 37, 90, 82));
    }
    for s in [-1., 1.] {
        let d = vec2(5.5, 5.5 * s);
        p.line_segment(
            [r.center() - d, r.center() + d],
            Stroke::new(1.6, if a.hovered() { WHITE } else { MUTED }),
        );
    }
    a.clicked()
}
fn button(ui: &Ui, r: Rect, id: &str, title: &str, accent: bool, enabled: bool) -> Response {
    let res = if enabled {
        interact(ui, r, id)
    } else {
        ui.interact(r, ui.id().with(id), Sense::hover())
    };
    let color = if !enabled {
        DIM
    } else if accent || res.hovered() {
        WHITE
    } else {
        MUTED
    };
    let p = ui.painter();
    if accent {
        let fill = if !enabled {
            C::from_rgb(30, 18, 66)
        } else if res.hovered() {
            C::from_rgb(103, 35, 255)
        } else {
            C::from_rgb(73, 18, 237)
        };
        p.rect_filled(r, 14., fill);
    } else if res.hovered() && enabled {
        p.rect_filled(r, 10., C::from_white_alpha(10));
    }
    label(
        p,
        r.center(),
        Align2::CENTER_CENTER,
        title,
        13.5,
        true,
        color,
    );
    res
}
fn small_button(ui: &Ui, r: Rect, id: impl std::hash::Hash, title: &str, active: bool) -> Response {
    let response = interact(ui, r, id);
    let p = ui.painter().with_clip_rect(r);
    if active || response.hovered() {
        surface(
            &p,
            r,
            8.,
            C::from_rgb(27, 18, 63),
            if active { VIOLET } else { LINE },
        );
    } else {
        p.rect_filled(r, 8., C::from_white_alpha(7));
    }
    label(
        &p,
        r.center(),
        Align2::CENTER_CENTER,
        title,
        12.,
        true,
        if active || response.hovered() {
            WHITE
        } else {
            MUTED
        },
    );
    response
}
fn tag(p: &Painter, xy: Pos2, s: &str, size: f32) {
    let galley = p.layout_no_wrap(
        s.into(),
        FontId::new(size, FontFamily::Name("ika-semibold".into())),
        LAVENDER,
    );
    let r = Rect::from_min_size(xy, galley.size() + vec2(19., 7.));
    p.rect_filled(r, 11., C::from_rgb(34, 25, 67));
    p.galley(r.min + vec2(9., 3.), galley, LAVENDER);
}
fn neon_pill(p: &Painter, r: Rect, tint: C, text: &str, glyph: Option<(&Art, usize)>, size: f32) {
    let cyan = tint == CYAN;
    let (base, bright) = if cyan {
        (C::from_rgb(5, 31, 46), C::from_rgb(18, 211, 218))
    } else {
        (C::from_rgb(23, 17, 56), C::from_rgb(170, 101, 248))
    };
    surface(p, r, 8., base, bright.gamma_multiply(0.58));
    windows_ui::ring(p, r.shrink(1.), 7., 1.2, |xy| {
        let u =
            ((xy.x - r.left()) / r.width() + if xy.y < r.center().y { 0.31 } else { 0. }).fract();
        bright.gamma_multiply(0.28 + 0.72 * (1. - (u * 2. - 1.).abs()))
    });
    if let Some((art, i)) = glyph {
        art.symbol(p, i, r.left_center() + vec2(17., 0.), 16., tint);
    }
    label(
        p,
        r.center() + vec2(if glyph.is_some() { 8. } else { 0. }, 0.),
        Align2::CENTER_CENTER,
        text,
        size,
        true,
        tint,
    );
}
fn image_fill(p: &Painter, tex: &TextureHandle, r: Rect, radius: f32) {
    image_crop(p, tex, r, [radius; 4], 1.);
}

fn instance_icon(
    p: &Painter,
    icon: Rect,
    texture: Option<&TextureHandle>,
    art: &Art,
    elapsed: Duration,
    hover: f32,
    reduced_motion: bool,
) {
    let lift = if reduced_motion { 0. } else { hover };
    let icon = icon.expand(1.25 * lift).translate(vec2(0., -2. * lift));
    if hover > 0. {
        let color = C::from_rgba_unmultiplied(168, 85, 247, (53. * hover).round() as u8);
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: 22, spread: 0, color }.as_shape(icon, 10.));
    }
    p.image(art.icon_plate.id(), icon.expand(0.32 * icon.width()), UV, C::WHITE);
    // Rotate the same gradient as the collection SVG around the icon center.
    let rotation = (-139. + elapsed.as_secs_f64().rem_euclid(10.) * 36.) as f32;
    let axis = egui::Vec2::angled(rotation.to_radians());
    windows_ui::window_frame(p, icon, 10., C::TRANSPARENT, 1.8, |point| {
        const RAMP: &[u8; 256 * 3] = include_bytes!("instance-icon-stroke.rgb");
        let t = (0.5 + (point - icon.center()).dot(axis) / icon.width()).clamp(0., 1.) * 255.;
        let i = t as usize;
        let color = |channel| {
            let a = f32::from(RAMP[i * 3 + channel]);
            let b = f32::from(RAMP[(i + 1).min(255) * 3 + channel]);
            (a + (b - a) * t.fract()).round() as u8
        };
        C::from_rgb(color(0), color(1), color(2))
    });
    if let Some(texture) = texture {
        // WebView uses a 38px slot with object-fit: contain and pixelated sampling.
        let size = texture.size_vec2();
        let slot = 38. * (1. + 0.08 * lift);
        let image = Rect::from_center_size(icon.center(), size * (slot / size.x.max(size.y)));
        p.image(texture.id(), image, UV, C::WHITE);
    }
    if hover > 0. {
        p.rect_filled(icon.shrink(1.8), 8., C::from_black_alpha((115. * hover).round() as u8));
        art.control(p, 9, icon.center(), 18., C::WHITE.gamma_multiply(hover));
    }
}

fn image_crop(p: &Painter, tex: &TextureHandle, r: Rect, [tl, tr, br, bl]: [f32; 4], zoom: f32) {
    let src = tex.size_vec2();
    let scale = (r.width() / src.x).max(r.height() / src.y) * zoom;
    let span = r.size() / (src * scale);
    let uv = Rect::from_center_size(pos2(0.5, 0.5), span);
    windows_ui::photo(
        p,
        tex.id(),
        r,
        uv,
        &windows_ui::rounded(r, tl, tr, br, bl),
        C::WHITE,
    );
}

fn gallery_image(
    p: &Painter,
    tex: &TextureHandle,
    stage: Rect,
    shift: f32,
    zoom: f32,
    opacity: f32,
) {
    if opacity <= 0. {
        return;
    }
    // The moving texture is sampled through the fixed rounded gallery window.
    let src = tex.size_vec2();
    let scale = (stage.width() / src.x).max(stage.height() / src.y) * zoom;
    let span = stage.size() / (src * scale);
    let visible = stage
        .translate(vec2(stage.width() * shift, 0.))
        .intersect(stage);
    let sample = stage.translate(vec2(stage.width() * shift, 0.));
    let uv = Rect::from_center_size(pos2(0.5, 0.5), span);
    windows_ui::photo(
        p,
        tex.id(),
        sample,
        uv,
        &windows_ui::rounded(
            visible,
            if shift <= 0. { 12. } else { 0. },
            if shift >= 0. { 12. } else { 0. },
            0.,
            0.,
        ),
        C::WHITE.gamma_multiply(opacity),
    );
}
fn icon_button(ui: &Ui, r: Rect, id: &str, info: &str, art: &Art, atlas: usize) -> Response {
    let click = interact(ui, r, id).on_hover_text(info);
    if click.hovered() {
        surface(ui.painter(), r, 7., FIELD, LINE);
    }
    art.control(
        ui.painter(),
        atlas,
        r.center(),
        16.,
        if click.hovered() { WHITE } else { MUTED },
    );
    click
}

fn system_memory() -> Option<u32> {
    use std::sync::OnceLock;
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    static MEMORY: OnceLock<Option<u32>> = OnceLock::new();
    *MEMORY.get_or_init(|| {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
        Some(((status.ullTotalPhys / (512 * 1024 * 1024)) as u32 * 512).max(2048))
    })
}

fn ram_label(mb: u32) -> String {
    let n = mb / 1024;
    match mb % 1024 {
        0 => format!("{n} GB"),
        512 => format!("{n}.5 GB"),
        _ => format!("{mb} MB"),
    }
}

fn paint_slider(
    ui: &Ui,
    at: Pos2,
    w: f32,
    name: &str,
    id: &str,
    value: &mut u32,
    max: u32,
    busy: bool,
    end_mark: &str,
    middle_mark: &str,
    compact: bool,
) {
    let p = ui.painter().clone();
    label(
        &p,
        at + vec2(23., 11.),
        Align2::LEFT_CENTER,
        name,
        12.5,
        true,
        WHITE,
    );
    let text = ram_label(*value);
    tag(
        &p,
        at + vec2(w - (text.chars().count() as f32 * 7. + 19.), 0.),
        &text,
        13.5,
    );
    let r = rect(at, 7., if compact { 19. } else { 22. }, w - 14., 17.);
    let h = ui
        .interact(r, ui.id().with(id), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if !busy
        && (h.clicked() || h.dragged())
        && let Some(pointer) = h.interact_pointer_pos()
    {
        let percent = ((pointer.x - r.left()) / r.width()).clamp(0., 1.);
        *value = ((512. + percent * max.saturating_sub(512) as f32) / 512.).round() as u32 * 512;
        *value = (*value).clamp(512, max);
    }
    let middle = r.center().y;
    let pct = value.saturating_sub(512) as f32 / max.saturating_sub(512).max(1) as f32;
    let knob = pos2(egui::lerp(r.x_range(), pct.clamp(0., 1.)), middle);
    p.hline(
        r.x_range(),
        middle,
        Stroke::new(6., C::from_rgb(35, 35, 54)),
    );
    p.line_segment(
        [r.left_center(), knob],
        Stroke::new(6., C::from_rgb(167, 76, 240)),
    );
    p.circle_filled(knob, 8.5, VIOLET.gamma_multiply(0.25));
    p.circle(
        knob,
        6.3,
        WHITE,
        Stroke::new(1.8, C::from_rgb(174, 86, 251)),
    );
    label(
        &p,
        pos2(at.x, r.bottom() + if compact { 5. } else { 9. }),
        Align2::LEFT_CENTER,
        "512 MB",
        10.,
        false,
        DIM,
    );
    label(
        &p,
        pos2(at.x + w, r.bottom() + if compact { 5. } else { 9. }),
        Align2::RIGHT_CENTER,
        end_mark,
        10.,
        false,
        DIM,
    );
    if !middle_mark.is_empty() {
        label(
            &p,
            pos2(r.center().x, r.bottom() + if compact { 5. } else { 9. }),
            Align2::CENTER_CENTER,
            middle_mark,
            10.,
            false,
            DIM,
        );
    }
}

#[derive(Clone, Copy)]
enum Pick {
    Cover,
    Icon,
    Java,
}
fn pick_file(window: &Window, pick: Pick) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        Win32::UI::Controls::Dialogs::*,
        core::{PWSTR, w},
    };
    let owner = crate::titlebar::window_handle(window).ok()?;
    let mut path = vec![0u16; 32768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        lpstrFilter: match pick {
            Pick::Java => w!("Java\0java.exe;javaw.exe\0\0"),
            _ => w!("Imágenes\0*.png;*.jpg;*.jpeg;*.webp;*.ico\0\0"),
        },
        lpstrTitle: match pick {
            Pick::Cover => w!("Cambiar portada"),
            Pick::Icon => w!("Cambiar icono"),
            Pick::Java => w!("Elegir ejecutable Java"),
        },
        Flags: OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_EXPLORER | OFN_FILEMUSTEXIST,
        ..Default::default()
    };
    let accepted = unsafe { GetOpenFileNameW(&mut dialog) };
    accepted.as_bool().then(|| {
        PathBuf::from(std::ffi::OsString::from_wide(
            &path[..path.iter().position(|&c| c == 0).unwrap_or(0)],
        ))
    })
}

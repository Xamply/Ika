use crate::{gpu::Params, minecraft, titlebar};
use image::{ImageReader, RgbaImage, imageops};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
    time::Instant,
};
use winit::{dpi::PhysicalPosition, event_loop::EventLoopProxy, window::Window};

#[path = "dock/wall_glyphs.rs"]
#[rustfmt::skip]
mod glyphs;

const GLIDE: f32 = 0.28;
const EASE: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
const CREATE_ICON_DURATION: f32 = 0.30;
const CREATE_TINT_DURATION: f32 = 0.20;
const CREATE_BORDER_PERIOD: f32 = 4.2;
const TOOLBAR_HOVER: f32 = 0.34;
const SEARCH_REVEAL: f32 = 0.38;
const SEARCH_CLICK: f32 = 0.5;
const CARD_HOVER: f32 = 0.42;
const SELECTED_HOVER: f32 = 0.55;
const ARROW_HOVER: f32 = 0.30;
const ARROW_PRESS: f32 = 0.55;
// Length of a filter icon's hover animation, in seconds.
const ICON_PLAY: f32 = 1.1;
// How long an interrupted icon animation takes to glide back to rest.
const ICON_RETURN: f32 = 0.45;
const CARD_EASE: [f32; 4] = [0.22, 1.0, 0.36, 1.0];
const WIDTHS: [f32; 5] = [224.0, 280.0, 364.0, 280.0, 224.0];
const HEIGHTS: [f32; 5] = [144.0, 172.0, 218.0, 172.0, 144.0];
const CENTERS: [f32; 5] = [-614.0, -342.0, 0.0, 342.0, 614.0];
const ATLAS_W: u32 = 1600;
const COVERS_H: u32 = 1536;
const ICON: u32 = 48;
const PER_ROW: u32 = ATLAS_W / ICON;
const BLOCKS: u32 = 24;
const GRADIENT_Y: u32 = 2304;
const CUSTOM_COVERS_Y: u32 = 2352;
const COVER_LIMIT: usize = 40;
// Icon rows must end before the gradient strip at GRADIENT_Y.
const MAX_ICONS: usize = 480;
const MIN_ZOOM: f32 = 0.65;
const MAX_ZOOM: f32 = 1.40;
const ZOOM_STEP: f32 = 0.05;
const ZOOM_DURATION: f32 = 0.12;
const CAROUSEL_TRAVEL_UP: i32 = 32;
const TOOLBAR_HALF_WIDTH: f32 = 368.0;
const TOOLBAR_HEIGHT: f32 = 44.0;
const TOOLBAR_HALF_HEIGHT: f32 = TOOLBAR_HEIGHT * 0.5;
const TOOLBAR_BLEED: f32 = 26.0;
const PILL_HEIGHT: f32 = 34.0;
// Group, create, all, recent, favorites, search. Shared with the shader.
const TOOLBAR_RECTS: [[f32; 4]; 6] = [
    [-TOOLBAR_HALF_WIDTH, -TOOLBAR_HALF_HEIGHT, 514.0, TOOLBAR_HEIGHT],
    [-360.0, -PILL_HEIGHT * 0.5, 164.0, PILL_HEIGHT],
    [-186.0, -PILL_HEIGHT * 0.5, 88.0, PILL_HEIGHT],
    [-92.0, -PILL_HEIGHT * 0.5, 112.0, PILL_HEIGHT],
    [26.0, -PILL_HEIGHT * 0.5, 112.0, PILL_HEIGHT],
    [162.0, -TOOLBAR_HALF_HEIGHT, 206.0, TOOLBAR_HEIGHT],
];

fn filter_rect(filter: u8) -> [f32; 2] {
    let [left, _, width, _] = TOOLBAR_RECTS[2 + usize::from(filter.min(2))];
    [left, width]
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Prev,
    Next,
    Card(i32),
    Filter(u8),
    Create,
    Search,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Text {
    words: [[u32; 4]; 4],
    starts: [[u32; 4]; 4],
    metrics: [u32; 4],
}

impl Text {
    fn write(&mut self, text: &str, limit: f32) {
        fn code(c: char) -> u8 {
            if (' '..='ÿ').contains(&c) {
                c as u8
            } else {
                b'?'
            }
        }
        let mut width = 0u32;
        let mut count = 0usize;
        for c in text.chars() {
            let c = code(c);
            let advance = glyphs::ADVANCE[usize::from(c - 32)] as u32;
            if count >= 63 || (width + advance) as f32 > limit {
                if count != 0 {
                    let last = count - 1;
                    let shift = (last % 4) * 8;
                    let word = &mut self.words[last / 16][(last / 4) % 4];
                    *word = (*word & !(255 << shift)) | (u32::from(b'.') << shift);
                }
                break;
            }
            let shift = (count % 4) * 8;
            self.words[count / 16][(count / 4) % 4] |= u32::from(c) << shift;
            self.starts[count / 16][(count / 4) % 4] |= width << shift;
            width += advance;
            count += 1;
            // Starts are packed into bytes. The last glyph may end past 255.
            if width > 255 {
                break;
            }
        }
        self.metrics = [width, count as u32, 0, 0];
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LayoutSettings {
    carousel_scale: f32,
    carousel_offset_y: i32,
    toolbar_scale: f32,
    toolbar_offset_y: i32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            carousel_scale: 1.0,
            carousel_offset_y: 0,
            toolbar_scale: 1.0,
            toolbar_offset_y: 0,
        }
    }
}

impl LayoutSettings {
    fn merge_changes(self, previous: Self, saved: &mut Self) {
        if self.carousel_scale != previous.carousel_scale {
            saved.carousel_scale = self.carousel_scale;
        }
        if self.carousel_offset_y != previous.carousel_offset_y {
            saved.carousel_offset_y = self.carousel_offset_y;
        }
        if self.toolbar_scale != previous.toolbar_scale {
            saved.toolbar_scale = self.toolbar_scale;
        }
        if self.toolbar_offset_y != previous.toolbar_offset_y {
            saved.toolbar_offset_y = self.toolbar_offset_y;
        }
    }
}

#[derive(Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    username: String,
    selected: String,
    #[serde(flatten)]
    layout: LayoutSettings,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("IkaNative/minecraft-home.json"))
    }

    fn read_at(path: &Path) -> Self {
        let mut settings: Self = std::fs::read(path).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if !minecraft::valid_username(&settings.username) {
            settings.username.clear();
        }
        if !(MIN_ZOOM..=MAX_ZOOM).contains(&settings.layout.carousel_scale) {
            settings.layout.carousel_scale = 1.0;
        }
        if !(MIN_ZOOM..=MAX_ZOOM).contains(&settings.layout.toolbar_scale) {
            settings.layout.toolbar_scale = 1.0;
        }
        settings
    }
}

struct SettingsStore {
    path: Option<PathBuf>,
    last_saved_layout: Cell<LayoutSettings>,
}

impl SettingsStore {
    fn open(path: Option<PathBuf>) -> (Self, Settings) {
        let settings = path.as_deref().map(Settings::read_at).unwrap_or_default();
        let store = Self { path, last_saved_layout: Cell::new(settings.layout) };
        (store, settings)
    }

    fn write(&self, current: &Settings, include_profile: bool) -> io::Result<()> {
        let path = self.path.as_deref()
            .ok_or_else(|| io::Error::other("No existe LOCALAPPDATA"))?;
        let mut saved = Settings::read_at(path);
        // Closing or saving the profile must not replace newer layout settings
        // written by another window. Retry unsaved layout changes on close.
        current.layout.merge_changes(self.last_saved_layout.get(), &mut saved.layout);
        if include_profile {
            saved.username.clone_from(&current.username);
            saved.selected.clone_from(&current.selected);
        }
        if saved != Settings::default() || path.exists() {
            let parent = path.parent().unwrap();
            std::fs::create_dir_all(parent)?;
            let mut draft = tempfile::NamedTempFile::new_in(parent)?;
            serde_json::to_writer(draft.as_file_mut(), &saved)?;
            draft.persist(path)?;
        }
        self.last_saved_layout.set(current.layout);
        Ok(())
    }
}

pub(crate) const TOOLBAR_LABELS: &[u8] = include_bytes!("home/toolbar_labels.png");

#[derive(Clone, Copy)]
pub struct CollectionArt {
    pub id: egui::TextureId,
    pub size: [u32; 2],
}

impl CollectionArt {
    pub fn uv(self, icon: u32) -> egui::Rect {
        let x = (icon % PER_ROW * ICON) as f32;
        let y = (COVERS_H + icon / PER_ROW * ICON) as f32;
        let [w, h] = self.size.map(|n| n as f32);
        // Sample inside the cell so zooming an icon never picks a neighbor.
        egui::Rect::from_min_max(
            egui::pos2((x + 0.5) / w, (y + 0.5) / h),
            egui::pos2((x + ICON as f32 - 0.5) / w, (y + ICON as f32 - 0.5) / h),
        )
    }
}

struct Card {
    instance: minecraft::Instance,
    icon: u32,
    custom_cover: u32,
    state: Game,
}

pub enum Game {
    Idle,
    Preparing(String),
    Loading(u32),
    Running(u32),
    Closed,
    Failed(String),
    Crashed(String),
}

#[allow(clippy::large_enum_variant)]
enum Update {
    Catalog(Vec<Card>, Option<RgbaImage>),
    Game(String, Game),
}

#[derive(Clone)]
struct Sender {
    tx: mpsc::Sender<Update>,
    wake: EventLoopProxy<()>,
}

impl Sender {
    fn send(&self, update: Update) {
        if self.tx.send(update).is_ok() {
            let _ = self.wake.send_event(());
        }
    }
}

fn read_icon(path: &Path) -> Option<RgbaImage> {
    icon_from(ImageReader::open(path).ok()?.with_guessed_format().ok()?)
}

fn read_icon_bytes(bytes: &[u8]) -> Option<RgbaImage> {
    icon_from(ImageReader::new(io::Cursor::new(bytes)).with_guessed_format().ok()?)
}

/// A custom icon picked in the WebView2 launcher. Its URLs are an asset file
/// (`http://asset.localhost/<percent-encoded path>`), a data URL, or a web
/// image (CurseForge or Modrinth), downloaded once into the native cache.
fn webview_icon(url: &str) -> Option<RgbaImage> {
    if let Some(encoded) = url
        .strip_prefix("http://asset.localhost/")
        .or_else(|| url.strip_prefix("https://asset.localhost/"))
    {
        return read_icon(Path::new(&percent_decode(encoded)?));
    }
    if url.starts_with("data:image/") {
        use base64::Engine;
        let (_, data) = url.split_once(";base64,")?;
        return read_icon_bytes(&base64::engine::general_purpose::STANDARD.decode(data).ok()?);
    }
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let cache = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("IkaNative/instance-icons");
    let name = format!("{:016x}", {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut hash);
        hash.finish()
    });
    let file = cache.join(name);
    if let Some(icon) = read_icon(&file) {
        return Some(icon);
    }
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .get(url)
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    io::Read::read_to_end(&mut io::Read::take(response.into_reader(), 4 * 1024 * 1024), &mut bytes).ok()?;
    let icon = read_icon_bytes(&bytes)?;
    if std::fs::create_dir_all(&cache).is_ok() {
        let _ = std::fs::write(&file, &bytes);
    }
    Some(icon)
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// The custom icon for an instance, matched like WebView2: by path, then by
/// folder name, then by display name.
fn custom_icon_url<'a>(icons: &'a BTreeMap<String, String>, instance: &minecraft::Instance) -> Option<&'a str> {
    let folder = Path::new(&instance.path).file_name().map(|f| f.to_string_lossy());
    icons
        .get(&instance.path)
        .or_else(|| folder.and_then(|f| icons.get(f.as_ref())))
        .or_else(|| icons.get(&instance.name))
        .map(String::as_str)
}

fn icon_from<R: io::BufRead + io::Seek>(mut reader: ImageReader<R>) -> Option<RgbaImage> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut icon = reader
        .decode()
        .ok()?
        .resize_to_fill(ICON, ICON, imageops::FilterType::Triangle)
        .to_rgba8();
    // The carousel and egui both blend these pixels as premultiplied sRGB.
    for pixel in icon.pixels_mut() {
        let a = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * a + 127) / 255) as u8;
        }
    }
    Some(icon)
}

/// The WebView2 launcher gives every instance without its own icon one of the
/// curated block faces (`instance-art/blocks.png`), without repeats until all
/// are used. This replays `getUniqueBlockFaceForInstance` exactly, in the same
/// call order (the instances sorted by name), so each instance keeps the block
/// it showed in 1.5.
fn webview_block_faces(keys: impl IntoIterator<Item = String>, count: usize) -> Vec<usize> {
    let mut available: Vec<usize> = Vec::new();
    let mut seen = std::collections::HashMap::<String, usize>::new();
    keys.into_iter()
        .map(|key| {
            let key = key.to_lowercase();
            if let Some(&face) = seen.get(&key) {
                return face;
            }
            if available.is_empty() {
                available = (0..count).collect();
                // JavaScript: hash = (hash * 31 + charCodeAt) >>> 0, over UTF-16 units.
                let hash = key
                    .encode_utf16()
                    .fold(0u32, |h, unit| h.wrapping_mul(31).wrapping_add(u32::from(unit)));
                for i in (1..available.len()).rev() {
                    let j = ((u64::from(hash) + i as u64) % (i as u64 + 1)) as usize;
                    available.swap(i, j);
                }
            }
            let face = available.remove(0) % count;
            seen.insert(key, face);
            face
        })
        .collect()
}

fn load_custom_cover(instance: &minecraft::Instance) -> Option<RgbaImage> {
    use image::imageops::FilterType;
    let path = minecraft::instances::cover_path(instance);
    if std::fs::metadata(&path).ok()?.len() > 12 * 1024 * 1024 { return None; }
    let mut reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    Some(reader.decode().ok()?.resize_to_fill(320, 192, FilterType::Triangle).to_rgba8())
}

/// The curated Minecraft block faces of the WebView2 launcher (32 px cells).
const BLOCK_FACES: &[u8] = include_bytes!("home/instance-art/blocks.png");
const BLOCK_FACE_NAMES: &str = include_str!("home/instance-art/blocks.txt");
const BLOCK_FACE_COLUMNS: u32 = 24;
const BLOCK_FACE_PX: u32 = 32;

fn block_face(atlas: &RgbaImage, face: usize) -> RgbaImage {
    let face = face as u32;
    let x = face % BLOCK_FACE_COLUMNS * BLOCK_FACE_PX;
    let y = face / BLOCK_FACE_COLUMNS * BLOCK_FACE_PX;
    let cell = imageops::crop_imm(atlas, x, y, BLOCK_FACE_PX, BLOCK_FACE_PX).to_image();
    // Pixel art: keep the texels sharp, like the WebView2 `is-block-face` image.
    imageops::resize(&cell, ICON, ICON, imageops::FilterType::Nearest)
}

fn make_catalog(
    instances: Vec<minecraft::Instance>,
    custom_icons: &BTreeMap<String, String>,
) -> (Vec<Card>, Option<RgbaImage>) {
    if instances.is_empty() {
        return (Vec::new(), None);
    }
    let covers = image::load_from_memory(include_bytes!("home/covers.jpg")).ok();
    let blocks = image::load_from_memory(include_bytes!("home/blocks.png")).ok();
    let gradient = image::load_from_memory(include_bytes!("home/gradient.png")).ok();
    let labels = image::load_from_memory(TOOLBAR_LABELS).ok();
    let faces = image::load_from_memory(BLOCK_FACES).ok().map(|i| i.to_rgba8());
    let face_count = BLOCK_FACE_NAMES.lines().count();
    // WebView2 assigns faces to every instance in its list order, also the
    // ones that later show their own icon, so replay the whole list.
    let assigned = webview_block_faces(
        instances.iter().map(|i| if i.path.is_empty() { i.name.clone() } else { i.path.clone() }),
        face_count,
    );
    let mut icons: Vec<RgbaImage> = Vec::new();
    let mut face_slots = std::collections::HashMap::<usize, u32>::new();
    let mut custom = Vec::new();
    let push_icon = |icons: &mut Vec<RgbaImage>, pixels: RgbaImage| {
        let id = BLOCKS + icons.len() as u32;
        icons.push(pixels);
        id
    };
    let cards = instances
        .into_iter()
        .enumerate()
        .map(|(i, instance)| {
            // Same precedence as WebView2: custom icon, the instance's own
            // icon file, then its unique block face.
            let own = (icons.len() < MAX_ICONS)
                .then(|| {
                    custom_icon_url(custom_icons, &instance)
                        .and_then(webview_icon)
                        .or_else(|| instance.icon_path.as_deref().and_then(|p| read_icon(Path::new(p))))
                })
                .flatten();
            let icon = if let Some(pixels) = own {
                push_icon(&mut icons, pixels)
            } else if let Some(atlas) = faces.as_ref().filter(|_| icons.len() < MAX_ICONS) {
                let face = assigned[i];
                match face_slots.get(&face) {
                    Some(&id) => id,
                    None => {
                        let id = push_icon(&mut icons, block_face(atlas, face));
                        face_slots.insert(face, id);
                        id
                    }
                }
            } else {
                i as u32 % BLOCKS
            };
            let custom_cover = (custom.len() < COVER_LIMIT)
                .then(|| load_custom_cover(&instance)).flatten()
                .map_or(0, |image| { custom.push(image); custom.len() as u32 });
            Card {
                instance,
                icon,
                custom_cover,
                state: Game::Idle,
            }
        })
        .collect();
    let atlas = covers
        .zip(blocks)
        .zip(gradient)
        .zip(labels)
        .map(|(((covers, blocks), gradient), labels)| {
            let art_height = (COVERS_H + (BLOCKS + icons.len() as u32).div_ceil(PER_ROW) * ICON)
                .max(GRADIENT_Y + 1);
            let art_height = if custom.is_empty() { art_height } else {
                debug_assert!(art_height <= CUSTOM_COVERS_Y);
                CUSTOM_COVERS_Y + (custom.len() as u32).div_ceil(5) * 192
            };
            let mut atlas = RgbaImage::new(ATLAS_W, art_height + labels.height());
            for (i, pixels) in custom.iter().enumerate() {
                imageops::replace(&mut atlas, pixels, (i % 5 * 320) as i64,
                    i64::from(CUSTOM_COVERS_Y) + (i / 5 * 192) as i64);
            }
            imageops::replace(&mut atlas, &covers.to_rgba8(), 0, 0);
            imageops::replace(&mut atlas, &blocks.to_rgba8(), 0, COVERS_H as i64);
            imageops::replace(&mut atlas, &gradient.to_rgba8(), 0, GRADIENT_Y as i64);
            imageops::replace(&mut atlas, &labels.to_rgba8(), 0, art_height as i64);
            for (i, pixels) in icons.iter().enumerate() {
                let id = i as u32 + BLOCKS;
                imageops::replace(
                    &mut atlas,
                    pixels,
                    (id % PER_ROW * ICON) as i64,
                    (COVERS_H + id / PER_ROW * ICON) as i64,
                );
            }
            atlas
        });
    (cards, atlas)
}

#[derive(Clone, Copy)]
struct HomeArea {
    width: f32,
    dpi: f32,
    top: f32,
    bottom: f32,
}

impl HomeArea {
    fn new(window: &Window) -> Self {
        let size = window.inner_size();
        Self::from_size(
            size.width as f32,
            size.height as f32,
            window.scale_factor() as f32,
        )
    }

    fn from_size(width: f32, height: f32, dpi: f32) -> Self {
        let dock_scale = dpi.min(width / 2160.0).min(height / 560.0);
        Self {
            width,
            dpi,
            top: 105.0 * dpi,
            bottom: height - 200.0 * dock_scale,
        }
    }
}

#[derive(Clone, Copy)]
struct Geometry {
    center: [f32; 2],
    scale: f32,
    five: bool,
    offset_y: i32,
    y_limits: [i32; 2],
    toolbar_anchor: f32,
}
impl Geometry {
    fn new(area: HomeArea, five: bool, zoom: f32, offset_y: i32) -> Self {
        let HomeArea { width: w, dpi, top, bottom } = area;
        let height = (bottom - top).max(1.0);
        let base_scale = dpi
            .min(w / if five { 1644.0 } else { 1136.0 })
            .min(height / 380.0);
        let baseline = bottom - 189.0 * base_scale;
        let scale = (base_scale * zoom).max(f32::EPSILON);
        let card_span = (HEIGHTS[2] * 0.5 + 4.0) * scale;
        let min = (((top + card_span - baseline) / dpi).ceil() as i32)
            .clamp(-CAROUSEL_TRAVEL_UP, 0);
        // Downward travel ends when the scaled cards reach the dock clearance.
        let max = (((bottom - card_span - baseline) / dpi).floor() as i32).max(0);
        let offset_y = offset_y.clamp(min, max);
        Self {
            center: [w * 0.5, baseline + offset_y as f32 * dpi],
            scale,
            five,
            offset_y,
            y_limits: [min, max],
            toolbar_anchor: baseline - 166.0 * base_scale,
        }
    }

    fn card(self, slot: usize, slide: f32) -> [f32; 4] {
        let (w, h) = (WIDTHS[slot], HEIGHTS[slot]);
        self.rect([CENTERS[slot] + slide - w * 0.5, -h * 0.5, w, h])
    }

    fn rect(self, [x, y, w, h]: [f32; 4]) -> [f32; 4] {
        [
            self.center[0] + x * self.scale,
            self.center[1] + y * self.scale,
            w * self.scale,
            h * self.scale,
        ]
    }

    fn arrow(self, sign: f32) -> [f32; 4] {
        let d = if self.five { 765.0 } else { 521.0 };
        self.rect([sign * d - 19.0, -19.0, 38.0, 38.0])
    }

}

#[derive(Clone, Copy)]
struct ToolbarGeometry {
    view: [f32; 4],
    offset_y: i32,
    y_limits: [i32; 2],
}

impl ToolbarGeometry {
    fn new(area: HomeArea, carousel: Geometry, has_cards: bool, zoom: f32, offset_y: i32) -> Self {
        let scale = (area.dpi * zoom)
            .min(area.width / (2.0 * (TOOLBAR_HALF_WIDTH + TOOLBAR_BLEED)))
            .max(f32::EPSILON);
        let half_height = TOOLBAR_HALF_HEIGHT * scale;
        let top = area.top + half_height + 12.0 * area.dpi;
        let card_half_height = if has_cards { HEIGHTS[2] * 0.5 } else { 28.0 };
        let bottom = carousel.center[1] - card_half_height * carousel.scale
            - half_height - 8.0 * area.dpi;
        let bottom = bottom.max(top);
        let anchor = carousel.toolbar_anchor.clamp(top, bottom);
        let min = ((top - anchor) / area.dpi).ceil() as i32;
        let max = ((bottom - anchor) / area.dpi).floor() as i32;
        let offset_y = offset_y.clamp(min, max);
        Self {
            view: [
                area.width * 0.5,
                anchor + offset_y as f32 * area.dpi,
                scale,
                TOOLBAR_BLEED,
            ],
            offset_y,
            y_limits: [min, max],
        }
    }

    fn local(self, p: PhysicalPosition<f64>) -> PhysicalPosition<f64> {
        let [x, y, scale, _] = self.view.map(f64::from);
        PhysicalPosition::new((p.x - x) / scale, (p.y - y) / scale)
    }

    fn contains(self, p: PhysicalPosition<f64>) -> bool {
        let p = self.local(p);
        p.x.abs() <= f64::from(TOOLBAR_HALF_WIDTH) && p.y.abs() <= f64::from(TOOLBAR_HALF_HEIGHT)
    }

    fn hit(self, p: PhysicalPosition<f64>) -> Option<Hit> {
        let p = self.local(p);
        TOOLBAR_RECTS[1..]
            .iter()
            .position(|&rect| contains(rect, p))
            .map(|idx| match idx {
                0 => Hit::Create,
                4 => Hit::Search,
                filter => Hit::Filter((filter - 1) as u8),
            })
    }
}

pub(crate) fn toolbar_bounds([x, y, scale, bleed]: [f32; 4]) -> [f32; 4] {
    let half_width = (TOOLBAR_HALF_WIDTH + bleed) * scale;
    let half_height = (TOOLBAR_HALF_HEIGHT + bleed) * scale;
    [x - half_width, y - half_height, x + half_width, y + half_height]
}

#[derive(Clone, Copy)]
enum LayoutTarget {
    Toolbar,
    Carousel,
}

fn contains([x, y, w, h]: [f32; 4], pos: PhysicalPosition<f64>) -> bool {
    let (px, py) = (pos.x as f32, pos.y as f32);
    px >= x && px < x + w && py >= y && py < y + h
}

pub struct Home {
    cards: Vec<Card>,
    selection: usize,
    settings: Settings,
    settings_store: SettingsStore,
    edit: Option<String>,
    edit_and_play: bool,
    notice: String,
    ready: bool,
    refreshing: bool,
    slide: Option<(Instant, i32)>,
    phase_started: Instant,
    create_icon: titlebar::Transition<1>,
    create_tint: titlebar::Transition<1>,
    zoom_target: f32,
    zoom_start: f32,
    zoom_started: Option<Instant>,
    toolbar_zoom: titlebar::Transition<1>,
    nudge_target: Option<LayoutTarget>,
    reduced_motion: bool,
    active_filter: u8,
    filter_glider: titlebar::Glider,
    /// Hover of Todas, Recientes, Favoritas and Buscar (may overshoot).
    toolbar_hover: titlebar::Transition<4>,
    /// Hover of each carousel slot, eased so the card lifts and settles.
    card_hover: titlebar::Transition<4>,
    /// Hover of the two carousel arrows and when each was last clicked.
    arrow_hover: titlebar::Transition<2>,
    arrow_pressed: [Option<Instant>; 2],
    /// Hover of the selected card, its own richer animation.
    selected_hover: titlebar::Transition<1>,
    selected_hover_since: Option<Instant>,
    /// When each filter icon started its hover animation (Todas, Recientes, Favoritas).
    /// Progress (0..1) of each filter icon's animation and whether it is
    /// gliding back to rest after the cursor left mid-way.
    icon_time: [f32; 4],
    /// Pose an interrupted icon froze at, and how much of its return is left.
    icon_return: [Option<(f32, f32)>; 4],
    /// When the search field was last clicked (magnifier zoom pulse).
    search_clicked: Option<Instant>,
    icon_last: Option<Instant>,
    /// Magnifier and text sliding between the placeholder and the typed text.
    search_reveal: titlebar::Transition<1>,
    /// Text typed in "Buscar instancia"; `Some` while the field has focus.
    search: String,
    search_focus: bool,
    /// Indices into `cards` that the carousel cycles through. All cards when
    /// the search is empty; `selection` always stays a real `cards` index.
    view: Vec<usize>,
    /// Icons picked per instance in WebView2, shared with the catalog thread.
    custom_icons: std::sync::Arc<BTreeMap<String, String>>,
    tx: Sender,
    rx: mpsc::Receiver<Update>,
}

impl Home {
    pub fn new(wake: EventLoopProxy<()>, reduced_motion: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let tx = Sender { tx, wake };
        let worker = tx.clone();
        std::thread::spawn(move || {
            let (cards, atlas) = make_catalog(minecraft::discover(), &BTreeMap::new());
            worker.send(Update::Catalog(cards, atlas));
        });
        let (settings_store, settings) = SettingsStore::open(Settings::path());
        let initial_zoom = settings.layout.carousel_scale;
        let now = Instant::now();
        let toolbar_zoom = titlebar::Transition::new([settings.layout.toolbar_scale], now);
        Self {
            cards: Vec::new(),
            selection: 0,
            settings,
            settings_store,
            edit: None,
            edit_and_play: false,
            notice: String::new(),
            ready: false,
            refreshing: true,
            slide: None,
            phase_started: now,
            create_icon: titlebar::Transition::new([0.0], now),
            create_tint: titlebar::Transition::new([0.0], now),
            zoom_target: initial_zoom,
            zoom_start: initial_zoom,
            zoom_started: None,
            toolbar_zoom,
            nudge_target: None,
            reduced_motion,
            active_filter: 0,
            filter_glider: titlebar::Glider::new(filter_rect(0), now),
            toolbar_hover: titlebar::Transition::new([0.0; 4], now),
            search_reveal: titlebar::Transition::new([0.0], now),
            card_hover: titlebar::Transition::new([0.0; 4], now),
            selected_hover: titlebar::Transition::new([0.0], now),
            arrow_hover: titlebar::Transition::new([0.0; 2], now),
            arrow_pressed: [None; 2],
            selected_hover_since: None,
            icon_time: [0.0; 4],
            icon_return: [None; 4],
            search_clicked: None,
            icon_last: None,
            search: String::new(),
            search_focus: false,
            view: Vec::new(),
            custom_icons: Default::default(),
            tx,
            rx,
        }
    }

    pub fn poll(&mut self, gpu: &mut crate::gpu::Gpu) -> bool {
        let mut changed = false;
        while let Ok(update) = self.rx.try_recv() {
            changed = true;
            match update {
                Update::Catalog(mut cards, atlas) => {
                    for card in &mut cards {
                        if let Some(old) = self.cards.iter_mut().find(|c| c.instance.path == card.instance.path) {
                            card.state = std::mem::replace(&mut old.state,Game::Idle);
                        }
                    }

                    self.selection = cards
                        .iter()
                        .position(|c| c.instance.path == self.settings.selected)
                        .unwrap_or(0);
                    self.cards = cards;
                    self.refilter();
                    self.ready = true;
                    self.refreshing = false;
                    if let Some(atlas) = atlas {
                        gpu.load_home_atlas(atlas);
                    }
                }
                Update::Game(path, state) => {
                    if let Some(card) = self.cards.iter_mut().find(|c| c.instance.path == path) {
                        card.state = state;
                    }
                }
            }
        }
        changed
    }

    pub fn running_game(&self) -> bool {
        self.cards.iter().any(|c| matches!(c.state, Game::Running(_)))
    }
    pub fn busy(&self) -> bool {
        self.cards.iter().any(|c| matches!(c.state, Game::Running(_) | Game::Loading(_) | Game::Preparing(_)))
    }
    pub fn refresh_catalog(&mut self) {
        self.refreshing = true;
        let worker = self.tx.clone();
        let icons = self.custom_icons.clone();
        std::thread::spawn(move || {
            let (cards, atlas) = make_catalog(minecraft::discover(), &icons);
            worker.send(Update::Catalog(cards, atlas));
        });
    }

    /// Applies the icons chosen in WebView2 once its profile has been read.
    pub fn set_webview_icons(&mut self, icons: &BTreeMap<String, String>) {
        if *self.custom_icons != *icons {
            self.custom_icons = std::sync::Arc::new(icons.clone());
            self.refresh_catalog();
        }
    }
    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn catalog_loading(&self) -> bool {
        self.refreshing
    }

    pub fn collection(&self) -> impl ExactSizeIterator<Item = &minecraft::Instance> {
        self.cards.iter().map(|card| &card.instance)
    }

    pub fn collection_card(&self, index: usize) -> Option<(&minecraft::Instance, u32)> {
        self.cards.get(index).map(|card| (&card.instance, card.icon))
    }

    pub fn select_after_refresh(&mut self, path: &Path) {
        self.settings.selected = path.to_string_lossy().into_owned();
        if let Err(error) = self.save() {
            eprintln!("Instancia seleccionada: {error}");
        }
        self.refresh_catalog();
    }

    pub fn active_instance(&self) -> Option<&minecraft::Instance> {
        self.cards.get(self.selection).map(|card| &card.instance)
    }

    pub fn console_instance(&self) -> Option<(&minecraft::Instance, &Game)> {
        self.cards.get(self.selection).map(|card| (&card.instance, &card.state))
    }

    pub fn selected(&self) -> usize {
        self.selection
    }

    pub fn select_instance(&mut self, index: usize, now: Instant) {
        if index < self.cards.len() && !self.view.contains(&index) {
            // Picked from Mi Colección while hidden by the search: show all again.
            self.search.clear();
            self.search_focus = false;
            self.refilter();
        }
        if index < self.cards.len()
            && index != self.selection
            && let (Some(to), Some(from)) = (
                self.view.iter().position(|&i| i == index),
                self.view_position(),
            )
        {
            self.activate(Hit::Card(to as i32 - from as i32), now);
            if let Err(error) = self.save() {
                eprintln!("Instancia seleccionada: {error}");
            }
        }
    }

    /// The card `offset` steps from the selection in the carousel (0 = selected).
    pub fn card_at(&self, offset: i32) -> Option<&minecraft::Instance> {
        (!self.view.is_empty()).then(|| &self.cards[self.index(offset)].instance)
    }

    /// The game process of an instance while it loads or runs.
    pub fn game_pid(&self, path: &str) -> Option<u32> {
        self.cards.iter().find(|c| c.instance.path == path).and_then(|c| match c.state {
            Game::Loading(pid) | Game::Running(pid) => Some(pid),
            _ => None,
        })
    }

    /// Selects the instance at `path` (if visible) and plays it.
    pub fn play_path(&mut self, path: &str, account: Option<(String, Option<minecraft::Skin>)>, now: Instant) {
        if let Some(index) = self.cards.iter().position(|c| c.instance.path == path) {
            self.select_instance(index, now);
        }
        self.play_as(account);
    }

    pub fn running_instance(&self) -> Option<&minecraft::Instance> {
        self.cards.iter().find(|c| matches!(c.state, Game::Running(_))).map(|c| &c.instance)
    }

    /// Card index `offset` steps from the selection inside the visible view.
    fn index(&self, offset: i32) -> usize {
        let len = self.view.len() as i32;
        let here = self.view_position().unwrap_or(0) as i32;
        self.view[(here + offset).rem_euclid(len) as usize]
    }

    fn view_position(&self) -> Option<usize> {
        self.view.iter().position(|&i| i == self.selection)
    }

    /// Rebuilds `view` from the search text, like the WebView2 home filter:
    /// name, folder, Minecraft version or loader containing the query.
    fn refilter(&mut self) {
        let query = self.search.trim().to_lowercase();
        self.view = self
            .cards
            .iter()
            .enumerate()
            .filter(|(_, card)| {
                let i = &card.instance;
                let folder = std::path::Path::new(&i.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                query.is_empty()
                    || i.name.to_lowercase().contains(&query)
                    || folder.contains(&query)
                    || i.mc_version.to_lowercase().contains(&query)
                    || i.loader.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        if self.view_position().is_none()
            && let Some(&first) = self.view.first()
        {
            self.selection = first;
        }
    }

    pub fn searching(&self) -> bool {
        self.search_focus
    }

    pub fn search_input(&mut self, text: &str) {
        if !self.search_focus {
            return;
        }
        let room = 40usize.saturating_sub(self.search.chars().count());
        self.search
            .extend(text.chars().filter(|c| !c.is_control()).take(room));
        self.refilter();
    }

    pub fn search_backspace(&mut self) {
        if self.search_focus && self.search.pop().is_some() {
            self.refilter();
        }
    }

    /// Esc clears the text first, then leaves the field.
    pub fn search_escape(&mut self) {
        if self.search.is_empty() {
            self.search_focus = false;
        } else {
            self.search.clear();
            self.refilter();
        }
    }

    pub fn search_blur(&mut self) {
        self.search_focus = false;
    }

    pub fn current_zoom(&self, now: Instant) -> f32 {
        let Some(start) = self.zoom_started else {
            return self.zoom_target;
        };
        let t = now.saturating_duration_since(start).as_secs_f32() / ZOOM_DURATION;
        if t >= 1.0 {
            return self.zoom_target;
        }
        let factor = titlebar::ease(t, EASE);
        self.zoom_start + (self.zoom_target - self.zoom_start) * factor
    }

    fn geometry(&self, window: &Window, now: Instant) -> Geometry {
        Geometry::new(
            HomeArea::new(window),
            self.view.len() >= 5,
            self.current_zoom(now),
            self.settings.layout.carousel_offset_y,
        )
    }

    fn toolbar_geometry(&self, window: &Window, carousel: Geometry, now: Instant) -> ToolbarGeometry {
        let [zoom] = self.toolbar_zoom.sample(now, ZOOM_DURATION, EASE);
        let mut toolbar = ToolbarGeometry::new(
            HomeArea::new(window),
            carousel,
            !self.view.is_empty(),
            zoom,
            self.settings.layout.toolbar_offset_y,
        );
        // Keep the bar's distance to the selected card while it rises and
        // grows on hover (same numbers as home_card in home.wgsl).
        let [star] = self.selected_hover.sample(now, SELECTED_HOVER, CARD_EASE);
        let rise = 9.0 * star + 0.045 * star * HEIGHTS[2] * 0.5;
        toolbar.view[1] -= rise * carousel.scale;
        toolbar
    }

    pub fn is_over_toolbar(&self, window: &Window, cursor: PhysicalPosition<f64>, now: Instant) -> bool {
        self.toolbar_geometry(window, self.geometry(window, now), now)
            .contains(cursor)
    }

    pub fn nudge_layout(
        &mut self,
        window: &Window,
        cursor: PhysicalPosition<f64>,
        direction: i32,
        now: Instant,
        repeat: bool,
    ) -> bool {
        let g = self.geometry(window, now);
        let toolbar = self.toolbar_geometry(window, g, now);
        // Keep the originally hovered row or carousel while the arrow is held.
        if !repeat || self.nudge_target.is_none() {
            self.nudge_target = if toolbar.contains(cursor) {
                Some(LayoutTarget::Toolbar)
            } else {
                let slide = self.shift(now).0;
                let over_card = (0..5)
                    .any(|slot| self.has_card(slot) && contains(g.card(slot, slide), cursor));
                let over_arrow = self.view.len() > 1
                    && [g.arrow(-1.0), g.arrow(1.0)].into_iter().any(|r| contains(r, cursor));
                (over_card || over_arrow).then_some(LayoutTarget::Carousel)
            };
        }
        let (offset, [min, max], saved) = match self.nudge_target {
            Some(LayoutTarget::Toolbar) => (
                toolbar.offset_y,
                toolbar.y_limits,
                &mut self.settings.layout.toolbar_offset_y,
            ),
            Some(LayoutTarget::Carousel) => (
                g.offset_y,
                g.y_limits,
                &mut self.settings.layout.carousel_offset_y,
            ),
            None => return false,
        };
        let next = (offset + direction.signum()).clamp(min, max);
        if next == offset {
            return false;
        }
        *saved = next;
        self.save_layout();
        true
    }

    pub fn is_over_carousel(&self, window: &Window, cursor: PhysicalPosition<f64>) -> bool {
        let size = window.inner_size();
        let w = size.width as f32;
        let h = size.height as f32;
        let dpi = window.scale_factor() as f32;
        let dock_scale = dpi.min(w / 2160.0).min(h / 560.0);
        let dock_top = h - 200.0 * dock_scale;
        let titlebar_bottom = 105.0 * dpi;

        let px = cursor.x as f32;
        let py = cursor.y as f32;

        if py <= titlebar_bottom || py >= dock_top || px < 0.0 || px > w {
            return false;
        }

        let g = self.geometry(window, Instant::now());
        let base_scale = dpi
            .min(w / if g.five { 1644.0 } else { 1136.0 })
            .min((h - titlebar_bottom - 200.0 * dock_scale).max(1.0) / 350.0);
        let eff_scale = base_scale.max(g.scale);
        let half_width = (if g.five { 850.0 } else { 600.0 }) * eff_scale + 80.0;
        let min_x = (g.center[0] - half_width).max(0.0);
        let max_x = (g.center[0] + half_width).min(w);

        px >= min_x && px <= max_x
    }

    pub fn zoom(
        &mut self,
        window: &Window,
        cursor: PhysicalPosition<f64>,
        amount: f32,
        now: Instant,
    ) -> bool {
        let toolbar = self.is_over_toolbar(window, cursor, now);
        if !toolbar && !self.is_over_carousel(window, cursor) {
            return false;
        }
        if amount.abs() < f32::EPSILON {
            return false;
        }
        let ticks = if amount.abs() >= 1.0 {
            amount.round() as i32
        } else {
            amount.signum() as i32
        };
        let step = ticks as f32 * ZOOM_STEP;
        let current_target = if toolbar { self.settings.layout.toolbar_scale } else { self.zoom_target };
        let next = ((current_target + step) * 100.0).round() / 100.0;
        let next = next.clamp(MIN_ZOOM, MAX_ZOOM);
        if (next - current_target).abs() < f32::EPSILON {
            return false;
        }
        if toolbar {
            self.settings.layout.toolbar_scale = next;
            if self.reduced_motion {
                self.toolbar_zoom = titlebar::Transition::new([next], now);
            } else {
                self.toolbar_zoom.set([next], now, ZOOM_DURATION, EASE);
            }
        } else {
            self.zoom_start = self.current_zoom(now);
            self.zoom_target = next;
            self.zoom_started = if self.reduced_motion { None } else { Some(now) };
            self.settings.layout.carousel_scale = next;
        }
        self.save_layout();
        true
    }

    pub fn reset_zoom(&mut self, now: Instant) -> bool {
        let next = 1.0;
        if (next - self.zoom_target).abs() < f32::EPSILON {
            return false;
        }
        let current = self.current_zoom(now);
        self.zoom_start = current;
        self.zoom_target = next;
        self.zoom_started = if self.reduced_motion { None } else { Some(now) };
        self.settings.layout.carousel_scale = next;
        self.save_layout();
        true
    }

    pub fn animated(&self, now: Instant) -> bool {
        // The arrows' borders turn continuously, like the titlebar edge.
        (!self.reduced_motion && self.view.len() > 1)
            || self.slide.is_some_and(|(start, _)| now.saturating_duration_since(start).as_secs_f32() < GLIDE)
            || self.zoom_started.is_some_and(|start| now.saturating_duration_since(start).as_secs_f32() < ZOOM_DURATION)
            || self.create_icon.running(now, 0.2)
            || self.create_tint.running(now, 0.2)
            || self.toolbar_zoom.running(now, ZOOM_DURATION)
            || self.toolbar_hover.running(now, TOOLBAR_HOVER)
            || self.search_reveal.running(now, SEARCH_REVEAL)
            || self.card_hover.running(now, CARD_HOVER)
            || self.selected_hover.running(now, SELECTED_HOVER)
            || self.arrow_hover.running(now, ARROW_HOVER)
            || self.arrow_pressed.iter().flatten().any(|t| now.saturating_duration_since(*t).as_secs_f32() < ARROW_PRESS)
            || self.selected_hover_since.is_some()
            || self.icon_time.iter().any(|&t| t > 0.0 && t < 1.0)
            || self.icon_return.iter().any(Option::is_some)
            || self.search_clicked.is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() < SEARCH_CLICK)
            // The search halo breathes in a loop while hovered.
            || self.toolbar_hover.sample(now, TOOLBAR_HOVER, titlebar::ICON_EASE)[3] > 0.001
            || self.refreshing
    }

    fn shift(&self, now: Instant) -> (f32, f32) {
        let Some((start, dir)) = self.slide else {
            return (0.0, 1.0);
        };
        let t = now.saturating_duration_since(start).as_secs_f32() / GLIDE;
        if t >= 1.0 {
            return (0.0, 1.0);
        }
        let remaining = 1.0 - titlebar::ease(t, EASE);
        (38.0 * dir as f32 * remaining, 1.0 - 0.15 * remaining)
    }

    fn has_card(&self, slot: usize) -> bool {
        match self.view.len() {
            0 => false,
            1 => slot == 2,
            2 => (1..=2).contains(&slot),
            3..=4 => (1..=3).contains(&slot),
            _ => true,
        }
    }

    pub fn hit(&self, window: &Window, p: PhysicalPosition<f64>, now: Instant) -> Option<Hit> {
        let g = self.geometry(window, now);
        if let Some(hit) = self.toolbar_geometry(window, g, now).hit(p) {
            return Some(hit);
        }
        if self.view.is_empty() {
            return None;
        }
        if self.view.len() > 1 {
            if contains(g.arrow(-1.0), p) {
                return Some(Hit::Prev);
            }
            if contains(g.arrow(1.0), p) {
                return Some(Hit::Next);
            }
        }
        let slide = self.shift(now).0;
        (0..5)
            .filter(|&slot| self.has_card(slot))
            .find(|&slot| contains(g.card(slot, slide), p))
            .map(|slot| Hit::Card(slot as i32 - 2))
    }

    pub fn activate(&mut self, hit: Hit, now: Instant) {
        if let Hit::Filter(filter) = hit {
            self.active_filter = filter.min(2);
            self.filter_glider
                .select(filter_rect(self.active_filter), now, self.reduced_motion);
            self.notice.clear();
            return;
        }
        if hit == Hit::Search {
            self.search_focus = true;
            self.search_clicked = (!self.reduced_motion).then_some(now);
            return;
        }
        if hit == Hit::Create {
            return;
        }
        if let Hit::Prev | Hit::Next = hit {
            self.arrow_pressed[usize::from(hit == Hit::Next)] = (!self.reduced_motion).then_some(now);
        }
        let offset = match hit {
            Hit::Prev => -1,
            Hit::Next => 1,
            Hit::Card(offset) => offset,
            Hit::Filter(_) | Hit::Create | Hit::Search => return,
        };
        if offset == 0 || self.view.len() < 2 || self.editing() {
            return;
        }
        self.selection = self.index(offset);
        self.settings
            .selected
            .clone_from(&self.cards[self.selection].instance.path);
        self.notice.clear();
        if !self.reduced_motion {
            self.slide = Some((now, offset.signum()));
        }
    }

    pub fn play(&mut self) {
        self.play_as(None);
    }

    pub fn play_as(&mut self, account: Option<(String, Option<minecraft::Skin>)>) {
        if self.refreshing || self.cards.is_empty() {
            return;
        }
        if self.editing() {
            self.edit_and_play = true;
            self.submit();
            return;
        }
        if account.is_none() && self.settings.username.is_empty() {
            let candidate = std::env::var("USERNAME")
                .ok()
                .filter(|name| minecraft::valid_username(name))
                .unwrap_or_else(|| "IkaPlayer".to_string());
            self.settings.username = candidate;
            if let Err(error) = self.save() {
                eprintln!("Perfil Minecraft: {error}");
            }
        }
        let card = &mut self.cards[self.selection];
        if matches!(card.state, Game::Preparing(_) | Game::Loading(_) | Game::Running(_)) {
            return;
        }
        // Only one preparation at a time: Minecraft versions share downloads and Java runtimes.
        if self
            .cards
            .iter()
            .any(|c| matches!(c.state, Game::Preparing(_) | Game::Loading(_)))
        {
            self.notice = "Ya se está preparando otra instancia".into();
            return;
        }
        let index = self.selection;
        self.cards[index].state = Game::Preparing("Preparando Minecraft...".into());
        let instance = self.cards[index].instance.clone();
        let tx = self.tx.clone();
        let (username, skin) = account.unwrap_or_else(|| (self.settings.username.clone(), None));
        self.notice.clear();
        std::thread::spawn(move || {
            // The downloader can report progress from multiple threads.
            let last = Mutex::new(String::new());
            let notify = |message: &str, done: usize, total: usize| {
                let text = if total > 1 && done <= total && !message.contains('(') {
                    format!("{message}  {}%", (done as u128 * 100 / total as u128))
                } else {
                    message.to_owned()
                };
                let Ok(mut previous) = last.lock() else {
                    return;
                };
                if text != *previous {
                    previous.clone_from(&text);
                    tx.send(Update::Game(instance.path.clone(), Game::Preparing(text)));
                }
            };
            let pid = Cell::new(None);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                minecraft::play(&instance, &username, skin, &notify, |child| {
                    pid.set(Some(child));
                    tx.send(Update::Game(instance.path.clone(), Game::Loading(child)));
                }, || {
                    if let Some(child) = pid.get() {
                        tx.send(Update::Game(instance.path.clone(), Game::Running(child)));
                    }
                })
            }))
            .unwrap_or_else(|_| Err("Falló la preparación de Minecraft".into()));
            if result.is_ok() {
                // Mods or configs may have changed while playing: refresh the
                // IkaBoost cache now, invisibly, instead of on the next Play.
                let (instance, username) = (instance.clone(), username.clone());
                std::thread::spawn(move || {
                    if let Err(e) = minecraft::prewarm(&instance, &username) {
                        eprintln!("IkaBoost: {e}");
                    }
                });
            }
            match result {
                Ok(()) => tx.send(Update::Game(instance.path.clone(), Game::Closed)),
                Err(error) => {
                    eprintln!("Minecraft: {error}");
                    let state = if pid.get().is_some() { Game::Crashed(error) } else { Game::Failed(error) };
                    tx.send(Update::Game(instance.path.clone(), state));
                }
            }
        });
    }

    fn save_layout(&self) {
        if let Err(error) = self.settings_store.write(&self.settings, false) {
            eprintln!("No se pudo guardar el diseño de Inicio: {error}");
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.settings_store.write(&self.settings, true)
    }

    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }

    pub fn play_state(&self) -> crate::dock::play::State {
        let Some(card) = self.cards.get(self.selection) else {
            return crate::dock::play::State::Idle;
        };
        match &card.state {
            Game::Idle | Game::Closed | Game::Failed(_) | Game::Crashed(_) => crate::dock::play::State::Idle,
            Game::Running(_) => crate::dock::play::State::Running,
            Game::Preparing(_) | Game::Loading(_) => crate::dock::play::State::Preparing,
        }
    }

    pub fn cancel(&mut self) {
        self.edit = None;
        self.edit_and_play = false;
        self.notice.clear();
    }

    pub fn input(&mut self, text: &str) {
        if let Some(edit) = &mut self.edit {
            let room = 16usize.saturating_sub(edit.len());
            edit.extend(
                text.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .take(room),
            );
            self.notice.clear();
        }
    }

    pub fn backspace(&mut self) {
        if let Some(edit) = &mut self.edit {
            edit.pop();
        }
    }

    pub fn submit(&mut self) {
        let Some(edit) = self.edit.as_ref() else {
            return;
        };
        if !minecraft::valid_username(edit) {
            self.notice = "Usa de 2 a 16 letras, números o _".into();
            return;
        }
        self.settings.username.clone_from(edit);
        self.edit = None;
        self.notice.clear();
        let auto = std::mem::take(&mut self.edit_and_play);
        if let Err(error) = self.save() {
            eprintln!("Perfil Minecraft: {error}");
            self.notice = "No se pudo guardar el nombre".into();
        } else if auto {
            self.play();
        }
    }

    pub fn uniforms(&mut self, p: &mut Params, window: &Window, now: Instant, hover: Option<Hit>) {
        let g = self.geometry(window, now);
        let (slide, opacity) = self.shift(now);
        let total = self.view.len();
        let hovered = match hover {
            Some(Hit::Card(offset)) => offset as f32 + 3.0,
            Some(Hit::Prev) => 6.0,
            Some(Hit::Next) => 7.0,
            Some(Hit::Filter(filter)) => 8.0 + filter as f32,
            Some(Hit::Create) => 12.0,
            Some(Hit::Search) => 13.0,
            None => 0.0,
        };
        p.home_view = [g.center[0], g.center[1], g.scale, opacity];
        p.home_toolbar_view = self.toolbar_geometry(window, g, now).view;
        p.home_toolbar_rects = TOOLBAR_RECTS;
        let status = if total == 0 && !self.cards.is_empty() {
            "Sin resultados para la búsqueda"
        } else if let Some(card) = self.cards.get(self.selection) {
            if !self.notice.is_empty() {
                self.notice.as_str()
            } else if self.editing() {
                "Nombre de Minecraft · Enter para guardar"
            } else {
                match &card.state {
                    Game::Idle | Game::Closed => "Flechas para elegir · Enter para jugar",
                    Game::Preparing(message) => message,
                    Game::Failed(message) | Game::Crashed(message) => message.lines().next().unwrap_or("Error de Minecraft"),
                    Game::Loading(_) => "Cargando Minecraft...",
                    Game::Running(_) => "Minecraft en ejecución",
                }
            }
        } else if self.ready {
            "No se encontraron instancias de Minecraft"
        } else {
            "Buscando instancias de Minecraft..."
        };
        p.home_state = [
            total.min(5) as f32,
            hovered,
            f32::from(self.editing()),
            slide,
        ];
        let create_hot = f32::from(hover == Some(Hit::Create));
        let (icon, tint) = if self.reduced_motion {
            (0.0, create_hot)
        } else {
            self.create_icon
                .set([create_hot], now, CREATE_ICON_DURATION, titlebar::ICON_EASE);
            self.create_tint
                .set([create_hot], now, CREATE_TINT_DURATION, EASE);
            let [icon] = self
                .create_icon
                .sample(now, CREATE_ICON_DURATION, titlebar::ICON_EASE);
            let [tint] = self.create_tint.sample(now, CREATE_TINT_DURATION, EASE);
            (icon, tint)
        };
        let hover_target = [
            f32::from(hover == Some(Hit::Filter(0))),
            f32::from(hover == Some(Hit::Filter(1))),
            f32::from(hover == Some(Hit::Filter(2))),
            f32::from(hover == Some(Hit::Search)),
        ];
        if self.reduced_motion {
            self.toolbar_hover = titlebar::Transition::new(hover_target, now);
        } else {
            self.toolbar_hover.set(hover_target, now, TOOLBAR_HOVER, titlebar::ICON_EASE);
        }
        p.home_controls[3] = self.toolbar_hover.sample(now, TOOLBAR_HOVER, titlebar::ICON_EASE);
        let open = f32::from(self.search_focus || !self.search.is_empty());
        if self.reduced_motion {
            self.search_reveal = titlebar::Transition::new([open], now);
        } else {
            self.search_reveal.set([open], now, SEARCH_REVEAL, titlebar::ICON_EASE);
        }
        let [reveal] = self.search_reveal.sample(now, SEARCH_REVEAL, titlebar::ICON_EASE);
        let click = self
            .search_clicked
            .map_or(1.0, |t| (now.saturating_duration_since(t).as_secs_f32() / SEARCH_CLICK).min(1.0));
        // Arrow border loop: one clockwise turn every 11 s, from the same clock
        // as every Home animation, so it never jumps when a period wraps.
        let ring = if self.reduced_motion { 0.0 } else { (now.saturating_duration_since(self.phase_started).as_secs_f64() / 11.0).fract() as f32 };
        p.home_controls[4] = [reveal, click, ring, 0.0];
        // Slots 0,1,3,4 (the selected slot 2 has its own frame).
        let card = |slot: i32| f32::from(hover == Some(Hit::Card(slot)));
        let targets = [card(-2), card(-1), card(1), card(2)];
        if self.reduced_motion {
            self.card_hover = titlebar::Transition::new(targets, now);
        } else {
            self.card_hover.set(targets, now, CARD_HOVER, CARD_EASE);
        }
        let [h0, h1, h3, h4] = self.card_hover.sample(now, CARD_HOVER, CARD_EASE);
        p.home_controls[5] = [h0, h1, h3, h4];
        let on_selected = hover == Some(Hit::Card(0));
        if on_selected && self.selected_hover_since.is_none() {
            self.selected_hover_since = Some(now);
        }
        if self.reduced_motion {
            self.selected_hover = titlebar::Transition::new([f32::from(on_selected)], now);
        } else {
            self.selected_hover.set([f32::from(on_selected)], now, SELECTED_HOVER, CARD_EASE);
        }
        let [selected] = self.selected_hover.sample(now, SELECTED_HOVER, CARD_EASE);
        if selected <= 0.0 && !on_selected {
            self.selected_hover_since = None;
        }
        // y: seconds since it started, drives the looping shine while hovered.
        let alive = self.selected_hover_since.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        p.home_controls[8] = [selected, alive, 0.0, 0.0];
        let arrows = [f32::from(hover == Some(Hit::Prev)), f32::from(hover == Some(Hit::Next))];
        if self.reduced_motion {
            self.arrow_hover = titlebar::Transition::new(arrows, now);
        } else {
            self.arrow_hover.set(arrows, now, ARROW_HOVER, titlebar::ICON_EASE);
        }
        let [prev_hot, next_hot] = self.arrow_hover.sample(now, ARROW_HOVER, titlebar::ICON_EASE);
        let press = |t: Option<Instant>| {
            t.map_or(1.0, |t| (now.saturating_duration_since(t).as_secs_f32() / ARROW_PRESS).min(1.0))
        };
        p.home_controls[9] = [prev_hot, next_hot, press(self.arrow_pressed[0]), press(self.arrow_pressed[1])];
        // Each filter icon plays forward while the cursor is over it. Leaving
        // before it ends doesn't cut it: the icon freezes the pose it reached
        // and glides that pose back to rest (no reversed bounce). When it had
        // finished, its last pose already is the resting one. Nothing pops.
        // home_controls[6] = clock of each icon; home_controls[7] = how much
        // of each pose is still shown (1 while playing, easing to 0 on return).
        let dt = self.icon_last.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32()).min(0.1);
        self.icon_last = Some(now);
        let mut clocks = [0.0f32; 4];
        let mut weights = [1.0f32; 4];
        // 0..3: Todas, Recientes, Favoritas; 3: the search magnifier.
        for i in 0..4 {
            let target = if i == 3 { Hit::Search } else { Hit::Filter(i as u8) };
            let over = hover == Some(target) && !self.reduced_motion;
            if over {
                if self.icon_return[i].take().is_some() {
                    self.icon_time[i] = 0.0;
                }
                self.icon_time[i] = (self.icon_time[i] + dt / ICON_PLAY).min(1.0);
            } else if self.icon_return[i].is_none() && self.icon_time[i] > 0.0 && self.icon_time[i] < 1.0 {
                self.icon_return[i] = Some((self.icon_time[i], 1.0));
            }
            if let Some((from, left)) = self.icon_return[i] {
                let left = (left - dt / ICON_RETURN).max(0.0);
                self.icon_time[i] = from;
                if left <= 0.0 {
                    self.icon_return[i] = None;
                    self.icon_time[i] = 0.0;
                } else {
                    self.icon_return[i] = Some((from, left));
                    // Ease-out: fast at first, gently landing at rest.
                    weights[i] = left * left * (3.0 - 2.0 * left);
                }
            } else if !over && self.icon_time[i] >= 1.0 {
                self.icon_time[i] = 0.0;
            }
            clocks[i] = self.icon_time[i];
        }
        p.home_controls[7] = weights;
        p.home_controls[6] = clocks;
        let [filter_x, filter_width] = self.filter_glider.sample(now);
        p.home_toolbar = [filter_x, filter_width, icon, tint];
        let elapsed = now.duration_since(self.phase_started).as_secs_f32();
        p.home_phase = [
            elapsed.rem_euclid(10.0),
            if self.reduced_motion {
                0.0
            } else {
                elapsed.rem_euclid(CREATE_BORDER_PERIOD) / CREATE_BORDER_PERIOD
            },
            self.active_filter as f32,
            // 0: placeholder, 1: typed text, 2: focused with a caret.
            if self.search_focus {
                2.0
            } else {
                f32::from(!self.search.is_empty())
            },
        ];
        // A steady caret: blinking would redraw the launcher all the time.
        let caret = if self.search_focus { "|" } else { "" };
        p.home_text[13].write(&format!("{}{caret}", self.search), 150.0);
        p.home_text[10].write(status, 260.0);
        if total == 0 {
            return;
        }
        p.home_text[12].write(&self.cards[self.selection].instance.name, 256.0);
        let profile = if let Some(edit) = &self.edit {
            format!("{edit}_")
        } else if self.settings.username.is_empty() {
            "Configurar jugador".into()
        } else {
            format!("Jugador: {}", self.settings.username)
        };
        p.home_text[11].write(&profile, 242.0);
        p.home_controls[0] = g.arrow(-1.0);
        p.home_controls[1] = g.arrow(1.0);
        for slot in if total >= 5 { 0..5 } else { 1..4 } {
            p.home_rects[slot] = g.card(slot, slide);
            if self.has_card(slot) {
                let idx = self.index(slot as i32 - 2);
                let card = &self.cards[idx];
                let i = &card.instance;
                p.home_images[slot] = [
                    idx as u32 % 40 + 1,
                    card.icon,
                    u32::from(matches!(card.state, Game::Preparing(_) | Game::Loading(_) | Game::Running(_))),
                    card.custom_cover,
                ];
                let factor = [0.78, 0.88, 1.0, 0.88, 0.78][slot];
                let space = WIDTHS[slot] - 78.0 * factor;
                p.home_text[slot * 2].write(&i.name, space / factor);
                let java = i.java_version.as_deref().unwrap_or("Java");
                let details = format!(
                    "{} · {} · {} mods · {}",
                    i.mc_version,
                    i.loader,
                    i.mod_count,
                    if i.mc_version.starts_with("26.") && java == "Java 21" {
                        "Java 25"
                    } else {
                        java
                    }
                );
                p.home_text[slot * 2 + 1].write(&details, space / (factor * 0.81));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dummy_sender() -> Sender {
        use std::sync::OnceLock;
        use winit::event_loop::EventLoop;
        use winit::platform::windows::EventLoopBuilderExtWindows;
        static PROXY: OnceLock<EventLoopProxy<()>> = OnceLock::new();
        let wake = PROXY
            .get_or_init(|| {
                let event_loop: EventLoop<()> =
                    EventLoop::builder().with_any_thread(true).build().unwrap();
                event_loop.create_proxy()
            })
            .clone();
        Sender {
            tx: mpsc::channel().0,
            wake,
        }
    }

    #[test]
    fn settings_defaults_carousel_scale_to_one() {
        let json = r#"{"username":"Test","selected":""}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.layout, LayoutSettings::default());
    }

    #[test]
    fn settings_roundtrip_carousel_layout() {
        let json = r#"{"username":"Test","selected":"inst1","carousel_scale":1.25,"carousel_offset_y":-13,"toolbar_scale":0.85,"toolbar_offset_y":-23}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert!((s.layout.carousel_scale - 1.25).abs() < f32::EPSILON);
        let out = serde_json::to_string(&s).unwrap();
        assert!(out.contains(r#""carousel_scale":1.25"#));
        let loaded = serde_json::from_str::<Settings>(&out).unwrap();
        assert_eq!(loaded.layout.carousel_offset_y, -13);
        assert!((loaded.layout.toolbar_scale - 0.85).abs() < f32::EPSILON);
        assert_eq!(loaded.layout.toolbar_offset_y, -23);
    }

    #[test]
    fn layout_survives_restart_and_another_windows_profile_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("minecraft-home.json");
        let (older, mut stale) = SettingsStore::open(Some(path.clone()));
        let (newer, mut current) = SettingsStore::open(Some(path.clone()));
        current.layout = LayoutSettings {
            carousel_scale: 1.25,
            carousel_offset_y: 32,
            toolbar_scale: 0.85,
            toolbar_offset_y: -23,
        };
        newer.write(&current, false).unwrap();
        stale.username = "Player".into();
        older.write(&stale, true).unwrap();

        let (_, restarted) = SettingsStore::open(Some(path));
        assert_eq!(restarted.layout, current.layout);
        assert_eq!(restarted.username, "Player");
    }

    #[test]
    fn toolbar_positions_and_hits_follow_its_own_layout() {
        let area = HomeArea::from_size(2560.0, 1440.0, 1.0);
        let carousel = Geometry::new(area, true, 1.0, 0);
        let original = ToolbarGeometry::new(area, carousel, true, 1.0, 0);
        let moved = ToolbarGeometry::new(area, carousel, true, 1.25, -70);
        assert_eq!(original.view[2], 1.0);
        assert_eq!(moved.view[1], original.view[1] - 70.0);
        assert_eq!(moved.view[2], 1.25);
        let search = TOOLBAR_RECTS[5];
        let cursor = PhysicalPosition::new(
            f64::from(moved.view[0] + (search[0] + search[2] * 0.5) * moved.view[2]),
            f64::from(moved.view[1]),
        );
        assert_eq!(moved.hit(cursor), Some(Hit::Search));
        assert_eq!(original.hit(cursor), None);
        let shifted_carousel = Geometry::new(area, true, 1.0, 18);
        assert_eq!(ToolbarGeometry::new(area, shifted_carousel, true, 1.25, -70).view, moved.view);
    }

    #[test]
    fn carousel_can_lower_to_dock_clearance_and_toolbar_stays_above_cards() {
        let area = HomeArea::from_size(2560.0, 1440.0, 1.0);
        let high = Geometry::new(area, true, 1.0, -1000);
        assert_eq!(high.offset_y, -32);
        assert!(Geometry::new(area, true, 1.0, 1000).offset_y > 32);
        for zoom in [0.65, 1.0, 1.4] {
            let low = Geometry::new(area, true, zoom, 1000);
            let [_, y, _, height] = low.card(2, 0.0);
            let padding = area.bottom - (y + height);
            assert!(padding >= 4.0 * low.scale - 0.001);
            assert!(padding < 4.0 * low.scale + area.dpi + 0.001);
        }
        let toolbar = ToolbarGeometry::new(area, high, true, 1.4, 1000);
        let toolbar_bottom = toolbar.view[1] + TOOLBAR_HALF_HEIGHT * toolbar.view[2];
        assert!(toolbar_bottom < high.card(2, 0.0)[1]);
    }

    #[test]
    fn zoom_step_and_clamping() {
        let mut cur = 1.0f32;
        // Zooming in with step 0.05
        for _ in 0..20 {
            cur = ((cur + ZOOM_STEP) * 100.0).round() / 100.0;
            cur = cur.clamp(MIN_ZOOM, MAX_ZOOM);
        }
        assert_eq!(cur, MAX_ZOOM);
        assert!((cur - 1.40).abs() < f32::EPSILON);

        // Zooming out with step 0.05
        for _ in 0..30 {
            cur = ((cur - ZOOM_STEP) * 100.0).round() / 100.0;
            cur = cur.clamp(MIN_ZOOM, MAX_ZOOM);
        }
        assert_eq!(cur, MIN_ZOOM);
        assert!((cur - 0.65).abs() < f32::EPSILON);
    }

    #[test]
    fn zoom_smooth_interpolation() {
        let now = Instant::now();
        let home = Home {
            cards: Vec::new(),
            selection: 0,
            settings: Settings::default(),
            settings_store: SettingsStore::open(None).0,
            edit: None,
            edit_and_play: false,
            notice: String::new(),
            ready: true,
            refreshing: false,
            slide: None,
            phase_started: now,
            create_icon: titlebar::Transition::new([0.0], now),
            create_tint: titlebar::Transition::new([0.0], now),
            zoom_target: 1.20,
            zoom_start: 1.00,
            zoom_started: Some(now),
            toolbar_zoom: titlebar::Transition::new([1.0], now),
            nudge_target: None,
            reduced_motion: false,
            active_filter: 0,
            filter_glider: titlebar::Glider::new(filter_rect(0), now),
            toolbar_hover: titlebar::Transition::new([0.0; 4], now),
            search_reveal: titlebar::Transition::new([0.0], now),
            card_hover: titlebar::Transition::new([0.0; 4], now),
            selected_hover: titlebar::Transition::new([0.0], now),
            arrow_hover: titlebar::Transition::new([0.0; 2], now),
            arrow_pressed: [None; 2],
            selected_hover_since: None,
            icon_time: [0.0; 4],
            icon_return: [None; 4],
            search_clicked: None,
            icon_last: None,
            search: String::new(),
            search_focus: false,
            view: Vec::new(),
            custom_icons: Default::default(),
            tx: dummy_sender(),
            rx: mpsc::channel().1,
        };

        // At t = 0, should be start zoom
        assert!((home.current_zoom(now) - 1.00).abs() < 1e-4);

        // Halfway through (60ms), should be easing towards target
        let mid = now + Duration::from_millis(60);
        let mid_val = home.current_zoom(mid);
        assert!(mid_val > 1.00 && mid_val < 1.20);

        // At end (120ms) or after, should reach target exactly
        let end = now + Duration::from_millis(120);
        assert!((home.current_zoom(end) - 1.20).abs() < 1e-4);

        let past = now + Duration::from_millis(200);
        assert!((home.current_zoom(past) - 1.20).abs() < 1e-4);
    }

    #[test]
    fn reset_zoom_restores_one() {
        let now = Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("minecraft-home.json");
        std::fs::write(&path, r#"{"carousel_scale":1.35}"#).unwrap();
        let (settings_store, settings) = SettingsStore::open(Some(path.clone()));
        let mut home = Home {
            cards: Vec::new(),
            selection: 0,
            settings,
            settings_store,
            edit: None,
            edit_and_play: false,
            notice: String::new(),
            ready: true,
            refreshing: false,
            slide: None,
            phase_started: now,
            create_icon: titlebar::Transition::new([0.0], now),
            create_tint: titlebar::Transition::new([0.0], now),
            zoom_target: 1.35,
            zoom_start: 1.35,
            zoom_started: None,
            toolbar_zoom: titlebar::Transition::new([1.0], now),
            nudge_target: None,
            reduced_motion: false,
            active_filter: 0,
            filter_glider: titlebar::Glider::new(filter_rect(0), now),
            toolbar_hover: titlebar::Transition::new([0.0; 4], now),
            search_reveal: titlebar::Transition::new([0.0], now),
            card_hover: titlebar::Transition::new([0.0; 4], now),
            selected_hover: titlebar::Transition::new([0.0], now),
            arrow_hover: titlebar::Transition::new([0.0; 2], now),
            arrow_pressed: [None; 2],
            selected_hover_since: None,
            icon_time: [0.0; 4],
            icon_return: [None; 4],
            search_clicked: None,
            icon_last: None,
            search: String::new(),
            search_focus: false,
            view: Vec::new(),
            custom_icons: Default::default(),
            tx: dummy_sender(),
            rx: mpsc::channel().1,
        };

        assert!(home.reset_zoom(now));
        assert_eq!(home.zoom_target, 1.00);
        assert_eq!(Settings::read_at(&path).layout.carousel_scale, 1.00);

        // Resetting again when already 1.0 should return false (no change)
        assert!(!home.reset_zoom(now));
    }

    fn test_home(names: &[(&str, &str, &str)]) -> Home {
        let now = Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let (settings_store, settings) = SettingsStore::open(Some(dir.path().join("h.json")));
        let cards = names
            .iter()
            .map(|(name, mc, loader)| Card {
                instance: minecraft::Instance {
                    name: (*name).into(),
                    path: format!("C:/instances/{name}"),
                    mod_count: 0,
                    mc_version: (*mc).into(),
                    loader: (*loader).into(),
                    loader_version: None,
                    icon_path: None,
                    java_version: None,
                    java_vendor: "",
                    total_time_played: None,
                    shader_count: 0,
                    resourcepack_count: 0,
                    max_mem_mb: 8192,
                },
                icon: 0,
                custom_cover: 0,
                state: Game::Idle,
            })
            .collect();
        let mut home = Home {
            cards,
            selection: 0,
            settings,
            settings_store,
            edit: None,
            edit_and_play: false,
            notice: String::new(),
            ready: true,
            refreshing: false,
            slide: None,
            phase_started: now,
            create_icon: titlebar::Transition::new([0.0], now),
            create_tint: titlebar::Transition::new([0.0], now),
            zoom_target: 1.0,
            zoom_start: 1.0,
            zoom_started: None,
            toolbar_zoom: titlebar::Transition::new([1.0], now),
            nudge_target: None,
            reduced_motion: true,
            active_filter: 0,
            filter_glider: titlebar::Glider::new(filter_rect(0), now),
            toolbar_hover: titlebar::Transition::new([0.0; 4], now),
            search_reveal: titlebar::Transition::new([0.0], now),
            card_hover: titlebar::Transition::new([0.0; 4], now),
            selected_hover: titlebar::Transition::new([0.0], now),
            arrow_hover: titlebar::Transition::new([0.0; 2], now),
            arrow_pressed: [None; 2],
            selected_hover_since: None,
            icon_time: [0.0; 4],
            icon_return: [None; 4],
            search_clicked: None,
            icon_last: None,
            search: String::new(),
            search_focus: false,
            view: Vec::new(),
            custom_icons: Default::default(),
            tx: dummy_sender(),
            rx: mpsc::channel().1,
        };
        home.refilter();
        home
    }

    #[test]
    fn search_filters_by_name_version_and_loader_and_cycles_inside_results() {
        let mut home = test_home(&[
            ("Keo Optimized", "1.20.1", "Forge"),
            ("RLCraft", "1.12.2", "Forge"),
            ("Fabulous", "1.21.1", "Fabric"),
            ("Keo Neo", "1.21.1", "NeoForge"),
        ]);
        let now = Instant::now();
        // Typing does nothing until the field is clicked.
        home.search_input("keo");
        assert_eq!(home.view.len(), 4);

        home.activate(Hit::Search, now);
        assert!(home.searching());
        home.search_input("KEO");
        assert_eq!(home.view, vec![0, 3]);
        assert_eq!(home.selection, 0);
        home.activate(Hit::Next, now);
        assert_eq!(home.selection, 3);
        home.activate(Hit::Next, now);
        assert_eq!(home.selection, 0, "wraps inside the results");

        home.search_escape();
        home.search_input("fabric");
        assert_eq!(home.view, vec![2]);
        assert_eq!(home.selection, 2, "selection moves into the results");

        home.search_escape();
        home.search_input("1.21");
        assert_eq!(home.view, vec![2, 3]);

        home.search_escape();
        home.search_input("zzz");
        assert!(home.view.is_empty());
        assert!(home.active_instance().is_some(), "Play keeps a real instance");

        // Esc clears, a second Esc leaves the field; everything is back.
        home.search_escape();
        assert!(home.searching());
        assert_eq!(home.view.len(), 4);
        home.search_escape();
        assert!(!home.searching());
    }

    #[test]
    fn picking_a_hidden_instance_from_the_collection_clears_the_search() {
        let mut home = test_home(&[("Alpha", "1.20.1", "Forge"), ("Beta", "1.20.1", "Forge"), ("Gamma", "1.21", "Fabric")]);
        let now = Instant::now();
        home.activate(Hit::Search, now);
        home.search_input("alpha");
        home.select_instance(2, now);
        assert_eq!(home.selection, 2);
        assert_eq!(home.view.len(), 3);
        assert!(!home.searching());
    }


    #[test]
    fn block_faces_match_the_webview2_assignment() {
        // Expected values come from running the original JavaScript
        // (getUniqueBlockFaceForInstance) over the same keys.
        let names: Vec<&str> = BLOCK_FACE_NAMES.lines().collect();
        let keys = ["C:/a/one", "C:/a/two", "C:/A/One", "C:/a/three"].map(String::from);
        let faces = webview_block_faces(keys, names.len());
        assert_eq!(faces[0], faces[2], "same key, case-insensitive");
        assert_ne!(faces[0], faces[1]);
        assert_ne!(faces[1], faces[3]);
        let expected = std::env::var("IKA_FACES_EXPECTED").ok();
        if let Some(path) = expected {
            let text = std::fs::read_to_string(path).unwrap();
            let rows: Vec<(String, String)> = text
                .lines()
                .map(|l| {
                    let (k, f) = l.split_once('\t').unwrap();
                    (k.to_owned(), f.to_owned())
                })
                .collect();
            let got = webview_block_faces(rows.iter().map(|(k, _)| k.clone()), names.len());
            for ((key, want), face) in rows.iter().zip(got) {
                assert_eq!(names[face], want, "{key}");
            }
            println!("checked {} instances", rows.len());
        }
    }

}

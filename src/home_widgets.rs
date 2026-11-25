mod collection;
mod occlusion;
mod paint;
pub use occlusion::ClockOcclusion;
pub use paint::add_font;

use crate::{dock, home::Home, minecraft, titlebar::Transition};
use egui::{Pos2, Rect, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
use winit::{
    dpi::PhysicalPosition,
    window::{CursorIcon, Window},
};

const SLIDE: f32 = 0.38;
const EASE: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
const HIDE: Duration = Duration::from_secs(5);
const FORMATS: [(Style, &str); 8] = [
    (Style::Pixel, "Pixel Neón"),
    (Style::Minecraft, "Minecraft"),
    (Style::Ios, "iOS"),
    (Style::PixelG, "Pixel"),
    (Style::OneUi, "One UI"),
    (Style::Mond, "Mond"),
    (Style::Words, "Palabras"),
    (Style::Ring, "Anillo"),
];

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    #[default]
    Pixel,
    Minecraft,
    Ios,
    PixelG,
    OneUi,
    Mond,
    Words,
    Ring,
}

impl Style {
    fn shows_seconds(self) -> bool {
        matches!(self, Self::Pixel | Self::Ring)
    }

    pub fn from_webview(value: &str) -> Self {
        match value {
            "minecraft" => Self::Minecraft,
            "ios" => Self::Ios,
            "pixel-g" => Self::PixelG,
            "oneui" => Self::OneUi,
            "mond" => Self::Mond,
            "words" => Self::Words,
            "ring" => Self::Ring,
            _ => Self::Pixel,
        }
    }

    fn size(self) -> egui::Vec2 {
        match self {
            Self::Pixel | Self::Minecraft => vec2(284., 115.),
            Self::PixelG | Self::Ring => vec2(185., 185.),
            Self::Mond | Self::Words => vec2(285., 150.),
            _ => vec2(280., 135.),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Side {
    Left,
    Right,
}

impl Side {
    const ALL: [Self; 2] = [Self::Left, Self::Right];
    fn index(self) -> usize {
        if self == Self::Left { 0 } else { 1 }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    left: f32,
    top: f32,
    viewport_width: f32,
    viewport_height: f32,
    width: f32,
    height: f32,
    scale: f32,
    docked_side: Option<Side>,
    style: Style,
    clock_24h: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            left: 229.,
            top: 134.,
            viewport_width: 2560.,
            viewport_height: 1382.,
            width: 284.,
            height: 115.,
            scale: 1.,
            docked_side: None,
            style: Style::Pixel,
            clock_24h: false,
        }
    }
}

impl Settings {
    fn read() -> (Self, Option<PathBuf>, bool) {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join("IkaNative/home-clock.json"));
        if let Some(mut saved) = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|data| serde_json::from_slice::<Self>(&data).ok())
        {
            saved.sanitize();
            return (saved, path, true);
        }
        let mut legacy = std::fs::read(minecraft::ika_root().join("home-layout.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&bytes).ok()
            })
            .unwrap_or_default();
        let mut saved = legacy
            .remove("ika_pixel_clock_pos")
            .and_then(|value| serde_json::from_str::<Self>(&value).ok())
            .unwrap_or_default();
        if let Some(scale) = legacy
            .get("ika_clock_scale")
            .and_then(|s| s.parse::<f32>().ok())
        {
            saved.scale = scale;
        }
        saved.sanitize();
        (saved, path, false)
    }

    fn sanitize(&mut self) {
        let d = Self::default();
        for (v, default) in [
            (&mut self.left, d.left),
            (&mut self.top, d.top),
            (&mut self.viewport_width, d.viewport_width),
            (&mut self.viewport_height, d.viewport_height),
            (&mut self.width, d.width),
            (&mut self.height, d.height),
        ] {
            if !(*v >= 0. && *v <= 32_768.) {
                *v = default;
            }
        }
        if !self.scale.is_finite() {
            self.scale = 1.;
        }
        self.scale = self.scale.clamp(0.5, 2.5);
    }
}

#[derive(Clone, Copy)]
struct Area {
    stage: Rect,
    height: f32,
    dpi: f32,
}

impl Area {
    fn from_window(window: &Window) -> Self {
        let size = window.inner_size();
        let dpi = window.scale_factor() as f32;
        let height = size.height as f32 / dpi;
        let top = 52.;
        Self {
            stage: Rect::from_min_max(
                pos2(0., top),
                pos2(
                    size.width as f32 / dpi,
                    (height - dock::hairline_height(window) / dpi).max(top),
                ),
            ),
            height,
            dpi,
        }
    }

    fn pointer(self, p: PhysicalPosition<f64>) -> Pos2 {
        pos2(p.x as f32 / self.dpi, p.y as f32 / self.dpi)
    }

    fn panel(self, side: Side, amount: f32) -> Rect {
        let width =
            (if side == Side::Left { 417.0_f32 } else { 420. }).min(self.stage.width() * 0.85);
        let x = if side == Side::Left {
            width * (amount - 1.)
        } else {
            self.stage.right() - width * amount
        };
        rect(x, self.stage.top(), width, self.stage.height())
    }

    fn edge(self, p: Pos2) -> Option<Side> {
        self.stage
            .contains(p)
            .then(|| {
                if p.x <= 32. {
                    Some(Side::Left)
                } else if p.x >= self.stage.right() - 32. {
                    Some(Side::Right)
                } else {
                    None
                }
            })
            .flatten()
    }

    fn clock(self, settings: &Settings) -> Rect {
        let inside = self.stage.shrink(10.);
        let size = settings.style.size();
        let scale = settings
            .scale
            .min(inside.width() / size.x)
            .min(inside.height() / size.y)
            .max(0.01);
        let size = size * scale;
        let x = settings.left * (self.stage.width() - size.x).max(1.)
            / (settings.viewport_width - settings.width).max(1.);
        let y = settings.top * (self.height - self.stage.top() - size.y).max(1.)
            / (settings.viewport_height - self.stage.top() - settings.height).max(1.);
        self.fit_clock(pos2(x, self.stage.top() + y), size)
    }

    fn fit_clock(self, p: Pos2, size: egui::Vec2) -> Rect {
        let inside = self.stage.shrink(10.);
        let min = pos2(inside.left(), inside.top() + 4.);
        let max = (inside.max - size).max(min);
        Rect::from_min_size(p.clamp(min, max), size)
    }
}

struct Panel {
    open: bool,
    slide: Transition<1>,
    hide_at: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Panel,
    Clock,
    Resize,
    Format,
    Style(Style),
    Reset,
    Collection(collection::Hit),
    Folder,
    Shortcut(usize),
    ClearActivity,
    RefreshNews,
}

pub enum Action {
    Instance(usize),
    InstanceFolder(usize),
    Create,
    Folder,
    Shortcut(usize),
}

#[derive(Clone, Copy)]
enum Drag {
    Move { offset: egui::Vec2, start: Pos2 },
    Resize { start: Pos2, scale: f32 },
}

pub struct Widgets {
    settings: Settings,
    path: Option<PathBuf>,
    native_settings: bool,
    sides: [Panel; 2],
    reduced_motion: bool,
    cursor: Option<Pos2>,
    drag: Option<Drag>,
    pressed: Option<Hit>,
    menu: Option<Pos2>,
    dirty: bool,
    show_clock: bool,
    show_panels: bool,
    obscured_dock_amount: Option<f32>,
    collection: collection::Collection,
    time: SYSTEMTIME,
    next_clock: Instant,
    next_slide: Option<Instant>,
    recent: std::collections::VecDeque<(String, String)>,
    news_scroll: f32,
}

impl Widgets {
    pub fn new(now: Instant, reduced_motion: bool) -> Self {
        let (settings, path, native_settings) = Settings::read();
        Self {
            settings,
            path,
            native_settings,
            reduced_motion,
            sides: std::array::from_fn(|_| Panel {
                open: false,
                slide: Transition::new([0.], now),
                hide_at: None,
            }),
            cursor: None,
            drag: None,
            pressed: None,
            menu: None,
            dirty: true,
            show_clock: false,
            show_panels: false,
            obscured_dock_amount: None,
            collection: collection::Collection::new(now),
            time: unsafe { GetLocalTime() },
            next_clock: now,
            next_slide: None,
            recent: Default::default(),
            news_scroll: 0.,
        }
    }

    pub fn record(&mut self, message: String) {
        let local = unsafe { GetLocalTime() };
        self.recent.push_front((
            format!(
                "{:02}/{:02}/{:04}   {:02}:{:02}",
                local.wDay, local.wMonth, local.wYear, local.wHour, local.wMinute
            ),
            message,
        ));
        self.recent.truncate(3);
        self.dirty = true;
    }

    pub fn apply_webview(&mut self, style: Style, clock_24h: bool) {
        if !self.native_settings
            && (self.settings.style != style || self.settings.clock_24h != clock_24h)
        {
            if self.settings.style != style {
                self.next_clock = Instant::now();
            }
            self.settings.style = style;
            self.settings.clock_24h = clock_24h;
            self.dirty = true;
        }
    }

    pub fn import_favorites(&mut self, favorites: &[String]) {
        self.dirty |= self.collection.import_favorites(favorites);
    }

    pub fn collection_editing(&self) -> bool {
        self.collection.editing()
    }

    pub fn collection_input(&mut self, text: &str) {
        self.collection.input(text);
        self.dirty = true;
    }

    pub fn collection_backspace(&mut self) {
        self.collection.backspace();
        self.dirty = true;
    }

    pub fn dismiss_collection_input(&mut self, window: &Window, now: Instant) {
        self.collection.dismiss();
        let area = Area::from_window(window);
        if self
            .cursor
            .is_none_or(|p| self.panel_at(area, p, now) != Some(Side::Left))
        {
            self.sides[Side::Left.index()].hide_at = Some(now + HIDE);
        }
        self.dirty = true;
    }

    pub fn blocks(
        &self,
        window: &Window,
        pointer: PhysicalPosition<f64>,
        now: Instant,
        home: &Home,
    ) -> bool {
        let area = Area::from_window(window);
        self.drag.is_some()
            || self
                .read_hit(area, area.pointer(pointer), now, home)
                .is_some()
    }

    fn save(&mut self) {
        self.native_settings = true;
        let Some(path) = &self.path else { return };
        let result = (|| -> std::io::Result<()> {
            let dir = path.parent().unwrap();
            std::fs::create_dir_all(dir)?;
            let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
            serde_json::to_writer(tmp.as_file_mut(), &self.settings)?;
            tmp.persist(path)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("No se pudo guardar el reloj de Inicio: {error}");
        }
    }

    fn set_panel(&mut self, side: Side, open: bool, now: Instant) {
        let p = &mut self.sides[side.index()];
        p.hide_at = None;
        if open == p.open {
            return;
        }
        p.open = open;
        if !open && side == Side::Left {
            self.collection.dismiss();
        }
        let end = [f32::from(open)];
        if self.reduced_motion {
            p.slide = Transition::new(end, now);
        } else {
            p.slide.set(end, now, SLIDE, EASE);
        }
        self.dirty = true;
    }

    fn amount(&self, side: Side, now: Instant) -> f32 {
        self.sides[side.index()].slide.sample(now, SLIDE, EASE)[0]
    }

    fn panel_at(&self, area: Area, p: Pos2, now: Instant) -> Option<Side> {
        Side::ALL.into_iter().rev().find(|&side| {
            self.amount(side, now) > 0.
                && area.stage.contains(p)
                && area.panel(side, self.amount(side, now)).contains(p)
        })
    }

    fn clock(&self, area: Area, now: Instant) -> Option<Rect> {
        let floating = area.clock(&self.settings);
        let Some(side) = self.settings.docked_side else {
            return Some(floating);
        };
        let amount = self.obscured_dock_amount.unwrap_or_else(|| self.amount(side, now));
        if amount <= 0. {
            return Some(floating);
        }
        Some({
            let panel = area.panel(side, 1.);
            let size = self.settings.style.size();
            let scale = 1.0f32
                .min((panel.width() - 24.) / size.x)
                .min((panel.height() - 24.) / size.y)
                .max(0.01);
            let size = size * scale;
            let docked = Rect::from_min_size(
                pos2(panel.center().x - size.x * 0.5, panel.top() + 12.),
                size,
            );
            Rect::from_min_max(
                floating.min.lerp(docked.min, amount),
                floating.max.lerp(docked.max, amount),
            )
        })
    }

    fn content(&self, area: Area, side: Side, now: Instant) -> Rect {
        let mut bounds = area.panel(side, self.amount(side, now)).shrink(12.);
        if self.settings.docked_side == Some(side)
            && let Some(clock) = self.clock(area, now)
        {
            bounds.min.y = clock.bottom() + 12.;
        }
        bounds
    }

    fn read_hit(&self, area: Area, p: Pos2, now: Instant, home: &Home) -> Option<Hit> {
        if let Some(menu) = self.menu {
            let bounds = menu_rect(area, menu);
            if bounds.contains(p) {
                return menu_choices(bounds)
                    .find(|(r, _, _)| r.contains(p))
                    .map(|(_, hit, _)| hit)
                    .or(Some(Hit::Panel));
            }
        }
        let panel = self.panel_at(area, p, now);
        if (panel.is_none() || panel == self.settings.docked_side)
            && area.stage.contains(p)
            && let Some(r) = self.clock(area, now)
        {
            if self.settings.docked_side.is_none() && handle(r).contains(p) {
                return Some(Hit::Resize);
            }
            if r.contains(p) {
                return Some(
                    if self.settings.style == Style::Pixel && format_button(r).contains(p) {
                        Hit::Format
                    } else {
                        Hit::Clock
                    },
                );
            }
        }
        let side = panel?;
        let r = self.content(area, side, now);
        if !r.contains(p) {
            return Some(Hit::Panel);
        }
        if side == Side::Left {
            if !self.recent.is_empty() && self.clear_activity(r).contains(p) {
                return Some(Hit::ClearActivity);
            }
            return self
                .collection
                .hit(self.collection_bounds(r), p, home)
                .map(Hit::Collection)
                .or(Some(Hit::Panel));
        } else {
            for (i, _) in shortcuts().iter().enumerate() {
                if shortcut_rect(r, i).contains(p) {
                    return Some(match i {
                        0 if home.collection().len() > 0 => Hit::Folder,
                        0 => Hit::Panel,
                        _ => Hit::Shortcut(i),
                    });
                }
            }
            if news_refresh(news_bounds(r)).contains(p) {
                return Some(Hit::RefreshNews);
            }
        }
        Some(Hit::Panel)
    }

    pub fn moved(
        &mut self,
        window: &Window,
        p: PhysicalPosition<f64>,
        now: Instant,
        home: &Home,
    ) -> Option<CursorIcon> {
        let area = Area::from_window(window);
        let p = area.pointer(p);
        let previous = self.cursor.replace(p);
        let target = area.edge(p).or_else(|| self.panel_at(area, p, now));
        for side in Side::ALL {
            if target == Some(side) {
                self.set_panel(side, true, now);
            } else if !(side == Side::Left && self.collection.editing())
                && self.sides[side.index()].open
                && self.sides[side.index()].hide_at.is_none()
            {
                self.sides[side.index()].hide_at = Some(now + HIDE);
            }
        }
        match self.drag {
            Some(Drag::Move { mut offset, start }) if (p - start).length() >= 3. => {
                if self.settings.docked_side.is_some() {
                    if let Some(r) = self.clock(area, now) {
                        let fraction = offset / r.size();
                        offset = fraction * area.clock(&self.settings).size();
                    }
                    self.settings.docked_side = None;
                    self.drag = Some(Drag::Move { offset, start });
                }
                let size = area.clock(&self.settings).size();
                self.store_rect(area, area.fit_clock(p - offset, size));
                self.dirty = true;
                return Some(CursorIcon::Grabbing);
            }
            Some(Drag::Move { .. }) => return Some(CursorIcon::Grab),
            Some(Drag::Resize { start, scale }) => {
                let base = self.settings.style.size();
                let delta = (p - start).dot(base) / base.length_sq();
                self.set_scale(area, scale + delta);
                return Some(CursorIcon::NwseResize);
            }
            None => {}
        }
        let hit = self.read_hit(area, p, now, home);
        self.dirty |= hit != previous.and_then(|p| self.read_hit(area, p, now, home));
        hit.map(|hit| match hit {
            Hit::Panel => CursorIcon::Default,
            Hit::Clock => CursorIcon::Grab,
            Hit::Resize => CursorIcon::NwseResize,
            Hit::Collection(collection::Hit::SearchField) => CursorIcon::Text,
            Hit::Collection(collection::Hit::Surface) => CursorIcon::Default,
            _ => CursorIcon::Pointer,
        })
    }

    pub fn leave(&mut self, now: Instant, focus_lost: bool) {
        self.cursor = None;
        self.dirty = true;
        if focus_lost {
            self.close(now);
        } else {
            for side in Side::ALL {
                let panel = &mut self.sides[side.index()];
                if panel.open && !(side == Side::Left && self.collection.editing()) {
                    panel.hide_at.get_or_insert(now + HIDE);
                }
            }
        }
    }

    pub fn close(&mut self, now: Instant) -> bool {
        let open = self.menu.is_some() || self.drag.is_some() || self.sides.iter().any(|p| p.open);
        self.menu = None;
        if self.drag.take().is_some() {
            self.save();
        }
        self.pressed = None;
        for side in Side::ALL {
            self.set_panel(side, false, now);
        }
        self.dirty |= open;
        open
    }

    pub fn button(
        &mut self,
        window: &Window,
        pointer: PhysicalPosition<f64>,
        now: Instant,
        pressed: bool,
        right: bool,
        home: &Home,
    ) -> (bool, Option<Action>) {
        let area = Area::from_window(window);
        let p = area.pointer(pointer);
        self.cursor = Some(p);
        let hit = self.read_hit(area, p, now, home);
        if right {
            if pressed {
                match hit {
                    Some(Hit::Clock | Hit::Resize | Hit::Format) => {
                        self.menu = Some(p);
                    }
                    Some(Hit::Collection(collection::Hit::Slot(index))) => {
                        self.collection.open_context(index, p);
                        self.dirty = true;
                        return (true, Some(Action::Instance(index)));
                    }
                    Some(Hit::Collection(collection::Hit::Selected)) => {
                        return (true, Some(Action::InstanceFolder(home.selected())));
                    }
                    _ => return (false, None),
                }
                self.dirty = true;
                return (true, None);
            }
            return (false, None);
        }
        if pressed {
            let inside_menu = self
                .menu
                .is_some_and(|point| menu_rect(area, point).contains(p));
            if !inside_menu {
                self.dirty |= self.menu.take().is_some();
                let hover_panel = self.panel_at(area, p, now);
                if hover_panel == Some(Side::Left) && !matches!(hit, Some(Hit::Collection(_))) {
                    self.collection.dismiss();
                }
                for side in Side::ALL {
                    if hover_panel != Some(side) {
                        self.set_panel(side, false, now);
                    }
                }
            }
            self.pressed = hit;
            match (hit, self.clock(area, now)) {
                (Some(Hit::Clock), Some(r)) => {
                    self.drag = Some(Drag::Move {
                        start: p,
                        offset: p - r.min,
                    })
                }
                (Some(Hit::Resize), Some(r)) => {
                    let scale = r.width() / self.settings.style.size().x;
                    self.drag = Some(Drag::Resize { start: p, scale });
                }
                _ => {}
            }
            self.dirty |= hit.is_some();
            return (hit.is_some(), None);
        }
        let down = self.pressed.take();
        if let Some(drag) = self.drag.take() {
            if let Drag::Move { start, .. } = drag
                && (p - start).length() >= 3.
                && let Some(side) = area.edge(p).or_else(|| self.panel_at(area, p, now))
            {
                self.settings.docked_side = Some(side);
                self.set_panel(side, true, now);
            }
            self.dirty = true;
            self.save();
            return (true, None);
        }
        if down.is_none() {
            return (false, None);
        }
        let action = match down.filter(|_| hit == down) {
            Some(Hit::Format) => {
                self.settings.clock_24h = !self.settings.clock_24h;
                self.menu = None;
                self.save();
                None
            }
            Some(Hit::Style(style)) => {
                self.settings.style = style;
                self.next_clock = now;
                self.menu = None;
                self.save();
                None
            }
            Some(Hit::Reset) => {
                self.settings.docked_side = None;
                self.set_scale(area, 1.);
                self.menu = None;
                self.save();
                None
            }
            Some(Hit::Collection(hit)) => {
                self.collection.activate(hit, home);
                match hit {
                    collection::Hit::Slot(index) => Some(Action::Instance(index)),
                    collection::Hit::Selected => Some(Action::InstanceFolder(home.selected())),
                    collection::Hit::Create => Some(Action::Create),
                    collection::Hit::Folder(index) => Some(Action::InstanceFolder(index)),
                    _ => None,
                }
            }
            Some(Hit::ClearActivity) => {
                self.recent.clear();
                None
            }
            Some(Hit::RefreshNews) => {
                self.news_scroll = 0.;
                None
            }
            Some(Hit::Folder) => Some(Action::Folder),
            Some(Hit::Shortcut(index)) => Some(Action::Shortcut(index)),
            _ => None,
        };
        self.dirty = true;
        (true, action)
    }

    pub fn wheel(
        &mut self,
        window: &Window,
        pointer: PhysicalPosition<f64>,
        now: Instant,
        ctrl: bool,
        delta: f32,
        home: &Home,
    ) -> bool {
        if delta == 0. {
            return false;
        }
        let area = Area::from_window(window);
        let p = area.pointer(pointer);
        if ctrl
            && self.settings.docked_side.is_none()
            && matches!(
                self.read_hit(area, p, now, home),
                Some(Hit::Clock | Hit::Resize | Hit::Format)
            )
        {
            let current = area.clock(&self.settings).width() / self.settings.style.size().x;
            self.set_scale(area, current + delta.clamp(-4., 4.) * 0.08);
            self.save();
            return true;
        }
        if self.panel_at(area, p, now) == Some(Side::Left) {
            let r = self.collection_bounds(self.content(area, Side::Left, now));
            self.dirty |= self.collection.scroll(r, p, delta, home);
            return true;
        }
        if self.panel_at(area, p, now) == Some(Side::Right) {
            let news = news_bounds(self.content(area, Side::Right, now));
            if news.contains(p) {
                self.news_scroll =
                    (self.news_scroll - delta * 38.).clamp(0., news_max_scroll(news));
                self.dirty = true;
                return true;
            }
        }
        false
    }

    fn store_rect(&mut self, area: Area, r: Rect) {
        self.settings.left = r.left();
        self.settings.top = r.top() - area.stage.top();
        self.settings.width = r.width();
        self.settings.height = r.height();
        self.settings.viewport_width = area.stage.width();
        self.settings.viewport_height = area.height;
    }

    fn set_scale(&mut self, area: Area, value: f32) {
        let anchor = area.clock(&self.settings).min;
        self.settings.scale = value.clamp(0.5, 2.5);
        let size = area.clock(&self.settings).size();
        self.store_rect(area, area.fit_clock(anchor, size));
        self.dirty = true;
    }

    pub fn transient_ui(&self, now: Instant) -> bool {
        self.menu.is_some()
            || self.drag.is_some()
            || self
                .sides
                .iter()
                .any(|p| p.open || p.slide.running(now, SLIDE))
    }

    pub fn tick(&mut self, show_clock: bool, show_panels: bool, now: Instant) -> bool {
        if self.show_panels != show_panels {
            self.show_panels = show_panels;
            self.dirty = true;
            if !show_panels {
                self.obscured_dock_amount = self.settings.docked_side
                    .filter(|_| show_clock)
                    .map(|side| self.amount(side, now));
                self.close(now);
                for panel in &mut self.sides {
                    panel.slide = Transition::new([0.], now);
                }
                self.cursor = None;
            } else {
                self.obscured_dock_amount = None;
            }
        }
        if self.show_clock != show_clock {
            self.show_clock = show_clock;
            self.dirty = true;
            if !show_clock {
                self.obscured_dock_amount = None;
            }
        }
        if !show_clock {
            self.next_slide = None;
            return std::mem::take(&mut self.dirty);
        }
        if now >= self.next_clock {
            self.time = unsafe { GetLocalTime() };
            let ms = u64::from(self.time.wMilliseconds);
            let next = if self.settings.style.shows_seconds() {
                1000 - ms
            } else {
                60_000 - (u64::from(self.time.wSecond) * 1000 + ms)
            };
            self.next_clock = now + Duration::from_millis(next);
            self.dirty = true;
        }
        if !show_panels {
            self.next_slide = None;
            return std::mem::take(&mut self.dirty);
        }
        for side in Side::ALL {
            if self.drag.is_none()
                && !(side == Side::Left && self.collection.editing())
                && self.sides[side.index()].hide_at.is_some_and(|at| at <= now)
            {
                self.set_panel(side, false, now);
            }
        }
        let animating = self.sides.iter().any(|p| p.slide.running(now, SLIDE))
            || (!self.reduced_motion && self.sides[Side::Left.index()].open);
        self.next_slide = animating.then_some(now + crate::COMPOSITOR_FRAME_INTERVAL);
        // Paint one final frame after the transition has stopped too.
        let was_dirty = std::mem::replace(&mut self.dirty, animating);
        was_dirty || animating
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        if !self.show_clock {
            return None;
        }
        [
            Some(self.next_clock.max(now)),
            self.next_slide,
            (self.drag.is_none())
                .then(|| {
                    Side::ALL
                        .into_iter()
                        .filter(|&side| !(side == Side::Left && self.collection.editing()))
                        .filter_map(|side| self.sides[side.index()].hide_at)
                        .min()
                })
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn activity_height(&self, r: Rect) -> f32 {
        (104. + self.recent.len().max(1) as f32 * 61.).min((r.height() - 96.).max(66.))
    }

    fn clear_activity(&self, r: Rect) -> Rect {
        rect(
            r.right() - 102.,
            r.top() + self.activity_height(r) - 34.,
            86.,
            24.,
        )
    }

    fn collection_bounds(&self, r: Rect) -> Rect {
        let top = r.top() + self.activity_height(r) + 12.;
        rect(r.left(), top, r.width(), (r.bottom() - top).clamp(0., 468.))
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

fn shortcut_rect(r: Rect, index: usize) -> Rect {
    rect(
        r.left() + 12.,
        r.top() + 40. + index as f32 * 57.,
        r.width() - 24.,
        50.,
    )
}

fn shortcuts() -> [&'static str; 4] {
    [
        "Abrir carpeta del juego",
        "Administrar mods",
        "Configuración de la instancia",
        "Copiar instancia",
    ]
}

const NEWS_CONTENT_HEIGHT: f32 = 4. * 116. - 12.;

fn news_bounds(r: Rect) -> Rect {
    let start = r.min + vec2(0., 277.);
    Rect::from_min_max(
        start,
        pos2(
            r.right(),
            r.bottom().min(start.y + NEWS_CONTENT_HEIGHT + 94.),
        ),
    )
}

fn news_refresh(r: Rect) -> Rect {
    if r.height() < 76. {
        return Rect::NOTHING;
    }
    rect(r.left() + 16., r.bottom() - 41., r.width() - 32., 30.)
}

fn news_max_scroll(r: Rect) -> f32 {
    (NEWS_CONTENT_HEIGHT - (r.height() - 94.).max(0.)).max(0.)
}

fn handle(r: Rect) -> Rect {
    Rect::from_center_size(r.right_bottom() - vec2(7., 7.), vec2(22., 22.))
}

fn format_button(r: Rect) -> Rect {
    let s = r.width() / 284.;
    rect(r.right() - 75. * s, r.top() + 9. * s, 62. * s, 18. * s)
}

fn menu_rect(area: Area, p: Pos2) -> Rect {
    area.fit_clock(p, vec2(245., 182.))
}

fn menu_choices(r: Rect) -> impl Iterator<Item = (Rect, Hit, &'static str)> {
    FORMATS
        .into_iter()
        .map(|(style, name)| (Hit::Style(style), name))
        .chain([
            (Hit::Format, "Cambiar 12 / 24 h"),
            (Hit::Reset, "Tamaño original"),
        ])
        .enumerate()
        .map(move |(i, (hit, name))| {
            (
                rect(
                    r.left() + 5. + (i % 2) as f32 * 118.,
                    r.top() + 7. + (i / 2) as f32 * 34.,
                    115.,
                    30.,
                ),
                hit,
                name,
            )
        })
}

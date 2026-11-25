use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use winit::{dpi::PhysicalPosition, window::Window};

#[path = "dock/wall_glyphs.rs"]
#[rustfmt::skip]
mod glyphs;

use crate::{
    crt::Tube,
    gpu::Params,
    titlebar::{ICON_EASE, Transition},
};

pub const ROWS: usize = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PosterKey([u8; 24]);

impl PosterKey {
    pub const EMPTY: Self = Self([0; 24]);

    fn new(id: &str) -> Self {
        match id.as_bytes().try_into() {
            Ok(bytes) if id.bytes().all(|b| b.is_ascii_hexdigit()) => Self(bytes),
            _ => Self::EMPTY,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        (self.0[0] != 0).then(|| std::str::from_utf8(&self.0).unwrap())
    }
}

pub const ACTIVE_LINE: usize = ROWS;
pub const SEARCH_LINE: usize = ROWS + 1;
pub const STATUS_LINE: usize = ROWS + 2;
pub const SORT_LINE: usize = ROWS + 3;
pub const DETAIL_LINE: usize = ROWS + 4;
const DETAIL_COUNT: usize = 8;
const SECOND_TITLE: usize = DETAIL_LINE + DETAIL_COUNT + 1;
const AUDIO_LINE: usize = SECOND_TITLE + ROWS;
pub const LINES: usize = AUDIO_LINE + 1;
const WIDTH: f32 = 1260.0;
const TOP: f32 = 179.0;
const CARD_W: f32 = 232.0;
const CARD_H: f32 = 160.0;
const X_STEP: f32 = 245.0;
const Y_STEP: f32 = 173.0;
const SCROLL_X: std::ops::Range<f32> = 1234.0..1258.0;
const SOURCE_FILTERS: [(f32, f32); 4] = [
    (20.0, 100.0),
    (128.0, 226.0),
    (362.0, 136.0),
    (506.0, 133.0),
];
const TAB_X: [f32; 5] = [20.0, 221.0, 401.0, 525.0, 714.0];
const INTERVALS: [u64; 7] = [15, 30, 60, 180, 300, 600, 1800];
const ONLINE_ITEM: usize = 1_000_000;
const STEAM_TAGS: [&str; 10] = [
    "",
    "minecraft",
    "Anime",
    "Landscape",
    "Pixel art",
    "Cyberpunk",
    "Relaxing",
    "Fantasy",
    "Sci-Fi",
    "Game",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    Outside,
    Inside,
    Close,
    Settings,
    Audio,
    Dim,
    Tab(usize),
    Search,
    Online,
    Local,
    Submit,
    Filter(usize),
    Sort,
    Page(bool),
    Import,
    Link,
    Folder,
    Refresh,
    Scrollbar(bool),
    Details(usize),
    CloseDetails,
    Property(usize, bool),
    PropertyPage(bool),
    ApplyProperties,
    ResetProperties,
    Apply(usize),
    Remove(usize),
    Power,
    Interval,
    AddCurrent,
    Clear,
}

pub enum Action {
    None,
    Apply(usize),
    Open(String),
    Audio,
}

struct Details {
    index: usize,
    serial: Option<u64>,
    page: usize,
    rows: Vec<Value>,
}

fn value_label(row: &Value) -> String {
    let val = &row["value"];
    match field(row, "type") {
        "bool" => if val == true { "Sí" } else { "No" }.to_string(),
        "slider" => val
            .as_f64()
            .map(|num| {
                let s = format!("{num:.2}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            })
            .unwrap_or_default(),
        "combo" => row["options"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|item| item["value"] == *val)
            .map(|opt| field(opt, "label"))
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

struct Item {
    name: Box<str>,
    scene: bool,
    kind: Box<str>,
    source: Box<str>,
    id: Box<str>,
    poster: PosterKey,
    tags: Box<str>,
    url: Box<str>,
    resolution: Box<str>,
    size: u32,
    can_play: bool,
    animated: bool,
    in_list: bool,
}

fn field<'a>(row: &'a Value, name: &str) -> &'a str {
    row[name].as_str().unwrap_or_default()
}

impl Item {
    fn new(row: &Value) -> Self {
        let kind = field(row, "kind");
        Self {
            name: field(row, "name").into(),
            scene: kind == "scene" && row["playable"].as_bool().unwrap_or(true),
            kind: kind.into(),
            source: field(row, "source").into(),
            id: field(row, "id").into(),
            poster: PosterKey::new(field(row, "poster")),
            tags: row["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                .into(),
            url: field(row, "url").into(),
            resolution: field(row, "resolution").into(),
            // Store tenths of a MiB; zero means the catalog has no size to show.
            size: row["size_bytes"].as_u64().map_or(0, |bytes| {
                ((bytes as f64 * (10.0 / 1048576.0)).round() as u32).saturating_add(1)
            }),
            can_play: matches!(kind, "scene" | "video" | "image")
                && row["playable"].as_bool().unwrap_or(true),
            animated: row["animated"].as_bool().unwrap_or(matches!(kind, "scene" | "video")),
            in_list: false,
        }
    }

    fn origin(&self) -> u32 {
        match &*self.source {
            "steam" => 1,
            "motion" => 2,
            "imported" => 3,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewSource {
    Local(usize),
    Online(PosterKey),
}

const HOVER_SECONDS: f32 = 0.19;

#[derive(Default)]
struct CardPreview {
    wanted: Option<(usize, PreviewSource)>,
    requested: Option<u64>,
    ready: bool,
    dirty: bool,
    serial: u64,
    since: Option<Instant>,
}

impl Hit {
    fn card(self) -> Option<usize> {
        match self {
            Self::Apply(i) | Self::Details(i) | Self::Remove(i) => Some(i),
            _ => None,
        }
    }
}

pub struct Wallpapers {
    items: Vec<Item>,
    remote: Vec<Item>,
    folder: Option<PathBuf>,
    cache: Option<PathBuf>,
    prefs: Option<PathBuf>,
    serial: u64,
    library_job: Option<u64>,
    online_job: Option<u64>,
    action_job: Option<u64>,
    details: Option<Details>,
    next_poll: Instant,
    next_poster_poll: Instant,
    poster_keys: [PosterKey; ROWS],
    poster_request_dirty: bool,
    status: Box<str>,
    pending_apply: Option<usize>,
    force_apply: bool,
    steam_online: bool,
    filter: usize,
    sort: usize,
    page: usize,
    more: bool,
    next_tag: Box<str>,
    submitted: String,
    link: Option<String>,
    tab: usize,
    underline: Transition<2>,
    tube: Option<Tube>,
    reduced: bool,
    search: String,
    editing: bool,
    settings: bool,
    scroll: f32,
    scroll_grab: Option<f32>,
    pub dim: f32,
    interval: usize,
    rotate_at: Option<Instant>,
    card_preview: CardPreview,
    card_zoom: Vec<(usize, Transition<1>)>,
}

impl Wallpapers {
    pub fn new(now: Instant) -> Self {
        Self {
            items: Vec::new(),
            remote: Vec::new(),
            folder: None,
            cache: None,
            prefs: None,
            serial: 0,
            library_job: None,
            online_job: None,
            action_job: None,
            details: None,
            next_poll: now,
            next_poster_poll: now,
            poster_keys: [PosterKey::EMPTY; ROWS],
            poster_request_dirty: false,
            status: "".into(),
            pending_apply: None,
            force_apply: false,
            steam_online: false,
            filter: 0,
            sort: 0,
            page: 1,
            more: false,
            next_tag: "".into(),
            submitted: String::new(),
            link: None,
            tab: 0,
            underline: Transition::new([TAB_X[0], TAB_X[1] - TAB_X[0] - 8.0], now),
            tube: None,
            reduced: crate::titlebar::reduced_motion(),
            search: String::new(),
            editing: false,
            settings: false,
            scroll: 0.0,
            scroll_grab: None,
            dim: 0.0,
            interval: 2,
            rotate_at: None,
            card_preview: CardPreview::default(),
            card_zoom: Vec::new(),
        }
    }

    pub fn open(&mut self, folder: Option<&Path>, active: &str, now: Instant) -> crate::Result<()> {
        if self.folder.is_none() {
            self.folder = folder.map(Path::to_owned);
            self.cache = std::env::var_os("IKA_WALLPAPER_CACHE")
                .map(|dir| PathBuf::from(dir).join("posters-hd"));
            self.prefs = std::env::var_os("IKA_WALLPAPER_PREFS").map(PathBuf::from);
            self.reload_library();
            if let Some(prefs) = self.prefs.as_deref().and_then(Self::read_json) {
                self.interval =
                    (prefs["interval"].as_u64().unwrap_or(2) as usize).min(INTERVALS.len() - 1);
                if let Some(ids) = prefs["list"].as_array() {
                    for item in &mut self.items {
                        item.in_list =
                            item.can_play && ids.iter().any(|id| id.as_str() == Some(&item.id));
                    }
                }
                if prefs["enabled"].as_bool() == Some(true) && self.items.iter().any(|i| i.in_list)
                {
                    self.rotate_at = Some(now + self.interval());
                }
            }
            if self.items.is_empty() {
                self.items.push(Item::new(&json!({"name": active})));
            }
        } else {
            self.reload_library();
        }
        if self.online() && self.remote.is_empty() && self.online_job.is_none() {
            self.query();
        }
        self.tube = Some(Tube::new(self.reduced));
        Ok(())
    }

    fn read_json(path: &Path) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    fn reload_library(&mut self) {
        let Some(data) = self
            .folder
            .as_ref()
            .and_then(|f| Self::read_json(&f.join("library.json")))
        else {
            return;
        };
        let Some(rows) = data.as_array() else { return };
        let old = std::mem::take(&mut self.items);
        self.items = rows.iter().map(Item::new).collect();
        // The bridge only appends entries and keeps existing indices stable.
        for (item, previous) in self.items.iter_mut().zip(old) {
            item.in_list = previous.in_list && item.can_play;
        }
    }

    fn save_prefs(&self) {
        let Some(path) = &self.prefs else { return };
        let list = self
            .items
            .iter()
            .filter(|i| i.in_list)
            .map(|i| &*i.id)
            .collect::<Vec<_>>();
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        let data =
            json!({"interval": self.interval, "enabled": self.rotate_at.is_some(), "list": list});
        if std::fs::write(&tmp, data.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    fn request(&mut self, channel: &str, mut data: Value) -> Option<u64> {
        let folder = self.folder.as_ref()?;
        self.serial += 1;
        data["serial"] = json!(self.serial);
        let tmp = folder.join(format!("{channel}.tmp"));
        if std::fs::write(&tmp, data.to_string())
            .and_then(|_| std::fs::rename(tmp, folder.join(format!("{channel}.request"))))
            .is_ok()
        {
            Some(self.serial)
        } else {
            self.status = "No se pudo contactar al puente".into();
            None
        }
    }

    fn online(&self) -> bool {
        self.tab == 2 || (self.tab == 1 && self.steam_online)
    }

    fn query(&mut self) {
        self.scroll = 0.0;
        self.more = false;
        self.remote.clear();
        if self.folder.is_none() {
            self.status = "Abre el launcher desde bridge/run.py".into();
            return;
        }
        self.status = "Cargando catálogo…".into();
        self.online_job = self.request(
            "online",
            json!({
                "source": if self.tab == 2 { "motion" } else { "steam" },
                "q": self.submitted, "tag": self.filter, "page": self.page, "sort": self.sort,
                "next_tag": &*self.next_tag
            }),
        );
    }

    pub fn submit(&mut self) {
        self.editing = false;
        if let Some(url) = self.link.take() {
            if !url.is_empty() && self.action_job.is_none() {
                self.status = "Descargando enlace…".into();
                self.action_job = self.request("action", json!({"kind": "url", "url": url.trim()}));
            }
        } else if self.online() {
            self.submitted.clone_from(&self.search);
            self.page = 1;
            self.next_tag = "".into();
            self.query();
        }
    }

    pub fn poll(&mut self, now: Instant, idle: bool) -> Option<(usize, bool)> {
        if now < self.next_poll
            || (!self.visible() && self.action_job.is_none() && self.pending_apply.is_none())
        {
            return None;
        }
        self.next_poll = now + Duration::from_millis(120);
        let mut reload = false;
        if let Some(folder) = &self.folder {
            if let Some(details) = &mut self.details
                && let Some(id) = details.serial
                && let Some(data) = Self::read_json(&folder.join("properties.json"))
                && data["serial"].as_u64() == Some(id)
            {
                details.serial = None;
                self.status = field(&data, "error").into();
                if let Some(rows) = data["rows"].as_array() {
                    details.rows.clone_from(rows);
                    if data["apply"].as_bool() == Some(true) {
                        self.pending_apply = Some(details.index);
                        self.force_apply = true;
                    }
                }
            }
            if let Some(id) = self.online_job
                && let Some(data) = Self::read_json(&folder.join("online.json"))
                && data["serial"].as_u64() == Some(id)
            {
                self.online_job = None;
                self.more = data["more"].as_bool().unwrap_or(false);
                if let tag = field(&data, "next_tag")
                    && !tag.is_empty()
                {
                    self.next_tag = tag.into();
                }
                self.status = field(&data, "error").into();
                self.remote = data["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(Item::new)
                    .collect();
            }
            if let Some(id) = self.action_job
                && let Some(data) = Self::read_json(&folder.join("action.json"))
                && data["serial"].as_u64() == Some(id)
            {
                self.action_job = None;
                self.status = field(&data, "error").into();
                if let Some(index) = data["index"].as_u64() {
                    self.pending_apply = Some(index as usize);
                    self.force_apply = false;
                    reload = true;
                }
            }
            if let Some(id) = self.library_job
                && let Some(data) = Self::read_json(&folder.join("library-ready.json"))
                && data["serial"].as_u64() == Some(id)
            {
                self.library_job = None;
                reload = true;
            }
        }
        if reload {
            self.reload_library();
        }
        if idle {
            self.pending_apply
                .take()
                .map(|index| (index, std::mem::take(&mut self.force_apply)))
        } else {
            None
        }
    }

    pub fn visible(&self) -> bool {
        self.tube.is_some()
    }
    pub fn animating(&self, now: Instant) -> bool {
        self.tube.as_ref().is_some_and(|tube| {
            tube.animating(now, 0.90) || (!self.reduced && (self.underline.running(now, 0.32)
                || self.card_zoom.iter().any(|(_, zoom)| zoom.running(now, HOVER_SECONDS))))
        })
    }

    pub fn presented(&mut self, now: Instant) {
        if let Some(tube) = &mut self.tube {
            tube.presented(now);
        }
    }

    pub fn sync(&mut self, now: Instant) -> bool {
        if self.tube.as_ref().is_some_and(|tube| tube.finished(now)) {
            self.tube = None;
            self.remote = Vec::new();
            self.online_job = None;
            self.poster_keys.fill(PosterKey::EMPTY);
            self.poster_request_dirty = false;
            self.card_zoom.clear();
            return true;
        }
        false
    }

    pub fn close(&mut self, now: Instant) {
        self.end_scroll();
        if let Some(tube) = &mut self.tube {
            tube.close(now);
        }
        self.editing = false;
        self.settings = false;
        self.link = None;
        self.details = None;
    }

    fn rect(window: &Window) -> [f32; 4] {
        let s = window.inner_size();
        let (w, h) = (s.width as f32, s.height as f32);
        let limit = h - crate::dock::reserved_height(window);
        let scale = (window.scale_factor() as f32)
            .min(w / 1326.0)
            .min(limit / 540.0)
            .max(f32::EPSILON);
        let height = (h * 0.92 / scale).min(900.0).min(limit / scale);
        [
            (w - WIDTH * scale) * 0.5,
            ((h - height * scale) * 0.5).min(limit - height * scale),
            scale,
            height,
        ]
    }

    fn filtered<'a>(&'a self, query: &'a str) -> impl Iterator<Item = (usize, &'a Item)> {
        let online = self.online();
        let items = if online { &self.remote } else { &self.items };
        items
            .iter()
            .enumerate()
            .map(move |(n, i)| (n + if online { ONLINE_ITEM } else { 0 }, i))
            .filter(move |(_, i)| {
                if online {
                    true
                } else {
                    match self.tab {
                        0 | 1 => {
                            let source = ["", "steam", "motion", "imported"][self.filter.min(3)];
                            (if self.tab == 0 {
                                source.is_empty() || &*i.source == source
                            } else {
                                &*i.source == "steam"
                                    && (self.filter == 0
                                        || i.tags.contains(
                                            &STEAM_TAGS[self.filter].to_ascii_lowercase(),
                                        ))
                            }) && (query.is_empty()
                                || i.name.to_lowercase().contains(query)
                                || i.tags.contains(query))
                        }
                        3 => i.in_list,
                        _ => false,
                    }
                }
            })
    }

    fn top(&self) -> f32 {
        if self.tab == 3 { TOP } else { 206.0 }
    }

    fn visible_items<'a>(
        &'a self,
        query: &'a str,
        height: f32,
    ) -> impl Iterator<Item = (usize, &'a Item)> {
        let offset = self.offset(height, self.filtered(query).count());
        let skipped = (offset / Y_STEP) as usize * 5;
        // Same slots as WGSL, including the partial rows at either clip edge.
        let end = (((height - 40.0 - self.top() + offset % Y_STEP).max(0.0) / Y_STEP).ceil()
            as usize)
            * 5;
        self.filtered(query).skip(skipped).take(end.min(ROWS))
    }

    pub fn posters(
        &mut self,
        window: &Window,
        now: Instant,
    ) -> Option<(&Path, &[PosterKey; ROWS])> {
        let tube = self.tube.as_ref()?;
        if tube.closing() || self.cache.is_none() {
            return None;
        }
        let awaiting_present = tube.awaiting_present();
        let mut keys = [PosterKey::EMPTY; ROWS];
        for (key, (_, item)) in keys
            .iter_mut()
            .zip(self.visible_items(&self.search.to_lowercase(), Self::rect(window)[3]))
        {
            *key = item.poster;
        }
        let changed = keys != self.poster_keys;
        if changed {
            self.poster_keys = keys;
            self.poster_request_dirty = true;
        }
        let poll = now >= self.next_poster_poll;
        if poll {
            self.next_poster_poll = now + Duration::from_millis(80);
            if self.poster_request_dirty {
                // Coalesce requests to the producer while the thumb is moving.
                let ids: Vec<_> = keys.iter().filter_map(PosterKey::as_str).collect();
                self.request("thumbs", json!({"posters": ids}));
                self.poster_request_dirty = false;
            }
        }
        // On scroll, start filling newly visible cards without waiting for the producer poll.
        // Send requests immediately, but keep disk reads off the first CRT frame.
        (!awaiting_present && (changed || poll))
            .then(|| (self.cache.as_deref().unwrap(), &self.poster_keys))
    }

    fn max_scroll(&self, height: f32, count: usize) -> f32 {
        (count.div_ceil(5) as f32 * Y_STEP - (height - 40.0 - self.top())).max(0.0)
    }

    fn offset(&self, height: f32, count: usize) -> f32 {
        self.scroll.clamp(0.0, self.max_scroll(height, count))
    }

    fn scroll_geometry(&self, height: f32, count: usize) -> Option<[f32; 4]> {
        let max = self.max_scroll(height, count);
        if max <= 0.0 {
            return None;
        }
        let start = self.top() + 10.0;
        let end = height - 50.0;
        let track = end - start;
        let viewport = height - 40.0 - self.top();
        let thumb = (track * viewport / (viewport + max)).clamp(28.0_f32.min(track), track);
        let y = start + (track - thumb) * (self.offset(height, count) / max);
        Some([start, end, y, thumb])
    }

    pub fn start_scroll(&mut self, window: &Window, y: f64) {
        let [_, t, s, height] = Self::rect(window);
        let y = (y as f32 - t) / s;
        let count = self.filtered(&self.search.to_lowercase()).count();
        if let Some([_, _, thumb, length]) = self.scroll_geometry(height, count) {
            if (thumb..thumb + length).contains(&y) {
                self.scroll_grab = Some(y - thumb);
            } else {
                let page = height - 40.0 - self.top() - Y_STEP;
                let step = page.max(Y_STEP) * if y < thumb { -1.0 } else { 1.0 };
                self.scroll =
                    (self.offset(height, count) + step).clamp(0.0, self.max_scroll(height, count));
            }
        }
    }

    pub fn drag_scroll(&mut self, window: &Window, y: f64) -> bool {
        let Some(grab) = self.scroll_grab else {
            return false;
        };
        let [_, t, s, height] = Self::rect(window);
        let count = self.filtered(&self.search.to_lowercase()).count();
        if let Some([start, end, _, length]) = self.scroll_geometry(height, count) {
            let travel = end - start - length;
            if travel > 0.0 {
                let fraction = (((y as f32 - t) / s - start - grab) / travel).clamp(0.0, 1.0);
                self.scroll = fraction * self.max_scroll(height, count);
            }
        }
        true
    }

    pub fn end_scroll(&mut self) {
        self.scroll_grab = None;
    }

    pub fn dragging_scroll(&self) -> bool {
        self.scroll_grab.is_some()
    }

    pub fn scroll(&mut self, window: &Window, delta: f32) {
        if self.dragging_scroll() {
            return;
        }
        let count = self.detail_count(Self::rect(window)[3]);
        if let Some(detail) = &mut self.details {
            if delta > 0.0 {
                detail.page = (detail.page + 1).min(detail.rows.len().saturating_sub(1) / count);
            } else if delta < 0.0 {
                detail.page = detail.page.saturating_sub(1);
            }
            return;
        }
        let query = self.search.to_lowercase();
        let [_, _, scale, height] = Self::rect(window);
        let count = self.filtered(&query).count();
        self.scroll = self.offset(height, count) + delta / scale;
        self.scroll = self.offset(height, count);
    }

    pub fn hit(
        &self,
        window: &Window,
        pos: PhysicalPosition<f64>,
        now: Instant,
        audio: bool,
    ) -> Hit {
        if self
            .tube
            .as_ref()
            .is_some_and(|tube| tube.closing() || tube.opening_before(now, 0.42))
        {
            return Hit::Inside;
        }
        let [l, t, s, h] = Self::rect(window);
        let x = (pos.x as f32 - l) / s;
        let y = (pos.y as f32 - t) / s;
        if !(0.0..WIDTH).contains(&x) || !(0.0..h).contains(&y) {
            return Hit::Outside;
        }
        if (1204.0..1241.0).contains(&x) && (14.0..48.0).contains(&y) {
            return Hit::Close;
        }
        if audio && (1017.0..1190.0).contains(&x) && (14.0..48.0).contains(&y) {
            return Hit::Audio;
        }
        if (20.0..54.0).contains(&x) && (14.0..48.0).contains(&y) {
            return Hit::Settings;
        }
        if self.settings && (20.0..304.0).contains(&x) && (51.0..145.0).contains(&y) {
            return if y >= 112.0 { Hit::Dim } else { Hit::Inside };
        }
        if let Some(detail) = &self.details {
            if (1195.0..1230.0).contains(&x) && (208.0..242.0).contains(&y) {
                return Hit::CloseDetails;
            }
            if (897.0..1239.0).contains(&x) && y >= 198.0 && y < h - 44.0 {
                if (h - 85.0..h - 49.0).contains(&y) {
                    return match x {
                        908.0..980.0 if detail.page > 0 => Hit::PropertyPage(false),
                        981.0..1030.0
                            if (detail.page + 1) * self.detail_count(h) < detail.rows.len() =>
                        {
                            Hit::PropertyPage(true)
                        }
                        1032.0..1126.0 => Hit::ResetProperties,
                        1130.0..1227.0 => Hit::ApplyProperties,
                        _ => Hit::Inside,
                    };
                }
                let ix = ((y - 262.0) / 39.0).floor();
                if ix >= 0.0 && (ix as usize) < self.detail_count(h) {
                    let count = self.detail_count(h);
                    let page = detail.page.min(detail.rows.len().saturating_sub(1) / count);
                    let i = page * count + ix as usize;
                    if i < detail.rows.len() && (1140.0..1227.0).contains(&x) {
                        return Hit::Property(i, x >= 1184.0);
                    }
                }
                return Hit::Inside;
            }
        }
        if (61.0..103.0).contains(&y) {
            if let Some(i) = TAB_X.windows(2).position(|b| x >= b[0] && x < b[1] - 8.0) {
                return Hit::Tab(i);
            }
        }
        if self.tab == 0 && (108.0..146.0).contains(&y) {
            return match x {
                20.0..1047.0 => Hit::Search,
                1057.0..1095.0 => Hit::Import,
                1105.0..1143.0 if self.link.is_some() => Hit::Submit,
                1105.0..1143.0 => Hit::Link,
                1153.0..1191.0 => Hit::Folder,
                1201.0..1239.0 => Hit::Refresh,
                _ => Hit::Inside,
            };
        }
        if (114.0..150.0).contains(&y) {
            return match self.tab {
                1 if (20.0..524.0).contains(&x) => Hit::Search,
                1 if self.steam_online && (534.0..634.0).contains(&x) => Hit::Submit,
                1 if (646.0..787.0).contains(&x) => Hit::Local,
                1 if (797.0..1040.0).contains(&x) => Hit::Online,
                1 if self.steam_online && (1050.0..1240.0).contains(&x) => Hit::Sort,
                2 if (20.0..1128.0).contains(&x) => Hit::Search,
                2 if (1138.0..1240.0).contains(&x) => Hit::Submit,
                3 if (20.0..218.0).contains(&x) => Hit::Power,
                3 if (302.0..421.0).contains(&x) => Hit::Interval,
                3 if (448.0..630.0).contains(&x) => Hit::AddCurrent,
                3 if (641.0..761.0).contains(&x) => Hit::Clear,
                _ => Hit::Inside,
            };
        }
        if self.tab == 0 && (166.0..192.0).contains(&y) {
            if let Some(i) = SOURCE_FILTERS
                .iter()
                .position(|&(l, w)| (l..l + w).contains(&x))
            {
                return Hit::Filter(i);
            }
        } else if matches!(self.tab, 1 | 2) && (158.0..190.0).contains(&y) && x >= 20.0 {
            let col = ((x - 20.0) / 116.0) as usize;
            if col < 10 && (x - 20.0) % 116.0 < 108.0 {
                return Hit::Filter(col);
            }
        }
        if self.online() && y >= h - 37.0 {
            if self.page > 1 && (1016.0..1078.0).contains(&x) {
                return Hit::Page(false);
            }
            if self.more && (1178.0..1240.0).contains(&x) {
                return Hit::Page(true);
            }
        }
        if x < 20.0 || y < self.top() || y >= h - 40.0 {
            return Hit::Inside;
        }
        let query = self.search.to_lowercase();
        let count = self.filtered(&query).count();
        if SCROLL_X.contains(&x)
            && let Some([start, end, thumb, length]) = self.scroll_geometry(h, count)
            && (start..end).contains(&y)
        {
            return Hit::Scrollbar((thumb..thumb + length).contains(&y));
        }
        let yy = y - self.top() + self.offset(h, count);
        let gx = x - 20.0;
        let col = (gx / X_STEP) as usize;
        if col >= 5 || gx % X_STEP >= CARD_W || yy % Y_STEP >= CARD_H {
            return Hit::Inside;
        }
        let row = (yy / Y_STEP) as usize;
        if let Some((i, item)) = self.filtered(&query).nth(row * 5 + col) {
            if item.scene
                && (171.0..195.0).contains(&(gx % X_STEP))
                && (77.0..103.0).contains(&(yy % Y_STEP))
            {
                Hit::Details(i)
            } else if self.tab == 3 && item.in_list
                && (198.0..222.0).contains(&(gx % X_STEP))
                && (77.0..103.0).contains(&(yy % Y_STEP))
            {
                Hit::Remove(i)
            } else {
                Hit::Apply(i)
            }
        } else {
            Hit::Inside
        }
    }

    pub fn dim_at(&mut self, window: &Window, x: f64) {
        let [l, _, s, _] = Self::rect(window);
        self.dim = (((x as f32 - l) / s - 36.0) / 252.0).clamp(0.0, 1.0) * 0.95;
    }

    pub fn local_target(&self, index: usize) -> Option<usize> {
        if index < ONLINE_ITEM {
            return self
                .items
                .get(index)
                .filter(|item| item.can_play)
                .map(|_| index);
        }
        if self.tab == 1 {
            let item = self.remote.get(index - ONLINE_ITEM)?;
            return self.items.iter().position(|local| {
                local.can_play && local.id.strip_prefix("we:") == Some(&item.id)
            });
        }
        None
    }

    fn preview_source(&self, index: usize, active: usize) -> Option<PreviewSource> {
        if let Some(local) = self.local_target(index) {
            return (local != active && self.items[local].animated).then_some(PreviewSource::Local(local));
        }
        let item = self.remote.get(index.checked_sub(ONLINE_ITEM)?)?;
        (item.animated && item.poster != PosterKey::EMPTY).then_some(PreviewSource::Online(item.poster))
    }

    pub fn poll_card_preview(&mut self, now: Instant, hover: Hit, active: usize,
        wake: Option<&crate::bridge_signal::Wake>, gpu: &mut crate::gpu::Gpu) -> bool {
        let Some(folder) = &self.folder else { return false };
        let wanted = hover.card().and_then(|i| self.preview_source(i, active).map(|source| (i, source)));
        let state = &mut self.card_preview;
        let changed = wanted != state.wanted;
        if changed {
            state.wanted = wanted;
            state.since = wanted.map(|_| now);
            state.ready = false;
            state.dirty |= state.requested.take().is_some();
            gpu.catalog_preview.clear_remote();
        }
        if state.since.is_some_and(|since| now - since >= Duration::from_millis(150)) {
            state.since = None;
            state.serial += 1;
            state.requested = Some(state.serial);
            state.dirty = true;
        }
        if state.dirty {
            let command = match (state.requested, wanted) {
                (Some(serial), Some((_, PreviewSource::Local(index)))) => format!("0 {serial} 1 {index}"),
                (Some(serial), Some((_, PreviewSource::Online(key)))) => format!("0 {serial} 2 {}", key.as_str().unwrap()),
                _ => "0 0 0 0".into(),
            };
            let temp = folder.join("panel-preview.tmp");
            if std::fs::write(&temp, command).and_then(|()| std::fs::rename(temp, folder.join("panel-preview.txt"))).is_ok() {
                state.dirty = false;
                if let Some(wake) = wake { wake.request(); }
            }
        }
        if !state.ready && let Some(serial) = state.requested {
            let result = folder.join(format!("panel-result-{serial}.txt"));
            if let Ok(value) = std::fs::read_to_string(&result) {
                state.ready = true;
                let _ = std::fs::remove_file(result);
                if value.starts_with("worker ") {
                    let file = folder.join(format!("surface-panel-{serial}.txt"));
                    let _ = crate::shared::Source::read(&file).and_then(|source| gpu.attach_catalog_preview(source));
                }
            }
        }
        changed || gpu.catalog_preview.remote_ready()
    }

    pub fn card_playing(&self, hover: Hit, active: usize) -> bool {
        self.card_preview.wanted.is_some_and(|(i, source)| hover.card() == Some(i)
            && self.preview_source(i, active) == Some(source))
    }

    pub fn card_uses_main(&self, hover: Hit, active: usize) -> bool {
        hover.card().and_then(|i| self.local_target(i)) == Some(active)
    }

    pub fn card_preview_deadline(&self, now: Instant) -> Option<Instant> {
        let state = &self.card_preview;
        if state.dirty || state.requested.is_some() && !state.ready {
            Some(now + Duration::from_millis(16))
        } else {
            state.since.map(|since| since + Duration::from_millis(150))
        }
    }

    pub fn activate(&mut self, hit: Hit, active: usize, now: Instant) -> Action {
        if self.tube.as_ref().is_some_and(Tube::closing) {
            return Action::None;
        }
        self.editing = hit == Hit::Search;
        if !matches!(hit, Hit::Settings | Hit::Dim | Hit::Inside) {
            self.settings = false;
        }
        match hit {
            Hit::Close | Hit::Outside => self.close(now),
            Hit::Settings => self.settings = !self.settings,
            Hit::Audio => return Action::Audio,
            Hit::Tab(tab) => {
                self.tab = tab;
                self.details = None;
                self.link = None;
                self.filter = 0;
                self.page = 1;
                self.search.clear();
                self.submitted.clear();
                self.next_tag = "".into();
                self.status = "".into();
                self.scroll = 0.0;
                self.online_job = None;
                self.remote.clear();
                if self.online() {
                    self.query();
                }
                let end = [TAB_X[tab], TAB_X[tab + 1] - TAB_X[tab] - 8.0];
                if self.reduced {
                    self.underline = Transition::new(end, now);
                } else {
                    self.underline.set(end, now, 0.32, ICON_EASE);
                }
            }
            Hit::Apply(i) => {
                if let Some(index) = self.local_target(i) {
                    return Action::Apply(index);
                }
                if i >= ONLINE_ITEM {
                    let Some(item) = self.remote.get(i - ONLINE_ITEM) else {
                        return Action::None;
                    };
                    if self.tab == 1 {
                        return Action::Open(item.url.to_string());
                    }
                    if self.action_job.is_none() {
                        let id = item.id.to_string();
                        self.status = "Descargando fondo…".into();
                        self.action_job =
                            self.request("action", json!({"kind": "download", "id": id}));
                    }
                    return Action::None;
                }
                self.status = "Requiere Wallpaper Engine".into();
                if let Some(item) = self.items.get(i)
                    && let Some(id) = item
                        .id
                        .strip_prefix("we:")
                        .filter(|id| id.bytes().all(|b| b.is_ascii_digit()))
                {
                    return Action::Open(format!(
                        "https://steamcommunity.com/sharedfiles/filedetails/?id={id}"
                    ));
                }
            }
            Hit::Details(i) => {
                let serial = self.request("properties", json!({"index": i}));
                self.details = Some(Details {
                    index: i,
                    serial,
                    page: 0,
                    rows: Vec::new(),
                });
            }
            Hit::CloseDetails => self.details = None,
            Hit::Property(i, up) => {
                if let Some(detail) = &mut self.details
                    && detail.serial.is_none()
                    && let Some(row) = detail.rows.get_mut(i)
                {
                    let old = &row["value"];
                    row["value"] = match field(row, "type") {
                        "bool" => json!(old != true),
                        "slider" => {
                            let val = old.as_f64().unwrap_or_default();
                            let delta =
                                row["step"].as_f64().unwrap_or(1.0) * if up { 1.0 } else { -1.0 };
                            let next = (val + delta).clamp(
                                row["min"].as_f64().unwrap_or(0.0),
                                row["max"].as_f64().unwrap_or(100.0),
                            );
                            json!(next)
                        }
                        "combo" => {
                            let options = row["options"].as_array().unwrap();
                            let n = options.iter().position(|o| o["value"] == *old).unwrap_or(0);
                            options[(n + if up { 1 } else { options.len() - 1 }) % options.len()]["value"].clone()
                        }
                        _ => old.clone(),
                    };
                }
            }
            Hit::ResetProperties => {
                if let Some(detail) = &mut self.details {
                    for row in &mut detail.rows {
                        row["value"] = row["default"].clone();
                    }
                }
            }
            Hit::ApplyProperties => {
                if let Some(detail) = &self.details {
                    let values = detail
                        .rows
                        .iter()
                        .map(|r| (field(r, "key").to_owned(), r["value"].clone()))
                        .collect::<serde_json::Map<_, _>>();
                    let serial = self.request(
                        "properties",
                        json!({"index": detail.index, "values": values}),
                    );
                    if let Some(detail) = &mut self.details {
                        detail.serial = serial;
                    }
                }
            }
            Hit::PropertyPage(next) => {
                if let Some(detail) = &mut self.details {
                    detail.page = if next {
                        detail.page + 1
                    } else {
                        detail.page.saturating_sub(1)
                    };
                }
            }
            Hit::Online | Hit::Local => {
                let online = hit == Hit::Online;
                if self.steam_online != online {
                    self.steam_online = online;
                    self.details = None;
                    self.scroll = 0.0;
                    self.filter = 0;
                    self.page = 1;
                    self.search.clear();
                    self.submitted.clear();
                    self.next_tag = "".into();
                    self.remote.clear();
                    self.status = "".into();
                    self.online_job = None;
                    if online {
                        self.query();
                    } else {
                        self.library_job = self.request("library", json!({}));
                    }
                }
            }
            Hit::Submit => self.submit(),
            Hit::Filter(i) => {
                self.filter = i;
                self.scroll = 0.0;
                self.page = 1;
                if self.online() {
                    if self.tab == 2 {
                        self.search.clear();
                        self.submitted.clear();
                        self.next_tag = "".into();
                    }
                    self.query();
                }
            }
            Hit::Sort => {
                self.sort = (self.sort + 1) % 4;
                self.page = 1;
                self.query();
            }
            Hit::Page(next) => {
                self.page = if next {
                    self.page + 1
                } else {
                    self.page.saturating_sub(1).max(1)
                };
                self.query();
            }
            Hit::Import if self.action_job.is_none() => {
                self.status = "Elige un archivo…".into();
                self.action_job = self.request("action", json!({"kind": "import"}));
            }
            Hit::Link if self.action_job.is_none() => {
                self.link = Some(String::new());
                self.editing = true;
            }
            Hit::Folder if self.action_job.is_none() => {
                self.status = "".into();
                self.action_job = self.request("action", json!({"kind": "folder"}));
            }
            Hit::Refresh => {
                self.status = "".into();
                self.library_job = self.request("library", json!({}));
            }
            Hit::Remove(i) => {
                if let Some(item) = self.items.get_mut(i) { item.in_list = false; }
                if !self.items.iter().any(|item| item.in_list) { self.rotate_at = None; }
                self.save_prefs();
            }
            Hit::AddCurrent => {
                if let Some(item) = self.items.get_mut(active).filter(|item| item.can_play) {
                    item.in_list = true;
                }
                self.save_prefs();
            }
            Hit::Clear => {
                for i in &mut self.items {
                    i.in_list = false;
                }
                self.rotate_at = None;
                self.save_prefs();
            }
            Hit::Power => {
                if self.rotate_at.is_some() {
                    self.rotate_at = None;
                } else if self.items.iter().any(|i| i.in_list) {
                    self.rotate_at = Some(now + self.interval());
                }
                self.save_prefs();
            }
            Hit::Interval => {
                self.interval = (self.interval + 1) % INTERVALS.len();
                self.applied(now);
                self.save_prefs();
            }
            _ => (),
        }
        Action::None
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(INTERVALS[self.interval])
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.rotate_at
    }

    pub fn applied(&mut self, now: Instant) {
        if self.rotate_at.is_some() {
            self.rotate_at = Some(now + self.interval());
        }
    }

    pub fn playback_error(&mut self) {
        self.status = "No se pudo reproducir el fondo".into();
    }

    pub fn rotate(&mut self, active: usize, now: Instant) -> Option<usize> {
        if now < self.rotate_at? {
            return None;
        }
        self.applied(now);
        let items = &self.items;
        if items.is_empty() {
            return None;
        }
        let start = (active + 1) % items.len();
        (0..items.len())
            .map(|i| (start + i) % items.len())
            .find(|&i| items[i].in_list && i != active)
    }

    pub fn key_text(&mut self, text: &str) {
        if self.editing {
            if let Some(url) = &mut self.link {
                let capacity = 2048usize.saturating_sub(url.chars().count());
                url.extend(text.chars().filter(|c| !c.is_control()).take(capacity));
            } else {
                let capacity = 64usize.saturating_sub(self.search.chars().count());
                self.search
                    .extend(text.chars().filter(|c| !c.is_control()).take(capacity));
            }
            self.scroll = 0.0;
        }
    }

    pub fn backspace(&mut self) {
        if self.editing {
            if let Some(url) = &mut self.link {
                url.pop();
            } else {
                self.search.pop();
            }
            self.scroll = 0.0;
        }
    }
    pub fn editing(&self) -> bool {
        self.editing
    }

    fn detail_count(&self, height: f32) -> usize {
        (((height - 85.0 - 262.0) / 39.0).floor() as usize).clamp(1, DETAIL_COUNT)
    }
    pub fn uniforms(
        &mut self,
        p: &mut Params,
        window: &Window,
        now: Instant,
        hover: Hit,
        active: usize,
        switching: bool,
        audio: Option<bool>,
        gpu: &crate::gpu::Gpu,
    ) {
        p.wall_state[3] = self.dim;
        let Some(tube) = &self.tube else {
            return;
        };
        let card = hover.card();
        if let Some(i) = card && !self.card_zoom.iter().any(|(other, _)| *other == i) {
            self.card_zoom.push((i, Transition::new([0.0], now)));
        }
        for (i, zoom) in &mut self.card_zoom {
            let target = [f32::from(card == Some(*i))];
            if self.reduced { *zoom = Transition::new(target, now); }
            else { zoom.set(target, now, HOVER_SECONDS, ICON_EASE); }
        }
        self.card_zoom.retain(|(i, zoom)| card == Some(*i) || zoom.running(now, HOVER_SECONDS));
        let rect = Self::rect(window);
        let frame = tube.frame(now);
        p.wall_rect = rect;
        p.wall_crt = frame.layers;
        let [header, tabs, workspace] = frame.content;
        p.wall_fade = [frame.backdrop, header, tabs, workspace];
        let h = match hover {
            Hit::Close => 1,
            Hit::Settings => 2,
            Hit::Audio => 29,
            Hit::Tab(i) => 4 + i as u32,
            Hit::Power => 9,
            Hit::Interval => 10,
            Hit::AddCurrent => 11,
            Hit::Clear => 12,
            Hit::Online => 13,
            Hit::Local => 14,
            Hit::Submit => 15,
            Hit::Sort => 16,
            Hit::Page(false) => 17,
            Hit::Page(true) => 18,
            Hit::Import => 19,
            Hit::Folder => 20,
            Hit::Refresh => 21,
            Hit::Filter(i) => 22 + i as u32,
            Hit::Link => 3,
            Hit::CloseDetails => 2_000_001,
            Hit::ResetProperties => 2_000_002,
            Hit::ApplyProperties => 2_000_003,
            Hit::PropertyPage(false) => 2_000_004,
            Hit::PropertyPage(true) => 2_000_005,
            Hit::Property(i, next) => 2_000_010 + i as u32 * 2 + u32::from(next),
            Hit::Details(i) => 32 + i as u32,
            Hit::Apply(i) | Hit::Remove(i) => 32 + i as u32,
            _ => 0,
        };
        let flags = u32::from(self.editing)
            | (u32::from(self.settings) << 1)
            | (u32::from(self.rotate_at.is_some()) << 2)
            | (u32::from(switching) << 3)
            | (u32::from(self.steam_online) << 4)
            | (u32::from(self.online()) << 5)
            | (u32::from(self.link.is_some()) << 6);
        p.wall_ui = [self.tab as u32, h, flags, INTERVALS[self.interval] as u32];
        p.wall_catalog = [
            self.filter as u32,
            self.page as u32,
            u32::from(self.more),
            self.top() as u32,
        ];
        let query = self.search.to_lowercase();
        let count = self.filtered(&query).count();
        let scroll = self.offset(rect[3], count);
        let underline = self.underline.sample(now, 0.32, ICON_EASE);
        p.wall_grid = [scroll % Y_STEP, 0.0, underline[0], underline[1]];
        p.wall_state[0] = self.items.len() as f32;
        p.wall_state[1] = self.items.iter().filter(|i| i.in_list).count() as f32;
        p.wall_state[2] = count as f32;
        if self.tab == 0 {
            p.wall_sources[0] = self.items.len() as u32;
            for item in &self.items {
                let source = item.origin() as usize;
                if source != 0 {
                    p.wall_sources[source] += 1;
                }
            }
        }
        if let Some([_, _, y, length]) = self.scroll_geometry(rect[3], count) {
            p.wall_scrollbar = [
                y,
                length,
                u32::from(hover == Hit::Scrollbar(true)) as f32,
                u32::from(self.dragging_scroll()) as f32,
            ];
        }
        p.wall_glows = [ROWS as u32; 4];
        let preview = self.card_playing(hover, active)
            && (gpu.catalog_preview.presented_remote || gpu.catalog_preview.remote_ready());
        for (slot, (index, item)) in self.visible_items(&query, rect[3]).enumerate() {
            let local = if index < ONLINE_ITEM {
                Some(index)
            } else if self.tab == 1 {
                self.items
                    .iter()
                    .position(|i| i.can_play && i.id.strip_prefix("we:") == Some(&item.id))
            } else {
                None
            };
            let metadata = local.map_or(item, |i| &self.items[i]);
            let resolution = match &*item.resolution {
                "4k" | "4K" => 1,
                "1440p" => 2,
                "1080p" => 3,
                _ => 0,
            };
            let zoom = self.card_zoom.iter().find(|(i, _)| *i == index)
                .map_or(0, |(_, zoom)| (zoom.sample(now, HOVER_SECONDS, ICON_EASE)[0] * 255.0).round() as u32);
            let flags = u32::from(local == Some(active))
                | (u32::from(metadata.scene) << 1)
                | (u32::from(item.in_list) << 2)
                | (u32::from(item.can_play) << 3)
                | (u32::from(index >= ONLINE_ITEM && self.tab == 2) << 6)
                | (u32::from(&*item.kind == "web") << 9)
                | (u32::from(index >= ONLINE_ITEM && self.tab == 1 && local.is_some()) << 10)
                | (u32::from(item.name.chars().any(|ch| ch > 'ÿ')) << 11)
                | (metadata.origin() << 12)
                | (resolution << 14)
                | (u32::from(!switching && card == Some(index) && local == Some(active)) << 16)
                | (u32::from(preview && card == Some(index)) << 17)
                | (zoom << 24);
            if local == Some(active) {
                p.wall_glows[0] = slot as u32;
            }
            if h == 32 + index as u32 {
                p.wall_glows[1] = slot as u32;
            }
            let (first, second) = split_title(&item.name, 212);
            let n = write_line(p, slot, first, 212);
            let line = SECOND_TITLE + slot;
            if !second.is_empty() {
                p.wall_rows[line][2] = write_line(p, line, second, 212);
            }
            let poster = if let Some(layer) = gpu.poster_layer(item.poster) {
                4 + layer
            } else {
                match &*item.id {
                    "we:3608457742" => 1,
                    "we:3635457818" => 2,
                    "we:3003524837" => 3,
                    _ => 0,
                }
            };
            p.wall_rows[slot] = [poster, flags, n, index as u32];
            p.wall_sizes[slot / 4][slot % 4] = metadata.size;
            p.wall_grid[1] = (slot + 1) as f32;
        }
        let active_name = self.items.get(active).map_or("—", |i| &i.name);
        p.wall_rows[ACTIVE_LINE][2] = write_line(p, ACTIVE_LINE, active_name, 252);
        if let Some(playing) = audio.filter(|_| !switching) {
            let label = if playing {
                "Silenciar sonido"
            } else {
                "Activar sonido"
            };
            p.wall_rows[AUDIO_LINE] = [
                u32::from(playing),
                0,
                write_line(p, AUDIO_LINE, label, 128),
                0,
            ];
        }
        p.wall_rows[SEARCH_LINE][2] = write_line(
            p,
            SEARCH_LINE,
            self.link.as_deref().unwrap_or(&self.search),
            252,
        );
        let status = if self.action_job.is_some() && self.status.is_empty() {
            "Procesando fondo…"
        } else {
            &self.status
        };
        p.wall_rows[STATUS_LINE][2] = write_line(p, STATUS_LINE, status, 252);
        p.wall_rows[SORT_LINE][2] = write_line(
            p,
            SORT_LINE,
            [
                "Mejor valorados",
                "Tendencias",
                "Más suscritos",
                "Más recientes",
            ][self.sort],
            164,
        );
        if let Some(detail) = &self.details {
            let count = self.detail_count(rect[3]);
            let page = detail.page.min(detail.rows.len().saturating_sub(1) / count);
            p.wall_details = [
                1,
                (detail.rows.len() - (page * count).min(detail.rows.len())).min(count) as u32,
                page as u32,
                u32::from((page + 1) * count < detail.rows.len()),
            ];
            for (slot, row) in detail
                .rows
                .iter()
                .enumerate()
                .skip(page * count)
                .take(count)
            {
                let line = DETAIL_LINE + slot - page * count;
                let label = format!("{}: {}", field(row, "name"), value_label(row));
                p.wall_rows[line][2] = write_line(p, line, &label, 213);
                p.wall_rows[line][3] = slot as u32;
            }
            let line = DETAIL_LINE + DETAIL_COUNT;
            p.wall_rows[line][2] = write_line(
                p,
                line,
                if detail.serial.is_some() {
                    "Cargando…"
                } else if detail.rows.is_empty() {
                    "Sin controles compatibles"
                } else {
                    self.items.get(detail.index).map_or("", |item| &item.name)
                },
                280,
            );
        }
    }
}

fn glyph_code(c: char) -> u8 {
    if (' '..='ÿ').contains(&c) {
        c as u8
    } else {
        b'?'
    }
}

fn advance(c: u8) -> u32 {
    u32::from(glyphs::ADVANCE[(c - 32) as usize])
}

fn split_title(text: &str, limit: u32) -> (&str, &str) {
    let mut width = 0;
    let mut word = 0;
    for (count, (i, c)) in text.char_indices().enumerate() {
        width += advance(glyph_code(c));
        if c.is_whitespace() {
            word = i;
        }
        if width > limit || count == 32 {
            let cut = if word > 0 { word } else { i };
            return (text[..cut].trim_end(), text[cut..].trim_start());
        }
    }
    (text, "")
}

fn write_line(p: &mut Params, line: usize, text: &str, limit: u32) -> u32 {
    let (mut count, mut width) = (0, 0);
    let mut run = [0; 32];
    for c in text.chars() {
        let code = glyph_code(c);
        let w = advance(code);
        if count == run.len() || width + w > limit {
            let dots = advance(b'.') * 3;
            while count > run.len() - 3 || width + dots > limit {
                count -= 1;
                width -= advance(run[count]);
            }
            run[count..count + 3].fill(b'.');
            count += 3;
            break;
        }
        run[count] = code;
        count += 1;
        width += w;
    }
    let mut x = 0;
    for (i, &code) in run.iter().take(count).enumerate() {
        let (a, b, shift) = (line * 2 + i / 16, i / 4 % 4, (i % 4) * 8);
        p.wall_words[a][b] |= u32::from(code) << shift;
        p.wall_advances[a][b] |= x << shift;
        x += advance(code);
    }
    count as u32 | (x << 8)
}

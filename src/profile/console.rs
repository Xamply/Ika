//! Floating Home console and the full log view, sharing the selected instance.
mod idle;
mod dino_world;

use crate::{dock, home::Game, minecraft};
use egui::{Align2, Color32 as C, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("ika-semibold".into()))
}
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winit::window::Window;

const WHITE: C = C::from_rgb(241, 245, 249);
const MUTED: C = C::from_rgb(156, 174, 200);
const PURPLE: C = C::from_rgb(88, 32, 255);
const GREEN: C = C::from_rgb(52, 211, 153);
const PINK: C = C::from_rgb(232, 121, 249);
const AMBER: C = C::from_rgb(253, 202, 115);
const RED: C = C::from_rgb(248, 110, 118);
const MAX_LINES: usize = 5_000;
const MAX_BYTES: u64 = 256 * 1024;
const MAX_HISTORY: usize = 384 * 1024;
const FILE: &str = "home-console.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Hidden,
    Home,
    Detail,
}

pub enum Action {
    Detail,
    Select(i32),
    Launch,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    origin: Option<[f32; 2]>,
    size: [f32; 2],
    maze_size: [f32; 2],
    minimized: bool,
    volume: u8,
    maze_volume: u8,
    game: idle::Kind,
}

impl Default for Settings {
    fn default() -> Self {
        Self { origin: None, size: [640., 360.], maze_size: [540., 740.], minimized: false,
            volume: 45, maze_volume: 9, game: idle::Kind::Dino }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Idle,
    Preparing,
    Loading,
    Running,
    Menu,
    World,
    Connecting,
    Playing,
    Closed,
    Failed,
    Crashed,
}

impl Status {
    fn text(self) -> &'static str {
        match self {
            Self::Idle => "Listo para jugar",
            Self::Menu => "Listo",
            Self::Preparing => "Iniciando...",
            Self::Loading => "Cargando Minecraft",
            Self::Running | Self::Playing => "Jugando",
            Self::World => "Cargando el mundo",
            Self::Connecting => "Conectando",
            Self::Closed => "Sesión terminada",
            Self::Failed => "No se pudo iniciar",
            Self::Crashed => "Crasheó",
        }
    }

    fn tint(self) -> C {
        match self {
            Self::Failed | Self::Crashed => RED,
            Self::Idle | Self::Menu | Self::Closed => GREEN,
            _ => PINK,
        }
    }

    fn has_game_output(self) -> bool {
        !matches!(self, Self::Idle | Self::Preparing | Self::Failed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level { Info, Warn, Error, Launcher }

impl Level {
    fn text(self) -> &'static str {
        match self { Self::Info => "INFO", Self::Warn => "AVISO", Self::Error => "ERROR", Self::Launcher => "IKA" }
    }
    fn tint(self) -> C {
        match self {
            Self::Info => C::from_rgb(67, 56, 202),
            Self::Warn => C::from_rgb(217, 119, 6),
            Self::Error => C::from_rgb(220, 38, 38),
            Self::Launcher => C::from_rgb(124, 58, 237),
        }
    }
}

struct Line {
    time: String,
    level: Level,
    thread: String,
    text: String,
}

impl Line {
    fn bytes(&self) -> usize { self.time.len() + self.thread.len() + self.text.len() }

    fn parse(mut raw: &str) -> Self {
        let mut time = "";
        let mut thread = "";
        let mut level = Level::Info;
        // Modern Java: [12:30:05] [Render thread/WARN]: message.
        // Older Forge logs can add an extra source field before the colon.
        if let Some((first, rest)) = bracket(raw) && first.as_bytes().get(2) == Some(&b':') {
            time = first;
            raw = rest;
        }
        if let Some((tag, rest)) = bracket(raw) {
            let (name, severity) = tag.rsplit_once('/').unwrap_or(("", tag));
            match severity.to_ascii_uppercase().as_str() {
                "WARN" | "WARNING" => level = Level::Warn,
                "ERROR" | "FATAL" => level = Level::Error,
                "INFO" | "DEBUG" | "TRACE" => {}
                _ => return Self { time: time.into(), level, thread: String::new(), text: raw.into() },
            }
            thread = name;
            raw = rest.trim_start_matches(':').trim_start();
        } else if raw.trim_start().starts_with("Caused by:") || raw.trim_start().starts_with("at ") {
            level = Level::Error;
        }
        Self { time: time.into(), level, thread: thread.into(), text: raw.into() }
    }
}

fn bracket(s: &str) -> Option<(&str, &str)> {
    let (a, b) = s.strip_prefix('[')?.split_once(']')?;
    Some((a, b.trim_start()))
}

#[derive(Default)]
struct Tail {
    path: PathBuf,
    offset: u64,
    fragment: Vec<u8>,
    lines: VecDeque<Line>,
    bytes: usize,
    total: u64,
}

impl Tail {
    fn append(&mut self, raw: &[u8]) {
        let text = String::from_utf8_lossy(&raw[..raw.len().min(8_192)]);
        let raw = text.trim_end_matches('\r');
        if raw.is_empty() { return; }
        let line = Line::parse(raw);
        self.bytes += line.bytes();
        self.lines.push_back(line);
        self.total += 1;
        while self.lines.len() > MAX_LINES || self.bytes > MAX_HISTORY {
            self.bytes -= self.lines.pop_front().unwrap().bytes();
        }
    }

    fn after(&self, count: u64) -> impl Iterator<Item = &Line> {
        let first = self.total - self.lines.len() as u64;
        self.lines.iter().skip(count.saturating_sub(first).min(self.lines.len() as u64) as usize)
    }

    fn read(&mut self, path: &Path, complete: bool) -> bool {
        let Ok(mut file) = File::open(path) else { return false };
        let Ok(meta) = file.metadata() else { return false };
        let end = meta.len();
        let reset = self.path != path || end < self.offset;
        if reset {
            let total = self.total;
            *self = Self { total, path: path.into(), ..Self::default() };
        }
        if end == self.offset {
            if complete && !self.fragment.is_empty() {
                let fragment = std::mem::take(&mut self.fragment);
                self.append(&fragment);
                return true;
            }
            return reset;
        }
        let start = self.offset.max(end.saturating_sub(MAX_BYTES));
        if file.seek(SeekFrom::Start(start)).is_err() {
            return false;
        }
        let mut bytes = std::mem::take(&mut self.fragment);
        if start != self.offset { bytes.clear(); }
        let prefix = bytes.len();
        if file.take(end - start).read_to_end(&mut bytes).is_err() {
            self.fragment = bytes;
            return false;
        }
        let actual_end = start + (bytes.len() - prefix) as u64;
        let skip = if start != self.offset { bytes.iter().position(|&b| b == b'\n').map_or(bytes.len(), |i| i + 1) } else { 0 };
        self.offset = actual_end;
        let mut rest = &bytes[skip..];
        let before = self.total;
        while let Some(i) = rest.iter().position(|&b| b == b'\n') {
            self.append(&rest[..i]);
            rest = &rest[i + 1..];
        }
        if complete { self.append(rest); }
        else { self.fragment.extend_from_slice(&rest[..rest.len().min(8_192)]); }
        reset || self.total != before
    }
}

struct Instance {
    key: String,
    name: String,
    version: String,
    loader: String,
    mods: usize,
    folder: PathBuf,
    log: PathBuf,
    report: Option<PathBuf>,
    icon: Option<PathBuf>,
}

impl Instance {
    fn new(instance: &minecraft::Instance) -> Self {
        let folder = minecraft::game_folder(instance);
        Self {
            key: instance.path.clone(), name: instance.name.clone(), version: instance.mc_version.clone(),
            loader: instance.loader.clone(), mods: instance.mod_count,
            log: folder.join("logs/ika-launcher-output.log"), report: recent_report(&folder), folder,
            icon: instance.icon_path.as_deref().map(PathBuf::from),
        }
    }
}

#[derive(Default)]
struct Metrics {
    last: Option<(Instant, u64)>,
    ram: usize,
    cpu: f32,
}

impl Metrics {
    fn read(&mut self, pid: u32, now: Instant) {
        use windows::Win32::{
            Foundation::{CloseHandle, FILETIME},
            System::{
                ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
                Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
            },
        };
        unsafe {
            let Ok(proc) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) else { return };
            let mut mem = PROCESS_MEMORY_COUNTERS::default();
            let size = std::mem::size_of_val(&mem) as u32;
            mem.cb = size;
            if GetProcessMemoryInfo(proc, &mut mem, size).is_ok() {
                self.ram = mem.WorkingSetSize / (1024 * 1024);
            }
            let [mut creation, mut exit, mut kernel, mut user] = [FILETIME::default(); 4];
            if GetProcessTimes(proc, &mut creation, &mut exit, &mut kernel, &mut user).is_ok() {
                let value = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
                let ticks = value(kernel) + value(user);
                if let Some((at, last)) = self.last.replace((now, ticks)) {
                    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
                    self.cpu = ((ticks.saturating_sub(last)) as f32 / 10_000_000.
                        / (now - at).as_secs_f32().max(0.01) / cores as f32 * 100.).clamp(0., 100.);
                }
            }
            let _ = CloseHandle(proc);
        }
    }
}

pub struct Console {
    settings: Settings,
    view: View,
    /// Opacity of the detailed view while it slides in (1 when settled).
    pub detail_opacity: f32,
    /// Opacity of the floating console while Home slides away.
    pub home_opacity: f32,
    instance: Option<Instance>,
    status: Status,
    milestone: Option<Status>,
    detail: String,
    tail: Tail,
    read_pending: bool,
    next_poll: Option<Instant>,
    next_game: Option<Instant>,
    last_game: Option<Instant>,
    revision: u64,
    hit: Rect,
    follow: [bool; 2],
    clear_count: u64,
    filter: String,
    command: String,
    levels: [bool; 3],
    notes: VecDeque<Line>,
    pointer_owned: bool,
    volume_open: bool,
    started: Option<Instant>,
    pid: Option<u32>,
    metrics: Metrics,
    game: idle::Game,
    icon: Option<egui::TextureHandle>,
    grass: Option<egui::TextureHandle>,
    report_text: Option<String>,
    latest_text: Option<String>,
    latest_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    split: f32,
    report_path: Option<PathBuf>,
    latest_path: Option<PathBuf>,
    crash_search: String,
    latest_search: String,
    ready_banner: Option<(String, String)>,
}

impl Console {
    pub fn new() -> Self {
        let mut settings = std::fs::read(super::storage::native_dir().join(FILE)).ok()
            .and_then(|raw| serde_json::from_slice::<Settings>(&raw).ok()).unwrap_or_default();
        settings.size[0] = settings.size[0].clamp(420., 1360.);
        settings.size[1] = settings.size[1].clamp(245., 940.);
        settings.maze_size[0] = settings.maze_size[0].clamp(420., 1360.);
        settings.maze_size[1] = settings.maze_size[1].clamp(245., 940.);
        settings.volume = settings.volume.min(100);
        settings.maze_volume = settings.maze_volume.min(100);
        let game = idle::Game::new(settings.game);
        Self {
            settings, game, view: View::Hidden, detail_opacity: 1.0, home_opacity: 1.0, instance: None, status: Status::Idle, milestone: None,
            detail: String::new(), tail: Tail::default(), read_pending: true, next_poll: None, next_game: None, last_game: None,
            revision: 0, hit: Rect::NOTHING, follow: [true; 2], clear_count: 0,
            filter: String::new(), command: String::new(), levels: [true; 3], notes: VecDeque::new(),
            pointer_owned: false, volume_open: false, started: None, pid: None, metrics: Metrics::default(),
            icon: None, grass: None, report_text: None, latest_text: None, latest_stamp: None, split: 0.5,
            report_path: None, latest_path: None, crash_search: String::new(), latest_search: String::new(),
            ready_banner: None,
        }
    }

    pub fn save(&self) {
        if let Ok(raw) = serde_json::to_vec(&self.settings) {
            let _ = super::storage::save_native(FILE, &raw);
        }
    }

    pub fn view(&self) -> View { self.view }
    pub fn contains(&self, at: Pos2) -> bool { self.hit.contains(at) }
    pub fn captured(&self) -> bool { self.pointer_owned }
    pub fn capture(&mut self, state: bool) { self.pointer_owned = state; }
    pub fn deadline(&self) -> Option<Instant> { self.next_poll.into_iter().chain(self.next_game).min() }

    pub fn pause_game(&mut self) -> bool {
        let changed = self.game.running() || self.volume_open;
        self.game.pause();
        self.volume_open = false;
        self.last_game = None;
        self.next_game = None;
        changed
    }

    pub fn tick(&mut self, view: View, selected: Option<(&minecraft::Instance, &Game)>, now: Instant) -> bool {
        let mut changed = view != self.view;
        if changed {
            self.view = view;
            self.hit = Rect::NOTHING;
            self.pointer_owned = false;
            self.volume_open = false;
            if view == View::Detail || self.pid.is_some() { self.read_pending = true; }
            self.next_game = None;
            self.last_game = None;
            self.game.pause();
        }
        if view == View::Hidden { self.next_poll = None; return changed; }
        let current = selected.map(|(inst, _)| inst.path.as_str());
        if current != self.instance.as_ref().map(|inst| inst.key.as_str()) {
            self.instance = selected.map(|(inst, _)| Instance::new(inst));
            self.tail = Tail::default();
            self.status = Status::Idle;
            self.milestone = None;
            self.detail.clear();
            self.read_pending = true;
            self.next_poll = None;
            self.started = None;
            self.pid = None;
            self.follow = [true; 2];
            self.clear_count = 0;
            self.notes.clear();
            self.icon = None;
            self.report_text = None;
            self.report_path = None;
            self.latest_text = None;
            self.latest_path = None;
            self.crash_search.clear();
            self.latest_search.clear();
            self.ready_banner = None;
            self.latest_stamp = None;
            self.revision = self.revision.wrapping_add(1);
            changed = true;
        }
        let state = selected.map_or(&Game::Idle, |(_, state)| state);
        let (status, pid, detail) = match state {
            Game::Idle => (Status::Idle, None, ""),
            Game::Preparing(message) => (Status::Preparing, None, message.as_str()),
            Game::Loading(pid) => (Status::Loading, Some(*pid), ""),
            Game::Running(pid) => (Status::Running, Some(*pid), ""),
            Game::Closed => (Status::Closed, None, ""),
            Game::Failed(message) => (Status::Failed, None, message.as_str()),
            Game::Crashed(message) => (Status::Crashed, None, message.as_str()),
        };
        if status != self.status || detail != self.detail {
            if status == Status::Preparing && self.status != Status::Preparing {
                self.tail = Tail::default();
                self.follow = [true; 2];
                self.clear_count = 0;
                self.notes.clear();
            }
            if status != self.status {
                self.read_pending = true;
                if matches!(status, Status::Closed | Status::Crashed | Status::Failed)
                    && let Some(inst) = &mut self.instance {
                    inst.report = recent_report(&inst.folder);
                    self.report_text = None;
                    self.report_path = inst.report.clone();
                }
            }
            self.status = status;
            self.detail = detail.into();
            changed = true;
        }
        if self.pid != pid {
            self.pid = pid;
            self.metrics = Metrics::default();
            if pid.is_some() {
                if let Some((instance, _)) = selected {
                    let folder = minecraft::game_folder(instance);
                    if let Some(current) = &mut self.instance && current.folder != folder {
                        current.log = folder.join("logs/ika-launcher-output.log");
                        current.report = recent_report(&folder);
                        current.folder = folder;
                        self.report_text = None;
                        self.latest_stamp = None;
                        self.latest_text = None;
                    }
                }
                self.started = Some(now);
                self.milestone = None;
                self.tail = Tail::default();
                self.clear_count = 0;
            }
        }

        let expanded = view == View::Detail || !self.settings.minimized;
        let tracking = self.instance.is_some() && expanded
            && (status.has_game_output() || view == View::Detail && status == Status::Idle);
        if tracking && (self.read_pending || pid.is_some() && self.next_poll.is_none_or(|at| now >= at)) {
            let inst = self.instance.as_ref().unwrap();
            let path = if status == Status::Idle && !inst.log.is_file() { inst.folder.join("logs/latest.log") } else { inst.log.clone() };
            let before = self.tail.total;
            if self.tail.read(&path, pid.is_none()) {
                if self.tail.total != before {
                    self.milestone = self.tail.lines.iter().rev().take(40).find_map(|line| {
                        let text = line.text.to_ascii_lowercase();
                        if text.starts_with("connecting to ") { Some(Status::Connecting) }
                        else if text.contains("joined the game") { Some(Status::Playing) }
                        else if text.contains("preparing spawn") || text.contains("starting integrated minecraft server") { Some(Status::World) }
                        else if text.contains("sound engine started") { Some(Status::Menu) }
                        else { None }
                    }).or(self.milestone);
                }
                changed = true;
            }
            if view == View::Detail && let Some(pid) = pid {
                self.metrics.read(pid, now);
                changed = true;
            }
            if view == View::Detail {
                if self.report_text.is_none() {
                    self.report_path = inst.report.clone();
                    self.report_text = inst.report.as_ref().and_then(|path| file_excerpt(path, false));
                }
                let latest = inst.folder.join("logs/latest.log");
                self.latest_path = Some(latest.clone());
                let stamp = latest.metadata().ok().map(|meta| (meta.len(), meta.modified().ok()));
                if stamp != self.latest_stamp {
                    self.latest_stamp = stamp;
                    self.latest_text = file_excerpt(&latest, true);
                    changed = true;
                }
                if self.ready_banner.is_none() {
                    let candidate = self.tail.lines.iter().find(|l| {
                        let t = l.text.to_ascii_lowercase();
                        t.contains("sound engine started") || t.contains("beds.png-atlas") || t.contains("applying holder lookups") || t.contains("openal initialized")
                    });
                    if let Some(line) = candidate {
                        let time = if line.time.is_empty() { "02:19:22".into() } else { line.time.clone() };
                        self.ready_banner = Some((time, "Minecraft está listo: ya puedes crear o entrar a un mundo".into()));
                    } else if matches!(self.milestone, Some(Status::Menu | Status::World | Status::Playing)) {
                        self.ready_banner = Some(("02:19:22".into(), "Minecraft está listo: ya puedes crear o entrar a un mundo".into()));
                    }
                }
            }
            self.read_pending = false;
            self.next_poll = if pid.is_some() { Some(now + Duration::from_millis(350)) } else { None };
        } else if !tracking {
            self.next_poll = None;
        }
        let interval = (view == View::Home && expanded && self.idle()).then(|| self.game.interval()).flatten();
        if let Some(interval) = interval {
            if self.next_game.is_none_or(|at| now >= at) {
                let elapsed = self.last_game.replace(now).map_or(0., |last| (now-last).as_secs_f64());
                changed |= self.game.tick(elapsed, self.volume());
                self.next_game = self.game.interval().map(|_| now + interval);
            }
        } else {
            if !self.idle() { self.game.pause(); }
            self.next_game = None;
            self.last_game = None;
        }
        changed
    }

    fn idle(&self) -> bool {
        self.status == Status::Idle || self.status == Status::Closed && self.tail.lines.is_empty()
    }

    fn maze_mode(&self) -> bool { self.idle() && self.settings.game == idle::Kind::V }

    fn body(r: Rect) -> Rect {
        Rect::from_min_max(r.min + vec2(12.,48.), r.max - vec2(12.,70.))
    }

    fn volume_rect(r: Rect) -> Rect {
        Rect::from_min_size(r.right_top() + vec2(-234.,49.), vec2(178.,42.))
    }

    pub fn dino_event(&mut self, window: &Window, pointer: Option<Pos2>, event: &winit::event::WindowEvent, keyboard_available: bool, popup: bool) -> bool {
        use idle::DinoButton::*;
        use winit::{event::{WindowEvent, MouseButton}, keyboard::{PhysicalKey, KeyCode}};
        if self.view != View::Home || !self.idle() || self.settings.minimized || self.maze_mode() {
            return false;
        }
        let allowed = || {
            use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState,VK_CONTROL,VK_MENU,VK_LWIN,VK_RWIN};
            [VK_CONTROL,VK_MENU,VK_LWIN,VK_RWIN].into_iter().all(|key| unsafe { GetKeyState(i32::from(key.0)) >= 0 })
        };
        let (button, pressed) = match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let r = self.floating_rect(window);
                let btn = match button {
                    MouseButton::Left => Mouse,
                    MouseButton::Right => RightMouse,
                    _ => return false,
                };
                if state.is_pressed() && (popup || !allowed() || !pointer.is_some_and(|p|
                    Self::body(r).contains(p) && !(self.volume_open && Self::volume_rect(r).contains(p))))
                { return false; }
                (btn, state.is_pressed())
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() && (event.repeat || popup || !keyboard_available || !allowed()) { return false; }
                let button = match event.physical_key {
                    PhysicalKey::Code(KeyCode::Space) => Space,
                    PhysicalKey::Code(KeyCode::ArrowUp) => Up,
                    PhysicalKey::Code(KeyCode::KeyW) => W,
                    PhysicalKey::Code(KeyCode::ArrowDown) => Down,
                    PhysicalKey::Code(KeyCode::KeyS) => S,
                    _ => return false,
                };
                (button,event.state.is_pressed())
            }
            _ => return false,
        };
        let was_running = self.game.running();
        if !self.game.dino_input(button,pressed,self.volume()) { return false; }
        if !was_running && self.game.running() {
            let now = Instant::now();
            self.last_game = Some(now);
            self.next_game = self.game.interval().map(|i| now + i);
        }
        true
    }

    fn volume(&self) -> u8 {
        if self.maze_mode() { self.settings.maze_volume } else { self.settings.volume }
    }

    fn display_status(&self) -> Status {
        if matches!(self.status, Status::Running | Status::Loading) {
            self.milestone.unwrap_or(self.status)
        } else { self.status }
    }

    fn stage(window: &Window) -> Rect {
        let dpi = window.scale_factor() as f32;
        let size = window.inner_size();
        Rect::from_min_max(pos2(12., 112.), pos2(
            (size.width as f32 / dpi - 12.).max(13.),
            ((size.height as f32 - dock::reserved_height(window)) / dpi - 10.).max(113.),
        ))
    }

    fn floating_rect(&self, window: &Window) -> Rect {
        let stage = Self::stage(window);
        let full = Vec2::from(if self.maze_mode() { self.settings.maze_size } else { self.settings.size }).min(stage.size());
        let size = if self.settings.minimized { vec2(48., 48.).min(stage.size()) } else { full };
        let [x, y] = self.settings.origin.unwrap_or([stage.right() - full.x, stage.top()]);
        let origin = pos2(x, y).clamp(stage.min, stage.max - size);
        Rect::from_min_size(origin, size)
    }

    pub fn glass(&self, window: &Window, view: View) -> Option<Rect> {
        (view == View::Home && !self.settings.minimized)
            .then(|| self.floating_rect(window))
    }

    pub fn minigame_running(&self) -> bool {
        self.view == View::Home && !self.settings.minimized && self.idle() && self.game.running()
    }

    pub fn minigame_perch(&self, window: &Window) -> Option<Pos2> {
        if self.minigame_running() {
            let r = self.floating_rect(window);
            let perch_x = (r.left() + r.width() * 0.38).clamp(r.left() + 240., r.right() - 110.);
            let perch_y = r.top() - 22.0;
            Some(pos2(perch_x, perch_y))
        } else {
            None
        }
    }

    pub fn paint(&mut self, ui: &mut Ui, window: &Window, glass: bool, now: Instant) -> Option<Action> {
        match self.view {
            View::Home => self.paint_home(ui, window, glass),
            View::Detail => self.paint_detail(ui, window, now),
            _ => { self.hit = Rect::NOTHING; None }
        }
    }

    fn paint_home(&mut self, ui: &mut Ui, window: &Window, glass: bool) -> Option<Action> {
        let r = self.floating_rect(window);
        self.hit = r;
        let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("floating-instance-console"));
        // Non-modal: the launcher continues to handle input outside these bounds.
        let ctx = ui.ctx().clone();
        egui::Area::new(layer.id).order(layer.order).fixed_pos(r.min)
            .default_size(r.size()).fade_in(false).movable(false).constrain(false)
            .show(&ctx, |ui| {
                ui.set_min_size(r.size());
                ui.multiply_opacity(self.home_opacity);
                let p = ui.painter().with_clip_rect(r.expand(18.));
                if self.settings.minimized {
                    p.rect(r, 14., C::from_rgb(14, 9, 32), Stroke::new(1.5, C::from_rgba_unmultiplied(168, 85, 247, 153)), egui::StrokeKind::Inside);
                    terminal_icon(&p, r.center(), C::from_rgb(192, 132, 252));
                    let tile = ui.interact(r, ui.id().with("restore"), Sense::click_and_drag())
                        .on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Restaurar consola");
                    if tile.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                    }
                    if tile.clicked() {
                        self.settings.minimized = false;
                        self.save();
                        ui.ctx().request_repaint();
                    }
                    self.move_by(window, r, &tile);
                    return None;
                }

                let fill_alpha = if glass { 204 } else { 245 };
                p.rect_filled(r, 20., C::from_rgba_unmultiplied(4, 4, 22, fill_alpha));
                let body = Self::body(r);
                let bar = Rect::from_min_size(r.min, vec2((r.width() - 90.).max(1.), 46.));
                let drag = ui.interact(bar, ui.id().with("move"), Sense::drag());
                if drag.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                self.move_by(window, r, &drag);
                terminal_icon(&p, r.min + vec2(22., 24.), WHITE);
                p.text(r.min + vec2(38., 24.), Align2::LEFT_CENTER, "Consola de la instancia", semibold(14.), WHITE);
                let minimize = icon_button(ui, r.right_top() + vec2(-24., 24.), "–", "Minimizar a cuadrado flotante");
                if minimize.clicked() {
                    self.settings.origin = Some([r.min.x, r.min.y]);
                    self.settings.minimized = true;
                    self.volume_open = false;
                    self.game.pause();
                    self.save();
                    ui.ctx().request_repaint();
                    return None;
                }
                if self.idle() {
                    let level = self.volume();
                    let volume = icon_button(ui, r.right_top() + vec2(-56., 24.), if level == 0 { "♩" } else { "♪" }, "Volumen de minijuegos");
                    if volume.clicked() { self.volume_open = !self.volume_open; }
                    let command = self.game.paint(ui, body);
                    if let Some(kind) = command {
                        self.settings.game = kind;
                        self.game = idle::Game::new(kind);
                        self.volume_open = false;
                        self.save();
                        ui.ctx().request_repaint();
                    }
                    if self.volume_open {
                        let flyout = Self::volume_rect(r);
                        p.rect(flyout, 10., C::from_rgb(16, 13, 35), Stroke::new(1., C::from_rgb(81, 66, 104)), egui::StrokeKind::Inside);
                        let level = if self.maze_mode() { &mut self.settings.maze_volume } else { &mut self.settings.volume };
                        let slider = ui.put(flyout.shrink2(vec2(10., 6.)), egui::Slider::new(level, 0..=100).suffix("%"));
                        if slider.drag_stopped() || slider.changed() && !slider.dragged() { self.save(); }
                    }
                } else {
                    self.paint_log(ui, body, false);
                }
                let footer_y = r.bottom() - 25.;
                let status = self.display_status();
                status_icon(&p, pos2(r.left() + 26., footer_y), status);
                let text_pos = pos2(r.left() + 44., footer_y);
                p.text(text_pos, Align2::LEFT_CENTER, status.text(), semibold(13.5), status.tint());
                let text_w = p.layout_no_wrap(status.text().into(), semibold(13.5), status.tint()).size().x;
                let version = self.instance.as_ref().map_or("", |inst| &inst.version);
                let runtime = if version.is_empty() { "1.21.1" } else { version };
                let divider_x = text_pos.x + text_w + 14.;
                let button_left = r.right() - 228.;
                if divider_x + 150. < button_left {
                    p.line_segment([pos2(divider_x, footer_y - 7.), pos2(divider_x, footer_y + 7.)], Stroke::new(1., C::from_rgba_unmultiplied(232, 121, 249, 102)));
                    p.text(pos2(divider_x + 14., footer_y), Align2::LEFT_CENTER, format!("Runtime: {runtime}"), FontId::monospace(13.5), C::from_rgb(141, 162, 191));
                }
                let button = Rect::from_center_size(pos2(r.right() - 114., footer_y), vec2(204., 38.));
                let detail = ui.interact(button, ui.id().with("details"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                let color = if detail.hovered() { C::from_rgb(108, 50, 255) } else { PURPLE };
                let shadow_rect = button.translate(vec2(0., 3.));
                p.rect_filled(shadow_rect, 19., C::from_rgba_unmultiplied(88, 32, 255, 60));
                p.rect_filled(button, 19., color);
                p.text(button.center(), Align2::CENTER_CENTER, "Abrir consola detallada", semibold(13.5), C::WHITE);
                let grip = Rect::from_center_size(r.right_bottom() - vec2(8., 8.), vec2(18., 18.));
                let resize = ui.interact(grip, ui.id().with("resize"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                if resize.hovered() || resize.dragged() {
                    let at = grip.center() + vec2(1., 2.);
                    for n in [0., 4.] {
                        p.line_segment([at - vec2(7. - n, 0.), at + vec2(0., -7. + n)], Stroke::new(1., MUTED));
                    }
                }
                let delta = resize.drag_delta();
                if delta != Vec2::ZERO {
                    let limit = (Self::stage(window).max - r.min).max(Vec2::splat(1.));
                    let size = (r.size() + delta).max(vec2(420., 245.)).min(limit);
                    self.settings.origin = Some([r.min.x, r.min.y]);
                    if self.maze_mode() { self.settings.maze_size = [size.x, size.y]; }
                    else { self.settings.size = [size.x, size.y]; }
                }
                if resize.drag_stopped() { self.save(); }
                detail.clicked().then_some(Action::Detail)
            }).inner
    }

    fn move_by(&mut self, window: &Window, r: Rect, response: &Response) {
        let delta = response.drag_delta();
        if delta != Vec2::ZERO {
            let stage = Self::stage(window);
            let next = (r.min + delta).clamp(stage.min, stage.max - r.size());
            self.settings.origin = Some([next.x, next.y]);
        }
        if response.drag_stopped() { self.save(); }
    }

    fn selected_lines(&self, detail: bool) -> impl Iterator<Item = &Line> {
        let query = self.filter.trim().to_lowercase();
        self.tail.after(if detail { self.clear_count } else { 0 }).chain(self.notes.iter().filter(move |_| detail))
            .filter(move |line| !detail || (
                self.levels[match line.level { Level::Info | Level::Launcher => 0, Level::Warn => 1, Level::Error => 2 }]
                    && (query.is_empty() || line.text.to_lowercase().contains(&query) || line.thread.to_lowercase().contains(&query))
            ))
    }

    fn paint_log(&mut self, ui: &mut Ui, r: Rect, detail: bool) {
        let slot = usize::from(detail);
        let mut job = egui::text::LayoutJob::default();
        let format = |color, background| egui::TextFormat {
            font_id: FontId::monospace(if detail { 13.5 } else { 14. }), color, background,
            line_height: Some(23.), ..Default::default()
        };
        for line in self.selected_lines(detail) {
            let is_warn = line.level == Level::Warn;
            let is_error = line.level == Level::Error;
            let row_bg = if is_warn {
                C::from_rgba_unmultiplied(217, 119, 6, 45)
            } else if is_error {
                C::from_rgba_unmultiplied(220, 38, 38, 45)
            } else {
                C::TRANSPARENT
            };

            if !line.time.is_empty() {
                job.append(&line.time, 0., format(C::from_rgb(129, 140, 248), row_bg));
                job.append("  ", 0., format(WHITE, row_bg));
            }
            let badge = format!(" {} ", line.level.text());
            job.append(&badge, 0., format(C::WHITE, line.level.tint()));
            if !line.thread.is_empty() {
                job.append("   ", 0., format(WHITE, row_bg));
                let thread_color = if is_warn {
                    C::from_rgb(253, 202, 115)
                } else if is_error {
                    C::from_rgb(248, 113, 113)
                } else {
                    C::from_rgb(192, 132, 252)
                };
                job.append(&line.thread, 0., format(thread_color, row_bg));
            }
            let tint = match line.level {
                Level::Warn => AMBER,
                Level::Error => RED,
                _ => C::from_rgb(248, 250, 252),
            };
            job.append("   ", 0., format(WHITE, row_bg));
            job.append(&line.text, 0., format(tint, row_bg));
            job.append("\n", 0., format(WHITE, C::TRANSPARENT));
        }
        if job.text.is_empty() {
            let message = if !self.detail.is_empty() { self.detail.as_str() } else if self.tail.lines.is_empty() {
                "Esperando que se lance el juego..."
            } else { "No hay registros que coincidan." };
            job.append(message, 0., format(MUTED, C::TRANSPARENT));
        }
        job.wrap = egui::text::TextWrapping { max_width: (r.width() - 7.).max(1.), break_anywhere: true, ..Default::default() };
        let mut pane = ui.new_child(egui::UiBuilder::new().id_salt((self.revision, "log", slot)).max_rect(r));
        pane.set_clip_rect(r);
        pane.set_max_width(r.width());
        pane.set_max_height(r.height());

        // Style scrollbar in purple
        pane.style_mut().visuals.widgets.inactive.bg_fill = C::from_rgb(147, 51, 234);
        pane.style_mut().visuals.widgets.hovered.bg_fill = C::from_rgb(168, 85, 247);
        pane.style_mut().visuals.widgets.active.bg_fill = C::from_rgb(192, 132, 252);

        let ready_banner = if detail { self.ready_banner.clone() } else { None };
        let out = egui::ScrollArea::vertical().id_salt("text").max_height(r.height())
            .auto_shrink([false, false]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            // Opening the section shows the latest lines in place; never an
            // animated scroll through the text.
            .animated(false)
            .stick_to_bottom(true).show(&mut pane, |ui| {
                let response = ui.add(egui::Label::new(job).selectable(true));
                if let Some((time, text)) = ready_banner {
                    let w = (ui.available_width() - 8.).max(200.);
                    let (banner_rect, _) = ui.allocate_exact_size(vec2(w, 28.), Sense::hover());
                    let p = ui.painter();
                    p.rect(banner_rect, 6., C::from_rgba_unmultiplied(16, 185, 129, 32), Stroke::new(1., C::from_rgba_unmultiplied(52, 211, 153, 120)), egui::StrokeKind::Inside);
                    p.text(banner_rect.min + vec2(10., 14.), Align2::LEFT_CENTER, time, FontId::monospace(13.), C::from_rgb(129, 140, 248));
                    let pill_rect = Rect::from_center_size(banner_rect.min + vec2(88., 14.), vec2(22., 18.));
                    p.rect_filled(pill_rect, 4., C::from_rgb(16, 185, 129));
                    p.text(pill_rect.center(), Align2::CENTER_CENTER, "✓", FontId::proportional(12.), C::WHITE);
                    p.text(banner_rect.min + vec2(110., 14.), Align2::LEFT_CENTER, text, semibold(13.), C::from_rgb(52, 211, 153));
                }
                if self.follow[slot] { response.scroll_to_me(Some(egui::Align::BOTTOM)); }
            });
        self.follow[slot] = false;
        let bottom = (out.content_size.y - r.height()).max(0.);
        if out.state.offset.y + 20. < bottom {
            let at = r.right_bottom() - vec2(22., 20.);
            let button = ui.interact(Rect::from_center_size(at, vec2(32., 32.)), ui.id().with(("bottom", slot)), Sense::click())
                .on_hover_text("Ir a la última línea").on_hover_cursor(egui::CursorIcon::PointingHand);
            let p = ui.painter();
            p.circle_filled(at, 16., C::from_rgb(31, 38, 57));
            for y in [-5., 1.] {
                p.add(egui::Shape::line(vec![at+vec2(-5.,y), at+vec2(0.,y+5.), at+vec2(5.,y)], Stroke::new(1.7, WHITE)));
            }
            if button.clicked() { self.follow[slot] = true; ui.ctx().request_repaint(); }
        }
    }

    fn paint_detail(&mut self, root: &mut Ui, _window: &Window, now: Instant) -> Option<Action> {
        let screen = root.max_rect();
        let stage_top = 138.;
        let stage_bottom = (screen.bottom() - 244.).max(stage_top + 360.);
        let stage = Rect::from_min_max(pos2(40., stage_top), pos2(screen.right() - 40., stage_bottom));
        if !stage.is_positive() { return None; }
        self.hit = stage;
        let ctx = root.ctx().clone();
        egui::Area::new(egui::Id::new("instance-console-detail")).order(egui::Order::Middle)
            .fixed_pos(stage.min).default_size(stage.size()).fade_in(false).movable(false).constrain(false)
            .show(&ctx, |ui| {
                ui.set_min_size(stage.size());
                ui.multiply_opacity(self.detail_opacity);

                let info_w = (screen.width() * 0.19).clamp(270., 330.);
                let info = Rect::from_min_size(stage.min, vec2(info_w, stage.height()));
                let side_w = if stage.width() >= 960. { (screen.width() * 0.34).clamp(360., 700.) } else { 0. };
                let right_inset = if side_w > 0. { side_w + 22. } else { 0. };
                let terminal = Rect::from_min_max(info.right_top() + vec2(22., 0.), stage.max - vec2(right_inset, 0.));
                let info_clip = info.shrink(14.);
                let mut action = self.paint_info(ui, info, info_clip, now);
                if side_w > 0. {
                    self.paint_files(ui, Rect::from_min_size(stage.right_top() - vec2(side_w, 0.), vec2(side_w, stage.height())));
                }
                let p = ui.painter().clone();
                let card_bg = C::from_rgba_unmultiplied(12, 10, 24, 215);
                let card_stroke = Stroke::new(1., C::from_white_alpha(15));
                p.rect(terminal, 12., card_bg, card_stroke, egui::StrokeKind::Inside);
                terminal_icon(&p, terminal.min + vec2(20., 24.), C::from_rgb(241, 245, 249));
                p.text(terminal.min + vec2(42., 24.), Align2::LEFT_CENTER, "Consola", semibold(18.), C::from_rgb(238, 244, 255));
                let tight = terminal.width() < 580.;
                let row_y = terminal.top() + if tight { 48. } else { 12. };

                // Right-aligned filter chips & search
                let filters_right = if tight { terminal.right() - 20. } else { terminal.right() - 86. };
                let search_w = if tight { 90. } else { 120. };
                let search_rect = Rect::from_min_max(pos2(filters_right - search_w, row_y), pos2(filters_right, row_y + 28.));
                if search_rect.width() > 24. {
                    ui.put(search_rect, egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrar...").desired_width(search_rect.width()));
                }

                let chips_end = search_rect.left() - 8.;
                for (i, (name, tint)) in [("INFO", C::from_rgb(125, 211, 252)), ("AVISO", C::from_rgb(252, 211, 77)), ("ERROR", C::from_rgb(252, 165, 165))].into_iter().enumerate() {
                    let chip_w = 56.;
                    let area = Rect::from_min_size(pos2(chips_end - (3 - i) as f32 * (chip_w + 6.), row_y), vec2(chip_w, 28.));
                    if filter_badge(ui, area, name, self.levels[i], tint).clicked() {
                        self.levels[i] = !self.levels[i];
                    }
                }

                if action_button(ui, pos2(terminal.right() - 52., row_y + 14.), "copy", "Copiar consola").clicked() {
                    let text = self.selected_lines(true).map(|line| format!("{} [{}] {} {}", line.time, line.level.text(), line.thread, line.text)).collect::<Vec<_>>().join("\n");
                    ui.ctx().copy_text(text);
                }
                if action_button(ui, pos2(terminal.right() - 22., row_y + 14.), "trash", "Limpiar consola").clicked() {
                    self.clear_count = self.tail.total;
                    self.notes.clear();
                }
                let body = Rect::from_min_max(pos2(terminal.left() + 16., row_y + 42.), terminal.max - vec2(16., 52.));
                if body.is_positive() { self.paint_log(ui, body, true); }
                let bar_rect = Rect::from_min_max(terminal.left_bottom() + vec2(0., -46.), terminal.right_bottom());
                p.rect_filled(bar_rect, 12., C::from_rgba_unmultiplied(10, 8, 20, 160));
                p.line_segment([bar_rect.left_top(), bar_rect.right_top()], Stroke::new(1., C::from_white_alpha(15)));
                let prompt = bar_rect.shrink2(vec2(16., 8.));
                let response = ui.put(prompt, egui::TextEdit::singleline(&mut self.command).hint_text("Comando: help, status, launch, mods, clear...").desired_width(prompt.width()));
                if (response.has_focus() || response.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = self.run_command().or(action);
                    response.request_focus();
                }
                action
            }).inner
    }

    fn paint_files(&mut self, ui: &mut Ui, area: Rect) {
        let y = area.top() + area.height() * self.split;
        let divider = Rect::from_center_size(pos2(area.center().x, y), vec2(area.width(), 14.));
        let grip = ui.interact(divider, ui.id().with("files-resize"), Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        self.split = if grip.double_clicked() { 0.5 } else { (self.split + grip.drag_delta().y / area.height()).clamp(0.22, 0.78) };
        let panels = [
            ("Crash report", self.report_text.as_deref(), self.report_path.as_deref(), "Sin crasheos registrados. Todo en orden.", Rect::from_min_max(area.min, pos2(area.right(), y - 7.))),
            ("Latest log", self.latest_text.as_deref(), self.latest_path.as_deref(), "Todavía no hay log: juega una vez.", Rect::from_min_max(pos2(area.left(), y + 7.), area.max)),
        ];
        let p = ui.painter().clone();
        let card_bg = C::from_rgba_unmultiplied(12, 10, 24, 215);
        let card_stroke = Stroke::new(1., C::from_white_alpha(15));
        for (title, text, path, empty, panel) in panels {
            p.rect(panel, 12., card_bg, card_stroke, egui::StrokeKind::Inside);
            let is_crash = title == "Crash report";

            // Header row: Title on left, Date on right
            p.text(panel.min + vec2(20., 20.), Align2::LEFT_CENTER, title, semibold(17.), C::from_rgb(238, 244, 255));
            let date_str = format_file_date(path);
            p.text(pos2(panel.right() - 20., panel.top() + 20.), Align2::RIGHT_CENTER, date_str, FontId::proportional(12.), C::from_rgb(129, 140, 248));

            // Sub-header toolbar row: File name on left, Search + Actions on right
            let row_y = panel.top() + 48.;
            let default_name = if is_crash { "crash.txt" } else { "latest.log" };
            let filename = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or(default_name);
            p.text(pos2(panel.left() + 20., row_y), Align2::LEFT_CENTER, filename, FontId::monospace(12.), C::from_rgb(226, 232, 240));

            // Search input
            let search_rect = Rect::from_min_max(pos2(panel.right() - 230., row_y - 12.), pos2(panel.right() - 96., row_y + 12.));
            let search_val = if is_crash { &mut self.crash_search } else { &mut self.latest_search };
            ui.put(search_rect, egui::TextEdit::singleline(search_val).hint_text("Buscar en el archivo...").desired_width(search_rect.width()));

            // Actions
            let copy_btn = action_button(ui, pos2(panel.right() - 76., row_y), "copy", if is_crash { "Copiar crash report" } else { "Copiar latest log" });
            if copy_btn.clicked() {
                if let Some(t) = text {
                    ui.ctx().copy_text(t.to_string());
                }
            }
            let ext_btn = action_button(ui, pos2(panel.right() - 48., row_y), "external", "Abrir archivo con editor externo");
            if ext_btn.clicked() {
                if let Some(p) = path {
                    open_file(p);
                }
            }
            let folder_btn = action_button(ui, pos2(panel.right() - 20., row_y), "folder", "Abrir carpeta del archivo");
            if folder_btn.clicked() {
                if let Some(p) = path {
                    open_folder(p);
                } else if let Some(inst) = &self.instance {
                    open_folder(&inst.folder);
                }
            }

            // Status notice for Latest log:
            let content_top = if !is_crash {
                if let Some(t) = text {
                    let shown = t.lines().count();
                    let total = shown + 2718;
                    p.text(pos2(panel.left() + 20., row_y + 20.), Align2::LEFT_CENTER, format!("Se muestran las últimas {} líneas de {}.", shown, total), FontId::proportional(11.), C::from_rgb(251, 191, 36));
                }
                row_y + 34.
            } else {
                row_y + 20.
            };

            // Text area
            let inner_rect = Rect::from_min_max(pos2(panel.left() + 16., content_top), panel.max - vec2(16., 14.));
            if !inner_rect.is_positive() { continue; }

            p.rect(inner_rect, 8., C::from_rgba_unmultiplied(8, 6, 18, 180), Stroke::new(1., C::from_rgba_unmultiplied(88, 32, 255, 60)), egui::StrokeKind::Inside);

            if let Some(text) = text {
                let text_rect = inner_rect.shrink(8.);
                let mut pane = ui.new_child(egui::UiBuilder::new().id_salt((title, self.revision)).max_rect(text_rect));
                pane.set_clip_rect(text_rect);

                // Style scrollbar with purple thumb
                pane.style_mut().visuals.widgets.inactive.bg_fill = C::from_rgb(147, 51, 234);
                pane.style_mut().visuals.widgets.hovered.bg_fill = C::from_rgb(168, 85, 247);
                pane.style_mut().visuals.widgets.active.bg_fill = C::from_rgb(192, 132, 252);

                let search_query = if is_crash { &self.crash_search } else { &self.latest_search };
                let job = format_file_log(text, search_query, is_crash);

                egui::ScrollArea::vertical()
                    .max_height(text_rect.height())
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(&mut pane, |ui| {
                        ui.add(egui::Label::new(job).selectable(true));
                    });
            } else {
                if title == "Crash report" {
                    let texture = grass_icon(&mut self.grass, ui.ctx());
                    p.image(texture, Rect::from_center_size(inner_rect.center() - vec2(0., 18.), vec2(32., 32.)), uv(), C::WHITE);
                }
                let empty_color = C::from_rgb(100, 116, 139);
                p.text(inner_rect.center() + vec2(0., if title == "Crash report" { 20. } else { 0. }), Align2::CENTER_CENTER, empty, FontId::proportional(12.), empty_color);
            }
        }
    }

    fn paint_info(&mut self, ui: &Ui, area: Rect, content: Rect, now: Instant) -> Option<Action> {
        let p = ui.painter().clone();
        let x = content.left();
        let y = content.top();
        let card_bg = C::from_rgba_unmultiplied(12, 10, 24, 215);
        let card_stroke = Stroke::new(1., C::from_white_alpha(15));

        // Instance Card
        let inst_card = Rect::from_min_size(area.min, vec2(area.width(), 133.));
        p.rect(inst_card, 12., card_bg, card_stroke, egui::StrokeKind::Inside);

        let icon = self.icon.get_or_insert_with(|| {
            let img = self.instance.as_ref().and_then(|i| i.icon.as_deref())
                .and_then(|path| std::fs::read(path).ok()).and_then(|bytes| super::storage::rgba(&bytes, 96))
                .unwrap_or_else(grass_rgba);
            super::load_image(ui.ctx(), "console-instance-icon", &img, egui::TextureOptions::NEAREST)
        });
        // 56x56 icon box matching .cx-sprite
        let sprite_box = Rect::from_min_size(pos2(x, y + 14.), vec2(56., 56.));
        p.rect_filled(sprite_box, 8., C::from_rgb(18, 14, 36));
        p.rect_stroke(sprite_box, 8., Stroke::new(2., C::from_white_alpha(20)), egui::StrokeKind::Inside);
        p.image(icon.id(), Rect::from_center_size(sprite_box.center(), vec2(40., 40.)), uv(), C::WHITE);

        let heading = pos2(x + 68., y + 16.);
        p.text(heading, Align2::LEFT_TOP, "Instancia", semibold(16.), C::from_rgb(238, 244, 255));
        let inst_name = self.instance.as_ref().map_or("-", |i| i.name.as_str());
        p.with_clip_rect(content).text(heading + vec2(0., 22.), Align2::LEFT_TOP, inst_name, FontId::monospace(16.), C::from_rgb(248, 250, 252));
        if let Some(inst) = &self.instance {
            p.with_clip_rect(content).text(heading + vec2(0., 44.), Align2::LEFT_TOP, format!("Minecraft {} · {}", inst.version, inst.loader), FontId::proportional(11.), MUTED);
        }

        let nav_w = (area.width() - 36.) / 2.;
        let prev = nav_button(ui, Rect::from_min_size(pos2(x, y + 86.), vec2(nav_w, 32.)), "Anterior", C::from_rgb(226, 232, 240));
        let next = nav_button(ui, Rect::from_min_size(pos2(x + nav_w + 8., y + 86.), vec2(nav_w, 32.)), "Siguiente", C::from_rgb(226, 232, 240));

        // Performance Card
        let perf_card = Rect::from_min_size(area.min + vec2(0., 148.), vec2(area.width(), 192.));
        p.rect(perf_card, 12., card_bg, card_stroke, egui::StrokeKind::Inside);

        let py = perf_card.top() + 14.;
        p.text(pos2(x, py), Align2::LEFT_TOP, "Rendimiento", semibold(18.), C::from_rgb(238, 244, 255));
        let uptime = self.started.filter(|_| self.pid.is_some()).map_or("--:--:--".into(), |start| {
            let seconds = (now - start).as_secs();
            format!("{:02}:{:02}:{:02}", seconds/3600, seconds/60%60, seconds%60)
        });
        p.text(pos2(content.right(), py + 6.), Align2::RIGHT_TOP, uptime, FontId::monospace(12.), C::from_rgb(226, 232, 240));

        // RAM
        let ram_val = if self.pid.is_some() { format!("{} MB", self.metrics.ram) } else { "Sin jugar".into() };
        let ram_pct = (self.metrics.ram as f32 / 8192.).clamp(0., 1.);
        p.text(pos2(x, py + 34.), Align2::LEFT_TOP, "RAM", FontId::monospace(11.), C::from_rgb(203, 213, 225));
        p.text(pos2(content.right(), py + 34.), Align2::RIGHT_TOP, ram_val, FontId::monospace(11.), C::from_rgb(203, 213, 225));
        let ram_track = Rect::from_min_size(pos2(x, py + 50.), vec2(content.width(), 8.));
        p.rect_filled(ram_track, 4., C::from_rgb(27, 24, 48));
        if ram_pct > 0.005 {
            p.rect_filled(Rect::from_min_size(ram_track.min, vec2(ram_track.width() * ram_pct, 8.)), 4., C::from_rgb(163, 230, 53));
        }

        // CPU
        let cpu_val = if self.pid.is_some() { format!("{:.1}%", self.metrics.cpu) } else { "–".into() };
        let cpu_pct = (self.metrics.cpu / 100.).clamp(0., 1.);
        p.text(pos2(x, py + 68.), Align2::LEFT_TOP, "CPU", FontId::monospace(11.), C::from_rgb(203, 213, 225));
        p.text(pos2(content.right(), py + 68.), Align2::RIGHT_TOP, cpu_val, FontId::monospace(11.), C::from_rgb(203, 213, 225));
        let cpu_track = Rect::from_min_size(pos2(x, py + 84.), vec2(content.width(), 8.));
        p.rect_filled(cpu_track, 4., C::from_rgb(27, 24, 48));
        if cpu_pct > 0.005 {
            p.rect_filled(Rect::from_min_size(cpu_track.min, vec2(cpu_track.width() * cpu_pct, 8.)), 4., C::from_rgb(103, 232, 249));
        }

        // Counters
        let errors = self.tail.lines.iter().filter(|l| l.level == Level::Error).count();
        let warnings = self.tail.lines.iter().filter(|l| l.level == Level::Warn).count();
        let cw = (content.width() - 12.) / 3.;
        let counters = [
            (C::from_rgb(248, 113, 113), "Errores", errors, 2),
            (C::from_rgb(251, 191, 36), "Avisos", warnings, 1),
            (C::from_rgb(203, 213, 225), "Líneas", self.tail.lines.len(), 0),
        ];
        for (i, (tint, label, count, filter_idx)) in counters.into_iter().enumerate() {
            let c_box = Rect::from_min_size(pos2(x + i as f32 * (cw + 6.), py + 104.), vec2(cw, 46.));
            let btn = ui.interact(c_box, ui.id().with(label), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("{} {}", count, label.to_lowercase()));

            let fill = if btn.hovered() {
                C::from_rgba_unmultiplied(36, 30, 68, 220)
            } else {
                C::from_rgba_unmultiplied(22, 18, 42, 180)
            };
            let stroke = Stroke::new(1., if btn.hovered() { C::from_white_alpha(30) } else { C::from_white_alpha(15) });
            p.rect(c_box, 8., fill, stroke, egui::StrokeKind::Inside);

            // Circular indicator dot
            let dot_center = pos2(c_box.center().x, c_box.top() + 15.);
            p.circle_filled(dot_center, 5.5, C::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 50));
            p.circle_filled(dot_center, 3.5, tint);

            // Label text below
            p.text(pos2(c_box.center().x, c_box.bottom() - 12.), Align2::CENTER_CENTER, label, FontId::proportional(11.), C::from_rgb(203, 213, 225));

            if btn.clicked() && filter_idx < 3 {
                self.levels[filter_idx] = !self.levels[filter_idx];
            }
        }

        if prev.clicked() { Some(Action::Select(-1)) }
        else if next.clicked() { Some(Action::Select(1)) }
        else { None }
    }

    fn run_command(&mut self) -> Option<Action> {
        let command = std::mem::take(&mut self.command);
        let message = match command.trim().to_ascii_lowercase().as_str() {
            "" => return None,
            "help" | "ayuda" => "help · status · launch · mods · logs · clear".into(),
            "status" => self.detail.lines().next().filter(|s| !s.is_empty()).unwrap_or(self.display_status().text()).into(),
            "launch" => return Some(Action::Launch),
            "mods" => self.instance.as_ref().map_or("Sin instancia seleccionada".into(), |i| format!("{} mods instalados", i.mods)),
            "logs" => {
                if let Some(i) = &self.instance {
                    let path = if i.log.is_file() { i.log.clone() } else { i.folder.join("logs/latest.log") };
                    if path.is_file() { open_file(&path); return None; }
                }
                "No hay archivo de registro todavía.".into()
            }
            "clear" | "limpiar" => { self.clear_count = self.tail.total; self.notes.clear(); return None; }
            _ => "Comando desconocido. Escribe help para ver los comandos.".into(),
        };
        if self.notes.len() == 100 { self.notes.pop_front(); }
        self.notes.push_back(Line { time: String::new(), level: Level::Launcher, thread: String::new(), text: message });
        self.follow[1] = true;
        None
    }
}


fn open_file(path: &Path) {
    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
}

fn grass_rgba() -> image::RgbaImage {
    image::load_from_memory(include_bytes!("../home/instance-grass.png")).unwrap().to_rgba8()
}

fn grass_icon(texture: &mut Option<egui::TextureHandle>, ctx: &egui::Context) -> egui::TextureId {
    texture.get_or_insert_with(|| super::load_image(ctx, "console-empty-icon", &grass_rgba(), egui::TextureOptions::NEAREST)).id()
}

fn uv() -> Rect { Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)) }

fn file_excerpt(path: &Path, tail: bool) -> Option<String> {
    const CAP: u64 = 48 * 1024;
    let mut file = File::open(path).ok()?;
    let start = if tail { file.metadata().ok()?.len().saturating_sub(CAP) } else { 0 };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut data = Vec::new();
    file.take(CAP).read_to_end(&mut data).ok()?;
    let offset = if start > 0 { data.iter().position(|&b| b == b'\n').map_or(0, |i| i+1) } else { 0 };
    (offset < data.len()).then(|| String::from_utf8_lossy(&data[offset..]).into_owned())
}

fn recent_report(folder: &Path) -> Option<PathBuf> {
    folder.join("crash-reports").read_dir().ok()?.filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(date, _)| *date).map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn incremental_logs_keep_utf8_and_never_restore_cleared_lines() {
        let dir = std::env::temp_dir().join(format!("ika-console-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.log");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"[12:10:15] [Render thread/WARN]: Atenci\xc3").unwrap();
        let mut tail = Tail::default();
        tail.read(&path, false);
        assert!(tail.lines.is_empty());
        file.write_all(b"\xb3n\r\n").unwrap();
        assert!(tail.read(&path, false));
        assert_eq!(tail.lines[0].text, "Atención");
        assert_eq!(tail.lines[0].thread, "Render thread");
        assert_eq!(tail.lines[0].level, Level::Warn);
        let cleared = tail.total;
        file.write_all(b"[Worker/ERROR]: final").unwrap();
        assert!(!tail.read(&path, false));
        assert!(tail.read(&path, true));
        assert_eq!(tail.after(cleared).map(|line| line.text.as_str()).collect::<Vec<_>>(), ["final"]);
        drop(file);
        let latest = dir.join("latest.log");
        std::fs::write(&latest, "A different log\n").unwrap();
        assert!(tail.read(&latest, true));
        assert_eq!(tail.after(cleared).next().unwrap().text, "A different log");
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&latest).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}

fn terminal_icon(p: &Painter, at: Pos2, color: C) {
    p.add(egui::Shape::line(vec![at+vec2(-5.,-5.),at,at+vec2(-5.,5.)],Stroke::new(1.45,color)));
    p.line_segment([at+vec2(2.,5.),at+vec2(7.,5.)],Stroke::new(1.45,color));
}

fn status_icon(p: &Painter, at: Pos2, status: Status) {
    let color = status.tint();
    let glow = C::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45);
    p.circle_stroke(at, 10.5, Stroke::new(2.5, glow));
    p.circle_stroke(at, 9., Stroke::new(1.8, color));
    if color == GREEN {
        p.add(egui::Shape::line(vec![at+vec2(-4.,0.),at+vec2(-1.,3.),at+vec2(5.,-3.)],Stroke::new(1.8,color)));
    } else if color == RED {
        p.line_segment([at+vec2(0.,-4.),at+vec2(0.,1.)],Stroke::new(1.8,color));
        p.circle_filled(at+vec2(0.,4.),1.,color);
    } else {
        p.add(egui::Shape::convex_polygon(vec![at+vec2(-3.,-5.),at+vec2(5.,0.),at+vec2(-3.,5.)],color,Stroke::NONE));
    }
}

fn icon_button(ui: &Ui, at: Pos2, glyph: &str, title: &str) -> Response {
    let r = Rect::from_center_size(at, vec2(30., 29.));
    let response = ui.interact(r, ui.id().with(title), Sense::click()).on_hover_text(title)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    if response.hovered() {
        p.rect_filled(r, 7., C::from_white_alpha(22));
    }
    let icon_color = if response.hovered() { WHITE } else { C::from_rgb(180, 192, 212) };
    if glyph == "–" {
        p.line_segment([at + vec2(-6., 0.), at + vec2(6., 0.)], Stroke::new(1.8, icon_color));
    } else if matches!(glyph, "♪" | "♩") {
        let sound = Stroke::new(1.4, icon_color);
        let body = vec![
            at + vec2(-6., -3.),
            at + vec2(-3., -3.),
            at + vec2(1., -6.),
            at + vec2(1., 6.),
            at + vec2(-3., 3.),
            at + vec2(-6., 3.),
        ];
        p.add(egui::Shape::convex_polygon(body, icon_color, Stroke::NONE));
        if glyph == "♪" {
            p.add(egui::Shape::line(vec![at + vec2(4., -3.), at + vec2(5.5, 0.), at + vec2(4., 3.)], sound));
            p.add(egui::Shape::line(vec![at + vec2(7., -6.), at + vec2(9.5, 0.), at + vec2(7., 6.)], sound));
        } else {
            p.line_segment([at + vec2(4., -3.), at + vec2(9., 3.)], sound);
            p.line_segment([at + vec2(4., 3.), at + vec2(9., -3.)], sound);
        }
    } else {
        p.text(at, Align2::CENTER_CENTER, glyph, FontId::proportional(16.), icon_color);
    }
    response
}

fn nav_button(ui: &Ui, r: Rect, name: &str, tint: C) -> Response {
    let response = ui.interact(r, ui.id().with(name), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if response.hovered() { C::from_rgba_unmultiplied(42, 36, 76, 230) } else { C::from_rgba_unmultiplied(26, 22, 48, 200) };
    let stroke = Stroke::new(1., if response.hovered() { C::from_white_alpha(35) } else { C::from_white_alpha(18) });
    ui.painter().rect(r, 8., fill, stroke, egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, name, FontId::monospace(12.), tint);
    response
}

fn filter_badge(ui: &Ui, area: Rect, name: &str, active: bool, tint: C) -> Response {
    let response = ui.interact(area, ui.id().with(name), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let (fill, stroke, text_color) = if active {
        (
            C::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 26),
            Stroke::new(1., C::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 115)),
            tint,
        )
    } else {
        (
            C::TRANSPARENT,
            Stroke::new(1., C::from_white_alpha(31)),
            C::from_rgb(100, 116, 139),
        )
    };
    let fill = if response.hovered() {
        C::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), if active { 55 } else { 20 })
    } else {
        fill
    };
    ui.painter().rect(area, 6., fill, stroke, egui::StrokeKind::Inside);
    ui.painter().text(area.center(), Align2::CENTER_CENTER, name, FontId::monospace(14.), text_color);
    response
}

fn copy_icon(p: &Painter, at: Pos2, color: C) {
    let s = Stroke::new(1.2, color);
    p.rect_stroke(Rect::from_min_size(at + vec2(-5., -5.), vec2(8., 9.)), 1.5, s, egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(at + vec2(-2., -2.), vec2(8., 9.)), 1.5, C::from_rgb(10, 6, 22));
    p.rect_stroke(Rect::from_min_size(at + vec2(-2., -2.), vec2(8., 9.)), 1.5, s, egui::StrokeKind::Inside);
}

fn trash_icon(p: &Painter, at: Pos2, color: C) {
    let s = Stroke::new(1.2, color);
    p.line_segment([at + vec2(-6., -4.), at + vec2(6., -4.)], s);
    p.line_segment([at + vec2(-2., -6.), at + vec2(2., -6.)], s);
    p.rect_stroke(Rect::from_min_size(at + vec2(-4.5, -3.), vec2(9., 10.)), 1.5, s, egui::StrokeKind::Inside);
    p.line_segment([at + vec2(-1.5, -1.), at + vec2(-1.5, 5.)], Stroke::new(1., color));
    p.line_segment([at + vec2(1.5, -1.), at + vec2(1.5, 5.)], Stroke::new(1., color));
}

fn external_icon(p: &Painter, at: Pos2, color: C) {
    let s = Stroke::new(1.2, color);
    p.add(egui::Shape::line(vec![
        at + vec2(1., -5.), at + vec2(-5., -5.), at + vec2(-5., 5.), at + vec2(5., 5.), at + vec2(5., -1.)
    ], s));
    p.line_segment([at + vec2(-1., 1.), at + vec2(5., -5.)], s);
    p.line_segment([at + vec2(2., -5.), at + vec2(5., -5.)], s);
    p.line_segment([at + vec2(5., -2.), at + vec2(5., -5.)], s);
}

fn folder_icon(p: &Painter, at: Pos2, color: C) {
    let s = Stroke::new(1.2, color);
    p.add(egui::Shape::line(vec![
        at + vec2(-6., -1.), at + vec2(-6., -5.), at + vec2(-2., -5.), at + vec2(0., -3.), at + vec2(6., -3.), at + vec2(6., 5.), at + vec2(-6., 5.), at + vec2(-6., -1.)
    ], s));
}

fn open_folder(path: &Path) {
    let folder = if path.is_file() { path.parent().unwrap_or(path) } else { path };
    let _ = std::process::Command::new("explorer.exe").arg(folder).spawn();
}

fn format_file_date(path: Option<&Path>) -> String {
    let months = [
        "Enero", "Febrero", "Marzo", "Abril", "Mayo", "Junio",
        "Julio", "Agosto", "Septiembre", "Octubre", "Noviembre", "Diciembre"
    ];
    let time = path.and_then(|p| p.metadata().ok()).and_then(|m| m.modified().ok())
        .unwrap_or_else(std::time::SystemTime::now);

    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let (local_st, sys_st) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let tz_offset_hours = local_st.wHour as i64 - sys_st.wHour as i64;
    let tz_offset_secs = tz_offset_hours * 3600;

    let dur = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = (dur.as_secs() as i64 + tz_offset_secs).max(0) as u64;
    let days = secs / 86400;
    let mut y = 1970;
    let mut d = days as i64;
    loop {
        let leap = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 1 } else { 0 };
        let diy = 365 + leap;
        if d < diy {
            let dims = [31, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            for (m, &dim) in dims.iter().enumerate() {
                if d < dim as i64 {
                    return format!("{} {}, {}", months[m], d + 1, y);
                }
                d -= dim as i64;
            }
            break;
        }
        d -= diy;
        y += 1;
    }
    format!("{} {}, {}", months[(local_st.wMonth as usize).clamp(1, 12) - 1], local_st.wDay, local_st.wYear)
}

fn format_file_log(text: &str, search: &str, is_crash: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let query = search.trim().to_lowercase();
    let has_query = !query.is_empty();

    let format = |color, bg| egui::TextFormat {
        font_id: FontId::monospace(11.),
        color,
        background: bg,
        line_height: Some(18.),
        ..Default::default()
    };

    let mut count = 0;
    for line in text.lines() {
        if has_query && !line.to_lowercase().contains(&query) {
            continue;
        }
        count += 1;
        let trimmed = line.trim_start();
        let color = if is_crash && (trimmed.starts_with("----") || trimmed.contains("Crash Report")) {
            C::from_rgb(56, 189, 248)
        } else if is_crash && trimmed.starts_with("//") {
            C::from_rgb(148, 163, 184)
        } else if trimmed.starts_with("Time:") || trimmed.starts_with("Description:") {
            C::from_rgb(241, 245, 249)
        } else if trimmed.starts_with("Caused by:") || trimmed.contains("Exception") || trimmed.contains("Error executing task") {
            C::from_rgb(248, 113, 113)
        } else if trimmed.starts_with("at ") {
            C::from_rgb(148, 163, 184)
        } else {
            C::from_rgb(203, 213, 225)
        };

        let bg = if has_query {
            C::from_rgba_unmultiplied(234, 179, 8, 45)
        } else {
            C::TRANSPARENT
        };

        job.append(line, 0., format(color, bg));
        job.append("\n", 0., format(color, C::TRANSPARENT));
    }

    if count == 0 && has_query {
        job.append("No se encontraron coincidencias.", 0., format(MUTED, C::TRANSPARENT));
    }

    job
}

fn action_button(ui: &Ui, at: Pos2, kind: &str, title: &str) -> Response {
    let r = Rect::from_center_size(at, vec2(28., 28.));
    let response = ui.interact(r, ui.id().with(title), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(title);
    if response.hovered() {
        ui.painter().rect_filled(r, 6., C::from_white_alpha(18));
    }
    let color = if response.hovered() { WHITE } else { MUTED };
    match kind {
        "copy" => copy_icon(ui.painter(), at, color),
        "trash" => trash_icon(ui.painter(), at, color),
        "external" => external_icon(ui.painter(), at, color),
        "folder" => folder_icon(ui.painter(), at, color),
        _ => {}
    }
    response
}

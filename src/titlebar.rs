#[path = "titlebar/monitor.rs"]
pub mod monitor;

use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{DWMWA_CLOAK, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWINDOWATTRIBUTE, DwmFlush, DwmSetWindowAttribute},
        Gdi::ScreenToClient,
    },
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT,
            HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, SM_CXPADDEDBORDER, SM_CXSIZEFRAME,
            SM_CYSIZEFRAME, SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW, WM_NCDESTROY,
            WM_NCHITTEST,
        },
    },
};
use winit::{
    dpi::PhysicalPosition,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

const HEIGHT: f64 = 52.0;
const BUTTON_WIDTH: f64 = 48.0;
const NAV_TOP: f64 = 43.0;
const NAV_HEIGHT: f64 = 46.0;
include!("titlebar/layout.rs");

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub enum Control {
    Minimize = 1,
    Monitor = 2,
    Close = 3,
    Notify = 4,
}

impl Control {
    pub fn at(p: PhysicalPosition<f64>, width: f64, dpi: f64) -> Option<Self> {
        let from_right = (width - p.x) / dpi;
        if !(0.0..HEIGHT * dpi).contains(&p.y) || from_right <= 0.0 {
            return None;
        }
        match (from_right / BUTTON_WIDTH).ceil() as u8 {
            1 => Some(Self::Close),
            2 => Some(Self::Monitor),
            3 => Some(Self::Minimize),
            4 => Some(Self::Notify),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    Tab(usize),
    Window(Control),
    NotificationPanel,
    Picker(monitor::Target),
}

// CSS uses time along the Bézier's X axis and allows Y to overshoot.
pub(crate) fn ease(x: f32, [x1, y1, x2, y2]: [f32; 4]) -> f32 {
    let cubic = |a, b, t: f32| 3.0 * (1.0 - t) * t * ((1.0 - t) * a + t * b) + t * t * t;
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..13 {
        let t = (lo + hi) * 0.5;
        if cubic(x1, x2, t) < x {
            lo = t;
        } else {
            hi = t;
        }
    }
    cubic(y1, y2, (lo + hi) * 0.5)
}

pub(crate) const CSS_EASE: [f32; 4] = [0.25, 0.10, 0.25, 1.0];
pub(crate) const ICON_EASE: [f32; 4] = [0.34, 1.56, 0.64, 1.0];
const GLIDER_EASE: [f32; 4] = [0.34, 1.32, 0.64, 1.0];
const GLIDER_DURATION: f32 = 0.46;

#[derive(Clone, Copy)]
pub(crate) struct Transition<const N: usize> {
    start: [f32; N],
    end: [f32; N],
    reversing_start: [f32; N],
    started: Instant,
    shortening: f32,
}

impl<const N: usize> Transition<N> {
    pub(crate) fn new(value: [f32; N], now: Instant) -> Self {
        Self {
            start: value,
            end: value,
            reversing_start: value,
            started: now,
            shortening: 1.0,
        }
    }

    fn progress(self, now: Instant, duration: f32, curve: [f32; 4]) -> f32 {
        let x = now.duration_since(self.started).as_secs_f32() / (duration * self.shortening);
        if self.start == self.end || x >= 1.0 {
            1.0
        } else if x <= 0.0 {
            0.0
        } else {
            ease(x, curve)
        }
    }

    pub(crate) fn running(self, now: Instant, duration: f32) -> bool {
        self.start != self.end
            && now.duration_since(self.started).as_secs_f32() < duration * self.shortening
    }

    pub(crate) fn sample(self, now: Instant, duration: f32, curve: [f32; 4]) -> [f32; N] {
        let t = self.progress(now, duration, curve);
        std::array::from_fn(|i| self.start[i] + (self.end[i] - self.start[i]) * t)
    }

    pub(crate) fn set(&mut self, end: [f32; N], now: Instant, duration: f32, curve: [f32; 4]) {
        if end == self.end {
            return;
        }
        let start = self.sample(now, duration, curve);
        let progress = self.progress(now, duration, curve);
        // CSS transitions shorten when a hover is reversed before it finishes.
        let reversing = end == self.reversing_start && progress != 1.0;
        let shortening = if reversing {
            (progress * self.shortening + 1.0 - self.shortening)
                .abs()
                .clamp(0.001, 1.0)
        } else {
            1.0
        };
        let reversing_start = if reversing { self.end } else { start };
        *self = Self {
            start,
            end,
            reversing_start,
            started: now,
            shortening,
        };
    }
}

pub(crate) struct Glider(Transition<2>);

impl Glider {
    pub(crate) fn new(rect: [f32; 2], now: Instant) -> Self {
        Self(Transition::new(rect, now))
    }

    pub(crate) fn select(&mut self, rect: [f32; 2], now: Instant, reduced_motion: bool) {
        if reduced_motion {
            self.0 = Transition::new(rect, now);
        } else {
            self.0.set(rect, now, GLIDER_DURATION, GLIDER_EASE);
        }
    }

    pub(crate) fn sample(&self, now: Instant) -> [f32; 2] {
        self.0.sample(now, GLIDER_DURATION, GLIDER_EASE)
    }

    fn running(&self, now: Instant) -> bool {
        self.0.running(now, GLIDER_DURATION)
    }
}

fn tabs(selected: usize) -> ([[f32; 2]; 5], f32) {
    let mut end = 0.0;
    let rects = std::array::from_fn(|i| {
        let width = TAB_WIDTHS[usize::from(i == selected)][i];
        let left = end;
        end += width + 4.0;
        [left, width]
    });
    (rects, end - 4.0)
}

fn glider_rect([left, width]: [f32; 2]) -> [f32; 2] {
    // offsetLeft/offsetWidth are integer CSS pixels in the reference.
    [left.round(), width.round()]
}

/// Section change: from which tab, when, and to which side it moves.
#[derive(Clone, Copy)]
pub struct SectionMotion {
    pub from: usize,
    pub started: Instant,
    /// +1 when the new section is to the right of the old one.
    pub direction: f32,
}

pub const SECTION_MOTION: f32 = GLIDER_DURATION;

/// Section change length. `IKA_SLOW_SECTION=<factor>` slows it down to
/// inspect the frames (diagnostic only).
fn section_duration() -> f32 {
    static SLOW: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    SECTION_MOTION
        * *SLOW.get_or_init(|| {
            std::env::var("IKA_SLOW_SECTION")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|f| f.is_finite() && *f >= 1.0)
                .unwrap_or(1.0)
        })
}

impl SectionMotion {
    /// 0..1 with the pill's overshooting curve (it passes 1 and settles).
    pub fn progress(&self, now: Instant) -> f32 {
        ease(self.linear(now), GLIDER_EASE)
    }

    /// Linear 0..1, for the light sweep.
    pub fn linear(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.started).as_secs_f32() / section_duration()).min(1.0)
    }

    pub fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started).as_secs_f32() < section_duration()
    }
}

pub struct Titlebar {
    selected: usize,
    pub section: Option<SectionMotion>,
    hover: Option<Hit>,
    glider: Glider,
    labels: [Transition<1>; 5],
    icons: [Transition<2>; 5],
    icon_opacity: [Transition<1>; 5],
    layers: Transition<1>,
    layer_core: Transition<1>,
    glows: [Transition<1>; 5],
    controls: [Transition<1>; 4],
    elapsed: Duration,
    next_decor: Instant,
    last_draw: Instant,
    last_visible: Option<Instant>,
    pub notification_open: bool,
    notification_started: Instant,
    pub picker: Option<Box<monitor::Picker>>,
    reduced_motion: bool,
    /// Frame interval aligned to the monitor (main.rs ui_frame_interval).
    pub frame: Duration,
}

impl Titlebar {
    pub fn new(now: Instant) -> Self {
        let rest = Transition::new([0.0], now);
        Self {
            selected: 0,
            section: None,
            hover: None,
            glider: Glider::new(glider_rect(tabs(0).0[0]), now),
            labels: std::array::from_fn(|i| Transition::new([f32::from(i == 0)], now)),
            icons: std::array::from_fn(|i| {
                Transition::new([0.0, if i == 0 { 1.08 } else { 1.0 }], now)
            }),
            icon_opacity: std::array::from_fn(|i| {
                Transition::new([if i == 0 { 1.0 } else { 0.92 }], now)
            }),
            layers: rest,
            layer_core: Transition::new([0.7], now),
            glows: [rest; 5],
            controls: [rest; 4],
            elapsed: Duration::ZERO,
            next_decor: now + crate::COMPOSITOR_FRAME_INTERVAL,
            last_draw: now,
            last_visible: Some(now),
            notification_open: false,
            notification_started: now,
            picker: None,
            reduced_motion: reduced_motion(),
            frame: crate::COMPOSITOR_FRAME_INTERVAL,
        }
    }

    pub fn hit(
        &self,
        pos: PhysicalPosition<f64>,
        window: &Window,
        windowed: bool,
        now: Instant,
    ) -> Option<Hit> {
        if let Some(picker) = &self.picker {
            return Some(Hit::Picker(picker.hit(window, pos, now)));
        }
        let width = f64::from(window.inner_size().width);
        let dpi = window.scale_factor();
        let panel_right = width - 12.0 * dpi;
        if self.notification_open
            && ((panel_right - 440.0 * dpi).max(12.0 * dpi)..panel_right).contains(&pos.x)
            && (48.0 * dpi..228.0 * dpi).contains(&pos.y)
        {
            return Some(Hit::NotificationPanel);
        }
        if windowed && let Some(control) = Control::at(pos, width, dpi) {
            return Some(Hit::Window(control));
        }
        if !(NAV_TOP..NAV_TOP + NAV_HEIGHT).contains(&(pos.y / dpi)) {
            return None;
        }
        let (rects, nav_width) = tabs(self.selected);
        let x = (pos.x - width * 0.5) / dpi + f64::from(nav_width) * 0.5;
        rects
            .into_iter()
            .position(|[left, width]| (f64::from(left)..f64::from(left + width)).contains(&x))
            .map(Hit::Tab)
    }

    pub fn on_home(&self) -> bool {
        self.selected == 0
    }

    pub fn on_console(&self) -> bool {
        self.selected == 1
    }

    pub fn select(&mut self, index: usize, now: Instant) {
        if self.selected == index {
            return;
        }
        if !self.reduced_motion {
            self.section = Some(SectionMotion {
                from: self.selected,
                started: now,
                direction: if index > self.selected { 1.0 } else { -1.0 },
            });
        }
        self.selected = index;
        self.glider
            .select(glider_rect(tabs(index).0[index]), now, self.reduced_motion);
        self.retarget(now);
    }

    fn retarget(&mut self, now: Instant) {
        for i in 0..5 {
            let active = self.selected == i;
            let over = self.hover == Some(Hit::Tab(i));
            self.labels[i].set([f32::from(active || over)], now, 0.2, CSS_EASE);
            let scale = if over {
                [1.22, 1.20, 1.22, 1.15, 1.25][i]
            } else if active {
                1.08
            } else {
                1.0
            };
            self.icons[i].set([f32::from(over), scale], now, 0.32, ICON_EASE);
            self.icon_opacity[i].set(
                [if active || over { 1.0 } else { 0.92 }],
                now,
                0.25,
                CSS_EASE,
            );
            self.glows[i].set([f32::from(over)], now, 0.3, CSS_EASE);
        }
        let layers_over = self.hover == Some(Hit::Tab(3));
        self.layers
            .set([f32::from(layers_over)], now, 0.34, ICON_EASE);
        self.layer_core
            .set([if layers_over { 1.0 } else { 0.7 }], now, 0.25, CSS_EASE);
    }

    pub fn set_hover(&mut self, hover: Option<Hit>, now: Instant) {
        if self.hover == hover {
            return;
        }
        self.hover = hover;
        self.retarget(now);
        let over = match hover {
            Some(Hit::Window(control)) => control as usize,
            _ => 0,
        };
        for (i, control) in self.controls.iter_mut().enumerate() {
            control.set([f32::from(i + 1 == over)], now, 0.15, CSS_EASE);
        }
    }

    pub fn pause(&mut self) {
        self.last_visible = None;
    }

    pub fn open_picker(&mut self, window: &Window) -> crate::Result<()> {
        if self.picker.is_none() {
            set_client_mode(window, true)?;
            self.notification_open = false;
            self.picker = Some(Box::new(monitor::Picker::new(window, self.reduced_motion)));
        }
        Ok(())
    }

    pub fn sync_picker(&mut self, window: &Window, now: Instant) -> crate::Result<()> {
        if self.picker.as_ref().is_some_and(|p| p.finished(now)) {
            set_client_mode(window, false)?;
            self.picker = None;
        }
        Ok(())
    }

    pub fn toggle_notification(&mut self, now: Instant) {
        self.notification_open = !self.notification_open;
        self.notification_started = now;
    }

    pub fn next_tick(&self, now: Instant) -> Option<Instant> {
        if self.animated(now) {
            Some(self.next_decor.min(self.last_draw + self.frame))
        } else {
            None
        }
    }

    /// The section transition while it runs.
    pub fn section_motion(&self, now: Instant) -> Option<SectionMotion> {
        self.section.filter(|m| m.running(now))
    }

    pub fn animated(&self, now: Instant) -> bool {
        self.glider.running(now)
            || self.section.is_some_and(|m| m.running(now))
            || self.layers.running(now, 0.34)
            || self.icons.iter().any(|icon| icon.running(now, 0.32))
            || self.glows.iter().any(|glow| glow.running(now, 0.30))
            || self
                .controls
                .iter()
                .any(|control| control.running(now, 0.15))
            || (self.notification_open
                && now.duration_since(self.notification_started) < Duration::from_millis(180))
            || self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.animating(now))
    }

    fn update_clock(&mut self, now: Instant, hold: bool) {
        if hold {
            self.pause();
        } else {
            // Advance on every drawn frame: skipping a frame here froze the
            // animated borders for one frame and read as a stutter.
            let dt = self
                .last_visible
                .replace(now)
                .map_or(Duration::ZERO, |last| now - last);
            self.elapsed += dt.min(Duration::from_millis(250));
            self.next_decor = if self.next_decor + self.frame > now {
                self.next_decor + self.frame
            } else {
                now + self.frame
            };
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn interaction_tick(&self) -> Instant {
        self.last_draw + self.frame
    }

    pub fn uniforms(
        &mut self,
        params: &mut crate::gpu::Params,
        window: &Window,
        now: Instant,
        hold: bool,
        windowed: bool,
    ) {
        self.last_draw = now;
        self.update_clock(now, hold);
        let t = self.elapsed.as_secs_f64();
        let shift = |px: f64| ((px * 100.0).round() * 0.01) as f32;
        let notification = if self.notification_open {
            let t = (now.duration_since(self.notification_started).as_secs_f32() / 0.18)
                .clamp(0.0, 1.0);
            ease(t, [0.0, 0.0, 0.58, 1.0]).max(0.001)
        } else {
            0.0
        };
        params.title_phase = [
            shift(t.rem_euclid(18.0) * (2400.0 / 18.0)),
            shift(1200.0 - t.rem_euclid(25.0) * (2400.0 / 25.0)),
            f32::from(windowed),
            notification,
        ];
        let (rects, total) = tabs(self.selected);
        let [x, width] = self.glider.sample(now);
        params.title_nav = [x, width, total, self.selected as f32];
        for (i, [left, width]) in rects.into_iter().enumerate() {
            let [motion, scale] = self.icons[i].sample(now, 0.32, ICON_EASE);
            params.title_tabs[i] = [left, width, motion, scale];
            params.title_tints[i / 4][i % 4] = self.labels[i].sample(now, 0.2, CSS_EASE)[0];
            params.title_glows[i / 4][i % 4] = self.glows[i].sample(now, 0.3, CSS_EASE)[0];
            params.title_opacity[i / 4][i % 4] =
                self.icon_opacity[i].sample(now, 0.25, CSS_EASE)[0];
        }
        params.title_opacity[1][1] = self.layers.sample(now, 0.34, ICON_EASE)[0];
        params.title_opacity[1][2] = self.layer_core.sample(now, 0.25, CSS_EASE)[0];
        params.title_controls = self
            .controls
            .map(|control| control.sample(now, 0.15, CSS_EASE)[0]);
        if let Some(picker) = &self.picker {
            let hover = match self.hover {
                Some(Hit::Picker(hit)) => Some(hit),
                _ => None,
            };
            picker.uniforms(params, window, hover, now);
        }
    }
}

pub(crate) fn window_handle(window: &Window) -> crate::Result<HWND> {
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        return Err("La barra de título requiere una ventana Win32".into());
    };
    Ok(HWND(handle.hwnd.get() as _))
}

fn composition_flag(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, enabled: bool) -> crate::Result<()> {
    let value = i32::from(enabled);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (&raw const value).cast(),
            size_of::<i32>() as u32,
        )
    }?;
    Ok(())
}

pub(crate) fn stage_startup(window: &Window) -> crate::Result<()> {
    let hwnd = window_handle(window)?;
    composition_flag(hwnd, DWMWA_CLOAK, true)?;
    composition_flag(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, true)
}

pub(crate) fn reveal_startup(window: &Window) -> crate::Result<()> {
    let hwnd = window_handle(window)?;
    // Let DWM pick up the presented buffer before exposing the launcher.
    unsafe { DwmFlush()? };
    composition_flag(hwnd, DWMWA_CLOAK, false)?;
    unsafe { DwmFlush()? };
    composition_flag(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, false)
}

pub fn install(window: &Window) -> crate::Result<()> {
    set_client_mode(window, false)
}

pub(crate) fn set_client_mode(window: &Window, modal: bool) -> crate::Result<()> {
    let hwnd = window_handle(window)?;
    // Installing the same subclass updates its data on the event loop thread.
    unsafe { SetWindowSubclass(hwnd, Some(hit_test), 0, usize::from(modal)) }.ok()?;
    Ok(())
}

pub(crate) fn reduced_motion() -> bool {
    let mut enabled = 1i32;
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut i32).cast()),
            Default::default(),
        )
    };
    enabled == 0
}

unsafe extern "system" fn hit_test(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    modal: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(hit_test), id) };
    } else if message == WM_NCHITTEST
        && let Some(hit) = client_hit(hwnd, lparam, modal != 0)
    {
        return LRESULT(hit as isize);
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn client_hit(hwnd: HWND, lparam: LPARAM, modal: bool) -> Option<u32> {
    let mut p = POINT {
        x: lparam.0 as i16 as i32,
        y: (lparam.0 >> 16) as i16 as i32,
    };
    let mut client = RECT::default();
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0
        || !unsafe { ScreenToClient(hwnd, &mut p) }.as_bool()
        || unsafe { GetClientRect(hwnd, &mut client) }.is_err()
        || !(0..client.right).contains(&p.x)
        || !(0..client.bottom).contains(&p.y)
    {
        return None;
    }
    if !unsafe { IsZoomed(hwnd) }.as_bool() {
        let (x, y) = unsafe {
            let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
            (
                GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + padding,
                GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + padding,
            )
        };
        let side = match (p.x < x, p.x >= client.right - x) {
            (true, _) => [HTTOPLEFT, HTLEFT, HTBOTTOMLEFT],
            (_, true) => [HTTOPRIGHT, HTRIGHT, HTBOTTOMRIGHT],
            _ => [HTTOP, HTCLIENT, HTBOTTOM],
        };
        let row = if p.y < y {
            0
        } else if p.y >= client.bottom - y {
            2
        } else {
            1
        };
        if side[row] != HTCLIENT {
            return Some(side[row]);
        }
    }
    if modal {
        return Some(HTCLIENT);
    }
    let scale = f64::from(dpi) / 96.0;
    let point = PhysicalPosition::new(f64::from(p.x) + 0.5, f64::from(p.y) + 0.5);
    let width = f64::from(client.right);
    let nav = point.y >= NAV_TOP * scale
        && (point.x - width * 0.5).abs() < f64::from(tabs(3).1) * scale * 0.5;
    if point.y < HEIGHT * scale && !nav && Control::at(point, width, scale).is_none() {
        // Let Windows handle dragging, snap, and double click to maximize.
        Some(HTCAPTION)
    } else {
        Some(HTCLIENT)
    }
}

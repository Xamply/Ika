use std::time::Instant;

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    monitor::MonitorHandle,
    window::Window,
};

use crate::{crt::Tube, gpu::Params};

const FIRST_ROW: f32 = 114.0;
const ROW_HEIGHT: f32 = 84.0;
const ROW_STEP: f32 = 96.0;
const PAD: f32 = 27.0;
pub const VISIBLE_ROWS: usize = 16;

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    Outside,
    Inside,
    Close,
    Monitor(usize),
}

pub struct Picker {
    screens: Vec<MonitorHandle>,
    current: Option<MonitorHandle>,
    primary: Option<MonitorHandle>,
    tube: Tube,
    unavailable: bool,
    scroll: f32,
    focus: usize,
    keyboard_focus: bool,
}

impl Picker {
    pub fn new(window: &Window, reduced_motion: bool) -> Self {
        Self {
            screens: window.available_monitors().collect(),
            current: window.current_monitor(),
            primary: window.primary_monitor(),
            tube: Tube::new(reduced_motion),
            unavailable: false,
            scroll: 0.0,
            focus: 0,
            keyboard_focus: false,
        }
    }

    fn layout(&self, window: &Window) -> [f32; 4] {
        let dpi = window.scale_factor() as f32;
        let viewport = window.inner_size();
        let (w, h) = (viewport.width as f32 / dpi, viewport.height as f32 / dpi);
        let size = [
            (w - 48.0).clamp(1.0, 560.0),
            self.natural_height().min((h - 80.0).max(1.0)),
        ];
        [(w - size[0]) * 0.5, (h - size[1]) * 0.5, size[0], size[1]]
    }

    fn natural_height(&self) -> f32 {
        let rows = self.screens.len();
        if rows == 0 {
            146.0
        } else {
            FIRST_ROW + rows as f32 * ROW_STEP - (ROW_STEP - ROW_HEIGHT)
                + PAD
                + if self.unavailable { 36.0 } else { 0.0 }
        }
    }

    fn scroll_offset(&self, height: f32) -> f32 {
        self.scroll
            .clamp(0.0, (self.natural_height() - height).max(0.0))
    }

    pub fn hit(&self, window: &Window, pos: PhysicalPosition<f64>, now: Instant) -> Target {
        if self.tube.closing() || self.tube.awaiting_present() {
            return Target::Inside;
        }
        let [left, top, w, h] = self.layout(window);
        let [sx, sy, ..] = self.tube.frame(now).layers.panel;
        let dpi = window.scale_factor() as f32;
        let x = (pos.x as f32 / dpi - left - w * 0.5) / sx + w * 0.5;
        let y = (pos.y as f32 / dpi - top - h * 0.5) / sy + h * 0.5;
        if !(0.0..w).contains(&x) || !(0.0..h).contains(&y) {
            return Target::Outside;
        }
        if (w - 47.0..w - 11.0).contains(&x) && (11.0..43.0).contains(&y) {
            return Target::Close;
        }
        if (PAD..w - PAD).contains(&x)
            && (PAD..h - PAD).contains(&y)
            && y + self.scroll_offset(h) >= FIRST_ROW
        {
            let row = y + self.scroll_offset(h) - FIRST_ROW;
            let index = (row / ROW_STEP).floor() as usize;
            if row % ROW_STEP < ROW_HEIGHT && index < self.screens.len() {
                return Target::Monitor(index);
            }
        }
        Target::Inside
    }

    pub fn finished(&self, now: Instant) -> bool {
        self.tube.finished(now)
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.tube.animating(now, 0.76)
    }

    pub fn presented(&mut self, now: Instant) {
        self.tube.presented(now);
    }

    pub fn scroll(&mut self, window: &Window, delta: f32) {
        let height = self.layout(window)[3];
        self.scroll = (self.scroll_offset(height) + delta)
            .clamp(0.0, (self.natural_height() - height).max(0.0));
    }

    pub fn focus_next(&mut self, window: &Window, reverse: bool) {
        if self.tube.closing() {
            return;
        }
        let count = self.screens.len() + 1;
        self.focus = (self.focus + if reverse { count - 1 } else { 1 }) % count;
        self.keyboard_focus = true;
        if let Some(index) = self.focus.checked_sub(1) {
            let height = self.layout(window)[3];
            let top = FIRST_ROW + index as f32 * ROW_STEP;
            let scroll = self.scroll_offset(height);
            if top < scroll + PAD {
                self.scroll(window, top - scroll - PAD);
            } else if top + ROW_HEIGHT > scroll + height - PAD {
                self.scroll(window, top + ROW_HEIGHT - scroll - height + PAD);
            }
        }
    }

    pub fn activate_focus(&mut self, window: &Window, now: Instant) {
        self.activate(
            window,
            self.focus
                .checked_sub(1)
                .map_or(Target::Close, Target::Monitor),
            now,
        );
    }

    pub fn activate(&mut self, window: &Window, target: Target, now: Instant) {
        if self.tube.closing() {
            return;
        }
        match target {
            Target::Inside => return,
            Target::Monitor(index) => {
                let Some(screen) = self.screens.get(index) else {
                    return;
                };
                if !window.available_monitors().any(|m| m == *screen) {
                    self.unavailable = true;
                    return;
                }
                let size = screen.size();
                let origin = screen.position();
                window.set_maximized(false);
                let _ =
                    window.request_inner_size(PhysicalSize::new(size.width / 2, size.height / 2));
                window.set_outer_position(PhysicalPosition::new(origin.x + 20, origin.y + 20));
                window.set_maximized(true);
                window.focus_window();
            }
            Target::Outside | Target::Close => {}
        }
        self.tube.close(now);
    }

    pub fn uniforms(&self, p: &mut Params, window: &Window, hover: Option<Target>, now: Instant) {
        let [left, top, w, h] = self.layout(window);
        let scroll = self.scroll_offset(h);
        let focus = self.keyboard_focus.then_some(self.focus);
        let first = ((scroll + PAD - FIRST_ROW) / ROW_STEP).floor().max(0.0) as usize;
        let mut frame = self.tube.frame(now);
        frame.layers.panel[3] = if self.tube.closing() {
            frame.content[0]
        } else {
            1.0
        };
        p.monitor_rect = [left, top, w, h];
        p.monitor_view = [
            scroll,
            first as f32,
            frame.backdrop,
            if self.screens.is_empty() {
                1.0
            } else if self.unavailable {
                2.0
            } else {
                0.0
            },
        ];
        p.monitor_crt = frame.layers;
        p.monitor_ui = [
            f32::from(hover == Some(Target::Close)),
            f32::from(focus == Some(0)),
            self.natural_height(),
            1.0,
        ];
        for (local, monitor) in self
            .screens
            .iter()
            .skip(first)
            .take(VISIBLE_ROWS)
            .enumerate()
        {
            let i = first + local;
            let size = monitor.size();
            let mut bits = (monitor.scale_factor() * 100.0).round() as u32;
            bits &= 0xffff;
            bits |= u32::from(self.primary.as_ref() == Some(monitor)) << 16;
            bits |= u32::from(self.current.as_ref() == Some(monitor)) << 17;
            bits |= u32::from(hover == Some(Target::Monitor(i))) << 18;
            bits |= u32::from(focus == Some(i + 1)) << 19;
            p.monitor_rows[local] = [i as u32 + 1, size.width, size.height, bits];
        }
    }
}

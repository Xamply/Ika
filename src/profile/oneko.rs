//! Oneko cat pet. Portado de la versión WebView2 legacy.
use egui::{
    pos2, vec2, Color32 as C, Id, LayerId, Order, Pos2, Rect, TextureHandle,
    TextureOptions, Ui, Vec2,
};
use rand::random;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use winit::event::MouseButton;

const SPRITE_SIZE: f32 = 50.0;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const STORAGE_FILE: &str = "oneko.json";

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct OnekoConfig {
    pub enabled: bool,
    pub black: bool,
}

impl Default for OnekoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            black: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anim {
    Idle,
    Alert,
    ScratchSelf,
    ScratchWallN,
    ScratchWallS,
    ScratchWallE,
    ScratchWallW,
    Tired,
    Sleeping,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Anim {
    /// Column and row (0..8, 0..4) in classic.png (256x128 grid of 32x32 tiles)
    fn frames(self) -> &'static [(usize, usize)] {
        match self {
            Self::Idle => &[(3, 3)],
            Self::Alert => &[(7, 3)],
            Self::ScratchSelf => &[(5, 0), (6, 0), (7, 0)],
            Self::ScratchWallN => &[(0, 0), (0, 1)],
            Self::ScratchWallS => &[(7, 1), (6, 2)],
            Self::ScratchWallE => &[(2, 2), (2, 3)],
            Self::ScratchWallW => &[(4, 0), (4, 1)],
            Self::Tired => &[(3, 2)],
            Self::Sleeping => &[(2, 0), (2, 1)],
            Self::N => &[(1, 2), (1, 3)],
            Self::NE => &[(0, 2), (0, 3)],
            Self::E => &[(3, 0), (3, 1)],
            Self::SE => &[(5, 1), (5, 2)],
            Self::S => &[(6, 3), (7, 2)],
            Self::SW => &[(5, 3), (6, 1)],
            Self::W => &[(4, 2), (4, 3)],
            Self::NW => &[(1, 0), (1, 1)],
        }
    }
}

pub struct Oneko {
    pub config: OnekoConfig,
    pub pos: Pos2,
    pub target: Pos2,
    pub sleeping: bool,
    pub perched: bool,
    pub anchor: Option<Pos2>,
    last_cursor: Pos2,
    idle_time: u32,
    anim: Option<Anim>,
    anim_frame: usize,
    frame_counter: usize,
    last_tick: Instant,
    drag_offset: Option<Vec2>,
    last_drag_pos: Pos2,
    drag_accum: f32,
    last_drag_render: Instant,
    last_drag_move: Instant,
    last_click: Option<(Instant, Pos2)>,
    texture_classic: Option<TextureHandle>,
    texture_black: Option<TextureHandle>,
}

impl Oneko {
    pub fn new() -> Self {
        let config_path = super::storage::native_dir().join(STORAGE_FILE);
        let config: OnekoConfig = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Self {
            config,
            pos: pos2(64.0, 88.0),
            target: pos2(64.0, 88.0),
            sleeping: false,
            perched: false,
            anchor: None,
            last_cursor: pos2(64.0, 88.0),
            idle_time: 0,
            anim: None,
            anim_frame: 0,
            frame_counter: 0,
            last_tick: Instant::now(),
            drag_offset: None,
            last_drag_pos: pos2(64.0, 88.0),
            drag_accum: 0.0,
            last_drag_render: Instant::now(),
            last_drag_move: Instant::now(),
            last_click: None,
            texture_classic: None,
            texture_black: None,
        }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
            let _ = super::storage::save_native(STORAGE_FILE, json.as_bytes());
        }
    }

    #[allow(dead_code)]
    pub fn set_target(&mut self, target: Pos2) {
        if self.drag_offset.is_none() && !self.sleeping {
            self.target = target;
        }
    }

    pub fn reset_idle(&mut self) {
        self.anim = None;
        self.anim_frame = 0;
        self.idle_time = 0;
    }

    #[allow(dead_code)]
    pub fn is_dragging(&self) -> bool {
        self.drag_offset.is_some()
    }

    pub fn covers(&self, p: Pos2) -> bool {
        Rect::from_center_size(self.pos, vec2(SPRITE_SIZE, SPRITE_SIZE)).contains(p)
    }

    pub fn on_cursor_moved(&mut self, p: Pos2, win_w: f32, win_h: f32) -> (bool, bool) {
        if let Some(offset) = self.drag_offset {
            let half = SPRITE_SIZE * 0.5;
            let new_x = (p.x - offset.x).clamp(half, win_w - half);
            let new_y = (p.y - offset.y).clamp(48.0 + half, win_h - half);
            let clamped = pos2(new_x, new_y);

            let dx = clamped.x - self.last_drag_pos.x;
            let dy = clamped.y - self.last_drag_pos.y;
            let dist = dx.hypot(dy);
            self.pos = clamped;
            self.target = clamped;

            let now = Instant::now();
            if dist > 0.0 {
                self.last_drag_move = now;
                self.drag_accum += dist;
                if self.drag_accum >= 24.0 {
                    let steps = (self.drag_accum / 24.0) as usize;
                    self.frame_counter = self.frame_counter.wrapping_add(steps);
                    self.drag_accum %= 24.0;
                }
                if dist >= 4.0 {
                    let new_anim = if dx.abs() > dy.abs() * 1.2 {
                        if dx > 0.0 { Anim::ScratchWallW } else { Anim::ScratchWallE }
                    } else if dy.abs() > dx.abs() * 1.2 {
                        if dy > 0.0 { Anim::ScratchWallN } else { Anim::ScratchWallS }
                    } else {
                        self.anim.unwrap_or(Anim::ScratchWallS)
                    };
                    self.anim = Some(new_anim);
                    self.last_drag_pos = clamped;
                }
            }

            let redraw = now.duration_since(self.last_drag_render) >= Duration::from_millis(16);
            if redraw {
                self.last_drag_render = now;
            }
            (true, redraw)
        } else {
            self.last_cursor = p;
            if !self.sleeping && !self.perched && self.anchor.is_none() {
                self.target = p;
            }
            (false, false)
        }
    }

    pub fn on_mouse_input(&mut self, pressed: bool, button: MouseButton, p: Pos2) -> bool {
        if button == MouseButton::Left {
            if pressed {
                if self.covers(p) {
                    let now = Instant::now();
                    if let Some((prev_time, prev_pos)) = self.last_click {
                        if now.duration_since(prev_time) <= Duration::from_millis(350)
                            && (p - prev_pos).length() <= 12.0
                        {
                            self.last_click = None;
                            self.drag_offset = None;
                            self.drag_accum = 0.0;
                            self.sleep();
                            return true;
                        }
                    }
                    self.last_click = Some((now, p));
                    self.drag_offset = Some(p - self.pos);
                    self.last_drag_pos = self.pos;
                    self.drag_accum = 0.0;
                    self.anim = None;
                    self.last_drag_render = now;
                    self.last_drag_move = now.checked_sub(Duration::from_millis(200)).unwrap_or(now);
                    return true;
                }
            } else if self.drag_offset.is_some() {
                self.drag_offset = None;
                self.drag_accum = 0.0;
                self.target = self.pos;
                self.reset_idle();
                return true;
            }
        } else if button == MouseButton::Right && pressed && self.covers(p) {
            self.config.black = !self.config.black;
            self.save();
            return true;
        }
        false
    }

    pub fn on_lost_capture(&mut self) -> bool {
        if self.drag_offset.take().is_some() {
            self.drag_accum = 0.0;
            self.target = self.pos;
            self.reset_idle();
            true
        } else {
            false
        }
    }

    pub fn needs_tick(&self) -> bool {
        if !self.config.enabled || self.perched {
            return false;
        }
        if self.drag_offset.is_some() {
            return self.anim.is_some();
        }
        let dx = self.pos.x - self.target.x;
        let dy = self.pos.y - self.target.y;
        let dist = dx.hypot(dy);
        let stop_dist = if self.anchor.is_some() { 4.0 } else { 48.0 };
        dist >= stop_dist || self.anim.is_some() || self.sleeping
    }

    pub fn update_minigame_perch(&mut self, perch: Option<Pos2>) -> bool {
        if let Some(target) = perch {
            if self.drag_offset.is_none() {
                self.anchor = Some(target);
                self.target = target;
                if self.sleeping {
                    self.sleeping = false;
                    self.reset_idle();
                }
                if self.perched {
                    let moved = self.pos != target;
                    self.pos = target;
                    return moved;
                }
                return false;
            }
            false
        } else if self.anchor.is_some() || self.perched {
            self.anchor = None;
            self.perched = false;
            self.target = self.last_cursor;
            self.reset_idle();
            true
        } else {
            false
        }
    }

    pub fn next_tick_deadline(&self) -> Instant {
        if self.drag_offset.is_some() {
            return self.last_drag_move + Duration::from_millis(120);
        }
        let interval = if self.needs_tick() {
            TICK_INTERVAL
        } else {
            Duration::from_millis(500)
        };
        self.last_tick + interval
    }

    pub fn sleep(&mut self) {
        self.sleeping = !self.sleeping;
        self.reset_idle();
    }

    fn idle(&mut self, window_w: f32, window_h: f32) {
        self.idle_time = self.idle_time.saturating_add(1);
        if self.sleeping {
            self.anim = Some(Anim::Sleeping);
        }
        let chance = if self.needs_tick() { 200 } else { 40 };
        if self.anim.is_none() && self.idle_time > 5 && (random::<u32>() % chance == 0) {
            let half = SPRITE_SIZE * 0.5;
            let mut choices = vec![Anim::Sleeping, Anim::ScratchSelf];
            if self.pos.x <= half + 8.0 {
                choices.push(Anim::ScratchWallW);
            }
            if self.pos.y <= 48.0 + half + 8.0 {
                choices.push(Anim::ScratchWallN);
            }
            if self.pos.x >= window_w - half - 8.0 {
                choices.push(Anim::ScratchWallE);
            }
            if self.pos.y >= window_h - half - 8.0 {
                choices.push(Anim::ScratchWallS);
            }
            let idx = random::<usize>() % choices.len();
            self.anim = Some(choices[idx]);
            self.anim_frame = 0;
        }

        if self.anim == Some(Anim::Sleeping) {
            self.anim_frame += 1;
            if self.anim_frame > 192 && !self.sleeping {
                self.reset_idle();
            }
        } else if self.anim.is_some() {
            self.anim_frame += 1;
            if self.anim_frame > 9 {
                self.reset_idle();
            }
        }
    }

    pub fn tick(&mut self, now: Instant, window_w: f32, window_h: f32) -> bool {
        if !self.config.enabled || self.perched {
            return false;
        }
        let interval = if self.needs_tick() {
            TICK_INTERVAL
        } else {
            Duration::from_millis(500)
        };
        if now.duration_since(self.last_tick) < interval {
            return false;
        }
        self.last_tick = now;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        if self.drag_offset.is_some() {
            if self.anim.is_some() && now.duration_since(self.last_drag_move) >= Duration::from_millis(120) {
                self.anim = None;
                return true;
            }
            return false;
        }

        let dx = self.pos.x - self.target.x;
        let dy = self.pos.y - self.target.y;
        let dist = dx.hypot(dy);

        let old_pos = self.pos;
        let old_anim = self.anim;
        let old_frame = self.anim_frame;

        let stop_dist = if self.anchor.is_some() { 4.0 } else { 48.0 };

        if (!self.anchor.is_some() && self.sleeping) || dist < stop_dist {
            if self.anchor.is_some() {
                self.pos = self.target;
                self.perched = true;
                self.anim = Some(Anim::Idle);
                self.idle_time = 0;
                return self.pos != old_pos || self.anim != old_anim;
            }
            self.idle(window_w, window_h);
            let sprite_changed = if self.anim == Some(Anim::Sleeping) {
                (self.anim_frame / 4) != (old_frame / 4)
            } else {
                self.anim_frame != old_frame
            };
            return self.anim != old_anim || sprite_changed;
        }

        self.anim = None;
        self.anim_frame = 0;

        if self.idle_time > 1 {
            self.idle_time = self.idle_time.min(7).saturating_sub(1);
            return true;
        }

        let step = dist.min(10.0);
        self.pos.x -= (dx / dist) * step;
        self.pos.y -= (dy / dist) * step;

        let half = SPRITE_SIZE * 0.5;
        self.pos.x = self.pos.x.clamp(half, window_w - half);
        let min_y = if let Some(a) = self.anchor { a.y.min(48.0 + half) } else { 48.0 + half };
        self.pos.y = self.pos.y.clamp(min_y, window_h - half);

        if self.anchor.is_some() {
            let rem_dx = self.pos.x - self.target.x;
            let rem_dy = self.pos.y - self.target.y;
            if rem_dx.hypot(rem_dy) < 4.0 {
                self.pos = self.target;
                self.perched = true;
                self.anim = Some(Anim::Idle);
                self.idle_time = 0;
            }
        }

        self.pos != old_pos || self.anim != old_anim
    }

    fn current_sprite(&self) -> (usize, usize) {
        if self.perched && self.drag_offset.is_none() {
            return Anim::Idle.frames()[0];
        }

        if self.drag_offset.is_some() {
            if let Some(anim) = self.anim {
                if Instant::now().duration_since(self.last_drag_move) < Duration::from_millis(120) {
                    let frames = anim.frames();
                    return frames[self.frame_counter % frames.len()];
                }
            }
            return Anim::Alert.frames()[0];
        }

        let dx = self.pos.x - self.target.x;
        let dy = self.pos.y - self.target.y;
        let dist = dx.hypot(dy);
        let stop_dist = if self.anchor.is_some() { 4.0 } else { 48.0 };

        if (!self.anchor.is_some() && self.sleeping) || dist < stop_dist {
            if let Some(anim) = self.anim {
                if anim == Anim::Sleeping {
                    if self.anim_frame < 8 {
                        Anim::Tired.frames()[0]
                    } else {
                        let frames = Anim::Sleeping.frames();
                        frames[(self.anim_frame / 4) % frames.len()]
                    }
                } else {
                    let frames = anim.frames();
                    frames[self.anim_frame % frames.len()]
                }
            } else {
                Anim::Idle.frames()[0]
            }
        } else if self.idle_time > 1 {
            Anim::Alert.frames()[0]
        } else {
            let dir = match (dy / dist > 0.5, dy / dist < -0.5, dx / dist > 0.5, dx / dist < -0.5) {
                (true, _, true, _) => Anim::NW,
                (true, _, _, true) => Anim::NE,
                (true, _, _, _) => Anim::N,
                (_, true, true, _) => Anim::SW,
                (_, true, _, true) => Anim::SE,
                (_, true, _, _) => Anim::S,
                (_, _, true, _) => Anim::W,
                (_, _, _, true) => Anim::E,
                _ => Anim::Idle,
            };
            let frames = dir.frames();
            frames[self.frame_counter % frames.len()]
        }
    }

    pub fn paint(&mut self, ui: &mut Ui) {
        if !self.config.enabled {
            return;
        }

        let pet_rect = Rect::from_center_size(self.pos, vec2(SPRITE_SIZE, SPRITE_SIZE));

        // Use top-level foreground layer so Oneko sits comfortably above the launcher UI
        let layer_id = LayerId::new(Order::Foreground, Id::new("oneko-pet-layer"));
        let p = ui.painter().clone().with_layer_id(layer_id);

        let (col, row) = self.current_sprite();
        let uv = Rect::from_min_max(
            pos2(col as f32 / 8.0, row as f32 / 4.0),
            pos2((col + 1) as f32 / 8.0, (row + 1) as f32 / 4.0),
        );

        // Texture management
        let is_black = self.config.black;
        let tex_id = if is_black {
            self.texture_black.get_or_insert_with(|| {
                let img = image::load_from_memory(include_bytes!("oneko/classic.png"))
                    .unwrap()
                    .to_rgba8();
                let mut inverted = img.clone();
                for pixel in inverted.pixels_mut() {
                    if pixel[3] > 0 {
                        pixel[0] = 255 - pixel[0];
                        pixel[1] = 255 - pixel[1];
                        pixel[2] = 255 - pixel[2];
                    }
                }
                super::load_image(
                    ui.ctx(),
                    "oneko-black-atlas",
                    &inverted,
                    TextureOptions::NEAREST,
                )
            }).id()
        } else {
            self.texture_classic.get_or_insert_with(|| {
                let img = image::load_from_memory(include_bytes!("oneko/classic.png"))
                    .unwrap()
                    .to_rgba8();
                super::load_image(
                    ui.ctx(),
                    "oneko-classic-atlas",
                    &img,
                    TextureOptions::NEAREST,
                )
            }).id()
        };

        p.image(tex_id, pet_rect, uv, C::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drag_oneko() {
        let mut oneko = Oneko::new();
        let center = oneko.pos;

        // Hover over oneko: does not consume and does not change cursor
        let (consumed, redraw) = oneko.on_cursor_moved(center, 1920.0, 1080.0);
        assert!(!consumed);
        assert!(!redraw);

        // Press down
        let handled = oneko.on_mouse_input(true, MouseButton::Left, center);
        assert!(handled);
        assert!(oneko.is_dragging());

        // Move while dragging (simulate passage of at least 16ms so redraw is true)
        oneko.last_drag_render -= Duration::from_millis(20);
        let (consumed, redraw) = oneko.on_cursor_moved(center + vec2(50.0, 50.0), 1920.0, 1080.0);
        assert!(consumed);
        assert!(redraw);
        assert_eq!(oneko.pos, center + vec2(50.0, 50.0));

        // Release
        let handled = oneko.on_mouse_input(false, MouseButton::Left, center + vec2(50.0, 50.0));
        assert!(handled);
        assert!(!oneko.is_dragging());
    }

    #[test]
    fn test_double_click_sleep() {
        let mut oneko = Oneko::new();
        let center = oneko.pos;
        assert!(!oneko.sleeping);

        // Click 1
        oneko.on_mouse_input(true, MouseButton::Left, center);
        oneko.on_mouse_input(false, MouseButton::Left, center);

        // Click 2 (double click)
        oneko.on_mouse_input(true, MouseButton::Left, center);
        assert!(oneko.sleeping);
    }

    #[test]
    fn test_drag_no_flicker_and_throttling() {
        let mut oneko = Oneko::new();
        let center = oneko.pos;

        // Start drag
        oneko.on_mouse_input(true, MouseButton::Left, center);
        let initial_frame = oneko.frame_counter;

        // 10 micro-movements of 0.5px (total 5px < 24px threshold) in rapid succession (<16ms)
        let mut redraw_count = 0;
        for i in 1..=10 {
            let p = center + vec2(i as f32 * 0.5, 0.0);
            let (consumed, redraw) = oneko.on_cursor_moved(p, 1920.0, 1080.0);
            assert!(consumed);
            if redraw {
                redraw_count += 1;
            }
        }
        // Micro-moves must NOT cause frame counter to flip (prevents flickering)
        assert_eq!(oneko.frame_counter, initial_frame);
        // Rapid moves within 16ms must NOT spam redraws (throttled to at most 1)
        assert!(redraw_count <= 1);

        // Move across screen by 48px after 20ms: should advance frame counter by exactly 2 steps
        oneko.last_drag_render -= Duration::from_millis(20);
        let (consumed, redraw) = oneko.on_cursor_moved(center + vec2(48.0, 0.0), 1920.0, 1080.0);
        assert!(consumed);
        assert!(redraw);
        assert_eq!(oneko.frame_counter, initial_frame + 2);
    }

    #[test]
    fn test_chasing_tick_rate() {
        let mut oneko = Oneko::new();
        let now = Instant::now();
        oneko.target = oneko.pos + vec2(200.0, 200.0); // Far away
        assert!(oneko.needs_tick());

        // First tick
        oneko.last_tick = now - Duration::from_millis(150);
        assert!(oneko.tick(now, 1920.0, 1080.0));

        // Immediate subsequent tick (<100ms) should return false
        assert!(!oneko.tick(now + Duration::from_millis(20), 1920.0, 1080.0));

        // Tick deadline must be exactly 100ms after last tick
        let deadline = oneko.next_tick_deadline();
        assert_eq!(deadline, now + TICK_INTERVAL);
    }

    #[test]
    fn test_drag_stationary_shows_alert_not_scratch() {
        let mut oneko = Oneko::new();
        let center = oneko.pos;

        // Press down to start dragging (stationary)
        oneko.on_mouse_input(true, MouseButton::Left, center);
        assert!(oneko.is_dragging());
        // Must show Alert (7, 3), NOT ScratchWall
        assert_eq!(oneko.current_sprite(), Anim::Alert.frames()[0]);

        // Move 40px to start scratch animation
        oneko.last_drag_render -= Duration::from_millis(20);
        oneko.on_cursor_moved(center + vec2(40.0, 0.0), 1920.0, 1080.0);
        // While moving, shows ScratchWallW
        assert!(Anim::ScratchWallW.frames().contains(&oneko.current_sprite()));

        // Stop moving for 150ms: sprite immediately becomes Alert
        oneko.last_drag_move -= Duration::from_millis(150);
        assert_eq!(oneko.current_sprite(), Anim::Alert.frames()[0]);

        // Tick repaints to clear anim
        let now = Instant::now();
        oneko.last_tick = now - Duration::from_millis(150);
        assert!(oneko.tick(now, 1920.0, 1080.0));
        assert_eq!(oneko.anim, None);

        // Next tick while still stationary returns false (no busy redraw)
        assert!(!oneko.tick(now + Duration::from_millis(100), 1920.0, 1080.0));
        assert_eq!(oneko.current_sprite(), Anim::Alert.frames()[0]);
    }

    #[test]
    fn test_initial_default_is_white() {
        let default_config = OnekoConfig::default();
        assert!(!default_config.black);
        assert!(default_config.enabled);
    }

    #[test]
    fn test_right_click_toggle_black() {
        let mut oneko = Oneko::new();
        let center = oneko.pos;
        let initial_black = oneko.config.black;

        let handled = oneko.on_mouse_input(true, MouseButton::Right, center);
        assert!(handled);
        assert_eq!(oneko.config.black, !initial_black);
        // Toggle back to preserve state
        oneko.on_mouse_input(true, MouseButton::Right, center);
        assert_eq!(oneko.config.black, initial_black);
    }

    #[test]
    fn test_save_and_reload_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("oneko.json");
        let config = OnekoConfig { enabled: true, black: true };
        let json = serde_json::to_string_pretty(&config).unwrap();
        std::fs::write(&path, json).unwrap();

        let loaded: OnekoConfig = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap();
        assert!(loaded.black);
        assert!(loaded.enabled);
    }

    #[test]
    fn test_minigame_perch_locking_and_release() {
        let mut oneko = Oneko::new();
        let initial_pos = oneko.pos;
        let perch = pos2(450.0, 120.0);

        // Minigame starts: anchors Oneko towards the window perch without teleporting
        oneko.update_minigame_perch(Some(perch));
        assert_eq!(oneko.pos, initial_pos); // NOT teleported!
        assert_eq!(oneko.target, perch);
        assert!(!oneko.perched); // Still walking
        assert!(oneko.needs_tick()); // Ticking to walk there

        // Moving mouse cursor does not change Oneko target when anchored
        let (consumed, redraw) = oneko.on_cursor_moved(pos2(100.0, 100.0), 1920.0, 1080.0);
        assert!(!consumed);
        assert!(!redraw);
        assert_eq!(oneko.target, perch);

        // Walk Oneko until it reaches the perch
        let now = Instant::now();
        for i in 0..100 {
            let t = now + Duration::from_millis((i + 1) * 100);
            oneko.tick(t, 1920.0, 1080.0);
            if oneko.perched {
                break;
            }
        }
        assert!(oneko.perched);
        assert_eq!(oneko.pos, perch);
        assert_eq!(oneko.current_sprite(), Anim::Idle.frames()[0]);
        assert!(!oneko.needs_tick()); // Zero GPU usage once arrived

        // Minigame paused/stopped: unperch Oneko
        assert!(oneko.update_minigame_perch(None));
        assert!(!oneko.perched);
        assert!(oneko.anchor.is_none());

        // Moving mouse cursor now updates target as normal
        oneko.on_cursor_moved(pos2(100.0, 100.0), 1920.0, 1080.0);
        assert_eq!(oneko.target, pos2(100.0, 100.0));
    }
}

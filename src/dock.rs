#[path = "dock/layout.rs"]
#[rustfmt::skip]
mod layout;
#[path = "dock/play.rs"]
pub mod play;

use crate::{
    gpu::Params,
    titlebar::{CSS_EASE, ICON_EASE, Transition, reduced_motion},
};
use std::time::{Duration, Instant};
use winit::{dpi::PhysicalPosition, window::Window};

pub fn wallpaper_stroke_phase(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f64().rem_euclid(18.0) * (2400.0 / 18.0)) as f32
}

/// Match dock_ramp(fract(x / 2400), 1.0) in wallpapers.wgsl.
pub fn wallpaper_stroke_rgb(x: f32) -> [u8; 3] {
    const RAMP: &[u8; 512 * 3] = include_bytes!("dock/wallpaper-stroke.rgb");
    let u = x.rem_euclid(2400.) * (511. / 2400.);
    let first = (u as usize).min(511);
    let second = (first + 1).min(511);
    let t = u - first as f32;
    std::array::from_fn(|i| {
        let a = f32::from(RAMP[first*3+i]);
        let b = f32::from(RAMP[second*3+i]);
        (a+(b-a)*t).round() as u8
    })
}

pub const WALLPAPERS: usize = 12;
pub const EXPORT: usize = 0;
pub const FOLDER: usize = 1;
pub const PLAY: usize = 2;
pub const EDIT: usize = 3;
pub const PROFILE: usize = 5;
pub const PREFERENCES: usize = 6;
pub const FRIENDS: usize = 7;
const COUNT: usize = 13;
const IN_OUT: [f32; 4] = [0.42, 0.0, 0.58, 1.0];
use play::{POP, PRESS};

struct Geometry {
    scale: f32,
    width: f32,
    height: f32,
}

impl Geometry {
    fn new(window: &Window) -> Self {
        let size = window.inner_size();
        let (w, h) = (size.width as f32, size.height as f32);
        // The center cluster and both trays fit without overlapping. On a
        // narrow restored window preserve their proportions as one group.
        let scale = (window.scale_factor() as f32)
            .min(w / 2160.0)
            .min(h / 560.0)
            .max(f32::EPSILON);
        Self {
            scale,
            width: w / scale,
            height: h / scale,
        }
    }

    fn rect(&self, i: usize) -> [f32; 4] {
        let [x, y, w, h] = layout::BUTTONS[i];
        let shift = match i {
            0..=4 => (self.width - 2560.0) * 0.5,
            5..=8 => 0.0,
            _ => self.width - 2560.0,
        };
        [x + shift, y, w, h]
    }
}

pub fn hit(window: &Window, cursor: PhysicalPosition<f64>) -> Option<usize> {
    let geo = Geometry::new(window);
    let x = cursor.x as f32 / geo.scale;
    let y = cursor.y as f32 / geo.scale - geo.height + 340.0;
    // Play's DOM bounds include animated, pointer-transparent effects, and
    // overlap a side button. Its visible hexagon is the actual hit region.
    let play_x = (x - geo.width * 0.5).abs();
    let play_y = (y - 201.0).abs();
    if play_y < 40.0 && play_x < 176.1 - play_y * 0.63 {
        return Some(PLAY);
    }
    (0..COUNT).filter(|&i| i != PLAY).find(|&i| {
        let [l, t, w, h] = geo.rect(i);
        x >= l && x < l + w && y >= t && y < t + h
    })
}

pub fn reserved_height(window: &Window) -> f32 {
    // Leave the Play button and its trays below the wallpaper dialog.
    190.0 * Geometry::new(window).scale
}

pub fn hairline_height(window: &Window) -> f32 {
    // Match the outer horizontal part of dock_wing, in the same 340px canvas.
    // Leave one physical pixel for the lit edge in front of the Home panels.
    let rail_y = 134.0 + 34.0 * (134.0 / 148.0);
    (340.0 - rail_y) * Geometry::new(window).scale + 1.0
}

pub fn profile_rect(window: &Window) -> ([f32;4], f32) {
    let g = Geometry::new(window);
    let [x,y,w,h] = g.rect(PROFILE);
    let d = g.scale / window.scale_factor() as f32;
    ([x*d,(y+g.height-340.)*d,w*d,h*d],d)
}

pub struct Dock {
    hot: [Transition<1>; COUNT],
    scale: [Transition<1>; 5],
    hover: Option<usize>,
    tip: Option<usize>,
    tip_alpha: Transition<1>,
    tip_lift: Transition<1>,
    play_started: Instant,
    play_fx: [Transition<1>; 5],
    play: play::Motion,
    reduced_motion: bool,
}

impl Dock {
    pub fn new(now: Instant) -> Self {
        Self {
            hot: [Transition::new([0.0], now); COUNT],
            scale: [Transition::new([1.0], now); 5],
            hover: None,
            tip: None,
            tip_alpha: Transition::new([0.0], now),
            tip_lift: Transition::new([6.0], now),
            play_started: now,
            play_fx: [Transition::new([0.0], now); 5],
            play: play::Motion::new(now),
            reduced_motion: reduced_motion(),
        }
    }

    pub fn play_pointer_down(&mut self, now: Instant) {
        self.play.pointer_down(now);
    }

    pub fn animated(&self, now: Instant) -> bool {
        (!self.reduced_motion
            && (self.play.animated(now)
                || self.hover == Some(PLAY)
                || self.play_fx.iter().any(|t| t.running(now, 0.62))))
            || self.hot.iter().any(|t| t.running(now, 0.35))
            || self.scale.iter().any(|t| t.running(now, 0.34))
            || self.tip_alpha.running(now, 0.16)
            || self.tip_lift.running(now, 0.22)
    }

    pub fn uniforms(
        &mut self,
        p: &mut Params,
        window: &Window,
        now: Instant,
        elapsed: Duration,
        hover: Option<usize>,
        down: Option<usize>,
        hold: bool,
        play_state: play::State,
    ) {
        let hover = hover.filter(|i| *i != PLAY || !play_state.locked());
        let down = down.filter(|i| *i != PLAY || !play_state.locked());
        let geo = Geometry::new(window);
        p.dock_view = [geo.width, geo.height, geo.scale, 0.0];
        let elapsed = if self.reduced_motion { Duration::ZERO } else { elapsed };
        let time = elapsed.as_secs_f64();
        let period = |s: f64| (time.rem_euclid(s) / s) as f32;
        p.dock_phase = [
            wallpaper_stroke_phase(elapsed),
            period(22.0) * 2400.0,
            -600.0 - period(27.0) * 2400.0,
            -1800.0 - period(31.0) * 2400.0,
        ];
        let angle = (25.0 + period(10.0) * 360.0).to_radians();
        p.dock_play = [
            angle.cos(),
            angle.sin(),
            period(3.0),
            0.0,
        ];
        let immediate = self.reduced_motion || hold;
        let (play, pop) = self.play.frame(
            now, play_state, down == Some(PLAY) && hover == Some(PLAY), immediate,
        );
        p.play = play;
        for i in 0..COUNT {
            let hot = hover == Some(i);
            let duration = if i == WALLPAPERS { 0.35 } else { 0.22 };
            let curve = if i == WALLPAPERS { CSS_EASE } else { IN_OUT };
            if immediate {
                self.hot[i] = Transition::new([f32::from(hot)], now);
            } else {
                self.hot[i].set([f32::from(hot)], now, duration, curve);
            }
            p.dock_hot[i / 4][i % 4] = self.hot[i].sample(now, duration, curve)[0];
            if i < self.scale.len() {
                let pressed = hot && down == Some(i);
                let end = if pressed {
                    if i == PLAY { 0.955 } else { 0.96 }
                } else if hot {
                    if i == PLAY { 1.028 } else { 1.10 }
                } else {
                    1.0
                };
                let (duration, curve) = match i {
                    PLAY if pressed => (0.06, PRESS),
                    PLAY => (0.34, POP),
                    _ => (0.30, ICON_EASE),
                };
                if immediate {
                    self.scale[i] = Transition::new([end], now);
                } else {
                    self.scale[i].set([end], now, duration, curve);
                }
                p.dock_scale[i / 4][i % 4] = self.scale[i].sample(now, duration, curve)[0];
            }
        }
        if let Some(scale) = pop {
            p.dock_scale[0][PLAY] = scale;
        }

        let play_hover = hover == Some(PLAY);
        if play_hover && self.hover != Some(PLAY) {
            self.play_started = now;
        }
        let t = if self.reduced_motion {
            0.0
        } else {
            (now - self.play_started).as_secs_f32()
        };
        let pulse = |period: f32| {
            let f = (t / period).fract();
            crate::titlebar::ease(1.0 - (2.0 * f - 1.0).abs(), IN_OUT)
        };
        let target = f32::from(play_hover);
        let mut v = [0.0; 5];
        for (i, (duration, curve)) in [
            (0.46, POP),
            (0.52, POP),
            (0.38, POP),
            (0.62, POP),
            (0.18, CSS_EASE),
        ]
        .into_iter()
        .enumerate()
        {
            if immediate {
                self.play_fx[i] = Transition::new([target], now);
            } else {
                self.play_fx[i].set([target], now, duration, curve);
            }
            v[i] = self.play_fx[i].sample(now, duration, curve)[0];
        }
        p.dock_fx = [v[0], v[1], v[2], v[3]];
        p.dock_pulse = [(t / 1.6).fract(), pulse(1.15), pulse(1.35), v[4]];

        if let Some(i) = hover.filter(|_| hover != self.hover) {
            self.tip = Some(i);
        }
        self.hover = hover;
        let visible = hover.is_some() && down.is_none();
        let lift = if visible { 0.0 } else { 6.0 };
        if immediate {
            self.tip_alpha = Transition::new([f32::from(visible)], now);
            self.tip_lift = Transition::new([lift], now);
        } else {
            self.tip_alpha
                .set([f32::from(visible)], now, 0.16, CSS_EASE);
            self.tip_lift.set([lift], now, 0.22, POP);
        }
        if let Some(i) = self.tip {
            let [x, y, w, _] = geo.rect(i);
            let label_x = if i == PLAY {
                geo.width * 0.5
            } else {
                x + w * 0.5
            };
            // SVG tooltips attach to the small painted icon, not its larger
            // transparent hit target. Play attaches above its visible rim.
            let label_y = match i {
                PLAY => 161.0,
                0..=4 => 188.0,
                _ => y,
            };
            p.dock_tip = [
                label_x.round(),
                (label_y - 12.0).round() + self.tip_lift.sample(now, 0.22, POP)[0],
                i as f32,
                self.tip_alpha.sample(now, 0.16, CSS_EASE)[0],
            ];
        }
    }
}

pub fn open_link(i: usize) {
    use windows::core::w;
    let url = match i {
        9 => w!("https://github.com/Xamply/Ika"),
        10 => w!("https://discord.gg/XbhaSDp65"),
        11 => w!("https://patreon.com/Faallyxmp?utm_source=ika-launcher"),
        _ => return,
    };
    open_url(url);
}

pub fn open_url(url: windows::core::PCWSTR) {
    let _ = shell_open(url);
}

pub fn open_target(url: &str) -> std::io::Result<()> {
    let wide: Vec<u16> = url.encode_utf16().chain([0]).collect();
    shell_open(windows::core::PCWSTR(wide.as_ptr()))
}

fn shell_open(url: windows::core::PCWSTR) -> std::io::Result<()> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{PCWSTR, w},
    };
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            url,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other("Windows no pudo abrir el enlace."))
    }
}

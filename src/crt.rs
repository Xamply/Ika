//! Window power on/off (CRT). Portado de la versión WebView2 legacy.
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::titlebar::ease;

const OPEN: f32 = 0.760;
const CLOSE: f32 = 0.420;
const LINEAR: Curve = [0.0, 0.0, 1.0, 1.0];
const UNFOLD: Curve = [0.22, 1.0, 0.36, 1.0];
const SQUEEZE: Curve = [0.55, 0.0, 0.75, 0.2];
const IN: Curve = [0.42, 0.0, 1.0, 1.0];
const OUT: Curve = [0.0, 0.0, 0.58, 1.0];
const IN_OUT: Curve = [0.42, 0.0, 0.58, 1.0];
const ENTER: Curve = [0.16, 1.0, 0.3, 1.0];
type Curve = [f32; 4];

enum Phase {
    Opening(Option<Instant>),
    Rest,
    Closing(Instant),
}

pub struct Tube {
    phase: Phase,
    reduced: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Layers {
    pub panel: [f32; 4], // scale X/Y, element opacity
    pub flash: [f32; 4], // scale X/Y, flash opacity, scanline opacity
    pub beam: [f32; 4],  // scale X, vertical separation / height, opacity, power off
    pub dot: [f32; 4],   // scale X/Y, opacity
}

pub struct Frame {
    pub layers: Layers,
    pub backdrop: f32,
    pub content: [f32; 3],
}

impl Tube {
    pub fn new(reduced: bool) -> Self {
        Self {
            phase: if reduced {
                Phase::Rest
            } else {
                Phase::Opening(None)
            },
            reduced,
        }
    }

    pub fn presented(&mut self, at: Instant) {
        if let Phase::Opening(first) = &mut self.phase {
            let started = *first.get_or_insert(at);
            if at - started >= Duration::from_millis(900) {
                self.phase = Phase::Rest;
            }
        }
    }

    pub fn close(&mut self, at: Instant) {
        if !self.closing() {
            self.phase = Phase::Closing(at);
        }
    }

    pub fn awaiting_present(&self) -> bool {
        matches!(self.phase, Phase::Opening(None))
    }

    pub fn closing(&self) -> bool {
        matches!(self.phase, Phase::Closing(_))
    }

    pub fn finished(&self, now: Instant) -> bool {
        matches!(self.phase, Phase::Closing(start) if self.reduced || (now - start).as_secs_f32() >= CLOSE)
    }

    pub fn opening_before(&self, now: Instant, seconds: f32) -> bool {
        match self.phase {
            Phase::Opening(Some(start)) => (now - start).as_secs_f32() < seconds,
            Phase::Opening(None) => true,
            _ => false,
        }
    }

    pub fn animating(&self, now: Instant, settle: f32) -> bool {
        !self.reduced
            && (self.opening_before(now, settle) || (self.closing() && !self.finished(now)))
    }

    pub fn frame(&self, now: Instant) -> Frame {
        if self.reduced {
            return Frame::rest();
        }
        match self.phase {
            Phase::Opening(start) => Frame::opening(start.map_or(0.0, |s| (now - s).as_secs_f32())),
            Phase::Rest => Frame::rest(),
            Phase::Closing(start) => Frame::closing((now - start).as_secs_f32()),
        }
    }
}

impl Frame {
    fn rest() -> Self {
        Self {
            layers: Layers {
                panel: [1.0, 1.0, 1.0, 0.0],
                ..Layers::zeroed()
            },
            backdrop: 1.0,
            content: [1.0; 3],
        }
    }

    fn opening(seconds: f32) -> Self {
        let mut frame = Self::rest();
        // Wallpapers has separate CSS entrance tracks for the header, tabs,
        // and workspace. The tube still transforms their common surface.
        frame.content = [
            curve((seconds - 0.30) / 0.42, ENTER),
            curve((seconds - 0.36) / 0.42, ENTER),
            curve((seconds - 0.40) / 0.50, ENTER),
        ];
        frame.backdrop = curve(seconds / 0.45, ENTER);
        if seconds >= OPEN {
            return frame;
        }
        let t = seconds / OPEN;
        let [x, y, panel] = track(
            t,
            &[
                (0.00, [0.004, 0.003, 0.0], LINEAR),
                (0.16, [0.004, 0.003, 0.0], LINEAR),
                (0.17, [0.010, 0.003, 1.0], UNFOLD),
                (0.42, [1.000, 0.003, 1.0], UNFOLD),
                (0.74, [1.000, 1.035, 1.0], IN_OUT),
                (0.86, [1.000, 0.988, 1.0], IN_OUT),
                (1.00, [1.000, 1.000, 1.0], LINEAR),
            ],
        );
        let [flash] = track(
            t,
            &[
                (0.00, [0.00], LINEAR),
                (0.16, [0.00], LINEAR),
                (0.17, [1.00], LINEAR),
                (0.46, [1.00], LINEAR),
                (0.74, [0.40], LINEAR),
                (0.90, [0.06], LINEAR),
                (1.00, [0.00], LINEAR),
            ],
        );
        let [scan] = track(
            t,
            &[
                (0.00, [0.0], LINEAR),
                (0.70, [0.0], LINEAR),
                (0.78, [0.8], LINEAR),
                (0.90, [0.4], LINEAR),
                (1.00, [0.0], LINEAR),
            ],
        );
        let [slide, beam] = track(
            t,
            &[
                (0.00, [0.000, 0.0], LINEAR),
                (0.16, [0.000, 0.0], LINEAR),
                (0.17, [0.000, 1.0], LINEAR),
                (0.42, [0.000, 1.0], OUT),
                (0.60, [0.275, 0.0], LINEAR),
            ],
        );
        let [dx, dy, dot] = track(
            t,
            &[
                (0.00, [0.1, 0.1, 0.0], LINEAR),
                (0.08, [1.4, 1.4, 1.0], UNFOLD),
                (0.17, [1.0, 1.0, 1.0], LINEAR),
                (0.30, [2.2, 0.6, 0.6], LINEAR),
                (0.42, [4.0, 0.3, 0.0], LINEAR),
            ],
        );
        frame.layers = Layers {
            panel: [x, y, panel, 0.0],
            flash: [x, y, flash, scan],
            beam: [x, slide, beam, 0.0],
            dot: [dx, dy, dot, 0.0],
        };
        frame
    }

    fn closing(seconds: f32) -> Self {
        let t = (seconds / CLOSE).min(1.0);
        let [x, y, panel] = track(
            t,
            &[
                (0.00, [1.000, 1.000, 1.0], SQUEEZE),
                (0.42, [1.010, 0.003, 1.0], SQUEEZE),
                (0.72, [0.004, 0.003, 1.0], LINEAR),
                (0.73, [0.004, 0.003, 0.0], LINEAR),
            ],
        );
        let [fx, fy, flash] = track(
            t,
            &[
                (0.00, [1.000, 1.000, 0.00], SQUEEZE),
                (0.30, [1.000, 0.300, 0.75], SQUEEZE),
                (0.42, [1.010, 0.003, 1.00], SQUEEZE),
                (0.72, [0.004, 0.003, 1.00], LINEAR),
                (0.73, [0.004, 0.003, 0.00], LINEAR),
            ],
        );
        let [width, beam] = track(
            t,
            &[
                (0.00, [1.000, 0.0], LINEAR),
                (0.38, [1.000, 0.0], LINEAR),
                (0.42, [1.000, 1.0], SQUEEZE),
                (0.72, [0.004, 1.0], LINEAR),
                (0.73, [0.004, 0.0], LINEAR),
            ],
        );
        let [scale, dot] = track(
            t,
            &[
                (0.00, [1.00, 0.0], LINEAR),
                (0.66, [1.00, 0.0], LINEAR),
                (0.74, [1.50, 1.0], LINEAR),
                (0.86, [0.80, 0.9], LINEAR),
                (1.00, [0.05, 0.0], LINEAR),
            ],
        );
        // CSS animation timing applies within the final 55%-100% segment.
        let backdrop = 1.0 - curve((seconds / 0.4 - 0.55) / 0.45, [0.4, 0.0, 0.2, 1.0]);
        Self {
            layers: Layers {
                panel: [x, y, panel, 0.0],
                flash: [fx, fy, flash, 0.0],
                beam: [width.min(x), 0.0, beam, 1.0],
                dot: [scale, scale, dot, 0.0],
            },
            backdrop,
            content: [1.0 - curve(seconds / 0.14, IN); 3],
        }
    }
}

fn curve(x: f32, easing: Curve) -> f32 {
    if x <= 0.0 || x >= 1.0 || easing == LINEAR {
        x.clamp(0.0, 1.0)
    } else {
        ease(x, easing)
    }
}

fn track<const N: usize>(t: f32, keys: &[(f32, [f32; N], Curve)]) -> [f32; N] {
    for pair in keys.windows(2) {
        let [(start, a, easing), (end, b, _)] = [pair[0], pair[1]];
        if t <= end {
            let x = curve((t - start) / (end - start), easing);
            return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * x);
        }
    }
    keys.last().unwrap().1
}

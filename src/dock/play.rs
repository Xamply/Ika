//! The dock button's motion follows the launch state of the selected instance.
use crate::titlebar::{CSS_EASE, Transition, ease};
use bytemuck::{Pod, Zeroable};
use std::time::Instant;

pub(super) const POP: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
pub(super) const PRESS: [f32; 4] = [0.32, 0.0, 0.67, 0.0];
const SCAN: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
const SLIDE: f32 = 0.44;
const FADE: f32 = 0.28;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Preparing,
    Running,
}

impl State {
    pub fn locked(self) -> bool {
        self != Self::Idle
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Frame {
    pub layers: [[f32; 4]; 4], // alpha, offset Y in SVG units, scale, reserved
    pub impact: [f32; 4],      // state, press, shockwave progress, flash alpha
    pub beams: [f32; 4],       // morph sweep, scan offset, scan alpha, orbit phase
    pub telemetry: [f32; 4],   // rotor phase, sonar spread, diamond pulse, chevrons
}

pub(super) struct Motion {
    state: State,
    changed: Option<Instant>,
    shock: Option<Instant>,
    press: Transition<1>,
    opacity: Transition<4>,
    position: Transition<4>,
}

impl Motion {
    pub fn new(now: Instant) -> Self {
        Self {
            state: State::Idle,
            changed: None,
            shock: None,
            press: Transition::new([0.], now),
            opacity: Transition::new([1., 0., 0., 0.], now),
            position: Transition::new([0., 22., 22., 22.], now),
        }
    }

    pub fn pointer_down(&mut self, now: Instant) {
        self.shock = Some(now);
    }

    pub fn animated(&self, now: Instant) -> bool {
        self.state.locked()
            || seconds(self.changed, now) < 0.58
            || seconds(self.shock, now) < 0.54
            || self.press.running(now, 0.09)
    }

    pub fn frame(
        &mut self,
        now: Instant,
        state: State,
        pressed: bool,
        still: bool,
    ) -> (Frame, Option<f32>) {
        if self.state != state {
            self.state = state;
            self.changed = (!still).then_some(now);
            self.shock = self.changed;
        }
        let lane = state as usize;
        let alpha = std::array::from_fn(|i| f32::from(i == lane));
        let pose = std::array::from_fn(|i| (i as i32 - lane as i32).signum() as f32 * 22.);
        let press = [f32::from(pressed && !state.locked())];
        let (press_time, press_curve) = if pressed { (0.06, PRESS) } else { (0.09, CSS_EASE) };
        if still {
            self.opacity = Transition::new(alpha, now);
            self.position = Transition::new(pose, now);
            self.press = Transition::new(press, now);
        } else {
            self.opacity.set(alpha, now, FADE, POP);
            self.position.set(pose, now, SLIDE, POP);
            self.press.set(press, now, press_time, press_curve);
        }
        let a = self.opacity.sample(now, FADE, POP);
        let y = self.position.sample(now, SLIDE, POP);
        let press = self.press.sample(now, press_time, press_curve)[0];
        let elapsed = seconds(self.changed, now);
        let effect = |time: f32, length: f32| if still { 1. } else { ease((time / length).min(1.), POP) };
        let motion = effect(elapsed, 0.46);
        let pop = if motion < 0.45 {
            0.962 + (1.026 - 0.962) * (motion / 0.45)
        } else {
            1.026 + (1. - 1.026) * ((motion - 0.45) / 0.55)
        };
        // During the CSS keyframe animation, its transform takes precedence over hover.
        let pop = (!still && elapsed < 0.46).then_some(pop);
        let flash = if !still && elapsed < 0.48 {
            0.52 * (1. - effect(elapsed, 0.48))
        } else {
            0.44 * press
        };
        let (orbit_period, scan_period, chevron_period) = if state == State::Running {
            (2.4, 2.6, 1.6)
        } else {
            (0.88, 1.15, 0.65)
        };
        let phase = |period: f32| {
            if still || !elapsed.is_finite() || !state.locked() { 0. } else { (elapsed / period).fract() }
        };
        let scan = phase(scan_period);
        let scan_alpha = ease((scan / 0.2).min((1. - scan) / 0.2).min(1.), SCAN);
        let pulse = |period| ease(1. - (2. * phase(period) - 1.).abs(), super::IN_OUT);
        (
            Frame {
                layers: std::array::from_fn(|i| [a[i], y[i], 1. - 0.16 * (y[i].abs() / 22.), 0.]),
                impact: [lane as f32, press, effect(seconds(self.shock, now), 0.54), flash],
                beams: [effect(elapsed, 0.58), ease(scan, SCAN), scan_alpha, phase(orbit_period)],
                telemetry: [phase(0.85), effect(phase(1.65), 1.), pulse(2.4), pulse(chevron_period)],
            },
            pop,
        )
    }
}

fn seconds(at: Option<Instant>, now: Instant) -> f32 {
    at.map_or(f32::INFINITY, |t| now.saturating_duration_since(t).as_secs_f32())
}

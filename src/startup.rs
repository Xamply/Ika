use bytemuck::{Pod, Zeroable};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const REVEAL: Duration = Duration::from_millis(200);
const SETTLE_TIMEOUT: Duration = Duration::from_millis(500);

pub struct Startup {
    first: Instant,
    ready: Option<Instant>,
    previous: Option<Instant>,
    stable: u8,
    reveal: Option<Instant>,
    reduced: bool,
    progress: Option<f32>,
    next_progress: Instant,
}

impl Startup {
    pub fn new(now: Instant, reduced: bool) -> Self {
        Self {
            first: now,
            ready: None,
            previous: None,
            stable: 0,
            reveal: None,
            reduced,
            progress: None,
            next_progress: now,
        }
    }

    pub fn loading(&self) -> bool {
        self.ready.is_none()
    }

    pub fn animating(&self) -> bool {
        !self.reduced
    }

    pub fn presented(&mut self, now: Instant, dock_ready: bool) {
        if dock_ready {
            self.ready.get_or_insert(now);
        }
    }

    pub fn update_progress(&mut self, now: Instant, source: &Path) {
        if now < self.next_progress {
            return;
        }
        self.next_progress = now + Duration::from_millis(125);
        if let Ok(text) = std::fs::read_to_string(source.with_file_name("preload-progress.txt"))
            && let Some((done, total)) = text.split_once(' ')
            && let (Ok(done), Ok(total)) = (done.parse::<u32>(), total.parse::<u32>())
            && total > 0
        {
            self.progress = Some(done.min(total) as f32 / total as f32);
        }
    }

    pub fn tick(&mut self, now: Instant) {
        let Some(ready) = self.ready else { return };
        if self.reveal.is_some() || self.reduced {
            return;
        }
        // A slow display must still run the diagonal reveal once the scene is ready.
        if now - ready >= SETTLE_TIMEOUT {
            self.reveal = Some(now);
            return;
        }
        // The reference settles on display ticks, not on media frames (which
        // can arrive at 24 FPS or be a still image).
        if self
            .previous
            .is_some_and(|prev| now - prev < Duration::from_millis(15))
        {
            return;
        }
        self.stable = if self
            .previous
            .replace(now)
            .is_some_and(|prev| now - prev <= Duration::from_millis(40))
        {
            self.stable + 1
        } else {
            0
        };
        if self.stable >= 1 {
            self.reveal = Some(now);
        }
    }

    pub fn paint(
        &self,
        now: Instant,
        size: [u32; 2],
        dpi: f32,
        source_ready: bool,
    ) -> Option<Paint> {
        let since_reveal = self.reveal.map(|start| now - start);
        if (self.reduced && !self.loading()) || since_reveal.is_some_and(|t| t >= REVEAL) {
            return None;
        }
        let seconds = if self.reduced {
            0.0
        } else {
            (now - self.first).as_secs_f32()
        };
        Some(Paint::new(
            size,
            dpi,
            seconds,
            since_reveal.map(|t| t.as_secs_f32()),
            self.progress
                .map(|value| if source_ready { 1.0 } else { value }),
        ))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Paint {
    screen: [f32; 4],      // CSS dimensions, device pixel ratio, overlay opacity
    stage: [f32; 4],       // opacity, scale, vertical translation, hover progress
    progress: [f32; 4],    // completed fraction; negative when there is no catalog batch
    leaves: [[f32; 4]; 8], // travel and blur radius in CSS pixels
    faces: [[f32; 4]; 24], // projected outer and inner corners of the wireframe
}

fn bezier(x: f32, c: [f32; 4]) -> f32 {
    if x <= 0.0 || x >= 1.0 {
        return x.clamp(0.0, 1.0);
    }
    let curve = |t: f32, a, b| {
        let u = 1.0 - t;
        t * (3.0 * u * (u * a + t * b) + t * t)
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..16 {
        let t = (lo + hi) * 0.5;
        if curve(t, c[0], c[2]) < x {
            lo = t;
        } else {
            hi = t;
        }
    }
    curve((lo + hi) * 0.5, c[1], c[3])
}

impl Paint {
    fn new(
        size: [u32; 2],
        dpi: f32,
        elapsed: f32,
        reveal: Option<f32>,
        progress: Option<f32>,
    ) -> Self {
        const EASE_IN_OUT: [f32; 4] = [0.42, 0.0, 0.58, 1.0];
        let [w, h] = size.map(|n| n as f32 / dpi);
        // Fit the original 680 ms leaf choreography into the shorter native reveal.
        let time = reveal.unwrap_or(0.0) * (0.680 / REVEAL.as_secs_f32());
        let fade = bezier(time / 0.120, [0.0, 0.0, 0.58, 1.0]);
        let hover = bezier(
            1.0 - (2.0 * (elapsed / 3.6).fract() - 1.0).abs(),
            EASE_IN_OUT,
        );
        let half = (w * 0.07).clamp(70.0, 104.0) * 0.5;
        let (sx, cx) = (-24.0f32).to_radians().sin_cos();
        let (sy, cy) = (-35.0 + 360.0 * (elapsed / 18.0).fract())
            .to_radians()
            .sin_cos();
        let project = |x, y, z| {
            let zx = -sy * x + cy * z;
            let depth = sx * y + cx * zx;
            let perspective = 850.0 / (850.0 - depth);
            [
                (cy * x + sy * z) * perspective,
                (cx * y - sx * zx - 12.0 * hover) * perspective,
            ]
        };
        let mut faces = [[0.0; 4]; 24];
        for (face, vertices) in faces.chunks_exact_mut(4).enumerate() {
            for (i, corner) in vertices.iter_mut().enumerate() {
                let [u, v] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]][i];
                let face_point = |r: f32| {
                    let fixed = if face % 2 == 0 { half } else { -half };
                    match face / 2 {
                        0 => project(u * r, v * r, fixed),
                        1 => project(fixed, u * r, v * r),
                        _ => project(v * r, fixed, u * r),
                    }
                };
                let [x, y] = face_point(half);
                let [ix, iy] = face_point(half - 1.0);
                *corner = [x, y, ix, iy];
            }
        }
        let leaves = std::array::from_fn(|i| {
            let t = ((time - i as f32 * 0.050) / 0.300).clamp(0.0, 1.0);
            let travel = bezier(t, [0.32, 0.0, 0.22, 1.0]);
            let mix = |a: f32, b: f32, t| a + (b - a) * bezier(t, EASE_IN_OUT);
            let blur = match t {
                ..0.25 => mix(0.0, 10.0, t / 0.25),
                ..0.55 => 10.0,
                ..0.80 => mix(10.0, 4.0, (t - 0.55) / 0.25),
                _ => mix(4.0, 0.0, (t - 0.80) / 0.20),
            };
            [0.207 * w * travel, -1.15 * h * travel, blur, 0.0]
        });
        Self {
            screen: [w, h, dpi, (1.0 - (time - 0.6256) / 0.0544).clamp(0.0, 1.0)],
            stage: [1.0 - fade, 1.0 - 0.08 * fade, -8.0 * fade, hover],
            progress: [progress.unwrap_or(-1.0), 0.0, 0.0, 0.0],
            leaves,
            faces,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_waits_for_a_complete_scene_in_both_motion_modes() {
        let start = Instant::now();
        let ready = start + Duration::from_secs(180);
        let size = [1280, 720];
        for reduced in [false, true] {
            let mut startup = Startup::new(start, reduced);
            startup.tick(ready);
            assert!(startup.loading());
            let cover = startup.paint(ready, size, 1.0, false).unwrap();
            assert!(cover.leaves.iter().all(|leaf| leaf[1] == 0.0));

            // Source frames alone must never uncover incomplete dock cards.
            for frame in 0..8 {
                let now = ready + Duration::from_millis(frame * 100);
                startup.presented(now, false);
                startup.tick(now);
                let cover = startup.paint(now, size, 1.0, true).unwrap();
                assert_eq!(cover.screen[3], 1.0);
                assert!(cover.leaves.iter().all(|leaf| leaf[1] == 0.0));
            }
            assert!(startup.loading());

            let ready = ready + Duration::from_secs(1);
            startup.presented(ready, true);
            assert!(!startup.loading());
            if reduced {
                assert!(startup.paint(ready, size, 1.0, true).is_none());
            } else {
                for frame in 0..=4 {
                    startup.tick(ready + Duration::from_millis(frame * 16));
                }
                let reveal = startup.reveal.unwrap();
                let mid = startup
                    .paint(reveal + REVEAL / 4, size, 1.0, true)
                    .unwrap();
                assert!(mid.leaves[0][1] < mid.leaves[1][1]);
                assert!(startup.paint(reveal + REVEAL, size, 1.0, true).is_none());
            }
        }
    }

    #[test]
    fn slow_frames_still_start_the_diagonal_reveal() {
        let start = Instant::now();
        let mut startup = Startup::new(start, false);
        startup.presented(start, true);
        for frame in 0..=8 {
            startup.tick(start + Duration::from_millis(frame * 250));
        }
        assert_eq!(startup.reveal, Some(start + SETTLE_TIMEOUT));
    }
}

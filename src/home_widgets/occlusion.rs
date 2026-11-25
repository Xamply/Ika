use crate::gpu::Params;
use egui::{Pos2, Rect, Vec2, pos2, vec2};

/// These windows are drawn by the scene shader before egui. Clip the clock
/// against their current screen silhouettes to keep the same stacking order
/// as egui dialogs, without adding a render target to the video compositor.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct ClockOcclusion {
    windows: [Option<WindowCover>; 2],
    shade: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct WindowCover {
    rect: Rect,
    radius: Vec2,
}

impl ClockOcclusion {
    pub fn from_scene(p: &Params) -> Self {
        let dpi = p.screen[3];
        let [x, y, scale, height] = p.wall_rect;
        let wall = (scale > 0. && p.wall_fade[0] > 0. && p.wall_crt.panel[2] > 0.).then(|| {
            let size = vec2(1260., height) * (scale / dpi);
            WindowCover::new(
                pos2(x / dpi, y / dpi) + size * 0.5,
                size,
                20. * scale / dpi,
                p.wall_crt.panel,
            )
        });
        let monitor_open = scale == 0. && p.monitor_ui[3] > 0.;
        let monitor =
            (monitor_open && p.monitor_view[2] > 0. && p.monitor_crt.panel[2] > 0.).then(|| {
                let [x, y, w, h] = p.monitor_rect;
                let size = vec2(w, h);
                WindowCover::new(pos2(x, y) + size * 0.5, size, 18., p.monitor_crt.panel)
            });
        let fade = p.title_phase[3];
        let notification = (fade > 0.).then(|| {
            let width = p.screen[0] / dpi;
            let size = vec2(440_f32.min(width - 24.), 180.);
            let center = pos2(width - 12. - size.x * 0.5, 138. - 6. * (1. - fade));
            let scale = 0.98 + 0.02 * fade;
            WindowCover::new(center, size, 14., [scale, scale, 0., 0.])
        });
        Self {
            windows: [wall.or(monitor), notification],
            shade: if monitor_open {
                0.85 * p.monitor_view[2]
            } else {
                0.
            },
        }
    }

    pub(super) fn opacity(self) -> f32 {
        1. - self.shade
    }

    pub(super) fn clips(self, clip: Rect, bounds: Rect, dpi: f32, mut paint: impl FnMut(Rect)) {
        let bounds = bounds.intersect(clip);
        let mut covers = self
            .windows
            .into_iter()
            .flatten()
            .filter(|cover| cover.rect.intersects(bounds));
        let Some(first) = covers.next() else {
            paint(clip);
            return;
        };
        let mut pieces = first.outside(clip, dpi);
        for cover in covers {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| cover.outside(piece, dpi))
                .collect();
        }
        for piece in pieces {
            if piece.is_positive() && piece.intersects(bounds) {
                paint(piece);
            }
        }
    }
}

impl WindowCover {
    fn new(center: Pos2, size: Vec2, radius: f32, panel: [f32; 4]) -> Self {
        let scale = vec2(panel[0], panel[1]);
        Self {
            rect: Rect::from_center_size(center, size * scale),
            radius: Vec2::splat(radius.min(0.5 * size.x.min(size.y))) * scale,
        }
    }

    fn outside(self, clip: Rect, dpi: f32) -> Vec<Rect> {
        if !self.rect.intersects(clip) {
            return vec![clip];
        }
        // Scissors use physical pixels. Follow the elliptical corners in
        // strips, merging consecutive rows that have the same side edges.
        let top = (self.rect.top() * dpi).round() as i32;
        let bottom = (self.rect.bottom() * dpi).round() as i32;
        let scale_y = |py: i32| py as f32 / dpi;
        let rect = |l, t, r, b| Rect::from_min_max(pos2(l, t), pos2(r, b)).intersect(clip);
        let mut clips = vec![
            rect(clip.left(), clip.top(), clip.right(), scale_y(top)),
            rect(clip.left(), scale_y(bottom), clip.right(), clip.bottom()),
        ];
        let mut from = top.max((clip.top() * dpi).round() as i32);
        let end = bottom.min((clip.bottom() * dpi).round() as i32);
        let flat_top = ((self.rect.top() + self.radius.y) * dpi).ceil() as i32;
        let flat_bottom = ((self.rect.bottom() - self.radius.y) * dpi).floor() as i32;
        let mut band = None;
        while from < end {
            let y = (from as f32 + 0.5) / dpi;
            let dy = (self.radius.y - (y - self.rect.top()).min(self.rect.bottom() - y)).max(0.);
            let dx = if dy > 0. && self.radius.y > 0. {
                let t = (dy / self.radius.y).min(1.);
                self.radius.x * (1. - (1. - t * t).sqrt())
            } else {
                0.
            };
            let l = ((self.rect.left() + dx) * dpi).round() / dpi;
            let r = ((self.rect.right() - dx) * dpi).round() / dpi;
            let to = if from >= flat_top && from < flat_bottom {
                end.min(flat_bottom)
            } else {
                from + 1
            };
            if let Some((start, left, right)) = band {
                if (left, right) != (l, r) {
                    clips.push(rect(clip.left(), scale_y(start), left, scale_y(from)));
                    clips.push(rect(right, scale_y(start), clip.right(), scale_y(from)));
                    band = Some((from, l, r));
                }
            } else {
                band = Some((from, l, r));
            }
            from = to;
        }
        if let Some((start, left, right)) = band {
            clips.push(rect(clip.left(), scale_y(start), left, scale_y(end)));
            clips.push(rect(right, scale_y(start), clip.right(), scale_y(end)));
        }
        clips.retain(Rect::is_positive);
        clips
    }
}

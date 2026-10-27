// PrintWindow can report success while a DirectX surface is being cleared or swapped.
// Keep those captures out of the glass layer, including clears confined to a small band.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 144;
const TILE_COLS: usize = 8;
const TILE_ROWS: usize = 8;
const TILE_W: usize = WIDTH / TILE_COLS;
const TILE_H: usize = HEIGHT / TILE_ROWS;
const CONFIRM_FRAMES: u8 = 3;

#[derive(Clone, Copy, Default)]
struct TileMetrics {
    luminance: u32,
    chroma: u32,
    range: u32,
}

impl TileMetrics {
    fn is_gray_clear(self) -> bool {
        self.luminance > 80 && self.chroma < 6 && self.range < 24
    }

    fn had_detail(self) -> bool {
        self.chroma > 18 || self.range > 80
    }
}

struct FrameMetrics {
    luminance: u32,
    bottom_right: u32,
    chroma: u32,
    range: u32,
    tiles: [TileMetrics; TILE_COLS * TILE_ROWS],
}

impl FrameMetrics {
    fn read(frame: &[u8]) -> Option<Self> {
        if frame.len() != 8 + WIDTH * HEIGHT * 4
            || frame[..4] != (WIDTH as u32).to_le_bytes()
            || frame[4..8] != (HEIGHT as u32).to_le_bytes()
        {
            return None;
        }

        let pixels = &frame[8..];
        let mut sums = [0u64; TILE_COLS * TILE_ROWS];
        let mut chromas = [0u64; TILE_COLS * TILE_ROWS];
        let mut counts = [0u64; TILE_COLS * TILE_ROWS];
        let mut minima = [u32::MAX; TILE_COLS * TILE_ROWS];
        let mut maxima = [0u32; TILE_COLS * TILE_ROWS];
        let mut total = 0u64;
        let mut chroma_total = 0u64;
        let mut count = 0u64;
        let mut bottom_right = 0u64;
        let mut bottom_count = 0u64;

        for y in (0..HEIGHT).step_by(4) {
            for x in (0..WIDTH).step_by(4) {
                let index = (y * WIDTH + x) * 4;
                let r = pixels[index] as u32;
                let g = pixels[index + 1] as u32;
                let b = pixels[index + 2] as u32;
                let luminance = r * 2 + g * 5 + b;
                let chroma = r.max(g).max(b) - r.min(g).min(b);
                let tile = (y / TILE_H) * TILE_COLS + x / TILE_W;
                sums[tile] += luminance as u64;
                chromas[tile] += chroma as u64;
                counts[tile] += 1;
                minima[tile] = minima[tile].min(luminance);
                maxima[tile] = maxima[tile].max(luminance);
                total += luminance as u64;
                chroma_total += chroma as u64;
                count += 1;
                if y >= 64 && x >= 96 {
                    bottom_right += luminance as u64;
                    bottom_count += 1;
                }
            }
        }

        let mut tiles = [TileMetrics::default(); TILE_COLS * TILE_ROWS];
        for (i, tile) in tiles.iter_mut().enumerate() {
            *tile = TileMetrics {
                luminance: (sums[i] / counts[i].max(1)) as u32,
                chroma: (chromas[i] / counts[i].max(1)) as u32,
                range: maxima[i].saturating_sub(minima[i]),
            };
        }
        Some(Self {
            luminance: (total / count) as u32,
            bottom_right: (bottom_right / bottom_count.max(1)) as u32,
            chroma: (chroma_total / count) as u32,
            range: maxima.iter().max()? - minima.iter().min()?,
            tiles,
        })
    }

    fn is_clear(&self) -> bool {
        self.luminance < 45 || (self.luminance > 80 && self.chroma < 12 && self.range < 35)
    }

    fn has_partial_clear(&self, previous: &Self) -> bool {
        self.tiles
            .iter()
            .zip(&previous.tiles)
            .any(|(current, previous)| current.is_gray_clear() && previous.had_detail())
    }

    fn is_outlier(&self, previous: &Self) -> bool {
        changed_luminance(self.luminance, previous.luminance)
            || (previous.bottom_right > 60 && self.bottom_right < previous.bottom_right * 55 / 100)
            || (previous.chroma > 18 && self.chroma < 8)
            // PrintWindow can miss one foreground layer in a small part of the scene.
            // The exposed layer can be colorful, so gray-clear detection and changes
            // spanning six tiles both miss it. Confirm even a localized abrupt change.
            || self.tiles.iter().zip(&previous.tiles).any(|(a, b)| {
                changed_luminance(a.luminance, b.luminance)
                    || (b.chroma > 18 && a.chroma < b.chroma / 4)
            })
    }

    fn agrees_with(&self, candidate: &Self) -> bool {
        self.tiles.iter().zip(&candidate.tiles).all(|(a, b)| {
            a.luminance.abs_diff(b.luminance) <= (b.luminance * 15 / 100).max(40)
                && a.chroma.abs_diff(b.chroma) <= (b.chroma / 4).max(12)
                && a.range.abs_diff(b.range) <= (b.range / 4).max(40)
        })
    }
}

fn changed_luminance(current: u32, previous: u32) -> bool {
    previous > 60 && (current < previous * 55 / 100 || current > previous * 165 / 100)
}

pub struct FrameCache {
    source: Option<(u64, isize)>,
    frame: Option<Vec<u8>>,
    metrics: Option<FrameMetrics>,
    candidate: Option<(FrameMetrics, u8)>,
}

impl FrameCache {
    pub const fn new() -> Self {
        Self {
            source: None,
            frame: None,
            metrics: None,
            candidate: None,
        }
    }

    pub fn update(&mut self, source: (u64, isize), frame: Option<Vec<u8>>) -> Option<Vec<u8>> {
        // A restarted scene may reuse its HWND; the play/stop generation also matters.
        if self.source != Some(source) {
            *self = Self::new();
            self.source = Some(source);
        }
        let captured = frame.and_then(|frame| Some((FrameMetrics::read(&frame)?, frame)));
        let Some((metrics, frame)) = captured else {
            self.candidate = None;
            return self.frame.clone();
        };

        if metrics.is_clear()
            || self
                .metrics
                .as_ref()
                .is_some_and(|previous| metrics.has_partial_clear(previous))
        {
            self.candidate = None;
            return self.frame.clone();
        }

        if self
            .metrics
            .as_ref()
            .is_some_and(|previous| metrics.is_outlier(previous))
        {
            // Count only consecutive captures that agree with each other. A sequence of
            // unrelated bad frames must never exhaust a retry budget and become visible.
            let confirmations = self
                .candidate
                .take()
                .filter(|(candidate, _)| metrics.agrees_with(candidate))
                .map_or(1, |(_, count)| count + 1);
            if confirmations < CONFIRM_FRAMES {
                self.candidate = Some((metrics, confirmations));
                return self.frame.clone();
            }
        }

        self.candidate = None;
        self.metrics = Some(metrics);
        self.frame = Some(frame);
        self.frame.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: (u64, isize) = (1, 42);

    fn scene(pixel: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(8 + WIDTH * HEIGHT * 4);
        frame.extend_from_slice(&(WIDTH as u32).to_le_bytes());
        frame.extend_from_slice(&(HEIGHT as u32).to_le_bytes());
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                frame.extend_from_slice(&pixel(x, y));
                frame.push(255);
            }
        }
        frame
    }

    fn neon_scene(offset: u8) -> Vec<u8> {
        scene(|x, y| {
            [
                (80 + x % 96) as u8 + offset,
                (35 + y % 36) as u8 + offset,
                (120 + (x + y) % 56) as u8 + offset,
            ]
        })
    }

    fn with_colored_tile(frame: &[u8], tile_x: usize, tile_y: usize) -> Vec<u8> {
        let mut patched = frame.to_vec();
        for y in tile_y * TILE_H..(tile_y + 1) * TILE_H {
            for x in tile_x * TILE_W..(tile_x + 1) * TILE_W {
                let index = 8 + (y * WIDTH + x) * 4;
                patched[index..index + 3].copy_from_slice(&[22, 33, 72]);
            }
        }
        patched
    }

    #[test]
    fn gray_band_with_similar_brightness_never_replaces_good_frame() {
        let good = neon_scene(0);
        let mut gray = good.clone();
        for y in 90..108 {
            for x in 128..192 {
                let index = 8 + (y * WIDTH + x) * 4;
                gray[index..index + 3].fill(91);
            }
        }
        let mut cache = FrameCache::new();
        assert_eq!(cache.update(SOURCE, Some(good.clone())), Some(good.clone()));
        for _ in 0..8 {
            assert_eq!(cache.update(SOURCE, Some(gray.clone())), Some(good.clone()));
        }
        let recovered = neon_scene(1);
        assert_eq!(
            cache.update(SOURCE, Some(recovered.clone())),
            Some(recovered)
        );
    }

    #[test]
    fn blank_and_uniform_gray_captures_keep_last_frame() {
        let good = neon_scene(0);
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        for color in [[0, 0, 0], [96, 96, 96]] {
            for _ in 0..8 {
                assert_eq!(
                    cache.update(SOURCE, Some(scene(|_, _| color))),
                    Some(good.clone())
                );
            }
        }
    }

    #[test]
    fn unrelated_outliers_do_not_exhaust_the_filter() {
        let good = neon_scene(0);
        let bright = neon_scene(80);
        let dark = scene(|x, y| {
            [
                ((80 + x % 96) / 3) as u8,
                ((35 + y % 36) / 3) as u8,
                ((120 + (x + y) % 56) / 3) as u8,
            ]
        });
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        for frame in [bright, dark].iter().cycle().take(8) {
            assert_eq!(
                cache.update(SOURCE, Some(frame.clone())),
                Some(good.clone())
            );
        }
    }

    #[test]
    fn consistent_lighting_change_is_accepted_without_freezing() {
        let good = neon_scene(0);
        let bright = neon_scene(80);
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        for _ in 0..2 {
            assert_eq!(
                cache.update(SOURCE, Some(bright.clone())),
                Some(good.clone())
            );
        }
        assert_eq!(cache.update(SOURCE, Some(bright.clone())), Some(bright));
    }

    #[test]
    fn isolated_colored_layer_drop_keeps_frame_until_capture_recovers() {
        let good = neon_scene(0);
        let recovered = neon_scene(1);
        for (tile_x, tile_y) in [(0, 0), (3, 1), (3, 7)] {
            let missing_layer = with_colored_tile(&good, tile_x, tile_y);
            let mut cache = FrameCache::new();
            cache.update(SOURCE, Some(good.clone()));
            for _ in 0..2 {
                assert!(
                    cache.update(SOURCE, Some(missing_layer.clone())).as_deref()
                        == Some(good.as_slice()),
                    "a colored layer missing in tile ({tile_x}, {tile_y}) must not flash"
                );
            }
            assert!(
                cache.update(SOURCE, Some(recovered.clone())).as_deref()
                    == Some(recovered.as_slice()),
                "the next valid animation frame must be shown immediately"
            );
        }
    }

    #[test]
    fn persistent_local_lighting_change_is_confirmed() {
        let good = neon_scene(0);
        let changed = with_colored_tile(&good, 3, 1);
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        for _ in 0..2 {
            assert!(
                cache.update(SOURCE, Some(changed.clone())).as_deref() == Some(good.as_slice())
            );
        }
        assert!(cache.update(SOURCE, Some(changed.clone())).as_deref() == Some(changed.as_slice()));
        assert!(cache.update(SOURCE, None).as_deref() == Some(changed.as_slice()));
    }

    #[test]
    fn ordinary_animation_is_not_delayed() {
        let mut cache = FrameCache::new();
        for offset in 0..12 {
            let frame = neon_scene(offset);
            assert_eq!(cache.update(SOURCE, Some(frame.clone())), Some(frame));
        }
    }

    #[test]
    fn textured_monochrome_is_not_a_gray_clear() {
        let monochrome = scene(|x, y| {
            let value = if (x / 4 + y / 4) % 2 == 0 { 40 } else { 180 };
            [value; 3]
        });
        let mut cache = FrameCache::new();
        assert_eq!(
            cache.update(SOURCE, Some(monochrome.clone())),
            Some(monochrome.clone())
        );
        cache.update((2, 42), Some(neon_scene(0)));
        for _ in 0..3 {
            cache.update((2, 42), Some(monochrome.clone()));
        }
        assert_eq!(cache.update((2, 42), None), Some(monochrome));
    }

    #[test]
    fn invalid_capture_breaks_transition_confirmation() {
        let good = neon_scene(0);
        let bright = neon_scene(80);
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        cache.update(SOURCE, Some(bright.clone()));
        assert_eq!(
            cache.update(SOURCE, Some(good[..100].to_vec())),
            Some(good.clone())
        );
        for _ in 0..2 {
            assert_eq!(
                cache.update(SOURCE, Some(bright.clone())),
                Some(good.clone())
            );
        }
        assert_eq!(cache.update(SOURCE, Some(bright.clone())), Some(bright));
    }

    #[test]
    fn dimensions_and_source_must_match() {
        let good = neon_scene(0);
        let mut invalid = good.clone();
        invalid[..4].copy_from_slice(&128u32.to_le_bytes());
        let mut cache = FrameCache::new();
        cache.update(SOURCE, Some(good.clone()));
        assert_eq!(cache.update(SOURCE, Some(invalid)), Some(good.clone()));
        assert_eq!(cache.update((2, 42), None), None);
        let next = neon_scene(80);
        assert_eq!(cache.update((2, 42), Some(next.clone())), Some(next));
        assert_eq!(cache.update((2, 43), None), None);
        assert_eq!(cache.update((2, 43), Some(good.clone())), Some(good));
    }
}

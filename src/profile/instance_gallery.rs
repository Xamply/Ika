use super::*;
use std::collections::VecDeque;

const DWELL: Duration = Duration::from_secs(2);
const SLIDE: Duration = Duration::from_millis(580);
const MAX_CACHED: usize = 4;

#[derive(Default)]
pub(super) struct Gallery {
    index: usize,
    since: Option<Instant>,
    picked: Option<usize>,
    slide: Option<Slide>,
    request: Option<String>,
    decoded: Option<(String, Option<image::RgbaImage>)>,
    textures: VecDeque<(String, Option<TextureHandle>)>,
}

struct Slide {
    started: Instant,
    previous: TextureHandle,
    direction: f32,
}

pub(super) struct Frame {
    pub index: usize,
    pub changed: bool,
    pub progress: Option<f32>,
    pub texture: Option<TextureHandle>,
    pub request: Option<String>,
    pub transition: Option<(TextureHandle, f32, f32)>,
}

impl Gallery {
    pub fn reset(&mut self) {
        // Leave the one image download already in flight to finish.
        *self = Self {
            request: self.request.take(),
            ..Self::default()
        };
    }

    pub fn pause(&mut self) {
        self.since = None;
        self.slide = None;
    }

    pub fn pick(&mut self, index: usize) {
        self.picked = Some(index);
    }

    pub fn receive(&mut self, url: String, image: Option<image::RgbaImage>) {
        if self.request.as_ref() == Some(&url) {
            self.request = None;
            self.decoded = Some((url, image));
        }
    }

    fn cached(&self, url: &str) -> Option<&Option<TextureHandle>> {
        self.textures
            .iter()
            .find(|(key, _)| key == url)
            .map(|(_, texture)| texture)
    }

    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        shots: &[String],
        bundled: Option<&(String, TextureHandle)>,
        thumbnails: &HashMap<String, TextureHandle>,
        animate: bool,
    ) -> Frame {
        let now = Instant::now();
        if let Some((url, image)) = self.decoded.take()
            && shots.contains(&url)
        {
            self.textures.retain(|(key, _)| *key != url);
            if self.textures.len() >= MAX_CACHED {
                let current = shots.get(self.index);
                let evict = self
                    .textures
                    .iter()
                    .position(|(key, _)| Some(key) != current)
                    .unwrap_or(0);
                self.textures.remove(evict);
            }
            self.textures.push_back((
                url,
                image.map(|image| {
                    load_image(
                        ctx,
                        "modpack-screenshot",
                        &image,
                        egui::TextureOptions::LINEAR,
                    )
                }),
            ));
        }
        let bundled_for = |url: &str| {
            bundled
                .filter(|(key, _)| key == url)
                .map(|(_, texture)| texture)
        };
        let texture_for = |gallery: &Self, index: usize| {
            let url = shots.get(index)?;
            bundled_for(url)
                .or_else(|| gallery.cached(url).and_then(Option::as_ref))
                .or_else(|| thumbnails.get(url))
                .cloned()
        };
        self.index = self.index.min(shots.len().saturating_sub(1));
        if !animate || shots.len() < 2 {
            self.pause();
        }
        let next_after = |gallery: &Self| {
            (1..shots.len())
                .map(|i| (gallery.index + i) % shots.len())
                .find(|&i| {
                    gallery.cached(&shots[i]).is_none_or(Option::is_some)
                        || texture_for(gallery, i).is_some()
                })
        };
        let mut texture = texture_for(self, self.index);
        let manual = self.picked.filter(|&i| i < shots.len());
        let cycling = animate && shots.len() > 1;
        if cycling && texture.is_some() {
            self.since.get_or_insert(now);
        }
        let next = cycling.then(|| next_after(self)).flatten();
        let expired = self.since.is_some_and(|since| now - since >= DWELL);
        let target = manual.or_else(|| expired.then_some(next).flatten());
        let mut changed = false;
        if let Some(target) = target {
            if target == self.index {
                self.since = cycling.then_some(now);
                self.picked = None;
            } else if let Some(incoming) = texture_for(self, target) {
                if animate && let Some(previous) = texture.take() {
                    self.slide = Some(Slide {
                        started: now,
                        previous,
                        direction: if manual.is_some() && target < self.index {
                            -1.
                        } else {
                            1.
                        },
                    });
                }
                texture = Some(incoming);
                self.index = target;
                self.since = cycling.then_some(now);
                self.picked = None;
                changed = true;
            } else if self.cached(&shots[target]).is_some() && manual.is_some() {
                self.picked = None;
            }
        }

        let request = if self.request.is_none() {
            // Keep the current photo sharp and prefetch one image ahead.
            std::iter::once(Some(self.index))
                .chain([self.picked, cycling.then(|| next_after(self)).flatten()])
                .flatten()
                .filter_map(|index| shots.get(index))
                .find(|url| bundled_for(url).is_none() && self.cached(url).is_none())
                .cloned()
        } else {
            None
        };
        if let Some(url) = &request {
            self.request = Some(url.clone());
        }
        let progress = self.since.filter(|_| next.is_some()).map(|since| {
            let elapsed = now - since;
            if elapsed < DWELL {
                ctx.request_repaint_after((DWELL - elapsed).min(Duration::from_millis(33)));
            }
            (elapsed.as_secs_f32() / DWELL.as_secs_f32()).min(1.)
        });
        if self
            .slide
            .as_ref()
            .is_some_and(|slide| now - slide.started >= SLIDE)
        {
            self.slide = None;
        }
        let transition = self.slide.as_ref().map(|slide| {
            ctx.request_repaint_after(Duration::from_millis(16));
            let time = (now - slide.started).as_secs_f32() / SLIDE.as_secs_f32();
            let t = crate::titlebar::ease(time.clamp(0., 1.), [0.22, 1., 0.36, 1.]);
            (slide.previous.clone(), slide.direction, t)
        });
        Frame {
            index: self.index,
            changed,
            progress,
            texture,
            request,
            transition,
        }
    }
}

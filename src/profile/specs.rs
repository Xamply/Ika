//! The instance specs strip under Play (mods, RAM, loader, JVM, shaders,
//! packs, play time). Ported from the WebView2 `instance-specs-bar`: same
//! texts and formats. The icons and separators are baked into the dock
//! chassis; this draws the values over it. Ctrl + wheel over the strip zooms
//! its text (0.65..1.75, saved like `ika_specs_bar_zoom`).
use crate::minecraft;
use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, pos2, vec2};
use std::time::Instant;
use winit::window::Window;

const FILE: &str = "specs-bar.json";
const ZOOM_MIN: f32 = 0.65;
const ZOOM_MAX: f32 = 1.75;
const ZOOM_STEP: f32 = 0.05;
const ZOOM_TIME: f32 = 0.16;
const SWAP_TIME: f32 = 0.28;
const VALUE: Color32 = Color32::WHITE;
const LABEL: Color32 = Color32::from_rgb(0x7b, 0x8a, 0x9e);

/// Whether each item shows "value label" on one line (true) or value over label.
const INLINE: [bool; 7] = [true, false, false, false, true, true, false];
/// Center of the strip and its baseline, in dock canvas pixels (under Play).
const CENTER_X: f32 = 1280.0;
const CENTER_Y: f32 = 300.0;
const ICONS: &[u8] = include_bytes!("specs-icons.png");
const ICON_CELL: f32 = 32.0;
/// Sizes at zoom 1, in canvas pixels, measured on the WebView2 bar baked in
/// the old chassis: 20 px glyphs, 12/10 px text, 14 px separators.
// Text keeps the size that already looked right; icons and separators were
// measured against it on the WebView2 bar (glyph ~1.15x the value text,
// separator ~0.85x), so all of them grow and shrink together.
const VALUE_SIZE: f32 = 24.0;
const LABEL_SIZE: f32 = 20.0;
const ICON: f32 = 28.0;
const ICON_GAP: f32 = 11.0;
const ITEM_GAP: f32 = 22.0;
const SEPARATOR: f32 = 24.0;
const SEPARATOR_COLOR: Color32 = Color32::from_rgb(65, 65, 78);

#[derive(Clone, PartialEq)]
struct Item {
    value: String,
    label: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    zoom: f32,
}

pub struct Specs {
    items: [Item; 7],
    /// Previous texts, fading out while the new ones come in.
    previous: Option<([Item; 7], Instant)>,
    key: String,
    zoom: f32,
    zoom_from: f32,
    zoom_started: Option<Instant>,
    reduced_motion: bool,
    icons: Option<egui::TextureHandle>,
    extent: Rect,
}

fn empty() -> [Item; 7] {
    std::array::from_fn(|i| Item {
        value: "—".into(),
        label: ["mods", "RAM", "Loader", "JVM", "shaders", "packs", "Tiempo jugado"][i].into(),
    })
}

pub fn play_time(seconds: u64) -> String {
    if seconds == 0 {
        return "0 hrs".into();
    }
    let (days, hours, mins) = (seconds / 86400, seconds % 86400 / 3600, seconds % 3600 / 60);
    if days > 0 {
        if hours > 0 { format!("{days}d {hours}h") } else { format!("{days}d") }
    } else if hours > 0 {
        if mins > 0 { format!("{hours}h {mins}m") } else { format!("{hours}h") }
    } else {
        format!("{mins} min")
    }
}

pub fn ram(mb: u32) -> String {
    let gb = format!("{:.1}", mb as f32 / 1024.0);
    format!("{} GB", gb.strip_suffix(".0").unwrap_or(&gb))
}

/// "Java N", following WebView2's detectJavaDistribution.
fn java(instance: &minecraft::Instance) -> String {
    if let Some(v) = instance.java_version.as_deref().filter(|v| !v.is_empty()) {
        return if v.starts_with("Java") { v.into() } else { format!("Java {v}") };
    }
    let mc = instance.mc_version.as_str();
    if ["1.20.5", "1.21", "1.22"].iter().any(|p| mc.starts_with(p)) {
        "Java 21".into()
    } else if ["1.18", "1.19", "1.20"].iter().any(|p| mc.starts_with(p)) {
        "Java 17".into()
    } else if mc.starts_with("1.17") {
        "Java 16".into()
    } else {
        "Java 8".into()
    }
}

/// WebView2 shows "Eclipse Temurin" for the Adoptium builds.
fn vendor(name: &str) -> String {
    if name == "Adoptium" { "Eclipse Temurin".into() } else { name.into() }
}

fn items(instance: &minecraft::Instance) -> [Item; 7] {
    let plural = |n: usize, one: &str, many: &str| if n == 1 { one.to_owned() } else { many.to_owned() };
    let loader = if instance.loader.is_empty() { "Fabric".to_owned() } else { instance.loader.clone() };
    [
        Item { value: instance.mod_count.to_string(), label: "mods".into() },
        Item { value: ram(instance.max_mem_mb), label: "RAM".into() },
        Item { value: loader, label: "Loader".into() },
        Item { value: java(instance), label: vendor(instance.java_vendor) },
        Item { value: instance.shader_count.to_string(), label: plural(instance.shader_count, "shader", "shaders") },
        Item { value: instance.resourcepack_count.to_string(), label: plural(instance.resourcepack_count, "pack", "packs") },
        Item { value: play_time(instance.total_time_played.unwrap_or(0)), label: "Tiempo jugado".into() },
    ]
}

impl Specs {
    pub fn new(reduced_motion: bool) -> Self {
        let zoom = std::fs::read(super::storage::native_dir().join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok())
            .map_or(1.0, |s| s.zoom)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        Self {
            items: empty(),
            previous: None,
            key: String::new(),
            zoom,
            zoom_from: zoom,
            zoom_started: None,
            reduced_motion,
            icons: None,
            extent: Rect::NOTHING,
        }
    }

    /// Takes the selected instance; changes cross-fade. Returns whether to redraw.
    pub fn update(&mut self, instance: Option<&minecraft::Instance>, now: Instant) -> bool {
        let next = instance.map_or_else(empty, items);
        let key = instance.map_or(String::new(), |i| i.path.clone());
        if next == self.items && key == self.key {
            return false;
        }
        let old = std::mem::replace(&mut self.items, next);
        if !self.reduced_motion && !self.key.is_empty() {
            self.previous = Some((old, now));
        }
        self.key = key;
        true
    }

    fn zoom_now(&self, now: Instant) -> f32 {
        let Some(start) = self.zoom_started else { return self.zoom };
        let t = (now.saturating_duration_since(start).as_secs_f32() / ZOOM_TIME).min(1.0);
        let e = crate::titlebar::ease(t, [0.16, 1.0, 0.3, 1.0]);
        self.zoom_from + (self.zoom - self.zoom_from) * e
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.zoom_started.is_some_and(|s| now.saturating_duration_since(s).as_secs_f32() < ZOOM_TIME)
            || self.previous.as_ref().is_some_and(|(_, s)| now.saturating_duration_since(*s).as_secs_f32() < SWAP_TIME)
    }

    /// The strip's area in logical points, for Ctrl + wheel: the last drawn
    /// extent, or its resting place before the first paint.
    pub fn bounds(&self, window: &Window) -> Rect {
        if self.extent.is_positive() {
            return self.extent;
        }
        let (x, y, d) = crate::dock::canvas_to_logical(window, CENTER_X, CENTER_Y);
        Rect::from_center_size(pos2(x, y), vec2(780.0 * d, 46.0 * d))
    }

    pub fn wheel(&mut self, steps: f32, now: Instant) -> bool {
        let target = ((self.zoom + steps.signum() * ZOOM_STEP) * 100.0).round() / 100.0;
        let target = target.clamp(ZOOM_MIN, ZOOM_MAX);
        if target == self.zoom {
            return false;
        }
        self.zoom_from = self.zoom_now(now);
        self.zoom = target;
        self.zoom_started = (!self.reduced_motion).then_some(now);
        if let Ok(json) = serde_json::to_vec(&Saved { zoom: target }) {
            let _ = super::storage::save_native(FILE, &json);
        }
        true
    }

    pub fn paint(&mut self, painter: &Painter, window: &Window, now: Instant) {
        let icons = self
            .icons
            .get_or_insert_with(|| {
                let image = image::load_from_memory(ICONS).expect("specs icons").to_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                painter.ctx().load_texture("specs-icons", color, egui::TextureOptions::LINEAR)
            })
            .id();
        let zoom = self.zoom_now(now);
        let fade = match &self.previous {
            Some((_, start)) => {
                let t = (now.saturating_duration_since(*start).as_secs_f32() / SWAP_TIME).min(1.0);
                crate::titlebar::ease(t, [0.16, 1.0, 0.3, 1.0])
            }
            None => 1.0,
        };
        if fade >= 1.0 {
            self.previous = None;
        }
        // Layout follows the current texts; during a swap, the wider of the
        // two lays out the row so icons and separators never jump.
        let (cx, cy, d) = crate::dock::canvas_to_logical(window, CENTER_X, CENTER_Y);
        let unit = d * zoom;
        let fonts = Fonts::new(unit);
        let widths: [f32; 7] = std::array::from_fn(|i| {
            let now_w = text_width(painter, &self.items[i], INLINE[i], &fonts);
            let old_w = self.previous.as_ref().map_or(0.0, |(old, _)| text_width(painter, &old[i], INLINE[i], &fonts));
            now_w.max(old_w)
        });
        let item_w = |i: usize| (ICON + ICON_GAP) * unit + widths[i];
        let total: f32 = (0..7).map(item_w).sum::<f32>() + 6.0 * (2.0 * ITEM_GAP + 1.0) * unit;
        let mut x = cx - total * 0.5;
        self.extent = Rect::from_center_size(pos2(cx, cy), vec2(total + 24.0 * unit, 46.0 * unit));
        for i in 0..7 {
            let icon = Rect::from_min_size(pos2(x, cy - ICON * unit * 0.5), vec2(ICON * unit, ICON * unit));
            let u = i as f32 * ICON_CELL / (7.0 * ICON_CELL);
            painter.image(
                icons,
                icon,
                Rect::from_min_max(pos2(u, 0.0), pos2(u + 1.0 / 7.0, 1.0)),
                Color32::WHITE,
            );
            let text_x = icon.right() + ICON_GAP * unit;
            if let Some((old, _)) = &self.previous {
                draw_text(painter, pos2(text_x, cy - 4.0 * unit * fade), &old[i], INLINE[i], &fonts, 1.0 - fade, unit);
            }
            draw_text(painter, pos2(text_x, cy + 4.0 * unit * (1.0 - fade)), &self.items[i], INLINE[i], &fonts, fade, unit);
            x += item_w(i);
            if i < 6 {
                let sep_x = x + ITEM_GAP * unit;
                painter.line_segment(
                    [pos2(sep_x, cy - SEPARATOR * unit * 0.5), pos2(sep_x, cy + SEPARATOR * unit * 0.5)],
                    egui::Stroke::new(2.0 * unit, SEPARATOR_COLOR),
                );
                x = sep_x + ITEM_GAP * unit + unit;
            }
        }
    }
}

struct Fonts {
    value: FontId,
    label: FontId,
}

impl Fonts {
    fn new(unit: f32) -> Self {
        Self {
            value: FontId::new(VALUE_SIZE * unit, FontFamily::Name("ika-semibold".into())),
            label: FontId::new(LABEL_SIZE * unit, FontFamily::Proportional),
        }
    }
}

fn galley_width(painter: &Painter, text: &str, font: &FontId) -> f32 {
    painter.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size().x
}

fn text_width(painter: &Painter, item: &Item, inline: bool, fonts: &Fonts) -> f32 {
    if inline {
        let space = galley_width(painter, " ", &fonts.value);
        galley_width(painter, &item.value, &fonts.value) + space + galley_width(painter, &item.label, &fonts.value)
    } else {
        galley_width(painter, &item.value, &fonts.value).max(galley_width(painter, &item.label, &fonts.label))
    }
}

fn draw_text(painter: &Painter, at: Pos2, item: &Item, inline: bool, fonts: &Fonts, alpha: f32, unit: f32) {
    if alpha <= 0.0 {
        return;
    }
    let tint = |c: Color32| c.gamma_multiply(alpha);
    if inline {
        let r = painter.text(at, Align2::LEFT_CENTER, &item.value, fonts.value.clone(), tint(VALUE));
        let space = galley_width(painter, " ", &fonts.value);
        painter.text(r.right_center() + vec2(space, 0.0), Align2::LEFT_CENTER, &item.label, fonts.value.clone(), tint(VALUE));
    } else {
        // Value and label sit tight, like the WebView2 bar (line-height ~1.1):
        // the glyph boxes carry their own ascent/descent padding, so overlap them.
        let overlap = 5.0 * unit;
        painter.text(at + vec2(0.0, overlap * 0.5), Align2::LEFT_BOTTOM, &item.value, fonts.value.clone(), tint(VALUE));
        painter.text(at - vec2(0.0, overlap * 0.5), Align2::LEFT_TOP, &item.label, fonts.label.clone(), tint(LABEL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_match_webview2() {
        assert_eq!(play_time(0), "0 hrs");
        assert_eq!(play_time(59), "0 min");
        assert_eq!(play_time(3 * 60), "3 min");
        assert_eq!(play_time(2 * 3600), "2h");
        assert_eq!(play_time(2 * 3600 + 5 * 60), "2h 5m");
        assert_eq!(play_time(86400 * 3 + 3600 * 4), "3d 4h");
        assert_eq!(ram(8192), "8 GB");
        assert_eq!(ram(6656), "6.5 GB");
        assert_eq!(ram(4096), "4 GB");
    }
}
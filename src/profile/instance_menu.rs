//! The instance menus of the WebView2 launcher: right click on a carousel card
//! (`carousel-context-menu`) and the dock's "⋮" button (`cyber-play-more-menu`).
//! Same items and order; each opens with a springy reveal, its items cascade
//! in, every icon has its own hover motion, and closing animates too.
use egui::{Align2, Color32 as C, FontFamily, FontId, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use std::{f32::consts::TAU, time::Instant};

const OPEN: f32 = 0.30;
const CLOSE: f32 = 0.18;
const HOVER: f32 = 0.20;
const ITEM_H: f32 = 34.0;
const WIDTH: f32 = 258.0;
const SPRING: [f32; 4] = [0.175, 0.885, 0.32, 1.275];
const OUT: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
const FILL: C = C::from_rgb(0x04, 0x04, 0x16);
/// Same loop as the titlebar's outer edge (title_phase.x: 2400 px in 18 s).
const RAMP_PERIOD: f32 = 18.0;

/// The titlebar's animated edge gradient (row 0 of titlebar/atlas.png),
/// decoded once so the menus use exactly its colors.
fn ramp() -> &'static [C] {
    static RAMP: std::sync::OnceLock<Vec<C>> = std::sync::OnceLock::new();
    RAMP.get_or_init(|| {
        let atlas = image::load_from_memory(include_bytes!("../titlebar/atlas.png"))
            .map(|i| i.to_rgba8())
            .ok();
        (0..2400u32)
            .map(|i| {
                atlas.as_ref().map_or(C::from_rgb(120, 0, 255), |a| {
                    let p = a.get_pixel(i % 600, i / 600);
                    C::from_rgb(p[0], p[1], p[2])
                })
            })
            .collect()
    })
}

fn ramp_at(x: f32) -> C {
    let r = ramp();
    let phase = x.rem_euclid(2400.0);
    let i = phase as usize;
    lerp(r[i % 2400], r[(i + 1) % 2400], phase.fract())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Play,
    Stop,
    Edit,
    Explorer,
    Duplicate,
    Delete,
    Manager,
    Compare,
    Mods,
    ResourcePacks,
    Shaders,
    Screenshots,
    Rename,
    Console,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Play,
    Stop,
    Pencil,
    Folder,
    Copy,
    Trash,
    Layers,
    Scale,
    Cube,
    Image,
    Sun,
    Camera,
    Rename,
    Terminal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Normal,
    Warn,
    Danger,
}

struct Item {
    action: Action,
    label: &'static str,
    icon: Icon,
    tint: C,
    tone: Tone,
    /// A separator line before this item.
    gap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Carousel,
    Dock,
}

pub struct Menu {
    pub kind: Kind,
    pub path: String,
    title: String,
    subtitle: String,
    items: Vec<Item>,
    anchor: Pos2,
    opened: Instant,
    closing: Option<Instant>,
    hover: Vec<(f32, Instant, f32)>,
    hovered: Option<usize>,
    /// Asks before moving the instance to the recycle folder.
    confirm: Option<Instant>,
    reduced_motion: bool,
    /// Launcher clock for the border loop.
    born: Instant,
}

fn ease(t: f32, curve: [f32; 4]) -> f32 {
    crate::titlebar::ease(t.clamp(0.0, 1.0), curve)
}

impl Menu {
    pub fn carousel(instance: &crate::minecraft::Instance, running: bool, at: Pos2, now: Instant, reduced_motion: bool) -> Self {
        let mut items = Vec::new();
        if running {
            items.push(item(Action::Stop, "Detener instancia", Icon::Stop, C::from_rgb(0xfb, 0xbf, 0x24), Tone::Warn, false));
        } else {
            items.push(item(Action::Play, "Jugar esta instancia", Icon::Play, C::from_rgb(0x00, 0xf5, 0xd4), Tone::Normal, false));
        }
        items.extend([
            item(Action::Edit, "Editar instancia", Icon::Pencil, C::from_rgb(0xfb, 0xbf, 0x24), Tone::Normal, false),
            item(Action::Explorer, "Abrir en el explorador", Icon::Folder, C::from_rgb(0xc0, 0x84, 0xfc), Tone::Normal, false),
            item(Action::Duplicate, "Duplicar instancia", Icon::Copy, C::from_rgb(0x38, 0xbd, 0xf8), Tone::Normal, false),
            item(Action::Delete, "Eliminar instancia", Icon::Trash, C::from_rgb(0xf8, 0x71, 0x71), Tone::Danger, true),
        ]);
        let loader = if instance.loader.is_empty() { "Forge" } else { &instance.loader };
        let version = if instance.mc_version.is_empty() { "1.20.1" } else { &instance.mc_version };
        Self::new(Kind::Carousel, instance, items, at, format!("Minecraft {version} · {loader}"), now, reduced_motion)
    }

    pub fn dock(instance: &crate::minecraft::Instance, running: bool, above: Pos2, now: Instant, reduced_motion: bool) -> Self {
        let violet = C::from_rgb(0xa8, 0x55, 0xf7);
        let mut items = vec![
            item(Action::Manager, "Abrir en el Gestor de Instancia", Icon::Layers, violet, Tone::Normal, false),
            item(Action::Compare, "Comparar con otra instancia", Icon::Scale, violet, Tone::Normal, false),
            item(Action::Mods, "Abrir carpeta de mods", Icon::Cube, violet, Tone::Normal, true),
            item(Action::ResourcePacks, "Abrir carpeta de resource packs", Icon::Image, violet, Tone::Normal, false),
            item(Action::Shaders, "Abrir carpeta de shaders", Icon::Sun, violet, Tone::Normal, false),
            item(Action::Screenshots, "Abrir carpeta de capturas", Icon::Camera, violet, Tone::Normal, false),
            item(Action::Rename, "Renombrar instancia", Icon::Rename, violet, Tone::Normal, true),
            item(Action::Console, "Ver consola de registros", Icon::Terminal, violet, Tone::Normal, false),
            item(Action::Duplicate, "Duplicar instancia", Icon::Copy, violet, Tone::Normal, false),
        ];
        if running {
            items.push(item(Action::Stop, "Detener instancia", Icon::Stop, C::from_rgb(0xfb, 0xbf, 0x24), Tone::Warn, true));
        }
        items.push(item(Action::Delete, "Eliminar instancia", Icon::Trash, C::from_rgb(0xf8, 0x71, 0x71), Tone::Danger, !running));
        let mut menu = Self::new(Kind::Dock, instance, items, above, String::new(), now, reduced_motion);
        menu.title.clear();
        menu
    }

    fn new(kind: Kind, instance: &crate::minecraft::Instance, items: Vec<Item>, anchor: Pos2, subtitle: String, now: Instant, reduced_motion: bool) -> Self {
        let hover = items.iter().map(|_| (0.0, now, 0.0)).collect();
        Self {
            kind,
            path: instance.path.clone(),
            title: instance.name.to_uppercase(),
            subtitle,
            items,
            anchor,
            opened: now,
            closing: None,
            hover,
            hovered: None,
            confirm: None,
            reduced_motion,
            born: now,
        }
    }

    pub fn close(&mut self, now: Instant) {
        if self.closing.is_none() {
            self.closing = Some(if self.reduced_motion { now - std::time::Duration::from_secs(1) } else { now });
        }
    }

    pub fn closing(&self) -> bool {
        self.closing.is_some()
    }

    pub fn finished(&self, now: Instant) -> bool {
        self.closing.is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() >= CLOSE)
    }

    pub fn animating(&self, now: Instant) -> bool {
        // The border gradient flows while the menu is open (like the titlebar);
        // icon motions loop while an item is hovered.
        !self.reduced_motion
            || self.hovered.is_some()
            || now.saturating_duration_since(self.opened).as_secs_f32() < OPEN + 0.3
            || self.closing.is_some()
            || self.hover.iter().any(|(_, t, _)| now.saturating_duration_since(*t).as_secs_f32() < HOVER)
            || self.confirm.is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() < OPEN)
    }

    fn header_h(&self) -> f32 {
        if self.title.is_empty() { 8.0 } else { 54.0 }
    }

    fn body_h(&self) -> f32 {
        let gaps = self.items.iter().filter(|i| i.gap).count() as f32;
        self.header_h() + self.items.len() as f32 * ITEM_H + gaps * 9.0 + 8.0
    }

    /// Final rectangle, kept inside the window.
    pub fn rect(&self, screen: Rect) -> Rect {
        let size = vec2(WIDTH, self.body_h());
        let mut min = match self.kind {
            Kind::Carousel => self.anchor,
            // Centered above the "⋮" button.
            Kind::Dock => self.anchor - vec2(size.x * 0.5, size.y + 10.0),
        };
        if min.x + size.x > screen.right() - 10.0 {
            min.x = (self.anchor.x - size.x).max(10.0);
        }
        min.x = min.x.max(10.0);
        if min.y + size.y > screen.bottom() - 10.0 {
            min.y = (screen.bottom() - size.y - 10.0).max(10.0);
        }
        min.y = min.y.max(10.0);
        Rect::from_min_size(min, size)
    }

    fn item_rect(&self, rect: Rect, index: usize) -> Rect {
        let gaps = self.items[..=index].iter().filter(|i| i.gap).count() as f32;
        let y = rect.top() + self.header_h() + index as f32 * ITEM_H + gaps * 9.0;
        Rect::from_min_size(pos2(rect.left() + 6.0, y), vec2(rect.width() - 12.0, ITEM_H - 2.0))
    }

    pub fn contains(&self, screen: Rect, p: Pos2) -> bool {
        self.rect(screen).expand(4.0).contains(p) || self.confirm_rect(screen).is_some_and(|r| r.contains(p))
    }

    fn confirm_rect(&self, screen: Rect) -> Option<Rect> {
        self.confirm.map(|_| Rect::from_center_size(screen.center(), vec2(380.0, 176.0)))
    }

    /// Updates the hovered item; returns whether to redraw.
    pub fn pointer(&mut self, screen: Rect, p: Option<Pos2>, now: Instant) -> bool {
        if self.closing.is_some() || self.confirm.is_some() {
            return false;
        }
        let rect = self.rect(screen);
        let hovered = p.and_then(|p| (0..self.items.len()).find(|&i| self.item_rect(rect, i).contains(p)));
        if hovered == self.hovered {
            return self.hovered.is_some();
        }
        for (i, h) in self.hover.iter_mut().enumerate() {
            let target = f32::from(Some(i) == hovered);
            if target != h.2 {
                let from = sample(*h, now);
                *h = (from, now, target);
            }
        }
        self.hovered = hovered;
        true
    }

    /// A click; returns the chosen action once confirmed (Delete asks first).
    pub fn click(&mut self, screen: Rect, p: Pos2, now: Instant) -> Option<Action> {
        if let Some(r) = self.confirm_rect(screen) {
            let (cancel, accept) = confirm_buttons(r);
            if accept.contains(p) {
                self.close(now);
                return Some(Action::Delete);
            }
            if cancel.contains(p) || !r.contains(p) {
                self.confirm = None;
            }
            return None;
        }
        let rect = self.rect(screen);
        let index = (0..self.items.len()).find(|&i| self.item_rect(rect, i).contains(p))?;
        let action = self.items[index].action;
        if action == Action::Delete {
            self.confirm = Some(now);
            return None;
        }
        self.close(now);
        Some(action)
    }

    pub fn paint(&self, painter: &Painter, now: Instant) {
        let screen = painter.clip_rect();
        let rect = self.rect(screen);
        let open_t = (now.saturating_duration_since(self.opened).as_secs_f32() / OPEN).min(1.0);
        let close_t = self.closing.map_or(0.0, |t| (now.saturating_duration_since(t).as_secs_f32() / CLOSE).min(1.0));
        let (shown, scale, dy, alpha) = if self.reduced_motion {
            (1.0, 1.0, 0.0, if self.closing.is_some() { 0.0 } else { 1.0 })
        } else if self.closing.is_some() {
            let e = ease(close_t, [0.4, 0.0, 1.0, 1.0]);
            (1.0, 1.0 - 0.22 * e, 14.0 * e, 1.0 - e)
        } else {
            let e = ease(open_t, SPRING);
            let reveal = ease(open_t / 0.6, OUT);
            (reveal, 0.68 + 0.32 * e, -16.0 * (1.0 - e), ease(open_t / 0.5, OUT))
        };
        if alpha <= 0.0 {
            return;
        }
        let origin = match self.kind {
            Kind::Carousel => rect.left_top(),
            Kind::Dock => rect.center_bottom(),
        };
        let r = Rect::from_min_max(
            origin + (rect.min - origin) * scale,
            origin + (rect.max - origin) * scale,
        )
        .translate(vec2(0.0, dy));
        let fade = |c: C| c.gamma_multiply(alpha);
        // Shadow, glass body and violet border.
        painter.rect_filled(r.translate(vec2(0.0, 12.0)).expand(6.0), 22.0, fade(C::from_black_alpha(110)));
        painter.rect_filled(r, 18.0, fade(FILL));
        self.border(painter, r, 18.0, alpha, now);
        // Reveal from the anchor side (clip grows over the opening).
        let clip = match self.kind {
            Kind::Carousel => Rect::from_min_max(r.min, pos2(r.right(), r.top() + r.height() * shown.max(0.02))),
            Kind::Dock => Rect::from_min_max(pos2(r.left(), r.bottom() - r.height() * shown.max(0.02)), r.max),
        };
        let p = painter.with_clip_rect(clip.intersect(screen));
        let s = scale;
        let at = |q: Pos2| origin + (q - origin) * s + vec2(0.0, dy);
        if !self.title.is_empty() {
            p.text(at(rect.min + vec2(16.0, 18.0)), Align2::LEFT_CENTER, &self.title, FontId::new(13.5 * s, FontFamily::Name("ika-semibold".into())), fade(C::WHITE));
            p.text(at(rect.min + vec2(16.0, 36.0)), Align2::LEFT_CENTER, &self.subtitle, FontId::new(11.0 * s, FontFamily::Proportional), fade(C::from_rgb(0xc0, 0x84, 0xfc)));
            let y = rect.top() + 48.0;
            p.line_segment([at(pos2(rect.left() + 10.0, y)), at(pos2(rect.right() - 10.0, y))], Stroke::new(1.0, fade(C::from_white_alpha(26))));
        }
        let elapsed = now.saturating_duration_since(self.opened).as_secs_f32();
        for (i, item) in self.items.iter().enumerate() {
            let base = self.item_rect(rect, i);
            if item.gap {
                let y = base.top() - 5.0;
                p.line_segment([at(pos2(rect.left() + 12.0, y)), at(pos2(rect.right() - 12.0, y))], Stroke::new(1.0, fade(C::from_white_alpha(20))));
            }
            // Cascade: each row slides in a little after the previous one.
            let row_t = if self.reduced_motion { 1.0 } else { ease((elapsed - 0.02 * (i as f32 + 1.0)) / 0.22, OUT) };
            let row_alpha = alpha * row_t;
            if row_alpha <= 0.0 {
                continue;
            }
            let hot = sample(self.hover[i], now);
            let slide = vec2(-8.0 * (1.0 - row_t) + 5.0 * hot, 0.0);
            let row = Rect::from_min_max(at(base.min + slide), at(base.max + slide));
            let accent = match item.tone {
                Tone::Danger => C::from_rgb(239, 68, 68),
                Tone::Warn => C::from_rgb(251, 191, 36),
                Tone::Normal => C::from_rgb(168, 85, 247),
            };
            if hot > 0.0 {
                p.rect_filled(row, 11.0 * s, accent.gamma_multiply(0.26 * hot * row_alpha));
                // Left neon bar grows from the middle.
                let h = row.height() * 0.7 * (0.2 + 0.8 * hot);
                let bar = Rect::from_center_size(pos2(row.left() + 1.5, row.center().y), vec2(3.0, h));
                p.rect_filled(bar, 2.0, lerp(C::from_rgb(0xc0, 0x84, 0xfc), accent, 0.5).gamma_multiply(hot * row_alpha));
            }
            let text = match item.tone {
                Tone::Danger => lerp(C::from_rgb(0xf8, 0x71, 0x71), C::from_rgb(0xfe, 0xca, 0xca), hot),
                Tone::Warn => C::from_rgb(0xfb, 0xbf, 0x24),
                Tone::Normal => lerp(C::from_rgb(0xe2, 0xe8, 0xf0), C::WHITE, hot),
            };
            let icon_at = row.left_center() + vec2(22.0 * s, 0.0);
            let motion = if Some(i) == self.hovered { now.saturating_duration_since(self.hover[i].1).as_secs_f32() } else { 0.0 };
            draw_icon(&p, item.icon, icon_at, 7.0 * s, item.tint.gamma_multiply(row_alpha), motion * hot.min(1.0), hot.min(1.0));
            p.text(row.left_center() + vec2(40.0 * s, 0.0), Align2::LEFT_CENTER, item.label, FontId::new(13.0 * s, FontFamily::Name("ika-semibold".into())), text.gamma_multiply(row_alpha));
        }
        if let Some(r) = self.confirm_rect(screen) {
            self.paint_confirm(painter, r, now);
        }
    }

    /// The titlebar's animated gradient running around the rounded outline.
    fn border(&self, painter: &Painter, r: Rect, radius: f32, alpha: f32, now: Instant) {
        let shift = if self.reduced_motion {
            0.0
        } else {
            (now.saturating_duration_since(self.born).as_secs_f32() % RAMP_PERIOD) * (2400.0 / RAMP_PERIOD)
        };
        let r = r.shrink(0.75);
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5);
        // Walk the outline: 4 edges and 4 quarter arcs, in steps of ~6 px.
        let mut pts: Vec<Pos2> = Vec::new();
        let corner = |c: Pos2, a0: f32, pts: &mut Vec<Pos2>| {
            for k in 0..=6 {
                let a = a0 + std::f32::consts::FRAC_PI_2 * k as f32 / 6.0;
                pts.push(c + vec2(a.cos(), a.sin()) * radius);
            }
        };
        let pi = std::f32::consts::PI;
        corner(pos2(r.right() - radius, r.top() + radius), -pi / 2.0, &mut pts);
        corner(pos2(r.right() - radius, r.bottom() - radius), 0.0, &mut pts);
        corner(pos2(r.left() + radius, r.bottom() - radius), pi / 2.0, &mut pts);
        corner(pos2(r.left() + radius, r.top() + radius), pi, &mut pts);
        pts.push(pts[0]);
        let mut walked = 0.0;
        for w in pts.windows(2) {
            let len = (w[1] - w[0]).length();
            let steps = (len / 6.0).ceil().max(1.0) as usize;
            for k in 0..steps {
                let a = w[0] + (w[1] - w[0]) * (k as f32 / steps as f32);
                let b = w[0] + (w[1] - w[0]) * ((k + 1) as f32 / steps as f32);
                let at = walked + len * (k as f32 + 0.5) / steps as f32;
                let color = ramp_at(at * 2.0 - shift).gamma_multiply(alpha);
                // Soft glow under a crisp core, like the titlebar hairline.
                painter.line_segment([a, b], Stroke::new(4.0, color.gamma_multiply(0.30)));
                painter.line_segment([a, b], Stroke::new(1.6, color));
            }
            walked += len;
        }
    }

    fn paint_confirm(&self, painter: &Painter, r: Rect, now: Instant) {
        let t = self.confirm.map_or(1.0, |t| ease(now.saturating_duration_since(t).as_secs_f32() / OPEN, SPRING));
        let r = Rect::from_center_size(r.center(), r.size() * (0.85 + 0.15 * t));
        let a = t.clamp(0.0, 1.0);
        painter.rect_filled(painter.clip_rect(), 0.0, C::from_black_alpha((110.0 * a) as u8));
        painter.rect_filled(r, 18.0, FILL.gamma_multiply(a));
        self.border(painter, r, 18.0, a, now);
        let bold = FontId::new(14.0, FontFamily::Name("ika-semibold".into()));
        painter.text(r.min + vec2(22.0, 28.0), Align2::LEFT_CENTER, "ELIMINAR INSTANCIA", bold.clone(), C::from_rgb(0xf8, 0x71, 0x71).gamma_multiply(a));
        painter.text(r.min + vec2(22.0, 58.0), Align2::LEFT_CENTER, &self.title, FontId::new(13.0, FontFamily::Proportional), C::WHITE.gamma_multiply(a));
        painter.text(r.min + vec2(22.0, 82.0), Align2::LEFT_CENTER, "Se moverá a la papelera de Ika; puedes recuperarla.", FontId::new(11.5, FontFamily::Proportional), C::from_rgb(0x94, 0xa3, 0xb8).gamma_multiply(a));
        let (cancel, accept) = confirm_buttons(r);
        painter.rect(cancel, 10.0, C::from_white_alpha((14.0 * a) as u8), Stroke::new(1.0, C::from_white_alpha((40.0 * a) as u8)), egui::StrokeKind::Inside);
        painter.text(cancel.center(), Align2::CENTER_CENTER, "Cancelar", bold.clone(), C::WHITE.gamma_multiply(a));
        painter.rect_filled(accept, 10.0, C::from_rgb(220, 38, 38).gamma_multiply(a));
        painter.text(accept.center(), Align2::CENTER_CENTER, "Eliminar instancia", bold, C::WHITE.gamma_multiply(a));
    }
}

fn confirm_buttons(r: Rect) -> (Rect, Rect) {
    let y = r.bottom() - 30.0;
    (
        Rect::from_center_size(pos2(r.right() - 230.0, y), vec2(110.0, 36.0)),
        Rect::from_center_size(pos2(r.right() - 92.0, y), vec2(150.0, 36.0)),
    )
}

fn item(action: Action, label: &'static str, icon: Icon, tint: C, tone: Tone, gap: bool) -> Item {
    Item { action, label, icon, tint, tone, gap }
}

/// Hover level with an overshooting ease, from a (from, start, target) record.
fn sample((from, start, target): (f32, Instant, f32), now: Instant) -> f32 {
    let t = (now.saturating_duration_since(start).as_secs_f32() / HOVER).min(1.0);
    from + (target - from) * ease(t, crate::titlebar::ICON_EASE)
}

fn lerp(a: C, b: C, t: f32) -> C {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    C::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

fn rot(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// Each icon has its own looping motion while hovered (`t` seconds hovered,
/// `h` hover level), mirroring the WebView2 keyframes.
fn draw_icon(p: &Painter, icon: Icon, at: Pos2, r: f32, color: C, t: f32, h: f32) {
    let pulse = (t * TAU / 0.9).sin() * 0.5 + 0.5;
    let k = r / 7.0;
    let line = |a: Vec2, b: Vec2, at: Pos2| p.line_segment([at + a * k, at + b * k], Stroke::new(1.6 * k, color));
    match icon {
        Icon::Play => {
            // Bounces forward and grows.
            let c = at + vec2(3.0 * pulse * h, 0.0) * k;
            let s = 1.0 + 0.25 * pulse * h;
            p.add(egui::Shape::convex_polygon(
                vec![c + vec2(-5.0, -6.0) * k * s, c + vec2(6.0, 0.0) * k * s, c + vec2(-5.0, 6.0) * k * s],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            let s = 1.0 + 0.12 * pulse * h;
            p.circle_stroke(at, 7.0 * k * s, Stroke::new(1.6 * k, color));
            p.rect_filled(Rect::from_center_size(at, vec2(5.0, 5.0) * k * s), 1.0, color);
        }
        Icon::Pencil | Icon::Rename => {
            // Wiggles as if writing.
            let a = 0.25 * (t * TAU / 0.8).sin() * h;
            let tip = at + rot(vec2(-5.0, 5.0), a) * k;
            let end = at + rot(vec2(5.0, -5.0), a) * k;
            p.line_segment([tip, end], Stroke::new(2.4 * k, color));
            if icon == Icon::Rename {
                p.rect_stroke(Rect::from_center_size(at + vec2(-1.5, 1.5) * k, vec2(11.0, 11.0) * k), 2.0, Stroke::new(1.2 * k, color.gamma_multiply(0.7)), egui::StrokeKind::Middle);
            }
        }
        Icon::Folder => {
            // The lid lifts.
            let lift = 2.0 * pulse * h;
            p.rect_stroke(Rect::from_center_size(at + vec2(0.0, 1.0) * k, vec2(14.0, 10.0) * k), 2.0, Stroke::new(1.6 * k, color), egui::StrokeKind::Middle);
            line(vec2(-7.0, -4.0 - lift), vec2(-2.0, -4.0 - lift), at);
        }
        Icon::Copy => {
            // The back card slides out.
            let o = vec2(2.5, -2.5) * (1.0 + 0.6 * pulse * h) * k;
            p.rect_stroke(Rect::from_center_size(at + o, vec2(9.0, 9.0) * k), 2.0, Stroke::new(1.4 * k, color.gamma_multiply(0.75)), egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_center_size(at - o * 0.4, vec2(9.0, 9.0) * k), 2.0, Stroke::new(1.6 * k, color), egui::StrokeKind::Middle);
        }
        Icon::Trash => {
            // The lid tips open.
            let a = -0.35 * pulse * h;
            let hinge = at + vec2(-6.0, -4.0) * k;
            p.line_segment([hinge, hinge + rot(vec2(12.0, 0.0), a) * k], Stroke::new(1.6 * k, color));
            p.rect_stroke(Rect::from_min_max(at + vec2(-5.0, -2.5) * k, at + vec2(5.0, 7.0) * k), 1.5, Stroke::new(1.6 * k, color), egui::StrokeKind::Middle);
        }
        Icon::Layers => {
            // The layers separate.
            for (n, dy) in [-4.0f32, 0.0, 4.0].into_iter().enumerate() {
                let spread = dy * (1.0 + 0.35 * pulse * h) - 0.0 * n as f32;
                let c = at + vec2(0.0, spread) * k;
                p.add(egui::Shape::closed_line(vec![c + vec2(0.0, -3.0) * k, c + vec2(7.0, 0.0) * k, c + vec2(0.0, 3.0) * k, c + vec2(-7.0, 0.0) * k], Stroke::new(1.3 * k, color)));
            }
        }
        Icon::Scale => {
            // The beam tips back and forth.
            let a = 0.3 * (t * TAU / 0.8).sin() * h;
            line(vec2(0.0, -7.0), vec2(0.0, 7.0), at);
            let l = at + rot(vec2(-6.0, -3.0), a) * k;
            let rr = at + rot(vec2(6.0, -3.0), a) * k;
            p.line_segment([l, rr], Stroke::new(1.6 * k, color));
            p.circle_stroke(l + vec2(0.0, 3.0) * k, 2.2 * k, Stroke::new(1.3 * k, color));
            p.circle_stroke(rr + vec2(0.0, 3.0) * k, 2.2 * k, Stroke::new(1.3 * k, color));
        }
        Icon::Cube => {
            // Turns like a block.
            let a = 0.5 * (t * TAU / 1.0).sin() * h;
            let pts: Vec<Pos2> = (0..6).map(|n| at + rot(vec2(7.0, 0.0), a + n as f32 * TAU / 6.0 + TAU / 12.0) * k).collect();
            p.add(egui::Shape::closed_line(pts, Stroke::new(1.6 * k, color)));
        }
        Icon::Image => {
            // The sun inside rises.
            p.rect_stroke(Rect::from_center_size(at, vec2(14.0, 14.0) * k), 2.0, Stroke::new(1.6 * k, color), egui::StrokeKind::Middle);
            p.circle_filled(at + vec2(-3.0, -3.0 - 1.5 * pulse * h) * k, 1.6 * k, color);
            line(vec2(-6.0, 5.0), vec2(0.0, 0.0), at);
            line(vec2(0.0, 0.0), vec2(6.0, 4.0), at);
        }
        Icon::Sun => {
            // Rays rotate slowly.
            p.circle_stroke(at, 3.5 * k, Stroke::new(1.6 * k, color));
            let spin = t * TAU / 2.4 * h;
            for n in 0..8 {
                let d = rot(vec2(1.0, 0.0), spin + n as f32 * TAU / 8.0);
                p.line_segment([at + d * 5.5 * k, at + d * 7.5 * k], Stroke::new(1.4 * k, color));
            }
        }
        Icon::Camera => {
            // The shutter flashes.
            p.rect_stroke(Rect::from_center_size(at + vec2(0.0, 1.0) * k, vec2(15.0, 10.0) * k), 2.5, Stroke::new(1.6 * k, color), egui::StrokeKind::Middle);
            p.circle_stroke(at + vec2(0.0, 1.0) * k, 3.0 * k * (1.0 - 0.3 * pulse * h), Stroke::new(1.6 * k, color));
            p.circle_filled(at + vec2(0.0, 1.0) * k, 1.4 * k * pulse * h, color);
        }
        Icon::Terminal => {
            // The prompt blinks.
            line(vec2(-6.0, -4.0), vec2(-2.0, 0.0), at);
            line(vec2(-2.0, 0.0), vec2(-6.0, 4.0), at);
            let blink = if h > 0.0 && (t * 2.2).fract() > 0.5 { 0.25 } else { 1.0 };
            p.line_segment([at + vec2(0.0, 5.0) * k, at + vec2(7.0, 5.0) * k], Stroke::new(1.6 * k, color.gamma_multiply(blink)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> crate::minecraft::Instance {
        crate::minecraft::Instance {
            name: "Keo".into(),
            path: "C:/x/Keo".into(),
            mod_count: 0,
            mc_version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: None,
            icon_path: None,
            java_version: None,
            java_vendor: "",
            total_time_played: None,
            shader_count: 0,
            resourcepack_count: 0,
            max_mem_mb: 8192,
        }
    }

    #[test]
    fn menus_match_webview2_and_delete_asks_first() {
        let now = Instant::now();
        let labels = |m: &Menu| m.items.iter().map(|i| i.label).collect::<Vec<_>>();
        let carousel = Menu::carousel(&instance(), false, pos2(100.0, 100.0), now, true);
        assert_eq!(labels(&carousel), ["Jugar esta instancia", "Editar instancia", "Abrir en el explorador", "Duplicar instancia", "Eliminar instancia"]);
        assert_eq!(carousel.subtitle, "Minecraft 1.20.1 · Forge");
        let running = Menu::carousel(&instance(), true, pos2(100.0, 100.0), now, true);
        assert_eq!(running.items[0].action, Action::Stop);
        let dock = Menu::dock(&instance(), false, pos2(600.0, 700.0), now, true);
        assert_eq!(dock.items.len(), 10);
        assert_eq!(dock.items.last().unwrap().action, Action::Delete);
        assert!(Menu::dock(&instance(), true, pos2(600.0, 700.0), now, true).items.iter().any(|i| i.action == Action::Stop));

        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let mut menu = Menu::carousel(&instance(), false, pos2(100.0, 100.0), now, true);
        let rect = menu.rect(screen);
        let delete = menu.item_rect(rect, 4).center();
        assert_eq!(menu.click(screen, delete, now), None, "delete asks first");
        let (cancel, accept) = confirm_buttons(menu.confirm_rect(screen).unwrap());
        assert_eq!(menu.click(screen, cancel.center(), now), None);
        assert!(menu.confirm.is_none());
        menu.click(screen, delete, now);
        let (_, accept2) = confirm_buttons(menu.confirm_rect(screen).unwrap());
        assert_eq!(accept, accept2);
        assert_eq!(menu.click(screen, accept.center(), now), Some(Action::Delete));
        assert!(menu.closing());
    }

    #[test]
    fn menu_stays_inside_the_window() {
        let now = Instant::now();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0));
        let menu = Menu::carousel(&instance(), false, pos2(1270.0, 700.0), now, true);
        let r = menu.rect(screen);
        assert!(screen.contains_rect(r), "{r:?}");
    }
}

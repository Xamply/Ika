use super::{
    Area, ClockOcclusion, Drag, Hit, Side, Style, Widgets, format_button, handle, menu_choices, menu_rect,
    news_bounds, news_max_scroll, news_refresh, rect, shortcut_rect, shortcuts,
};
use crate::home::{CollectionArt, Home};
use egui::{
    Align2, Color32 as Color, FontFamily, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, pos2,
    vec2,
};
use std::time::Instant;
use winit::window::Window;

const PURPLE: Color = Color::from_rgb(168, 85, 247);
const TEAL: Color = Color::from_rgb(0, 245, 212);
const MUTED: Color = Color::from_rgb(170, 160, 199);
const FONT: &str = "Silkscreen";
const MINECRAFT_FONT: &str = "Minecraftia";

impl Widgets {
    pub fn paint_home_clock(
        &self,
        painter: &Painter,
        window: &Window,
        now: Instant,
        show_menu: bool,
        occlusion: ClockOcclusion,
    ) {
        let area = Area::from_window(window);
        if !area.stage.is_positive() {
            return;
        }
        let stage = painter.with_clip_rect(area.stage);
        if let Some(r) = self.clock(area, now) {
            let bounds = r.expand(24. * r.width() / self.settings.style.size().x);
            occlusion.clips(stage.clip_rect(), bounds, area.dpi, |clip| {
                let mut clock = stage.with_clip_rect(clip);
                clock.multiply_opacity(occlusion.opacity());
                self.paint_clock(&clock, r);
            });
        }
        if show_menu && let Some(menu) = self.menu {
            let r = menu_rect(area, menu);
            frame(&stage, r, 1.);
            for (cell, hit, label) in menu_choices(r) {
                let selected = hit == Hit::Style(self.settings.style);
                button(&stage, cell, label, selected, self.cursor, 1.);
            }
        }
    }

    pub fn paint(
        &self,
        painter: &Painter,
        window: &Window,
        home: &Home,
        art: Option<CollectionArt>,
        now: Instant,
    ) {
        let area = Area::from_window(window);
        if !area.stage.is_positive() {
            return;
        }
        let stage = painter.with_clip_rect(area.stage);
        let moving = matches!(self.drag, Some(Drag::Move { .. }));
        for side in Side::ALL {
            let amount = self.amount(side, now);
            if amount <= 0. {
                continue;
            }
            let r = area.panel(side, amount);
            let p = stage.with_clip_rect(r);
            let opacity = amount.sqrt();
            p.rect_filled(r, 0, Color::from_rgb(4, 4, 22).gamma_multiply(opacity));
            let x = if side == Side::Left {
                r.right() - 1.
            } else {
                r.left() + 1.
            };
            let drop_target = moving && self.cursor.is_some_and(|cursor| r.contains(cursor));
            let edge = if drop_target {
                Stroke::new(2., TEAL.gamma_multiply(0.65 * opacity))
            } else {
                Stroke::new(1.5, PURPLE.gamma_multiply(0.55 * opacity))
            };
            p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], edge);
            self.paint_panel(
                &p,
                self.content(area, side, now),
                side,
                home,
                art,
                opacity,
                now,
            );
        }
    }

    fn paint_panel(
        &self,
        p: &Painter,
        r: Rect,
        side: Side,
        home: &Home,
        art: Option<CollectionArt>,
        a: f32,
        now: Instant,
    ) {
        if !r.is_positive() {
            return;
        }
        if side == Side::Left {
            let activity = rect(r.left(), r.top(), r.width(), self.activity_height(r));
            self.paint_activity(p, activity, r, home, a);
            self.collection.paint(
                p,
                self.collection_bounds(r),
                home,
                art,
                self.cursor,
                a,
                now,
                self.reduced_motion,
            );
        } else {
            self.paint_shortcuts(p, r, a);
            self.paint_news(p, news_bounds(r), a);
        }
    }

    fn paint_activity(&self, p: &Painter, area: Rect, layout: Rect, home: &Home, a: f32) {
        let amber = Color::from_rgb(251, 191, 36);
        section(
            p,
            area,
            "Actividad Reciente",
            0,
            Color::from_rgb(244, 114, 182),
            a,
        );
        let body = Rect::from_min_max(area.min + vec2(9., 38.), area.max - vec2(9., 40.));
        if body.is_positive() {
            let p = p.with_clip_rect(body);
            let empty = if home.running_game() {
                "Minecraft en ejecución"
            } else if home.busy() {
                "Preparando Minecraft…"
            } else {
                "Todavía no hay actividad registrada"
            };
            let rows = self.recent.len().max(1);
            for i in 0..rows {
                let (date, title) = self
                    .recent
                    .get(i)
                    .map_or(("", empty), |(date, title)| (date.as_str(), title.as_str()));
                let (tag, tint) = if title.starts_with("Juego iniciado") {
                    ("JUEGO INICIADO", Color::from_rgb(110, 231, 183))
                } else if title.starts_with("Fondo aplicado") {
                    ("FONDO", Color::from_rgb(244, 114, 182))
                } else if self.recent.is_empty() {
                    ("ACTIVIDAD", amber)
                } else {
                    ("INSTANCIA", SKY)
                };
                let y = body.top() + i as f32 * 61.;
                let dot = pos2(body.left() + 11., y + 14.);
                if i + 1 < rows {
                    p.line_segment(
                        [dot + vec2(0., 8.), dot + vec2(0., 56.)],
                        Stroke::new(2., PURPLE.gamma_multiply(a * 0.25)),
                    );
                }
                p.circle_filled(dot, 8., tint.gamma_multiply(a * 0.14));
                p.circle_filled(dot, 5., tint.gamma_multiply(a));
                let card = rect(body.left() + 25., y, body.width() - 29., 52.);
                panel_item(&p, card, self.cursor, a);
                badge(&p, card.min + vec2(11., 7.), tag, tint, a);
                label(
                    &p,
                    card.right_top() + vec2(-10., 10.),
                    Align2::RIGHT_TOP,
                    date,
                    11.,
                    STEEL.gamma_multiply(a),
                );
                strong(
                    &p.with_clip_rect(card.shrink2(vec2(10., 0.))),
                    card.min + vec2(11., 27.),
                    Align2::LEFT_TOP,
                    title,
                    13.,
                    Color::WHITE.gamma_multiply(a),
                );
            }
        }
        label(
            p,
            pos2(area.left() + 16., area.bottom() - 22.),
            Align2::LEFT_CENTER,
            &format!("{} eventos registrados", self.recent.len()),
            12.,
            STEEL.gamma_multiply(a),
        );
        let clear = self.clear_activity(layout);
        let red = Color::from_rgb(252, 165, 165);
        pill_button(
            p,
            clear,
            "Limpiar",
            Some(7),
            red,
            self.cursor.filter(|_| !self.recent.is_empty()),
            if self.recent.is_empty() { a * 0.55 } else { a },
        );
    }

    fn paint_shortcuts(&self, p: &Painter, area: Rect, a: f32) {
        section(p, area, "Accesos rápidos", 1, SKY, a);
        for (i, title) in shortcuts().into_iter().enumerate() {
            let row = shortcut_rect(area, i);
            let hot = self.cursor.is_some_and(|c| row.contains(c));
            if hot {
                p.rect_filled(row, 10., SKY.gamma_multiply(0.08 * a));
            }
            let icon = rect(row.left() + 8., row.top() + 6., 38., 38.);
            p.rect(
                icon,
                9.,
                if hot {
                    SKY.gamma_multiply(0.22 * a)
                } else {
                    Color::from_rgb(10, 16, 39).gamma_multiply(a)
                },
                Stroke::new(1.5, SKY.gamma_multiply(a * if hot { 1. } else { 0.32 })),
                StrokeKind::Inside,
            );
            glyph(
                p,
                i + 3,
                icon.center(),
                20.,
                if hot { Color::WHITE } else { SKY }.gamma_multiply(a),
            );
            label(
                &p.with_clip_rect(row),
                row.left_center() + vec2(59., 0.),
                Align2::LEFT_CENTER,
                title,
                13.5,
                if hot { Color::WHITE } else { PALE }.gamma_multiply(a),
            );
        }
    }

    fn paint_news(&self, p: &Painter, area: Rect, a: f32) {
        if area.height() < 76. {
            return;
        }
        section(p, area, "Novedades", 2, SKY, a);
        let refresh = news_refresh(area);
        let body = Rect::from_min_max(
            area.min + vec2(16., 41.),
            pos2(area.right() - 16., refresh.top() - 12.),
        );
        if body.is_positive() {
            let p = p.with_clip_rect(body);
            let scroll = self.news_scroll.min(news_max_scroll(area));
            for (i, (tag, date, title, detail)) in NEWS.into_iter().enumerate() {
                let item = rect(
                    body.left(),
                    body.top() + i as f32 * 116. - scroll,
                    body.width(),
                    104.,
                );
                if !item.intersects(body) {
                    continue;
                }
                panel_item(&p, item, self.cursor.filter(|c| body.contains(*c)), a);
                badge(&p, item.min + vec2(14., 10.), tag, SKY, a);
                label(
                    &p,
                    item.right_top() + vec2(-14., 13.),
                    Align2::RIGHT_TOP,
                    date,
                    11.,
                    STEEL.gamma_multiply(a),
                );
                let w = (item.width() - 28.).max(1.);
                let pos = item.min + vec2(14., 33.);
                let color = Color::WHITE.gamma_multiply(a);
                let title = p.layout(title.into(), bold(13.5), color, w);
                let height = title.size().y;
                p.galley(pos, title, color);
                let color = PALE.gamma_multiply(a);
                p.galley(
                    pos + vec2(0., height + 3.),
                    p.layout(detail.into(), FontId::proportional(11.5), color, w),
                    color,
                );
            }
        }
        pill_button(
            p,
            refresh,
            "Actualizar Novedades",
            None,
            PURPLE,
            self.cursor,
            a,
        );
    }

    fn paint_clock(&self, p: &Painter, r: Rect) {
        let style = self.settings.style;
        let s = r.width() / style.size().x;
        let t = self.time;
        let hour = if self.settings.clock_24h || style == Style::Minecraft {
            t.wHour
        } else {
            (t.wHour + 11) % 12 + 1
        };
        let am = if self.settings.clock_24h {
            ""
        } else if t.wHour < 12 {
            "AM"
        } else {
            "PM"
        };
        let hour_text = format!("{hour:02}");
        let minute = format!("{:02}", t.wMinute);
        let time = format!("{hour_text}:{minute}");
        let weekday = [
            "Domingo",
            "Lunes",
            "Martes",
            "Miércoles",
            "Jueves",
            "Viernes",
            "Sábado",
        ][usize::from(t.wDayOfWeek.min(6))];
        let month_name = [
            "Enero",
            "Febrero",
            "Marzo",
            "Abril",
            "Mayo",
            "Junio",
            "Julio",
            "Agosto",
            "Septiembre",
            "Octubre",
            "Noviembre",
            "Diciembre",
        ][usize::from(t.wMonth.clamp(1, 12) - 1)];
        let month = month_name.to_ascii_lowercase();
        let date = format!("{weekday}, {} de {month}", t.wDay);
        let day_short =
            ["DOM", "LUN", "MAR", "MIÉ", "JUE", "VIE", "SÁB"][usize::from(t.wDayOfWeek.min(6))];
        let short = format!("{day_short}, {} {} {}", t.wDay, &month[..3], t.wYear).to_uppercase();
        let center = |y| pos2(r.center().x, r.top() + y * s);
        let front = |text: &str, y, size, color| {
            clock_text(
                p,
                center(y),
                Align2::CENTER_CENTER,
                text,
                size * s,
                color,
                false,
            )
        };
        match style {
            Style::Pixel => {
                let radius = (10. * s).min(25.) as u8;
                p.rect_filled(
                    r.translate(vec2(0., 3. * s)),
                    radius,
                    Color::from_black_alpha(75),
                );
                p.rect(
                    r,
                    radius,
                    Color::from_rgba_unmultiplied(10, 6, 24, 190),
                    Stroke::new(2. * s, PURPLE.gamma_multiply(0.55)),
                    StrokeKind::Inside,
                );
                let point = |x, y| r.min + vec2(x * s, y * s);
                for a in [r.min, r.max - vec2(6., 6.) * s] {
                    p.rect_filled(Rect::from_min_size(a, vec2(6., 6.) * s), 0, PURPLE);
                }
                p.rect_filled(
                    Rect::from_min_size(point(20., 17.), vec2(6., 6.) * s),
                    0,
                    TEAL,
                );
                clock_text(
                    p,
                    point(32., 20.),
                    Align2::LEFT_CENTER,
                    "SYS_TIME",
                    11. * s,
                    MUTED,
                    true,
                );
                let f = format_button(r);
                p.rect_filled(f, 3, Color::from_white_alpha(15));
                clock_text(
                    p,
                    f.center(),
                    Align2::CENTER_CENTER,
                    if self.settings.clock_24h {
                        "24H"
                    } else {
                        "12H"
                    },
                    9. * s,
                    MUTED,
                    true,
                );
                clock_text(
                    p,
                    point(89., 56.),
                    Align2::RIGHT_CENTER,
                    &hour_text,
                    54. * s,
                    Color::WHITE,
                    true,
                );
                clock_text(
                    p,
                    point(106., 56.),
                    Align2::CENTER_CENTER,
                    ":",
                    54. * s,
                    TEAL.gamma_multiply(if t.wSecond.is_multiple_of(2) {
                        1.
                    } else {
                        0.72
                    }),
                    true,
                );
                clock_text(
                    p,
                    point(124., 56.),
                    Align2::LEFT_CENTER,
                    &minute,
                    54. * s,
                    Color::WHITE,
                    true,
                );
                clock_text(
                    p,
                    point(242., 50.),
                    Align2::CENTER_CENTER,
                    &format!("{:02}", t.wSecond),
                    16. * s,
                    TEAL,
                    true,
                );
                clock_text(
                    p,
                    point(242., 69.),
                    Align2::CENTER_CENTER,
                    am,
                    11. * s,
                    Color::from_rgb(255, 112, 166),
                    true,
                );
                clock_text(
                    p,
                    center(95.),
                    Align2::CENTER_CENTER,
                    &short,
                    13. * s,
                    MUTED,
                    true,
                );
            }
            Style::Minecraft => {
                minecraft_line(p, center(84.), &time, 78. * s, 2. * s, Color::WHITE);
                minecraft_line(
                    p,
                    center(109.),
                    &format!("{month_name} {}, {}", t.wDay, t.wYear),
                    17. * s,
                    s,
                    Color::from_white_alpha(242),
                );
            }
            Style::Ios => {
                front(&date, 24., 17., Color::WHITE);
                front(&format!("{hour}:{minute}"), 77., 88., Color::WHITE);
            }
            Style::PixelG => {
                front(&hour_text, 45., 81., Color::from_rgb(234, 211, 255));
                front(&minute, 110., 81., Color::from_rgb(242, 157, 207));
                front(&short, 165., 13., Color::WHITE);
            }
            Style::OneUi => {
                front(&time, 50., 77., Color::WHITE);
                front(&format!("{date} {am}"), 100., 16., Color::WHITE);
            }
            Style::Mond => {
                front(&weekday.to_uppercase(), 49., 39., Color::WHITE);
                front(
                    &format!("{} {} {}", t.wDay, month.to_uppercase(), t.wYear),
                    87.,
                    16.,
                    Color::WHITE,
                );
                front(&format!("— {time} {am} —"), 114., 17., Color::WHITE);
            }
            Style::Words => {
                let h = (t.wHour + 11) % 12 + 1;
                let m = t.wMinute;
                front(
                    if h == 1 { "es la" } else { "son las" },
                    31.,
                    20.,
                    Color::WHITE,
                );
                front(
                    if h == 1 { "una" } else { number(h) },
                    72.,
                    56.,
                    Color::from_rgb(242, 157, 207),
                );
                let tail = match m {
                    0 => "en punto".into(),
                    15 => "y cuarto".into(),
                    30 => "y media".into(),
                    1..=29 => format!("y {}", number(m)),
                    _ if m % 10 == 0 => format!("y {}", number(m)),
                    _ => format!("y {} y {}", number(m - m % 10), number(m % 10)),
                };
                front(&tail, 119., 17., Color::WHITE);
            }
            Style::Ring => {
                let radius = 78. * s;
                p.circle_stroke(
                    r.center(),
                    radius,
                    Stroke::new(5. * s, Color::from_white_alpha(65)),
                );
                let points = (0..=t.wSecond)
                    .map(|i| {
                        let angle = f32::from(i) * std::f32::consts::TAU / 60.
                            - std::f32::consts::FRAC_PI_2;
                        r.center() + vec2(angle.cos(), angle.sin()) * radius
                    })
                    .collect::<Vec<_>>();
                if points.len() > 1 {
                    p.add(egui::Shape::line(points, Stroke::new(5. * s, TEAL)));
                }
                front(&time, 83., 48., Color::WHITE);
                front(&short, 115., 12., Color::WHITE);
            }
        }
        let drag = self.drag.is_some();
        if self.settings.docked_side.is_none()
            && (drag || self.cursor.is_some_and(|p| r.expand(5.).contains(p)))
        {
            let h = handle(r);
            let color = if drag || self.cursor.is_some_and(|p| h.contains(p)) {
                TEAL
            } else {
                MUTED
            };
            p.rect(
                h,
                5,
                Color::from_rgba_unmultiplied(18, 12, 34, 237),
                Stroke::new(1., color),
                StrokeKind::Inside,
            );
            let pos = h.center();
            for i in [2., 6.] {
                p.line_segment(
                    [pos + vec2(i, -6. + i), pos + vec2(-6. + i, i)],
                    Stroke::new(1.5, color),
                );
            }
        }
    }
}

pub fn add_font(fonts: &mut egui::FontDefinitions) {
    for (name, bytes) in [
        (
            FONT,
            include_bytes!("../home/silkscreen-bold.ttf").as_slice(),
        ),
        (
            MINECRAFT_FONT,
            include_bytes!("../home/minecraftia.ttf").as_slice(),
        ),
    ] {
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.into(), "ika-ui".into()]);
    }
}

fn minecraft_line(p: &Painter, baseline: Pos2, text: &str, size: f32, spacing: f32, color: Color) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.,
        egui::TextFormat {
            font_id: FontId::new(size, FontFamily::Name(MINECRAFT_FONT.into())),
            extra_letter_spacing: spacing,
            color,
            ..Default::default()
        },
    );
    let galley = p.layout_job(job);
    // Align the glyph baseline to the WebView clock's two explicit line boxes.
    if let Some(glyph) = galley.rows.first().and_then(|row| row.glyphs.first()) {
        let x = baseline.x - (galley.size().x + spacing) * 0.5;
        p.galley(pos2(x, baseline.y - glyph.pos.y), galley, color);
    }
}

fn clock_text(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    color: Color,
    pixel: bool,
) {
    let font = if pixel {
        FontId::new(size, FontFamily::Name(FONT.into()))
    } else {
        FontId::proportional(size)
    };
    p.text(
        pos + vec2(1.5, 2.),
        align,
        text,
        font.clone(),
        Color::from_black_alpha(185),
    );
    p.text(pos, align, text, font, color);
}

fn number(n: u16) -> &'static str {
    const NUMBERS: [&str; 30] = [
        "cero",
        "uno",
        "dos",
        "tres",
        "cuatro",
        "cinco",
        "seis",
        "siete",
        "ocho",
        "nueve",
        "diez",
        "once",
        "doce",
        "trece",
        "catorce",
        "quince",
        "dieciséis",
        "diecisiete",
        "dieciocho",
        "diecinueve",
        "veinte",
        "veintiuno",
        "veintidós",
        "veintitrés",
        "veinticuatro",
        "veinticinco",
        "veintiséis",
        "veintisiete",
        "veintiocho",
        "veintinueve",
    ];
    match n {
        30 => "treinta",
        40 => "cuarenta",
        50 => "cincuenta",
        _ => NUMBERS.get(usize::from(n)).copied().unwrap_or(""),
    }
}

fn label(p: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color) {
    p.text(pos, align, text, FontId::proportional(size), color);
}

fn frame(p: &Painter, r: Rect, a: f32) {
    p.rect(
        r,
        10,
        Color::from_rgba_unmultiplied(18, 12, 34, 242).gamma_multiply(a),
        Stroke::new(1., PURPLE.gamma_multiply(0.47 * a)),
        StrokeKind::Inside,
    );
}

const SKY: Color = Color::from_rgb(56, 189, 248);
const STEEL: Color = Color::from_rgb(148, 163, 184);
const PALE: Color = Color::from_rgb(203, 213, 225);
const NEWS: [(&str, &str, &str, &str); 4] = [
    (
        "NUEVA VERSIÓN",
        "26 Sep 2026",
        "Minecraft 1.21 Tricky Trials & Bundles",
        "Explora las Cámaras del Juicio, el nuevo martillo Mace (Mazo), los Breeze y nuevos bloques de cobre para tus construcciones.",
    ),
    (
        "LAUNCHER",
        "25 Sep 2026",
        "Mod Comparer v3.4 - Alto Rendimiento",
        "Detección instantánea de incompatibilidades, análisis de dependencias cruzadas y verificación de hashes en segundo plano.",
    ),
    (
        "PERSONALIZACIÓN",
        "24 Sep 2026",
        "Fondos MotionBGs en 4K & Controles de Vista",
        "Ajusta zoom y encuadre con Ctrl + Rueda y Ctrl + Flechas de forma independiente en cada fondo de pantalla.",
    ),
    (
        "OPTIMIZACIÓN",
        "22 Sep 2026",
        "Soporte Java 21 & Shenandoah GC",
        "Tiempos de recolección de basura por debajo de 1ms, eliminando micro-parones en modpacks de más de 300 mods.",
    ),
];

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("ika-semibold".into()))
}
fn strong(p: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color) {
    p.text(pos, align, text, bold(size), color);
}

fn section(p: &Painter, r: Rect, title: &str, icon: usize, tint: Color, a: f32) {
    if r.width() < 35. {
        return;
    }
    let at = r.min + vec2(24., 20.);
    glyph(p, icon, at, 18., tint.gamma_multiply(a));
    strong(
        p,
        at + vec2(17., 0.),
        Align2::LEFT_CENTER,
        title,
        15.5,
        Color::from_rgb(226, 232, 240).gamma_multiply(a),
    );
}

fn panel_item(p: &Painter, r: Rect, cursor: Option<Pos2>, a: f32) {
    let hot = cursor.is_some_and(|pos| r.contains(pos));
    p.rect(
        r,
        13.,
        if hot {
            Color::from_rgb(26, 17, 54)
        } else {
            Color::from_rgb(15, 10, 33)
        }
        .gamma_multiply(a),
        Stroke::new(
            1.,
            PURPLE.gamma_multiply(if hot { 0.5 * a } else { 0.22 * a }),
        ),
        StrokeKind::Inside,
    );
}

fn badge(p: &Painter, at: Pos2, title: &str, tint: Color, a: f32) {
    let ink = tint.gamma_multiply(a);
    let galley = p.layout_no_wrap(title.into(), bold(10.), ink);
    let r = Rect::from_min_size(at, vec2(galley.size().x + 15., 17.));
    p.rect_filled(r, 6., tint.gamma_multiply(a * 0.18));
    p.galley(
        pos2(r.left() + 7., r.center().y - galley.size().y / 2.),
        galley,
        ink,
    );
}

fn pill_button(
    p: &Painter,
    r: Rect,
    title: &str,
    icon: Option<usize>,
    tint: Color,
    cursor: Option<Pos2>,
    a: f32,
) {
    let hot = cursor.is_some_and(|pos| r.contains(pos));
    p.rect(
        r,
        8.,
        tint.gamma_multiply(a * if hot { 0.26 } else { 0.13 }),
        Stroke::new(1., tint.gamma_multiply(a * if hot { 0.7 } else { 0.35 })),
        StrokeKind::Inside,
    );
    let color = if hot { Color::WHITE } else { tint }.gamma_multiply(a);
    if let Some(i) = icon {
        glyph(p, i, r.left_center() + vec2(16., 0.), 13., color);
    }
    strong(
        p,
        r.center() + vec2(if icon.is_some() { 8. } else { 0. }, 0.),
        Align2::CENTER_CENTER,
        title,
        12.,
        color,
    );
}

fn glyph(p: &Painter, icon: usize, at: Pos2, size: f32, c: Color) {
    let at = |x, y| at + vec2((x - 12.) * size / 24., (y - 12.) * size / 24.);
    let pen = Stroke::new((size / 12.).max(1.), c);
    let path = |xy: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            xy.iter().map(|&(x, y)| at(x, y)).collect(),
            pen,
        ))
    };
    let box_at = |x, y, r, b| {
        p.rect_stroke(
            Rect::from_min_max(at(x, y), at(r, b)),
            2.,
            pen,
            StrokeKind::Inside,
        )
    };
    match icon {
        0 => {
            p.circle_stroke(at(12., 12.), size * 0.41, pen);
            path(&[(12., 6.), (12., 12.), (16., 14.)]);
        }
        1 => {
            path(&[
                (13., 2.),
                (3., 14.),
                (12., 14.),
                (11., 22.),
                (21., 10.),
                (12., 10.),
                (13., 2.),
            ]);
        }
        2 => {
            box_at(3., 3., 21., 21.);
            path(&[(7., 8.), (17., 8.)]);
            path(&[(7., 12.), (17., 12.)]);
            path(&[(7., 16.), (14., 16.)]);
        }
        3 => {
            path(&[
                (3., 19.),
                (3., 5.),
                (10., 5.),
                (12., 8.),
                (21., 8.),
                (21., 19.),
                (3., 19.),
            ]);
        }
        4 => {
            for y in [3., 14.] {
                for x in [3., 14.] {
                    box_at(x, y, x + 7., y + 7.);
                }
            }
        }
        5 => {
            let points: Vec<_> = (0..=32)
                .map(|n| {
                    let t = n as f32 * std::f32::consts::TAU / 32.;
                    let r = if n % 4 < 2 { 10. } else { 8. };
                    at(12. + r * t.cos(), 12. + r * t.sin())
                })
                .collect();
            p.add(egui::Shape::line(points, pen));
            p.circle_stroke(at(12., 12.), size * 0.15, pen);
        }
        6 => {
            path(&[(7., 16.), (3., 16.), (3., 3.), (16., 3.), (16., 7.)]);
            box_at(8., 8., 22., 22.);
        }
        _ => {
            path(&[(3., 6.), (21., 6.)]);
            path(&[(6., 6.), (7., 21.), (17., 21.), (18., 6.)]);
            path(&[(9., 6.), (9., 3.), (15., 3.), (15., 6.)]);
        }
    }
}

fn button(p: &Painter, r: Rect, text: &str, selected: bool, cursor: Option<Pos2>, a: f32) {
    if !r.is_positive() {
        return;
    }
    let hover = cursor.is_some_and(|pos| r.contains(pos));
    let color = if selected {
        TEAL
    } else if hover {
        PURPLE
    } else {
        MUTED
    };
    p.rect(
        r,
        6,
        color.gamma_multiply(0.11 * a),
        Stroke::new(1., color.gamma_multiply(0.35 * a)),
        StrokeKind::Inside,
    );
    label(
        &p.with_clip_rect(r.shrink2(vec2(6., 0.))),
        pos2(r.left() + 8., r.center().y),
        Align2::LEFT_CENTER,
        text,
        12.,
        if selected { TEAL } else { Color::WHITE }.gamma_multiply(a),
    );
}

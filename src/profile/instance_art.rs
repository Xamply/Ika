//! The original cover mosaic and Minecraft block picker, drawn over the instance form.
use super::*;
use std::io::{Cursor, Read};

const COVER_NAMES: &str = include_str!("../home/instance-art/covers.txt");
const BLOCK_NAMES: &str = include_str!("../home/instance-art/blocks.txt");
const COVER_ZIP: &[u8] = include_bytes!("../home/instance-art/covers.zip");
const BLOCK_ATLAS: &[u8] = include_bytes!("../home/instance-art/blocks.png");
const ATLAS_COLS: usize = 24;
const BLOCK_PX: u32 = 32;
const COVER_ROWS: [usize; 10] = [8, 13, 10, 15, 7, 11, 9, 14, 8, 12];
const PINK: C = C::from_rgb(255, 78, 210);
const SCROLL: C = C::from_rgb(88, 32, 255);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Cover,
    Icon,
}

impl Kind {
    fn names(self) -> &'static str {
        match self {
            Self::Cover => COVER_NAMES,
            Self::Icon => BLOCK_NAMES,
        }
    }
    pub(super) fn name(self, index: usize) -> Option<&'static str> {
        self.names().lines().nth(index)
    }
    pub(super) fn index(self, name: &str) -> Option<usize> {
        self.names().lines().position(|entry| entry == name)
    }
    fn len(self) -> usize {
        self.names().lines().count()
    }
    pub(super) fn file_bytes(self, index: usize) -> Option<(&'static str, Vec<u8>)> {
        match self {
            Self::Cover => Some(("jpg", cover_bytes(index)?)),
            Self::Icon => {
                let atlas = image::load_from_memory(BLOCK_ATLAS).ok()?;
                let (x, y) = block_xy(index);
                let icon = atlas.crop_imm(x, y, BLOCK_PX, BLOCK_PX);
                let mut png = Cursor::new(Vec::new());
                icon.write_to(&mut png, image::ImageFormat::Png).ok()?;
                Some(("png", png.into_inner()))
            }
        }
    }
}

fn cover_bytes(index: usize) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(COVER_ZIP)).ok()?;
    let mut file = zip.by_index(index).ok()?;
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn block_xy(i: usize) -> (u32, u32) {
    (
        (i % ATLAS_COLS) as u32 * BLOCK_PX,
        (i / ATLAS_COLS) as u32 * BLOCK_PX,
    )
}

#[derive(Clone, Copy)]
pub(super) enum Result {
    Close,
    Import(Kind),
    Select(Kind, usize),
}

pub(super) struct Picker {
    kind: Kind,
    selected: Option<usize>,
    reveal: bool,
    block_atlas: Option<TextureHandle>,
    covers: HashMap<(usize, bool), TextureHandle>,
    before: Option<(usize, Instant)>,
}

impl Picker {
    pub(super) fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Foreground, egui::Id::new("instance-art-picker"))
    }

    pub(super) fn area(&self, view: Rect) -> Rect {
        let cover = self.kind == Kind::Cover;
        let width = if cover { (view.width() * 0.97).min(1320.) } else { (view.width() * 0.92).min(520.) };
        let height = if cover { (view.height() * 0.95).min(920.) } else { (view.height() * 0.8).min(466.) };
        Rect::from_center_size(view.center(), vec2(width, height))
    }

    pub(super) fn new(kind: Kind, current: Option<&str>) -> Self {
        let selected = current
            .and_then(|name| kind.names().lines().position(|n| n == name))
            .or_else(|| (kind == Kind::Cover).then_some(0));
        Self {
            kind,
            selected,
            reveal: true,
            block_atlas: None,
            covers: HashMap::new(),
            before: None,
        }
    }

    pub(super) fn paint(
        &mut self,
        ui: &mut Ui,
        view: Rect,
        r: Rect,
        phase: f32,
        reduce_motion: bool,
    ) -> Option<Result> {
        let cover = self.kind == Kind::Cover;
        let width = r.width();
        ui.set_clip_rect(view);
        let backdrop = interact(ui, view, "picker-backdrop");
        let outside = backdrop.clicked()
            && backdrop
                .interact_pointer_pos()
                .is_some_and(|p| !r.contains(p));
        let p = ui.painter().clone();
        windows_ui::shadow(&p, r, 20., 24);
        windows_ui::dialog_frame(&p, r, 20., BG, 1.8, phase);
        let pad = if cover { 24. } else { 20. };
        neon_pill(
            &p,
            rect(r.min, pad, 18., if cover { 167. } else { 145. }, 34.),
            CYAN,
            if cover {
                "ELEGIR PORTADA"
            } else {
                "ELEGIR ICONO"
            },
            None,
            16.,
        );
        if outside || close_button(ui, rect(r.right_top(), -pad - 34., 18., 34., 34.)) {
            return Some(Result::Close);
        }

        let import = rect(r.min, pad, 72., width - 2. * pad - 119., 34.);
        let random = rect(import.right_top(), 10., 0., 109., 34.);
        if action(
            ui,
            import,
            "import",
            if cover {
                "Importar fondo propio"
            } else {
                "Importar imagen propia"
            },
            true,
        ) {
            return Some(Result::Import(self.kind));
        }
        let choose_random = action(ui, random, "random", "Aleatorio", false);
        let random_index = || {
            let n = self.kind.len();
            let mut i = rand::random::<u32>() as usize % n;
            if Some(i) == self.selected {
                i = (i + 1) % n;
            }
            i
        };
        if choose_random {
            let index = random_index();
            if cover {
                self.select_cover(index, reduce_motion);
            } else {
                return Some(Result::Select(self.kind, index));
            }
        }
        let title = if cover {
            format!("FONDOS DISPONIBLES ({})", self.kind.len())
        } else {
            "BLOQUES DISPONIBLES".into()
        };
        label(
            &p,
            r.min + vec2(pad, 126.),
            Align2::LEFT_CENTER,
            &title,
            11.,
            true,
            MUTED,
        );
        if cover {
            label(
                &p,
                r.right_top() + vec2(-pad, 126.),
                Align2::RIGHT_CENTER,
                "Haz clic en una tarjeta para verla en su forma original",
                11.5,
                false,
                DIM,
            );
        }
        let mut scroll = Rect::from_min_max(
            r.min + vec2(pad, 142.),
            r.max - vec2(pad, if cover { 66. } else { 20. }),
        );
        if !cover {
            scroll.max.y = scroll.max.y.min(scroll.top() + 310.);
        }
        let choice = if cover {
            self.mosaic(ui, scroll, phase, reduce_motion)
        } else {
            self.icons(ui, scroll, phase)
        };
        if let Some(index) = choice {
            return Some(Result::Select(self.kind, index));
        }
        if cover {
            let confirm = rect(r.right_bottom(), -pad - 192., -54., 192., 34.);
            if form_submit(ui, confirm, "Seleccionar esta portada", true, true).clicked() {
                return self.selected.map(|i| Result::Select(self.kind, i));
            }
        }
        None
    }

    fn icons(&mut self, ui: &mut Ui, area: Rect, phase: f32) -> Option<usize> {
        let texture = self.block_atlas.get_or_insert_with(|| {
            let atlas = image::load_from_memory(BLOCK_ATLAS).unwrap().to_rgba8();
            load_image(
                ui.ctx(),
                "instance-block-picker",
                &atlas,
                egui::TextureOptions::NEAREST,
            )
        });
        let selected = self.selected;
        let reveal = std::mem::take(&mut self.reveal);
        let gap = 8.;
        let inset = 2.;
        let width = area.width() - inset - 9.;
        let cols = ((width + gap) / (46. + gap)).floor().max(1.) as usize;
        let cell = (width + gap) / cols as f32 - gap;
        let count = self.kind.len();
        let mut clicked = None;
        scroller(ui, area, "blocks", |ui, viewport| {
            let origin = ui.min_rect().min + vec2(inset, 4.);
            ui.set_min_size(vec2(
                width,
                4. + (cell + gap) * count.div_ceil(cols) as f32 + 6.,
            ));
            let visible = viewport.translate(ui.min_rect().min.to_vec2());
            for (i, name) in BLOCK_NAMES.lines().enumerate() {
                let tile = rect(
                    origin,
                    (i % cols) as f32 * (cell + gap),
                    (i / cols) as f32 * (cell + gap),
                    cell,
                    cell,
                );
                if reveal && selected == Some(i) && !visible.contains_rect(tile) {
                    ui.scroll_to_rect(tile, Some(egui::Align::Center));
                }
                if !visible.intersects(tile) {
                    continue;
                }
                let a = interact(ui, tile, ("block", i)).on_hover_text(name.replace('_', " "));
                let p = ui.painter();
                let active = selected == Some(i);
                surface(
                    p,
                    tile,
                    8.,
                    if a.hovered() {
                        C::from_rgb(35, 24, 69)
                    } else if active {
                        C::from_rgb(36, 14, 45)
                    } else {
                        C::from_white_alpha(8)
                    },
                    if a.hovered() { VIOLET } else { NONE },
                );
                let (x, y) = block_xy(i);
                let size = texture.size_vec2();
                let uv = Rect::from_min_max(
                    pos2(x as f32 / size.x, y as f32 / size.y),
                    pos2(
                        (x + BLOCK_PX) as f32 / size.x,
                        (y + BLOCK_PX) as f32 / size.y,
                    ),
                );
                let icon = 36_f32.min(tile.width() - 8.);
                p.image(
                    texture.id(),
                    Rect::from_center_size(tile.center(), vec2(icon, icon)),
                    uv,
                    WHITE,
                );
                if active {
                    selection_ring(p, tile, 8., phase);
                }
                if a.clicked() {
                    clicked = Some(i);
                }
            }
        });
        clicked
    }

    fn select_cover(&mut self, index: usize, reduce_motion: bool) {
        if self.selected == Some(index) {
            return;
        }
        self.before = self
            .selected
            .filter(|_| !reduce_motion)
            .map(|i| (i, Instant::now()));
        self.selected = Some(index);
        self.reveal = true;
    }

    fn mosaic(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        phase: f32,
        reduce_motion: bool,
    ) -> Option<usize> {
        let active = self.selected.unwrap_or(0);
        let layout = cover_layout(self.kind.len(), active);
        let max_rows = layout.iter().map(|c| c.start + c.rows).max().unwrap_or(1);
        let mut start = None;
        let previous = self.before.and_then(|(i, when)| {
            let t = when.elapsed().as_secs_f32() / 0.44;
            (t < 1.).then(|| {
                start = Some(1. - (1. - t).powi(4));
                cover_layout(self.kind.len(), i)
            })
        });
        if previous.is_some() {
            ui.ctx().request_repaint();
        } else {
            self.before = None;
        }
        let reveal = std::mem::take(&mut self.reveal);
        let width = area.width() - 18.;
        let gap = 14.;
        let col = (width - 3. * gap) / 4.;
        let stride = gap + 14.;
        let mut next = None;
        let mut commit = None;
        scroller(ui, area, "covers", |ui, viewport| {
            let origin = ui.min_rect().min + vec2(6., 14.);
            ui.set_min_size(vec2(width, 14. + stride * max_rows as f32 + 28.));
            let visible = viewport.translate(ui.min_rect().min.to_vec2());
            let placement = |c: CoverSlot| {
                rect(
                    origin,
                    c.column as f32 * (col + gap),
                    c.start as f32 * stride,
                    c.cols as f32 * (col + gap) - gap,
                    c.rows as f32 * stride - gap,
                )
            };
            let mut in_range = Vec::new();
            for i in (0..layout.len())
                .filter(|i| *i != active)
                .chain(std::iter::once(active))
            {
                let target = placement(layout[i]);
                let tile = if let (Some(old), Some(t)) = (&previous, start) {
                    let old = placement(old[i]);
                    Rect::from_min_max(old.min.lerp(target.min, t), old.max.lerp(target.max, t))
                } else {
                    target
                };
                let selected = i == active;
                if selected && reveal && !visible.shrink(12.).contains_rect(target) {
                    ui.scroll_to_rect(target.expand(12.), None);
                }
                if visible.expand(72.).intersects(tile) {
                    in_range.push((i, selected));
                }
                if !visible.intersects(tile) {
                    continue;
                }
                let a = interact(ui, tile, ("cover", i)).on_hover_text(format!("Fondo {}", i + 1));
                let p = ui.painter();
                surface(p, tile, 12., CARD, C::from_white_alpha(20));
                if !self.covers.contains_key(&(i, selected))
                    && let Some(image) =
                        cover_bytes(i).and_then(|bytes| image::load_from_memory(&bytes).ok())
                {
                    let max = if selected { 720 } else { 340 };
                    let rgba = image.thumbnail(max, max).to_rgba8();
                    self.covers.insert(
                        (i, selected),
                        load_image(
                            ui.ctx(),
                            "instance-cover-option",
                            &rgba,
                            egui::TextureOptions::LINEAR,
                        ),
                    );
                }
                if let Some(art) = self.covers.get(&(i, selected)) {
                    let image = tile.shrink(1.);
                    let src = art.size_vec2();
                    let fit = (image.width() / src.x).max(image.height() / src.y);
                    let uv = Rect::from_center_size(pos2(0.5, 0.5), image.size() / (src * fit));
                    let tint = if selected || a.hovered() {
                        WHITE
                    } else {
                        C::from_gray(185)
                    };
                    windows_ui::photo(
                        p,
                        art.id(),
                        image,
                        uv,
                        &windows_ui::rounded(image, 11., 11., 11., 11.),
                        tint,
                    );
                }
                if selected {
                    selection_ring(p, tile, 12., phase);
                    let badge = rect(tile.left_bottom(), 10., -31., 71., 23.);
                    surface(
                        p,
                        badge,
                        6.,
                        C::from_rgba_unmultiplied(6, 9, 22, 210),
                        C::from_white_alpha(46),
                    );
                    label(
                        p,
                        badge.center(),
                        Align2::CENTER_CENTER,
                        &format!("Fondo {}", i + 1),
                        10.5,
                        true,
                        WHITE,
                    );
                } else if a.hovered() {
                    p.rect_stroke(tile, 12., Stroke::new(1.5, VIOLET), StrokeKind::Inside);
                }
                if a.double_clicked() {
                    commit = Some(i);
                }
                if a.clicked() {
                    next = Some(i);
                }
            }
            self.covers.retain(|key, _| in_range.contains(key));
        });
        if let Some(i) = next {
            self.select_cover(i, reduce_motion);
            ui.ctx().request_repaint();
        }
        commit
    }
}

fn scroller(ui: &mut Ui, area: Rect, id: &str, show: impl FnOnce(&mut Ui, Rect)) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.set_clip_rect(area);
        let style = ui.style_mut();
        style.spacing.scroll.bar_width = 6.;
        style.spacing.scroll.floating_width = 6.;
        style.spacing.scroll.foreground_color = true;
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            widget.fg_stroke.color = SCROLL;
        }
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .max_height(area.height())
            .show_viewport(ui, show);
    });
}

fn action(ui: &Ui, r: Rect, id: &str, title: &str, import: bool) -> bool {
    let a = interact(ui, r, id);
    let p = ui.painter();
    surface(
        p,
        r,
        9.,
        if import {
            C::from_rgba_unmultiplied(139, 92, 246, if a.hovered() { 72 } else { 41 })
        } else {
            C::from_white_alpha(if a.hovered() { 26 } else { 13 })
        },
        NONE,
    );
    let ink = if import || a.hovered() { WHITE } else { MUTED };
    let x = r.center().x - (text_width(p, title, 13., true) + 23.) / 2.;
    let at = pos2(x + 8., r.center().y);
    let stroke = Stroke::new(1.3, ink);
    if import {
        p.line(
            [
                at + vec2(-6., 2.),
                at + vec2(-6., 6.),
                at + vec2(6., 6.),
                at + vec2(6., 2.),
            ]
            .to_vec(),
            stroke,
        );
        p.line(
            [at + vec2(-4., -2.), at + vec2(0., -6.), at + vec2(4., -2.)].to_vec(),
            stroke,
        );
        p.line_segment([at + vec2(0., -6.), at + vec2(0., 3.)], stroke);
    } else {
        p.rect_stroke(
            Rect::from_center_size(at, vec2(13., 13.)),
            3.,
            stroke,
            StrokeKind::Inside,
        );
        for (x, y) in [(-3., -3.), (-3., 3.), (0., 0.), (3., -3.), (3., 3.)] {
            p.circle_filled(at + vec2(x, y), 0.8, ink);
        }
    }
    label(
        p,
        pos2(x + 23., at.y),
        Align2::LEFT_CENTER,
        title,
        13.,
        true,
        ink,
    );
    a.clicked()
}

fn selection_ring(p: &Painter, r: Rect, round: f32, phase: f32) {
    p.rect_stroke(
        r.expand(1.),
        round,
        Stroke::new(3., PINK.gamma_multiply(0.25)),
        StrokeKind::Inside,
    );
    windows_ui::ring(p, r, round, 2., |pos| {
        let t = (0.5 + 0.5 * ((pos.x - pos.y + phase) * 0.025).cos()).powi(4);
        let m = |a: u8| (a as f32 + (255. - a as f32) * t) as u8;
        C::from_rgb(255, m(78), m(210))
    });
}

#[derive(Clone, Copy)]
struct CoverSlot {
    column: usize,
    cols: usize,
    start: usize,
    rows: usize,
}

// Same first row, height rhythm and gap filling as the WebView2 mosaic.
fn cover_layout(count: usize, active: usize) -> Vec<CoverSlot> {
    fn place(
        cards: &mut [CoverSlot],
        heights: &mut [usize; 4],
        last: &mut [usize; 4],
        i: usize,
        c: usize,
        cols: usize,
        row: usize,
        rows: usize,
    ) {
        cards[i] = CoverSlot {
            column: c,
            cols,
            start: row,
            rows,
        };
        for j in c..c + cols {
            heights[j] = row + rows;
            last[j] = i;
        }
    }
    let mut cards: Vec<_> = (0..count)
        .map(|i| CoverSlot {
            column: 0,
            cols: if i == active { 2 } else { 1 },
            start: 0,
            rows: if i == active {
                12
            } else {
                COVER_ROWS[i % COVER_ROWS.len()]
            },
        })
        .collect();
    let mut heights = [0; 4];
    let mut last = [usize::MAX; 4];
    let first = match active {
        0 => [(0, 2, 12), (2, 1, 13), (3, 1, 10)],
        1 => [(0, 1, 8), (1, 2, 12), (3, 1, 10)],
        2 => [(0, 1, 8), (1, 1, 13), (2, 2, 12)],
        _ => [(0, 1, 8), (1, 2, 8), (3, 1, 10)],
    };
    for (i, (c, w, h)) in first.into_iter().enumerate() {
        place(&mut cards, &mut heights, &mut last, i, c, w, 0, h);
    }
    for i in 3..count {
        let w = cards[i].cols;
        let c = if w == 1 {
            (0..4).min_by_key(|&c| heights[c]).unwrap()
        } else {
            (0..3)
                .filter(|&c| {
                    (c..c + 2).all(|j| {
                        heights[j] == heights[c].max(heights[c + 1]) || cards[last[j]].cols == 1
                    })
                })
                .min_by_key(|&c| {
                    (
                        heights[c].max(heights[c + 1]),
                        heights[c].abs_diff(heights[c + 1]),
                    )
                })
                .unwrap_or(0)
        };
        let row = heights[c..c + w].iter().copied().max().unwrap();
        for j in c..c + w {
            if heights[j] < row && cards[last[j]].cols == 1 {
                cards[last[j]].rows += row - heights[j];
            }
        }
        let h = cards[i].rows;
        place(&mut cards, &mut heights, &mut last, i, c, w, row, h);
    }
    for c in 0..3 {
        let (a, b) = (last[c], last[c + 1]);
        if a == b {
            continue;
        }
        if cards[a].cols == 1 && cards[a].start == heights[c + 1] && heights[c + 1] < heights[c] {
            cards[a].cols = 2;
            heights[c + 1] = heights[c];
            last[c + 1] = a;
        } else if cards[b].cols == 1 && cards[b].start == heights[c] && heights[c] < heights[c + 1]
        {
            cards[b].column = c;
            cards[b].cols = 2;
            heights[c] = heights[c + 1];
            last[c] = b;
        }
    }
    let bottom = *heights.iter().max().unwrap();
    for c in 0..4 {
        let i = last[c];
        let end = cards[i].start + cards[i].rows;
        if end < bottom
            && (cards[i].cols == 1 || (cards[i].column..cards[i].column + 2).all(|j| last[j] == i))
        {
            cards[i].rows += bottom - end;
        }
    }
    cards
}

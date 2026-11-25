use super::rect;
use crate::{
    home::{CollectionArt, Home},
    minecraft::Instance,
};
use egui::{
    Align2, Color32 as Color, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2,
};
use std::{collections::BTreeSet, path::PathBuf, sync::OnceLock, time::Instant};

const COLUMNS: usize = 5;
const GAP: f32 = 8.;
const OPTION_HEIGHT: f32 = 23.;
const INK: Color = Color::from_rgb(248, 250, 252);
const MUTED: Color = Color::from_rgb(148, 163, 184);
const LILAC: Color = Color::from_rgb(192, 132, 252);
const JADE: Color = Color::from_rgb(52, 211, 153);
const BACKGROUND: Color = Color::from_rgba_premultiplied(13, 10, 26, 250);
const TABS: [&str; 4] = ["Todas", "Instancias", "Modpacks", "Favoritos"];
const FIELDS: [&str; 3] = ["Versión", "Loader", "Java"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Tab(usize),
    Filters,
    Search,
    SearchField,
    ClearSearch,
    ResetFilters,
    Field(usize),
    FilterValue(usize, usize),
    Slot(usize),
    Selected,
    Favorite(usize),
    Folder(usize),
    Create,
    Surface,
}

struct Layout {
    outer: Rect,
    filters: Rect,
    search: Rect,
    tabs: [Rect; 4],
    fields: [Rect; 3],
    reset: Rect,
    search_box: Rect,
    clear: Rect,
    grid: Rect,
    footer: Rect,
    cell: f32,
}

impl Layout {
    fn new(outer: Rect, state: &Collection) -> Self {
        let s = (outer.width() / 305.).min(1.);
        let x = outer.center().x - 145. * s;
        let y = outer.top() + 48.;
        let nav = |x0, width| rect(x + x0 * s, y, width * s, 23.);
        let h = if state.filters { 34. } else { 0. };
        let search_box = rect(outer.left() + 14., y + 29. + h, outer.width() - 28., 27.);
        let footer = rect(
            outer.left() + 14.,
            outer.bottom() - 62.,
            outer.width() - 28.,
            50.,
        );
        let grid = Rect::from_min_max(
            pos2(
                outer.left() + 14.,
                y + 37. + h + if state.search { 34. } else { 0. },
            ),
            pos2(outer.right() - 14., footer.top() - 12.),
        );
        let field_w = (outer.width() - 50.) / 3.;
        Self {
            outer,
            filters: nav(0., 24.),
            search: nav(30., 24.),
            tabs: [
                nav(60., 43.),
                nav(109., 56.),
                nav(171., 57.),
                nav(234., 57.),
            ],
            fields: std::array::from_fn(|i| {
                rect(
                    outer.left() + 14. + (field_w + 6.) * i as f32,
                    y + 29.,
                    field_w,
                    26.,
                )
            }),
            reset: rect(outer.right() - 19., y + 31., 14., 22.),
            search_box,
            clear: rect(
                search_box.right() - 26.,
                search_box.top(),
                26.,
                search_box.height(),
            ),
            grid,
            footer,
            cell: ((grid.width() - GAP * (COLUMNS - 1) as f32) / COLUMNS as f32).max(1.),
        }
    }

    fn slot(&self, i: usize, scroll: f32) -> Rect {
        let pitch = self.cell + GAP;
        rect(
            self.grid.left() + (i % COLUMNS) as f32 * pitch,
            self.grid.top() + (i / COLUMNS) as f32 * pitch - scroll,
            self.cell,
            self.cell,
        )
    }

    fn scroll_limit(&self, len: usize) -> f32 {
        let rows = (len + 1).div_ceil(COLUMNS); // + Crear
        (rows as f32 * (self.cell + GAP) - GAP - self.grid.height().max(0.)).max(0.)
    }

    fn option(&self, field: usize, option: usize) -> Rect {
        let r = self.fields[field];
        rect(
            r.left(),
            r.bottom() + 4. + OPTION_HEIGHT * option as f32,
            r.width(),
            OPTION_HEIGHT,
        )
    }

    fn options(&self, popup: Popup, len: usize) -> std::ops::Range<usize> {
        let room = ((self.outer.bottom() - 4. - self.option(popup.field, 0).top()) / OPTION_HEIGHT)
            .floor()
            .max(0.) as usize;
        let visible = room.min(len);
        let first = popup.offset.min(len - visible);
        first..first + visible
    }
}

#[derive(Clone, Copy)]
struct Popup {
    field: usize,
    offset: usize,
}

pub(super) struct Collection {
    tab: usize,
    filters: bool,
    search: bool,
    typing: bool,
    query: String,
    values: [Option<String>; 3],
    popup: Option<Popup>,
    scroll: f32,
    context: Option<(usize, Pos2)>,
    favorites: BTreeSet<String>,
    favorites_path: Option<PathBuf>,
    saved_favorites: bool,
    epoch: Instant,
}

impl Collection {
    pub fn new(now: Instant) -> Self {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join("IkaNative/home-favorites.json"));
        let favorites = path
            .as_ref()
            .and_then(|file| std::fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Self {
            tab: 0,
            filters: false,
            search: false,
            typing: false,
            query: String::new(),
            values: Default::default(),
            popup: None,
            scroll: 0.,
            context: None,
            favorites_path: path,
            saved_favorites: favorites.is_some(),
            favorites: favorites.unwrap_or_default(),
            epoch: now,
        }
    }

    pub fn import_favorites(&mut self, legacy: &[String]) -> bool {
        if self.saved_favorites {
            return false;
        }
        self.saved_favorites = true;
        self.favorites = legacy.iter().map(|path| path_key(path)).collect();
        !self.favorites.is_empty()
    }

    fn save_favorites(&mut self) {
        self.saved_favorites = true;
        let Some(path) = &self.favorites_path else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            let dir = path.parent().unwrap();
            std::fs::create_dir_all(dir)?;
            let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
            serde_json::to_writer(tmp.as_file_mut(), &self.favorites)?;
            tmp.persist(path)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Favoritos de Inicio: {error}");
        }
    }

    pub fn dismiss(&mut self) {
        self.typing = false;
        self.popup = None;
        self.context = None;
    }

    pub fn editing(&self) -> bool {
        self.typing
    }

    pub fn input(&mut self, text: &str) {
        let remaining = 100usize.saturating_sub(self.query.chars().count());
        self.query
            .extend(text.chars().filter(|ch| !ch.is_control()).take(remaining));
        self.scroll = 0.;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.scroll = 0.;
    }

    fn matching(&self, home: &Home) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        home.collection()
            .enumerate()
            .filter_map(|(i, inst)| {
                let keep = match self.tab {
                    1 => !is_modpack(inst),
                    2 => is_modpack(inst),
                    3 => self.favorites.contains(&path_key(&inst.path)),
                    _ => true,
                };
                (keep
                    && (query.is_empty() || inst.name.to_lowercase().contains(&query))
                    && self
                        .values
                        .iter()
                        .enumerate()
                        .all(|(f, value)| value.as_ref().is_none_or(|v| v == field(inst, f))))
                .then_some(i)
            })
            .collect()
    }

    fn values(home: &Home, f: usize) -> Vec<&str> {
        let values: BTreeSet<_> = home
            .collection()
            .map(|inst| field(inst, f))
            .filter(|value| !value.is_empty())
            .collect();
        values.into_iter().collect()
    }

    fn has_filters(&self) -> bool {
        self.values.iter().any(Option::is_some) || !self.query.trim().is_empty()
    }

    fn context_box(outer: Rect, p: Pos2) -> Rect {
        let size = vec2(160., 54.);
        Rect::from_min_size(p.clamp(outer.min, (outer.max - size).max(outer.min)), size)
    }

    pub fn hit(&self, bounds: Rect, p: Pos2, home: &Home) -> Option<Hit> {
        if bounds.height() < 100. || !bounds.contains(p) {
            return None;
        }
        let g = Layout::new(bounds, self);
        if let Some((index, anchor)) = self.context
            && Self::context_box(bounds, anchor).contains(p)
        {
            return Some(if p.y < Self::context_box(bounds, anchor).center().y {
                Hit::Favorite(index)
            } else {
                Hit::Folder(index)
            });
        }
        if let Some(popup) = self.popup {
            for (row, i) in g
                .options(popup, Self::values(home, popup.field).len() + 1)
                .enumerate()
            {
                if g.option(popup.field, row).contains(p) {
                    return Some(Hit::FilterValue(popup.field, i));
                }
            }
        }
        if g.filters.contains(p) {
            return Some(Hit::Filters);
        }
        if g.search.contains(p) {
            return Some(Hit::Search);
        }
        if let Some(i) = g.tabs.iter().position(|b| b.contains(p)) {
            return Some(Hit::Tab(i));
        }
        if self.filters {
            if g.reset.contains(p) && self.has_filters() {
                return Some(Hit::ResetFilters);
            }
            if let Some(i) = g.fields.iter().position(|b| b.contains(p)) {
                return Some(Hit::Field(i));
            }
        }
        if self.search && g.search_box.contains(p) {
            return Some(if g.clear.contains(p) && !self.query.is_empty() {
                Hit::ClearSearch
            } else {
                Hit::SearchField
            });
        }
        if g.footer.contains(p) && home.collection_card(home.selected()).is_some() {
            return Some(Hit::Selected);
        }
        if g.grid.contains(p) {
            let matches = self.matching(home);
            let scroll = self.scroll.min(g.scroll_limit(matches.len()));
            for (i, index) in matches.iter().enumerate() {
                if g.slot(i, scroll).contains(p) {
                    return Some(Hit::Slot(*index));
                }
            }
            if g.slot(matches.len(), scroll).contains(p) {
                return Some(Hit::Create);
            }
        }
        Some(Hit::Surface)
    }

    pub fn open_context(&mut self, index: usize, cursor: Pos2) {
        self.dismiss();
        self.context = Some((index, cursor));
    }

    pub fn activate(&mut self, hit: Hit, home: &Home) {
        self.typing = false;
        if !matches!(hit, Hit::Field(_) | Hit::FilterValue(_, _)) {
            self.popup = None;
        }
        self.context = None;
        match hit {
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0.;
            }
            Hit::Filters => self.filters = !self.filters,
            Hit::Search => {
                self.search = !self.search;
                self.typing = self.search;
            }
            Hit::SearchField => self.typing = true,
            Hit::ClearSearch => {
                self.query.clear();
                self.scroll = 0.;
                self.typing = true;
            }
            Hit::ResetFilters => {
                self.values = Default::default();
                self.query.clear();
                self.scroll = 0.;
            }
            Hit::Field(field) => {
                self.popup = (!self.popup.is_some_and(|p| p.field == field))
                    .then_some(Popup { field, offset: 0 });
            }
            Hit::FilterValue(f, i) => {
                self.values[f] = i
                    .checked_sub(1)
                    .and_then(|i| Self::values(home, f).get(i).map(|v| (*v).to_owned()));
                self.popup = None;
                self.scroll = 0.;
            }
            Hit::Favorite(i) => {
                if let Some((inst, _)) = home.collection_card(i) {
                    let key = path_key(&inst.path);
                    if !self.favorites.remove(&key) {
                        self.favorites.insert(key);
                    }
                    self.save_favorites();
                }
            }
            _ => {}
        }
    }

    pub fn scroll(&mut self, bounds: Rect, cursor: Pos2, delta: f32, home: &Home) -> bool {
        let g = Layout::new(bounds, self);
        if let Some(popup) = &mut self.popup {
            let len = Self::values(home, popup.field).len() + 1;
            let rows = g.options(*popup, len);
            if !rows.is_empty()
                && g.option(popup.field, 0)
                    .union(g.option(popup.field, rows.len() - 1))
                    .contains(cursor)
            {
                popup.offset = rows
                    .start
                    .saturating_add_signed(-delta.signum() as isize)
                    .min(len - rows.len());
                return true;
            }
        }
        if !g.grid.contains(cursor) {
            return false;
        }
        self.scroll = (self.scroll - delta * (g.cell + GAP))
            .clamp(0., g.scroll_limit(self.matching(home).len()));
        true
    }

    pub fn paint(
        &self,
        p: &Painter,
        bounds: Rect,
        home: &Home,
        art: Option<CollectionArt>,
        cursor: Option<Pos2>,
        opacity: f32,
        now: Instant,
        reduced_motion: bool,
    ) {
        if bounds.height() < 100. {
            return;
        }
        let g = Layout::new(bounds, self);
        let p = p.with_clip_rect(g.outer);
        let tint = |c: Color| c.gamma_multiply(opacity);
        p.rect_filled(g.outer, 12, tint(BACKGROUND));
        let center = bounds.min + vec2(24., 24.);
        for y in [-5., 5.] {
            for x in [-5., 5.] {
                p.rect_stroke(
                    Rect::from_center_size(center + vec2(x, y), Vec2::splat(7.)),
                    1,
                    Stroke::new(1.7, tint(JADE)),
                    StrokeKind::Middle,
                );
            }
        }
        text(
            &p,
            bounds.min + vec2(44., 24.),
            Align2::LEFT_CENTER,
            "Mi Colección",
            13.5,
            tint(INK),
        );

        let hover = |r: Rect| cursor.is_some_and(|p| r.contains(p));
        let filter_color = tint(if self.filters || self.has_filters() {
            LILAC
        } else if hover(g.filters) {
            INK
        } else {
            MUTED
        });
        let c = g.filters.center();
        p.add(egui::Shape::closed_line(
            vec![
                c + vec2(-6., -5.),
                c + vec2(6., -5.),
                c + vec2(1.6, 0.4),
                c + vec2(1.6, 5.5),
                c + vec2(-1.6, 3.7),
                c + vec2(-1.6, 0.4),
            ],
            Stroke::new(1.15, filter_color),
        ));
        magnifier(
            &p,
            g.search.center(),
            tint(if self.search || !self.query.is_empty() {
                LILAC
            } else if hover(g.search) {
                INK
            } else {
                MUTED
            }),
        );
        for (i, title) in TABS.iter().enumerate() {
            text(
                &p,
                g.tabs[i].center(),
                Align2::CENTER_CENTER,
                title,
                11.5 * (bounds.width() / 305.).min(1.),
                tint(if i == self.tab || hover(g.tabs[i]) {
                    INK
                } else {
                    MUTED
                }),
            );
        }
        if self.filters {
            for (i, field) in g.fields.iter().enumerate() {
                outline(
                    &p,
                    *field,
                    5,
                    tint(
                        if hover(*field) || self.popup.is_some_and(|p| p.field == i) {
                            LILAC
                        } else {
                            Color::from_white_alpha(31)
                        },
                    ),
                );
                let value =
                    self.values[i]
                        .as_deref()
                        .unwrap_or(if i == 0 { "Todas" } else { "Todos" });
                truncate(
                    &p,
                    field.shrink2(vec2(5., 0.)),
                    &format!("{}: {value} ▾", FIELDS[i]),
                    10.,
                    tint(INK),
                    1,
                );
            }
            if self.has_filters() {
                text(
                    &p,
                    g.reset.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    14.,
                    tint(MUTED),
                );
            }
        }
        if self.search {
            outline(
                &p,
                g.search_box,
                6,
                tint(if self.typing {
                    LILAC
                } else {
                    Color::from_white_alpha(31)
                }),
            );
            magnifier(&p, g.search_box.left_center() + vec2(13., 0.), tint(MUTED));
            if self.query.is_empty() {
                text(
                    &p,
                    g.search_box.left_center() + vec2(25., 0.),
                    Align2::LEFT_CENTER,
                    "Buscar por nombre…",
                    11.5,
                    tint(MUTED),
                );
            } else {
                let r = Rect::from_min_max(
                    g.search_box.min + vec2(25., 0.),
                    pos2(g.clear.left(), g.search_box.bottom()),
                );
                truncate(&p, r, &self.query, 11.5, tint(INK), 1);
                text(
                    &p,
                    g.clear.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    15.,
                    tint(MUTED),
                );
            }
        }

        let matches = self.matching(home);
        let scroll = self.scroll.min(g.scroll_limit(matches.len()));
        if g.grid.is_positive() {
            let grid = p.with_clip_rect(g.grid.expand2(vec2(12., 7.)));
            let mut selected = None;
            for (i, index) in matches.iter().enumerate() {
                let r = g.slot(i, scroll);
                if !r.intersects(g.grid) {
                    continue;
                }
                if *index == home.selected() {
                    selected = Some((*index, r));
                } else {
                    slot(&grid, r, *index, home, art, hover(r), tint(Color::WHITE));
                }
            }
            if let Some((index, r)) = selected {
                grid.add(
                    egui::epaint::RectShape::filled(
                        r,
                        0,
                        tint(Color::from_rgba_unmultiplied(0, 48, 228, 176)),
                    )
                    .with_blur_width(13.),
                );
                grid.add(
                    egui::epaint::RectShape::filled(
                        r,
                        0,
                        tint(Color::from_rgba_unmultiplied(255, 78, 210, 92)),
                    )
                    .with_blur_width(7.),
                );
                // Keep the glow outside, including when an instance icon has transparent pixels.
                grid.rect_filled(r, 0, tint(Color::from_rgb(13, 10, 26)));
                slot(&grid, r, index, home, art, hover(r), tint(Color::WHITE));
                let phase = if reduced_motion {
                    0.
                } else {
                    now.duration_since(self.epoch).as_secs_f32() % 10. / 10.
                };
                selected_border(&grid, r, phase, opacity);
            }
            if matches.is_empty() {
                let pos = g.grid.center();
                magnifier(&grid, pos - vec2(0., 15.), tint(MUTED));
                text(
                    &grid,
                    pos + vec2(0., 7.),
                    Align2::CENTER_CENTER,
                    if home.ready() {
                        "No hay instancias con estos filtros"
                    } else {
                        "Cargando instancias…"
                    },
                    12.,
                    tint(MUTED),
                );
            }
            let add = g.slot(matches.len(), scroll);
            if add.intersects(g.grid) {
                add_slot(&grid, add, opacity);
            }
        }

        if let Some((inst, _)) = home.collection_card(home.selected()) {
            outline(
                &p,
                g.footer,
                9,
                tint(if hover(g.footer) {
                    LILAC
                } else {
                    Color::from_white_alpha(31)
                }),
            );
            let r = g.footer.shrink2(vec2(11., 0.));
            let tag = "Instancia seleccionada:";
            text(
                &p,
                r.left_top() + vec2(0., 7.),
                Align2::LEFT_TOP,
                tag,
                12.,
                tint(LILAC),
            );
            let name = rect(r.left() + 126., r.top() + 3., r.width() - 126., 21.);
            truncate(&p, name, &inst.name, 12., tint(INK), 1);
            folder_icon(&p, r.left_top() + vec2(6., 31.), tint(LILAC));
            let path = rect(r.left() + 17., r.top() + 24., r.width() - 17., 23.);
            truncate_with_font(
                &p,
                path,
                &inst.path,
                FontId::monospace(10.5),
                tint(MUTED),
                2,
            );
        }
        if let Some(popup) = self.popup {
            let f = popup.field;
            let values = Self::values(home, f);
            let options = g.options(popup, values.len() + 1);
            for (row, i) in options.enumerate() {
                let value = if i == 0 {
                    if f == 0 { "Todas" } else { "Todos" }
                } else {
                    values[i - 1]
                };
                let r = g.option(f, row);
                p.rect_filled(
                    r,
                    2,
                    tint(if hover(r) {
                        Color::from_rgb(49, 32, 74)
                    } else {
                        Color::from_rgb(24, 19, 42)
                    }),
                );
                truncate(&p, r.shrink2(vec2(6., 0.)), value, 10.5, tint(INK), 1);
            }
        }
        if let Some((index, anchor)) = self.context {
            let r = Self::context_box(bounds, anchor);
            p.rect_filled(r, 7, tint(Color::from_rgb(25, 19, 43)));
            outline(&p, r, 7, tint(Color::from_white_alpha(31)));
            let favorite = home
                .collection_card(index)
                .is_some_and(|(inst, _)| self.favorites.contains(&path_key(&inst.path)));
            let favorite_text = if favorite {
                "Quitar de Favoritos"
            } else {
                "Añadir a Favoritos"
            };
            for (i, label) in [favorite_text, "Abrir en el Explorador"]
                .into_iter()
                .enumerate()
            {
                let item = rect(
                    r.left() + 4.,
                    r.top() + 3. + i as f32 * 24.,
                    r.width() - 8.,
                    24.,
                );
                if hover(item) {
                    p.rect_filled(item, 4, tint(LILAC.gamma_multiply(0.2)));
                }
                text(
                    &p,
                    item.left_center() + vec2(7., 0.),
                    Align2::LEFT_CENTER,
                    label,
                    11.5,
                    tint(INK),
                );
            }
        }
    }
}

fn field(inst: &Instance, f: usize) -> &str {
    [&inst.mc_version, &inst.loader, inst.java_vendor][f]
}

fn path_key(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn is_modpack(inst: &Instance) -> bool {
    let name = inst.name.to_lowercase();
    let folder = inst
        .path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    inst.mod_count >= 15
        || name.contains("pack")
        || name.contains("better")
        || name.contains("all the mods")
        || folder.contains("pack")
}

fn slot(
    p: &Painter,
    r: Rect,
    index: usize,
    home: &Home,
    art: Option<CollectionArt>,
    hover: bool,
    tint: Color,
) {
    if let Some((_, icon)) = home.collection_card(index)
        && let Some(art) = art
    {
        let clip = p.with_clip_rect(r);
        let image = if hover { r.expand(r.width() * 0.04) } else { r };
        clip.image(art.id, image, art.uv(icon), tint);
    }
    outline(
        p,
        r,
        0,
        Color::from_white_alpha(31).gamma_multiply(tint.a() as f32 / 255.),
    );
}

fn add_slot(p: &Painter, r: Rect, opacity: f32) {
    let s = Stroke::new(1., Color::from_white_alpha(38).gamma_multiply(opacity));
    for (a, b) in [
        (r.left_top(), r.right_top()),
        (r.right_top(), r.right_bottom()),
        (r.right_bottom(), r.left_bottom()),
        (r.left_bottom(), r.left_top()),
    ] {
        p.extend(egui::Shape::dashed_line(&[a, b], s, 4., 3.));
    }
    let c = r.center() - vec2(0., 7.);
    let ink = JADE.gamma_multiply(opacity);
    for d in [vec2(6., 0.), vec2(0., 6.)] {
        p.line_segment([c - d, c + d], Stroke::new(1.9, ink));
    }
    text(
        p,
        r.center() + vec2(0., 10.),
        Align2::CENTER_CENTER,
        "CREAR",
        10.,
        ink,
    );
}

fn selected_border(p: &Painter, r: Rect, time: f32, opacity: f32) {
    static RAMP: OnceLock<Vec<[u8; 3]>> = OnceLock::new();
    let colors = RAMP.get_or_init(|| {
        image::load_from_memory(include_bytes!("../home/gradient.png"))
            .expect("bundled WebView2 gradient")
            .to_rgb8()
            .pixels()
            .map(|p| p.0)
            .collect()
    });
    let (sin, cos) = (-139_f32.to_radians() + time * std::f32::consts::TAU).sin_cos();
    let c = r.center();
    let mut mesh = egui::Mesh::default();
    let corners = |r: Rect| {
        [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
    };
    let outer = corners(r);
    let inner = corners(r.shrink((r.width() * 0.25).min(2.8)));
    for edge in 0..4 {
        let next = (edge + 1) % 4;
        let first = mesh.vertices.len() as u32;
        for i in 0..=36 {
            let u = i as f32 / 36.;
            let q = outer[edge].lerp(outer[next], u);
            let t = ((q.x - c.x) * cos + (q.y - c.y) * sin) / r.width().max(1.) + 0.5;
            let [r, g, b] = colors[(t.clamp(0., 1.) * (colors.len() - 1) as f32) as usize];
            let color = Color::from_rgb(r, g, b).gamma_multiply(opacity);
            mesh.colored_vertex(q, color);
            mesh.colored_vertex(inner[edge].lerp(inner[next], u), color);
            if i > 0 {
                let k = first + (i - 1) * 2;
                mesh.add_triangle(k, k + 1, k + 2);
                mesh.add_triangle(k + 1, k + 2, k + 3);
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

fn text(p: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color) {
    p.text(pos, align, text, FontId::proportional(size), color);
}

fn truncate(p: &Painter, r: Rect, text: &str, size: f32, color: Color, rows: usize) {
    truncate_with_font(p, r, text, FontId::proportional(size), color, rows);
}

fn truncate_with_font(p: &Painter, r: Rect, text: &str, font: FontId, color: Color, rows: usize) {
    if !r.is_positive() {
        return;
    }
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, r.width());
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = rows > 1;
    let galley = p.layout_job(job);
    p.with_clip_rect(r).galley(
        pos2(r.left(), r.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

fn outline(p: &Painter, r: Rect, radius: u8, color: Color) {
    p.rect_stroke(r, radius, Stroke::new(1., color), StrokeKind::Inside);
}

fn magnifier(p: &Painter, center: Pos2, color: Color) {
    let s = Stroke::new(1.35, color);
    p.circle_stroke(center - vec2(1.1, 1.1), 4.1, s);
    p.line_segment([center + vec2(2.2, 2.2), center + vec2(5.4, 5.4)], s);
}

fn folder_icon(p: &Painter, c: Pos2, color: Color) {
    let points = [
        (-5., -4.),
        (-1., -4.),
        (0.8, -2.),
        (5., -2.),
        (5., 4.),
        (-5., 4.),
    ]
    .into_iter()
    .map(|(x, y)| c + vec2(x, y))
    .collect();
    p.add(egui::Shape::closed_line(points, Stroke::new(1.2, color)));
}

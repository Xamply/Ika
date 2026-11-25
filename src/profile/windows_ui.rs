//! Native versions of the WebView2 settings hub and friends dialog.
use super::*;
#[path = "skin_menu.rs"]
mod skin_menu;
use egui::{
    Align, Align2, FontFamily, FontId, Mesh, Painter, Response, Sense, Stroke, StrokeKind, Ui,
    UiBuilder,
};
pub(super) use skin_menu::SkinMenuActions;
use std::sync::Arc;

const INK: Color32 = Color32::from_rgb(235, 240, 253);
const MUTED: Color32 = Color32::from_rgb(159, 175, 196);
const PLACEHOLDER: Color32 = Color32::from_rgb(107, 124, 151);
const SURFACE: Color32 = Color32::from_rgb(4, 4, 22);
const ROW: Color32 = Color32::from_rgb(15, 15, 33);
const CYAN: Color32 = Color32::from_rgb(0, 239, 226);
const PURPLE: Color32 = Color32::from_rgb(76, 21, 242);
const FIELD: Color32 = Color32::from_rgb(17, 13, 39);
const FIELD_EDGE: Color32 = Color32::from_rgb(72, 42, 116);

#[derive(Default)]
pub(super) struct Dialog {
    page: Option<usize>,
    search: String,
    focus_friend: bool,
    account_add: bool,
    focus_new_account: bool,
    pub(super) confirm_skin_delete: bool,
    avatar_menu: bool,
    result_focus: Option<&'static str>,
    result_position: usize,
    pub(super) surface: Option<Rect>,
    generation: u64,
    target: Option<Instant>,
}

#[derive(Default)]
pub(super) struct Actions {
    pub close: bool,
    pub cover: bool,
    pub crop: bool,
    pub crop_done: bool,
    pub crop_cancel: bool,
    pub add: bool,
    pub identity_save: bool,
    pub avatar: bool,
    pub avatar_clear: bool,
    pub sign_out: bool,
    pub browse_instances: bool,
    pub save_geometry: bool,
    pub select_account: Option<String>,
    pub friend: Option<(&'static str, String)>,
    pub app: Option<AppAction>,
}

#[derive(Clone, Copy)]
pub enum AppAction {
    Wallpapers,
    WallpaperDim(f32),
    GameWindow(bool),
    InstanceFolderChanged,
}

pub(super) struct Art {
    cover: TextureHandle,
    icons: TextureHandle,
    doodle: TextureHandle,
    laika: TextureHandle,
}
impl Art {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let load = |name, data| {
            let pixels = image::load_from_memory(data)
                .expect("valid bundled profile artwork")
                .into_rgba8();
            load_image(ctx, name, &pixels, egui::TextureOptions::LINEAR)
        };
        Self {
            cover: load(
                "default-profile-cover",
                include_bytes!("art/default-cover.jpg"),
            ),
            icons: load("profile-symbols", include_bytes!("art/icons.png")),
            doodle: {
                let pixels = image::load_from_memory(include_bytes!("art/doodle.png"))
                    .expect("valid doodle tile")
                    .into_rgba8();
                load_image(
                    ctx,
                    "profile-doodle",
                    &pixels,
                    egui::TextureOptions::LINEAR_REPEAT,
                )
            },
            laika: load("laika-portrait", include_bytes!("art/laika.webp")),
        }
    }
}

impl Dialog {
    pub(super) fn open(&mut self, panel: Panel) {
        *self = Self {
            focus_friend: panel == Panel::Friends,
            generation: self.generation.wrapping_add(1),
            ..Default::default()
        };
    }
}

pub(super) fn install_fonts(ctx: &egui::Context) {
    let root = std::env::var_os("WINDIR").map(std::path::PathBuf::from);
    let mut fonts = egui::FontDefinitions::default();
    let strong = FontFamily::Name("ika-semibold".into());
    let mut heavy = fonts.families[&FontFamily::Proportional].clone();
    if let Some(root) = root {
        for (file, family, list) in [
            (
                "segoeui.ttf",
                "ika-ui",
                fonts.families.get_mut(&FontFamily::Proportional).unwrap(),
            ),
            ("segoeuib.ttf", "ika-bold", &mut heavy),
        ] {
            if let Ok(bytes) = std::fs::read(root.join("Fonts").join(file)) {
                fonts.font_data.insert(
                    family.to_owned(),
                    Arc::new(egui::FontData::from_owned(bytes)),
                );
                list.insert(0, family.to_owned());
            }
        }
    }
    fonts.families.insert(strong, heavy);
    crate::home_widgets::add_font(&mut fonts);
    ctx.set_fonts(fonts);
}

fn font(size: f32, bold: bool) -> FontId {
    FontId::new(
        size,
        if bold {
            FontFamily::Name("ika-semibold".into())
        } else {
            FontFamily::Proportional
        },
    )
}

fn label(
    p: &Painter,
    xy: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    bold: bool,
    color: Color32,
) -> Rect {
    p.text(xy, align, text, font(size, bold), color)
}
fn text(text: &str, size: f32, bold: bool, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .font(font(size, bold))
        .color(color)
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}
fn rect(xy: Pos2, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(xy + vec2(x, y), vec2(w.max(1.), h.max(1.)))
}
fn outline(p: &Painter, rect: Rect, radius: f32, fill: Color32, border: Color32) {
    p.rect(
        rect,
        radius,
        fill,
        Stroke::new(1., border),
        StrokeKind::Inside,
    );
}

fn quad(points: &mut Vec<Pos2>, from: Pos2, control: Pos2, to: Pos2) {
    for n in 1..=9 {
        let t = n as f32 / 9.;
        points.push(from.lerp(control, t).lerp(control.lerp(to, t), t));
    }
}
pub(super) fn rounded(rect: Rect, tl: f32, tr: f32, br: f32, bl: f32) -> Vec<Pos2> {
    let [x, y, r, b] = [rect.left(), rect.top(), rect.right(), rect.bottom()];
    let mut points = vec![pos2(x + tl, y), pos2(r - tr, y)];
    if tr > 0. {
        quad(&mut points, pos2(r - tr, y), pos2(r, y), pos2(r, y + tr));
    }
    points.push(pos2(r, b - br));
    if br > 0. {
        quad(&mut points, pos2(r, b - br), pos2(r, b), pos2(r - br, b));
    }
    points.push(pos2(x + bl, b));
    if bl > 0. {
        quad(&mut points, pos2(x + bl, b), pos2(x, b), pos2(x, b - bl));
    }
    points.push(pos2(x, y + tl));
    if tl > 0. {
        quad(&mut points, pos2(x, y + tl), pos2(x, y), pos2(x + tl, y));
    }
    points
}
pub(super) fn photo(p: &Painter, id: egui::TextureId, rect: Rect, uv: Rect, vs: &[Pos2], tint: Color32) {
    let mut mesh = Mesh::with_texture(id);
    for &pos in vs {
        let rel = (pos - rect.min) / rect.size();
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: uv.min + uv.size() * rel,
            color: tint,
        });
    }
    for i in 1..vs.len().saturating_sub(1) {
        mesh.add_triangle(0, i as u32, i as u32 + 1);
    }
    p.add(egui::Shape::mesh(mesh));
}
fn head(p: &Painter, tex: &TextureHandle, rect: Rect, radius: f32) {
    photo(
        p,
        tex.id(),
        rect,
        FULL_UV,
        &rounded(rect, radius, radius, radius, radius),
        Color32::WHITE,
    );
}
fn pixel_head_rect(rect: Rect, dpi: f32) -> Rect {
    let side = (rect.width().min(rect.height()) * dpi / 8.).round().max(1.) * 8.;
    let center = rect.center().to_vec2() * dpi;
    let min = (center - Vec2::splat(side * 0.5)).round() / dpi;
    Rect::from_min_size(min.to_pos2(), Vec2::splat(side / dpi))
}
fn pixel_head(p: &Painter, tex: &TextureHandle, rect: Rect, radius: f32) {
    head(p, tex, pixel_head_rect(rect, p.ctx().pixels_per_point()), radius);
}
fn icon(p: &Painter, art: &Art, index: usize, center: Pos2, size: f32, tint: Color32) {
    let atlas_width = art.icons.size_vec2().x / 3.;
    let uv = Rect::from_min_max(
        pos2((index * 24 + 2) as f32 / atlas_width, 2. / 24.),
        pos2((index * 24 + 22) as f32 / atlas_width, 22. / 24.),
    );
    p.image(
        art.icons.id(),
        Rect::from_center_size(center, Vec2::splat(size)),
        uv,
        tint,
    );
}
pub(super) fn shadow(p: &Painter, rect: Rect, radius: f32, a: u8) {
    for i in (1..=6).rev() {
        p.rect_filled(
            rect.translate(vec2(0., 4.)).expand(i as f32 * 2.),
            radius + i as f32 * 2.,
            Color32::from_black_alpha(a),
        );
    }
}
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}
pub(super) fn ring(p: &Painter, rect: Rect, radius: f32, width: f32, color: impl Fn(Pos2) -> Color32) {
    let outer = rounded(rect, radius, radius, radius, radius);
    let inner = rounded(
        rect.shrink(width),
        radius - width,
        radius - width,
        radius - width,
        radius - width,
    );
    let mut mesh = Mesh::default();
    for (i, (&a, &b)) in outer.iter().zip(&inner).enumerate() {
        let mut push = |pos| mesh.colored_vertex(pos, color(pos));
        push(a);
        push(b);
        if i != 0 {
            let n = (i * 2) as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n, n - 1, n + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
/// Paint the fill and gradient stroke on the same circular outline. Meshes
/// do not get egui's edge feathering, so supply a one-pixel coverage ramp.
pub(super) fn window_frame(
    p: &Painter,
    rect: Rect,
    radius: f32,
    fill: Color32,
    width: f32,
    stroke: impl Fn(Pos2) -> Color32,
) {
    let scale = p.ctx().layer_transform_to_global(p.layer_id()).map_or(1., |t| t.scaling);
    let px = p.ctx().pixels_per_point() * scale;
    let radius = radius.min(0.5 * rect.width().min(rect.height()));
    let width = width.min(radius * 0.5);
    let feather = (0.5 / px).min(width * 0.5);
    let inset = [-feather, feather, width - feather, width + feather];
    let corner_box = rect.shrink(radius);
    let steps = (radius * px * 0.65).ceil().clamp(12., 64.) as u32;
    let mut mesh = Mesh::default();
    let corners = [
        (corner_box.right_top(), -90.0_f32),
        (corner_box.right_bottom(), 0.),
        (corner_box.left_bottom(), 90.),
        (corner_box.left_top(), 180.),
    ];
    let mut push = |band: [Pos2; 4]| {
        for (point, color) in band.into_iter().zip([
            Color32::TRANSPARENT, stroke(band[1]), stroke(band[2]), fill,
        ]) {
            mesh.colored_vertex(point, color);
        }
    };
    for (i, &(center, angle)) in corners.iter().enumerate() {
        for step in 0..=steps {
            let unit = Vec2::angled((angle + 90. * step as f32 / steps as f32).to_radians());
            push(inset.map(|d| center + unit * (radius - d)));
        }
        // Sample the same gradient along the straight segments as in the arcs.
        let (next, next_angle) = corners[(i + 1) % corners.len()];
        let spacing = (rect.width() / 40.).clamp(1., 14.);
        let line = ((next - center).length() / spacing).ceil().max(1.) as u32;
        let unit = Vec2::angled(next_angle.to_radians());
        for step in 1..line {
            let center = center.lerp(next, step as f32 / line as f32);
            push(inset.map(|d| center + unit * (radius - d)));
        }
    }
    let count = mesh.vertices.len() as u32 / 4;
    for i in 0..count {
        let a = i * 4;
        let b = ((i + 1) % count) * 4;
        for band in 0..3 {
            mesh.add_triangle(a + band, b + band, a + band + 1);
            mesh.add_triangle(a + band + 1, b + band, b + band + 1);
        }
    }
    if fill != Color32::TRANSPARENT {
        for i in 1..count - 1 {
            mesh.add_triangle(3, i * 4 + 3, (i + 1) * 4 + 3);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// The WebView SVG translates one horizontal gradient to the right across
/// the entire frame. Sample it in window coordinates on every side and arc.
pub(super) fn dialog_frame(
    p: &Painter, rect: Rect, radius: f32, fill: Color32, width: f32, phase: f32,
) {
    window_frame(p, rect, radius, fill, width, |point| {
        let [r, g, b] = crate::dock::wallpaper_stroke_rgb(point.x - rect.left() - phase);
        Color32::from_rgb(r, g, b)
    });
}
fn cross(p: &Painter, at: Pos2, size: f32, width: f32, color: Color32) {
    for sign in [-1., 1.] {
        let shift = vec2(size * 0.5, sign * size * 0.5);
        p.line_segment([at - shift, at + shift], Stroke::new(width, color));
    }
}
fn chevron(p: &Painter, center: Pos2, dir: f32, size: f32, color: Color32) {
    p.add(egui::Shape::line(
        vec![
            center + vec2(-dir * size * 0.30, -size / 2.),
            center + vec2(dir * size * 0.30, 0.),
            center + vec2(-dir * size * 0.30, size / 2.),
        ],
        Stroke::new(1.6, color),
    ));
}
fn click(ui: &Ui, rect: Rect, id: impl std::hash::Hash) -> Response {
    ui.interact(rect, ui.id().with(id), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn close_x(ui: &Ui, rect: Rect) -> bool {
    let r = click(ui, rect, "close").on_hover_text("Cerrar ventana");
    if r.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(4.), 7., rgba(225, 37, 90, 82));
    }
    cross(
        ui.painter(),
        rect.center(),
        11.,
        1.6,
        if r.hovered() { INK } else { MUTED },
    );
    r.clicked()
}
fn button(ui: &Ui, rect: Rect, caption: &str, accent: bool) -> bool {
    let r = click(ui, rect, caption);
    let hot = r.hovered();
    let p = ui.painter();
    let fill = if accent {
        if hot {
            Color32::from_rgb(96, 43, 255)
        } else {
            PURPLE
        }
    } else if hot {
        Color32::from_rgb(41, 30, 64)
    } else {
        FIELD
    };
    if accent {
        for i in (1..=4).rev() {
            p.rect_filled(
                rect.expand(i as f32 * 1.8).translate(vec2(0., 2.)),
                12.,
                rgba(78, 22, 246, 11),
            );
        }
    }
    outline(
        p,
        rect,
        if accent { 12. } else { 8. },
        fill,
        if accent {
            Color32::TRANSPARENT
        } else {
            FIELD_EDGE
        },
    );
    label(
        p,
        rect.center(),
        Align2::CENTER_CENTER,
        caption,
        14.,
        true,
        INK,
    );
    r.clicked()
}
fn edit(
    ui: &mut Ui,
    area: Rect,
    id: &'static str,
    value: &mut String,
    hint: &str,
    limit: usize,
    align: Align,
) -> Response {
    let empty = value.is_empty();
    let edit = egui::TextEdit::singleline(value)
        .id_salt(id)
        .char_limit(limit)
        .font(font(14., false))
        .text_color(INK)
        .horizontal_align(align)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::symmetric(2, 0))
        .desired_width(area.width())
        .min_size(area.size());
    let response = ui.place(area, edit);
    if empty {
        let p = ui.painter_at(area);
        let (at, alignment) = if align == Align::Center {
            (area.center(), Align2::CENTER_CENTER)
        } else {
            (area.left_center() + vec2(2., 0.), Align2::LEFT_CENTER)
        };
        label(&p, at, alignment, hint, 14., false, PLACEHOLDER);
    }
    response
}
fn skin_input_frame(ui: &Ui, area: Rect, border: Color32) {
    outline(ui.painter(), area, 8., FIELD, border);
}
fn inside<R>(
    ui: &mut Ui,
    area: Rect,
    id: impl std::hash::Hash,
    render: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.scope_builder(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(area)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(area));
            ui.set_max_width(area.width());
            render(ui)
        },
    )
    .inner
}

impl Profile {
    pub(super) fn paint_dock(&self, ui: &Ui, window: &Window) {
        let ([x, y, w, h], s) = crate::dock::profile_rect(window);
        let rect = Rect::from_min_size(pos2(x, y), vec2(w, h));
        let p = ui.painter_at(rect);
        let at = |x, y| rect.min + vec2(x, y) * s;
        // Exact silhouette of the WebView2 profile clip. The tray beneath has no card baked in.
        let mut points = vec![at(2., 50.), at(2., 30.)];
        quad(&mut points, at(2., 30.), at(2., 2.), at(30., 2.));
        points.extend([at(274., 2.), at(298., 26.), at(230., 94.), at(30., 94.)]);
        quad(&mut points, at(30., 94.), at(2., 94.), at(2., 66.));
        let art = self.art.as_ref().unwrap();
        let (tex, crop) = self
            .image
            .as_ref()
            .map_or((&art.cover, Crop::default()), |i| (i, self.data.crop));
        photo(
            &p,
            tex.id(),
            rect,
            cover_uv(rect.size(), tex.size_vec2(), crop),
            &points,
            Color32::from_gray(105),
        );
        let mut avatar = Rect::from_min_size(at(19., 19.), Vec2::splat(58. * s));
        if let Some(t) = &self.avatar {
            if self.avatar_is_skin {
                avatar = pixel_head_rect(avatar, p.ctx().pixels_per_point());
            }
            head(&p, t, avatar, 15. * s);
        }
        p.circle_filled(
            avatar.right_bottom() - vec2(3., 3.) * s,
            7. * s,
            Color32::from_rgb(7, 18, 29),
        );
        p.circle_filled(
            avatar.right_bottom() - vec2(3., 3.) * s,
            5. * s,
            Color32::from_rgb(84, 230, 160),
        );
        if let Some(a) = self
            .data
            .accounts
            .iter()
            .find(|a| a.username == self.data.active)
        {
            let names = p.with_clip_rect(rect.with_max_x(at(242., 0.).x));
            label(
                &names,
                at(88., 37.),
                Align2::LEFT_CENTER,
                a.label(),
                19. * s,
                true,
                Color32::WHITE,
            );
            label(
                &names,
                at(88., 60.),
                Align2::LEFT_CENTER,
                &format!("@{}", a.username),
                13.5 * s,
                false,
                MUTED,
            );
        }
    }

    pub(super) fn paint_dialog(
        &mut self,
        ctx: &egui::Context,
        ui: &Ui,
        window: &Window,
        gpu: &PaintGpu,
        panel: Panel,
        dim: f32,
        border_phase: f32,
    ) -> Actions {
        let bounds = ui.max_rect();
        let friends = panel == Panel::Friends;
        let area = if friends {
            Rect::from_center_size(
                bounds.center() - vec2(0., 25.5),
                vec2(
                    (bounds.width() - 20.).clamp(1., 880.),
                    (bounds.height() - 165.).clamp(140., 532.),
                ),
            )
        } else {
            preferences_rect(bounds, self.data.preferences_geometry)
        };
        self.dialog.surface = Some(area);
        // Preferences is a floating panel. Friends keeps its own backdrop.
        if friends {
            let bottom = bounds.bottom()
                - crate::dock::reserved_height(window) / window.scale_factor() as f32;
            let veil = Rect::from_min_max(
                bounds.min + vec2(0., 52.),
                pos2(bounds.right(), bottom.max(bounds.top() + 52.)),
            );
            ui.painter()
                .rect_filled(veil, 0., Color32::from_black_alpha(199));
        }
        let mut cmd = Actions::default();
        if ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| !area.contains(p))
        }) {
            cmd.close = true;
            if friends {
                return cmd;
            }
        }

        egui::Area::new(if friends {
            egui::Id::new("ika-friends-window")
        } else {
            preferences_layer().id
        })
        .order(egui::Order::Foreground)
        .fixed_pos(area.min)
        .default_size(area.size())
        .movable(false)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_min_size(area.size());
            ui.spacing_mut().item_spacing = vec2(6., 8.);
            ui.visuals_mut().override_text_color = Some(INK);
            let r = if friends { 15. } else { 21. };
            shadow(ui.painter(), area, r, 19);
            dialog_frame(ui.painter(), area, r, SURFACE, if friends { 1.6 } else { 1.8 }, border_phase);
            if friends {
                self.friends_ui(ui, area, &mut cmd);
            } else {
                self.settings_ui(ui, area, gpu, dim, &mut cmd);
                self.resize_preferences(ui, area, bounds, &mut cmd);
            }
        });
        cmd
    }
}

pub(super) fn preferences_layer() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, egui::Id::new("ika-settings-window"))
}

fn preferences_rect(bounds: Rect, geometry: storage::PreferencesGeometry) -> Rect {
    let max = (bounds.size() - Vec2::splat(12.)).max(Vec2::splat(1.));
    let height = geometry
        .height
        .unwrap_or(720_f32.min(bounds.height() * 0.9))
        .clamp(480_f32.min(max.y), max.y);
    let size = vec2((bounds.width() * 0.94).min(640.).min(max.x), height);
    let centered = bounds.center() - size * 0.5;
    let origin = geometry.origin.map_or(centered, |[x, y]| pos2(x, y));
    let limit = bounds.min + Vec2::splat(6.);
    let far = (bounds.max - size - Vec2::splat(6.)).max(limit);
    Rect::from_min_size(origin.clamp(limit, far), size)
}

impl Profile {
    fn resize_preferences(&mut self, ui: &Ui, area: Rect, bounds: Rect, cmd: &mut Actions) {
        // Resize from either horizontal edge, keeping the opposite edge anchored.
        for top in [true, false] {
            let y = if top { area.top() } else { area.bottom() - 8. };
            let grip = rect(pos2(area.left(), y), 0., 0., area.width(), 8.);
            let response = ui
                .interact(
                    grip,
                    ui.id().with(("resize-preferences", top)),
                    Sense::drag(),
                )
                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeVertical);
            if response.dragged() {
                let d = response.drag_delta().y;
                let min_h = 480_f32.min(area.height());
                let (t, b) = if top {
                    (
                        (area.top() + d).clamp(bounds.top() + 6., area.bottom() - min_h),
                        area.bottom(),
                    )
                } else {
                    (
                        area.top(),
                        (area.bottom() + d).clamp(
                            area.top() + min_h,
                            (bounds.bottom() - 6.).max(area.top() + min_h),
                        ),
                    )
                };
                self.data.preferences_geometry.height = Some(b - t);
                self.data.preferences_geometry.origin = Some([area.left(), t]);
                self.dirty = true;
            }
            cmd.save_geometry |= response.drag_stopped();
        }
    }
}

const NAV: [(&str, &str, &str); 6] = [
    (
        "Cuenta",
        "Gestión de usuario, sesión activa, apodo y frase de estado",
        "nombre handle avatar skin perfil portada encuadre",
    ),
    (
        "Perfiles de Instancias",
        "Directorio de Minecraft (.minecraft), carpetas y arranque",
        "ram java memoria directorio ruta inicio recursos carpetas",
    ),
    (
        "Aspecto Visual",
        "Intensidad de neón cyber, wallpapers, reloj pixel",
        "fondo fondos transparencia blur desenfoque color temas apariencia",
    ),
    (
        "Sistema",
        "Inicio automático con Windows",
        "autostart inicio arrancar ordenador",
    ),
    (
        "Idioma",
        "Idioma de la interfaz del lanzador (Español / English)",
        "language español inglés",
    ),
    (
        "Acerca de Ika Launcher",
        "La historia detrás de Ika",
        "laika créditos version acerca",
    ),
];

fn normal(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .collect()
}
#[derive(Clone, Copy)]
struct SettingResult {
    page: usize,
    title: &'static str,
    key: &'static str,
    aliases: &'static str,
}
const RESULTS: &[SettingResult] = &[
    SettingResult {
        page: 0,
        title: "Nombre para Mostrar (Display Name)",
        key: "display-name",
        aliases: "nombre apodo perfil",
    },
    SettingResult {
        page: 0,
        title: "Nombre de Usuario / Handle",
        key: "handle",
        aliases: "usuario username cuenta identificador",
    },
    SettingResult {
        page: 0,
        title: "Frase de Estado",
        key: "status-quote",
        aliases: "mensaje bocadillo perfil",
    },
    SettingResult {
        page: 0,
        title: "Cambiar avatar",
        key: "avatar",
        aliases: "foto imagen perfil skin",
    },
    SettingResult {
        page: 2,
        title: "Cambiar portada",
        key: "cover",
        aliases: "imagen vídeo video perfil cambiar wallpaper",
    },
    SettingResult {
        page: 2,
        title: "Ajustar encuadre",
        key: "crop",
        aliases: "portada recortar zoom imagen video",
    },
    SettingResult {
        page: 1,
        title: "Carpeta raíz de Instancias / Minecraft",
        key: "instance-folder",
        aliases: "instalaciones directorio ruta carpetas recursos perfiles",
    },
    SettingResult {
        page: 1,
        title: "Al iniciar el juego",
        key: "launch-window-action",
        aliases: "arranque ocultar ventana minecraft",
    },
    SettingResult {
        page: 2,
        title: "Wallpapers",
        key: "wallpapers",
        aliases: "catálogo biblioteca imágenes fondos",
    },
    SettingResult {
        page: 2,
        title: "Oscurecer el fondo",
        key: "dim",
        aliases: "brillo oscuridad intensidad wallpaper",
    },
    SettingResult {
        page: 3,
        title: "Abrir Ika automáticamente al arrancar el ordenador",
        key: "autostart",
        aliases: "windows sistema inicio arranque autostart",
    },
    SettingResult {
        page: 3,
        title: "Oneko · Mascota de escritorio",
        key: "oneko",
        aliases: "oneko gato gatito pet mascota kyrie25 cursor",
    },
    SettingResult {
        page: 4,
        title: "Idioma de la interfaz",
        key: "language",
        aliases: "idioma español inglés language english traducción",
    },
    SettingResult {
        page: 5,
        title: "Acerca de Ika Launcher",
        key: "about",
        aliases: "laika créditos historia",
    },
];
fn match_rank(query: &str, item: &SettingResult) -> Option<u16> {
    let words = |s: &str| {
        normal(s)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let title = words(item.title);
    let alias = words(item.aliases);
    let mut rank = 0;
    for part in words(query) {
        let score = title
            .iter()
            .chain(&alias)
            .enumerate()
            .filter_map(|(idx, word)| {
                let exact = word == &part;
                let prefix = word.starts_with(&part);
                let typo = part.len() >= 4 && one_edit_apart(word.as_bytes(), part.as_bytes());
                let penalty = u16::from(idx >= title.len());
                if exact {
                    Some(penalty)
                } else if prefix {
                    Some(2 + penalty)
                } else if typo {
                    Some(4 + penalty)
                } else {
                    None
                }
            })
            .min()?;
        rank += score;
    }
    Some(rank)
}
fn one_edit_apart(a: &[u8], b: &[u8]) -> bool {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    if long.len() - short.len() > 1 {
        return false;
    }
    let at = long
        .iter()
        .zip(short)
        .position(|(a, b)| a != b)
        .unwrap_or(short.len());
    if at == short.len() {
        return true;
    }
    if long.len() != short.len() {
        long[at + 1..] == short[at..]
    } else if long[at + 1..] == short[at + 1..] {
        true
    } else {
        at + 1 < short.len()
            && long[at] == short[at + 1]
            && long[at + 1] == short[at]
            && long[at + 2..] == short[at + 2..]
    }
}
fn results(query: &str) -> Vec<&'static SettingResult> {
    if !query.chars().any(char::is_alphanumeric) {
        return Vec::new();
    }
    let mut matches = RESULTS
        .iter()
        .filter_map(|item| match_rank(query, item).map(|rank| (rank, item)))
        .collect::<Vec<_>>();
    matches.sort_by_key(|(rank, _)| *rank);
    matches.into_iter().map(|(_, item)| item).collect()
}

fn pill(p: &Painter, area: Rect, title: &str, friends: bool) {
    let r = if friends { 8. } else { 12. };
    outline(p, area, r, rgba(2, 67, 77, 155), rgba(0, 185, 191, 115));
    if friends {
        ring(p, area, r, 1.2, |xy| {
            let t = (xy.x - area.left()) / area.width();
            mix(
                Color32::from_rgb(7, 56, 67),
                Color32::from_rgb(0, 235, 226),
                t,
            )
        });
    } else {
        p.circle_filled(pos2(area.left() + 13., area.center().y), 3., CYAN);
    }
    label(
        p,
        area.center() + vec2(if friends { 0. } else { 6. }, -0.5),
        Align2::CENTER_CENTER,
        title,
        if friends { 14. } else { 11.5 },
        true,
        CYAN,
    );
}

impl Profile {
    fn settings_ui(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        gpu: &PaintGpu,
        dim: f32,
        cmd: &mut Actions,
    ) {
        if self.dialog.page == Some(5) {
            self.laika_ui(ui, area, cmd);
            return;
        }
        let art = self.art.as_ref().unwrap();
        let w = area.width();
        let origin = area.min;
        let shrink = (area.height() / 480.).clamp(0.36, 1.);
        let cover_h = 260. * shrink;
        let cover = rect(origin, 1., 1., w - 2., cover_h - 1.);
        let media = self
            .image
            .as_ref()
            .map(|t| (t.id(), t.size_vec2()))
            .or_else(|| {
                gpu.cover
                    .as_ref()
                    .map(|c| (c.id, vec2(c.width as f32, c.height as f32)))
            });
        let crop = self.crop_edit.unwrap_or(self.data.crop);
        let vp = rounded(cover, 20., 20., 0., 0.);
        let uv = if let Some((id, size)) = media {
            let uv = cover_uv(cover.size(), size, crop);
            photo(ui.painter(), id, cover, uv, &vp, Color32::WHITE);
            uv
        } else {
            ui.painter().add(egui::Shape::convex_polygon(
                vp.clone(),
                Color32::from_rgb(12, 18, 38),
                Stroke::NONE,
            ));
            let tiling = cover.size() / 220.;
            let uv = Rect::from_center_size(pos2(0.5, 0.5), tiling);
            photo(
                ui.painter(),
                art.doodle.id(),
                cover,
                uv,
                &vp,
                Color32::WHITE,
            );
            FULL_UV
        };
        ui.painter().hline(
            cover.x_range(),
            cover.bottom(),
            Stroke::new(1., rgba(110, 101, 247, 90)),
        );
        let hovered = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| cover.contains(p)));

        if let Some(crop) = &mut self.crop_edit {
            let drag = ui
                .interact(cover, ui.id().with("crop-cover"), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Move);
            let d = drag.drag_delta();
            let overflow = cover.size() * ((Vec2::splat(1.) / uv.size()) - Vec2::splat(1.));
            if overflow.x > 0. && d.x != 0. {
                crop.pos_x = (crop.pos_x - d.x * 100. / overflow.x).clamp(0., 100.);
                self.dirty = true;
            }
            if overflow.y > 0. && d.y != 0. {
                crop.pos_y = (crop.pos_y - d.y * 100. / overflow.y).clamp(0., 100.);
                self.dirty = true;
            }
            if hovered {
                let y = ui.input(|i| i.smooth_scroll_delta.y);
                if y != 0. {
                    crop.zoom = (crop.zoom * (y * 0.0016).exp()).clamp(1., 12.);
                    self.dirty = true;
                }
            }
        } else {
            let drag = ui
                .interact(cover, ui.id().with("settings-grip"), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            if drag.dragged() {
                let bounds = ui.ctx().content_rect();
                let lower = bounds.min + Vec2::splat(6.);
                let upper = (bounds.max - area.size() - Vec2::splat(6.)).max(lower);
                let position = (area.min + drag.drag_delta()).clamp(lower, upper);
                self.data.preferences_geometry.origin = Some([position.x, position.y]);
                self.dirty = true;
            }
            cmd.save_geometry |=
                drag.drag_stopped() && self.data.preferences_geometry.origin.is_some();
            ui.painter().rect_filled(
                rect(origin, w * 0.5 - 23., 10., 46., 4.),
                3.,
                rgba(218, 228, 255, 76),
            );
        }
        pill(
            ui.painter(),
            rect(origin, 20., 20., 126., 25.),
            "PREFERENCIAS",
            false,
        );
        cmd.close |= close_x(ui, rect(origin, w - 55., 14., 34., 36.));
        if hovered || self.cover_importing {
            let replace = rect(origin, w - 162., cover_h - 44., 145., 30.);
            let camera = rect(replace.min, 7., 6., 18., 18.);
            let enabled = self.loaded && !self.cover_importing;
            if self.crop_edit.is_none() {
                cmd.cover |= button(
                    ui,
                    replace,
                    if self.cover_importing {
                        "Cargando…"
                    } else {
                        "      Cambiar portada"
                    },
                    false,
                ) && enabled;
                icon(ui.painter(), art, 7, camera.center(), 16., CYAN);
                if media.is_some() {
                    cmd.crop |= button(
                        ui,
                        rect(origin, w - 310., cover_h - 44., 140., 30.),
                        "Ajustar encuadre",
                        false,
                    );
                }
            }
        }

        let dia = 104. * shrink.max(0.62);
        let avatar =
            Rect::from_center_size(pos2(area.center().x, cover.bottom()), Vec2::splat(dia));
        for i in (1..=9).rev() {
            ui.painter().rect_filled(
                avatar.expand(i as f32 * 2.5),
                21. + i as f32 * 2.5,
                rgba(0, 188, 255, 8),
            );
        }
        ui.painter()
            .rect_filled(avatar, 20., Color32::from_rgb(5, 15, 29));
        if let Some(tex) = &self.avatar {
            let draw = if self.avatar_is_skin { pixel_head } else { head };
            draw(ui.painter(), tex, avatar.shrink(3.), 17.);
        } else {
            icon(ui.painter(), art, 8, avatar.center(), dia * 0.43, MUTED);
        }
        ring(ui.painter(), avatar, 20., 3.5, |p| {
            let amount =
                (((p.x - avatar.left()) + (p.y - avatar.top())) / (dia * 2.)).clamp(0., 1.);
            if amount < 0.57 {
                mix(CYAN, Color32::from_rgb(38, 183, 255), amount / 0.57)
            } else {
                mix(
                    Color32::from_rgb(38, 183, 255),
                    Color32::from_rgb(166, 64, 255),
                    (amount - 0.57) / 0.43,
                )
            }
        });
        let account = self
            .data
            .accounts
            .iter()
            .find(|a| a.username == self.data.active);
        let avatar_response = click(ui, avatar, "avatar-cover").on_hover_text("Cambiar avatar");
        if account.is_some() && avatar_response.hovered() {
            ui.painter()
                .rect_filled(avatar.shrink(3.), 17., Color32::from_black_alpha(125));
            icon(ui.painter(), art, 7, avatar.center(), 26., INK);
        }
        if account.is_some() && avatar_response.clicked() {
            self.dialog.avatar_menu = !self.dialog.avatar_menu;
        }
        let display = account.map_or(
            if self.loaded {
                "Invitado"
            } else {
                "Cargando…"
            },
            |a| a.label(),
        );
        let name_y = avatar.bottom() + (27. * shrink).max(14.);
        let handle_y = name_y + (24. * shrink).max(16.);
        label(
            ui.painter(),
            pos2(area.center().x, name_y),
            Align2::CENTER_CENTER,
            display,
            21.,
            true,
            INK,
        );
        if let Some(a) = account {
            label(
                ui.painter(),
                pos2(area.center().x, handle_y),
                Align2::CENTER_CENTER,
                &format!("@{}", a.username),
                13.5,
                true,
                Color32::from_rgb(37, 203, 255),
            );
        }

        let search_y = (handle_y - origin.y + 22. * shrink).max(cover_h + 86. * shrink);
        let nav_h = 39.;
        let nav_w = (w - if self.dialog.page.is_some() { 28. } else { 32. }).min(480.);
        let in_detail = self.dialog.page.is_some();
        let search_w = if in_detail {
            (w * 0.43).min(270.)
        } else {
            nav_w
        };
        let search = rect(
            origin,
            if in_detail {
                w - 16. - search_w
            } else {
                (w - search_w) * 0.5
            },
            search_y,
            search_w,
            nav_h,
        );
        if let Some(page) = self.dialog.page {
            let back = rect(origin, 14., search_y + 1., 102., 36.);
            if click(ui, back, "settings-back")
                .on_hover_text("Volver")
                .clicked()
            {
                self.dialog.page = None;
                self.dialog.result_focus = None;
                self.dialog.search.clear();
                self.dialog.account_add = false;
                self.dirty = true;
            }
            let breadcrumb = ui.painter().with_clip_rect(rect(
                origin,
                14.,
                search_y,
                search.left() - origin.x - 24.,
                nav_h,
            ));
            chevron(
                &breadcrumb,
                back.left_center() + vec2(17., 0.),
                -1.,
                11.,
                CYAN,
            );
            label(
                &breadcrumb,
                pos2(back.left() + 36., back.center().y),
                Align2::LEFT_CENTER,
                &format!("Ajustes   /   {}", NAV[page].0),
                14.,
                true,
                INK,
            );
        }
        for i in [7., 4., 2.] {
            ui.painter()
                .rect_filled(search.expand(i), 10. + i, rgba(0, 211, 226, 4));
        }
        outline(
            ui.painter(),
            search,
            10.,
            Color32::from_rgb(12, 18, 32),
            Color32::from_rgb(3, 96, 106),
        );
        icon(
            ui.painter(),
            art,
            6,
            pos2(search.left() + 20., search.center().y),
            15.,
            CYAN,
        );
        let field = rect(search.min, 36., 9., search.width() - 67., 23.);
        let query = edit(
            ui,
            field,
            "settings-query",
            &mut self.dialog.search,
            "Buscar en ajustes (RAM, Java, idioma, neón...)",
            100,
            Align::Min,
        );
        if query.changed() {
            self.dialog.result_position = 0;
            self.dirty = true;
        }
        let clear = rect(search.right_top(), -36., 4., 28., 31.);
        if click(ui, clear, "clear-settings-search")
            .on_hover_text("Limpiar búsqueda")
            .clicked()
        {
            self.dialog.search.clear();
            query.request_focus();
            self.dirty = true;
        }
        cross(ui.painter(), clear.center(), 8., 1., PLACEHOLDER);

        let body_y = search.bottom() + 18.;
        let body = Rect::from_min_max(
            pos2(area.left() + 14., body_y.min(area.bottom() - 12.)),
            area.right_bottom() - vec2(14., 2.),
        );
        let submit = query.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if query.has_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.dialog.result_position = self.dialog.result_position.saturating_add(1);
                self.dirty = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.dialog.result_position = self.dialog.result_position.saturating_sub(1);
                self.dirty = true;
            }
        }
        let query = self.dialog.search.trim().to_owned();
        let page = self.dialog.page;
        inside(ui, body, "settings-body", |ui| {
            egui::ScrollArea::vertical()
                .id_salt((
                    self.dialog.generation,
                    page,
                    !query.is_empty(),
                    self.crop_edit.is_some(),
                ))
                .auto_shrink([false, false])
                .max_height(body.height())
                .show(ui, |ui| {
                    ui.set_max_width(body.width());
                    ui.spacing_mut().item_spacing.y = 4.;
                    ui.add_space(4.);
                    if self.crop_edit.is_some() {
                        self.crop_controls(ui, cmd);
                    } else if query.is_empty() && page.is_some() {
                        self.settings_detail(ui, page.unwrap(), dim, cmd);
                    } else if !query.is_empty() {
                        self.search_ui(ui, &query, submit);
                    } else {
                        for (index, (title, desc, _)) in NAV.iter().enumerate() {
                            let (row, response) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 68.),
                                Sense::click(),
                            );
                            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if response.hovered() || response.has_focus() {
                                outline(
                                    ui.painter(),
                                    row,
                                    12.,
                                    Color32::from_rgb(16, 17, 38),
                                    rgba(109, 113, 218, 56),
                                );
                            }
                            let badge = rect(row.min, 14., 13., 42., 42.);
                            outline(ui.painter(), badge, 12., ROW, Color32::from_rgb(35, 35, 54));
                            icon(
                                ui.painter(),
                                self.art.as_ref().unwrap(),
                                index,
                                badge.center(),
                                20.,
                                MUTED,
                            );
                            let content = ui.painter().with_clip_rect(Rect::from_min_max(
                                row.min + vec2(70., 0.),
                                row.max - vec2(32., 0.),
                            ));
                            let at = row.min + vec2(70., 24.);
                            label(&content, at, Align2::LEFT_CENTER, title, 14.5, true, INK);
                            label(
                                &content,
                                at + vec2(0., 18.),
                                Align2::LEFT_CENTER,
                                desc,
                                12.5,
                                false,
                                MUTED,
                            );
                            if index == 4 && row.width() > 380. {
                                let chip = rect(row.right_top(), -107., 22., 78., 24.);
                                outline(ui.painter(), chip, 7., FIELD, FIELD_EDGE);
                                label(
                                    ui.painter(),
                                    chip.center(),
                                    Align2::CENTER_CENTER,
                                    "Español",
                                    11.,
                                    true,
                                    INK,
                                );
                            }
                            chevron(
                                ui.painter(),
                                pos2(row.right() - 19., row.center().y),
                                1.,
                                8.,
                                PLACEHOLDER,
                            );
                            if response.clicked() {
                                self.dialog.page = Some(index);
                                self.dialog.result_focus = None;
                                self.dirty = true;
                            }
                        }
                    }
                    if let Some(e) = self.profile_error.as_ref().or(self.data.error.as_ref()) {
                        ui.label(text(e, 13., false, Color32::LIGHT_RED));
                    }
                });
        });
        if self.dialog.avatar_menu {
            let menu = rect(
                origin,
                (w * 0.5 + dia * 0.5 - 8.).min(w - 182.),
                cover_h + 10.,
                164.,
                78.,
            );
            shadow(ui.painter(), menu, 10., 12);
            outline(ui.painter(), menu, 10., SURFACE, FIELD_EDGE);
            cmd.avatar |= button(
                ui,
                rect(menu.min, 6., 5., 152., 31.),
                "Cambiar avatar",
                false,
            );
            cmd.avatar_clear |= button(
                ui,
                rect(menu.min, 6., 41., 152., 31.),
                "Restaurar avatar",
                false,
            );
            if cmd.avatar || cmd.avatar_clear {
                self.dialog.avatar_menu = false;
                self.dirty = true;
            }
        }
    }

    fn search_ui(&mut self, ui: &mut Ui, query: &str, submit: bool) {
        let found = results(query);
        if found.is_empty() {
            ui.label(text(
                "No encontramos ajustes para esta búsqueda.",
                14.,
                false,
                MUTED,
            ));
            return;
        }
        self.dialog.result_position = self.dialog.result_position.min(found.len() - 1);
        let active = self.dialog.result_position;
        let mut picked = submit.then(|| found[active]);
        for (idx, item) in found.into_iter().enumerate() {
            let (area, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 51.), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() || idx == active {
                outline(ui.painter(), area, 9., ROW, Color32::from_rgb(40, 38, 77));
            }
            let p = ui.painter_at(area.shrink2(vec2(8., 0.)));
            label(
                &p,
                area.min + vec2(13., 15.),
                Align2::LEFT_CENTER,
                item.title,
                14.,
                true,
                INK,
            );
            label(
                &p,
                area.min + vec2(13., 34.),
                Align2::LEFT_CENTER,
                NAV[item.page].0,
                12.,
                false,
                MUTED,
            );
            chevron(&p, area.right_center() - vec2(12., 0.), 1., 8., MUTED);
            if response.clicked() {
                picked = Some(item);
            }
            if idx == active
                && ui.input(|i| {
                    i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
                })
            {
                response.scroll_to_me(None);
            }
        }
        if let Some(item) = picked {
            self.dialog.page = Some(item.page);
            self.dialog.search.clear();
            self.dialog.result_focus = Some(item.key);
            if item.key == "avatar" {
                self.dialog.avatar_menu = true;
            }
            self.dirty = true;
        }
    }

    fn crop_controls(&mut self, ui: &mut Ui, cmd: &mut Actions) {
        let Some(crop) = &mut self.crop_edit else {
            return;
        };
        ui.label(text("Ajustar encuadre", 15., true, INK));
        ui.label(text(
            "Arrastra la portada para moverla. Usa la rueda para acercar o alejar.",
            13.,
            false,
            MUTED,
        ));
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::Slider::new(&mut crop.pos_x, 0. ..=100.).text("Horizontal"));
            ui.add(egui::Slider::new(&mut crop.pos_y, 0. ..=100.).text("Vertical"));
            ui.add(
                egui::Slider::new(&mut crop.zoom, 1. ..=12.)
                    .text("Zoom")
                    .logarithmic(true),
            );
        });
        if matches!(self.data.cover, Some(Cover::Video(_))) {
            ui.horizontal(|ui| {
                ui.label("Inicio (s)");
                ui.add(
                    egui::DragValue::new(&mut crop.start_time)
                        .range(0. ..=86400.)
                        .speed(0.1),
                );
                ui.label("Duración (s)");
                ui.add(
                    egui::DragValue::new(&mut crop.duration)
                        .range(0.1..=86400.)
                        .speed(0.1),
                );
            });
        }
        ui.horizontal(|ui| {
            let r = ui.allocate_space(vec2(156., 35.)).1;
            cmd.crop_done |= button(ui, r, "Guardar encuadre", true);
            let r = ui.allocate_space(vec2(91., 35.)).1;
            cmd.crop_cancel |= button(ui, r, "Cancelar", false);
        });
    }

    fn settings_detail(&mut self, ui: &mut Ui, page: usize, dim: f32, cmd: &mut Actions) {
        ui.spacing_mut().item_spacing.y = 8.;
        match page {
            0 => {
                if !self.loaded {
                    ui.spinner();
                    return;
                }
                if let Some(a) = self
                    .data
                    .accounts
                    .iter()
                    .find(|a| a.username == self.data.active)
                {
                    let edit = self.identity_edit.get_or_insert_with(|| storage::Identity {
                        display_name: a.display_name.clone(),
                        status_quote: a.status_quote.clone(),
                    });
                    setting_anchor(
                        ui,
                        &mut self.dialog.result_focus,
                        "display-name",
                        "Nombre para Mostrar (Display Name)",
                        13.,
                    );
                    let area = ui.allocate_space(vec2(ui.available_width(), 36.)).1;
                    skin_input_frame(ui, area, FIELD_EDGE);
                    let name = edit_field_in(
                        ui,
                        area,
                        "display-name",
                        &mut edit.display_name,
                        "Apodo",
                        35,
                    );
                    setting_anchor(
                        ui,
                        &mut self.dialog.result_focus,
                        "handle",
                        "Nombre de Usuario / Handle",
                        13.,
                    );
                    let area = ui.allocate_space(vec2(ui.available_width(), 36.)).1;
                    skin_input_frame(ui, area, FIELD_EDGE);
                    label(
                        ui.painter(),
                        area.left_center() + vec2(12., 0.),
                        Align2::LEFT_CENTER,
                        &a.username,
                        14.,
                        false,
                        INK,
                    );
                    ui.label(text(
                        "Tu identificador en Minecraft y Amigos.",
                        12.,
                        false,
                        MUTED,
                    ));
                    setting_anchor(
                        ui,
                        &mut self.dialog.result_focus,
                        "status-quote",
                        "Frase de Estado (Bocadillo de perfil)",
                        13.,
                    );
                    let area = ui.allocate_space(vec2(ui.available_width(), 62.)).1;
                    skin_input_frame(ui, area, FIELD_EDGE);
                    let quote = ui.place(
                        area.shrink(6.),
                        egui::TextEdit::multiline(&mut edit.status_quote)
                            .id_salt("status-quote")
                            .hint_text("Escribe tu frase de estado…")
                            .font(font(14., false))
                            .frame(egui::Frame::NONE)
                            .char_limit(120)
                            .desired_rows(2)
                            .desired_width(area.width() - 12.),
                    );
                    let changed = (edit.display_name != a.display_name)
                        || (edit.status_quote != a.status_quote);
                    if changed {
                        let save = ui.allocate_space(vec2(140., 34.)).1;
                        cmd.identity_save |= button(ui, save, "Guardar perfil", true)
                            || (name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    } else if quote.changed() || name.changed() {
                        self.dirty = true;
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    let r = ui.allocate_space(vec2(156., 34.)).1;
                    if button(ui, r, "+ Agregar otra cuenta", false) {
                        self.dialog.account_add = !self.dialog.account_add;
                        self.dirty = true;
                    }
                    if !self.data.active.is_empty() {
                        let r = ui.allocate_space(vec2(121., 34.)).1;
                        if button(ui, r, "Cerrar sesión", false) {
                            self.remove_confirm = !self.remove_confirm;
                            self.dirty = true;
                        }
                    }
                });
                if self.remove_confirm && !self.data.active.is_empty() {
                    ui.label(text(
                        "¿Cerrar la sesión de esta cuenta en el launcher?",
                        13.,
                        false,
                        MUTED,
                    ));
                    let r = ui.allocate_space(vec2(128., 34.)).1;
                    cmd.sign_out |= button(ui, r, "Cerrar sesión", true);
                }
                if self.dialog.account_add {
                    ui.horizontal(|ui| {
                        let area = ui
                            .allocate_space(vec2((ui.available_width() - 112.).max(92.), 36.))
                            .1;
                        skin_input_frame(ui, area, FIELD_EDGE);
                        let input = edit_field_in(
                            ui,
                            area,
                            "new-account",
                            &mut self.new_account,
                            "Nombre de Minecraft",
                            16,
                        );
                        let add = ui.allocate_space(vec2(100., 36.)).1;
                        cmd.add |= button(ui, add, "Agregar", true)
                            || (input.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    });
                }
                if self.data.accounts.len() > 1 || self.data.active.is_empty() {
                    for a in &self.data.accounts {
                        if a.username == self.data.active {
                            continue;
                        }
                        let r = ui.allocate_space(vec2(ui.available_width(), 36.)).1;
                        if button(ui, r, &format!("{}  @{}", a.label(), a.username), false) {
                            cmd.select_account = Some(a.username.clone());
                        }
                    }
                }
            }
            1 => self.instances_ui(ui, cmd),
            2 => {
                setting_anchor(
                    ui,
                    &mut self.dialog.result_focus,
                    "wallpapers",
                    "Wallpapers",
                    14.,
                );
                ui.label(text(
                    "Escoge el fondo que quieres usar en el launcher.",
                    13.,
                    false,
                    MUTED,
                ));
                let r = ui.allocate_space(vec2(168., 36.)).1;
                if button(ui, r, "Abrir Wallpapers", true) {
                    cmd.app = Some(AppAction::Wallpapers);
                }
                let mut percent = (dim * 100.).round() as u8;
                setting_anchor(
                    ui,
                    &mut self.dialog.result_focus,
                    "dim",
                    "Oscurecer el fondo",
                    14.,
                );
                if ui
                    .add(egui::Slider::new(&mut percent, 0..=95).suffix("%"))
                    .changed()
                {
                    cmd.app = Some(AppAction::WallpaperDim(f32::from(percent) / 100.));
                }
                let r = ui.allocate_space(vec2(162., 36.)).1;
                cmd.cover |=
                    button(ui, r, "Cambiar portada", false) && self.loaded && !self.cover_importing;
                if self.data.cover.is_some() {
                    let r = ui.allocate_space(vec2(162., 36.)).1;
                    scroll_to(ui, &mut self.dialog.result_focus, "crop", r);
                    cmd.crop |= button(ui, r, "Ajustar encuadre", false);
                }
            }
            3 => self.system_ui(ui),
            4 => {
                setting_anchor(
                    ui,
                    &mut self.dialog.result_focus,
                    "language",
                    "Idioma de la interfaz",
                    14.,
                );
                ui.label(text("Español (Latinoamérica / España)", 14., false, INK));
                ui.label(text(
                    "La traducción al inglés de la interfaz nativa todavía no está disponible.",
                    13.,
                    false,
                    MUTED,
                ));
            }
            _ => {}
        }
    }

    fn laika_ui(&mut self, ui: &Ui, area: Rect, cmd: &mut Actions) {
        let art = self.art.as_ref().unwrap();
        let p = ui.painter();
        let center = area.center().x;
        let factor = (area.height() / 660.).min(1.);
        let y = |v| area.top() + v * factor;
        let side = 178. * factor;
        let portrait = Rect::from_center_size(pos2(center, y(214.)), Vec2::splat(side));
        for n in (1..=15).rev() {
            p.circle_filled(
                portrait.center(),
                (89. + n as f32 * 4.) * factor,
                rgba(93, 49, 235, 7),
            );
        }
        for i in 0..70i32 {
            let x = ((i * 3889 + 7) % 8191) as f32 / 8191.;
            let z = ((i * 5219 + 17) % 7559) as f32 / 7559.;
            p.circle_filled(
                area.min + area.size() * vec2(0.025 + 0.95 * x, 0.03 + 0.94 * z),
                0.6 + (i % 5) as f32 * 0.19,
                rgba(176, 179, 253, 70 + (i % 5) as u8 * 18),
            );
        }
        head(p, &art.laika, portrait, side * 0.5);
        let back = rect(area.min, 15., 14., 37., 36.);
        chevron(p, back.center(), -1., 12., INK);
        if click(ui, back, "laika-back")
            .on_hover_text("Volver")
            .clicked()
        {
            self.dialog.page = None;
            self.dirty = true;
        }
        cmd.close |= close_x(ui, rect(area.right_top(), -52., 14., 36., 36.));
        label(
            p,
            pos2(center, y(344.)),
            Align2::CENTER_CENTER,
            "E N  M E M O R I A  D E",
            12.,
            true,
            MUTED,
        );
        label(
            p,
            pos2(center, y(399.)),
            Align2::CENTER_CENTER,
            "Laika",
            68. * factor,
            true,
            Color32::from_rgb(208, 188, 255),
        );
        label(
            p,
            pos2(center, y(453.)),
            Align2::CENTER_CENTER,
            "Ika lleva su nombre.",
            18. * factor,
            false,
            INK,
        );
        for (offset, line) in [
            (509., "Todo este proyecto existe gracias a ella."),
            (534., "Fue la mejor compañera que pude tener."),
            (559., "Hoy es una estrella más en este cielo,"),
            (584., "y cada vez que abres Ika, sigue brillando."),
            (630., "Con amor, Faally"),
        ] {
            label(
                p,
                pos2(center, y(offset)),
                Align2::CENTER_CENTER,
                line,
                15. * factor,
                false,
                MUTED,
            );
        }
    }
}

fn edit_field_in(
    ui: &mut Ui,
    field: Rect,
    id: &'static str,
    value: &mut String,
    hint: &str,
    limit: usize,
) -> Response {
    edit(
        ui,
        Rect::from_center_size(field.center(), vec2(field.width() - 20., 23.)),
        id,
        value,
        hint,
        limit,
        Align::Min,
    )
}

impl Profile {
    fn friends_ui(&mut self, ui: &mut Ui, area: Rect, cmd: &mut Actions) {
        let art = self.art.as_ref().unwrap();
        let origin = area.min;
        let w = area.width();
        let h = area.height();
        let p = ui.painter();
        pill(p, rect(origin, 24., 16., 89., 34.), "AMIGOS", true);
        cmd.close |= close_x(ui, rect(origin, w - 48., 9., 43., 35.));

        let pad = if w > 625. { 24. } else { 12. };
        let gap = if w > 625. { 32. } else { 15. };
        let content_w = w - pad * 2. - gap;
        let left_w = content_w * 5. / 9.;
        let right_w = content_w - left_w;
        let left = rect(origin, pad, 70., left_w, h - 87.);
        let right = rect(origin, pad + left_w + gap, 70., right_w, h - 87.);
        let title = label(
            ui.painter(),
            left.min,
            Align2::LEFT_TOP,
            "Tus amigos",
            20.,
            true,
            INK,
        );
        if let Some(count) = self
            .friends
            .as_ref()
            .map(|s| s.friends.len())
            .filter(|n| *n != 0)
        {
            label(
                ui.painter(),
                pos2(title.right() + 5., title.top() + 5.),
                Align2::LEFT_TOP,
                &format!("({count})"),
                15.,
                false,
                MUTED,
            );
        }
        label(
            ui.painter(),
            right.min,
            Align2::LEFT_TOP,
            "Agregar amigo",
            20.,
            true,
            INK,
        );
        let primed = self.loaded && !self.data.active.is_empty();
        inside(
            ui,
            left.shrink2(vec2(0., 1.)).with_min_y(left.top() + 38.),
            "friends-list",
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.;
                egui::ScrollArea::vertical()
                    .id_salt(("list", self.dialog.generation))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if !self.loaded || (self.friends.is_none() && self.friend_pending) {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(text("Cargando amigos…", 13., false, MUTED));
                            });
                            return;
                        }
                        if !primed {
                            ui.label(text(
                                "Inicia sesión con una cuenta para usar Amigos.",
                                14.,
                                false,
                                MUTED,
                            ));
                            return;
                        }
                        let Some(state) = &self.friends else { return };
                        if !state.requests.is_empty() {
                            ui.label(text("Quieren ser tus amigos", 13., true, MUTED));
                        }
                        for req in &state.requests {
                            friend_row(
                                ui,
                                art,
                                &self.friend_heads,
                                &req.from,
                                &[("Aceptar", "accept"), ("Rechazar", "decline")],
                                self.friend_pending,
                                cmd,
                            );
                        }
                        if state.friends.is_empty() {
                            let (box_, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 120.),
                                Sense::hover(),
                            );
                            outline(ui.painter(), box_, 13., ROW, Color32::from_rgb(32, 31, 52));
                            icon(
                                ui.painter(),
                                art,
                                9,
                                pos2(box_.center().x, box_.top() + 35.),
                                27.,
                                PLACEHOLDER,
                            );
                            let message = "Todavía no tienes amigos en Ika";
                            label(
                                ui.painter(),
                                pos2(box_.center().x, box_.top() + 68.),
                                Align2::CENTER_CENTER,
                                message,
                                14.,
                                true,
                                INK,
                            );
                            label(
                                ui.painter(),
                                pos2(box_.center().x, box_.top() + 93.),
                                Align2::CENTER_CENTER,
                                "Agrega a alguien a la derecha.",
                                13.,
                                false,
                                MUTED,
                            );
                        }
                        for who in &state.friends {
                            friend_row(
                                ui,
                                art,
                                &self.friend_heads,
                                who,
                                &[("Quitar", "remove")],
                                self.friend_pending || self.friend_remove.is_some(),
                                cmd,
                            );
                        }
                    });
            },
        );

        let check = rect(right.min, 0., right.height() - 22., right.width(), 22.);
        let strip = rect(
            right.min,
            0.,
            34.,
            right.width(),
            (right.height() - 56.).max(1.),
        );
        inside(ui, strip, "friend-form-scroll", |ui| {
            egui::ScrollArea::vertical().id_salt(("form",self.dialog.generation)).auto_shrink([false,false]).show(ui,|ui| {
                ui.spacing_mut().item_spacing.y = 8.;
                ui.add(egui::Label::new(text("Escribe su nombre de jugador. Le llegará una notificación al instante.",13.,false,MUTED)).wrap());
                ui.add_space(10.);
                let card = ui.allocate_space(vec2(ui.available_width(),239.)).1;
                outline(ui.painter(),card,16.,Color32::from_rgb(14,9,37),Color32::from_rgb(43,26,77));
                let icon_box = Rect::from_center_size(pos2(card.center().x,card.top()+65.),Vec2::splat(84.));
                ui.painter().rect_filled(icon_box,20.,Color32::from_rgb(22,18,48));
                let target_key = self.friend_target.trim().to_ascii_lowercase();
                if let Some(Some(head_tex)) = self.friend_heads.get(&target_key) {
                    pixel_head(ui.painter(),head_tex,icon_box.shrink(6.),15.);
                } else {
                    icon(ui.painter(),art,8,icon_box.center(),40.,Color32::from_rgb(113,98,171));
                }
                let input = rect(card.min,21.,123.,card.width()-42.,41.);
                skin_input_frame(ui,input,FIELD_EDGE);
                let response = edit(ui,Rect::from_center_size(input.center(),vec2(input.width()-10.,22.)),"friend-name",&mut self.friend_target,"Nombre de jugador",16,Align::Center);
                if self.dialog.focus_friend && primed {
                    response.request_focus();
                    self.dialog.focus_friend = false;
                    self.dirty = true;
                }
                if response.changed() {
                    self.friend_error = None;
                    self.dialog.target = Some(Instant::now()+Duration::from_millis(350));
                }
                let send = rect(card.min,21.,174.,card.width()-42.,44.);
                let caption = if self.friend_mutating { "Enviando…" } else { "Enviar solicitud" };
                if (button(ui,send,caption,true) || (response.lost_focus() && ui.input(|i|i.key_pressed(egui::Key::Enter))))
                      && primed && !self.friend_pending && self.friend_remove.is_none() {
                    let target = self.friend_target.trim();
                    let error = if !crate::minecraft::valid_username(target) {
                        Some("Escribe un nombre de jugador de 2 a 16 caracteres.")
                    } else if target.eq_ignore_ascii_case(&self.data.active) {
                        Some("No puedes añadirte a ti mismo.")
                    } else if self.friends.as_ref().is_some_and(|s| s.friends.iter().any(|n|n.eq_ignore_ascii_case(target))) {
                        Some("Ese jugador ya está en tus amigos.")
                    } else if self.friends.as_ref().is_some_and(|s| s.sent.iter().any(|n|n.eq_ignore_ascii_case(target))) {
                        Some("Ya enviaste una solicitud a ese jugador.")
                    } else { None };
                    if let Some(error) = error { self.friend_error = Some(error.into()); self.dirty = true; }
                    else { cmd.friend=Some(("request",target.to_owned())); }
                }
                if let Some(error) = &self.friend_error { ui.label(text(error,13.,false,Color32::LIGHT_RED)); }
                if let Some(state) = &self.friends && !state.sent.is_empty() {
                    ui.label(text("Esperando respuesta",13.,true,MUTED));
                    for who in &state.sent {
                        friend_row(ui,art,&self.friend_heads,who,&[("Cancelar","cancel")],self.friend_pending,cmd);
                    }
                }
            });
        });
        let response = ui
            .interact(check, ui.id().with("instance-autoaccept"), Sense::hover())
            .on_hover_text(
                "La recepción de instancias aún no está disponible en el launcher nativo.",
            );
        let _ = response;
        let mark = rect(check.min, 0., 5., 12., 12.);
        outline(
            ui.painter(),
            mark,
            2.,
            Color32::from_rgb(222, 222, 234),
            Color32::from_rgb(134, 135, 161),
        );
        if self.data.friends_autoaccept {
            ui.painter().add(egui::Shape::line(
                vec![
                    mark.min + vec2(2.5, 5.5),
                    mark.min + vec2(5., 8.),
                    mark.min + vec2(9.5, 3.),
                ],
                Stroke::new(1.5, Color32::from_rgb(73, 36, 143)),
            ));
        }
        label(
            &ui.painter_at(check),
            check.left_center() + vec2(21., 0.),
            Align2::LEFT_CENTER,
            "Instalar sin preguntar las instancias que me envíen mis amigos",
            12.,
            false,
            MUTED,
        );

        if cmd
            .friend
            .as_ref()
            .is_some_and(|(action, _)| *action == "remove")
        {
            self.friend_remove = cmd.friend.take().map(|(_, who)| who);
            self.dirty = true;
        }
        if let Some(who) = self.friend_remove.clone() {
            let r = Rect::from_center_size(area.center(), vec2((w - 30.).min(392.), 162.));
            ui.painter()
                .rect_filled(area.shrink(2.), 14., Color32::from_black_alpha(145));
            shadow(ui.painter(), r, 14., 16);
            outline(ui.painter(), r, 14., SURFACE, FIELD_EDGE);
            label(
                ui.painter(),
                r.min + vec2(17., 22.),
                Align2::LEFT_TOP,
                "QUITAR AMIGO",
                14.,
                true,
                CYAN,
            );
            label(
                ui.painter(),
                r.min + vec2(17., 58.),
                Align2::LEFT_TOP,
                &format!("¿Quitar a {who} de tus amigos?"),
                16.,
                true,
                INK,
            );
            if button(
                ui,
                rect(r.right_bottom(), -111., -50., 93., 35.),
                "Quitar",
                true,
            ) {
                cmd.friend = Some(("remove", who));
                self.friend_remove = None;
                self.dirty = true;
            }
            if button(
                ui,
                rect(r.right_bottom(), -222., -50., 102., 35.),
                "Cancelar",
                false,
            ) {
                self.friend_remove = None;
                self.dirty = true;
            }
        }
    }
}

fn friend_row(
    ui: &mut Ui,
    art: &Art,
    heads: &std::collections::BTreeMap<String, Option<TextureHandle>>,
    who: &str,
    buttons: &[(&str, &'static str)],
    busy: bool,
    cmd: &mut Actions,
) {
    let (row, hover) = ui.allocate_exact_size(vec2(ui.available_width(), 52.), Sense::hover());
    let hovered = hover.contains_pointer();
    ui.painter().rect_filled(
        row,
        12.,
        if hovered {
            Color32::from_rgb(20, 19, 43)
        } else {
            ROW
        },
    );
    let icon_box = rect(row.min, 8., 8., 36., 36.);
    if let Some(Some(tex)) = heads.get(&who.to_ascii_lowercase()) {
        pixel_head(ui.painter(), tex, icon_box, 8.);
    } else {
        icon(
            ui.painter(),
            art,
            8,
            icon_box.center(),
            24.,
            Color32::from_rgb(112, 96, 168),
        );
    }
    let show_actions = hovered || buttons[0].1 != "remove";
    let compact = row.width() < 290.;
    let bw = if compact { 62. } else { 78. };
    let text_right = row.right()
        - 8.
        - if show_actions {
            buttons.len() as f32 * (bw + 4.)
        } else {
            0.
        };
    let clip = Rect::from_min_max(row.min + vec2(53., 0.), pos2(text_right, row.bottom()));
    let painter = ui.painter_at(clip);
    label(
        &painter,
        row.left_center() + vec2(56., 0.),
        Align2::LEFT_CENTER,
        who,
        15.5,
        true,
        INK,
    );
    if show_actions && !busy {
        ui.push_id((who, buttons[0].1), |ui| {
            for (index, &(title, action)) in buttons.iter().rev().enumerate() {
                let r = rect(
                    row.right_top(),
                    -8. - (bw + 4.) * index as f32 - bw,
                    11.,
                    bw,
                    30.,
                );
                if button(ui, r, title, action == "accept") {
                    cmd.friend = Some((action, who.to_owned()));
                }
            }
        });
    }
}

impl Dialog {
    pub(super) fn contains(&self, point: Pos2) -> bool {
        self.surface.is_none_or(|rect| rect.contains(point))
    }
    pub(super) fn target_deadline(&self) -> Option<Instant> {
        self.target
    }
    pub(super) fn take_target(&mut self, now: Instant) -> bool {
        if self.target.is_some_and(|t| t <= now) {
            self.target = None;
            true
        } else {
            false
        }
    }
}

impl Profile {
    fn instances_ui(&mut self, ui: &mut Ui, cmd: &mut Actions) {
        let path = crate::minecraft::instances_root();
        setting_anchor(
            ui,
            &mut self.dialog.result_focus,
            "instance-folder",
            "Carpeta raíz de Instancias / Minecraft",
            13.,
        );
        ui.horizontal(|ui| {
            let r = ui
                .allocate_space(vec2((ui.available_width() - 174.).max(95.), 36.))
                .1;
            skin_input_frame(ui, r, FIELD_EDGE);
            label(
                &ui.painter_at(r.shrink(5.)),
                r.left_center() + vec2(10., 0.),
                Align2::LEFT_CENTER,
                &path.to_string_lossy(),
                14.,
                false,
                INK,
            );
            let apply = ui.allocate_space(vec2(90., 36.)).1;
            cmd.browse_instances |= button(ui, apply, "Explorar…", false);
            let open = ui.allocate_space(vec2(70., 36.)).1;
            if button(ui, open, "Abrir", true) {
                let wide = windows::core::HSTRING::from(path.as_os_str());
                crate::dock::open_url(windows::core::PCWSTR(wide.as_ptr()));
            }
        });
        ui.label(text("Ubicación de las instancias de Ika. El launcher también detecta las de Prism y otros lanzadores.",12.,false,MUTED));
        ui.add_space(8.);
        setting_anchor(
            ui,
            &mut self.dialog.result_focus,
            "launch-window-action",
            "Al iniciar el juego",
            13.,
        );
        let mut mode = game_window_mode();
        let old = mode;
        egui::ComboBox::from_id_salt("launch-window-action")
            .width(ui.available_width())
            .selected_text(if mode == 1 {
                "Ocultar launcher mientras juega"
            } else {
                "Mantener launcher abierto"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, 0, "Mantener launcher abierto");
                ui.selectable_value(&mut mode, 1, "Ocultar launcher mientras juega");
            });
        if mode != old {
            self.profile_error =
                storage::save_native("game-window.txt", if mode == 1 { b"hide" } else { b"keep" })
                    .err();
            if self.profile_error.is_none() {
                cmd.app = Some(AppAction::GameWindow(mode == 1));
            }
            self.dirty = true;
        }
        ui.label(text(
            "Acción que realiza el launcher al ejecutar Minecraft.",
            12.,
            false,
            MUTED,
        ));
    }
    fn system_ui(&mut self, ui: &mut Ui) {
        let state = self.system_autostart.get_or_insert_with(autostart_state);
        let width = ui.available_width();
        let r = ui.allocate_space(vec2(width, 75.)).1;
        scroll_to(ui, &mut self.dialog.result_focus, "autostart", r);
        outline(ui.painter(), r, 11., ROW, FIELD_EDGE);
        label(
            &ui.painter_at(r.shrink2(vec2(13., 0.))),
            r.min + vec2(14., 15.),
            Align2::LEFT_TOP,
            "Abrir Ika automáticamente al arrancar el ordenador",
            14.,
            true,
            INK,
        );
        label(
            &ui.painter_at(r.shrink2(vec2(13., 0.))),
            r.min + vec2(14., 40.),
            Align2::LEFT_TOP,
            "Ika se abre solo cuando inicias sesión en Windows.",
            12.,
            false,
            MUTED,
        );
        match state {
            Ok(on) => {
                let mut want = *on;
                if ui
                    .put(
                        rect(r.right_top(), -37., 24., 25., 25.),
                        egui::Checkbox::without_text(&mut want),
                    )
                    .changed()
                {
                    match set_autostart(want) {
                        Ok(()) => {
                            *on = want;
                            self.profile_error = None;
                        }
                        Err(e) => self.profile_error = Some(e),
                    }
                    self.dirty = true;
                }
            }
            Err(e) => {
                ui.label(text(e, 13., false, MUTED));
            }
        }
        ui.add_space(8.);
        let r = ui.allocate_space(vec2(width, 75.)).1;
        scroll_to(ui, &mut self.dialog.result_focus, "oneko", r);
        outline(ui.painter(), r, 11., ROW, FIELD_EDGE);
        label(
            &ui.painter_at(r.shrink2(vec2(13., 0.))),
            r.min + vec2(14., 15.),
            Align2::LEFT_TOP,
            "Oneko · Mascota de escritorio",
            14.,
            true,
            INK,
        );
        label(
            &ui.painter_at(r.shrink2(vec2(13., 0.))),
            r.min + vec2(14., 40.),
            Align2::LEFT_TOP,
            "Gatito de kyrie25 que sigue el cursor. Clic derecho cambia color, doble clic duerme.",
            12.,
            false,
            MUTED,
        );
        let mut oneko_enabled = self.oneko.config.enabled;
        if ui
            .put(
                rect(r.right_top(), -37., 24., 25., 25.),
                egui::Checkbox::without_text(&mut oneko_enabled),
            )
            .changed()
        {
            self.oneko.config.enabled = oneko_enabled;
            self.oneko.save();
            self.dirty = true;
        }
        if self.oneko.config.enabled {
            ui.add_space(8.);
            let r_color = ui.allocate_space(vec2(width, 75.)).1;
            outline(ui.painter(), r_color, 11., ROW, FIELD_EDGE);
            label(
                &ui.painter_at(r_color.shrink2(vec2(13., 0.))),
                r_color.min + vec2(14., 15.),
                Align2::LEFT_TOP,
                "Oneko negro · Pelaje oscuro",
                14.,
                true,
                INK,
            );
            label(
                &ui.painter_at(r_color.shrink2(vec2(13., 0.))),
                r_color.min + vec2(14., 40.),
                Align2::LEFT_TOP,
                if self.oneko.config.black {
                    "Pelaje negro activo. Desmarca para volver al blanco clásico (o clic derecho)."
                } else {
                    "Pelaje blanco clásico. Marca para usar la variante negra (o clic derecho)."
                },
                12.,
                false,
                MUTED,
            );
            let mut oneko_black = self.oneko.config.black;
            if ui
                .put(
                    rect(r_color.right_top(), -37., 24., 25., 25.),
                    egui::Checkbox::without_text(&mut oneko_black),
                )
                .changed()
            {
                self.oneko.config.black = oneko_black;
                self.oneko.save();
                self.dirty = true;
            }
        }
    }
}

pub fn game_window_mode() -> u8 {
    u8::from(
        std::fs::read_to_string(storage::native_dir().join("game-window.txt"))
            .is_ok_and(|s| s.trim() == "hide"),
    )
}
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "IkaLauncherNative";
fn reg() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("reg.exe");
    cmd.creation_flags(0x08000000);
    cmd
}
fn autostart_command() -> Result<String, String> {
    let path = std::env::var_os("IKA_BRIDGE_SCRIPT")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_file());
    let python = std::env::var_os("IKA_BRIDGE_PYTHON")
        .map(std::path::PathBuf::from)
        .and_then(|p| {
            if p.file_name().is_some_and(|n| n == "pythonw.exe") {
                Some(p)
            } else {
                Some(p.with_file_name("pythonw.exe"))
            }
        })
        .filter(|p| p.is_file());
    match (python, path) {
        (Some(python), Some(path)) => Ok(format!(
            "\"{}\" -B \"{}\"",
            python.display(),
            path.display()
        )),
        _ => Err(
            "Abre el launcher con bridge/run.py y una instalación de Python con pythonw.exe."
                .into(),
        ),
    }
}
fn autostart_state() -> Result<bool, String> {
    autostart_command()?;
    reg()
        .args(["query", RUN_KEY, "/v", RUN_NAME])
        .output()
        .map(|o| o.status.success())
        .map_err(|_| "No se pudo consultar el inicio de Windows.".into())
}
fn set_autostart(want: bool) -> Result<(), String> {
    let output = if want {
        let cmd = autostart_command()?;
        reg()
            .args([
                "add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &cmd, "/f",
            ])
            .output()
    } else {
        reg()
            .args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"])
            .output()
    };
    if output.is_ok_and(|o| o.status.success()) {
        Ok(())
    } else {
        Err("No se pudo actualizar el inicio de Windows.".into())
    }
}

fn setting_anchor(
    ui: &mut Ui,
    focus: &mut Option<&'static str>,
    key: &str,
    title: &str,
    size: f32,
) {
    let label = ui.label(text(title, size, true, INK));
    scroll_to(ui, focus, key, label.rect);
}
fn scroll_to(ui: &Ui, focus: &mut Option<&'static str>, key: &str, rect: Rect) {
    if *focus == Some(key) {
        *focus = None;
        ui.scroll_to_rect(rect.expand(10.), Some(Align::Min));
    }
}

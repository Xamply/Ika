use super::*;

const SHELL: Color32 = Color32::from_rgba_premultiplied(15, 11, 25, 250);
const EDGE: Color32 = Color32::from_rgb(64, 41, 94);
const GLOW: Color32 = Color32::from_rgb(0, 245, 212);
const LILAC: Color32 = Color32::from_rgb(192, 132, 252);
const SLATE: Color32 = Color32::from_rgb(148, 163, 184);

#[derive(Default)]
pub(in crate::profile) struct SkinMenuActions {
    pub close: bool,
    pub dragging: bool,
    pub slide: i32,
    pub slim: Option<bool>,
    pub activate: bool,
    pub upload: bool,
    pub select: Option<String>,
    pub add_account: bool,
    pub sign_out: bool,
    pub delete_skin: bool,
}

impl Profile {
    pub(in crate::profile) fn paint_skin_menu(
        &mut self,
        ctx: &egui::Context,
        ui: &Ui,
        window: &Window,
    ) -> SkinMenuActions {
        let bounds = ui.max_rect();
        let ([x, y, _, _], _) = crate::dock::profile_rect(window);
        let width = 260_f32.min(bounds.width() - 16.).max(1.);
        let rows = self.data.accounts.len();
        let list_height = if rows == 0 {
            26.
        } else {
            (rows as f32 * 46. - 2.).min(220.)
        };
        let desired = 331.
            + list_height
            + 59.
            + if self.has_account() { 42. } else { 0. }
            + if self.dialog.account_add { 42. } else { 0. }
            + if self.profile_error.is_some() {
                36.
            } else {
                0.
            };
        let bottom = (y - 8.).clamp(bounds.top() + 1., bounds.bottom());
        let height = desired.min((bottom - bounds.top() - 12.).max(1.));
        let area = Rect::from_min_size(
            pos2(
                x.max(16.)
                    .min((bounds.right() - width - 8.).max(bounds.left())),
                bottom - height,
            ),
            vec2(width, height),
        );
        let mut action = SkinMenuActions::default();
        if ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| !area.contains(p))
        }) {
            action.close = true;
            return action;
        }
        egui::Area::new(egui::Id::new("ika-skin-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            .default_size(area.size())
            .movable(false)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_min_size(area.size());
                shadow(ui.painter(), area, 14., 19);
                outline(ui.painter(), area, 14., SHELL, EDGE);
                let body = area.shrink(9.);
                let generation = self.dialog.generation;
                inside(ui, body, "skin-menu-body", |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("skin-menu-scroll", generation))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(4., 5.);
                            let card = ui.allocate_space(vec2(ui.available_width(), 288.)).1;
                            self.skin_card(ui, card, &mut action, window);
                            let header = ui.allocate_space(vec2(ui.available_width(), 24.)).1;
                            label(
                                ui.painter(),
                                header.left_center() + vec2(10., 0.),
                                Align2::LEFT_CENTER,
                                "CUENTAS DE USUARIO",
                                10.5,
                                true,
                                GLOW,
                            );
                            let list = ui.allocate_space(vec2(ui.available_width(), list_height)).1;
                            if rows == 0 {
                                label(
                                    ui.painter(),
                                    list.left_center() + vec2(10., 0.),
                                    Align2::LEFT_CENTER,
                                    if !self.loaded {
                                        "Cargando cuentas…"
                                    } else if self.data.origin_found {
                                        "No hay cuentas guardadas"
                                    } else {
                                        "No se encontraron cuentas guardadas"
                                    },
                                    11.5,
                                    false,
                                    SLATE,
                                );
                            } else {
                                // The allocated list already reserves its full height in the menu.
                                let mut list_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .id_salt("skin-menu-accounts")
                                        .max_rect(list)
                                        .layout(egui::Layout::top_down(Align::Min)),
                                );
                                list_ui.set_clip_rect(ui.clip_rect().intersect(list));
                                egui::ScrollArea::vertical()
                                    .max_height(list.height())
                                    .auto_shrink([false, false])
                                    .show(&mut list_ui, |ui| {
                                        ui.spacing_mut().item_spacing.y = 2.;
                                        for account in &self.data.accounts {
                                            let active = account.username == self.data.active;
                                            let row = ui
                                                .allocate_space(vec2(ui.available_width(), 44.))
                                                .1;
                                            let hit = click(
                                                ui,
                                                row,
                                                ("skin-menu-account", &account.username),
                                            );
                                            if active {
                                                outline(
                                                    ui.painter(),
                                                    row,
                                                    8.,
                                                    rgba(0, 245, 212, 30),
                                                    rgba(0, 245, 212, 89),
                                                );
                                            } else if hit.hovered() {
                                                outline(
                                                    ui.painter(),
                                                    row,
                                                    8.,
                                                    rgba(168, 85, 247, 38),
                                                    rgba(168, 85, 247, 76),
                                                );
                                            }
                                            let icon_area = rect(row.min, 11., 9., 26., 26.);
                                            if let Some(t) =
                                                active.then_some(self.avatar.as_ref()).flatten()
                                            {
                                                let draw = if self.avatar_is_skin { pixel_head } else { head };
                                                draw(ui.painter(), t, icon_area, 4.);
                                            } else {
                                                ui.painter().rect_filled(icon_area, 4., FIELD);
                                                icon(
                                                    ui.painter(),
                                                    self.art.as_ref().unwrap(),
                                                    8,
                                                    icon_area.center(),
                                                    17.,
                                                    SLATE,
                                                );
                                            }
                                            let p = ui.painter_at(row.shrink2(vec2(8., 0.)));
                                            label(
                                                &p,
                                                row.min + vec2(47., 15.),
                                                Align2::LEFT_CENTER,
                                                account.label(),
                                                12.5,
                                                true,
                                                INK,
                                            );
                                            label(
                                                &p,
                                                row.min + vec2(47., 29.),
                                                Align2::LEFT_CENTER,
                                                &format!(
                                                    "@{}",
                                                    account.username.to_ascii_lowercase()
                                                ),
                                                10.5,
                                                false,
                                                SLATE,
                                            );
                                            if hit.clicked() {
                                                action.select = Some(account.username.clone());
                                                action.close = true;
                                            }
                                        }
                                    });
                            }
                            ui.add_space(4.);
                            let divider = ui.allocate_space(vec2(ui.available_width(), 1.)).1;
                            ui.painter().hline(
                                divider.x_range(),
                                divider.center().y,
                                Stroke::new(1., rgba(255, 255, 255, 20)),
                            );
                            ui.add_space(4.);
                            let add = ui.allocate_space(vec2(ui.available_width(), 34.)).1;
                            if menu_link(ui, add, "Agregar otra cuenta", false) {
                                self.dialog.account_add = !self.dialog.account_add;
                                if self.dialog.account_add {
                                    self.dialog.focus_new_account = true;
                                }
                                self.dirty = true;
                            }
                            if self.dialog.account_add {
                                let field = ui.allocate_space(vec2(ui.available_width(), 33.)).1;
                                let input = rect(field.min, 0., 0., field.width() - 76., 32.);
                                skin_input_frame(ui, input, FIELD_EDGE);
                                let edit = edit_field_in(
                                    ui,
                                    input,
                                    "skin-menu-new-account",
                                    &mut self.new_account,
                                    "Nombre de Minecraft",
                                    16,
                                );
                                if self.dialog.focus_new_account {
                                    edit.request_focus();
                                    edit.scroll_to_me(None);
                                    self.dialog.focus_new_account = false;
                                    self.dirty = true;
                                }
                                action.add_account |= button(
                                    ui,
                                    rect(field.right_top(), -72., 0., 72., 32.),
                                    "Agregar",
                                    true,
                                ) || (edit.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            }
                            if self.has_account() {
                                let sign_out = ui.allocate_space(vec2(ui.available_width(), 34.)).1;
                                action.sign_out = menu_link(ui, sign_out, "Cerrar sesión", true);
                                action.close |= action.sign_out;
                            }
                            if let Some(e) =
                                self.profile_error.as_ref().or(self.data.error.as_ref())
                            {
                                ui.label(text(e, 12., false, Color32::LIGHT_RED));
                            }
                        });
                });
            });
        action
    }

    fn skin_card(&mut self, ui: &Ui, card: Rect, action: &mut SkinMenuActions, window: &Window) {
        let w = card.width() - 2.;
        let inside = card.min + vec2(1., 1.);
        let view = rect(inside, 0., 27., w, 195.);
        let drag = ui
            .interact(view, ui.id().with("skin-model-drag"), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_and_drag_cursor(egui::CursorIcon::Grabbing);
        action.dragging = drag.dragged();
        if action.dragging {
            if !self.rotation.dragging()
                && let Some(position) = ui.input(|i| i.pointer.latest_pos())
            {
                self.start_model_drag(window, position, drag.drag_delta());
            }
        } else {
            self.rotation.end();
        }
        let art = self.art.as_ref().unwrap();
        let p = ui.painter();
        outline(p, card, 10., Color32::from_rgb(15, 11, 26), EDGE);
        let header = rect(inside, 0., 0., w, 27.);
        p.rect_filled(
            header,
            egui::CornerRadius {
                nw: 9,
                ne: 9,
                sw: 0,
                se: 0,
            },
            Color32::from_rgb(23, 16, 38),
        );
        p.circle_filled(
            header.left_center() + vec2(12., 0.),
            6.,
            rgba(0, 245, 212, 20),
        );
        p.circle_filled(header.left_center() + vec2(12., 0.), 3., GLOW);
        label(
            p,
            header.left_center() + vec2(20., 0.),
            Align2::LEFT_CENTER,
            "3D PREVIEW",
            10.,
            true,
            GLOW,
        );
        let name = &self.data.active;
        let list = self
            .data
            .wardrobe
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let custom = list.iter().position(|skin| skin.id == self.preview_skin.id);
        let empty = self.preview_empty;
        let index = if empty {
            list.len() + 2
        } else {
            custom.map_or(1, |i| i + 2)
        };
        let pos = rect(header.center_top(), -8., 6., 37., 16.);
        outline(p, pos, 4., rgba(168, 85, 247, 46), rgba(168, 85, 247, 76));
        label(
            p,
            pos.center(),
            Align2::CENTER_CENTER,
            &format!("{index} / {}", list.len() + 2),
            9.5,
            true,
            LILAC,
        );
        let hint = rect(header.right_top(), -70., 3., 65., 21.);
        let _ = ui
            .interact(hint, ui.id().with("skin-rotate-hint"), Sense::hover())
            .on_hover_text("Arrastra con el ratón para rotar 360°");
        label(
            p,
            hint.right_center() - vec2(4., 0.),
            Align2::RIGHT_CENTER,
            "Rotar 360°",
            9.5,
            false,
            Color32::from_rgb(167, 139, 250),
        );
        let turn = hint.left_center() + vec2(4., 0.);
        let line = (0..=15)
            .map(|i| turn + Vec2::angled(-0.85 + std::f32::consts::TAU * i as f32 / 17.) * 4.4)
            .collect::<Vec<_>>();
        p.add(egui::Shape::line(line, Stroke::new(0.8, LILAC)));
        p.line_segment(
            [turn + vec2(5., -5.), turn + vec2(5., -1.5)],
            Stroke::new(0.8, LILAC),
        );
        preview_gradient(p, view);
        if let Some(tex) = &self.model {
            let side = view.width().min(189.);
            let box_ = Rect::from_center_size(view.center(), Vec2::splat(side));
            p.image(tex.id(), box_, FULL_UV, Color32::WHITE);
        }
        for (dir, cx, label) in [
            (-1., view.left() + 22., "Skin anterior"),
            (1., view.right() - 22., "Siguiente skin"),
        ] {
            let r = Rect::from_center_size(pos2(cx, view.center().y), Vec2::splat(28.));
            let res = click(ui, r, ("skin-menu-arrow", label)).on_hover_text(label);
            let fill = if res.hovered() {
                Color32::from_rgb(72, 41, 114)
            } else {
                Color32::from_rgb(15, 11, 26)
            };
            outline(
                p,
                r,
                14.,
                fill,
                if res.hovered() {
                    GLOW
                } else {
                    Color32::from_rgb(84, 48, 124)
                },
            );
            chevron(
                p,
                r.center(),
                dir,
                6.5,
                if res.hovered() { GLOW } else { INK },
            );
            if res.clicked() {
                action.slide = dir as i32;
            }
        }
        if empty {
            let r = Rect::from_center_size(view.center() + vec2(0., -4.), Vec2::splat(44.));
            let res = click(ui, r, "skin-upload-plus")
                .on_hover_text("Cargar textura de Minecraft (.png)");
            outline(
                p,
                r,
                22.,
                rgba(28, 18, 50, 226),
                if res.hovered() { GLOW } else { INK },
            );
            plus(p, r.center(), 17., 2., INK);
            let c =
                Rect::from_center_size(pos2(view.center().x, r.bottom() + 18.), vec2(128., 19.));
            outline(p, c, 6., rgba(15, 11, 26, 234), rgba(255, 255, 255, 95));
            label(
                p,
                c.center(),
                Align2::CENTER_CENTER,
                "CARGAR TEXTURA (.PNG)",
                9.,
                true,
                INK,
            );
            if res.clicked() || click(ui, c, "skin-upload-label").clicked() {
                if name.is_empty() {
                    self.dialog.account_add = true;
                    self.dialog.focus_new_account = true;
                    self.dirty = true;
                } else {
                    action.upload = true;
                }
            }
        }
        let meta = rect(inside, 0., 222., w, 27.);
        p.rect_filled(meta, 0., Color32::from_rgb(17, 12, 29));
        p.hline(
            meta.x_range(),
            meta.top(),
            Stroke::new(1., rgba(168, 85, 247, 38)),
        );
        let title = if empty {
            "Nueva Skin"
        } else if let Some(i) = custom {
            &list[i].name
        } else if name.is_empty() {
            "Steve"
        } else {
            name
        };
        let title_p = p.with_clip_rect(rect(meta.min, 10., 0., 104., meta.height()));
        let title_end = label(
            &title_p,
            meta.left_center() + vec2(10., 0.),
            Align2::LEFT_CENTER,
            title,
            11.5,
            true,
            INK,
        )
        .right()
        .min(meta.left() + 111.);
        let (tag, tw) = if empty {
            ("VACÍA", 38.)
        } else if custom.is_some() {
            ("CUSTOM", 49.)
        } else {
            ("OFICIAL", 46.)
        };
        let chip = Rect::from_min_size(pos2(title_end + 6., meta.center().y - 7.), vec2(tw, 14.));
        outline(p, chip, 4., rgba(168, 85, 247, 51), rgba(168, 85, 247, 89));
        label(
            p,
            chip.center(),
            Align2::CENTER_CENTER,
            tag,
            8.5,
            true,
            LILAC,
        );
        let status = if empty {
            "Haz clic en +"
        } else if custom.is_some() {
            "Personalizada"
        } else if name.is_empty() {
            "Invitado"
        } else {
            "En línea"
        };
        label(
            p,
            meta.right_center() - vec2(if custom.is_some() { 26. } else { 10. }, 0.),
            Align2::RIGHT_CENTER,
            status,
            10.,
            false,
            SLATE,
        );
        if custom.is_some() {
            let r = rect(meta.right_top(), -23., 3., 19., 21.);
            let t = click(ui, r, "delete-custom-skin")
                .on_hover_text("Eliminar esta skin personalizada");
            icon(
                p,
                art,
                10,
                r.center(),
                12.,
                if t.hovered() {
                    Color32::LIGHT_RED
                } else {
                    Color32::from_rgb(239, 68, 68)
                },
            );
            if t.clicked() {
                self.dialog.confirm_skin_delete = !self.dialog.confirm_skin_delete;
                self.dirty = true;
            }
        }
        let footer = rect(inside, 0., 249., w, 37.);
        p.rect_filled(
            footer,
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 9,
                se: 9,
            },
            Color32::from_rgb(13, 9, 23),
        );
        p.hline(
            footer.x_range(),
            footer.top(),
            Stroke::new(1., rgba(168, 85, 247, 31)),
        );
        let mode = rect(footer.min, 10., 7., 88., 22.);
        outline(p, mode, 6., rgba(15, 11, 26, 219), rgba(168, 85, 247, 76));
        for (caption, slim, dx, sx) in [("Steve", false, 3., 44.), ("Alex", true, 49., 36.)] {
            let format = rect(mode.min, dx, 3., sx, 16.);
            let res = click(ui, format, caption);
            let selected = self.preview_skin.slim == slim;
            if selected || res.hovered() {
                p.rect_filled(
                    format,
                    4.,
                    if selected {
                        GLOW
                    } else {
                        rgba(168, 85, 247, 50)
                    },
                );
            }
            label(
                p,
                format.center(),
                Align2::CENTER_CENTER,
                caption,
                9.5,
                true,
                if selected {
                    Color32::from_rgb(13, 11, 20)
                } else {
                    SLATE
                },
            );
            if res.clicked() {
                action.slim = Some(slim);
            }
        }
        let current = self.data.selection(name);
        let selected = !empty
            && (name.is_empty()
                || (current.id == self.preview_skin.id && current.slim == self.preview_skin.slim));
        let apply = rect(footer.min, 106., 7., footer.width() - 116., 22.);
        let enabled = !name.is_empty() && !empty && !selected;
        let pressed = enabled.then(|| click(ui, apply, "skin-activate"));
        let hot = pressed.as_ref().is_some_and(Response::hovered);
        let (fill, edge, tint, caption) = if selected {
            (
                rgba(34, 197, 94, 51),
                rgba(34, 197, 94, 127),
                Color32::from_rgb(74, 222, 128),
                "Skin activa",
            )
        } else if empty {
            (
                rgba(139, 92, 246, 38),
                rgba(168, 85, 247, 52),
                SLATE,
                "Cargar primero",
            )
        } else {
            (
                rgba(139, 92, 246, if hot { 133 } else { 84 }),
                if hot { GLOW } else { rgba(168, 85, 247, 115) },
                INK,
                "Establecer skin",
            )
        };
        outline(p, apply, 6., fill, edge);
        label(
            p,
            apply.center() + vec2(6., 0.),
            Align2::CENTER_CENTER,
            caption,
            10.,
            true,
            tint,
        );
        let mark = apply.center() - vec2(36., 0.);
        p.add(egui::Shape::line(
            vec![
                mark + vec2(-3., 0.),
                mark + vec2(-1., 2.),
                mark + vec2(4., -2.5),
            ],
            Stroke::new(1.15, tint),
        ));
        action.activate = pressed.is_some_and(|r| r.clicked());
        if self.dialog.confirm_skin_delete {
            let confirm =
                Rect::from_center_size(view.center(), vec2((view.width() - 20.).min(190.), 88.));
            outline(p, confirm, 10., SHELL, EDGE);
            label(
                p,
                confirm.center_top() + vec2(0., 24.),
                Align2::CENTER_CENTER,
                "¿Quitar esta skin?",
                14.,
                true,
                INK,
            );
            if button(ui, rect(confirm.min, 9., 46., 83., 31.), "Cancelar", false) {
                self.dialog.confirm_skin_delete = false;
                self.dirty = true;
            }
            if button(
                ui,
                rect(confirm.right_top(), -92., 46., 83., 31.),
                "Quitar",
                true,
            ) {
                self.dialog.confirm_skin_delete = false;
                action.delete_skin = true;
                self.dirty = true;
            }
        }
    }
}

fn preview_gradient(p: &Painter, area: Rect) {
    // The viewport is static; it has no backdrop sampling over the wallpaper.
    let mut mesh = Mesh::default();
    let base = Color32::from_rgb(16, 12, 26);
    let accent = Color32::from_rgb(44, 27, 72);
    let n = 12u32;
    for y in 0..=n {
        for x in 0..=n {
            let uv = vec2(x as f32 / n as f32, y as f32 / n as f32);
            let r = ((uv.x - 0.5) * 1.2).hypot((uv.y - 0.31) * 0.9);
            mesh.colored_vertex(
                area.min + area.size() * uv,
                mix(base, accent, (1. - r * 1.65).clamp(0., 1.)),
            );
            if x != 0 && y != 0 {
                let i = y * (n + 1) + x;
                mesh.add_triangle(i - n - 2, i - n - 1, i - 1);
                mesh.add_triangle(i - 1, i - n - 1, i);
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

fn menu_link(ui: &Ui, area: Rect, label_: &str, danger: bool) -> bool {
    let res = click(ui, area, label_);
    let p = ui.painter();
    let color = if res.hovered() && danger {
        Color32::from_rgb(252, 165, 165)
    } else {
        Color32::from_rgb(203, 213, 225)
    };
    if res.hovered() {
        p.rect_filled(
            area,
            8.,
            if danger {
                rgba(239, 68, 68, 46)
            } else {
                rgba(255, 255, 255, 15)
            },
        );
    }
    let at = area.left_center() + vec2(18., 0.);
    if danger {
        p.add(egui::Shape::line(
            vec![
                at + vec2(-1., -6.),
                at + vec2(-5., -6.),
                at + vec2(-5., 6.),
                at + vec2(-1., 6.),
            ],
            Stroke::new(1.2, color),
        ));
        p.line_segment(
            [at + vec2(-2., 0.), at + vec2(6., 0.)],
            Stroke::new(1.2, color),
        );
        chevron(p, at + vec2(4., 0.), 1., 6., color);
    } else {
        plus(p, at, 8., 1.25, color);
    }
    label(
        p,
        area.left_center() + vec2(35., 0.),
        Align2::LEFT_CENTER,
        label_,
        12.,
        true,
        color,
    );
    res.clicked()
}

fn plus(p: &Painter, center: Pos2, size: f32, width: f32, color: Color32) {
    for v in [vec2(size * 0.5, 0.), vec2(0., size * 0.5)] {
        p.line_segment([center - v, center + v], Stroke::new(width, color));
    }
}

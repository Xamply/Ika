mod model;
mod console;
mod oneko;
mod paint;
mod social;
mod storage;
mod tube;
mod windows_ui;
mod instance_ui;
pub use instance_ui::Mode as InstanceMode;
pub use paint::PaintGpu;
pub use windows_ui::{AppAction, game_window_mode};
pub use console::{Action as ConsoleAction, View as ConsoleView};
pub use oneko::Oneko;

use self::social::State;
use crate::{bridge_signal::Wake, home_widgets::ClockOcclusion, minecraft::Skin, shared::Source};
use egui::{Color32, Pos2, Rect, TextureHandle, Vec2, pos2, vec2};
use std::{
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
use storage::{Cover, Crop};
use winit::{event::WindowEvent, event_loop::EventLoopProxy, window::Window};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Profile,
    Preferences,
    Friends,
    Instance,
}

enum Update {
    Data(storage::Data),
    Skin(String, Option<Skin>),
    Social(String, Result<State, String>),
    Cover(Result<(Cover, Crop), String>),
    Heads(Vec<(String, Option<image::RgbaImage>)>),
}

fn take_layer(ctx: &egui::Context, layer: egui::LayerId) -> Vec<egui::epaint::ClippedShape> {
    let mut shapes: Vec<_> = ctx.graphics_mut(|graphics| {
        std::mem::take(graphics.entry(layer)).all_entries().cloned().collect()
    });
    if let Some(transform) = ctx.layer_transform_to_global(layer) {
        for shape in &mut shapes {
            shape.transform(transform);
        }
    }
    shapes
}

pub struct Profile {
    data: storage::Data,
    tx: mpsc::Sender<Update>,
    rx: mpsc::Receiver<Update>,
    wake: EventLoopProxy<()>,
    loaded: bool,
    pub panel: Option<Panel>,
    instance_dialog: Option<instance_ui::Dialog>,
    instance_saved: Option<(std::path::PathBuf, String)>,
    ctx: egui::Context,
    input: Option<egui_winit::State>,
    dirty: bool,
    dock_visible: bool,
    clock_occlusion: ClockOcclusion,
    console: Option<console::Console>,
    console_action: Option<ConsoleAction>,
    pub oneko: Oneko,
    skin_pending: Option<String>,
    avatar: Option<TextureHandle>,
    avatar_is_skin: bool,
    grabbing_cursor: Option<winit::window::CustomCursor>,
    skin_image: Option<(image::RgbaImage, bool)>,
    model: Option<model::PreviewTexture>,
    rotation: model::Rotation,
    preview_skin: storage::SkinSelection,
    preview_image: Option<image::RgbaImage>,
    preview_empty: bool,
    image: Option<TextureHandle>,
    friends: Option<State>,
    friend_target: String,
    friend_error: Option<String>,
    friend_pending: bool,
    friend_refresh: Option<Instant>,
    profile_error: Option<String>,
    new_account: String,
    identity_edit: Option<storage::Identity>,
    remove_confirm: bool,
    cover_token: u64,
    cover_running: Option<(u64, Instant)>,
    cover_failed: bool,
    cover_importing: bool,
    cover_restart: bool,
    crop_edit: Option<Crop>,
    dialog: windows_ui::Dialog,
    art: Option<windows_ui::Art>,
    friend_heads: std::collections::BTreeMap<String, Option<TextureHandle>>,
    friend_mutating: bool,
    friend_remove: Option<String>,
    app_action: Option<AppAction>,
    system_autostart: Option<Result<bool, String>>,
    next_paint: Option<Instant>,
    border_tick: Option<Instant>,
    preferences_tube: Option<crate::crt::Tube>,
    tube_tick: Option<Instant>,
    reduced_motion: bool,
}
impl Profile {
    pub fn has_account(&self) -> bool {
        !self.data.active.is_empty()
    }
    pub fn new(wake: EventLoopProxy<()>, enabled: bool, reduced_motion: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        if enabled {
            let (tx, wake) = (tx.clone(), wake.clone());
            std::thread::spawn(move || {
                let result =
                    std::panic::catch_unwind(storage::load).unwrap_or_else(|_| storage::Data {
                        error: Some("No se pudieron leer los perfiles de WebView2.".into()),
                        ..Default::default()
                    });
                let _ = tx.send(Update::Data(result));
                let _ = wake.send_event(());
            });
        }
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        windows_ui::install_fonts(&ctx);
        Self {
            data: Default::default(),
            tx,
            rx,
            wake,
            loaded: !enabled,
            panel: None,
            instance_dialog: None,
            instance_saved: None,
            ctx,
            input: None,
            dirty: false,
            dock_visible: false,
            clock_occlusion: ClockOcclusion::default(),
            console: enabled.then(console::Console::new),
            console_action: None,
            oneko: Oneko::new(),
            grabbing_cursor: None,
            skin_pending: None,
            avatar: None,
            avatar_is_skin: false,
            skin_image: None,
            model: None,
            rotation: Default::default(),
            preview_skin: storage::SkinSelection {
                id: String::new(),
                slim: false,
            },
            preview_image: None,
            preview_empty: false,
            image: None,
            friends: None,
            friend_target: String::new(),
            friend_error: None,
            friend_pending: false,
            friend_refresh: None,
            profile_error: None,
            cover_token: 0,
            cover_running: None,
            cover_failed: false,
            cover_importing: false,
            cover_restart: false,
            crop_edit: None,
            new_account: String::new(),
            identity_edit: None,
            remove_confirm: false,
            dialog: Default::default(),
            art: None,
            friend_heads: Default::default(),
            friend_mutating: false,
            friend_remove: None,
            app_action: None,
            system_autostart: None,
            next_paint: None,
            border_tick: None,
            preferences_tube: None,
            tube_tick: None,
            reduced_motion,
        }
    }
    pub fn init_cursor(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Ok(img) = image::load_from_memory(include_bytes!("profile/cursor_grabbing.png")) {
            let rgba = img.to_rgba8();
            if let Ok(source) = winit::window::CustomCursor::from_rgba(
                rgba.into_raw(),
                32,
                32,
                16,
                16,
            ) {
                self.grabbing_cursor = Some(event_loop.create_custom_cursor(source));
            }
        }
    }
    pub fn loading(&self) -> bool {
        !self.loaded
    }
    pub fn dock_ready(&self) -> bool {
        // A pending skin only changes the dock when no custom avatar is set.
        self.loaded
            && (self.skin_pending.as_ref() != Some(&self.data.active)
                || self.data.accounts.iter().any(|a| {
                    a.username == self.data.active && !a.custom_avatar_url.is_empty()
                }))
    }
    pub fn open_instance(&mut self, mode: InstanceMode, source: Option<crate::minecraft::Instance>) {
        if mode != InstanceMode::Create && source.is_none() { return; }
        self.open(Panel::Instance);
        self.instance_dialog = Some(instance_ui::Dialog::new(mode, source, self.wake.clone()));
        if self.has_tube() && !self.reduced_motion {
            self.preferences_tube = Some(crate::crt::Tube::new(false));
        }
    }
    pub fn take_instance_saved(&mut self) -> Option<(std::path::PathBuf, String)> {
        self.instance_saved.take()
    }
    fn has_tube(&self) -> bool {
        self.panel == Some(Panel::Preferences)
            || self.instance_dialog.is_some()
    }
    pub fn open(&mut self, panel: Panel) {
        if let Some(console) = &mut self.console { console.pause_game(); }
        self.instance_dialog = None;
        self.panel = Some(panel);
        self.preferences_tube = (panel == Panel::Preferences && !self.reduced_motion)
            .then(|| crate::crt::Tube::new(false));
        self.tube_tick = None;
        self.dialog.open(panel);
        self.friend_remove = None;
        self.dirty = true;
        self.cover_failed = false;
        self.crop_edit = None;
        self.identity_edit = None;
        self.remove_confirm = false;
        if panel == Panel::Profile {
            self.reset_preview();
        }
        if panel == Panel::Friends {
            self.social(None);
        }
    }
    pub fn close(&mut self) {
        if self.instance_dialog.take().is_some() {
            self.ctx.set_transform_layer(windows_ui::preferences_layer(), egui::emath::TSTransform::IDENTITY);
        }
        self.panel = None;
        self.preferences_tube = None;
        self.tube_tick = None;
        self.rotation.cancel();
        self.model = None;
        self.preview_image = None;
        self.friend_remove = None;
        self.next_paint = None;
        self.border_tick = None;
        self.dirty = true;
    }
    pub fn dismiss(&mut self, now: Instant) {
        if self.instance_dialog.as_ref().is_some_and(instance_ui::Dialog::busy) { return; }
        if self.has_tube() && !self.reduced_motion {
            self.preferences_tube
                .get_or_insert_with(|| crate::crt::Tube::new(false))
                .close(now);
            self.tube_tick = None;
            self.dirty = true;
        } else {
            self.close();
        }
    }

    pub fn presented(&mut self, now: Instant) {
        if let Some(tube) = &mut self.preferences_tube {
            tube.presented(now);
        }
        if let Some(dialog) = &mut self.instance_dialog {
            dialog.picker_presented(now);
        }
    }
    pub fn account(&self) -> Option<(String, Option<Skin>)> {
        let n = &self.data.active;
        (!n.is_empty()).then(|| (n.clone(), self.data.skins.get(n).cloned()))
    }
    fn reset_skin(&mut self) {
        self.avatar = None;
        self.model = None;
        self.skin_image = None;
        self.rotation = Default::default();
        self.reset_preview();
        self.dirty = true;
    }
    fn reset_preview(&mut self) {
        self.preview_empty = false;
        self.preview_skin = self.data.selection(&self.data.active);
        self.preview_image = None;
        self.model = None;
    }
    fn slot(&mut self, step: i32) {
        let name = &self.data.active;
        let list = self
            .data
            .wardrobe
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let current = if self.preview_empty {
            list.len() + 1
        } else {
            list.iter()
                .position(|s| s.id == self.preview_skin.id)
                .map_or(0, |n| n + 1)
        };
        let next = (current as i32 + step).rem_euclid(list.len() as i32 + 2) as usize;
        self.preview_empty = next == list.len() + 1;
        self.dialog.confirm_skin_delete = false;
        self.preview_skin = if self.preview_empty || next == 0 {
            storage::SkinSelection {
                id: String::new(),
                slim: self.data.official.get(name).is_some_and(|s| s.slim),
            }
        } else {
            let slot = &list[next - 1];
            storage::SkinSelection {
                id: slot.id.clone(),
                slim: slot.model_type == "slim",
            }
        };
        self.preview_image = None;
        self.model = None;
        self.dirty = true;
    }
    fn activate_skin(&mut self) {
        let name = self.data.active.clone();
        if name.is_empty() || self.preview_empty {
            return;
        }
        let key = name.to_ascii_lowercase();
        let old = self
            .data
            .edits
            .selected_skins
            .insert(key.clone(), self.preview_skin.clone());
        if let Err(e) = self.data.edits.save() {
            if let Some(old) = old {
                self.data.edits.selected_skins.insert(key, old);
            } else {
                self.data.edits.selected_skins.remove(&key);
            }
            self.profile_error = Some(e);
        } else {
            self.data.apply_skin(&name, &self.preview_skin.clone());
            self.profile_error = None;
            self.reset_skin();
        }
        self.dirty = true;
    }
    fn upload_skin(&mut self, window: &Window) {
        let name = self.data.active.clone();
        if name.is_empty() {
            return;
        }
        let Some(path) = file_dialog(window, false) else {
            return;
        };
        let Some(slot) = storage::SkinSlot::read_file(&path) else {
            self.profile_error = Some("Elige una textura PNG de Minecraft válida.".into());
            self.dirty = true;
            return;
        };
        let old_edits = self.data.edits.clone();
        let key = name.to_ascii_lowercase();
        if let Some(removed) = self.data.edits.hidden_skins.get_mut(&key) {
            removed.retain(|id| *id != slot.id);
        }
        let list = self.data.wardrobe.entry(name.clone()).or_default();
        if !list.iter().any(|x| x.id == slot.id) {
            let added = self
                .data
                .edits
                .uploaded_skins
                .entry(name.to_ascii_lowercase())
                .or_default();
            added.push(slot.clone());
            if let Err(e) = self.data.edits.save() {
                self.data.edits = old_edits;
                self.profile_error = Some(e);
                self.dirty = true;
                return;
            }
            list.push(slot.clone());
        }
        self.preview_empty = false;
        self.preview_skin = storage::SkinSelection {
            id: slot.id,
            slim: false,
        };
        self.profile_error = None;
        self.preview_image = None;
        self.model = None;
        self.dirty = true;
    }
    fn delete_preview_skin(&mut self) {
        let name = self.data.active.clone();
        let key = name.to_ascii_lowercase();
        let id = self.preview_skin.id.clone();
        if name.is_empty() || id.is_empty() || self.preview_empty {
            return;
        }
        let was_active = self.data.selection(&name).id == id;
        let original = self.data.edits.clone();
        self.data
            .edits
            .hidden_skins
            .entry(key.clone())
            .or_default()
            .push(id.clone());
        if let Some(list) = self.data.edits.uploaded_skins.get_mut(&key) {
            list.retain(|s| s.id != id);
        }
        let official = storage::SkinSelection {
            id: String::new(),
            slim: self.data.official.get(&name).is_some_and(|skin| skin.slim),
        };
        if was_active {
            self.data.edits.selected_skins.insert(key, official.clone());
        }
        match self.data.edits.save() {
            Ok(()) => {
                if let Some(list) = self.data.wardrobe.get_mut(&name) {
                    list.retain(|s| s.id != id);
                }
                if was_active {
                    self.data.apply_skin(&name, &official);
                }
                self.profile_error = None;
                self.reset_skin();
            }
            Err(error) => {
                self.data.edits = original;
                self.profile_error = Some(error);
            }
        }
        self.dirty = true;
    }
    fn upload_cover(&mut self, window: &Window) {
        if self.cover_importing || !self.loaded {
            return;
        }
        let Some(path) = file_dialog(window, true) else {
            return;
        };
        self.cover_importing = true;
        self.dirty = true;
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let result = storage::import_cover(&path);
            let _ = tx.send(Update::Cover(result));
            let _ = wake.send_event(());
        });
    }
    fn load_public_skin(&mut self) {
        let n = &self.data.active;
        if n.is_empty() || self.data.skins.contains_key(n) || self.skin_pending.as_ref() == Some(n)
        {
            return;
        }
        if let Some(a) = self
            .data
            .accounts
            .iter()
            .find(|a| a.username == *n)
            .cloned()
        {
            self.skin_pending = Some(n.clone());
            let (tx, wake) = (self.tx.clone(), self.wake.clone());
            std::thread::spawn(move || {
                let skin = storage::public_skin(&a);
                let _ = tx.send(Update::Skin(a.username, skin));
                let _ = wake.send_event(());
            });
        }
    }
    fn select(&mut self, n: String) {
        if self.data.active == n {
            return;
        }
        if let Err(e) = storage::save_selection(&n) {
            self.profile_error = Some(format!("No se pudo guardar la cuenta seleccionada: {e}"));
            self.dirty = true;
            return;
        }
        self.selected(n);
    }
    fn selected(&mut self, n: String) {
        self.data.active = n;
        self.identity_edit = None;
        self.remove_confirm = false;
        self.profile_error = None;
        self.friends = None;
        self.friend_pending = false;
        self.friend_mutating = false;
        self.friend_error = None;
        self.friend_refresh = None;
        self.reset_skin();
        self.load_public_skin();
    }
    fn add_account(&mut self) {
        let name = self.new_account.trim().to_string();
        if !crate::minecraft::valid_username(&name) {
            self.profile_error = Some("Usa de 2 a 16 letras, números o guiones bajos.".into());
            self.dirty = true;
            return;
        }
        if let Some(existing) = self
            .data
            .accounts
            .iter()
            .find(|a| a.username.eq_ignore_ascii_case(&name))
        {
            self.select(existing.username.clone());
            return;
        }
        let original = self.data.edits.clone();
        self.data
            .edits
            .removed
            .retain(|n| !n.eq_ignore_ascii_case(&name));
        let account = storage::Account {
            username: name.clone(),
            ..Default::default()
        };
        if !self
            .data
            .edits
            .added
            .iter()
            .any(|a| a.username.eq_ignore_ascii_case(&name))
        {
            self.data.edits.added.push(account.clone());
        }
        if let Err(e) = self.data.edits.save() {
            self.data.edits = original;
            self.profile_error = Some(e);
            self.dirty = true;
            return;
        }
        self.data.accounts.push(account);
        self.new_account.clear();
        self.select(name);
        self.dirty = true;
    }
    fn save_identity(&mut self) {
        let Some(a) = self
            .data
            .accounts
            .iter_mut()
            .find(|a| a.username == self.data.active)
        else {
            return;
        };
        let Some(mut edit) = self.identity_edit.take() else {
            return;
        };
        edit.display_name = edit.display_name.trim().to_owned();
        edit.status_quote = edit.status_quote.trim().to_owned();
        let name = a.username.to_ascii_lowercase();
        let old = self.data.edits.identity.insert(name.clone(), edit.clone());
        match self.data.edits.save() {
            Ok(()) => {
                a.display_name = edit.display_name;
                a.status_quote = edit.status_quote;
                self.profile_error = None;
            }
            Err(e) => {
                if let Some(old) = old {
                    self.data.edits.identity.insert(name, old);
                } else {
                    self.data.edits.identity.remove(&name);
                }
                self.identity_edit = Some(edit);
                self.profile_error = Some(e);
            }
        }
        self.dirty = true;
    }
    fn social(&mut self, action: Option<(&'static str, String)>) {
        if self.friend_pending || self.data.active.is_empty() {
            return;
        }
        self.friend_pending = true;
        self.friend_mutating = action.is_some();
        self.friend_error = None;
        self.dirty = true;
        let (name, tx, wake) = (self.data.active.clone(), self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let result = match action {
                Some((action, who)) => social::friend(&name, action, &who),
                None => social::sync(&name),
            };
            let _ = tx.send(Update::Social(name, result));
            let _ = wake.send_event(());
        });
    }
    pub fn poll(
        &mut self,
        now: Instant,
        visible: bool,
        bridge: Option<&Path>,
        wake: Option<&Wake>,
        gpu: &mut PaintGpu,
    ) -> bool {
        let mut changed = false;
        if let Some(dialog) = self.instance_dialog.as_mut() {
            let (dirty, saved) = dialog.poll();
            let dirty = dialog.tick_picker(now) || dirty;
            changed |= dirty;
            self.dirty |= dirty;
            if saved.is_some() {
                self.instance_saved = saved;
                self.dismiss(now);
            }
        }
        for msg in self.rx.try_iter().collect::<Vec<_>>() {
            changed = true;
            self.dirty = true;
            match msg {
                Update::Data(data) => {
                    self.loaded = true;
                    self.data = data;
                    self.reset_skin();
                    self.load_public_skin();
                    if self.panel == Some(Panel::Friends) {
                        self.social(None);
                    }
                }
                Update::Skin(n, skin) => {
                    if self.skin_pending.as_ref() == Some(&n) {
                        self.skin_pending = None;
                    }
                    if let Some(mut skin) = skin {
                        self.data.official.insert(n.clone(), skin.clone());
                        let selection = self.data.selection(&n);
                        if selection.id.is_empty() || !self.data.skins.contains_key(&n) {
                            if self
                                .data
                                .edits
                                .selected_skins
                                .contains_key(&n.to_ascii_lowercase())
                            {
                                skin.slim = selection.slim;
                            }
                            self.data.skins.insert(n.clone(), skin);
                        }
                        if n == self.data.active {
                            self.reset_skin();
                        }
                    }
                }
                Update::Social(n, result) if n == self.data.active => {
                    self.friend_pending = false;
                    self.friend_mutating = false;
                    self.friend_refresh = Some(now + Duration::from_secs(30));
                    match result {
                        Ok(mut state) => {
                            state
                                .friends
                                .sort_unstable_by_key(|s| s.to_ascii_lowercase());
                            if state
                                .sent
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case(self.friend_target.trim()))
                            {
                                self.friend_target.clear();
                            }
                            let names = state
                                .friends
                                .iter()
                                .chain(&state.sent)
                                .chain(state.requests.iter().map(|r| &r.from))
                                .cloned()
                                .collect();
                            self.friends = Some(state);
                            self.load_friend_heads(names);
                            self.friend_error = None;
                        }
                        Err(e) => self.friend_error = Some(e),
                    }
                }
                Update::Social(..) => {}
                Update::Heads(heads) => {
                    for (name, avatar) in heads {
                        if let Some(slot) = self.friend_heads.get_mut(&name) {
                            *slot = avatar.map(|a| {
                                load_image(
                                    &self.ctx,
                                    "friend-head",
                                    &a,
                                    egui::TextureOptions::NEAREST,
                                )
                            });
                        }
                    }
                }
                Update::Cover(result) => {
                    self.cover_importing = false;
                    match result {
                        Ok((cover, crop)) => {
                            self.data.cover = Some(cover);
                            self.data.crop = crop;
                            self.crop_edit = None;
                            self.profile_error = storage::save_crop(crop).err();
                            self.cover_restart = true;
                            self.image = None;
                        }
                        Err(e) => self.profile_error = Some(e),
                    }
                }
            }
        }
        if visible
            && self.panel == Some(Panel::Friends)
            && self.friend_refresh.is_some_and(|t| t <= now)
        {
            self.social(None);
            self.friend_refresh = None;
            changed = true;
        }
        if visible && self.panel == Some(Panel::Friends) && self.dialog.take_target(now) {
            self.load_friend_heads(vec![self.friend_target.trim().to_owned()]);
        }
        if self.next_paint.is_some_and(|t| t <= now) {
            self.next_paint = None;
            if visible && (self.panel.is_some() || self.console.as_ref().is_some_and(|c| c.view() != ConsoleView::Hidden)) {
                self.dirty = true;
                changed = true;
            }
        }
        if visible && self.rotation.deadline().is_some_and(|t| t <= now) {
            self.dirty = true;
            changed = true;
        }
        if let Some(tube) = &self.preferences_tube {
            if tube.finished(now) {
                self.close();
                changed = true;
            } else if visible && self.tube_tick.is_none_or(|t| t <= now) {
                let animating = tube.animating(now, 0.9);
                self.tube_tick = animating.then_some(now + crate::COMPOSITOR_FRAME_INTERVAL);
                if !animating {
                    self.preferences_tube = None;
                }
                self.dirty = true;
                changed = true;
            }
        }
        if visible && (self.has_tube() || self.panel == Some(Panel::Friends)) && !self.reduced_motion {
            if self.border_tick.is_none_or(|tick| tick <= now) {
                let frame = crate::COMPOSITOR_FRAME_INTERVAL;
                self.border_tick = Some(self.border_tick.map_or(now + frame, |tick| {
                    (tick + frame).max(now + Duration::from_millis(1))
                }));
                self.dirty = true;
                changed = true;
            }
        } else {
            self.border_tick = None;
        }
        let playing = visible && self.panel == Some(Panel::Preferences) && !self.cover_failed;
        if std::mem::take(&mut self.cover_restart) {
            self.cover_failed = false;
            if self.cover_running.take().is_some() {
                if let Some(folder) = bridge {
                    let _ = send_cover(folder, 0);
                }
                if let Some(w) = wake {
                    w.request();
                }
                gpu.clear_cover();
            }
            self.dirty = true;
            changed = true;
        }
        if playing && bridge.is_none() && matches!(self.data.cover, Some(Cover::Video(_))) {
            self.cover_failed = true;
            self.dirty = true;
            changed = true;
        }
        if let (true, Some(folder), Some(Cover::Video(path))) = (playing, bridge, &self.data.cover)
        {
            if let Some((token, started)) = self.cover_running {
                if gpu.cover.is_none() {
                    let result =
                        std::fs::read_to_string(folder.join(format!("profile-result-{token}.txt")));
                    if result.as_deref().is_ok_and(|x| x.starts_with("worker ")) {
                        let attached =
                            Source::read(&folder.join(format!("surface-profile-{token}.txt")))
                                .and_then(|s| gpu.attach_cover(s));
                        if attached.is_err() {
                            self.cover_failed = true;
                        }
                        self.dirty = true;
                        changed = true;
                    } else if result.is_ok() || now - started > Duration::from_secs(12) {
                        self.cover_failed = true;
                        self.dirty = true;
                        changed = true;
                    }
                }
                if gpu.cover_ready() {
                    changed = true;
                }
            } else {
                self.cover_token += 1;
                let token = self.cover_token;
                let data = serde_json::json!({"path":path,"start":self.data.crop.start_time,"duration":self.data.crop.duration});
                let requested = std::fs::write(
                    folder.join(format!("profile-cover-{token}.json")),
                    data.to_string(),
                )
                .and_then(|()| send_cover(folder, token));
                if requested.is_ok() {
                    self.cover_running = Some((token, now));
                    if let Some(w) = wake {
                        w.request();
                    }
                } else {
                    self.cover_failed = true;
                }
                self.dirty = true;
                changed = true;
            }
        }
        if (!playing || bridge.is_none()) && self.cover_running.take().is_some() {
            if let Some(folder) = bridge {
                let _ = send_cover(folder, 0);
            }
            if let Some(w) = wake {
                w.request();
            }
            gpu.clear_cover();
            self.dirty = true;
            changed = true;
        }
        changed
    }
    pub fn deadline(&self) -> Option<Instant> {
        let modal = if self.panel == Some(Panel::Friends) {
            self.friend_refresh
                .into_iter()
                .chain(self.dialog.target_deadline())
                .min()
        } else {
            self.cover_running
                .map(|(_, t)| t + Duration::from_secs(12))
                .filter(|&t| t > Instant::now())
        };
        let oneko_wake = self.oneko.config.enabled.then(|| self.oneko.next_tick_deadline());
        modal
            .into_iter()
            .chain(self.next_paint)
            .chain(self.border_tick)
            .chain(self.tube_tick)
            .chain(self.rotation.deadline())
            .chain(self.console.as_ref().and_then(console::Console::deadline))
            .chain(oneko_wake)
            .min()
    }

    pub fn poll_oneko(&mut self, window: &Window, now: Instant) -> bool {
        if !self.oneko.config.enabled {
            return false;
        }
        let perch = self.console.as_ref().and_then(|c| c.minigame_perch(window));
        if self.oneko.update_minigame_perch(perch) {
            self.dirty = true;
            return true;
        }
        let size = window.inner_size();
        let dpi = window.scale_factor() as f32;
        if self.oneko.tick(now, size.width as f32 / dpi, size.height as f32 / dpi) {
            self.dirty = true;
            return true;
        }
        false
    }

    pub fn oneko_covers(&self, window: &Window, p: winit::dpi::PhysicalPosition<f64>) -> bool {
        if !self.oneko.config.enabled {
            return false;
        }
        let dpi = window.scale_factor();
        let logical = pos2((p.x / dpi) as f32, (p.y / dpi) as f32);
        self.oneko.covers(logical)
    }

    pub fn oneko_event(
        &mut self,
        window: &Window,
        pointer: Option<winit::dpi::PhysicalPosition<f64>>,
        event: &WindowEvent,
    ) -> bool {
        if !self.oneko.config.enabled {
            return false;
        }
        let dpi = window.scale_factor();
        let size = window.inner_size();
        let win_w = size.width as f32 / dpi as f32;
        let win_h = size.height as f32 / dpi as f32;

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let perch = self.console.as_ref().and_then(|c| c.minigame_perch(window));
                self.oneko.update_minigame_perch(perch);
                let logical = pos2((position.x / dpi) as f32, (position.y / dpi) as f32);
                let (consumed, redraw) = self.oneko.on_cursor_moved(logical, win_w, win_h);
                if consumed {
                    if redraw {
                        self.dirty = true;
                        window.request_redraw();
                    }
                    return true;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(p) = pointer else { return false };
                let logical = pos2((p.x / dpi) as f32, (p.y / dpi) as f32);
                let handled = self.oneko.on_mouse_input(state.is_pressed(), *button, logical);
                if handled {
                    self.dirty = true;
                    window.request_redraw();
                    return true;
                }
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                if self.oneko.on_lost_capture() {
                    window.set_cursor(winit::window::CursorIcon::Default);
                    self.dirty = true;
                    window.request_redraw();
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub fn poll_console(&mut self, view: ConsoleView, home: Option<&crate::home::Home>, now: Instant) -> bool {
        let changed = self.console.as_mut().is_some_and(|console| {
            console.tick(view, home.and_then(crate::home::Home::console_instance), now)
        });
        self.dirty |= changed;
        changed
    }

    pub fn console_glass(&self, window: &Window, view: ConsoleView) -> Option<Rect> {
        self.console.as_ref()?.glass(window, view)
    }

    pub fn console_covers(&self, window: &Window, p: winit::dpi::PhysicalPosition<f64>) -> bool {
        let dpi = window.scale_factor();
        self.console.as_ref().is_some_and(|console| {
            console.contains(pos2((p.x / dpi) as f32, (p.y / dpi) as f32))
        })
    }

    pub fn console_captured(&self) -> bool {
        self.console.as_ref().is_some_and(|c| c.captured())
    }

    pub fn take_console_action(&mut self) -> Option<ConsoleAction> {
        self.console_action.take()
    }

    pub fn console_event(
        &mut self,
        window: &Window,
        pointer: Option<winit::dpi::PhysicalPosition<f64>>,
        event: &WindowEvent,
    ) -> bool {
        let over = pointer.is_some_and(|p| self.console_covers(window, p));
        let Some(console) = &mut self.console else { return false };
        if console.view() == ConsoleView::Hidden || matches!(event, WindowEvent::RedrawRequested) {
            return false;
        }
        let captured = console.captured();
        let popup = egui::Popup::is_any_open(&self.ctx);
        let logical = pointer.map(|p| {
            let dpi = window.scale_factor();
            pos2((p.x/dpi) as f32, (p.y/dpi) as f32)
        });
        if console.dino_event(window, logical, event, !self.ctx.egui_wants_keyboard_input(), popup) {
            let perch = console.minigame_perch(window);
            self.oneko.update_minigame_perch(perch);
            self.dirty = true;
            window.request_redraw();
            return true;
        }
        let pointer_event = matches!(event, WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::CursorMoved { .. });
        let keyboard = matches!(event, WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_));
        let handle = (pointer_event && (over || captured || popup))
            || (keyboard && (self.ctx.egui_wants_keyboard_input() || popup || console.view() == ConsoleView::Detail));
        if (matches!(event, WindowEvent::Focused(false)) || !handle && matches!(event, WindowEvent::MouseInput { state, .. } if state.is_pressed()))
            && console.pause_game()
        {
            let perch = console.minigame_perch(window);
            self.oneko.update_minigame_perch(perch);
            self.dirty = true;
            window.request_redraw();
        }
        if let WindowEvent::MouseInput { state, .. } = event {
            console.capture(state.is_pressed() && handle);
        }
        let state = self.input.get_or_insert_with(|| Self::input(&self.ctx, window));
        // Feed pointer exit and focus changes as well so hover, selection and
        // pressed state are cleared when the launcher takes the next event.
        if handle || matches!(event, WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } | WindowEvent::Focused(_) | WindowEvent::ModifiersChanged(_))
            || matches!(event, WindowEvent::MouseInput { .. }) && self.ctx.egui_wants_keyboard_input()
        {
            let event = state.on_window_event(window, event);
            if event.repaint {
                self.dirty = true;
                window.request_redraw();
            }
        }
        handle
    }
    fn load_friend_heads(&mut self, names: Vec<String>) {
        let mut fetch = Vec::new();
        for name in names {
            let key = name.to_ascii_lowercase();
            if self.friend_heads.len() >= 160
                || !crate::minecraft::valid_username(&name)
                || self.friend_heads.contains_key(&key)
            {
                continue;
            }
            self.friend_heads.insert(key.clone(), None);
            fetch.push((key, name));
        }
        if fetch.is_empty() {
            return;
        }
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            for (key, name) in fetch {
                let _ = tx.send(Update::Heads(vec![(key, social::public_head(&name))]));
                let _ = wake.send_event(());
            }
        });
    }
    pub fn exposes_chrome_at(&self, window: &Window, p: winit::dpi::PhysicalPosition<f64>) -> bool {
        let dpi = window.scale_factor() as f32;
        matches!(self.panel, Some(Panel::Preferences | Panel::Friends))
            && !self
                .dialog
                .contains(pos2(p.x as f32 / dpi, p.y as f32 / dpi))
            && !self.ctx.egui_is_using_pointer()
    }
    pub fn take_app_action(&mut self) -> Option<AppAction> {
        self.app_action.take()
    }
    fn change_avatar(&mut self, window: &Window, restore: bool) {
        let Some(a) = self
            .data
            .accounts
            .iter_mut()
            .find(|a| a.username == self.data.active)
        else {
            return;
        };
        let value = if restore {
            String::new()
        } else {
            let Some(path) = file_dialog_for(window, FileKind::Avatar) else {
                return;
            };
            let bytes = path
                .metadata()
                .ok()
                .filter(|m| m.len() <= 4 * 1024 * 1024)
                .and_then(|_| std::fs::read(path).ok())
                .filter(|b| storage::rgba(b, 1024).is_some());
            let Some(bytes) = bytes else {
                self.profile_error =
                    Some("Elige una imagen PNG, JPG o WebP de hasta 4 MB y 1024 píxeles.".into());
                self.dirty = true;
                return;
            };
            use base64::Engine;
            format!(
                "data:image/unknown;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        };
        let key = a.username.to_ascii_lowercase();
        let old = self.data.edits.avatars.insert(key.clone(), value.clone());
        match self.data.edits.save() {
            Ok(()) => {
                a.custom_avatar_url = value;
                self.avatar = None;
                self.profile_error = None;
            }
            Err(error) => {
                if let Some(old) = old {
                    self.data.edits.avatars.insert(key, old);
                } else {
                    self.data.edits.avatars.remove(&key);
                }
                self.profile_error = Some(error);
            }
        }
        self.dirty = true;
    }

    pub fn event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        // egui-winit reports repaint=true for RedrawRequested. Forwarding
        // that event would make each draw queue another draw with a panel open.
        if matches!(event, WindowEvent::RedrawRequested) {
            return false;
        }
        if self.panel.is_none() {
            if matches!(
                event,
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
            ) {
                self.dirty = true;
            }
            return false;
        }
        if matches!(event,WindowEvent::KeyboardInput{event,..}
            if event.state.is_pressed() && event.logical_key==Key::Named(NamedKey::Escape))
        {
            if self.friend_remove.take().is_some() || self.crop_edit.take().is_some()
                || self.instance_dialog.as_mut().is_some_and(instance_ui::Dialog::close_art_picker)
            {
                self.dirty = true;
            } else {
                self.dismiss(Instant::now());
            }
            window.request_redraw();
            return true;
        }
        if (self.preferences_tube.as_ref().is_some_and(|tube| {
            tube.closing() || tube.opening_before(Instant::now(), 0.42)
        }) || self.instance_dialog.as_ref().is_some_and(|dialog| dialog.picker_blocks_input(Instant::now()))) && matches!(
            event,
            WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::Ime(_)
        ) {
            return true;
        }
        let state = self
            .input
            .get_or_insert_with(|| Self::input(&self.ctx, window));
        let mut defer = false;
        if self.rotation.dragging() {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let now = Instant::now();
                    let dpi = window.scale_factor();
                    self.rotation.move_to(
                        pos2((position.x / dpi) as f32, (position.y / dpi) as f32),
                        now,
                    );
                    defer = self.rotation.deadline().is_none_or(|t| t > now);
                    // All motion updates the pose, but egui only needs the most recent
                    // position to paint hover state at the next visible frame.
                    let events = &mut state.egui_input_mut().events;
                    if matches!(events.last(), Some(egui::Event::PointerMoved(_))) {
                        events.pop();
                    }
                }
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Released,
                    button: winit::event::MouseButton::Left,
                    ..
                }
                | WindowEvent::Focused(false)
                | WindowEvent::ScaleFactorChanged { .. } => self.rotation.end(),
                _ => {}
            }
        }
        let result = state.on_window_event(window, event);
        if result.repaint && !defer {
            self.dirty = true;
            window.request_redraw();
        }
        matches!(
            event,
            WindowEvent::MouseInput { .. }
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorLeft { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::Ime(_)
        )
    }
    fn input(ctx: &egui::Context, window: &Window) -> egui_winit::State {
        egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(4096),
        )
    }
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn webview_clock(&self) -> Option<(crate::home_widgets::Style, bool)> {
        self.data.origin_found.then_some((self.data.clock_style, self.data.clock_24h))
    }

    pub fn webview_favorites(&self) -> Option<&[String]> {
        self.data.origin_found.then_some(self.data.favorites.as_slice())
    }

    pub fn paint(
        &mut self,
        window: &Window,
        gpu: &mut PaintGpu,
        show_dock: bool,
        dim: f32,
        elapsed: Duration,
        widgets: Option<(&crate::home_widgets::Widgets, &crate::home::Home)>,
        show_panels: bool,
        clock_occlusion: ClockOcclusion,
        console_glass: bool,
    ) {
        let show_dock = show_dock && !self.data.active.is_empty();
        if self.dock_visible != show_dock {
            self.dock_visible = show_dock;
            self.dirty = true;
        }
        if self.clock_occlusion != clock_occlusion {
            self.clock_occlusion = clock_occlusion;
            self.dirty = true;
        }
        let new_pose = self.panel == Some(Panel::Profile) && self.rotation.take_frame(Instant::now());
        if !self.dirty && !new_pose {
            return;
        }
        self.dirty = false;
        if !show_dock && self.panel.is_none() && widgets.is_none()
            && self.console.as_ref().is_none_or(|console| console.view() == ConsoleView::Hidden)
        {
            gpu.clear();
            return;
        }
        if self.art.is_none() {
            windows_ui::install_fonts(&self.ctx);
            self.art = Some(windows_ui::Art::new(&self.ctx));
        }
        if self.avatar.is_none()
            && let Some(a) = self
                .data
                .accounts
                .iter()
                .find(|a| a.username == self.data.active)
        {
            let (png, slim) = self.data.skins.get(&a.username).map_or(
                (include_bytes!("profile/steve.png").as_slice(), false),
                |s| (s.png.as_slice(), s.slim),
            );
            if let Some(skin) = storage::rgba(png, 1024) {
                let custom = storage::data_url(&a.custom_avatar_url, 4 * 1024 * 1024)
                    .and_then(|b| storage::rgba(&b, 1024));
                self.avatar_is_skin = custom.is_none();
                let avatar = custom.unwrap_or_else(|| model::head(&skin));
                self.avatar = Some(load_image(
                    &self.ctx,
                    "profile-head",
                    &avatar,
                    if self.avatar_is_skin {
                        egui::TextureOptions::NEAREST
                    } else {
                        egui::TextureOptions::LINEAR
                    },
                ));
                self.skin_image = Some((skin, slim));
            }
        }
        if self.panel == Some(Panel::Profile) && (self.model.is_none() || new_pose) {
            self.draw_model();
        }
        if (self.panel == Some(Panel::Preferences) || show_dock)
            && self.image.is_none()
            && let Some(Cover::Image(image)) = &self.data.cover
        {
            self.image = Some(load_image(
                &self.ctx,
                "preferences-cover",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
        let ctx = self.ctx.clone();
        let mut input = self
            .input
            .get_or_insert_with(|| Self::input(&ctx, window))
            .take_egui_input(window);
        input.focused = window.has_focus();
        let mut commands = windows_ui::Actions::default();
        let mut skin_menu = windows_ui::SkinMenuActions::default();
        let animation = self
            .preferences_tube
            .as_ref()
            .map(|tube| tube.frame(Instant::now()));
        let mut preferences_shapes = Vec::new();
        let mut picker_shapes = Vec::new();
        let mut picker_surface = None;
        let picker_animation = self.instance_dialog.as_ref().and_then(|d| d.picker_frame(Instant::now()));
        let mut close_instance = false;
        let collection_art = gpu.collection_art;
        let elapsed = if self.reduced_motion { Duration::ZERO } else { elapsed };
        let border_phase = crate::dock::wallpaper_stroke_phase(elapsed);
        let output = ctx.run_ui(input, |ui| {
            if let Some((widgets, _home)) = widgets {
                let now = Instant::now();
                widgets.paint_home_clock(ui.painter(), window, now, show_panels, clock_occlusion);
            }
            if show_dock {
                self.paint_dock(ui, window);
            }
            if let Some(console) = &mut self.console {
                self.console_action = console.paint(ui, window, console_glass, Instant::now()).or(self.console_action.take());
            }
            if let Some((widgets, home)) = widgets {
                let now = Instant::now();
                if show_panels {
                    let panels_layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("home-sliding-panels"));
                    let panels_painter = ui.painter().clone().with_layer_id(panels_layer);
                    widgets.paint(&panels_painter, window, home, collection_art, now);
                }
            }
            if self.oneko.config.enabled {
                let perch = self.console.as_ref().and_then(|c| c.minigame_perch(window));
                if self.oneko.update_minigame_perch(perch) {
                    self.dirty = true;
                }
                self.oneko.paint(ui);
            }
            let Some(panel) = self.panel else { return };
            if let Some(instance) = self.instance_dialog.as_mut() {
                let (close, surface, picker) = instance.paint(
                    &ctx, ui, window, border_phase, elapsed, self.reduced_motion,
                    animation.as_ref().map_or(1., |frame| frame.backdrop),
                );
                self.dialog.surface = Some(surface);
                close_instance = close;
                picker_surface = picker;
                if picker.is_some() {
                    picker_shapes = take_layer(&ctx, instance_ui::Dialog::picker_layer());
                }
            } else if panel == Panel::Profile {
                skin_menu = self.paint_skin_menu(&ctx, ui, window);
            } else {
                commands = self.paint_dialog(&ctx, ui, window, gpu, panel, dim, border_phase);
            }
            if animation.is_some() {
                preferences_shapes = take_layer(&ctx, windows_ui::preferences_layer());
            }
        });
        let console_visible = self.console.as_ref().is_some_and(|console| console.view() != ConsoleView::Hidden);
        if let Some(input) = self.input.as_mut() {
            if self.panel.is_some() || console_visible {
                let is_grabbing = output.platform_output.cursor_icon == egui::CursorIcon::Grabbing;
                input.handle_platform_output(window, output.platform_output);
                if is_grabbing && let Some(cursor) = &self.grabbing_cursor {
                    window.set_cursor(cursor.clone());
                }
            }
        }
        let mut jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        let mut tube = animation.zip(self.dialog.surface).map(|(frame, area)| {
            let mut panel = ctx.tessellate(preferences_shapes, output.pixels_per_point);
            tube::transform(&mut panel, area.center(), frame.layers.panel);
            if frame.layers.panel[2] > 0.0 {
                jobs.extend(panel);
            }
            tube::Paint::new(area, output.pixels_per_point, frame.layers)
        });
        if let Some(area) = picker_surface {
            let screen = ctx.content_rect();
            let backdrop = picker_animation.as_ref().map_or(1., |f| f.backdrop);
            let veil = egui::epaint::ClippedShape {
                clip_rect: screen,
                shape: egui::Shape::rect_filled(screen, 0., Color32::from_black_alpha((166. * backdrop).round() as u8)),
            };
            jobs.extend(ctx.tessellate(vec![veil], output.pixels_per_point));
            let mut panel = ctx.tessellate(picker_shapes, output.pixels_per_point);
            if let Some(frame) = picker_animation {
                tube::transform(&mut panel, area.center(), frame.layers.panel);
                if frame.layers.panel[2] > 0. {
                    jobs.extend(panel);
                }
                tube = Some(tube::Paint::new(area, output.pixels_per_point, frame.layers));
            } else {
                jobs.extend(panel);
            }
        }
        gpu.set_tube(tube);
        let model = (self.panel == Some(Panel::Profile))
            .then(|| self.model.as_ref().map(model::PreviewTexture::id))
            .flatten();
        gpu.update(jobs, output.textures_delta, output.pixels_per_point, model);
        gpu.set_dragging_model(skin_menu.dragging);
        if close_instance {
            self.dismiss(Instant::now());
            window.request_redraw();
        }
        if (self.panel.is_some() || console_visible)
            && let Some(root) = output.viewport_output.get(&egui::ViewportId::ROOT)
        {
            if root.repaint_delay < Duration::from_secs(10) {
                self.next_paint =
                    Instant::now().checked_add(root.repaint_delay.max(Duration::from_millis(16)));
            }
        }
        if let Some(n) = commands.select_account.take() {
            self.select(n);
        }
        if commands.avatar || commands.avatar_clear {
            self.change_avatar(window, commands.avatar_clear);
        }
        if commands.sign_out {
            self.select(String::new());
        }
        if commands.save_geometry
            && let Err(error) = self.data.preferences_geometry.save()
        {
            self.profile_error = Some(error);
            self.dirty = true;
        }
        if commands.app.is_some() {
            self.app_action = commands.app;
        }
        if commands.browse_instances
            && let Some(path) = pick_instance_folder(window)
        {
            self.profile_error =
                storage::save_native("instance-folder.txt", path.to_string_lossy().as_bytes())
                    .err();
            if self.profile_error.is_none() {
                self.app_action = Some(AppAction::InstanceFolderChanged);
            }
            self.dirty = true;
        }
        if let Some(n) = skin_menu.select {
            self.select(n);
        }
        if skin_menu.sign_out {
            self.select(String::new());
        }
        if skin_menu.slide != 0 {
            self.slot(skin_menu.slide);
        }
        if let Some(slim) = skin_menu.slim {
            self.preview_skin.slim = slim;
            self.preview_image = None;
            self.model = None;
            self.dirty = true;
        }
        if skin_menu.activate {
            self.activate_skin();
        }
        if skin_menu.delete_skin {
            self.delete_preview_skin();
        }
        if skin_menu.upload {
            self.upload_skin(window);
        }
        if commands.cover {
            self.upload_cover(window);
        }
        if commands.crop {
            self.crop_edit = Some(self.data.crop);
            self.dirty = true;
        }
        if commands.crop_cancel {
            self.crop_edit = None;
            self.dirty = true;
        }
        if commands.crop_done
            && let Some(crop) = self.crop_edit.take()
        {
            match storage::save_crop(crop) {
                Ok(()) => {
                    self.cover_restart = self.data.crop.start_time != crop.start_time
                        || self.data.crop.duration != crop.duration;
                    self.data.crop = crop;
                    self.profile_error = None;
                }
                Err(e) => {
                    self.crop_edit = Some(crop);
                    self.profile_error = Some(e);
                }
            }
            self.dirty = true;
        }
        if skin_menu.add_account || commands.add {
            self.add_account();
        }
        if commands.identity_save {
            self.save_identity();
        }
        if let Some(action) = commands.friend {
            self.social(Some(action));
        }
        if skin_menu.close || commands.close {
            self.dismiss(Instant::now());
        }
        if self.dirty {
            window.request_redraw();
        }
    }
    fn start_model_drag(&mut self, window: &Window, position: Pos2, delta: Vec2) {
        let interval = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .filter(|&rate| rate > 0)
            .map_or(crate::COMPOSITOR_FRAME_INTERVAL, |rate| {
                Duration::from_nanos(1_000_000_000_000 / u64::from(rate))
            });
        let now = Instant::now();
        self.rotation.begin(position, interval);
        self.rotation.add(delta, now);
        if self.rotation.take_frame(now) {
            self.draw_model();
        }
    }

    fn draw_model(&mut self) {
        if self.preview_image.is_none() {
            let name = &self.data.active;
            let own = self.data.selection(name);
            let preview = if self.preview_skin.id.is_empty() {
                self.data.official.get(name).cloned()
            } else {
                self.data
                    .wardrobe
                    .get(name)
                    .and_then(|s| s.iter().find(|s| s.id == self.preview_skin.id))
                    .and_then(|s| s.decode(self.preview_skin.slim))
            };
            self.preview_image = if self.preview_empty {
                Some(model::mannequin())
            } else {
                preview
                    .as_ref()
                    .and_then(|s| storage::rgba(&s.png, 1024))
                    .or_else(|| {
                        (own.id == self.preview_skin.id)
                            .then(|| self.skin_image.as_ref().map(|(i, _)| i.clone()))
                            .flatten()
                    })
                    .or_else(|| storage::rgba(include_bytes!("profile/steve.png"), 1024))
            };
        }
        if let Some(skin) = &self.preview_image {
            let frame = model::render(
                skin,
                self.preview_skin.slim,
                self.rotation.angles[0],
                self.rotation.angles[1],
                256,
            );
            if let Some(model) = &mut self.model {
                model.update(frame);
            } else {
                self.model = Some(model::PreviewTexture::new(&self.ctx, frame));
            }
        }
    }
}

enum FileKind {
    Cover,
    Skin,
    Avatar,
}
fn file_dialog(window: &Window, cover: bool) -> Option<std::path::PathBuf> {
    file_dialog_for(
        window,
        if cover {
            FileKind::Cover
        } else {
            FileKind::Skin
        },
    )
}
fn file_dialog_for(window: &Window, kind: FileKind) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        Win32::{Foundation::HWND, UI::Controls::Dialogs::*},
        core::{PWSTR, w},
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let RawWindowHandle::Win32(raw) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let mut path = vec![0u16; 32768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(raw.hwnd.get() as *mut _),
        lpstrFilter: match kind {
            FileKind::Cover => w!(
                "Imágenes y vídeos\0*.png;*.jpg;*.jpeg;*.webp;*.gif;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.ogv\0\0"
            ),
            FileKind::Skin => w!("Textura Minecraft (*.png)\0*.png\0\0"),
            FileKind::Avatar => w!("Imágenes\0*.png;*.jpg;*.jpeg;*.webp\0\0"),
        },
        lpstrTitle: match kind {
            FileKind::Cover => w!("Elegir portada"),
            FileKind::Skin => w!("Cargar textura de Minecraft"),
            FileKind::Avatar => w!("Elegir avatar"),
        },
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER,
        ..Default::default()
    };
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return None;
    }
    let len = path.iter().position(|&c| c == 0)?;
    Some(std::ffi::OsString::from_wide(&path[..len]).into())
}

fn send_cover(folder: &Path, token: u64) -> std::io::Result<()> {
    let part = folder.join("profile-preview.tmp");
    std::fs::write(&part, format!("0 {token} 3 -\n"))?;
    // Windows rename cannot replace a file that another process currently has open.
    let dst = folder.join("profile-preview.txt");
    for _ in 0..3 {
        match std::fs::rename(&part, &dst) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => std::thread::yield_now(),
            Err(e) => return Err(e),
        }
    }
    std::fs::rename(part, dst)
}
const FULL_UV: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(1., 1.),
};
pub(crate) fn load_image(
    ctx: &egui::Context,
    name: &str,
    image: &image::RgbaImage,
    options: egui::TextureOptions,
) -> TextureHandle {
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        options,
    )
}
fn cover_uv(dst: Vec2, src: Vec2, crop: Crop) -> Rect {
    let scale = (dst.x / src.x).max(dst.y / src.y) * crop.zoom;
    let w = (dst.x / (src.x * scale)).min(1.);
    let h = (dst.y / (src.y * scale)).min(1.);
    let (x, y) = ((1. - w) * crop.pos_x / 100., (1. - h) * crop.pos_y / 100.);
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}
fn pick_instance_folder(window: &Window) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        Win32::UI::Shell::{
            BIF_RETURNONLYFSDIRS, BROWSEINFOW, ILFree, SHBrowseForFolderW, SHGetPathFromIDListW,
        },
        core::{PWSTR, w},
    };
    let hwnd = crate::titlebar::window_handle(window).ok()?;
    let mut display = [0u16; 260];
    let mut path = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: hwnd,
        pszDisplayName: PWSTR(display.as_mut_ptr()),
        lpszTitle: w!("Elige la carpeta de instancias de Minecraft"),
        ulFlags: BIF_RETURNONLYFSDIRS,
        ..Default::default()
    };
    unsafe {
        let id = SHBrowseForFolderW(&info);
        if id.is_null() {
            return None;
        }
        let ok = SHGetPathFromIDListW(id, &mut path).as_bool();
        ILFree(Some(id));
        let len = path.iter().position(|&c| c == 0)?;
        let result = std::path::PathBuf::from(std::ffi::OsString::from_wide(&path[..len]));
        (ok && result.is_dir()).then_some(result)
    }
}

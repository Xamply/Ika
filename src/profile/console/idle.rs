//! The two idle games from the original floating console.
use egui::{Align2, Color32 as C, FontFamily, FontId, Key, Painter, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind { #[default] Dino, V }

#[path = "dino.rs"]
mod dino;
use dino::Dino;
pub(super) use dino::Button as DinoButton;

pub struct Game {
    dino: Option<Dino>,
    maze: Maze,
    playing: bool,
    stopped: bool,
}

impl Game {
    pub fn new(kind: Kind) -> Self {
        Self { dino: (kind == Kind::Dino).then(|| Dino::new(None)), maze: Maze::new(),
            playing: false, stopped: false }
    }

    pub fn running(&self) -> bool {
        self.dino.as_ref().map_or(self.playing, Dino::running)
    }

    pub fn pause(&mut self) {
        if let Some(dino) = &mut self.dino { dino.pause(); }
        self.playing = false;
    }

    pub fn interval(&self) -> Option<std::time::Duration> {
        if let Some(dino) = &self.dino { dino.interval() }
        else { self.playing.then_some(crate::COMPOSITOR_FRAME_INTERVAL) }
    }

    pub fn dino_input(&mut self, button: DinoButton, pressed: bool, volume: u8) -> bool {
        self.dino.as_mut().is_some_and(|dino| dino.input(button, pressed, volume))
    }

    pub fn tick(&mut self, dt: f64, volume: u8) -> bool {
        if let Some(dino) = &mut self.dino { return dino.tick(dt, volume); }
        let (lost, sound) = self.maze.tick(dt.min(0.05) as f32);
        if let Some(sound) = sound { chime(sound, volume); }
        if lost {
            self.playing = false;
            self.stopped = true;
        }
        true
    }

    pub fn paint(&mut self, ui: &mut Ui, area: Rect) -> Option<Kind> {
        if !area.is_positive() { return None; }
        let response = ui.interact(area, ui.id().with("idle-console-game"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let p = ui.painter().with_clip_rect(area);
        if let Some(dino) = &mut self.dino {
            dino.paint(&p, area);
            if dino.paused() {
                pause_label(&p, area, "PAUSA", "Haz clic en el juego para continuar");
            }
        } else {
            if response.clicked() { response.request_focus(); }
            let keys = |key| response.has_focus() && ui.input(|i| i.key_pressed(key));
            if (response.clicked() || keys(Key::Space) || keys(Key::ArrowUp)) && !self.playing {
                if self.stopped { self.maze = Maze::new(); }
                self.stopped = false;
                self.playing = true;
                ui.ctx().request_repaint();
            }
            if keys(Key::P) || keys(Key::Escape) { self.playing = false; }
            for (d,key) in [Key::ArrowRight, Key::ArrowUp, Key::ArrowLeft, Key::ArrowDown].into_iter().enumerate() {
                if keys(key) {
                    self.maze.desired = d;
                    if !self.stopped {
                        self.playing = true;
                        ui.ctx().request_repaint();
                    }
                }
            }
            self.maze.paint(&p, area);
            if !self.playing {
                pause_label(&p, area, if self.stopped { "FIN DE LA PARTIDA" } else { "EN PAUSA" }, "Espacio o clic para jugar");
            }
        }
        let mut chosen = None;
        if !self.running() {
            response.context_menu(|ui| {
                for (kind, text) in [(Kind::Dino, "Dino"), (Kind::V, ":v")] {
                    if ui.selectable_label(self.dino.is_some() == (kind == Kind::Dino), text).clicked() {
                        if self.dino.is_some() != (kind == Kind::Dino) { chosen = Some(kind); }
                        ui.close();
                    }
                }
            });
        }
        chosen
    }
}

fn pause_label(p: &Painter, area: Rect, title: &str, hint: &str) {
    let font = |size| FontId::new(size, FontFamily::Name("Silkscreen".into()));
    let title_text = if title == "EN PAUSA" || title == "PAUSA" {
        "PAUSA"
    } else {
        title
    };
    let center = area.center();
    let title_galley = p.layout_no_wrap(title_text.into(), font(15.), C::WHITE);
    let hint_galley = p.layout_no_wrap(hint.into(), font(12.), C::WHITE);
    let title_h = title_galley.size().y;
    let top = center.y - (title_h + 8. + hint_galley.size().y) / 2.;

    let title_pos = pos2(center.x - title_galley.size().x / 2., top);
    p.galley(title_pos, title_galley, C::WHITE);

    let hint_pos = pos2(center.x - hint_galley.size().x / 2., top + title_h + 8.);
    p.galley(hint_pos, hint_galley, C::WHITE);
}

const W: usize = 28;
const H: usize = 31;
const SPAWN: usize = 23*W+13;
const HOME: [usize;4] = [11*W+13,14*W+13,14*W+11,14*W+16];
const DIR: [(isize,isize);4] = [(1,0),(0,-1),(-1,0),(0,1)];
const MAP: &str = include_str!("maze.txt");

struct Maze {
    tiles: [u8; W*H],
    dots: [u8; W*H],
    player: usize,
    desired: usize,
    direction: usize,
    ghosts: [usize;4],
    score: u32,
    lives: u8,
    power: u8,
    step: u32,
    elapsed: f32,
}

impl Maze {
    fn new() -> Self {
        let mut tiles = [b'_';W*H];
        let mut dots = [0;W*H];
        for (y,row) in MAP.lines().enumerate() {
            for (x, tile) in row.bytes().enumerate() {
                let i = y*W+x;
                tiles[i] = tile;
                dots[i] = if tile == b'o' { 2 } else { u8::from(tile == b'.') };
            }
        }
        Self { tiles,dots,player:SPAWN,desired:2,direction:2,ghosts:HOME,score:0,lives:3,power:0,step:0,elapsed:0. }
    }
    fn neighbor(&self, at: usize, d: usize, ghost: bool) -> Option<usize> {
        let (dx,dy)=DIR[d];
        let y = (at/W).checked_add_signed(dy).filter(|&y| y<H)?;
        let x = at%W;
        let x = if y==14 { (x as isize+dx).rem_euclid(W as isize) as usize } else { x.checked_add_signed(dx).filter(|&x| x<W)? };
        let i = y*W+x;
        let tag = self.tiles[i];
        let pen = (11..=16).contains(&x) && (13..=15).contains(&y);
        (tag != b'#' && tag != b'_' && (ghost || (tag != b'-' && !pen))).then_some(i)
    }
    fn distance(&self, at: usize) -> [u16;W*H] {
        let mut out = [u16::MAX; W*H];
        out[at] = 0;
        let mut todo = std::collections::VecDeque::from([at]);
        while let Some(i) = todo.pop_front() {
            for d in 0..4 {
                if let Some(n) = self.neighbor(i,d,true) && out[n]==u16::MAX {
                    out[n] = out[i]+1;
                    todo.push_back(n);
                }
            }
        }
        out
    }
    fn tick(&mut self, dt: f32) -> (bool, Option<usize>) {
        self.elapsed += dt;
        if self.elapsed < 0.115 { return (false,None); }
        self.elapsed -= 0.115;
        self.step += 1;
        self.power = self.power.saturating_sub(1);
        if let Some(at) = self.neighbor(self.player,self.desired,false) {
            self.direction = self.desired;
            self.player = at;
        } else if let Some(at) = self.neighbor(self.player,self.direction,false) { self.player = at; }
        let previous = self.dots[self.player];
        self.dots[self.player] = 0;
        self.score += match previous { 1=>10, 2=>50, _=>0 };
        if previous==2 { self.power=70; }
        let route = self.distance(self.player);
        for i in 0..4 {
            if self.step < 7 + 9*i as u32 || (self.step+i as u32)%2 != 0 { continue; }
            let next = (0..4).filter_map(|d| self.neighbor(self.ghosts[i],(d+i)%4,true));
            let destination = if self.power > 0 { next.max_by_key(|&at| route[at]) } else {
                next.min_by_key(|&at| route[at].saturating_add(if self.ghosts.contains(&at) { 3 } else { 0 }))
            };
            if let Some(at) = destination { self.ghosts[i]=at; }
        }
        if let Some(i) = self.ghosts.iter().position(|&at| at==self.player) {
            if self.power == 0 {
                self.lives -= 1;
                self.player = SPAWN;
                self.ghosts = HOME;
                self.step = 0;
                return (self.lives==0,Some(2));
            }
            self.ghosts[i] = HOME[i];
            self.score += 200;
        }
        if !self.dots.iter().any(|&dot| dot!=0) {
            let (score, lives)=(self.score,self.lives);
            *self=Self::new();
            self.score=score;
            self.lives=lives;
        }
        (false, (previous>0).then_some(0))
    }
    fn paint(&self, p: &Painter, area: Rect) {
        let tile=(area.width()/31.).min((area.height()-26.)/31.).max(2.);
        let rect=Rect::from_center_size(area.center()+vec2(0., -9.),vec2(W as f32,H as f32)*tile);
        let color=C::from_rgb(41,68,243);
        for (i, &tag) in self.tiles.iter().enumerate() {
            let center = rect.min + vec2((i%W) as f32+0.5,(i/W) as f32+0.5)*tile;
            if tag == b'#' {
                p.rect_filled(Rect::from_center_size(center,Vec2::splat(tile+0.1)),1.,C::from_rgb(25,25,91));
                for (dx,dy) in DIR {
                    let x=(i%W).checked_add_signed(dx);
                    let y=(i/W).checked_add_signed(dy);
                    let wall = x.zip(y).is_some_and(|(x,y)| x<W && y<H && self.tiles[y*W+x]==b'#');
                    if !wall {
                        let n=vec2(dx as f32,dy as f32)*tile*0.45;
                        let t=vec2(-n.y,n.x);
                        p.line_segment([center+n-t,center+n+t],Stroke::new((tile*0.17).max(1.),color));
                    }
                }
            } else if self.dots[i] != 0 {
                p.circle_filled(center, tile*if self.dots[i]==2 {0.29} else {0.11},C::from_rgb(255,206,166));
            }
        }
        for (i, at) in std::iter::once(self.player).chain(self.ghosts).enumerate() {
            let c=rect.min+vec2((at%W) as f32+0.5,(at/W) as f32+0.5)*tile;
            let tint=if i>0 && self.power>0 { color } else {
                [C::from_rgb(255,226,65),C::from_rgb(236,54,84),C::from_rgb(255,129,199),
                 C::from_rgb(51,224,226),C::from_rgb(244,150,48)][i]
            };
            p.circle_filled(c, tile*0.45,tint);
            if i==0 {
                let (x,y)=DIR[self.direction];
                let d=vec2(x as f32,y as f32)*tile*0.53;
                let n=vec2(-d.y,d.x)*0.48;
                p.add(egui::Shape::convex_polygon(vec![c,c+d+n,c+d-n],C::from_rgb(10,7,24),Stroke::NONE));
            } else {
                for dx in [-0.16,0.16] { p.circle_filled(c+vec2(dx,-0.08)*tile,tile*0.11,C::WHITE); }
            }
        }
        p.text(pos2(area.center().x,area.bottom()-7.),Align2::CENTER_BOTTOM,
            format!("{:05}     VIDAS {}",self.score,self.lives),FontId::new(11., FontFamily::Name("Silkscreen".into())),C::from_rgb(255,215,58));
    }
}

fn chime(kind: usize, volume: u8) {
    if volume == 0 { return; }
    use std::sync::OnceLock;
    use windows::{
        Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
        core::PCWSTR,
    };
    static SOUNDS: [OnceLock<Vec<u8>>;30] = [const { OnceLock::new() };30];
    let level = usize::from(volume.div_ceil(10)).clamp(1,10);
    let waves = SOUNDS[kind*10+level-1].get_or_init(|| {
        let count: u32 = if kind==0 { 690 } else { 1520 };
        let mut wave = Vec::with_capacity(44+count as usize);
        let mut header = |key: &[u8], value: u32| { wave.extend(key); wave.extend(value.to_le_bytes()); };
        header(b"RIFF",36+count);
        header(b"WAVEfmt ",16);
        wave.extend([1,0,1,0]);
        wave.extend(22_050u32.to_le_bytes());
        wave.extend(22_050u32.to_le_bytes());
        wave.extend([1,0,8,0]);
        wave.extend(b"data");
        wave.extend(count.to_le_bytes());
        let frequency = [475., 675., 215.][kind];
        for i in 0..count {
            let t=i as f32/count as f32;
            let bend=if kind==2 { 1.-t*0.56 } else { 1.+t*0.26 };
            let signal=(i as f32/22_050.*frequency*bend*std::f32::consts::TAU).sin();
            wave.push((128.+signal*level as f32*3.7*(1.-t)) as u8);
        }
        wave
    });
    // The cached wave outlives WinMM's asynchronous playback.
    unsafe { let _=PlaySoundW(PCWSTR(waves.as_ptr().cast()),None,SND_MEMORY|SND_ASYNC|SND_NODEFAULT); }
}

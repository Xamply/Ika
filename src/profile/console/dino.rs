//! Native Dino runner. Portado de la versión WebView2 legacy.
#[path = "dino/sound.rs"]
mod sound;

use egui::{Align2, Color32 as C, FontFamily, FontId, Painter, Rect, TextureHandle, pos2, vec2};
use rand::random;
use std::time::Duration;

const BASE_SPEED: f64 = 2.65;
const DINO_X: f64 = 28.;
const HIGH_SCORE: &str = "dino-high-score.txt";
const FRAME_MS: f64 = 16.6667;
const IDLE_FRAME: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq, Eq)]
enum State { Idle, Running, GameOver }

#[derive(Clone, Copy)]
#[repr(usize)]
enum Sprite { Stand, Blink, Run1, Run2, Crash, Duck1, Duck2, Small, SmallPair, Large, LargePair, Bird1, Bird2, Horizon }

const ATLAS: [[f64; 4]; 14] = [
    [848.,2.,44.,47.], [892.,2.,44.,47.], [936.,2.,44.,47.], [980.,2.,44.,47.],
    [1024.,2.,44.,47.], [1112.,2.,59.,47.], [1171.,2.,59.,47.], [228.,2.,17.,35.],
    [245.,2.,34.,35.], [332.,2.,25.,50.], [357.,2.,50.,50.], [134.,2.,46.,40.],
    [180.,2.,46.,40.], [2.,54.,600.,12.],
];

struct Obstacle { x: f64, above_ground: f64, sprite: Sprite, flap: f64 }

impl Obstacle {
    fn size(&self) -> (f64, f64) {
        let [_,_,w,h] = ATLAS[self.sprite as usize];
        (w,h)
    }
    fn hitbox(&self) -> Hitbox {
        let (x,y,w,h) = match self.sprite {
            Sprite::Small => (3.,4.,11.,29.),
            Sprite::SmallPair => (4.,4.,26.,29.),
            Sprite::Large => (4.,4.,17.,44.),
            Sprite::LargePair => (5.,4.,40.,44.),
            _ => (8.,12.,30.,18.),
        };
        Hitbox(self.x+x,-self.above_ground+y,w,h)
    }
}

struct Hitbox(f64,f64,f64,f64);
impl Hitbox {
    fn overlaps(&self, other: &Self) -> bool {
        self.0 < other.0+other.2 && self.0+self.2 > other.0
            && self.1 < other.1+other.3 && self.1+self.3 > other.1
    }
}

#[derive(Clone, Copy)]
struct Cloud { x: f64, y: f64, scale: f64, speed: f64 }
const CLOUDS: [Cloud;4] = [
    Cloud { x:50.,y:14.,scale:0.85,speed:0.08 }, Cloud { x:190.,y:24.,scale:1.1,speed:0.14 },
    Cloud { x:330.,y:12.,scale:0.75,speed:0.06 }, Cloud { x:470.,y:18.,scale:1.,speed:0.11 },
];

#[derive(Clone, Copy)]
pub(in crate::profile::console) enum Button { Mouse, RightMouse, Space, Up, W, Down, S }
impl Button {
    fn mask(self) -> u8 { 1 << self as u8 }
    fn jump(self) -> bool { matches!(self, Self::Mouse | Self::Space | Self::Up | Self::W) }
    fn duck(self) -> bool { matches!(self, Self::RightMouse | Self::Down | Self::S) }
}

pub(super) struct Dino {
    state: State,
    paused: bool,
    buttons: u8,
    queued: bool,
    y: f64,
    vy: f64,
    grounded: bool,
    ducking: bool,
    distance: f64,
    speed: f64,
    horizon: f64,
    biome: usize,
    score: u32,
    high: u32,
    step_timer: f64,
    step: bool,
    blink_timer: f64,
    blinking: bool,
    flash_timer: f64,
    next_obstacle: f64,
    obstacles: Vec<Obstacle>,
    clouds: [Cloud; 4],
    width: f64,
    sprite: Option<TextureHandle>,
    audio: sound::Audio,
}

impl Dino {
    pub(super) fn new(high: Option<u32>) -> Self {
        let high = high.unwrap_or_else(|| std::fs::read_to_string(super::super::super::storage::native_dir().join(HIGH_SCORE))
            .ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0));
        Self {
            state: State::Idle, paused: false, buttons: 0, queued: false, y: 0., vy: 0.,
            grounded: true, ducking: false, distance: 0., speed: BASE_SPEED, horizon: 0.,
            biome: random::<usize>() % 6, score: 0, high, step_timer: 0., step: false,
            blink_timer: 0., blinking: false, flash_timer: 0., next_obstacle: 280.,
            obstacles: Vec::new(), clouds: CLOUDS, width: 480., sprite: None, audio: Default::default(),
        }
    }

    fn restart(&mut self) {
        self.state = State::Running;
        self.distance = 0.;
        self.score = 0;
        self.speed = BASE_SPEED;
        self.horizon = 0.;
        self.obstacles.clear();
        self.next_obstacle = 280.;
        self.y = 0.;
        self.vy = 0.;
        self.grounded = true;
        self.ducking = false;
        self.queued = false;
        self.flash_timer = 0.;
        self.step_timer = 0.;
        self.step = false;
        self.blink_timer = 0.;
        self.blinking = false;
        self.biome = (self.biome + 1 + random::<usize>() % 5) % 6;
    }

    pub(super) fn interval(&self) -> Option<Duration> {
        if self.paused { None } else if self.state == State::Running { Some(crate::COMPOSITOR_FRAME_INTERVAL) }
        else { Some(IDLE_FRAME) }
    }

    pub(super) fn paused(&self) -> bool { self.paused }
    pub(super) fn running(&self) -> bool { self.state == State::Running && !self.paused }

    pub(super) fn pause(&mut self) {
        if self.state != State::Running || self.paused { return; }
        self.paused = true;
        self.buttons = 0;
        self.queued = false;
        self.ducking = false;
        self.audio.stop();
    }

    fn jumping(&self) -> bool {
        self.buttons & (Button::Mouse.mask() | Button::Space.mask() | Button::Up.mask() | Button::W.mask()) != 0
    }

    fn ducking_held(&self) -> bool {
        self.buttons & (Button::RightMouse.mask() | Button::Down.mask() | Button::S.mask()) != 0
    }

    fn jump(&mut self, volume: u8) {
        if self.state != State::Running { self.restart(); }
        self.queued = true;
        if self.grounded {
            self.queued = false;
            self.ducking = false;
            let full_vy = -8.2 - ((self.speed-BASE_SPEED)*0.25).min(0.6);
            self.vy = if self.jumping() { full_vy } else { -2.2 };
            self.grounded = false;
            self.audio.play(sound::Cue::Jump, volume);
        }
    }

    pub(super) fn input(&mut self, button: Button, pressed: bool, volume: u8) -> bool {
        // Clicking resumes a paused canvas; keyboard presses while paused
        // belong to the launcher, just as they do in the original.
        if self.paused {
            if matches!(button, Button::Mouse) && pressed { self.paused = false; }
            else { return false; }
        }
        let mask = button.mask();
        if pressed {
            let was_down = self.buttons & mask != 0;
            self.buttons |= mask;
            if !was_down {
                if button.jump() {
                    self.jump(volume);
                } else if button.duck() {
                    self.ducking = true;
                    if !self.grounded {
                        let fast_fall = if matches!(button, Button::RightMouse) { 5.0 } else { 5.5 };
                        self.vy = self.vy.max(fast_fall);
                    }
                }
            }
        } else {
            if self.buttons & mask == 0 { return false; }
            self.buttons &= !mask;
            if button.jump() {
                if !self.jumping() && !self.grounded && self.vy < -2.2 { self.vy = -2.2; }
            } else if button.duck() {
                if !self.ducking_held() { self.ducking = false; }
            }
        }
        true
    }

    fn spawn(&mut self) {
        let (sprite, above_ground) = if self.score > 120 && random::<f64>() < 0.35 {
            (Sprite::Bird1, if random::<f64>() < 0.45 { 72. } else { 50. })
        } else {
            let r = random::<f64>();
            if self.score > 120 && r < 0.25 { (Sprite::LargePair, 50.) }
            else if self.score > 60 && r < 0.55 { (Sprite::Large, 50.) }
            else if r < 0.8 { (Sprite::SmallPair, 35.) }
            else { (Sprite::Small, 35.) }
        };
        self.obstacles.push(Obstacle { x: self.width + 20., above_ground, sprite, flap: 0. });
        self.next_obstacle = 280. + (random::<f64>()*200.).floor();
    }

    pub(super) fn tick(&mut self, elapsed: f64, volume: u8) -> bool {
        if self.paused { return false; }
        if self.state == State::Idle || self.state == State::GameOver {
            let was = self.blinking;
            self.blink_timer += elapsed.min(0.25)*1000./FRAME_MS;
            if self.blink_timer > 150. {
                self.blinking = true;
                if self.blink_timer > 170. { self.blinking = false; self.blink_timer = 0.; }
            }
            return was != self.blinking;
        }
        let dt = elapsed.min(0.064)*1000./FRAME_MS;
        if dt <= 0. { return false; }
        self.distance += self.speed*dt;
        self.score = (self.distance/10.).floor() as u32;
        if self.score > 0 && self.score.is_multiple_of(100) && self.flash_timer <= 0. {
            self.flash_timer = 35.;
            self.audio.play(sound::Cue::Score, volume);
        }
        if self.flash_timer > 0. { self.flash_timer -= dt; }
        self.speed = BASE_SPEED + (f64::from(self.score)*0.0015).min(1.8);
        self.horizon -= self.speed*dt;
        let gravity = if self.jumping() && self.vy < 0. { 0.22 } else { 0.38 };
        self.y += self.vy*dt;
        self.vy += gravity*dt;
        if self.y >= 0. && self.vy >= 0. {
            self.y = 0.;
            self.vy = 0.;
            self.grounded = true;
            if self.queued || self.jumping() && !self.ducking { self.jump(volume); }
        }
        self.step_timer += self.speed*dt;
        if self.step_timer >= 16. { self.step_timer = 0.; self.step = !self.step; }
        self.next_obstacle -= self.speed*dt;
        if self.next_obstacle <= 0. { self.spawn(); }
        let y = self.y-47.;
        let dino = if self.ducking { Hitbox(DINO_X+4., y+20., 50., 25.) }
            else { Hitbox(DINO_X+8., y+4., 28., 39.) };
        let mut hit = false;
        for ob in self.obstacles.iter_mut().rev() {
            ob.x -= self.speed*dt;
            if matches!(ob.sprite, Sprite::Bird1 | Sprite::Bird2) {
                ob.flap += dt;
                if ob.flap >= 9. {
                    ob.flap = 0.;
                    ob.sprite = if matches!(ob.sprite, Sprite::Bird1) { Sprite::Bird2 } else { Sprite::Bird1 };
                }
            }
            hit |= dino.overlaps(&ob.hitbox());
        }
        self.obstacles.retain(|ob| ob.x >= -80.);
        if hit {
            self.state = State::GameOver;
            self.audio.play(sound::Cue::Hit, volume);
            if self.score > self.high {
                self.high = self.score;
                let _ = super::super::super::storage::save_native(HIGH_SCORE, self.high.to_string().as_bytes());
            }
        }
        for cloud in &mut self.clouds {
            cloud.x -= self.speed*cloud.speed*dt;
            if cloud.x < -70. { cloud.x = self.width+20.+random::<f64>()*80.; }
        }
        true
    }

    pub(super) fn paint(&mut self, p: &Painter, area: Rect) {
        self.width = f64::from(area.width().floor().max(280.));
        let height = f64::from(area.height().floor().max(120.));
        let width = self.width;
        let ground = height-20.;
        let origin = pos2(area.left().round(), area.top().round());
        super::super::dino_world::draw(p, origin, width, ground, self.biome as f64*1600.+self.distance, self.flash_timer);
        let rect = |x:f64,y:f64,w:f64,h:f64| Rect::from_min_size(origin+vec2(x as f32,y as f32), vec2(w as f32,h as f32));
        for Cloud { x, y, scale:s, .. } in self.clouds {
            let (x,y) = (x.round(),y.round());
            let cloud = C::from_rgba_unmultiplied(226,232,240,199);
            p.rect_filled(rect(x+12.*s,y,22.*s,4.*s),0.,cloud);
            p.rect_filled(rect(x+6.*s,y+4.*s,34.*s,4.*s),0.,cloud);
            p.rect_filled(rect(x,y+8.*s,46.*s,5.*s),0.,cloud);
            p.rect_filled(rect(x+2.*s,y+12.*s,42.*s,1.5*s),0.,C::from_rgba_unmultiplied(192,132,252,102));
        }
        let tex = self.sprite.get_or_insert_with(|| {
            let im = image::load_from_memory(include_bytes!("dino.png")).unwrap().to_rgba8();
            super::super::super::load_image(p.ctx(), "dino-original-atlas", &im, egui::TextureOptions::NEAREST)
        });
        let atlas = tex.size_vec2();
        let image = |sprite: Sprite, x, y, w, h| {
            let [sx,sy,sw,sh] = ATLAS[sprite as usize];
            let uv = Rect::from_min_max(pos2(sx as f32/atlas.x,sy as f32/atlas.y),
                pos2((sx+sw) as f32/atlas.x,(sy+sh) as f32/atlas.y));
            p.image(tex.id(),rect(x,y,w,h),uv,C::WHITE);
        };
        let mut tile = (self.horizon%600.).floor();
        while tile < width {
            image(Sprite::Horizon,tile,ground-4.,600.,12.);
            tile += 600.;
        }
        p.rect_filled(rect(0.,ground+7.,width,1.),0.,C::from_rgba_unmultiplied(168,85,247,102));
        for ob in &self.obstacles {
            let (w,h) = ob.size();
            image(ob.sprite, ob.x.round(), (ground-ob.above_ground).round(), w,h);
        }
        let sprite = match self.state {
            State::GameOver => Sprite::Crash,
            _ if self.ducking => if self.grounded && self.step && self.state == State::Running { Sprite::Duck2 } else { Sprite::Duck1 },
            State::Idle if self.blinking => Sprite::Blink,
            State::Running if self.grounded => if self.step { Sprite::Run2 } else { Sprite::Run1 },
            _ => Sprite::Stand,
        };
        image(sprite,DINO_X,(ground-47.+self.y).round(),if self.ducking {59.} else {44.},47.);
        let font = |size| FontId::new(size, FontFamily::Name("Silkscreen".into()));
        if self.flash_timer%10. < 5. {
            p.text(origin+vec2((width-78.) as f32,19.),Align2::RIGHT_BOTTOM,format!("HI {:05}",self.high),font(11.),C::from_rgb(100,116,139));
            p.text(origin+vec2((width-14.) as f32,19.),Align2::RIGHT_BOTTOM,format!("{:05}",self.score),font(11.),C::from_rgb(0,245,212));
        }
        if self.state == State::GameOver {
            let at = origin+vec2((width/2.) as f32,(ground*0.4).clamp(64.,96.).round() as f32);
            let pink = C::from_rgb(255,42,133);
            let glow = C::from_rgba_unmultiplied(255, 42, 133, 100);
            let galley = p.layout_no_wrap("G A M E   O V E R".into(),font(15.),pink);
            let at = at-vec2(galley.size().x/2.,galley.size().y*0.78);
            for d in [vec2(-1., 0.), vec2(1., 0.), vec2(0., -1.), vec2(0., 1.)] {
                p.galley(at + d, galley.clone(), glow);
            }
            p.galley(at, galley, pink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(dino: &mut Dino) { dino.tick(FRAME_MS/1000.,0); }

    #[test]
    fn press_hold_release_and_airborne_click_match_original_jump() {
        let mut d = Dino::new(Some(0));
        let initial_biome = d.biome;
        assert!(d.input(Button::Mouse,true,0));
        assert_ne!(d.biome,initial_biome);
        frame(&mut d);
        assert!((d.y+8.2).abs()<1e-9 && (d.vy+7.98).abs()<1e-9);
        d.input(Button::Space,true,0);
        d.input(Button::Space,false,0);
        assert!((d.vy+7.98).abs()<1e-9);
        d.input(Button::Mouse,false,0);
        assert_eq!(d.vy,-2.2);
        d.input(Button::Mouse,true,0);
        d.input(Button::Mouse,false,0);
        let mut lifts = 0;
        for _ in 0..55 {
            frame(&mut d);
            if d.y == 0. && !d.grounded { lifts += 1; }
        }
        assert_eq!(lifts,1);
        assert!(d.grounded);
    }

    #[test]
    fn duck_moves_hitbox_below_high_pterodactyl_and_pause_clears_holds() {
        let mut d = Dino::new(Some(u32::MAX));
        d.restart();
        d.input(Button::Down,true,0);
        d.obstacles.push(Obstacle { x:42.,above_ground:72.,sprite:Sprite::Bird1,flap:0. });
        frame(&mut d);
        assert!(d.state == State::Running);
        d.obstacles[0].above_ground = 50.;
        frame(&mut d);
        assert!(d.state == State::GameOver);
        d.input(Button::Mouse,true,0);
        d.pause();
        assert_eq!(d.buttons,0);
        assert!(d.interval().is_none());
        assert!(!d.input(Button::Space,true,0));
        assert!(d.input(Button::Mouse,true,0));
        assert!(!d.paused());
    }
}

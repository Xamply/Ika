//! Procedural scenery from the Dino runner. Portado de la versión WebView2 legacy.
use egui::{Color32 as C, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};

const WORLD_LEN: f64 = 9600.;
struct Building {
    x: f64,
    w: f64,
    h: f64,
    kind: &'static str,
    windows: &'static str,
    water_tower: bool,
}
const NY_BUILDINGS: &[Building] = &[
    Building {
        x: 20.,
        w: 38.,
        h: 62.,
        kind: "brick",
        windows: "yellow",
        water_tower: true,
    },
    Building {
        x: 58.,
        w: 46.,
        h: 86.,
        kind: "artdeco",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 104.,
        w: 34.,
        h: 54.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: false,
    },
    Building {
        x: 138.,
        w: 56.,
        h: 102.,
        kind: "chrysler",
        windows: "blue",
        water_tower: false,
    },
    Building {
        x: 194.,
        w: 40.,
        h: 72.,
        kind: "glass",
        windows: "cyan",
        water_tower: false,
    },
    Building {
        x: 234.,
        w: 36.,
        h: 58.,
        kind: "brick",
        windows: "yellow",
        water_tower: true,
    },
    Building {
        x: 270.,
        w: 64.,
        h: 110.,
        kind: "empire",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 334.,
        w: 38.,
        h: 66.,
        kind: "artdeco",
        windows: "amber",
        water_tower: false,
    },
    Building {
        x: 372.,
        w: 44.,
        h: 78.,
        kind: "glass",
        windows: "blue",
        water_tower: false,
    },
    Building {
        x: 416.,
        w: 32.,
        h: 52.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: false,
    },
    Building {
        x: 448.,
        w: 48.,
        h: 92.,
        kind: "highrise",
        windows: "yellow",
        water_tower: true,
    },
    Building {
        x: 496.,
        w: 52.,
        h: 76.,
        kind: "billboard",
        windows: "neon",
        water_tower: false,
    },
    Building {
        x: 548.,
        w: 36.,
        h: 60.,
        kind: "brick",
        windows: "yellow",
        water_tower: false,
    },
    Building {
        x: 584.,
        w: 42.,
        h: 84.,
        kind: "stepped",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 626.,
        w: 46.,
        h: 68.,
        kind: "glass",
        windows: "cyan",
        water_tower: false,
    },
    Building {
        x: 672.,
        w: 34.,
        h: 54.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: false,
    },
    Building {
        x: 706.,
        w: 58.,
        h: 98.,
        kind: "chrysler",
        windows: "blue",
        water_tower: false,
    },
    Building {
        x: 764.,
        w: 40.,
        h: 70.,
        kind: "artdeco",
        windows: "amber",
        water_tower: true,
    },
    Building {
        x: 804.,
        w: 36.,
        h: 58.,
        kind: "brick",
        windows: "yellow",
        water_tower: false,
    },
    Building {
        x: 840.,
        w: 50.,
        h: 88.,
        kind: "highrise",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 890.,
        w: 38.,
        h: 64.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: false,
    },
    Building {
        x: 928.,
        w: 60.,
        h: 104.,
        kind: "empire",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 988.,
        w: 42.,
        h: 72.,
        kind: "glass",
        windows: "blue",
        water_tower: false,
    },
    Building {
        x: 1030.,
        w: 34.,
        h: 56.,
        kind: "brick",
        windows: "yellow",
        water_tower: true,
    },
    Building {
        x: 1064.,
        w: 48.,
        h: 82.,
        kind: "billboard",
        windows: "neon",
        water_tower: false,
    },
    Building {
        x: 1112.,
        w: 36.,
        h: 62.,
        kind: "artdeco",
        windows: "amber",
        water_tower: false,
    },
    Building {
        x: 1148.,
        w: 44.,
        h: 76.,
        kind: "highrise",
        windows: "cyan",
        water_tower: false,
    },
    Building {
        x: 1192.,
        w: 32.,
        h: 50.,
        kind: "brick",
        windows: "yellow",
        water_tower: false,
    },
    Building {
        x: 1224.,
        w: 54.,
        h: 94.,
        kind: "chrysler",
        windows: "blue",
        water_tower: false,
    },
    Building {
        x: 1278.,
        w: 38.,
        h: 66.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: true,
    },
    Building {
        x: 1316.,
        w: 46.,
        h: 84.,
        kind: "glass",
        windows: "warm",
        water_tower: false,
    },
    Building {
        x: 1362.,
        w: 40.,
        h: 68.,
        kind: "artdeco",
        windows: "yellow",
        water_tower: false,
    },
    Building {
        x: 1402.,
        w: 34.,
        h: 54.,
        kind: "brick",
        windows: "fire_escape",
        water_tower: false,
    },
    Building {
        x: 1436.,
        w: 52.,
        h: 90.,
        kind: "highrise",
        windows: "amber",
        water_tower: false,
    },
    Building {
        x: 1488.,
        w: 38.,
        h: 62.,
        kind: "brick",
        windows: "yellow",
        water_tower: false,
    },
    Building {
        x: 1526.,
        w: 44.,
        h: 74.,
        kind: "billboard",
        windows: "neon",
        water_tower: false,
    },
    Building {
        x: 1570.,
        w: 36.,
        h: 58.,
        kind: "brick",
        windows: "yellow",
        water_tower: true,
    },
    Building {
        x: 1606.,
        w: 42.,
        h: 70.,
        kind: "artdeco",
        windows: "cyan",
        water_tower: false,
    },
];

const fn rgb(hex: u32) -> C {
    C::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}
fn screen_x(world: f64, scroll: f64, object_width: f64) -> f64 {
    let x = (world - scroll).rem_euclid(WORLD_LEN);
    if x > WORLD_LEN - object_width - 20. {
        x - WORLD_LEN
    } else {
        x
    }
}

struct Canvas<'a> {
    p: &'a Painter,
    origin: Pos2,
    color: C,
    outline: C,
    line_width: f64,
    path: Vec<Pos2>,
}
impl Canvas<'_> {
    fn point(&self, x: f64, y: f64) -> Pos2 {
        self.origin + vec2(x as f32, y as f32)
    }
    fn fill_rect(&self, x: f64, y: f64, w: f64, h: f64) {
        self.p.rect_filled(
            Rect::from_min_size(self.point(x, y), vec2(w as f32, h as f32)),
            0.,
            self.color,
        );
    }
    fn stroke_rect(&self, x: f64, y: f64, w: f64, h: f64) {
        self.p.rect_stroke(
            Rect::from_min_size(self.point(x, y), vec2(w as f32, h as f32)),
            0.,
            Stroke::new(self.line_width as f32, self.outline),
            StrokeKind::Middle,
        );
    }
    fn begin_path(&mut self) {
        self.path.clear();
    }
    fn vertex(&mut self, x: f64, y: f64) {
        self.path.push(self.point(x, y));
    }
    fn fill(&self) {
        self.p.add(Shape::convex_polygon(
            self.path.clone(),
            self.color,
            Stroke::NONE,
        ));
    }
}

pub(super) fn draw(
    p: &Painter,
    origin: Pos2,
    width: f64,
    ground_y: f64,
    distance: f64,
    flash_timer: f64,
) {
    let mut ctx = Canvas {
        p,
        origin,
        color: C::WHITE,
        outline: C::WHITE,
        line_width: 1.,
        path: Vec::new(),
    };
    let anim_time = distance * 0.05;
    let far_scroll = distance * 0.14;
    let mid_scroll = distance * 0.36;
    let star_count = 36.0;
    {
        let mut s = 0.0;
        while s < star_count {
            let s_world_x = ((s * 269.0) + 43.0) % WORLD_LEN;
            let sx = screen_x(s_world_x, far_scroll * 0.5, 4.0);
            if (sx >= (-5.0)) && (sx <= (width + 5.0)) {
                let sy = 8.0 + ((s * 37.0) % (20.0f64).max(ground_y - 75.0));
                let twinkle = ((anim_time * 2.0) + s).sin() > 0.35;
                ctx.color = if twinkle {
                    rgb(0xffffff)
                } else {
                    C::from_rgba_unmultiplied(192, 132, 252, 140)
                };
                ctx.fill_rect(
                    round(sx),
                    sy,
                    if (s % 4.0) == 0.0 { 2.0 } else { 1.0 },
                    if (s % 4.0) == 0.0 { 2.0 } else { 1.0 },
                );
            }
            s += 1.0;
        }
    }
    {
        let b_start_x = screen_x(20.0, far_scroll, 1500.0);
        if (b_start_x <= (width + 100.0)) && ((b_start_x + 1500.0) >= (-100.0)) {
            ctx.color = rgb(0x0c071d);
            {
                let mut step = 0.0;
                while step < 30.0 {
                    let bx = b_start_x + (step * 50.0);
                    if (bx >= (-60.0)) && (bx <= (width + 60.0)) {
                        let bh = 55.0 + ((step * 37.0) % 50.0);
                        ctx.fill_rect(round(bx), ground_y - bh, 46.0, bh);
                        if (step % 3.0) == 0.0 {
                            ctx.color = rgb(0x6d28d9);
                            ctx.fill_rect(round(bx + 21.0), (ground_y - bh) - 12.0, 3.0, 12.0);
                            ctx.color = if (flash_timer % 20.0) < 10.0 {
                                rgb(0xef4444)
                            } else {
                                rgb(0xfca5a5)
                            };
                            ctx.fill_rect(round(bx + 21.0), (ground_y - bh) - 14.0, 3.0, 2.0);
                            ctx.color = rgb(0x0c071d);
                        }
                    }
                    step += 1.0;
                }
            }
            let br_x1 = b_start_x + 420.0;
            let br_x2 = b_start_x + 1120.0;
            for br_x in [br_x1, br_x2] {
                if (br_x >= (-80.0)) && (br_x <= (width + 80.0)) {
                    ctx.color = rgb(0x1e1338);
                    ctx.fill_rect(br_x - 10.0, ground_y - 78.0, 6.0, 78.0);
                    ctx.fill_rect(br_x + 4.0, ground_y - 78.0, 6.0, 78.0);
                    ctx.fill_rect(br_x - 10.0, ground_y - 78.0, 20.0, 5.0);
                    ctx.fill_rect(br_x - 10.0, ground_y - 45.0, 20.0, 4.0);
                    ctx.color = C::from_rgba_unmultiplied(168, 85, 247, 102);
                    {
                        let mut c = 0.0;
                        while c < 28.0 {
                            let cy = (ground_y - 75.0) + ((c / 28.0f64).powf(2.0) * 45.0);
                            ctx.fill_rect((br_x - 10.0) - c, cy, 2.0, 2.0);
                            ctx.fill_rect((br_x + 10.0) + c, cy, 2.0, 2.0);
                            c += 4.0;
                        }
                    }
                }
            }
        }
    }
    {
        let mp_start_x = screen_x(1600.0, far_scroll, 1600.0);
        if (mp_start_x <= (width + 100.0)) && ((mp_start_x + 1600.0) >= (-100.0)) {
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let mx = mp_start_x + (step * 60.0);
                    if (mx >= (-80.0)) && (mx <= (width + 80.0)) {
                        let is_peak = step == 12.0;
                        let mh = if is_peak {
                            98.0
                        } else {
                            50.0 + ((step * 43.0) % 40.0)
                        };
                        ctx.color = if is_peak {
                            rgb(0x1a0b36)
                        } else {
                            if (step % 2.0) == 0.0 {
                                rgb(0x130926)
                            } else {
                                rgb(0x0e061e)
                            }
                        };
                        {
                            let mut y = 0.0;
                            while y < mh {
                                let w = 12.0 + (y * if is_peak { 0.95 } else { 1.3 }).floor();
                                if (y < 14.0) && (!is_peak) {
                                    ctx.color = rgb(0xe2e8f0);
                                }
                                ctx.fill_rect(
                                    round(mx - (w / 2.0)),
                                    (ground_y - mh) + y,
                                    w,
                                    4.0,
                                );
                                ctx.color = if is_peak {
                                    rgb(0x1a0b36)
                                } else {
                                    if (step % 2.0) == 0.0 {
                                        rgb(0x130926)
                                    } else {
                                        rgb(0x0e061e)
                                    }
                                };
                                y += 4.0;
                            }
                        }
                    }
                    step += 1.0;
                }
            }
        }
    }
    {
        let ch_start_x = screen_x(3200.0, far_scroll, 1600.0);
        if (ch_start_x <= (width + 100.0)) && ((ch_start_x + 1600.0) >= (-100.0)) {
            let karst_pillars = [3450.0, 3800.0, 4200.0, 4600.0];
            for kp_world in karst_pillars {
                let kpx = round(screen_x(kp_world, far_scroll, 60.0));
                if (kpx >= (-60.0)) && (kpx <= (width + 60.0)) {
                    let kp_h = 92.0;
                    let kp_top = ground_y - kp_h;
                    ctx.color = rgb(0x120d2a);
                    ctx.fill_rect(kpx - 14.0, kp_top + 8.0, 28.0, kp_h - 8.0);
                    ctx.fill_rect(kpx - 10.0, kp_top, 20.0, 8.0);
                    ctx.color = rgb(0x064e3b);
                    ctx.fill_rect(kpx - 16.0, kp_top - 4.0, 12.0, 6.0);
                    ctx.fill_rect(kpx + 6.0, kp_top - 2.0, 10.0, 5.0);
                    ctx.color = C::from_rgba_unmultiplied(148, 163, 184, 64);
                    ctx.fill_rect(kpx - 22.0, ground_y - 24.0, 44.0, 24.0);
                }
            }
        }
    }
    {
        let eu_start_x = screen_x(4800.0, far_scroll, 1600.0);
        if (eu_start_x <= (width + 100.0)) && ((eu_start_x + 1600.0) >= (-100.0)) {
            {
                let mut step = 0.0;
                while step < 20.0 {
                    let ax = eu_start_x + (step * 80.0);
                    if (ax >= (-90.0)) && (ax <= (width + 90.0)) {
                        let ah = 65.0 + ((step * 51.0) % 36.0);
                        {
                            let mut y = 0.0;
                            while y < ah {
                                let w = 14.0 + (y * 1.15).floor();
                                ctx.color = if y < 16.0 {
                                    rgb(0xf1f5f9)
                                } else {
                                    if y < 28.0 {
                                        rgb(0x94a3b8)
                                    } else {
                                        rgb(0x140e2b)
                                    }
                                };
                                ctx.fill_rect(
                                    round(ax - (w / 2.0)),
                                    (ground_y - ah) + y,
                                    w,
                                    4.0,
                                );
                                y += 4.0;
                            }
                        }
                    }
                    step += 1.0;
                }
            }
        }
    }
    {
        let eg_start_x = screen_x(6400.0, far_scroll, 1600.0);
        if (eg_start_x <= (width + 100.0)) && ((eg_start_x + 1600.0) >= (-100.0)) {
            let pyr1 = round(screen_x(6900.0, far_scroll, 100.0));
            if (pyr1 >= (-100.0)) && (pyr1 <= (width + 100.0)) {
                let p_h = 78.0;
                {
                    let mut y = 0.0;
                    while y < p_h {
                        let hw = (y * 0.72f64).floor();
                        ctx.color = rgb(0xd97706);
                        ctx.fill_rect(pyr1 - hw, (ground_y - p_h) + y, hw, 3.0);
                        ctx.color = rgb(0x78350f);
                        ctx.fill_rect(pyr1, (ground_y - p_h) + y, hw, 3.0);
                        y += 3.0;
                    }
                }
            }
            let pyr2 = round(screen_x(7300.0, far_scroll, 80.0));
            if (pyr2 >= (-80.0)) && (pyr2 <= (width + 80.0)) {
                let p_h = 62.0;
                {
                    let mut y = 0.0;
                    while y < p_h {
                        let hw = (y * 0.7f64).floor();
                        ctx.color = if y < 14.0 {
                            rgb(0xfde68a)
                        } else {
                            rgb(0xb45309)
                        };
                        ctx.fill_rect(pyr2 - hw, (ground_y - p_h) + y, hw, 3.0);
                        ctx.color = if y < 14.0 {
                            rgb(0xd97706)
                        } else {
                            rgb(0x451a03)
                        };
                        ctx.fill_rect(pyr2, (ground_y - p_h) + y, hw, 3.0);
                        y += 3.0;
                    }
                }
            }
        }
    }
    {
        let fuji_x = round(screen_x(8800.0, far_scroll, 140.0));
        if (fuji_x >= (-140.0)) && (fuji_x <= (width + 140.0)) {
            let f_h = 86.0;
            let f_top = ground_y - f_h;
            {
                let mut y = 0.0;
                while y < f_h {
                    let w = 22.0 + (y * 1.35).floor();
                    ctx.color = if y < 24.0 {
                        rgb(0xffffff)
                    } else {
                        if y < 38.0 {
                            rgb(0x6366f1)
                        } else {
                            rgb(0x1e1b4b)
                        }
                    };
                    ctx.fill_rect(fuji_x - (w / 2.0).floor(), f_top + y, w, 4.0);
                    y += 4.0;
                }
            }
        }
    }
    {
        for b in NY_BUILDINGS {
            let bx = round(screen_x(b.x, mid_scroll, b.w));
            if (bx >= (-b.w)) && (bx <= (width + b.w)) {
                let b_top = ground_y - b.h;
                if b.kind == "brick" {
                    ctx.color = rgb(0x2d1645);
                    ctx.fill_rect(bx, b_top, b.w, b.h);
                    ctx.color = rgb(0x4c1d95);
                    ctx.fill_rect(bx - 1.0, b_top, b.w + 2.0, 3.0);
                    ctx.color = if b.windows == "yellow" {
                        rgb(0xfef08a)
                    } else {
                        rgb(0xfbbf24)
                    };
                    {
                        let mut wy = b_top + 8.0;
                        while wy < (ground_y - 6.0) {
                            ctx.fill_rect(bx + 5.0, wy, 4.0, 5.0);
                            ctx.fill_rect((bx + b.w) - 9.0, wy, 4.0, 5.0);
                            if b.w > 36.0 {
                                ctx.fill_rect((bx + (b.w / 2.0).floor()) - 2.0, wy, 4.0, 5.0);
                            }
                            wy += 9.0;
                        }
                    }
                    if b.windows == "fire_escape" {
                        ctx.color = rgb(0x6b21a8);
                        ctx.fill_rect(bx + 2.0, b_top + 14.0, 10.0, 2.0);
                        ctx.fill_rect(bx + 2.0, b_top + 28.0, 10.0, 2.0);
                        ctx.fill_rect(bx + 2.0, b_top + 42.0, 10.0, 2.0);
                        ctx.fill_rect(bx + 2.0, b_top + 14.0, 1.0, 30.0);
                    }
                } else if (b.kind == "chrysler") || (b.kind == "empire") {
                    ctx.color = rgb(0x170e30);
                    ctx.fill_rect(bx, b_top + 20.0, b.w, b.h - 20.0);
                    ctx.fill_rect(bx + 4.0, b_top + 8.0, b.w - 8.0, 12.0);
                    ctx.fill_rect(bx + 8.0, b_top, b.w - 16.0, 8.0);
                    ctx.color = rgb(0x7c3aed);
                    ctx.fill_rect(bx, b_top + 20.0, b.w, 2.0);
                    ctx.fill_rect(bx + 4.0, b_top + 8.0, b.w - 8.0, 2.0);
                    ctx.color = rgb(0xc084fc);
                    ctx.fill_rect(
                        (bx + (b.w / 2.0).floor()) - 1.0,
                        b_top - 14.0,
                        2.0,
                        14.0,
                    );
                    ctx.color = if (flash_timer % 20.0) < 10.0 {
                        rgb(0xef4444)
                    } else {
                        rgb(0xffffff)
                    };
                    ctx.fill_rect((bx + (b.w / 2.0).floor()) - 2.0, b_top - 16.0, 4.0, 3.0);
                    ctx.color = rgb(0xfef08a);
                    {
                        let mut wy = b_top + 24.0;
                        while wy < (ground_y - 6.0) {
                            ctx.fill_rect(bx + 8.0, wy, 2.0, 3.0);
                            ctx.fill_rect((bx + (b.w / 2.0).floor()) - 1.0, wy, 2.0, 3.0);
                            ctx.fill_rect((bx + b.w) - 10.0, wy, 2.0, 3.0);
                            wy += 8.0;
                        }
                    }
                } else if b.kind == "glass" {
                    ctx.color = rgb(0x0c1b33);
                    ctx.fill_rect(bx, b_top, b.w, b.h);
                    ctx.color = rgb(0x38bdf8);
                    ctx.fill_rect(bx, b_top, b.w, 2.0);
                    ctx.outline = C::from_rgba_unmultiplied(56, 189, 248, 102);
                    ctx.line_width = 1.0;
                    ctx.stroke_rect(bx + 4.0, b_top + 6.0, b.w - 8.0, b.h - 12.0);
                    ctx.color = C::from_rgba_unmultiplied(56, 189, 248, 89);
                    {
                        let mut wy = b_top + 8.0;
                        while wy < (ground_y - 6.0) {
                            ctx.fill_rect(bx + 6.0, wy, b.w - 12.0, 4.0);
                            wy += 8.0;
                        }
                    }
                } else if b.kind == "billboard" {
                    ctx.color = rgb(0x1e1338);
                    ctx.fill_rect(bx, b_top, b.w, b.h);
                    ctx.color = rgb(0x0f172a);
                    ctx.fill_rect(bx + 4.0, b_top + 10.0, b.w - 8.0, 18.0);
                    ctx.color = rgb(0xa855f7);
                    ctx.fill_rect(bx + 3.0, b_top + 9.0, b.w - 6.0, 1.0);
                    ctx.fill_rect(bx + 3.0, b_top + 28.0, b.w - 6.0, 1.0);
                    let frame = (anim_time * 1.5).floor() % 3.0;
                    if frame == 0.0 {
                        ctx.color = rgb(0xff2a85);
                        ctx.fill_rect((bx + (b.w / 2.0).floor()) - 4.0, b_top + 14.0, 8.0, 8.0);
                    } else if frame == 1.0 {
                        ctx.color = rgb(0x00f5d4);
                        ctx.fill_rect(
                            (bx + (b.w / 2.0).floor()) - 6.0,
                            b_top + 15.0,
                            12.0,
                            6.0,
                        );
                    } else {
                        ctx.color = rgb(0xfef08a);
                        ctx.fill_rect(
                            (bx + (b.w / 2.0).floor()) - 2.0,
                            b_top + 13.0,
                            4.0,
                            10.0,
                        );
                        ctx.fill_rect(
                            (bx + (b.w / 2.0).floor()) - 5.0,
                            b_top + 16.0,
                            10.0,
                            4.0,
                        );
                    }
                } else {
                    ctx.color = rgb(0x180d2e);
                    ctx.fill_rect(bx, b_top, b.w, b.h);
                    ctx.color = rgb(0xfbbf24);
                    {
                        let mut wy = b_top + 6.0;
                        while wy < (ground_y - 6.0) {
                            ctx.fill_rect(bx + 6.0, wy, 3.0, 3.0);
                            ctx.fill_rect((bx + b.w) - 9.0, wy, 3.0, 3.0);
                            wy += 7.0;
                        }
                    }
                }
                if b.water_tower {
                    let wt_x = bx + 6.0;
                    let wt_y = b_top - 14.0;
                    ctx.color = rgb(0x334155);
                    ctx.fill_rect(wt_x + 1.0, wt_y + 7.0, 2.0, 7.0);
                    ctx.fill_rect(wt_x + 9.0, wt_y + 7.0, 2.0, 7.0);
                    ctx.color = rgb(0x78350f);
                    ctx.fill_rect(wt_x, wt_y, 12.0, 7.0);
                    ctx.color = rgb(0x451a03);
                    ctx.fill_rect(wt_x - 1.0, wt_y - 2.0, 14.0, 2.0);
                    ctx.fill_rect(wt_x + 3.0, wt_y - 4.0, 6.0, 2.0);
                }
            }
        }
    }
    {
        let mp_base = screen_x(1600.0, mid_scroll, 1600.0);
        if (mp_base <= (width + 100.0)) && ((mp_base + 1600.0) >= (-100.0)) {
            ctx.color = rgb(0x064e3b);
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let rx = mp_base + (step * 60.0);
                    if (rx >= (-80.0)) && (rx <= (width + 80.0)) {
                        let r_h = 58.0 + ((step * 0.4).sin() * 20.0);
                        ctx.fill_rect(round(rx), ground_y - r_h, 64.0, r_h);
                    }
                    step += 1.0;
                }
            }
            {
                let mut t = 0.0;
                while t < 10.0 {
                    let tw_x = (mp_base + 240.0) + (t * 90.0);
                    if (tw_x >= (-100.0)) && (tw_x <= (width + 100.0)) {
                        let t_y = (ground_y - 14.0) - ((t % 5.0) * 8.0);
                        ctx.color = rgb(0x334155);
                        ctx.fill_rect(round(tw_x), t_y, 82.0, 8.0);
                        ctx.color = rgb(0x475569);
                        ctx.fill_rect(round(tw_x), t_y, 82.0, 2.0);
                        ctx.color = rgb(0x15803d);
                        ctx.fill_rect(round(tw_x), t_y - 3.0, 80.0, 3.0);
                        ctx.color = rgb(0x22c55e);
                        ctx.fill_rect(round(tw_x + 8.0), t_y - 4.0, 4.0, 2.0);
                        ctx.fill_rect(round(tw_x + 38.0), t_y - 4.0, 4.0, 2.0);
                    }
                    t += 1.0;
                }
            }
            let cit_x = mp_base + 680.0;
            if (cit_x >= (-80.0)) && (cit_x <= (width + 80.0)) {
                ctx.color = rgb(0x475569);
                ctx.fill_rect(cit_x - 25.0, ground_y - 30.0, 50.0, 30.0);
                ctx.color = rgb(0x334155);
                ctx.fill_rect(cit_x - 5.0, ground_y - 16.0, 10.0, 16.0);
                ctx.fill_rect(cit_x - 18.0, ground_y - 24.0, 6.0, 6.0);
                ctx.fill_rect(cit_x + 12.0, ground_y - 24.0, 6.0, 6.0);
                ctx.color = rgb(0x78350f);
                ctx.begin_path();
                ctx.vertex(cit_x - 28.0, ground_y - 30.0);
                ctx.vertex(cit_x, ground_y - 44.0);
                ctx.vertex(cit_x + 28.0, ground_y - 30.0);

                ctx.fill();
            }
            ctx.color = C::from_rgba_unmultiplied(226, 232, 240, 102);
            ctx.fill_rect(
                (mp_base + 120.0) + ((anim_time * 10.0) % 600.0),
                ground_y - 48.0,
                80.0,
                8.0,
            );
            ctx.fill_rect(
                (mp_base + 540.0) - ((anim_time * 8.0) % 400.0),
                ground_y - 36.0,
                110.0,
                10.0,
            );
        }
    }
    {
        let ch_base = screen_x(3200.0, mid_scroll, 1600.0);
        if (ch_base <= (width + 100.0)) && ((ch_base + 1600.0) >= (-100.0)) {
            ctx.color = rgb(0x1e293b);
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let mx = ch_base + (step * 60.0);
                    if (mx >= (-80.0)) && (mx <= (width + 80.0)) {
                        let m_h = 46.0 + ((step * 0.5).sin() * 18.0);
                        ctx.fill_rect(round(mx), ground_y - m_h, 62.0, m_h);
                    }
                    step += 1.0;
                }
            }
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let wx = ch_base + (step * 60.0);
                    if (wx >= (-80.0)) && (wx <= (width + 80.0)) {
                        let m_h = 46.0 + ((step * 0.5).sin() * 18.0);
                        let w_top = ground_y - m_h;
                        ctx.color = rgb(0x64748b);
                        ctx.fill_rect(round(wx), w_top - 6.0, 62.0, 6.0);
                        ctx.color = rgb(0x94a3b8);
                        {
                            let mut c = 0.0;
                            while c < 60.0 {
                                ctx.fill_rect(round(wx + c), w_top - 9.0, 6.0, 3.0);
                                c += 10.0;
                            }
                        }
                    }
                    step += 1.0;
                }
            }
            let watchtowers = [(ch_base + 280.0), (ch_base + 750.0), (ch_base + 1240.0)];
            for wt_x in watchtowers {
                if (wt_x >= (-60.0)) && (wt_x <= (width + 60.0)) {
                    let wt_top = ground_y - 62.0;
                    ctx.color = rgb(0x475569);
                    ctx.fill_rect(wt_x - 16.0, wt_top, 32.0, 40.0);
                    ctx.color = rgb(0x64748b);
                    ctx.fill_rect(wt_x - 18.0, wt_top, 36.0, 3.0);
                    ctx.color = rgb(0x94a3b8);
                    ctx.fill_rect(wt_x - 18.0, wt_top - 4.0, 5.0, 4.0);
                    ctx.fill_rect(wt_x - 7.0, wt_top - 4.0, 5.0, 4.0);
                    ctx.fill_rect(wt_x + 2.0, wt_top - 4.0, 5.0, 4.0);
                    ctx.fill_rect(wt_x + 13.0, wt_top - 4.0, 5.0, 4.0);
                    ctx.color = rgb(0x0f172a);
                    ctx.fill_rect(wt_x - 9.0, wt_top + 8.0, 5.0, 10.0);
                    ctx.fill_rect(wt_x + 4.0, wt_top + 8.0, 5.0, 10.0);
                    ctx.color = rgb(0xef4444);
                    ctx.fill_rect(wt_x - 1.0, wt_top - 12.0, 8.0, 6.0);
                    ctx.color = rgb(0xcbd5e1);
                    ctx.fill_rect(wt_x - 2.0, wt_top - 12.0, 1.0, 12.0);
                }
            }
        }
    }
    {
        let eu_base = screen_x(4800.0, mid_scroll, 1600.0);
        if (eu_base <= (width + 100.0)) && ((eu_base + 1600.0) >= (-100.0)) {
            let wf_x = eu_base + 380.0;
            if (wf_x >= (-100.0)) && (wf_x <= (width + 100.0)) {
                let cliff_h = 68.0;
                let cliff_top = ground_y - cliff_h;
                ctx.color = rgb(0x1e1b4b);
                ctx.fill_rect(wf_x - 44.0, cliff_top, 32.0, cliff_h);
                ctx.fill_rect(wf_x + 12.0, cliff_top + 6.0, 34.0, cliff_h - 6.0);
                ctx.color = rgb(0x4338ca);
                ctx.fill_rect(wf_x - 44.0, cliff_top, 32.0, 3.0);
                ctx.fill_rect(wf_x + 12.0, cliff_top + 6.0, 34.0, 3.0);
                ctx.color = rgb(0x38bdf8);
                ctx.fill_rect(wf_x - 12.0, cliff_top + 3.0, 24.0, cliff_h - 3.0);
                ctx.color = rgb(0xffffff);
                {
                    let mut wy = cliff_top + 4.0;
                    while wy < (ground_y - 4.0) {
                        let flow_offset = (((anim_time * 20.0) + wy) % 6.0).floor();
                        ctx.fill_rect((wf_x - 10.0) + (flow_offset % 3.0), wy, 4.0, 4.0);
                        ctx.fill_rect((wf_x + 4.0) - (flow_offset % 3.0), wy, 4.0, 4.0);
                        wy += 6.0;
                    }
                }
                ctx.color = rgb(0xe0f2fe);
                let foam_w = 34.0 + ((anim_time * 10.0).sin() * 4.0).floor();
                ctx.fill_rect(
                    wf_x - (foam_w / 2.0).floor(),
                    ground_y - 5.0,
                    foam_w,
                    5.0,
                );
                ctx.color = rgb(0xffffff);
                ctx.fill_rect(
                    (wf_x - (foam_w / 2.0).floor()) + 2.0,
                    ground_y - 7.0,
                    foam_w - 4.0,
                    2.0,
                );
            }
            let cast_x = eu_base + 750.0;
            if (cast_x >= (-80.0)) && (cast_x <= (width + 80.0)) {
                let c_h = 66.0;
                let c_top = ground_y - c_h;
                ctx.color = rgb(0xe2e8f0);
                ctx.fill_rect(cast_x - 22.0, c_top + 14.0, 44.0, c_h - 14.0);
                ctx.fill_rect(cast_x - 24.0, c_top + 12.0, 48.0, 2.0);
                ctx.color = rgb(0xcbd5e1);
                ctx.fill_rect(cast_x - 22.0, c_top + 4.0, 8.0, 24.0);
                ctx.fill_rect(cast_x + 14.0, c_top + 4.0, 8.0, 24.0);
                ctx.color = rgb(0x1e40af);
                ctx.fill_rect(cast_x - 23.0, c_top - 4.0, 10.0, 8.0);
                ctx.fill_rect(cast_x - 21.0, c_top - 10.0, 6.0, 6.0);
                ctx.fill_rect(cast_x - 19.0, c_top - 14.0, 2.0, 4.0);
                ctx.fill_rect(cast_x + 13.0, c_top - 4.0, 10.0, 8.0);
                ctx.fill_rect(cast_x + 15.0, c_top - 10.0, 6.0, 6.0);
                ctx.fill_rect(cast_x + 17.0, c_top - 14.0, 2.0, 4.0);
                ctx.color = rgb(0x1e293b);
                ctx.fill_rect(cast_x - 5.0, ground_y - 14.0, 10.0, 14.0);
            }
            let v_x = eu_base + 1150.0;
            if (v_x >= (-80.0)) && (v_x <= (width + 80.0)) {
                ctx.color = rgb(0x78350f);
                ctx.fill_rect(v_x - 20.0, ground_y - 24.0, 40.0, 24.0);
                ctx.color = rgb(0xb91c1c);
                ctx.begin_path();
                ctx.vertex(v_x - 24.0, ground_y - 24.0);
                ctx.vertex(v_x, ground_y - 38.0);
                ctx.vertex(v_x + 24.0, ground_y - 24.0);

                ctx.fill();
                ctx.color = rgb(0x334155);
                ctx.fill_rect(v_x + 10.0, ground_y - 42.0, 4.0, 10.0);
                ctx.color = C::from_rgba_unmultiplied(226, 232, 240, 128);
                ctx.fill_rect(v_x + 10.0, ground_y - 47.0, 4.0, 4.0);
            }
        }
    }
    {
        let eg_base = screen_x(6400.0, mid_scroll, 1600.0);
        if (eg_base <= (width + 100.0)) && ((eg_base + 1600.0) >= (-100.0)) {
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let dx = eg_base + (step * 60.0);
                    if (dx >= (-80.0)) && (dx <= (width + 80.0)) {
                        let d_h = 34.0 + ((step * 0.45).sin() * 16.0);
                        ctx.color = rgb(0xf59e0b);
                        ctx.fill_rect(round(dx), ground_y - d_h, 62.0, d_h);
                        ctx.color = rgb(0xd97706);
                        ctx.fill_rect(round(dx + 28.0), ground_y - d_h, 34.0, d_h);
                    }
                    step += 1.0;
                }
            }
            let sp_x = eg_base + 820.0;
            if (sp_x >= (-60.0)) && (sp_x <= (width + 60.0)) {
                ctx.color = rgb(0xd97706);
                ctx.fill_rect(sp_x - 24.0, ground_y - 18.0, 48.0, 18.0);
                ctx.fill_rect(sp_x - 28.0, ground_y - 8.0, 12.0, 8.0);
                ctx.color = rgb(0xf59e0b);
                ctx.fill_rect(sp_x + 12.0, ground_y - 32.0, 14.0, 16.0);
                ctx.color = rgb(0xb45309);
                ctx.fill_rect(sp_x + 10.0, ground_y - 34.0, 18.0, 4.0);
                ctx.fill_rect(sp_x + 10.0, ground_y - 30.0, 3.0, 10.0);
                ctx.color = rgb(0x78350f);
                ctx.fill_rect(sp_x + 22.0, ground_y - 18.0, 3.0, 5.0);
            }
            let oa_x = eg_base + 1200.0;
            if (oa_x >= (-60.0)) && (oa_x <= (width + 60.0)) {
                ctx.color = rgb(0x0284c7);
                ctx.fill_rect(oa_x - 20.0, ground_y - 4.0, 40.0, 4.0);
                ctx.color = rgb(0x38bdf8);
                ctx.fill_rect(oa_x - 16.0, ground_y - 3.0, 32.0, 2.0);
                for po in [(-16.0), 14.0] {
                    let px = oa_x + po;
                    ctx.color = rgb(0x78350f);
                    ctx.fill_rect(px, ground_y - 26.0, 3.0, 26.0);
                    ctx.color = rgb(0x15803d);
                    ctx.fill_rect(px - 10.0, ground_y - 30.0, 8.0, 3.0);
                    ctx.fill_rect(px + 5.0, ground_y - 30.0, 8.0, 3.0);
                    ctx.fill_rect(px - 4.0, ground_y - 34.0, 11.0, 4.0);
                }
            }
        }
    }
    {
        let jp_base = screen_x(8000.0, mid_scroll, 1600.0);
        if (jp_base <= (width + 100.0)) && ((jp_base + 1600.0) >= (-100.0)) {
            ctx.color = rgb(0x064e3b);
            {
                let mut step = 0.0;
                while step < 26.0 {
                    let jx = jp_base + (step * 60.0);
                    if (jx >= (-80.0)) && (jx <= (width + 80.0)) {
                        let jh = 42.0 + ((step * 0.4).sin() * 16.0);
                        ctx.fill_rect(round(jx), ground_y - jh, 62.0, jh);
                    }
                    step += 1.0;
                }
            }
            let pag_x = jp_base + 400.0;
            if (pag_x >= (-60.0)) && (pag_x <= (width + 60.0)) {
                let pag_h = 72.0;
                let pag_top = ground_y - pag_h;
                ctx.color = rgb(0x7f1d1d);
                ctx.fill_rect(pag_x - 8.0, pag_top + 14.0, 16.0, pag_h - 14.0);
                {
                    let mut p = 0.0;
                    while p < 5.0 {
                        let py = (ground_y - 12.0) - (p * 11.0);
                        let pw = 32.0 - (p * 4.0);
                        ctx.color = rgb(0x0f172a);
                        ctx.fill_rect(pag_x - (pw / 2.0).floor(), py, pw, 3.0);
                        ctx.color = rgb(0xdc2626);
                        ctx.fill_rect((pag_x - (pw / 2.0).floor()) - 2.0, py + 1.0, 2.0, 2.0);
                        ctx.fill_rect(pag_x + (pw / 2.0).floor(), py + 1.0, 2.0, 2.0);
                        p += 1.0;
                    }
                }
                ctx.color = rgb(0xfbbf24);
                ctx.fill_rect(pag_x - 1.0, pag_top, 2.0, 14.0);
            }
            let torii_x = jp_base + 1150.0;
            if (torii_x >= (-60.0)) && (torii_x <= (width + 60.0)) {
                ctx.color = rgb(0xef4444);
                ctx.fill_rect(torii_x - 14.0, ground_y - 26.0, 4.0, 26.0);
                ctx.fill_rect(torii_x + 10.0, ground_y - 26.0, 4.0, 26.0);
                ctx.fill_rect(torii_x - 18.0, ground_y - 26.0, 36.0, 4.0);
                ctx.fill_rect(torii_x - 15.0, ground_y - 21.0, 30.0, 3.0);
                ctx.color = rgb(0x0f172a);
                ctx.fill_rect(torii_x - 19.0, ground_y - 27.0, 38.0, 2.0);
            }
            ctx.color = rgb(0xf472b6);
            {
                let mut petal = 0.0;
                while petal < 6.0 {
                    let pet_x =
                        (((jp_base + 200.0) + (petal * 160.0)) + (anim_time * 15.0)) % 1600.0;
                    let px = screen_x(8000.0 + pet_x, mid_scroll, 10.0);
                    if (px >= 0.0) && (px <= width) {
                        let py = (ground_y - 20.0) - (((petal * 23.0) + (anim_time * 8.0)) % 45.0);
                        ctx.fill_rect(round(px), round(py), 2.0, 2.0);
                    }
                    petal += 1.0;
                }
            }
        }
    }
}

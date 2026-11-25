use image::RgbaImage;
use std::time::{Duration, Instant};

// A high-polling mouse can produce hundreds of samples per second. Keep the
// preview smooth without making each input sample a new GPU submission.
const PREVIEW_FRAME: Duration = Duration::from_nanos(1_000_000_000 / 120);

pub(super) struct Rotation {
    pub angles: [f32; 2],
    pointer: Option<egui::Pos2>,
    frame: Duration,
    next_frame: Option<Instant>,
    pending: Option<Instant>,
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            angles: [-0.34, 0.12],
            pointer: None,
            frame: crate::COMPOSITOR_FRAME_INTERVAL,
            next_frame: None,
            pending: None,
        }
    }
}

impl Rotation {
    pub fn dragging(&self) -> bool {
        self.pointer.is_some()
    }

    pub fn begin(&mut self, position: egui::Pos2, frame: Duration) {
        self.pointer = Some(position);
        self.frame = frame.max(PREVIEW_FRAME);
    }

    pub fn end(&mut self) {
        self.pointer = None;
    }

    pub fn cancel(&mut self) {
        self.end();
        self.pending = None;
        self.next_frame = None;
    }

    pub fn move_to(&mut self, position: egui::Pos2, now: Instant) {
        if let Some(previous) = self.pointer.replace(position) {
            self.add(position - previous, now);
        }
    }

    pub fn add(&mut self, delta: egui::Vec2, now: Instant) {
        if delta == egui::Vec2::ZERO {
            return;
        }
        let angles = [
            (self.angles[0] + delta.x * 0.015).rem_euclid(std::f32::consts::TAU),
            (self.angles[1] + delta.y * 0.014).clamp(-1.45, 1.45),
        ];
        if angles != self.angles {
            self.angles = angles;
            self.pending.get_or_insert_with(|| self.next_frame.map_or(now, |t| t.max(now)));
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.pending
    }

    pub fn take_frame(&mut self, now: Instant) -> bool {
        if self.pending.is_some_and(|t| t <= now) {
            self.pending = None;
            self.next_frame = Some(now + self.frame);
            true
        } else {
            false
        }
    }
}

pub(super) struct PreviewTexture {
    handle: egui::TextureHandle,
    bounds: Option<[usize; 4]>,
}

impl PreviewTexture {
    pub fn new(ctx: &egui::Context, frame: egui::ColorImage) -> Self {
        let bounds = content_bounds(&frame);
        let handle = ctx.load_texture("minecraft-skin", frame, egui::TextureOptions::LINEAR);
        Self { handle, bounds }
    }

    pub fn id(&self) -> egui::TextureId {
        self.handle.id()
    }

    pub fn update(&mut self, frame: egui::ColorImage) {
        let bounds = content_bounds(&frame);
        let dirty = match (self.bounds, bounds) {
            (Some([l, t, r, b]), Some([x, y, end_x, end_y])) => {
                Some([l.min(x), t.min(y), r.max(end_x), b.max(end_y)])
            }
            (previous, next) => previous.or(next),
        };
        if let Some([x, y, end_x, end_y]) = dirty {
            // Include the old pose so transparent pixels erase its silhouette.
            // The camera canvas and texture handle keep their original size.
            let patch = frame.region_by_pixels([x, y], [end_x - x, end_y - y]);
            self.handle.set_partial([x, y], patch, egui::TextureOptions::LINEAR);
        }
        self.bounds = bounds;
    }
}

fn content_bounds(frame: &egui::ColorImage) -> Option<[usize; 4]> {
    let [width, height] = frame.size;
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (y, row) in frame.pixels.chunks_exact(width).enumerate() {
        if let Some(x) = row.iter().position(|p| p.a() != 0) {
            let end = row[x..].iter().rposition(|p| p.a() != 0).unwrap() + x + 1;
            left = left.min(x);
            top = top.min(y);
            right = right.max(end);
            bottom = y + 1;
        }
    }
    (right > left && bottom > top).then_some([left, top, right, bottom])
}

type Vec3 = [f32; 3];
struct Cuboid {
    x: f32,
    y: f32,
    z: f32,
    w: u32,
    h: u32,
    d: u32,
    u: u32,
    v: u32,
    grow: f32,
    mirror: bool,
}

pub(super) fn head(skin: &RgbaImage) -> RgbaImage {
    let s = skin.width() / 64;
    let side = 8 * s;
    // WebView2 ignored a completely opaque hat face, which older skins
    // often filled with a solid placeholder over the real head.
    let use_hat = (0..side).any(|y| {
        (0..side).any(|x| skin.get_pixel(40 * s + x, 8 * s + y).0[3] < 255)
    });
    let mut out = RgbaImage::new(side, side);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let base = skin.get_pixel(8 * s + x, 8 * s + y).0;
        let hat = skin.get_pixel(40 * s + x, 8 * s + y).0;
        let a = if use_hat { u16::from(hat[3]) } else { 0 };
        let mix = |i| ((u16::from(hat[i]) * a + u16::from(base[i]) * (255 - a) + 127) / 255) as u8;
        *p = image::Rgba([mix(0), mix(1), mix(2), 255]);
    }
    out
}

/// Skin template for the upload slot: clear faces with a one-pixel outline,
/// using the same UV layout as the WebView2 mannequin.
pub(super) fn mannequin() -> RgbaImage {
    let mut skin = RgbaImage::new(64, 64);
    for (u, v, w, h, d) in [
        (0, 0, 8, 8, 8),
        (16, 16, 8, 12, 4),
        (40, 16, 4, 12, 4),
        (32, 48, 4, 12, 4),
        (0, 16, 4, 12, 4),
        (16, 48, 4, 12, 4),
    ] {
        for (x, y, width, height) in [
            (u + d, v, w, d),
            (u + w + d, v, w, d),
            (u, v + d, d, h),
            (u + d, v + d, w, h),
            (u + d + w, v + d, d, h),
            (u + 2 * d + w, v + d, w, h),
        ] {
            for j in 0..height {
                for i in 0..width {
                    let edge = i == 0 || i == width - 1 || j == 0 || j == height - 1;
                    skin.put_pixel(
                        x + i,
                        y + j,
                        image::Rgba([255, 255, 255, if edge { 255 } else { 10 }]),
                    );
                }
            }
        }
    }
    skin
}

pub(super) fn render(
    skin: &RgbaImage,
    slim: bool,
    yaw: f32,
    pitch: f32,
    dim: u32,
) -> egui::ColorImage {
    let legacy = skin.height() == skin.width() / 2;
    let aw = if slim && !legacy { 3 } else { 4 };
    let parts = [
        (-4., -16., -4., 8, 8, 8, 0, 0, 32, 0, 0.48, false),
        (-4., -8., -2., 8, 12, 4, 16, 16, 16, 32, 0.24, false),
        (
            -4. - aw as f32,
            -8.,
            -2.,
            aw,
            12,
            4,
            40,
            16,
            40,
            32,
            0.24,
            false,
        ),
        (
            4.,
            -8.,
            -2.,
            aw,
            12,
            4,
            if legacy { 40 } else { 32 },
            if legacy { 16 } else { 48 },
            48,
            48,
            0.24,
            legacy,
        ),
        (-4., 4., -2., 4, 12, 4, 0, 16, 0, 32, 0.24, false),
        (
            0.,
            4.,
            -2.,
            4,
            12,
            4,
            if legacy { 0 } else { 16 },
            if legacy { 16 } else { 48 },
            0,
            48,
            0.24,
            legacy,
        ),
    ];
    let n = dim as usize;
    let mut pixels = vec![egui::Color32::TRANSPARENT; n * n];
    let mut depth = vec![f32::NEG_INFINITY; n * n];
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let project = |[x, y, z]: Vec3| {
        let (x, z) = (x * cy + z * sy, z * cy - x * sy);
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        let f = dim as f32 / (38. - z * 0.14);
        [dim as f32 / 2. + x * f, dim as f32 / 2. + y * f, z]
    };
    for (i, (x, y, z, w, h, d, u, v, ou, ov, grow, mirror)) in parts.into_iter().enumerate() {
        cuboid(
            Cuboid {
                x,
                y,
                z,
                w,
                h,
                d,
                u,
                v,
                grow: 0.,
                mirror,
            },
            skin,
            &project,
            &mut pixels,
            &mut depth,
            n,
        );
        if !legacy || i == 0 {
            cuboid(
                Cuboid {
                    x,
                    y,
                    z,
                    w,
                    h,
                    d,
                    u: ou,
                    v: ov,
                    grow,
                    mirror: false,
                },
                skin,
                &project,
                &mut pixels,
                &mut depth,
                n,
            );
        }
    }
    egui::ColorImage::new([n, n], pixels)
}
fn cuboid(
    c: Cuboid,
    skin: &RgbaImage,
    proj: &impl Fn(Vec3) -> Vec3,
    pixels: &mut [egui::Color32],
    depth: &mut [f32],
    n: usize,
) {
    let Cuboid {
        x,
        y,
        z,
        w,
        h,
        d,
        u,
        v,
        grow,
        mirror,
    } = c;
    let (x0, x1, y0, y1, z0, z1) = (
        x - grow,
        x + w as f32 + grow,
        y - grow,
        y + h as f32 + grow,
        z - grow,
        z + d as f32 + grow,
    );
    // Quads are wound clockwise as seen from outside, starting at the UV's top left.
    let faces = [
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [u + d, v + d, w, h],
            1.00,
        ),
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [u + d + w, v + d, d, h],
            0.79,
        ),
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + 2 * d + w, v + d, w, h],
            0.84,
        ),
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [u, v + d, d, h],
            0.91,
        ),
        (
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [u + d, v, w, d],
            1.00,
        ),
        (
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [u + d + w, v, w, d],
            0.71,
        ),
    ];
    let edge =
        |a: Vec3, b: Vec3, p: Vec3| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    let scale = skin.width() / 64;
    for (i, (vertices, tex, shade)) in faces.iter().enumerate() {
        let [u, v, w, h] = if mirror {
            faces[match i {
                1 => 3,
                3 => 1,
                i => i,
            }]
            .1
        } else {
            *tex
        };
        let p = vertices.map(proj);
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            let (pa, pb, pc) = (p[a], p[b], p[c]);
            let area = edge(pa, pb, pc);
            if area <= 0.0001 {
                continue;
            }
            let yrange = pa[1].min(pb[1]).min(pc[1]).floor().max(0.) as usize
                ..(pa[1].max(pb[1]).max(pc[1]).ceil() as usize).min(n);
            let xrange = pa[0].min(pb[0]).min(pc[0]).floor().max(0.) as usize
                ..(pa[0].max(pb[0]).max(pc[0]).ceil() as usize).min(n);
            for y in yrange {
                for x in xrange.clone() {
                    let sample = [x as f32 + 0.5, y as f32 + 0.5, 0.];
                    let (wa, wb, wc) = (
                        edge(pb, pc, sample) / area,
                        edge(pc, pa, sample) / area,
                        edge(pa, pb, sample) / area,
                    );
                    if wa < 0. || wb < 0. || wc < 0. {
                        continue;
                    }
                    let z = pa[2] * wa + pb[2] * wb + pc[2] * wc;
                    let idx = y * n + x;
                    if z < depth[idx] {
                        continue;
                    }
                    let mut tx = uv[a][0] * wa + uv[b][0] * wb + uv[c][0] * wc;
                    let ty = uv[a][1] * wa + uv[b][1] * wb + uv[c][1] * wc;
                    if mirror {
                        tx = 1. - tx;
                    }
                    let px = u * scale + ((tx * (w * scale) as f32) as u32).min(w * scale - 1);
                    let py = v * scale + ((ty * (h * scale) as f32) as u32).min(h * scale - 1);
                    let [r, g, b, mut alpha] = skin.get_pixel(px, py).0;
                    if grow == 0. {
                        alpha = 255;
                    }
                    if alpha == 0 {
                        continue;
                    }
                    let rgb = [r, g, b].map(|v| (v as f32 * shade) as u8);
                    let dst = pixels[idx];
                    let m = |s, d| {
                        ((u16::from(s) * u16::from(alpha) + u16::from(d) * u16::from(255 - alpha))
                            / 255) as u8
                    };
                    pixels[idx] = egui::Color32::from_rgba_premultiplied(
                        m(rgb[0], dst.r()),
                        m(rgb[1], dst.g()),
                        m(rgb[2], dst.b()),
                        alpha.saturating_add(m(0, dst.a())),
                    );
                    depth[idx] = z;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn head_preserves_front_texels_and_composites_transparent_hat() {
        use image::{Rgba, RgbaImage};

        let mut skin = RgbaImage::new(64, 32);
        for y in 0..8 {
            for x in 0..8 {
                skin.put_pixel(8 + x, 8 + y, Rgba([x as u8, y as u8, 128, 255]));
                skin.put_pixel(40 + x, 8 + y, Rgba([204, 112, 0, 255]));
            }
        }
        let front = super::head(&skin);
        assert_eq!(front.dimensions(), (8, 8));
        assert_eq!(front[(7, 1)], Rgba([7, 1, 128, 255]));

        skin.put_pixel(47, 9, Rgba([204, 112, 0, 128]));
        skin.put_pixel(41, 12, Rgba([0; 4]));
        let with_hat = super::head(&skin);
        assert_eq!(with_hat[(7, 1)], Rgba([106, 57, 64, 255]));
        assert_eq!(with_hat[(1, 4)], front[(1, 4)]);
        assert_eq!(with_hat[(3, 6)], Rgba([204, 112, 0, 255]));
    }

    #[test]
    fn rapid_drag_samples_keep_the_full_motion_and_one_frame_deadline() {
        use egui::pos2;
        use std::time::{Duration, Instant};

        let mut rotation = super::Rotation::default();
        let now = Instant::now();
        let interval = super::PREVIEW_FRAME;
        rotation.begin(pos2(0., 0.), Duration::from_millis(4));
        rotation.move_to(pos2(10., 0.), now);
        assert!(rotation.take_frame(now));

        let first = now + Duration::from_millis(1);
        rotation.move_to(pos2(30., 200.), first);
        rotation.move_to(pos2(70., 170.), first + Duration::from_millis(1));
        assert!(!rotation.take_frame(first));
        rotation.end();
        assert_eq!(rotation.deadline(), Some(now + interval));
        assert!(rotation.take_frame(now + interval));
        assert!((rotation.angles[0] - 0.71).abs() < 1e-5);
        assert!((rotation.angles[1] - 1.03).abs() < 1e-5);
        assert_eq!(rotation.deadline(), None);
    }

    #[test]
    fn changing_pose_clears_old_pixels_without_replacing_the_texture() {
        use egui::{Color32, ColorImage, Context, ImageData};

        let ctx = Context::default();
        let mut first = ColorImage::filled([8, 8], Color32::TRANSPARENT);
        first[(1, 2)] = Color32::WHITE;
        let mut texture = super::PreviewTexture::new(&ctx, first);
        let id = texture.id();
        let _ = ctx.tex_manager().write().take_delta();

        let mut next = ColorImage::filled([8, 8], Color32::TRANSPARENT);
        next[(6, 4)] = Color32::WHITE;
        texture.update(next);
        let delta = ctx.tex_manager().write().take_delta();
        assert_eq!(delta.set.len(), 1);
        let (updated, patch) = &delta.set[0];
        assert_eq!(*updated, id);
        assert_eq!(patch.pos, Some([1, 2]));
        let ImageData::Color(image) = &patch.image;
        assert_eq!(image.size, [6, 3]);
        assert_eq!(image[(0, 0)], Color32::TRANSPARENT);
        assert_eq!(image[(5, 2)], Color32::WHITE);
    }

    #[test]
    fn rotating_changes_visible_skin_faces() {
        let skin = super::super::storage::rgba(include_bytes!("steve.png"), 1024).unwrap();
        let front = super::render(&skin, false, 0., 0., 180);
        let back = super::render(&skin, false, 3.14, 0., 180);
        assert_ne!(front.pixels, back.pixels);
        assert!(front.pixels.iter().filter(|p| p.a() == 255).count() > 3000);
    }
}

@group(0) @binding(4) var chrome_atlas: texture_2d<f32>;

const CHROME_FILL = vec3(4.0, 4.0, 22.0) / 255.0;
const NAV_FILL = vec3(7.0, 5.0, 24.0) / 255.0;
const LABEL_COLOR = vec3(203.0, 213.0, 225.0) / 255.0;
const CONTROL_COLOR = vec3(148.0, 163.0, 184.0) / 255.0;
const CONTROL_HOVER = vec3(241.0, 245.0, 249.0) / 255.0;

// SVG/CSS paint is composited in sRGB before encoding the swapchain pixel.
// rgb is premultiplied so the hairline can spill over the animated wallpaper.
fn over(below: vec4<f32>, color: vec3<f32>, opacity: f32) -> vec4<f32> {
    return vec4(mix(below.rgb, color, opacity), below.a + (1.0 - below.a) * opacity);
}

fn linear_to_srgb(rgb: vec3<f32>) -> vec3<f32> {
    let c = clamp(rgb, vec3(0.0), vec3(1.0));
    return select(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055,
                  c > vec3(0.0031308));
}

fn gradient_pixel(index: i32, row: i32) -> vec3<f32> {
    return textureLoad(chrome_atlas, vec2(index % 600, row + index / 600), 0).rgb;
}

fn ramp(x: f32, row: i32) -> vec3<f32> {
    let phase = fract(x / 2400.0) * 2400.0;
    let i = i32(phase);
    // The nav ramp ends on a different color; keep its SVG repeat seam sharp.
    let j = select((i + 1) % 2400, min(i + 1, 2399), row == 4);
    return mix(gradient_pixel(i, row), gradient_pixel(j, row), fract(phase));
}

fn sprite(rect: vec4<f32>, point: vec2<f32>) -> vec4<f32> {
    if any(point < vec2(0.0)) || any(point >= rect.zw) { return vec4(0.0); }
    let dims = vec2<f32>(textureDimensions(chrome_atlas));
    return textureSampleLevel(chrome_atlas, smooth_sampler, (rect.xy + point) * 2.0 / dims, 0.0);
}

fn centered_mask(rect: vec4<f32>, point: vec2<f32>) -> f32 {
    return sprite(rect, point + rect.zw * 0.5).a;
}

fn coverage(distance: f32, dpi: f32) -> f32 {
    return clamp(0.5 - distance * dpi, 0.0, 1.0);
}

fn title_edge(xy: vec2<f32>, left: f32) -> f32 {
    let x = 470.0 - abs(xy.x - left - 470.0);
    if x <= 0.0 { return xy.y - 52.0; }
    if x >= 90.0 { return xy.y - 102.0; }
    // Invert the reference cubic: (0,52), (40,52), (50,102), (90,102).
    var t = x / 90.0;
    for (var i = 0; i < 3; i++) {
        let bx = t * (120.0 + t * (-90.0 + 60.0 * t));
        t -= (bx - x) / (120.0 + t * (-180.0 + 180.0 * t));
    }
    let y = 52.0 + 50.0 * t * t * (3.0 - 2.0 * t);
    let slope = 300.0 * t * (1.0 - t) / (120.0 + t * (-180.0 + 180.0 * t));
    return (xy.y - y) * inverseSqrt(1.0 + slope * slope);
}

fn gaussian_cdf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.2316419 * abs(x));
    let tail = 0.39894228 * exp(-0.5 * x * x) *
        t * (0.31938153 + t * (-0.35656378 + t * (1.78147794 + t * (-1.82125598 + t * 1.33027443))));
    return select(1.0 - tail, tail, x < 0.0);
}

fn blurred_stroke(d: f32, half_width: f32, sigma: f32) -> f32 {
    return gaussian_cdf((d + half_width) / sigma) - gaussian_cdf((d - half_width) / sigma);
}

fn hairline(below: vec4<f32>, d: f32, color: vec3<f32>, half_widths: vec2<f32>,
            opacity: f32, dpi: f32) -> vec4<f32> {
    let core = coverage(abs(d) - half_widths.x, dpi);
    let source = coverage(abs(d) - half_widths.y, dpi);
    let glow1 = blurred_stroke(d, half_widths.y, 1.5);
    // The second SVG blur takes result "glow1" as its default input.
    let glow2 = blurred_stroke(d, half_widths.y, 3.8078866);
    let filtered_alpha = 1.0 - (1.0 - glow1) * (1.0 - glow2) * (1.0 - source);
    // SVG filters operate on linear RGB. Match the dark outer halo of the reference
    // by quantizing its premultiplied color before returning to the sRGB layers.
    let linear = srgb_to_linear(color);
    let premul = floor(linear * filtered_alpha * 255.0 + 0.30) / 255.0;
    let filtered = linear_to_srgb(premul / max(filtered_alpha, 0.0001));
    return over(over(below, filtered, opacity * filtered_alpha), color, core);
}

fn tab_icon(i: u32, point: vec2<f32>, motion: f32, scale: f32, color: vec3<f32>,
            opacity: f32, glow: f32, below: vec4<f32>) -> vec4<f32> {
    let offset = select(vec2(0.0), vec2(0.0, -3.5), i == 0u) +
        select(vec2(0.0), vec2(3.5, 0.0), i == 1u);
    var pt = (point - motion * offset) / scale;
    if i == 2u {
        let angle = -0.2443461 * motion;
        pt = vec2(cos(angle) * pt.x - sin(angle) * pt.y, sin(angle) * pt.x + cos(angle) * pt.y);
    }
    if i == 3u {
        let split = p.title_opacity[1].y;
        var layer = below;
        layer = over(layer, color, opacity * centered_mask(MANAGER_PLATES[0], pt - vec2(0.0, -3.5) * split));
        layer = over(layer, color, opacity * centered_mask(MANAGER_PLATES[1], pt - vec2(-3.0, 2.0) * split));
        layer = over(layer, color, opacity * centered_mask(MANAGER_PLATES[2], pt - vec2(3.0, 2.0) * split));
        return over(layer, color,
            opacity * p.title_opacity[1].z *
            centered_mask(MANAGER_PLATES[3], pt / mix(0.85, 1.7, split)));
    }
    let glow_colors = array(vec3(168.0, 85.0, 247.0), vec3(0.0, 245.0, 212.0),
                            vec3(56.0, 189.0, 248.0), vec3(0.0), vec3(244.0, 114.0, 182.0));
    let tint = glow_colors[i] / 255.0;
    let shade = exp(-dot(pt, pt) / 128.0) * glow * 0.16;
    return over(over(below, tint, shade), color, opacity * centered_mask(TAB_ICONS[i], pt));
}

fn draw_nav(below: vec4<f32>, xy: vec2<f32>, width: f32, dpi: f32) -> vec4<f32> {
    let total = p.title_nav.z;
    let pt = xy - vec2((width - total) * 0.5, 43.0);
    let size = vec2(total, 46.0);
    if any(pt < vec2(-18.0)) || any(pt > size + 22.0) { return below; }
    let d = rounded_distance(pt, size, 23.0);
    let shadow = 0.45 * gaussian_cdf(-rounded_distance(pt - vec2(0, 4), size, 23.0) / 8.0);
    var color = over(below, vec3(0.0), shadow);
    let inside = coverage(d, dpi);
    if inside > 0.0 {
        color = over(color, NAV_FILL, inside);
        let glider = vec2((pt.x - p.title_nav.x) * 164.0 / p.title_nav.y, pt.y);
        let glider_d = rounded_distance(glider, vec2(164, 46), 23.0);
        let gradient = clamp((glider.x + glider.y) / 210.0, 0.0, 1.0);
        let purple = mix(vec3(124.0, 58.0, 237.0), vec3(99.0, 34.0, 219.0), gradient) / 255.0;
        color = over(color, purple, inside * coverage(glider_d, dpi));
    }
    let offsets = array(22.0, 18.0, 18.0, 18.0, 18.0);
    for (var i = 0u; i < 5u; i++) {
        let tab = p.title_tabs[i];
        let icon_size = TAB_ICONS[i].z;
        let x = pt.x - tab.x - offsets[i];
        let ink = p.title_tints[i / 4u][i % 4u];
        let selected = u32(p.title_nav.w) == i;
        let text_color = mix(LABEL_COLOR, vec3(1.0), ink);
        let icon_x = x - icon_size * 0.5;
        if abs(icon_x) < 35.0 {
            color = tab_icon(i, vec2(icon_x, pt.y - 23.0), tab.z, tab.w, text_color,
                             p.title_opacity[i / 4u][i % 4u], p.title_glows[i / 4u][i % 4u], color);
        }
        let text = TAB_TEXT[2u * i + u32(selected)];
        let alpha = sprite(text, vec2(x - icon_size - 9.0, pt.y - 11.0)).a;
        color = over(color, text_color, alpha);
    }
    if abs(d + 0.5) <= 14.0 {
        color = hairline(color, d + 0.5, ramp(pt.x - p.title_phase.y, 4), vec2(0.7, 1.6), 0.30, dpi);
    }
    return color;
}

fn draw_titlebar(xy: vec2<f32>, width: f32, dpi: f32) -> vec4<f32> {
    let left = (width - 940.0) * 0.5;
    var edge = xy.y - 52.0;
    if xy.y >= 37.0 { edge = title_edge(xy, left); }
    let alpha = coverage(edge, dpi);
    var color = vec4(CHROME_FILL * alpha, alpha);
    if xy.y < 52.0 && (xy.x < left || xy.x >= left + 940.0) {
        let logo = sprite(LOGO_SPRITE, xy - vec2(16.0, 12.0));
        color = over(color, logo.rgb, logo.a);
        color = over(color, LABEL_COLOR, sprite(NAME_SPRITE, xy - vec2(53.0, 15.0)).a);
    }
    if abs(edge) <= 14.0 {
        color = hairline(color, edge, ramp(xy.x - left - p.title_phase.x, 0), vec2(1.0, 2.1), 0.45, dpi);
    }
    if xy.y >= 24.0 { color = draw_nav(color, xy, width, dpi); }
    let right = width - xy.x;
    if p.title_phase.z > 0.5 && xy.y < 52.0 && right > 0.0 && right <= 192.0 {
        let slot = ceil(right / 48.0);
        let i = array(2u, 1u, 0u, 3u)[u32(slot) - 1u];
        let hover = p.title_controls[i];
        var ink = mix(CONTROL_COLOR, CONTROL_HOVER, hover);
        if i == 2u {
            color = over(color, vec3(232.0, 17.0, 35.0) / 255.0, hover);
            ink = mix(CONTROL_COLOR, vec3(1.0), hover);
        } else if i == 3u && p.title_phase.w > 0.0 {
            color = over(color, vec3(167.0, 139.0, 250.0) / 255.0, 0.16);
            ink = CONTROL_HOVER;
        } else {
            color = over(color, vec3(1.0), 0.08 * hover);
        }
        let point = vec2((slot - 0.5) * 48.0 - right, xy.y - 26.0);
        color = over(color, ink, centered_mask(CONTROL_SPRITES[i], point));
    }
    return color;
}

fn notification_panel(below: vec4<f32>, xy: vec2<f32>, width: f32, dpi: f32) -> vec4<f32> {
    let fade = p.title_phase.w;
    let size = vec2(min(440.0, width - 24.0), 180.0);
    let origin = vec2(width - 12.0 - size.x, 48.0);
    let scale = mix(0.98, 1.0, fade);
    let pt = (xy - origin - vec2(0.0, -6.0 * (1.0 - fade)) - size * 0.5) / scale + size * 0.5;
    let d = rounded_distance(pt, size, 14.0);
    if d > 28.0 { return below; }
    var color = over(below, vec3(0.0), fade * 0.45 * gaussian_cdf(-d / 12.0));
    let edge = coverage(d, dpi);
    if edge <= 0.0 { return color; }
    color = over(color, vec3(8.0, 8.0, 26.0) / 255.0, edge * fade * 0.94);
    let border = max(0.0, edge - coverage(d + 1.0, dpi));
    color = over(color, vec3(139.0, 92.0, 246.0) / 255.0, fade * 0.25 * border);
    color = over(color, vec3(124.0, 92.0, 255.0) / 255.0, fade * 0.18 * coverage(abs(pt.y - 55.5) - 0.5, dpi));
    let muted = vec3(125.0, 118.0, 168.0) / 255.0;
    let links = vec3(184.0, 175.0, 240.0) / 255.0;
    color = over(color, vec3(1.0), fade * sprite(NOTIFICATION_SPRITES[0], pt - vec2(19.0, 17.0)).a);
    color = over(color, links, fade * sprite(NOTIFICATION_SPRITES[1], pt - vec2(size.x - 188.0, 17.0)).a);
    color = over(color, links, fade * sprite(NOTIFICATION_SPRITES[2], pt - vec2(size.x - 66.0, 17.0)).a);
    color = over(color, muted, fade * sprite(NOTIFICATION_SPRITES[3], pt - vec2(size.x * 0.5 - 14.0, 90.0)).a);
    return over(color, muted, fade * sprite(NOTIFICATION_SPRITES[4], pt - vec2(size.x * 0.5 - 108.0, 126.0)).a);
}

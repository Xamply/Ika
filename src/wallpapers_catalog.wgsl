// Catalog chrome. Sizes and offsets are in the same logical pixels as the hit regions.
const WP_SOURCE_FILTERS = array(WP_FILTER_ALL, WP_FILTER_STEAM, WP_FILTER_MOTION, WP_FILTER_IMPORTED);
const WP_FILTER_X = array(20.0, 128.0, 362.0, 506.0);
const WP_FILTER_W = array(100.0, 226.0, 136.0, 133.0);
const WP_SOURCE_TINT = array(vec3(1.0), vec3(0.0, 240.0, 255.0) / 255.0,
    vec3(255.0, 45.0, 135.0) / 255.0, vec3(0.0, 255.0, 157.0) / 255.0);
const WP_SCROLLBAR_COLOR = vec3(75.0, 21.0, 242.0) / 255.0;
const WP_SCROLLBAR_HOT = vec3(88.0, 32.0, 255.0) / 255.0;

fn wp_source_filters(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    var ui = below;
    for (var i = 0u; i < 4u; i++) {
        let q = pt - vec2(WP_FILTER_X[i], 166.0);
        let size = vec2(WP_FILTER_W[i], 26.0);
        if any(q < vec2(-7.0)) || any(q > size + 7.0) { continue; }
        let selected = i == p.wall_catalog.x;
        let hot = p.wall_ui.y == 22u + i;
        let pink = vec3(217.0, 70.0, 239.0) / 255.0;
        let d = rounded_distance(q, size, 13.0);
        if selected { ui = over(ui, pink, 0.25 * gaussian_cdf(-d / 3.5)); }
        let dark = vec3(24.0, 17.0, 54.0) / 255.0;
        let base = select(mix(dark, vec3(35.0, 25.0, 74.0) / 255.0, f32(hot)),
                          mix(below.rgb, mix(vec3(139.0, 92.0, 246.0) / 255.0, pink, clamp(q.x / size.x, 0.0, 1.0)), 0.25),
                          selected);
        ui = wp_shape(ui, q, size, 13.0, base, select(mix(base, WP_TEXT, select(0.08, 0.18, hot)), pink, selected), dpi);
        if i > 0u {
            let dot = length(q - vec2(16.0, 13.0)) - 3.4;
            ui = over(ui, WP_SOURCE_TINT[i], 0.8 * gaussian_cdf(-dot / 2.7));
            ui = over(ui, WP_SOURCE_TINT[i], coverage(dot, dpi));
        }
        ui = wp_label(ui, q, select(26.0, 12.0, i == 0u), 13.0, WP_SOURCE_FILTERS[i],
                      select(vec3(203.0, 213.0, 225.0) / 255.0, WP_TEXT, selected || hot));
        let count = p.wall_sources[i];
        let digits = 1.0 + f32(count >= 10u) + f32(count >= 100u) + f32(count >= 1000u);
        let w = digits * 6.0 + 16.0;
        let x = size.x - 12.0 - w;
        let fill = mix(base, select(vec3(0.0), pink, selected), select(0.35, 0.30, selected));
        ui = wp_shape(ui, q - vec2(x, 6.0), vec2(w, 14.0), 7.0, fill, fill, dpi);
        ui = wp_number(ui, (q - vec2(x + 0.5 * w, 13.0)) / 0.80, count, select(WP_MUTED, WP_TEXT, selected));
    }
    let right = 1240.0 - WP_FILTER_READY.rect.z;
    ui = wp_label(ui, pt, right, 179.0, WP_FILTER_READY, WP_MUTED);
    let total = p.wall_sources.x;
    let digits = 1.0 + f32(total >= 10u) + f32(total >= 100u) + f32(total >= 1000u);
    return wp_number(ui, (pt - vec2(right - 4.0 - 3.2 * digits, 179.0)) / 0.8, total, WP_MUTED);
}

fn wp_toolbar_icon(ui: vec4<f32>, q: vec2<f32>, index: u32, dpi: f32) -> vec4<f32> {
    if any(abs(q) > vec2(20.0)) { return ui; }
    let link = (p.wall_ui.z & 64u) != 0u;
    let hot = p.wall_ui.y == array(19u, select(3u, 15u, link), 20u, 21u)[index];
    var out = ui;
    if index == 0u || (index == 1u && link) || hot {
        let violet = mix(WP_SCROLLBAR_COLOR, WP_SCROLLBAR_HOT, f32(hot));
        let fill = select(vec3(18.0, 13.0, 39.0) / 255.0, violet, index == 0u || (index == 1u && link));
        out = wp_shape(out, q + 19.0, vec2(38.0), 8.0, fill, mix(fill, WP_PURPLE, 0.16), dpi);
    }
    var d = 10.0;
    switch index {
        case 0u: {
            d = min(segment_distance(q, vec2(-4.0, 0.0), vec2(4.0, 0.0)),
                    segment_distance(q, vec2(0.0, -4.0), vec2(0.0, 4.0)));
        }
        case 1u: {
            if link {
                d = min(segment_distance(q, vec2(-4.5, 0.0), vec2(-1.2, 3.3)),
                        segment_distance(q, vec2(-1.2, 3.3), vec2(5.2, -3.3)));
            } else {
                let r = vec2(q.x - q.y, q.x + q.y) * 0.7071068;
                let a = abs(rounded_distance(r - vec2(-8.0, -2.7), vec2(7.7, 5.4), 2.6));
                let b = abs(rounded_distance(r - vec2(0.3, -2.7), vec2(7.7, 5.4), 2.6));
                d = min(select(10.0, a, r.x < -1.9), select(10.0, b, r.x > 1.9));
                d = min(d, segment_distance(r, vec2(-3.6, 0.0), vec2(3.6, 0.0)));
            }
        }
        case 2u: {
            let body = abs(rounded_distance(q + vec2(7.0, 4.0), vec2(14.0, 10.0), 1.5));
            d = select(body, body + 3.0, q.x < -1.0 && q.y < -3.0);
            d = min(d, segment_distance(q, vec2(-6.9, -3.2), vec2(-6.9, -5.8)));
            d = min(d, segment_distance(q, vec2(-6.9, -5.8), vec2(-2.6, -5.8)));
            d = min(d, segment_distance(q, vec2(-2.6, -5.8), vec2(-0.8, -4.0)));
        }
        default: {
            let circle = abs(length(q) - 5.5);
            d = select(circle, 10.0, abs(q.y) < 1.7);
            let r = select(q, -q, q.x < 0.0);
            d = min(d, segment_distance(r, vec2(6.0, -5.1), vec2(6.0, -1.4)));
            d = min(d, segment_distance(r, vec2(2.3, -1.4), vec2(6.0, -1.4)));
        }
    }
    return over(out, WP_TEXT, coverage(d - 0.72, dpi));
}

fn wp_downloaded_toolbar(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    var ui = below;
    if pt.y < 150.0 {
        let edit = (p.wall_ui.z & 1u) != 0u;
        let link = (p.wall_ui.z & 64u) != 0u;
        ui = wp_shape(ui, pt - vec2(20.0, 108.0), vec2(1027.0, 38.0), 8.0, vec3(11.0, 9.0, 27.0) / 255.0,
                      select(vec3(38.0, 25.0, 72.0) / 255.0, WP_PURPLE * 0.72, edit), dpi);
        if p.wall_rows[31].z > 0u {
            ui = wp_line(ui, pt - vec2(34.0, 127.0), 31u, WP_TEXT);
        } else if link {
            ui = wp_caption(ui, pt - vec2(34.0, 127.0), W_DIRECTO, WP_MUTED);
        }
        if edit {
            let x = 34.0 + f32(p.wall_rows[31].z >> 8u);
            ui = over(ui, WP_TEXT, coverage(abs(pt.x - x) - 0.5, dpi) * coverage(abs(pt.y - 127.0) - 7.0, dpi));
        }
        if pt.x >= 1052.0 && pt.x < 1244.0 {
            let i = u32((pt.x - 1052.0) / 48.0);
            ui = wp_toolbar_icon(ui, pt - vec2(1076.0 + f32(i) * 48.0, 127.0), i, dpi);
        }
    } else if pt.y >= 159.0 {
        ui = wp_source_filters(ui, pt, dpi);
    }
    return ui;
}

fn wp_chip(ui: vec4<f32>, q: vec2<f32>, w: f32, fill: vec3<f32>, opacity: f32,
           stroke: vec3<f32>, strength: f32, dpi: f32) -> vec4<f32> {
    let d = rounded_distance(q, vec2(w, 18.0), 4.0);
    let a = coverage(d, dpi);
    return over(over(ui, fill, a * opacity), stroke, (a - coverage(d + 1.0, dpi)) * strength);
}

fn wp_file_size(ui: vec4<f32>, q: vec2<f32>, encoded: u32, dpi: f32) -> vec4<f32> {
    let size = min(encoded - 1u, 999999u);
    let whole = size / 10u;
    let digits = 1u + u32(whole >= 10u) + u32(whole >= 100u) + u32(whole >= 1000u) + u32(whole >= 10000u);
    let numerals = f32(digits) * 6.2;
    let width = numerals + 42.0;
    let pt = q - vec2(224.0 - width, 8.0);
    if any(pt < vec2(-1.0)) || pt.x > width + 1.0 || pt.y > 19.0 { return ui; }
    var out = wp_chip(ui, pt, width, vec3(0.0), 0.78, WP_TEXT, 0.18, dpi);
    let x = pt.x - 8.0;
    if x >= 0.0 && x < numerals {
        let i = u32(x / 6.2);
        let digit = whole / array(1u, 10u, 100u, 1000u, 10000u)[digits - i - 1u] % 10u;
        out = over(out, WP_TEXT, wp_glyph(48u + digit, vec2((x - f32(i) * 6.2) / 0.80 + 0.4, (pt.y - 9.0) / 0.80 + 9.0)));
    } else if x >= numerals && x < numerals + 10.0 {
        out = over(out, WP_TEXT, wp_glyph(46u, vec2((x - numerals) / 0.80, (pt.y - 9.0) / 0.80 + 9.0)));
        out = over(out, WP_TEXT, wp_glyph(48u + size % 10u, vec2((x - numerals - 3.0) / 0.80, (pt.y - 9.0) / 0.80 + 9.0)));
    }
    return wp_label(out, pt, numerals + 22.0, 9.0, WP_BADGE_MB, WP_TEXT);
}

fn wp_catalog_badges(ui: vec4<f32>, q: vec2<f32>, flags: u32, size: u32, dpi: f32) -> vec4<f32> {
    var out = ui;
    let source = (flags >> 12u) & 3u;
    let resolution = (flags >> 14u) & 3u;
    let scene = (flags & 2u) != 0u;
    let motion = source == 2u || (flags & 64u) != 0u;
    let imported = source == 3u;
    let color = select(WP_SOURCE_TINT[select(select(1u, 2u, motion), 3u, imported)], WP_TEXT, scene);
    var label = WP_BADGE_STEAM;
    if motion { label = WP_BADGE_MOTION; }
    else if imported { label = WP_BADGE_IMPORTED; }
    if scene { label = WP_SCENE; }
    let width = label.rect.z + select(16.0, 18.0, motion || imported);
    let pt = q - vec2(8.0);
    if pt.x <= width + 4.0 {
        if scene {
            let d = rounded_distance(pt, vec2(width, 18.0), 4.0);
            out = over(out, vec3(147.0, 51.0, 234.0) / 255.0, 0.65 * gaussian_cdf(-d / 3.4));
            let ramp = clamp(dot(pt / vec2(width, 18.0), vec2(0.72, 0.28)), 0.0, 1.0);
            let fill = mix(vec3(147.0, 51.0, 234.0), vec3(6.0, 182.0, 212.0), ramp) / 255.0;
            out = wp_chip(out, pt, width, fill, 1.0, WP_TEXT, 0.50, dpi);
        } else {
            let fill = select(select(vec3(16.0, 32.0, 58.0), vec3(45.0, 10.0, 35.0), motion), vec3(10.0, 40.0, 30.0), imported) / 255.0;
            out = wp_chip(out, pt, width, fill, 0.90, color, 0.40, dpi);
        }
        out = wp_label(out, pt, (width - label.rect.z) * 0.5, 9.0, label, color);
    }
    if resolution > 0u {
        let s = array(WP_BADGE_4K, WP_BADGE_1440P, WP_BADGE_1080P)[resolution - 1u];
        let x = max(width - 1.0, 0.0);
        let w = s.rect.z + 11.0;
        if abs(pt.x - x - w * 0.5) < w * 0.5 + 1.0 {
            out = wp_chip(out, pt - vec2(x, 0.0), w, vec3(0.0), 0.78, WP_TEXT, 0.15, dpi);
            out = wp_label(out, pt, x + (w - s.rect.z) * 0.5, 9.0, s, WP_TEXT);
        }
    }
    if size > 0u {
        out = wp_file_size(out, q, size, dpi);
    } else if (flags & 64u) != 0u && pt.x >= 126.0 {
        let subtle = (flags & 1u) == 0u;
        out = wp_caption(out, pt - vec2(130.0, 9.0), W_DESCARGAR, select(WP_TEXT, WP_TEAL, subtle));
    } else if (flags & 512u) != 0u && pt.x >= 120.0 {
        out = wp_caption(out, pt - vec2(122.0, 9.0), W_WEB, WP_TEXT);
    }
    return out;
}

fn wp_scrollbar(ui: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    let bar = p.wall_scrollbar;
    if bar.y <= 0.0 { return ui; }
    let q = pt - vec2(1245.0, bar.x);
    let d = rounded_distance(q, vec2(6.0, bar.y), 3.0);
    return over(ui, select(WP_SCROLLBAR_COLOR, WP_SCROLLBAR_HOT, bar.z > 0.0 || bar.w > 0.0), coverage(d, dpi));
}

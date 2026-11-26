struct HomeText {
    words: array<vec4<u32>, 4>,
    starts: array<vec4<u32>, 4>,
    metrics: vec4<u32>,
};

@group(0) @binding(9) var home_atlas: texture_2d<f32>;

fn home_byte(a: array<vec4<u32>, 4>, i: u32) -> u32 {
    return (a[i / 16u][(i / 4u) % 4u] >> ((i % 4u) * 8u)) & 255u;
}

fn home_label(ui: vec4<f32>, q: vec2<f32>, idx: u32, tint: vec3<f32>, center: bool) -> vec4<f32> {
    let metrics = p.home_text[idx].metrics;
    let x = q.x + select(0.0, f32(metrics.x) * 0.5, center);
    if x < 0.0 || x >= f32(metrics.x) || abs(q.y) >= 9.0 { return ui; }
    var lo = 0u;
    var hi = metrics.y;
    while hi > lo + 1u {
        let mid = (lo + hi) / 2u;
        if f32(home_byte(p.home_text[idx].starts, mid)) > x { hi = mid; } else { lo = mid; }
    }
    let code = home_byte(p.home_text[idx].words, lo);
    let glyph = vec2(x - f32(home_byte(p.home_text[idx].starts, lo)) + 1.0, q.y + 9.0);
    return over(ui, tint, wp_glyph(code, glyph));
}

fn home_sample(origin: vec2<f32>, size: vec2<f32>, uv: vec2<f32>) -> vec4<f32> {
    let at = origin + clamp(uv * size, vec2(0.5), size - 0.5);
    return textureSampleLevel(home_atlas, smooth_sampler, at / vec2<f32>(textureDimensions(home_atlas)), 0.0);
}

fn home_line(pt: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ba = b - a;
    let h = clamp(dot(pt - a, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
    return length(pt - (a + ba * h));
}

fn sd_star5(p: vec2<f32>, r: f32, rf: f32) -> f32 {
    let k1 = vec2(0.809016994375, -0.587785252292);
    let k2 = vec2(-k1.x, k1.y);
    var pt = vec2(abs(p.x), -p.y);
    pt -= 2.0 * max(dot(k1, pt), 0.0) * k1;
    pt -= 2.0 * max(dot(k2, pt), 0.0) * k2;
    pt.x = abs(pt.x);
    pt.y -= r;
    let ba = rf * vec2(-k1.y, k1.x) - vec2(0.0, 1.0);
    let h = clamp(dot(pt, ba) / dot(ba, ba), 0.0, r);
    return length(pt - ba * h) * sign(pt.y * ba.x - pt.x * ba.y);
}

fn create_inst_border_color(t: f32) -> vec3<f32> {
    let u = fract(t);
    if u < 0.18 {
        return mix(vec3(0.027, 0.396, 0.451), vec3(0.000, 0.659, 0.710), u / 0.18);
    } else if u < 0.36 {
        return mix(vec3(0.000, 0.659, 0.710), vec3(0.000, 0.941, 0.902), (u - 0.18) / 0.18);
    } else if u < 0.50 {
        return mix(vec3(0.000, 0.941, 0.902), vec3(0.494, 0.988, 0.965), (u - 0.36) / 0.14);
    } else if u < 0.64 {
        return mix(vec3(0.494, 0.988, 0.965), vec3(0.000, 0.898, 0.875), (u - 0.50) / 0.14);
    } else if u < 0.82 {
        return mix(vec3(0.000, 0.898, 0.875), vec3(0.031, 0.569, 0.698), (u - 0.64) / 0.18);
    } else {
        return mix(vec3(0.031, 0.569, 0.698), vec3(0.027, 0.396, 0.451), (u - 0.82) / 0.18);
    }
}

fn home_box_glow(d: f32, sigma: f32) -> f32 {
    // Approximate the Gaussian blur of the button's silhouette near its edge.
    let t = abs(d) / sigma;
    let tail = 0.5 * exp(-0.5 * t * t - 0.8 * t);
    return select(1.0 - tail, tail, d >= 0.0);
}

fn home_create_button(ui: vec4<f32>, q: vec2<f32>, size: vec2<f32>, d: f32, mask: f32, aa: f32) -> vec4<f32> {
    let hover = p.home_toolbar.w;
    let shadow_d = rounded_distance(q - vec2(0.0, 2.0), size, size.y * 0.5);
    let rest = (1.0 - hover) * 0.20 * home_box_glow(shadow_d, 4.0);
    let hot = hover * 0.35 * home_box_glow(d, 8.0);
    var out = over(ui, vec3(16.0, 185.0, 129.0) / 255.0, rest * (1.0 - mask));
    out = over(out, vec3(52.0, 211.0, 153.0) / 255.0, hot * (1.0 - mask));

    let fill_t = clamp((q.x + q.y) / (size.x + size.y), 0.0, 1.0);
    let fill = mix(vec3(11.0, 59.0, 44.0), vec3(7.0, 58.0, 46.0), fill_t) / 255.0;
    out = over(out, fill, mask * mix(0.90, 0.94, fill_t));

    // The SVG has a 1.2 px core over a 2.2 px stroke with two blur layers.
    let inset_size = size - vec2(1.5);
    let stroke_d = abs(rounded_distance(q - vec2(0.75), inset_size, min(17.0, inset_size.y * 0.5)));
    let stroke = create_inst_border_color(q.x / 260.0 + p.home_phase.y);
    let dist_sq = stroke_d * stroke_d;
    let wide = 1.0 - smoothstep(1.1 - aa, 1.1 + aa, stroke_d);
    let narrow_blur = 0.585 * exp(-dist_sq / 4.5);
    let wide_blur = 0.251 * exp(-dist_sq / 24.5);
    let bloom = mix(0.5, 0.68, hover) * (1.0 - (1.0 - wide) * (1.0 - narrow_blur) * (1.0 - wide_blur));
    out = over(out, stroke, bloom);
    let core = 1.0 - smoothstep(0.6 - aa, 0.6 + aa, stroke_d);
    return over(out, stroke, core);
}

// The same angle, taken the short way within a symmetry `period`: drawing is
// unchanged, but scaling it towards 0 returns the shortest way to rest.
fn home_wrap(angle: f32, period: f32) -> f32 {
    return angle - period * round(angle / period);
}

fn home_rotate(q: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2(c * q.x - s * q.y, s * q.x + c * q.y);
}

// Ease-out with a small overshoot that lands exactly on 1, so an animation
// that ends stops on its resting pose without a jump.
fn home_spring(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) - 1.0;
    return 1.0 + 2.70158 * x * x * x + 1.70158 * x * x;
}

fn home_toolbar_icon(ui: vec4<f32>, center: vec2<f32>, kind: u32, tint: vec3<f32>, aa: f32) -> vec4<f32> {
    let q = center;
    if any(abs(q) > vec2(select(11.0, 14.0, kind == 4u))) { return ui; }
    var d = 100.0;
    var extra = 0.0;
    var extra_tint = 0.0;
    var glint = 0.0;
    if kind == 0u {
        let h_line = home_line(q, vec2(-5.5, 0.0), vec2(5.5, 0.0)) - 1.25;
        let v_line = home_line(q, vec2(0.0, -5.5), vec2(0.0, 5.5)) - 1.25;
        d = min(h_line, v_line);
    } else if kind == 1u {
        // Todas: the four tiles spread out, turn a quarter and snap back.
        let t = p.home_controls[6].x;
        let w = p.home_controls[7].x;
        // Quarter turns look identical, so an interrupted turn returns the short way.
        let turn = home_wrap(-1.5708 * home_spring(t), 1.5708) * w;
        let burst = sin(3.14159 * clamp(t * 1.4, 0.0, 1.0)) * w;
        let r = home_rotate(q, turn);
        let pq = abs(r) - vec2(3.6 + 1.3 * burst);
        let bq = abs(pq) - vec2(2.1 - 0.35 * burst);
        d = length(max(bq, vec2(0.0))) + min(max(bq.x, bq.y), 0.0) - 1.2;
    } else if kind == 2u {
        // Recientes: time flies. The minute hand spins two turns leaving a
        // fading trail, the hour hand one turn, the hour marks light up and
        // the dial breathes; both hands land back on their resting time.
        let t = p.home_controls[6].y;
        let w = p.home_controls[7].y;
        let sweep = home_spring(t);
        let speed = sin(3.14159 * clamp(t, 0.0, 1.0)) * w;
        let dial = 1.0 + 0.10 * speed;
        let ring = abs(length(q) - 5.8 * dial) - 1.0;
        let hour = home_rotate(vec2(0.0, -3.8), home_wrap(6.2832 * sweep, 6.2832) * w);
        let minute = home_rotate(vec2(3.0, 1.4), home_wrap(12.566 * sweep, 6.2832) * w);
        let h_hand = home_line(q, vec2(0.0, 0.0), hour) - 1.0;
        let m_hand = home_line(q, vec2(0.0, 0.0), minute) - 1.0;
        d = min(ring, min(h_hand, m_hand));
        if speed > 0.001 {
            let radius = length(q);
            let angle = atan2(q.y, q.x);
            // Trail behind the minute hand, longer while it is fastest.
            let behind = fract((atan2(minute.y, minute.x) - angle) / 6.2832) * 6.2832;
            let reach = 2.6 * speed;
            let trail = (1.0 - behind / max(reach, 0.001)) * step(behind, reach)
                * (1.0 - smoothstep(3.0, 3.6, radius)) * smoothstep(0.6, 1.2, radius);
            extra_tint = max(extra_tint, trail * 0.55);
            // The twelve hour marks flash just inside the dial.
            let snapped = round(angle / 0.5236) * 0.5236;
            let mark_dir = vec2(cos(snapped), sin(snapped));
            let mark = home_line(q, mark_dir * 3.9 * dial, mark_dir * 4.5 * dial) - 0.45;
            extra_tint = max(extra_tint, (1.0 - smoothstep(-aa, aa, mark)) * speed);
        }
    } else if kind == 3u {
        // Favoritas: the star spins, pops bigger and throws four sparkles.
        let t = p.home_controls[6].z;
        let w = p.home_controls[7].z;
        let pop = 1.0 + 0.28 * sin(3.14159 * clamp(t * 1.6, 0.0, 1.0)) * w;
        let r = home_rotate(q, home_wrap(1.2566 * home_spring(t), 1.2566) * w) / pop;
        d = sd_star5(r, 7.2, 0.44) * pop;
        let spark_t = clamp((t - 0.15) / 0.6, 0.0, 1.0);
        if spark_t > 0.0 && spark_t < 1.0 && w > 0.01 {
            let reach = 6.0 + 4.5 * spark_t;
            let fade = (1.0 - spark_t) * w;
            for (var i = 0u; i < 4u; i++) {
                let dir = home_rotate(vec2(0.0, -1.0), 0.7854 + 1.5708 * f32(i));
                let dot_d = length(q - dir * reach) - 1.1 * fade;
                extra = max(extra, (1.0 - smoothstep(-aa, aa, dot_d)) * fade);
            }
        }
    } else {
        // Buscar: the lens reveals. A soft light fills the glass from the
        // rim towards the center like an iris opening, the handle gives a
        // small recoil, and the light fades as the iris closes again.
        // A click zooms the lens in and out once (home_controls[4].y).
        let t = p.home_controls[6].w;
        let w = p.home_controls[7].w;
        let click = p.home_controls[4].y;
        let zoom = 1.0 + 0.32 * sin(3.14159 * clamp(click * 1.6, 0.0, 1.0)) * (1.0 - click);
        let open = sin(3.14159 * clamp(t, 0.0, 1.0)) * w;
        let recoil = sin(3.14159 * clamp(t * 2.2, 0.0, 1.0)) * w;
        let m = q / zoom;
        let lens_c = vec2(-1.8, -1.8);
        let ring = (abs(length(m - lens_c) - 4.6) - 1.05) * zoom;
        let handle = (home_line(m, vec2(1.8, 1.8) + vec2(0.9) * recoil, vec2(6.2, 6.2) + vec2(0.9) * recoil) - 1.15) * zoom;
        d = min(ring, handle);
        // Iris of light inside the glass: an annulus that grows inwards.
        // Lit between the rim (r = 3.4) and an inner edge that closes towards
        // the center as the iris opens.
        let r = length(m - lens_c);
        let inner = mix(3.4, 0.0, open);
        let iris = smoothstep(inner - 0.5, inner + 0.5, r) * (1.0 - smoothstep(3.2, 3.6, r));
        glint = iris * open * 0.55;
    }
    let mask = max(1.0 - smoothstep(-aa, aa, d), extra_tint);
    let lit = over(ui, tint, mask);
    return over(over(lit, vec3(1.0, 0.93, 0.55), extra), vec3(1.0), glint);
}

fn home_toolbar_text(ui: vec4<f32>, pt: vec2<f32>, label: vec4<f32>, tint: vec3<f32>) -> vec4<f32> {
    let at = pt * HOME_LABEL_RASTER_SCALE;
    if any(at < vec2(0.0)) || any(at >= label.zw) { return ui; }
    let dims = vec2<f32>(textureDimensions(home_atlas));
    let origin = label.xy + vec2(0.0, dims.y - HOME_LABEL_STRIP_HEIGHT);
    let uv = (origin + clamp(at, vec2(0.5), label.zw - 0.5)) / dims;
    let spread = max(0.0, 0.25 * (HOME_LABEL_RASTER_SCALE / p.home_toolbar_view.z - 1.0));
    var ink = textureSampleLevel(home_atlas, smooth_sampler, uv, 0.0).a;
    if spread > 0.0 {
        let d = vec2(spread) / dims;
        let diag = vec2(d.x, -d.y);
        ink = 0.25 * (textureSampleLevel(home_atlas, smooth_sampler, uv - d, 0.0).a +
                      textureSampleLevel(home_atlas, smooth_sampler, uv + d, 0.0).a +
                      textureSampleLevel(home_atlas, smooth_sampler, uv - diag, 0.0).a +
                      textureSampleLevel(home_atlas, smooth_sampler, uv + diag, 0.0).a);
    }
    return over(ui, tint, ink);
}

fn home_toolbar_button(ui: vec4<f32>, q: vec2<f32>, rect: vec4<f32>, kind: u32,
                      aa: f32) -> vec4<f32> {
    let radius = rect.w * 0.5;
    let d = rounded_distance(q - rect.xy, rect.zw, radius);
    if d > select(aa, p.home_toolbar_view.w, kind == 0u) { return ui; }

    var out = ui;
    if kind == 0u {
        let mask = 1.0 - smoothstep(-aa, aa, d);
        out = home_create_button(out, q - rect.xy, rect.zw, d, mask, aa);
    }
    // Hover (home_controls[3]): a soft lit capsule under the filter and a
    // lift of its icon; the search field brightens and glows.
    var hot = 0.0;
    if kind >= 1u && kind <= 3u {
        hot = p.home_controls[3][kind - 1u];
        let selected_tab = f32(kind - 1u) == p.home_phase.z;
        let shown = clamp(hot, 0.0, 1.0) * select(1.0, 0.0, selected_tab);
        let mask = 1.0 - smoothstep(-aa, aa, d);
        out = over(out, vec3(0.62, 0.55, 0.95), mask * 0.16 * shown);
        let rim = 1.0 - smoothstep(-1.2, 0.0, abs(d));
        out = over(out, vec3(0.70, 0.62, 1.0), rim * mask * 0.45 * shown);
    } else if kind == 4u {
        hot = p.home_controls[3].w;
    }

    // Center the icon and label together in the button or search field.
    var label_index = 0u;
    if kind == 4u {
        label_index = 7u;
    } else if kind > 0u {
        let tab = kind - 1u;
        label_index = 1u + 2u * tab + u32(f32(tab) == p.home_phase.z);
    }
    let label = HOME_TOOLBAR_LABELS[label_index];
    let text_size = label.zw / HOME_LABEL_RASTER_SCALE;
    let icon_scale = select(1.08, 1.02, kind == 0u);
    let icon_w = 15.0 * icon_scale;
    let gap = 7.0;
    let content_w = icon_w + gap + text_size.x;
    let content_x = rect.x + (rect.z - content_w) * 0.5;
    let mid_y = rect.y + rect.w * 0.5;

    let icon_center = vec2(content_x + icon_w * 0.5, mid_y);
    let create_tint = mix(vec3(0.431, 0.906, 0.718), vec3(1.0), p.home_toolbar.w);
    let button_tint = select(vec3(1.0), create_tint, kind == 0u);
    let search_tint = mix(vec3(0.580, 0.639, 0.722), vec3(0.82, 0.78, 1.0), clamp(hot, 0.0, 1.0));
    let icon_tint = select(button_tint, search_tint, kind == 4u);
    var icon_q = q - icon_center;
    // Filters animate their own icon (home_toolbar_icon); only the capsule
    // glow is shared.
    var scale = icon_scale;
    if kind == 0u {
        let motion = p.home_toolbar.z;
        let angle = radians(-90.0 * motion);
        let c = cos(angle);
        let s = sin(angle);
        icon_q = vec2(c * icon_q.x - s * icon_q.y, s * icon_q.x + c * icon_q.y);
        scale *= 1.0 + 0.12 * motion;
    }
    if kind == 4u {
        // Opening the field slides the magnifier from the centered placeholder
        // to the left edge while the placeholder fades into the typed text.
        let reveal = p.home_controls[4].x;
        let shown = clamp(reveal, 0.0, 1.0);
        let lens = vec2(mix(icon_center.x, rect.x + 22.0, reveal), mid_y);
        let lens_tint = mix(icon_tint, vec3(0.78, 0.82, 0.95), shown);
        out = home_toolbar_icon(out, (q - lens) / scale, 4u, lens_tint, aa / scale);
        let text_x = mix(content_x + icon_w + gap, rect.x + 40.0, reveal);
        let placeholder = home_toolbar_text(out, q - vec2(text_x, mid_y - text_size.y * 0.5), label,
            mix(vec3(0.392, 0.455, 0.545), vec3(0.70, 0.72, 0.86), clamp(hot, 0.0, 1.0)));
        out = mix(out, placeholder, 1.0 - shown);
        let typed = home_label(out, q - vec2(text_x, mid_y), 13u, vec3(0.93, 0.95, 1.0), false);
        return mix(out, typed, shown);
    }
    out = home_toolbar_icon(out, icon_q / scale, kind, icon_tint, aa / scale);

    let text_tint = button_tint;
    let text_origin = vec2(content_x + icon_w + gap, mid_y - text_size.y * 0.5);
    return home_toolbar_text(out, q - text_origin, label, text_tint);
}

fn home_toolbar(ui: vec4<f32>, q: vec2<f32>, aa: f32) -> vec4<f32> {
    let group = p.home_toolbar_rects[0];
    let search = p.home_toolbar_rects[5];
    let group_d = rounded_distance(q - group.xy, group.zw, group.w * 0.5);
    let search_d = rounded_distance(q - search.xy, search.zw, search.w * 0.5);

    // Group glass background
    let group_mask = 1.0 - smoothstep(-aa, aa, group_d);
    let group_bg = mix(vec3(0.078, 0.055, 0.157), vec3(0.047, 0.031, 0.098),
                       clamp((q.y - group.y) / group.w, 0.0, 1.0));
    var out = over(ui, group_bg, group_mask * 0.72);

    // Group 1px border and top highlight
    let group_edge = 1.0 - smoothstep(-1.2, 0.0, abs(group_d));
    let top_highlight = 1.0 - smoothstep(group.y, group.y + 12.0, q.y);
    let group_border_color = mix(vec3(0.35, 0.38, 0.58) * 0.35, vec3(0.70, 0.75, 0.95) * 0.45, top_highlight);
    out = over(out, group_border_color, group_edge * group_mask * 0.65);

    // Search dark capsule background
    let search_mask = 1.0 - smoothstep(-aa, aa, search_d);
    let search_hot = clamp(p.home_controls[3].w, 0.0, 1.0);
    // The halo breathes in a loop while hovered (rise, fall, rise...). Leaving
    // fades it out from wherever it was, through the eased hover level.
    // 2.5 s divides the 10 s phase wrap, so the loop never jumps.
    let breath = 0.5 - 0.5 * cos(6.2832 * p.home_phase.x / 2.5);
    let halo_level = mix(0.25, 1.0, breath) * clamp(p.home_controls[3].w, 0.0, 1.0);
    let halo = exp(-max(search_d, 0.0) / 7.0) * (1.0 - search_mask);
    out = over(out, vec3(0.545, 0.361, 0.965), halo * 0.30 * halo_level);
    out = over(out, mix(vec3(0.045, 0.035, 0.09), vec3(0.085, 0.065, 0.16), search_hot), search_mask * 0.88);

    // Search 1px border, violet while the field has focus.
    let search_edge = 1.0 - smoothstep(-1.2, 0.0, abs(search_d));
    let focused = p.home_phase.w > 1.5;
    let idle_border = mix(vec3(0.35, 0.38, 0.55) * 0.35, vec3(0.62, 0.50, 0.95), search_hot);
    let search_border = select(idle_border, vec3(0.545, 0.361, 0.965), focused);
    out = over(out, search_border, search_edge * search_mask * select(0.65 + 0.35 * search_hot, 1.0, focused));

    // Move one highlight under the fixed filter labels, including across their gaps.
    let pill_template = p.home_toolbar_rects[2];
    let pill_pt = q - vec2(p.home_toolbar.x, pill_template.y);
    let pill_size = vec2(p.home_toolbar.y, pill_template.w);
    let pill_d = rounded_distance(pill_pt, pill_size, pill_size.y * 0.5);
    let pill_mask = 1.0 - smoothstep(-aa, aa, pill_d);
    let pill_bg = mix(vec3(0.486, 0.227, 0.929), vec3(0.388, 0.133, 0.859),
                     clamp(pill_pt.x / pill_size.x, 0.0, 1.0));
    out = over(out, pill_bg, pill_mask * group_mask * 0.98);

    out = home_toolbar_button(out, q, p.home_toolbar_rects[1], 0u, aa);
    out = home_toolbar_button(out, q, p.home_toolbar_rects[2], 1u, aa);
    out = home_toolbar_button(out, q, p.home_toolbar_rects[3], 2u, aa);
    out = home_toolbar_button(out, q, p.home_toolbar_rects[4], 3u, aa);
    out = home_toolbar_button(out, q, search, 4u, aa);
    return out;
}

fn home_card(ui: vec4<f32>, pt: vec2<f32>, slot: u32, aa: f32) -> vec4<f32> {
    let rect = p.home_rects[slot];
    if rect.z <= 0.0 { return ui; }
    let scale = p.home_view.z;
    let margin = 24.0 * scale;
    if pt.x < rect.x - margin || pt.x > rect.x + rect.z + margin || pt.y < rect.y - margin || pt.y > rect.y + rect.w + margin {
        return ui;
    }
    let selected = slot == 2u;
    let far = slot == 0u || slot == 4u;
    // Eased hover (home_controls[5]) for the side cards: the card rises a
    // little and grows, its cover zooms and a soft light sweeps across it.
    let hover_slot = select(select(slot, slot - 1u, slot > 2u), 0u, selected);
    let lift = select(clamp(p.home_controls[5][hover_slot], 0.0, 1.2), 0.0, selected);
    // The selected card has its own, bigger moment (home_controls[8]).
    let star = select(0.0, clamp(p.home_controls[8].x, 0.0, 1.2), selected);
    let star_shown = clamp(star, 0.0, 1.0);
    let grow = 1.0 + 0.035 * lift + 0.045 * star;
    let center = rect.xy + rect.zw * 0.5 - vec2(0.0, 7.0 * scale * lift + 9.0 * scale * star);
    let q = (pt - center) / (scale * grow) + rect.zw / scale * 0.5;
    let size = rect.zw / scale;
    if any(q < vec2(-24.0)) || any(q > size + 24.0) { return ui; }
    let hot = lift > 0.001;
    let radius = select(select(14.0, 12.0, far), 16.0, selected);
    let d = rounded_distance(q, size, radius);
    let inside = 1.0 - smoothstep(-aa, aa, d);
    let info = p.home_images[slot];
    if info.x == 0u {
        var ghost = over(ui, vec3(0.055, 0.04, 0.11), inside * 0.38);
        let dash = step(0.45, fract((q.x + q.y) / 11.0));
        ghost = over(ghost, vec3(0.78, 0.80, 0.92), inside * smoothstep(-2.0, 0.0, d) * 0.22 * dash);
        let c = abs(q - size * 0.5);
        let plus = min(max(c.x - 8.0, c.y - 0.8), max(c.y - 8.0, c.x - 0.8));
        return over(ghost, vec3(0.64, 0.67, 0.80), 0.57 * (1.0 - smoothstep(-aa, aa, plus)));
    }
    var out = ui;
    if selected {
        // The halo widens and brightens towards violet while hovered.
        let halo = exp(-max(d + 1.0, 0.0) * mix(0.30, 0.16, star_shown));
        out = over(out, mix(vec3(0.0, 0.14, 0.80), vec3(0.42, 0.20, 0.95), star_shown), halo * mix(0.52, 0.78, star_shown));
    }
    if inside <= 0.0 { return out; }
    let custom = info.w > 0u;
    let index = select(info.x, info.w, custom) - 1u;
    let origin = vec2(f32(index % 5u) * 320.0, f32(index / 5u) * 192.0 + select(0.0, 2352.0, custom));
    let crop = cover_scale(size.x / size.y, 320.0 / 192.0);
    // Hovered selected card: slow zoom plus a gentle drift, like a living cover.
    let drift = vec2(sin(p.home_controls[8].y * 0.9), cos(p.home_controls[8].y * 0.7)) * 0.012 * star_shown;
    let uv = (q / size - 0.5) * crop / (1.0 + 0.06 * clamp(lift, 0.0, 1.0) + 0.08 * star_shown) + 0.5 + drift;
    let image = home_sample(origin, vec2(320.0, 192.0), uv).rgb;
    let y = q.y / size.y;
    let fade = 0.45 * smoothstep(0.24, 0.55, y) + 0.49 * smoothstep(0.55, 1.0, y);
    var card = vec4(mix(image, vec3(0.016, 0.016, 0.086), fade), 1.0);

    let factor = select(select(0.88, 0.78, far), 1.0, selected);
    let icon = select(select(38.0, 32.0, far), 44.0, selected);
    // The icon rises a little out of the card when the selected card is hovered.
    let icon_pos = vec2(12.0 * factor, size.y - icon - 10.0 * factor - 5.0 * star);
    let icon_q = q - icon_pos;
    if all(icon_q >= vec2(-aa)) && all(icon_q <= vec2(icon + aa)) {
        let icon_d = rounded_distance(icon_q, vec2(icon), 10.0 * factor);
        let mask = 1.0 - smoothstep(-aa, aa, icon_d);
        let io = vec2(f32(info.y % 33u) * 48.0, 1536.0 + f32(info.y / 33u) * 48.0);
        let avatar = home_sample(io, vec2(48.0), icon_q / icon);
        card = over(card, vec3(0.035, 0.04, 0.08), mask);
        card = overlay_over(card, avatar * mask);
        card = over(card, vec3(0.8, 0.85, 1.0), mask * smoothstep(-1.8, 0.0, icon_d) * 0.23);
    }
    let text_x = icon_pos.x + icon + 10.0 * factor;
    card = home_label(card, (q - vec2(text_x, size.y - 41.0 * factor)) / factor, slot * 2u, vec3(0.99), false);
    card = home_label(card, (q - vec2(text_x, size.y - 23.0 * factor)) / (factor * 0.81), slot * 2u + 1u,
        vec3(0.72, 0.78, 0.90), false);
    if info.z != 0u {
        let dot = length(q - vec2(size.x - 17.0, 17.0));
        card = over(card, vec3(0.30, 0.95, 0.63), 1.0 - smoothstep(4.0 - aa, 4.0 + aa, dot));
    }
    if selected {
        card = over(card, vec3(0.012, 0.29, 0.84), smoothstep(-10.0, -0.1, d) * 0.76);
        card = over(card, vec3(0.28, 0.52, 1.0), smoothstep(-2.6 - 1.0 * star_shown, 0.0, d) * 0.9);
        // Exact WebView2 TopRosao gradient: the native atlas contains its
        // original 293 stops, and this reproduces animateTransform's
        // -139 -> 221 degree rotation around (160,96) every 10 seconds.
        let edge = 1.0 - smoothstep(0.9 + 0.8 * star_shown, 3.4 + 1.8 * star_shown, abs(d));
        // The gradient border speeds up while hovered (10 s turn -> ~3 s).
        let angle = radians(-139.0 + 36.0 * p.home_phase.x + 120.0 * p.home_controls[8].y * star_shown);
        let centered = q - vec2(160.0, 96.0);
        let rotated_x = cos(angle) * centered.x - sin(angle) * centered.y + 160.0;
        let gradient_t = clamp(rotated_x / 320.0, 0.0, 1.0);
        let stroke = home_sample(vec2(0.0, 2304.0), vec2(1024.0, 1.0),
            vec2(gradient_t, 0.5)).rgb;
        card = over(card, stroke, edge * 0.98);
        if star > 0.001 {
            // Holographic sheen: a soft diagonal band loops across the cover,
            // tinted by the border gradient, plus a bright first pass on entry.
            let loop_x = fract(p.home_controls[8].y / 2.6);
            let band = (q.x - q.y * 0.55) / size.x - mix(-0.5, 1.5, loop_x);
            let sheen = exp(-band * band * 40.0) * 0.16 * star_shown;
            let tint = home_sample(vec2(0.0, 2304.0), vec2(1024.0, 1.0), vec2(fract(loop_x + q.x / size.x), 0.5)).rgb;
            card = over(card, mix(vec3(1.0), tint, 0.5), sheen);
            let first = (q.x - q.y * 0.55) / size.x - mix(-0.5, 1.5, clamp(star, 0.0, 1.0));
            card = over(card, vec3(1.0), exp(-first * first * 30.0) * 0.22 * star_shown * (1.0 - star_shown * 0.7));
        }
    } else {
        let shown = clamp(lift, 0.0, 1.0);
        card = vec4(card.rgb * (1.0 + 0.10 * shown), 1.0);
        // A diagonal band of light crosses the cover while the hover grows.
        let band = (q.x - q.y * 0.6) / size.x - mix(-0.4, 1.3, shown);
        card = over(card, vec3(1.0), exp(-band * band * 60.0) * 0.20 * shown * (1.0 - shown * 0.6));
    }
    return over(out, card.rgb, inside);
}

fn home_arrow(ui: vec4<f32>, pos: vec2<f32>, i: u32, aa: f32) -> vec4<f32> {
    let rect = p.home_controls[i];
    let scale = p.home_view.z;
    let dir = select(-1.0, 1.0, i == 1u);
    // Hover (eased, may overshoot) and click (0..1 since the last press).
    let hot = p.home_controls[9][i];
    // The whole button slides outwards, towards where it points.
    let center = rect.xy + rect.zw * 0.5 + vec2(dir * 6.0 * hot * scale, 0.0);
    let hot_c = clamp(hot, 0.0, 1.0);
    let click = p.home_controls[9][i + 2u];
    let pressing = click < 1.0;
    // Squeeze in on press, then spring back.
    let squeeze = select(0.0, sin(3.14159 * clamp(click * 2.2, 0.0, 1.0)) * (1.0 - click), pressing);
    let size = 19.0 * (1.0 - 0.14 * squeeze);
    let ring_r = 19.0 + 26.0 * click;
    let max_dist = (select(size, max(size, ring_r + 2.0), pressing) + 6.0) * scale;
    if abs(pos.x - center.x) > max_dist || abs(pos.y - center.y) > max_dist {
        return ui;
    }
    let q = (pos - center) / scale;
    let d = length(q) - size;
    var out = ui;
    if pressing {
        // A ring ripples out from the button and fades.
        let ring = abs(length(q) - ring_r) - 1.4 * (1.0 - click);
        out = over(out, vec3(0.75, 0.62, 1.0), (1.0 - smoothstep(-aa, aa, ring)) * (1.0 - click) * 0.8);
    }
    if d > aa { return out; }
    let mask = 1.0 - smoothstep(-aa, aa, d);
    let fill = mix(vec3(0.055, 0.035, 0.11), vec3(0.31, 0.14, 0.47), hot_c);
    out = over(out, mix(fill, vec3(0.50, 0.30, 0.85), squeeze), mask * 0.84);
    // Animated border: the titlebar's edge ramp wraps exactly once around the
    // circle (2400 samples per turn, so the loop has no seam) and turns
    // clockwise, one turn every 11 s (home_controls[4].z, 0..1).
    let turn = atan2(q.y, q.x) / 6.2832;
    let ring_color = ramp((turn + 1.0 - p.home_controls[4].z) * 2400.0, 0);
    let width = 1.6;
    let border = 1.0 - smoothstep(-aa, aa, abs(d + width) - width);
    // A soft inner glow of the same color so the gradient reads at a glance.
    let glow = exp(-max(-(d + 2.0 * width), 0.0) / 3.0) * step(d, 0.0) * 0.35;
    out = over(out, ring_color, glow * mask);
    out = over(out, ring_color, border);
    // The chevron kicks forward on click.
    let nudge = 4.0 * squeeze;
    let cq = q - vec2(dir * nudge, 0.0);
    let x = cq.x * dir;
    let l = max(abs(cq.y) - 7.0, abs(x - (3.0 - abs(cq.y) * 0.8)) - 1.2);
    return over(out, vec3(0.95, 0.94, 1.0), 1.0 - smoothstep(-aa, aa, l));
}

fn draw_home_carousel(pos: vec2<f32>) -> vec4<f32> {
    let s = p.home_view.z;
    if s <= 0.0 { return vec4(0.0); }
    let q = (pos - p.home_view.xy) / s;
    let aa = max(0.65, 0.75 / s);
    let total = p.home_state.x;
    if abs(q.x) > select(615.0, 860.0, total >= 5.0) || abs(q.y) >= 134.0 {
        return vec4(0.0);
    }
    var ui = vec4(0.0);
    if total == 0.0 {
        let d = rounded_distance(q + vec2(166.0, 28.0), vec2(332.0, 56.0), 13.0);
        let pill = over(ui, vec3(0.035, 0.032, 0.08), 0.86 * (1.0 - smoothstep(-aa, aa, d)));
        return home_label(pill, q, 10u, vec3(0.89, 0.91, 1.0), true) * p.home_view.w;
    }
    for (var slot = 0u; slot < 5u; slot++) {
        ui = home_card(ui, pos, slot, aa);
    }
    if total > 1.0 {
        ui = home_arrow(ui, pos, 0u, aa);
        ui = home_arrow(ui, pos, 1u, aa);
    }
    return ui * p.home_view.w;
}

fn draw_home_layers(pos: vec2<f32>, toolbar_pos: vec2<f32>) -> vec4<f32> {
    let ui = draw_home_carousel(pos);
    let scale = p.home_toolbar_view.z;
    if scale <= 0.0 { return ui; }
    let q = (toolbar_pos - p.home_toolbar_view.xy) / scale;
    let group = p.home_toolbar_rects[0];
    let search = p.home_toolbar_rects[5];
    let bleed = vec2(p.home_toolbar_view.w);
    if any(q < group.xy - bleed) || any(q > search.xy + search.zw + bleed) { return ui; }
    return home_toolbar(ui, q, max(0.65, 0.75 / scale));
}

// Section change (home_controls[2]): x = how far Home has left (0..1, may
// overshoot like the tab pill), y = direction (+1 leaves to the left),
// z = linear time, w = 1 while it runs.
fn draw_home(pos: vec2<f32>) -> vec4<f32> {
    // Section change: shift the lookups instead of branching, so the Home
    // shader is inlined only once (FXC compile time grows with each copy).
    let motion = p.home_controls[2];
    let running = motion.w > 0.5;
    // Same move as the clock and floating console (Profile::home_slide).
    let shift = select(vec2(0.0), vec2(-motion.y * p.screen.x * 0.55 * motion.x, 0.0), running);
    let fade = select(1.0, clamp(1.0 - motion.x, 0.0, 1.0), running);
    return draw_home_layers(pos - shift, pos - shift) * fade;
}

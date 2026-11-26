@group(0) @binding(5) var dock_atlas: texture_2d<f32>;

fn dock_sprite(s: DockSprite, point: vec2<f32>) -> vec4<f32> {
    let xy = point - s.origin;
    if any(xy < vec2(-0.5)) || any(xy >= s.rect.zw + 0.5) { return vec4(0.0); }
    let dims = vec2<f32>(textureDimensions(dock_atlas));
    return textureSampleLevel(dock_atlas, smooth_sampler, (s.rect.xy + xy) / dims, 0.0);
}

fn dock_over(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    return above + below * (1.0 - above.a);
}

fn dock_ramp(t: f32, row: f32) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(dock_atlas));
    let xy = DOCK_RAMPS.rect.xy + vec2(clamp(t, 0.0, 1.0) * 511.0, row) + 0.5;
    return textureSampleLevel(dock_atlas, smooth_sampler, xy / dims, 0.0).rgb;
}

fn dock_wing(xy: vec2<f32>, dpi: f32) -> vec4<f32> {
    let svg = (xy - vec2(784.296875, 134.0)) * (148.0 / 134.0);
    let right = svg.x > 547.5;
    let x = select(svg.x, 1095.0 - svg.x, right);
    if x > 40.0 { return vec4(0.0); }
    let y = 74.0 + clamp((x + 360.0) * (40.0 / 46.666667), -40.0, 0.0);
    let d = (svg.y - y) * (134.0 / 148.0);
    if d < -64.0 { return vec4(0.0); }
    var ui = over(vec4(0.0), CHROME_FILL, coverage(-d, dpi));
    if abs(d) < 14.0 {
        let phase = select(p.dock_phase.x, -p.dock_phase.x, right);
        let color = dock_ramp(fract((svg.x + phase) / 2400.0), select(0.0, 1.0, right));
        let diagonal = x < -360.0 && x > -406.666667;
        ui = hairline(ui, d * select(1.0, 0.7593, diagonal), color,
                      vec2(1.0, 2.1) * (134.0 / 148.0), 0.45, dpi);
    }
    return ui;
}

fn dock_edges(below: vec4<f32>, xy: vec2<f32>) -> vec4<f32> {
    let alpha = dock_sprite(DOCK_EDGES, xy);
    let x = xy.x - 24.0;
    let tray = x - 230.0;
    var ui = below;
    if alpha.r > 0.0 {
        ui = over(ui, dock_ramp(fract((x + p.dock_phase.y) / 2400.0), 2.0), alpha.r);
    }
    if alpha.g > 0.0 {
        ui = over(ui, dock_ramp(fract((x + p.dock_phase.z) / 2400.0), 3.0), alpha.g);
    }
    if alpha.b > 0.0 {
        ui = over(ui, dock_ramp(fract((tray + p.title_phase.y) / 2400.0), 4.0), alpha.b);
    }
    if alpha.a > 0.0 {
        ui = over(ui, dock_ramp(fract((tray + p.dock_phase.w) / 2400.0), 5.0), alpha.a);
    }
    return ui;
}

fn dock_icon(i: u32, xy: vec2<f32>) -> vec4<f32> {
    let box = DOCK_BUTTONS[i];
    let mid = box.xy + box.zw * 0.5;
    let scale = select(1.0, p.dock_scale[min(i / 4u, 1u)][i % 4u], i < 5u);
    let pt = mid + (xy - mid) / scale;
    let hot = p.dock_hot[i / 4u][i % 4u];
    let rest = dock_sprite(DOCK_ICONS[i], pt);
    if hot == 0.0 { return rest; }
    return mix(rest, dock_sprite(DOCK_HOTS[i], pt), hot);
}

fn segment_distance(pt: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    return length(pt - a - ab * clamp(dot(pt - a, ab) / dot(ab, ab), 0.0, 1.0));
}

fn dock_dash(s: vec4<f32>, offset: f32, period: f32, on: f32) -> f32 {
    let angle = atan2(s.g / s.a * 2.0 - 1.0, s.r / s.a * 2.0 - 1.0);
    let x = fract((angle * (1000.0 / 6.283185307) + offset) / period) * period;
    return smoothstep(-1.0, 1.0, min(x, on - x)) + smoothstep(period - 1.0, period + 1.0, x);
}

fn dock_play_hover(below: vec4<f32>, pt: vec2<f32>, local: vec2<f32>, fill: f32) -> vec4<f32> {
    let fx = p.dock_fx;
    let pulse = p.dock_pulse;
    var ui = below;
    if fx.w > 0.0 && fx.w < 1.0 && fill > 0.0 {
        let svg = local + vec2(547.5, 74.0);
        let x = svg.x + (svg.y - 18.0) * (50.0 / 112.0) - mix(-80.0, 520.0, fx.w);
        let light = 0.22 * coverage(abs(x - 320.0) - 25.0, p.dock_view.z)
            + 0.55 * coverage(abs(x - 360.0) - 6.0, p.dock_view.z);
        ui = over(ui, vec3(1.0), light * fill * pulse.w);
    }
    let center = vec2(1280.0, 201.0);
    let inset = mix(vec2(0.955, 0.82), vec2(0.968, 0.865), fx.z);
    let hud = dock_sprite(DOCK_PLAY_HUD, center + (pt - center) / inset);
    if hud.a > 0.0 {
        ui = over(ui, vec3(1.0), 0.72 * pulse.w * hud.a * dock_dash(hud, 160.0 * (1.0 - fx.y), 250.0, 170.0));
    }
    let edge = dock_sprite(DOCK_PLAY_TRACK, pt);
    if edge.a > 0.0 {
        let rim = dock_dash(edge, 0.0, 500.0, 500.0 * fx.x);
        let orbit = dock_dash(edge, (1.0 - pulse.x) * 1000.0, 500.0, 85.0);
        ui = over(ui, vec3(1.0), pulse.w * edge.a * max(0.79 * rim, orbit));
    }
    let wing = vec2(547.5 - abs(local.x), local.y + 74.0);
    let q = (wing - vec2(mix(384.0, 402.0, fx.z), 74.0)) / mix(0.84, 1.0, fx.z) + vec2(402.0, 74.0);
    if q.x > 384.0 && q.x < 426.0 && abs(local.y) < 16.0 {
        let tip = vec2(391.0, 74.0);
        let d = min(segment_distance(q, vec2(402.0, 62.0), tip), segment_distance(q, tip, vec2(402.0, 86.0)));
        let q2 = q - vec2(4.0 * pulse.y, 0.0);
        let tip2 = vec2(405.0, 74.0);
        let d2 = min(segment_distance(q2, vec2(413.0, 66.0), tip2), segment_distance(q2, tip2, vec2(413.0, 82.0)));
        ui = over(ui, vec3(1.0), pulse.w * max(coverage(d - 1.2, p.dock_view.z),
                  coverage(d2 - 0.9, p.dock_view.z) * mix(0.55, 1.0, pulse.y)));
    }
    return ui;
}

// WebView2 #jugarBorderShimmer: a 200 px white band (x 447.5..647.5 in the
// button's SVG space) with feathered stops, translated -360 -> 360 during the
// first 42% of every 3 s, then held off-screen. `x` is the SVG x coordinate.
fn play_shimmer_band(x: f32, t: f32) -> f32 {
    let shift = mix(-360.0, 360.0, min(t / 0.42, 1.0));
    let u = (x - shift - 447.5) / 200.0;
    if u <= 0.0 || u >= 1.0 { return 0.0; }
    let stops = array<vec2<f32>, 11>(
        vec2(0.0, 0.0), vec2(0.08, 0.04), vec2(0.18, 0.18), vec2(0.30, 0.45), vec2(0.42, 0.80),
        vec2(0.50, 1.0), vec2(0.58, 0.80), vec2(0.70, 0.45), vec2(0.82, 0.18), vec2(0.92, 0.04), vec2(1.0, 0.0));
    for (var i = 1u; i < 11u; i++) {
        if u <= stops[i].x {
            let a = stops[i - 1u];
            let b = stops[i];
            return mix(a.y, b.y, (u - a.x) / (b.x - a.x));
        }
    }
    return 0.0;
}

fn dock_play(below: vec4<f32>, xy: vec2<f32>) -> vec4<f32> {
    let center = vec2(1280.0, 201.0);
    let pt = center + (xy - center) / p.dock_scale[0].z;
    let hot = p.dock_hot[0].z;
    var fill = dock_sprite(DOCK_PLAY, pt);
    if hot > 0.0 { fill = mix(fill, dock_sprite(DOCK_PLAY_HOT, pt), hot); }
    var ui = dock_over(below, fill);
    let local = (pt - center) * (148.0 / 134.0);
    // SVG x of this pixel (the button is centered at 547.5 in its viewBox).
    let svg_x = 547.5 + local.x;
    // The shimmer keeps sweeping on hover too.
    let band = play_shimmer_band(svg_x, p.dock_play.z);
    let glint = band;
    let edge = dock_sprite(DOCK_PLAY_EDGE, pt).a;
    if edge > 0.0 {
        let t = 0.5 + dot(local, p.dock_play.xy) / 389.0;
        ui = over(ui, dock_ramp(t, 6.0), edge * (1.0 - hot));
        ui = over(ui, vec3(1.0), edge * hot * 0.45);
        // play-base-shimmer-border: the band as the stroke color, through
        // shimmerGlow (feGaussianBlur 1.2 merged under the source stroke).
        if glint > 0.001 {
            var blur = edge;
            for (var k = 0u; k < 6u; k++) {
                let a = f32(k) * 1.047198;
                blur += dock_sprite(DOCK_PLAY_EDGE, pt + vec2(cos(a), sin(a)) * 1.1).a;
            }
            blur /= 7.0;
            let stroke = 1.0 - (1.0 - blur) * (1.0 - edge);
            ui = over(ui, vec3(1.0), stroke * glint);
        }
        if p.dock_play.w > 0.0 {
            // Loading state: a fast cyan sweep travels around the same
            // outline used by the WebView2 Play button.
            let loading = dock_dash(dock_sprite(DOCK_PLAY_EDGE, pt), p.dock_play.w * 1000.0, 1000.0, 260.0);
            ui = over(ui, vec3(0.20, 0.86, 1.0), loading * 0.92);
        }
    }
    if p.dock_pulse.w > 0.0 { ui = dock_play_hover(ui, pt, local, dock_sprite(DOCK_PLAY_HOT, pt).a); }
    let state = p.play.impact.x;
    let state_alpha = p.play.layers[u32(state)].x;
    let sweep = p.play.beams.x;
    if state > 0.5 && sweep > 0.0 && sweep < 1.0 {
        let beam = coverage(abs(pt.x - center.x - mix(-80.0, 520.0, sweep)) - 20.0, p.dock_view.z);
        ui = over(ui, vec3(1.0), beam * fill.a * 0.35 * state_alpha);
    }
    // Kinetic text slots from the WebView2 button. Each state owns its
    // rasterized type and slides vertically through the same viewport.
    let idle = p.play.layers[0];
    let slots = array(DOCK_PLAY_TEXT_PREP, DOCK_PLAY_TEXT_RUN);
    if idle.x > 0.0 {
        let q = pt - vec2(0.0, idle.y);
        // WebView2 hover: letter-spacing 0.4 -> 2.2 px over 0.36 s. Both end
        // states are the reference's own crisp rasters (DOCK_PLAY_TEXT at
        // rest, DOCK_PLAY_TEXT_HOT spaced). In between, each letter of
        // "Jugar" glides from its rest place to its spaced place and its
        // pixels cross-fade between the two rasters; nothing is resampled
        // or scaled, so the type stays sharp.
        let lk = clamp(p.dock_label.x, 0.0, 1.0);
        // Letter cuts in each sprite, in canvas x (sprite origin + column).
        let rest_cut = array<f32, 4>(1267.0 + 14.0, 1267.0 + 28.0, 1267.0 + 42.5, 1267.0 + 56.0);
        let hot_cut = array<f32, 4>(1264.0 + 14.5, 1264.0 + 30.5, 1264.0 + 48.0, 1264.0 + 64.0);
        let rest_left = array<f32, 5>(1267.0, rest_cut[0], rest_cut[1], rest_cut[2], rest_cut[3]);
        let hot_left = array<f32, 5>(1264.0, hot_cut[0], hot_cut[1], hot_cut[2], hot_cut[3]);
        var letter = 0u;
        for (var g = 0u; g < 4u; g++) {
            if q.x >= mix(rest_cut[g], hot_cut[g], lk) { letter = g + 1u; }
        }
        let here = mix(rest_left[letter], hot_left[letter], lk);
        let from_rest = dock_sprite(DOCK_PLAY_TEXT, q + vec2(rest_left[letter] - here, 0.0));
        let from_hot = dock_sprite(DOCK_PLAY_TEXT_HOT, q + vec2(hot_left[letter] - here, 0.0));
        var label = mix(from_rest, from_hot, lk);
        ui = dock_over(ui, label * idle.x);
        let arrow_center = vec2(1245.5946, 201.0);
        let breath = hot * p.dock_pulse.z;
        let arrow = dock_sprite(DOCK_PLAY_ICON, arrow_center + (q - arrow_center) / (1.0 + breath * 0.10));
        ui = over(ui, vec3(1.0), idle.x * hot * mix(arrow.r * 0.25, arrow.g * 0.9, breath));
        ui = over(ui, vec3(1.0), arrow.a * idle.x);
        // play-shimmer-sweep-wrap: the same band, masked onto the arrow and
        // the "Jugar" label, with textShimmerBloom (blur 2.8 + 1.2): a glow
        // sampled around the glyphs, so the wave lights the type itself.
        if band > 0.001 {
            // feMerge(blur 2.8, blur 1.2, source): each blur is a small
            // Gaussian estimate of the glyph coverage around this pixel.
            let glyph = max(label.a, arrow.a);
            var wide = glyph;
            var near = glyph;
            for (var k = 0u; k < 8u; k++) {
                let a = f32(k) * 0.785398;
                let o = vec2(cos(a), sin(a));
                wide += max(dock_sprite(DOCK_PLAY_TEXT, q + o * 2.5).a, dock_sprite(DOCK_PLAY_ICON, q + o * 2.5).a);
                near += max(dock_sprite(DOCK_PLAY_TEXT, q + o * 1.1).a, dock_sprite(DOCK_PLAY_ICON, q + o * 1.1).a);
            }
            wide /= 9.0;
            near /= 9.0;
            let bloom = 1.0 - (1.0 - wide) * (1.0 - near) * (1.0 - glyph);
            ui = over(ui, vec3(1.0), bloom * band * idle.x);
        }
    }
    for (var i = 1u; i < 3u; i++) {
        let layer = p.play.layers[i];
        if layer.x > 0.0 {
            let q = pt - vec2(0.0, layer.y);
            ui = dock_over(ui, dock_sprite(slots[i - 1u], q) * layer.x);
        }
    }
    if state > 0.5 {
        let icon_center = center + vec2(-86.0, 0.0);
        let delta = pt - icon_center;
        let r = length(delta);
        if state < 1.5 {
            // Preparing: a fast reactor arc and the centered white diamond.
            let ring = coverage(abs(r - 9.5) - 1.2, p.dock_view.z);
            let angle = fract(atan2(delta.y, delta.x) / 6.28318 + p.play.telemetry.x);
            let arc = smoothstep(0.82, 0.98, angle) + smoothstep(0.02, 0.0, angle);
            ui = over(ui, vec3(1.0), ring * arc * state_alpha);
            let diamond = coverage(abs(delta.x) + abs(delta.y) - 4.2, p.dock_view.z);
            ui = over(ui, vec3(1.0), diamond * state_alpha);
        } else {
            // Running: the settled WebView2 beacon is a compact white reactor
            // core. Its sonar ring expands around the soft circular core.
            let ring = coverage(abs(r - 8.0 - p.play.telemetry.y * 10.0) - 0.9, p.dock_view.z);
            ui = over(ui, vec3(1.0), ring * (1.0 - p.play.telemetry.y) * state_alpha);
            let outer = coverage(abs(r - 9.6) - 1.1, p.dock_view.z);
            let core = smoothstep(10.5, 8.2, r) * (0.92 + 0.08 * p.play.telemetry.z);
            ui = over(ui, vec3(0.92, 0.94, 1.0), outer * state_alpha);
            ui = over(ui, vec3(1.0), core * state_alpha);
        }
        let outline = dock_sprite(DOCK_PLAY_OUTLINE, pt);
        if p.play.impact.z > 0.0 {
            let shock = outline.a * (1.0 - p.play.impact.z) * (1.0 - p.play.impact.z);
            ui = over(ui, vec3(1.0), shock * 0.9 * state_alpha);
        }
        let flash = p.play.impact.w;
        if flash > 0.0 { ui = over(ui, vec3(1.0), flash * outline.a * state_alpha); }
    }
    return ui;
}

@group(0) @binding(10) var dock_preview: texture_2d<f32>;
@group(0) @binding(11) var dock_previous: texture_2d<f32>;

fn draw_dock(pos: vec2<f32>) -> vec4<f32> {
    let xy = pos / p.dock_view.z - vec2(0.0, p.dock_view.y - 340.0);
    if xy.y < 89.0 { return vec4(0.0); }
    let center = xy - vec2((p.dock_view.x - 2560.0) * 0.5, 0.0);
    var ui = vec4(0.0);
    if xy.y < 260.0 {
        let half_x = clamp(1280.0 - abs(center.x - 1280.0), 301.0, 1279.0);
        let coord = DOCK_SHADOW.origin + (vec2(half_x, center.y) - DOCK_SHADOW.origin) * 0.5;
        ui = dock_sprite(DOCK_SHADOW, coord);
    }
    ui = dock_over(ui, dock_wing(center, p.dock_view.z));
    if center.x >= 769.0 && center.x < 1791.0 {
        ui = dock_over(ui, dock_sprite(DOCK_CHASSIS, center));
        if center.y >= 130.0 && center.y < 275.0 {
            if abs(center.x - 1280.0) < 225.0 { ui = dock_play(ui, center); }
            if center.y >= 155.0 && center.y < 247.0 {
                if center.x < 975.0 { ui = dock_over(ui, dock_icon(0u, center)); }
                else if center.x < 1100.0 { ui = dock_over(ui, dock_icon(1u, center)); }
                else if center.x >= 1587.0 { ui = dock_over(ui, dock_icon(4u, center)); }
                else if center.x >= 1485.0 { ui = dock_over(ui, dock_icon(3u, center)); }
            }
        }
    }
    let mirror = p.dock_view.x - xy.x;
    if xy.y >= 197.0 && xy.y < 321.0 {
        if xy.x < 577.0 {
            // The active card's cover and identity are drawn by the profile
            // layer. Only the adjacent tray is baked in this case.
            if p.dock_scale[1].w > 1.5 {
                ui = dock_over(ui, dock_sprite(DOCK_LEFT_ACTIVE, xy));
            } else {
                ui = dock_over(ui, dock_sprite(DOCK_LEFT, xy));
            }
            ui = dock_edges(ui, xy);
            if xy.x >= 308.0 && xy.x < 520.0 && xy.y >= 237.0 {
                let index = u32(clamp(round((xy.x - 349.0) / 64.0), 0.0, 2.0));
                ui = dock_over(ui, dock_icon(index + 6u, xy));
            }
        } else if mirror < 577.0 {
            let right = vec2(2560.0 - mirror, xy.y);
            ui = dock_over(ui, dock_sprite(DOCK_RIGHT, right));
            let mask = dock_sprite(DOCK_WALLPAPER_MASK, right).a;
            if mask > 0.0 {
                let uv = 0.5 + (right - vec2(2386.0, 260.0)) / vec2(300.0, 96.0);
                if p.dock_view.w >= 0.0 {
                    var picture = textureSampleLevel(dock_preview, smooth_sampler, uv, 0.0).rgb;
                    if p.dock_view.w > 0.0 {
                        let previous = textureSampleLevel(dock_previous, smooth_sampler, uv, 0.0).rgb;
                        picture = mix(picture, previous, p.dock_view.w);
                    }
                    ui = over(ui, picture, mask);
                }
                ui = dock_over(ui, dock_sprite(DOCK_WALLPAPER_LABEL, right));
            }
            ui = dock_edges(ui, vec2(mirror, xy.y));
            if right.x > 2020.0 && right.x < 2252.0 && xy.y >= 237.0 {
                let index = u32(clamp(round((right.x - 2083.0) / 64.0), 0.0, 2.0));
                ui = dock_over(ui, dock_icon(index + 9u, right));
            }
        }
    }
    if p.dock_tip.w > 0.0 {
        ui = dock_over(ui, dock_sprite(DOCK_TIPS[u32(p.dock_tip.z)], xy - p.dock_tip.xy) * p.dock_tip.w);
    }
    return ui;
}

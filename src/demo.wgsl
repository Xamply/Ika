struct TubeLayers {
    panel: vec4<f32>,
    flash: vec4<f32>,
    beam: vec4<f32>,
    dot: vec4<f32>,
};

struct PlayFrame {
    layers: array<vec4<f32>, 4>,
    impact: vec4<f32>,
    beams: vec4<f32>,
    telemetry: vec4<f32>,
}

struct Params {
    screen: vec4<f32>, // window size, source aspect, DPI scale
    frame: vec4<f32>,  // atlas rectangle in texels
    button: vec4<f32>, // client rectangle in pixels
    glass: vec4<f32>,  // backdrop panel in pixels
    state: vec4<f32>,  // blur, hover, pressed, point sampling
    glyphs: array<vec4<u32>, 8>,
    motion: array<vec4<f32>, 8>,
    next_button: vec4<f32>,
    next_state: vec4<f32>,
    next_glyphs: array<vec4<u32>, 8>,
    title_phase: vec4<f32>, // SVG offsets, window controls visible, notifications open
    title_nav: vec4<f32>, // glider offset, glider width, nav width, selected tab
    title_tabs: array<vec4<f32>, 5>,
    title_tints: array<vec4<f32>, 2>,
    title_glows: array<vec4<f32>, 2>,
    title_opacity: array<vec4<f32>, 2>,
    title_controls: vec4<f32>,
    monitor_rect: vec4<f32>, // left, top, width, height
    monitor_view: vec4<f32>, // scroll, first buffered row, backdrop fade, message
    monitor_crt: TubeLayers,
    monitor_ui: vec4<f32>,
    monitor_rows: array<vec4<u32>, 16>,
    dock_view: vec4<f32>,
    dock_phase: vec4<f32>,
    dock_play: vec4<f32>,
    dock_tip: vec4<f32>,
    dock_hot: array<vec4<f32>, 4>,
    dock_scale: array<vec4<f32>, 2>,
    dock_fx: vec4<f32>,
    dock_pulse: vec4<f32>,
    play: PlayFrame,
    wall_rect: vec4<f32>,
    wall_crt: TubeLayers,
    wall_fade: vec4<f32>,
    wall_state: vec4<f32>,
    wall_grid: vec4<f32>,
    wall_ui: vec4<u32>,
    wall_catalog: vec4<u32>,
    wall_details: vec4<u32>,
    wall_glows: vec4<u32>,
    wall_sources: vec4<u32>,
    wall_scrollbar: vec4<f32>,
    home_view: vec4<f32>,
    home_state: vec4<f32>,
    home_toolbar: vec4<f32>, // filter left and width, create icon motion and tint
    home_toolbar_view: vec4<f32>, // center, scale, bleed
    home_toolbar_rects: array<vec4<f32>, 6>,
    home_phase: vec4<f32>, // status clock, create border phase, active filter, reserved
    home_rects: array<vec4<f32>, 5>,
    home_images: array<vec4<u32>, 5>,
    home_controls: array<vec4<f32>, 3>,
    home_text: array<HomeText, 18>,
    wall_rows: array<vec4<u32>, 74>,
    wall_words: array<vec4<u32>, 148>,
    wall_advances: array<vec4<u32>, 148>,
    wall_sizes: array<vec4<u32>, 8>,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var smooth_sampler: sampler;
@group(0) @binding(3) var effect_masks: texture_2d_array<f32>;
@group(0) @binding(7) var outgoing_wallpaper: texture_2d<f32>;
@group(0) @binding(13) var glass_blurred: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn sample_frame(uv: vec2<f32>, sharp: bool, lod: f32) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(atlas));
    let xy = p.frame.xy + clamp(uv * p.frame.zw, vec2(0.5), p.frame.zw - 0.5);
    if sharp {
        let texel = textureLoad(atlas, vec2<i32>(floor(xy)), 0);
        return texel.rgb * texel.a;
    }
    let texel = textureSampleLevel(atlas, smooth_sampler, xy / dims, lod);
    return texel.rgb * texel.a;
}

fn effect_mask(uv: vec2<f32>, layer: i32) -> vec2<f32> {
    return textureSampleLevel(effect_masks, smooth_sampler, uv, layer, 0.0).rg;
}

fn shake(uv: vec2<f32>, layer: i32, amount: f32) -> vec2<f32> {
    return uv + (effect_mask(uv, layer) - 127.0 / 255.0) * (2.0 * amount);
}

fn shimmer(uv: vec2<f32>, mask_id: i32, controls: vec4<f32>, t: f32) -> f32 {
    let angle = 1.57079632679 - controls.x;
    let line = dot(uv, vec2(cos(angle), -sin(angle)));
    let phase = fract((line + t * controls.y) / controls.z) * controls.z;
    let gradient = smoothstep(0.08, 0.51, phase) * (1.0 - smoothstep(0.51, 0.92, phase));
    return gradient * gradient * effect_mask(uv, mask_id).x * controls.w;
}

fn hash(x: vec2<f32>) -> f32 {
    let s = fract(vec3(x.xyx) * 0.1031);
    let q = s + dot(s, s.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn mote(grid: vec2<f32>, t: f32, density: f32, radius: f32) -> f32 {
    let cell = floor(grid);
    let seed = hash(cell);
    if seed > density { return 0.0; }
    let phase = hash(cell + 31.4) * 6.2831853;
    let center = vec2(0.5) + sin(vec2(t * 0.76 + phase, t * 0.51 + phase * 1.71)) * 0.24;
    let distance = length(fract(grid) - center) / (radius * (0.55 + hash(cell + 6.2)));
    let opacity = 0.45 + sin(t * (2.4 + seed) + phase) * 0.39;
    return (1.0 - smoothstep(0.0, 1.0, distance)) * opacity;
}

fn chisa(uv: vec2<f32>, lod: f32) -> vec3<f32> {
    let t = p.motion[2].z;
    let shake_amount = p.motion[0];

    // Walk backwards through the displacement chain to sample the original
    // image once. Masks are the original TEX entries, sampled in linear space.
    let final_shake = shake(uv, 3, shake_amount.w);
    var coord = final_shake + p.motion[2].xy * effect_mask(final_shake, 5).x;
    let settings = p.motion[1];
    let phase = (2.7 + coord.x * 10.0 + coord.y * 5.0) * settings.w;
    let speed = settings.z * t;
    let ax = dot(sin(phase + speed * vec4(1.0, -0.161616, 0.008333, -0.000198)), vec4(1.0));
    let ay = dot(sin(phase + 0.4 + speed * vec4(-0.5, 0.041667, -0.001389, 0.0000248)), vec4(1.0));
    let aspect = p.screen.z * settings.y;
    let sway = settings.x * settings.x * 0.005 * effect_mask(coord, 4).x;
    coord += vec2(ax / aspect, ay * aspect) * sway;
    coord = shake(coord, 2, shake_amount.z);
    coord = shake(coord, 1, shake_amount.y);
    coord = shake(coord, 0, shake_amount.x);
    var rgb = sample_frame(coord, false, lod);

    // Single-pass approximations of the native gradients and soft rays.
    let gloss = shimmer(final_shake, 6, p.motion[3], t);
    let red_gloss = shimmer(final_shake, 7, p.motion[4], t);
    rgb += (1.0 - rgb) * (gloss * 0.43 + red_gloss * vec3(0.34, 0.018, 0.026));
    let ray = p.motion[5];
    let grain = 0.65 + 0.35 * sin(t * ray.x * 6.2831853 + uv.x * ray.w * 7.0 + uv.y * 14.0);
    let brightness = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    rgb += vec3(0.12, 0.10, 0.10) * effect_mask(final_shake, 8).x
        * smoothstep(ray.y * 0.45, ray.y, brightness) * mix(1.0, grain, ray.z);

    let emitters = p.motion[6];
    let dust = mote((uv - emitters.zw) * vec2(19.0, 13.0), t, 0.23, 0.073);
    let ember_grid = (uv - emitters.xy) * vec2(15.0, 14.0);
    let ash = mote(ember_grid + vec2(t * 0.19, t * 0.68), t, 0.39, 0.108);
    let flame_zone = (1.0 - smoothstep(0.0, 0.34, abs(uv.x - emitters.x + 0.09))) * smoothstep(0.40, 0.95, uv.y);
    return rgb + dust * vec3(1.0, 0.72, 0.24) * 0.48
        + ash * flame_zone * vec3(0.97, 0.022, 0.013) * 1.35;
}

fn srgb_to_linear(rgb: vec3<f32>) -> vec3<f32> {
    let c = clamp(rgb, vec3(0.0), vec3(1.0));
    return select(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), c > vec3(0.04045));
}

fn cover_scale(view_aspect: f32, wallpaper_aspect: f32) -> vec2<f32> {
    let ratio = view_aspect / wallpaper_aspect;
    return min(vec2(1.0), vec2(ratio, 1.0 / ratio));
}

fn shared_cover_rgb(uv: vec2<f32>, view_aspect: f32) -> vec3<f32> {
    var current = textureSampleLevel(atlas, smooth_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
    let old_weight = p.motion[7].x;
    if old_weight > 0.0 {
        let old_scale = cover_scale(view_aspect, p.motion[7].y);
        let current_scale = cover_scale(view_aspect, p.screen.z);
        let old_uv = 0.5 + (uv - 0.5) * (old_scale / current_scale);
        let old = textureSampleLevel(outgoing_wallpaper, smooth_sampler, old_uv, 0.0).rgb;
        current = mix(current, old, old_weight);
    }
    return current;
}

fn shared_rgb(uv: vec2<f32>) -> vec3<f32> {
    return shared_cover_rgb(uv, p.screen.x / p.screen.y);
}

fn wallpaper_srgb(uv: vec2<f32>) -> vec3<f32> {
    if p.motion[7].w > 0.5 {
        let rgb = shared_rgb(uv);
        if p.wall_state.w > 0.0 {
            return rgb * (1.0 - p.wall_state.w);
        }
        return rgb;
    } else if p.motion[2].w > 0.5 {
        let linear = chisa(clamp(uv, vec2(0.0), vec2(1.0)), 0.0);
        var dimmed = linear;
        if p.wall_state.w > 0.0 {
            dimmed *= srgb_to_linear(vec3(1.0 - p.wall_state.w));
        }
        return linear_to_srgb(dimmed);
    } else {
        let linear = sample_frame(uv, p.state.w > 0.5, 0.0);
        var dimmed = linear;
        if p.wall_state.w > 0.0 {
            dimmed *= srgb_to_linear(vec3(1.0 - p.wall_state.w));
        }
        return linear_to_srgb(dimmed);
    }
}

fn wallpaper(uv: vec2<f32>, sharp: bool, lod: f32) -> vec3<f32> {
    var linear: vec3<f32>;
    if p.motion[7].w > 0.5 {
        // The official SDR backbuffer is opaque with sRGB-encoded RGB values.
        linear = srgb_to_linear(shared_rgb(uv));
    } else if p.motion[2].w > 0.5 {
        linear = chisa(clamp(uv, vec2(0.0), vec2(1.0)), lod);
    } else {
        linear = sample_frame(uv, sharp, lod);
    }
    if p.wall_state.w > 0.0 {
        linear *= srgb_to_linear(vec3(1.0 - p.wall_state.w));
    }
    return linear;
}

fn glass_capture_rect() -> vec4<f32> {
    // Leave room beyond the panel for the Gaussian's 28 logical-pixel radius.
    let pad = 32.0 * p.next_state.z;
    return vec4(p.glass.xy - pad, p.glass.zw + 2.0 * pad);
}

@fragment
fn glass_capture(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let cover = cover_scale(p.screen.x / p.screen.y, p.screen.z);
    let screen_step = cover / p.screen.xy;
    let rect = glass_capture_rect();
    let step = rect.zw / vec2<f32>(textureDimensions(glass_blurred));
    let xy = rect.xy + pos.xy * step;
    let uv = (xy / p.screen.xy - 0.5) * cover + 0.5;
    // Prefilter each downsampled texel, then blur the actual composited wallpaper.
    let d = screen_step * step * 0.25;
    let mip = max(0.0, log2(max(d.x * p.frame.z, d.y * p.frame.w) * 2.0));
    let diagonal = vec2(d.x, -d.y);
    return vec4(0.25 * (wallpaper(uv - d, false, mip) + wallpaper(uv + d, false, mip) +
        wallpaper(uv - diagonal, false, mip) + wallpaper(uv + diagonal, false, mip)), 1.0);
}

fn rounded_distance(pt: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(pt - size / 2.0) - (size / 2.0 - radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn paint_button(pt: vec2<f32>, size: vec2<f32>, dpi: f32, distance: f32,
                hover: f32, down: f32, label_width: f32, label: array<vec4<u32>, 8>) -> vec4<f32> {
    var color = vec3(0.018, 0.024, 0.048);
    if hover > 0.5 { color = vec3(0.037, 0.057, 0.108); }
    if down > 0.5 { color = vec3(0.012, 0.016, 0.031); }
    if distance > -1.15 * dpi { color = vec3(0.32, 0.44, 0.65); }
    let glyph = floor((pt - (size - vec2(label_width, 16.0) * dpi) * 0.5) / (2.0 * dpi));
    if all(glyph >= vec2(0.0)) && glyph.x < label_width * 0.5 && glyph.y < 8.0 {
        let x = u32(glyph.x);
        let row = label[u32(glyph.y)][x / 32u];
        if ((row >> (x % 32u)) & 1u) != 0u { color = vec3(0.96, 0.97, 1.0); }
    }
    return vec4<f32>(color, 1.0);
}

fn draw_base(pos: vec4<f32>) -> vec4<f32> {
    let dpi = p.screen.w;
    let screen_aspect = p.screen.x / p.screen.y;
    let cover = min(vec2(1.0), vec2(screen_aspect / p.screen.z, p.screen.z / screen_aspect));
    let uv = (pos.xy / p.screen.xy - 0.5) * cover + 0.5;
    if pos.y < 116.0 * dpi || (p.title_phase.w > 0.0 && pos.y < 256.0 * dpi) {
        let xy = pos.xy / dpi;
        let width = p.screen.x / dpi;
        var ui = vec4(0.0);
        if xy.y < 116.0 { ui = draw_titlebar(xy, width, dpi); }
        if p.title_phase.w > 0.0 { ui = notification_panel(ui, xy, width, dpi); }
        if ui.a >= 1.0 {
            return vec4(srgb_to_linear(ui.rgb), 1.0);
        }
        if ui.a > 0.0 {
            let background = wallpaper_srgb(uv);
            return vec4(srgb_to_linear(ui.rgb + background * (1.0 - ui.a)), 1.0);
        }
    }
    var launcher_ui = vec4(0.0);
    if p.home_view.z > 0.0 { launcher_ui = draw_home(pos.xy); }
    let tool_scale = p.next_state.z;
    if p.button.z > 0.0 {
        let pt = pos.xy - p.button.xy;
        let distance = rounded_distance(pt, p.button.zw, 11.0 * tool_scale);
        if distance <= 0.0 {
            return paint_button(pt, p.button.zw, tool_scale, distance, p.state.y, p.state.z, 128.0, p.glyphs);
        }
    }
    if p.next_button.z > 0.0 {
        let pt = pos.xy - p.next_button.xy;
        let distance = rounded_distance(pt, p.next_button.zw, 11.0 * tool_scale);
        if distance <= 0.0 {
            return paint_button(pt, p.next_button.zw, tool_scale, distance,
                                p.next_state.x, p.next_state.y, 208.0, p.next_glyphs);
        }
    }
    if p.glass.z > 0.0 {
        let console = p.next_state.w > 0.5;
        let radius = select(16.0 * tool_scale, 20.0 * dpi, console);
        let glass_distance = rounded_distance(pos.xy - p.glass.xy, p.glass.zw, radius);
        if p.state.x > 0.5 && glass_distance < 0.5 {
            let rect = glass_capture_rect();
            let warped = (pos.xy - rect.xy) / rect.zw;
            var rgb = textureSampleLevel(glass_blurred, smooth_sampler, warped, 0.0).rgb;
            if !console {
                rgb = mix(rgb, vec3(0.025, 0.028, 0.040), 0.24);
                let rim = smoothstep(-1.05 * tool_scale - 0.5, -1.05 * tool_scale + 0.5, glass_distance);
                rgb = mix(rgb, vec3(0.78), 0.48 * rim);
            }
            let panel_alpha = coverage(glass_distance, 1.0);
            if panel_alpha < 1.0 {
                rgb = mix(wallpaper(uv, p.state.w > 0.5, 0.0), rgb, panel_alpha);
            }
            return vec4(rgb, 1.0);
        } else if glass_distance < 0.0 {
            var rgb = mix(wallpaper(uv, p.state.w > 0.5, 0.0), vec3(0.016, 0.020, 0.036), 0.32);
            if glass_distance > -1.05 * tool_scale { rgb = mix(rgb, vec3(0.78), 0.48); }
            return vec4<f32>(rgb, 1.0);
        }
    }
    // The dock stays above the carousel's shadow at its lowest position.
    if pos.y > p.screen.y - 251.0 * p.dock_view.z {
        launcher_ui = overlay_over(launcher_ui, draw_dock(pos.xy));
    }
    if launcher_ui.a >= 1.0 { return vec4(srgb_to_linear(launcher_ui.rgb), 1.0); }
    if launcher_ui.a > 0.0 {
        let bg = wallpaper_srgb(uv);
        return vec4(srgb_to_linear(launcher_ui.rgb + bg * (1.0 - launcher_ui.a)), 1.0);
    }
    if p.monitor_view.z > 0.0 && p.motion[7].w > 0.5 {
        let d = 4.0 * dpi * cover / p.screen.xy;
        let blurred = 0.25 * (shared_rgb(uv + d) + shared_rgb(uv - d) +
            shared_rgb(uv + vec2(d.x, -d.y)) + shared_rgb(uv + vec2(-d.x, d.y)));
        return vec4(srgb_to_linear(mix(shared_rgb(uv), blurred, p.monitor_view.z * 0.8)), 1.0);
    }
    return vec4(wallpaper(uv, p.state.w > 0.5, 0.0), 1.0);
}

@fragment
fn shared_wallpaper_fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let screen_aspect = p.screen.x / p.screen.y;
    let cover = min(vec2(1.0), vec2(screen_aspect / p.screen.z, p.screen.z / screen_aspect));
    let uv = (pos.xy / p.screen.xy - 0.5) * cover + 0.5;
    let source = shared_rgb(uv);
    var rgb = srgb_to_linear(source);
    if p.wall_state.w > 0.0 {
        rgb *= srgb_to_linear(vec3(1.0 - p.wall_state.w));
    }
    // Wallpapers is transparent around the window. Only the monitor picker has a veil.
    if p.monitor_ui.w > 0.0 {
        let shade = 0.85 * p.monitor_view.z;
        if p.monitor_view.z > 0.0 {
            let d = 4.0 * p.screen.w * cover / p.screen.xy;
            let blurred = 0.25 * (shared_rgb(uv + d) + shared_rgb(uv - d) +
                shared_rgb(uv + vec2(d.x, -d.y)) + shared_rgb(uv + vec2(-d.x, d.y)));
            rgb = srgb_to_linear(mix(source, blurred, p.monitor_view.z * 0.8));
        }
        rgb = srgb_to_linear(vec3(5.0, 3.0, 12.0) * (shade / 255.0) +
            linear_to_srgb(rgb) * (1.0 - shade));
    }
    return vec4(rgb, 1.0);
}

// Shared video already arrives as an opaque sRGB-encoded frame. The normal
// scene shader converts every pixel to linear space and back; that is wasted
// work for an MP4 with no scene effects. Targeting the encoded swapchain view
// keeps one texture sample per output pixel while the UI is overlaid later.
@fragment
fn shared_video_fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let screen_aspect = p.screen.x / p.screen.y;
    let cover = min(vec2(1.0), vec2(screen_aspect / p.screen.z, p.screen.z / screen_aspect));
    let uv = (pos.xy / p.screen.xy - 0.5) * cover + 0.5;
    var rgb = shared_rgb(uv);
    if p.wall_state.w > 0.0 {
        rgb *= (1.0 - p.wall_state.w);
    }
    return vec4(rgb, 1.0);
}

@fragment
fn fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    if p.monitor_ui.w == 0.0 && p.wall_rect.z == 0.0 { return draw_base(pos); }
    var overlay = vec4(0.0);
    if p.wall_rect.z > 0.0 {
        overlay = draw_wallpapers(pos.xy);
        if overlay.a >= 1.0 { return vec4(srgb_to_linear(overlay.rgb), 1.0); }
        let base = draw_base(pos);
        if overlay.a > 0.0 {
            return vec4(srgb_to_linear(overlay.rgb + linear_to_srgb(base.rgb) * (1.0 - overlay.a)), 1.0);
        }
        return base;
    }
    overlay = draw_picker(pos.xy / p.screen.w, p.screen.w);
    if overlay.a >= 1.0 { return vec4(srgb_to_linear(overlay.rgb), 1.0); }
    var base = draw_base(pos);
    if overlay.a > 0.0 {
        base = vec4(srgb_to_linear(overlay.rgb + linear_to_srgb(base.rgb) * (1.0 - overlay.a)), 1.0);
    }
    return base;
}

// UI-only pass used after a direct copy of a matching-size shared frame.
// It never samples the wallpaper, so fullscreen video does not consume 3D
// just to resample its own pixels.
fn overlay_over(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    return vec4(above.rgb + below.rgb * (1.0 - above.a),
                above.a + below.a * (1.0 - above.a));
}

@fragment
fn overlay(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    var dialog = vec4(0.0);
    if p.wall_rect.z > 0.0 {
        dialog = draw_wallpapers(pos.xy);
        if dialog.a >= 1.0 { return dialog; }
    }
    return overlay_over(draw_launcher_overlay(pos), dialog);
}

fn draw_launcher_overlay(pos: vec4<f32>) -> vec4<f32> {
    let dpi = p.screen.w;
    var ui = vec4(0.0);
    if pos.y < 116.0 * dpi {
        ui = overlay_over(ui, draw_titlebar(pos.xy / dpi, p.screen.x / dpi, dpi));
    }
    if p.title_phase.w > 0.0 && pos.y < 256.0 * dpi {
        ui = overlay_over(ui, notification_panel(vec4(0.0), pos.xy / dpi,
                                                  p.screen.x / dpi, dpi));
    }
    if p.home_view.z > 0.0 {
        ui = overlay_over(ui, draw_home(pos.xy));
    }
    let tool_scale = p.next_state.z;
    if p.button.z > 0.0 {
        let pt = pos.xy - p.button.xy;
        let distance = rounded_distance(pt, p.button.zw, 11.0 * tool_scale);
        if distance <= 0.0 {
            return paint_button(pt, p.button.zw, tool_scale, distance,
                                p.state.y, p.state.z, 128.0, p.glyphs);
        }
    }
    if p.next_button.z > 0.0 {
        let pt = pos.xy - p.next_button.xy;
        let distance = rounded_distance(pt, p.next_button.zw, 11.0 * tool_scale);
        if distance <= 0.0 {
            return paint_button(pt, p.next_button.zw, tool_scale, distance,
                                p.next_state.x, p.next_state.y, 208.0, p.next_glyphs);
        }
    }
    if p.glass.z > 0.0 && p.next_state.w > 0.5 {
        let d = rounded_distance(pos.xy - p.glass.xy, p.glass.zw, 20.0 * dpi);
        if d < 0.5 {
            let rect = glass_capture_rect();
            let rgb = linear_to_srgb(textureSampleLevel(glass_blurred, smooth_sampler, (pos.xy - rect.xy) / rect.zw, 0.0).rgb);
            let a = coverage(d, 1.0);
            ui = overlay_over(ui, vec4(rgb * a, a));
        }
    } else if p.glass.z > 0.0 && p.state.x <= 0.5 {
        let glass_distance = rounded_distance(pos.xy - p.glass.xy, p.glass.zw, 16.0 * tool_scale);
        if glass_distance < 0.0 {
            let screen_aspect = p.screen.x / p.screen.y;
            let cover = min(vec2(1.0), vec2(screen_aspect / p.screen.z, p.screen.z / screen_aspect));
            let uv = (pos.xy / p.screen.xy - 0.5) * cover + 0.5;
            var rgb = shared_rgb(uv);
            rgb = mix(rgb, vec3(0.016, 0.020, 0.036), 0.32);
            if glass_distance > -1.05 * tool_scale { rgb = mix(rgb, vec3(0.78), 0.48); }
            return vec4(rgb, 1.0);
        }
    }
    if pos.y > p.screen.y - 251.0 * p.dock_view.z {
        ui = overlay_over(ui, draw_dock(pos.xy));
    }
    return ui;
}

// Home already blends its artwork in display (sRGB) space. The shared
// wallpaper is in that same space. Target the UNORM swapchain view to avoid
// decoding and immediately re-encoding every pixel.
@fragment
fn shared_home(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    var ui = vec4(0.0);
    let dpi = p.screen.w;
    if pos.y < 116.0 * dpi {
        ui = draw_titlebar(pos.xy / dpi, p.screen.x / dpi, dpi);
    }
    if p.title_phase.w > 0.0 && pos.y < 256.0 * dpi {
        ui = notification_panel(ui, pos.xy / dpi, p.screen.x / dpi, dpi);
    }
    if ui.a == 0.0 { ui = draw_home(pos.xy); }
    if pos.y > p.screen.y - 251.0 * p.dock_view.z {
        ui = overlay_over(ui, draw_dock(pos.xy));
    }
    if ui.a >= 1.0 { return vec4(ui.rgb, 1.0); }
    let bg = home_background(pos.xy);
    return vec4(ui.rgb + bg * (1.0 - ui.a), 1.0);
}

fn home_background(xy: vec2<f32>) -> vec3<f32> {
    let crop = p.motion[6];
    return textureSampleLevel(atlas, smooth_sampler, xy * crop.xy + crop.zw, 0.0).rgb;
}

@fragment
fn wallpaper_only(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4(home_background(pos.xy), 1.0);
}

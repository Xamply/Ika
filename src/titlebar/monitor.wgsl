const MODAL_TEXT = vec3(251.0, 251.0, 254.0) / 255.0;
const MODAL_MUTED = vec3(184.0, 179.0, 214.0) / 255.0;
const MODAL_FOCUS = vec3(0.0, 245.0, 212.0) / 255.0;

fn picker_text(box: vec4<f32>, pt: vec2<f32>, x: f32, y: f32) -> f32 {
    return sprite(box, pt - vec2(x, y - box.w * 0.5)).r;
}

// x = mask at this pixel, y = next text origin. Only decimal digits vary by monitor;
// reuse the same atlas, including for the label and both status suffixes.
fn picker_number(pt: vec2<f32>, x: f32, y: f32, n: u32, bold: bool) -> vec2<f32> {
    var power = 1u;
    while power <= n / 10u { power *= 10u; }
    var left = x;
    var mask = 0.0;
    loop {
        let digit = MONITOR_DIGITS[(n / power) % 10u + select(0u, 10u, bold)];
        if pt.x >= left && pt.x < left + digit.z {
            mask = max(mask, picker_text(digit, pt, left, y));
        }
        left += select(7.01, 9.20, bold);
        if power == 1u { break; }
        power /= 10u;
    }
    return vec2(mask, left);
}

fn picker_rows(below: vec4<f32>, point: vec2<f32>, size: vec2<f32>, dpi: f32) -> vec4<f32> {
    if any(point < vec2(27.0)) || any(point >= size - 27.0) { return below; }
    let pt = point + vec2(0.0, p.monitor_view.x);
    var color = below;
    color = over(color, MODAL_TEXT, picker_text(MONITOR_SPRITES[0], pt, 27.0, 42.0));
    color = over(color, MODAL_MUTED, picker_text(MONITOR_SPRITES[1], pt, 27.0, 81.5));
    if p.monitor_view.w == 1.0 {
        return over(color, MODAL_MUTED, picker_text(MONITOR_SPRITES[8], pt, 27.0, 112.0));
    }
    if p.monitor_view.w == 2.0 {
        color = over(color, MODAL_MUTED,
            picker_text(MONITOR_SPRITES[9], pt, 27.0, p.monitor_ui.z - 37.0));
    }
    if pt.y < 109.0 { return color; }
    // The small halo outside a focused button also needs its row selected.
    let i = floor((pt.y - 109.0) / 96.0);
    let local = i - p.monitor_view.y;
    if local < 0.0 || local >= 16.0 { return color; }
    let row = p.monitor_rows[u32(local)];
    if row.x == 0u { return color; }
    let row_pt = pt - vec2(27.0, 114.0 + i * 96.0);
    let row_size = vec2(size.x - 54.0, 84.0);
    let d = rounded_distance(row_pt, row_size, 12.0);
    let inside = coverage(d, dpi);
    if (row.w & (1u << 19u)) != 0u {
        color = over(color, MODAL_FOCUS, coverage(abs(d - 4.0) - 1.0, dpi));
    }
    if inside <= 0.0 { return color; }
    let fill = select(vec3(8.0, 6.0, 17.0), vec3(17.0, 13.0, 34.0), (row.w & (1u << 18u)) != 0u) / 255.0;
    color = over(color, fill, inside);
    let border = select(vec3(41.0, 31.0, 79.0) / 255.0,
        mix(fill, vec3(168.0, 85.0, 247.0) / 255.0, 0.7), (row.w & (1u << 17u)) != 0u);
    color = over(color, border, inside - coverage(d + 1.0, dpi));
    if row_pt.x < 19.0 || row_pt.x > row_size.x - 19.0 { return color; }
    let label = MONITOR_SPRITES[2];
    let title = picker_number(row_pt, 19.0 + label.z + 2.0, 29.5, row.x, true);
    color = over(color, MODAL_TEXT, max(title.x, picker_text(label, row_pt, 19.0, 29.5)));

    let line_y = 56.5;
    if abs(row_pt.y - line_y) > 10.0 { return color; }
    let w = picker_number(row_pt, 19.0, line_y, row.y, false);
    let times = MONITOR_SPRITES[3];
    let h = picker_number(row_pt, w.y + times.z + 5.0, line_y, row.z, false);
    let scale = MONITOR_SPRITES[4];
    let pct = picker_number(row_pt, h.y + scale.z + 5.0, line_y, row.w & 0xffffu, false);
    let percent = MONITOR_SPRITES[5];
    let primary = MONITOR_SPRITES[6];
    var alpha = max(max(w.x, h.x), pct.x);
    alpha = max(alpha, picker_text(times, row_pt, w.y + 3.5, line_y));
    alpha = max(alpha, picker_text(scale, row_pt, h.y + 3.5, line_y));
    alpha = max(alpha, picker_text(percent, row_pt, pct.y, line_y));
    var x = pct.y + percent.z + 1.5;
    if (row.w & (1u << 16u)) != 0u {
        alpha = max(alpha, picker_text(primary, row_pt, x, line_y));
        x += primary.z + 1.5;
    }
    if (row.w & (1u << 17u)) != 0u {
        alpha = max(alpha, picker_text(MONITOR_SPRITES[7], row_pt, x, line_y));
    }
    return over(color, MODAL_MUTED, alpha);
}

fn picker_box(below: vec4<f32>, center: vec2<f32>, size: vec2<f32>, dpi: f32) -> vec4<f32> {
    let screen = p.monitor_crt.panel;
    if screen.z <= 0.0 { return below; }
    let pt = center / screen.xy + size * 0.5;
    let d = rounded_distance(pt, size, 18.0);
    let a = tube_edge(pt, size, 18.0, screen.xy, dpi);
    var panel = over(vec4(0.0), vec3(0.0), 0.6 *
        gaussian_cdf(-rounded_distance(pt - vec2(0.0, 24.0), size, 18.0) / 40.0));
    if a <= 0.0 { return tube_over(below, panel * screen.z); }
    // Flatten content into its opaque box before applying the CRT layer opacity.
    var box = vec4(CHROME_FILL, 1.0);
    box = over(box, vec3(157.0, 78.0, 221.0) / 255.0, 1.0 - coverage(d + 1.0, dpi));
    if screen.w > 0.0 {
        let content_pt = pt + vec2(0.0, 6.0 * (1.0 - screen.w));
        var ui = picker_rows(box, content_pt, size, dpi);
        let close_pt = content_pt - vec2(size.x - 47.0, 11.0);
        let close_d = rounded_distance(close_pt, vec2(36.0, 32.0), 8.0);
        ui = over(ui, vec3(232.0, 17.0, 35.0) / 255.0, p.monitor_ui.x * coverage(close_d, dpi));
        ui = over(ui, MODAL_FOCUS, p.monitor_ui.y * coverage(abs(close_d - 4.0) - 1.0, dpi));
        ui = over(ui, mix(CONTROL_COLOR, vec3(1.0), p.monitor_ui.x),
            centered_mask(CONTROL_SPRITES[2], close_pt - vec2(18.0, 16.0)));
        let stroke = rounded_distance(content_pt - 1.0, size - 2.0, 14.0);
        ui = over(ui, ramp(pt.x - p.title_phase.x, 0), coverage(abs(stroke) - 0.8, dpi));
        box = mix(box, ui, screen.w);
    }
    panel = over(panel, box.rgb, a);
    return tube_over(below, panel * screen.z);
}

fn draw_picker(xy: vec2<f32>, dpi: f32) -> vec4<f32> {
    let size = p.monitor_rect.zw;
    let center = xy - (p.monitor_rect.xy + size * 0.5);
    let shade = vec4(vec3(5.0, 3.0, 12.0) * (0.85 / 255.0), 0.85);
    let parent = p.monitor_view.z;
    if any(abs(center) > size * 0.52 + vec2(125.0, 150.0)) { return shade * parent; }
    let body = picker_box(shade, center, size, dpi) * parent;
    // The picker already applies the CRT panel transform and opacity in
    // `picker_box`.  Applying `tube_light` here adds the animated phosphor
    // halo outside the window; that halo is what leaves the distracting
    // yellow monitor frames behind after the interaction ends.
    return body;
}

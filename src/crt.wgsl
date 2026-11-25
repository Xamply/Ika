@group(0) @binding(8) var crt_atlas: texture_2d<f32>;

fn tube_over(base: vec4<f32>, layer: vec4<f32>) -> vec4<f32> {
    return layer + base * (1.0 - layer.a);
}

// The element is rendered untransformed and then scaled. Use its edge normal
// to keep antialiasing one physical pixel wide even in the thin line phase.
fn tube_edge(pt: vec2<f32>, size: vec2<f32>, radius: f32, scale: vec2<f32>, dpi: f32) -> f32 {
    let q = abs(pt - size * 0.5) - (size * 0.5 - radius);
    let corner = max(q, vec2(0.0));
    let d = length(corner) + min(max(q.x, q.y), 0.0) - radius;
    if all(scale == vec2(1.0)) { return coverage(d, dpi); }
    var normal = select(vec2(0.0, 1.0), vec2(1.0, 0.0), q.x > q.y);
    if all(q > vec2(0.0)) { normal = normalize(corner); }
    return coverage(d, dpi / length(normal / scale));
}

fn tube_sprite(uv: vec2<f32>, origin: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    var color = vec4(0.0);
    if all(uv >= vec2(0.0)) && all(uv < vec2(1.0)) {
        let texel = origin + clamp(uv * size, vec2(0.5), size - 0.5);
        color = textureSampleLevel(crt_atlas, smooth_sampler, texel / vec2(256.0, 384.0), 0.0);
    }
    return color;
}

fn tube_beam(x: f32, y: f32, width: f32, off: f32, dpi: f32) -> vec4<f32> {
    let uv = vec2(x / width + 0.5, y / 64.0 + 0.5);
    // Keep the glow inside the transformed screen, including the split phase.
    let tip = coverage(abs(x) - width * 0.5 + 0.5 / dpi, dpi);
    return tube_sprite(uv, vec2(0.0, off * 128.0), vec2(256.0, 128.0)) * tip;
}

fn tube_phosphor(r: f32) -> vec4<f32> {
    let center = vec4(1.0);
    let inner = vec4(vec3(244.0, 240.0, 255.0) * (0.97 / 255.0), 0.97);
    let middle = vec4(vec3(205.0, 185.0, 255.0) * (0.90 / 255.0), 0.90);
    let edge = vec4(vec3(160.0, 130.0, 255.0) * (0.78 / 255.0), 0.78);
    var color = mix(middle, edge, clamp((r - 0.72) / 0.28, 0.0, 1.0));
    if r < 0.38 { color = mix(center, inner, r / 0.38); }
    else if r < 0.72 { color = mix(inner, middle, (r - 0.38) / 0.34); }
    return color;
}

fn tube_light(base: vec4<f32>, center: vec2<f32>, size: vec2<f32>, dpi: f32, radius: f32,
              layers: TubeLayers) -> vec4<f32> {
    var color = base;
    let flash = layers.flash;
    let beam = layers.beam;
    let dot = layers.dot;
    if flash.z > 0.0 || flash.w > 0.0 {
        let pt = center / flash.xy + size * 0.5;
        let mask = tube_edge(pt, size, radius, flash.xy, dpi);
        if flash.z > 0.0 {
            var light = vec4(0.0);
            if beam.w == 0.0 {
                let d = rounded_distance(pt, size, radius);
                let outer = 0.25 * gaussian_cdf((8.0 - d) / 16.0) * (1.0 - mask);
                let inner = 0.45 * gaussian_cdf((3.0 - d) / 7.0) * (1.0 - mask);
                light = over(light, vec3(139.0, 92.0, 246.0) / 255.0, outer);
                light = over(light, vec3(214.0, 196.0, 255.0) / 255.0, inner);
            }
            let r = length(center / (flash.xy * size)) * 1.41421356;
            light = tube_over(light, tube_phosphor(r) * mask);
            color = tube_over(color, light * flash.z);
        }
        if flash.w > 0.0 {
            // The CSS scan element groups the edge tints and the 4px black pattern.
            let u = pt.x / size.x;
            let red = 0.05 * clamp(1.0 - u / 0.30, 0.0, 1.0);
            let blue = 0.05 * clamp((u - 0.70) / 0.30, 0.0, 1.0);
            let tint = vec4(red, blue * (200.0 / 255.0), red * (90.0 / 255.0) + blue, red + blue);
            let row = fract(pt.y * 0.25) * 4.0;
            let scan = 0.2 * min(min(row, 4.0 - row) / 1.5, 1.0);
            color = tube_over(color, over(tint, vec3(0.0), scan) * mask * flash.w);
        }
    }
    if beam.z > 0.0 {
        let slide = beam.y * size.y;
        let width = size.x * beam.x;
        color = tube_over(color, tube_beam(center.x, center.y + slide, width, beam.w, dpi) * beam.z);
        color = tube_over(color, tube_beam(center.x, center.y - slide, width, beam.w, dpi) * beam.z);
    }
    if dot.z > 0.0 {
        let uv = center / (128.0 * dot.xy) + 0.5;
        color = tube_over(color, tube_sprite(uv, vec2(beam.w * 128.0, 256.0), vec2(128.0)) * dot.z);
    }
    return color;
}

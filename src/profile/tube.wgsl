struct TubeLayers {
    panel: vec4<f32>,
    flash: vec4<f32>,
    beam: vec4<f32>,
    dot: vec4<f32>,
}

struct Paint {
    area: vec4<f32>,
    screen: vec4<f32>,
    layers: TubeLayers,
}

@group(0) @binding(0) var<uniform> p: Paint;
@group(0) @binding(1) var smooth_sampler: sampler;

fn over(below: vec4<f32>, color: vec3<f32>, opacity: f32) -> vec4<f32> {
    return vec4(mix(below.rgb, color, opacity), below.a + (1.0 - below.a) * opacity);
}

fn coverage(distance: f32, dpi: f32) -> f32 {
    return clamp(0.5 - distance * dpi, 0.0, 1.0);
}

fn rounded_distance(pt: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(pt - size * 0.5) - (size * 0.5 - radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn gaussian_cdf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.2316419 * abs(x));
    let tail = 0.39894228 * exp(-0.5 * x * x) *
        t * (0.31938153 + t * (-0.35656378 + t * (1.78147794 + t * (-1.82125598 + t * 1.33027443))));
    return select(1.0 - tail, tail, x < 0.0);
}

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let points = array(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(points[i], 0., 1.);
}

@fragment
fn fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dpi = p.screen.x;
    let center = pos.xy / dpi - p.area.xy - p.area.zw * 0.5;
    return tube_light(vec4(0.0), center, p.area.zw, dpi, 21.0, p.layers);
}

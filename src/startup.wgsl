struct Paint {
    screen: vec4<f32>,
    stage: vec4<f32>,
    progress: vec4<f32>,
    leaves: array<vec4<f32>, 8>,
    faces: array<vec4<f32>, 24>,
};
@group(0) @binding(0) var<uniform> p: Paint;

const BASE = vec3(4.0, 4.0, 22.0) / 255.0;
const ACCENT = vec3(157.0, 78.0, 221.0) / 255.0;

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn over(below: vec4<f32>, color: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4(mix(below.rgb, color, alpha), below.a + (1.0 - below.a) * alpha);
}

// Normal CDF for the blur along a CSS clip edge. Physical pixel coverage
// handles zero blur and the subpixel edge of each projected cube face.
fn edge(d: f32, blur: f32) -> f32 {
    if blur < 0.1 { return clamp(d * p.screen.z + 0.5, 0.0, 1.0); }
    let x = d / blur;
    let a = abs(x) * 0.70710678;
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let erf = 1.0 - (((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t
                      - 0.284496736) * t + 0.254829592) * t) * exp(-a * a);
    return 0.5 + 0.5 * sign(x) * erf;
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x * b.y - a.y * b.x; }

fn quad(pt: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, d: vec2<f32>) -> f32 {
    let orientation = sign(cross2(b - a, c - a));
    if orientation == 0.0 { return -1000.0; }
    let corners = array(a, b, c, d);
    var distance = 1000.0;
    for (var i = 0u; i < 4u; i++) {
        let start = corners[i];
        let side = corners[(i + 1u) % 4u] - start;
        distance = min(distance, orientation * cross2(side, pt - start) / max(length(side), 0.001));
    }
    return distance;
}

@fragment
fn fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let size = p.screen.xy;
    let xy = pos.xy / p.screen.z;
    var alpha = 1.0;
    // The leaves tile the entire viewport until the first one moves.
    if p.leaves[0].y < 0.0 {
        let slant = 0.18 * size.x / size.y;
        let normal = inverseSqrt(1.0 + slant * slant);
        let x = xy.x + slant * xy.y;
        var clear = 1.0;
        for (var i = 0u; i < 8u; i++) {
            let leaf = p.leaves[i];
            let left = (f32(i) * 0.17 - 0.09) * size.x - 16.0 + leaf.x + slant * leaf.y;
            let right = left + 0.17 * size.x + 32.0;
            let side = min(x - left, right - x) * normal;
            let vertical = min(xy.y - leaf.y, leaf.y + size.y - xy.y);
            if min(side, vertical) >= -3.5 * leaf.z - 1.0 {
                clear *= 1.0 - edge(side, leaf.z) * edge(vertical, leaf.z);
            }
        }
        alpha = 1.0 - clear;
    }
    var color = vec4(BASE * alpha, alpha);
    if p.stage.x > 0.0 {
        let pt = (xy - size * 0.5) / p.stage.y - vec2(0.0, p.stage.z);
        let gradient = 0.07 * clamp(1.0 - length(pt / size) * (sqrt(2.0) / 0.45), 0.0, 1.0);
        let shadow_scale = 1.0 - 0.2 * p.stage.w;
        let shadow_pt = pt - vec2(67.0 * (1.0 - shadow_scale), 79.5);
        let shadow = 0.24 * (0.8 - 0.4 * p.stage.w)
            * clamp(1.0 - length(shadow_pt / vec2(67.0, 13.5)) / (0.7 * shadow_scale), 0.0, 1.0);
        var stage = over(vec4(0.0), ACCENT, gradient);
        stage = over(stage, ACCENT, shadow);
        if all(abs(pt) < vec2(100.0)) {
            for (var i = 0u; i < 24u; i += 4u) {
                let a = p.faces[i]; let b = p.faces[i + 1u];
                let c = p.faces[i + 2u]; let d = p.faces[i + 3u];
                let outer = quad(pt, a.xy, b.xy, c.xy, d.xy);
                let inner = quad(pt, a.zw, b.zw, c.zw, d.zw);
                let halo = max(0.0, edge(outer, 3.5) - edge(inner, 3.5));
                stage = over(stage, vec3(1.0), 0.10 * halo);
                let opacity = 0.85 * clamp(edge(outer, 0.0) - edge(inner, 0.0), 0.0, 1.0);
                stage = over(stage, vec3(1.0), opacity);
            }
        }
        if p.progress.x >= 0.0 {
            let width = min(160.0, size.x * 0.4);
            let q = xy - vec2((size.x - width) * 0.5, size.y * 0.5 + 111.0);
            let track = edge(min(min(q.x, width - q.x), 1.5 - abs(q.y)), 0.0);
            let fill = edge(min(min(q.x, width * p.progress.x - q.x), 1.5 - abs(q.y)), 0.0);
            stage = over(stage, ACCENT, 0.28 * track);
            stage = over(stage, ACCENT, fill);
        }
        stage *= p.stage.x;
        color = stage + color * (1.0 - stage.a);
    }
    return color * p.screen.w;
}

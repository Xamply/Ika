struct Crop {
    uv: vec4<f32>,
    linear: vec4<f32>,
};
@group(0) @binding(0) var<uniform> crop: Crop;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var filtering: sampler;

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn snapshot(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let center = crop.uv.xy + pos.xy * crop.uv.zw;
    // Each bilinear tap covers roughly two source texels per axis. Include
    // more of the footprint when shrinking a large source to avoid moire.
    let footprint = crop.uv.zw * vec2<f32>(textureDimensions(source));
    let taps = vec2<u32>(clamp(ceil(footprint * 0.5), vec2(1.0), vec2(4.0)));
    let linear = crop.linear.x > 0.5;
    var sum = vec3(0.0);
    for (var y = 0u; y < taps.y; y += 1u) {
        for (var x = 0u; x < taps.x; x += 1u) {
            let offset = (vec2<f32>(vec2(x, y)) + 0.5) / vec2<f32>(taps) - 0.5;
            let pixel = textureSampleLevel(source, filtering, center + offset * crop.uv.zw, 0.0);
            sum += select(pixel.rgb, pixel.rgb * pixel.a, linear);
        }
    }
    var rgb = sum / f32(taps.x * taps.y);
    if linear {
        rgb = select(1.055 * pow(rgb, vec3(1.0 / 2.4)) - 0.055, rgb * 12.92, rgb <= vec3(0.0031308));
    }
    return vec4<f32>(rgb, 1.0);
}

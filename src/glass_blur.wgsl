@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}

// Gaussian with sigma 4.5 texels. Bilinear taps combine adjacent weights
// without leaving gaps between them, even on a single-mip wallpaper.
fn gaussian(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    let pairs = array(
        vec2(1.48149, 0.16701565), vec2(3.45690, 0.13087174), vec2(5.43251, 0.08437210),
        vec2(7.40845, 0.04475119), vec2(9.38482, 0.01952752), vec2(11.36172, 0.00700980),
        vec2(13.33924, 0.00206993),
    );
    var rgb = textureSampleLevel(source, linear_sampler, uv, 0.0).rgb * 0.08876416;
    for (var i = 0u; i < 7u; i++) {
        let d = pairs[i].x * axis;
        rgb += (textureSampleLevel(source, linear_sampler, uv - d, 0.0).rgb +
                textureSampleLevel(source, linear_sampler, uv + d, 0.0).rgb) * pairs[i].y;
    }
    return vec4(rgb, 1.0);
}

// Both passes sample from texel centers; the final panel only upscales the
// filtered result. Running the paired taps at fractional texels would band.
@fragment
fn horizontal(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    return gaussian(pos.xy * texel, vec2(texel.x, 0.0));
}

@fragment
fn vertical(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    return gaussian(pos.xy * texel, vec2(0.0, texel.y));
}

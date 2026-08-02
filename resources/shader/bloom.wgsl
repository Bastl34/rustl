// bloom mip chain (CoD: Advanced Warfare style, thresholdless)
//
// downsample: hdr scene color -> mip 0 -> mip 1 -> ... (13 tap filter, the first
// pass uses a karis average to suppress fireflies/flicker from single hot pixels)
// upsample: mip N additively blended up the chain with a 3x3 tent filter
// (the additive blending is configured on the pipeline: src = ONE, dst = ONE)
//
// the result in mip 0 is mixed into the final image by the composite pass

// set at pipeline creation (unused here - kept for the shared fullscreen pipeline setup)
override REVERSE_Z: bool = false;

struct VertexOutput
{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// fullscreen triangle (no vertex buffer)
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput
{
    var out: VertexOutput;

    let pos = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.position = vec4<f32>(pos * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x, 1.0 - pos.y);

    return out;
}

// source: the hdr scene color (first pass) or the previous (bigger) bloom mip
@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;

fn luma(color: vec3<f32>) -> f32
{
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// 13 tap sample positions (in source texels around the target pixel center):
//
//   a . b . c
//   . j . k .
//   d . e . f
//   . l . m .
//   g . h . i

struct DownsampleTaps
{
    a: vec3<f32>, b: vec3<f32>, c: vec3<f32>,
    d: vec3<f32>, e: vec3<f32>, f: vec3<f32>,
    g: vec3<f32>, h: vec3<f32>, i: vec3<f32>,
    j: vec3<f32>, k: vec3<f32>, l: vec3<f32>, m: vec3<f32>,
};

fn downsample_taps(uv: vec2<f32>) -> DownsampleTaps
{
    let texel = 1.0 / vec2<f32>(textureDimensions(t_source));

    var taps: DownsampleTaps;

    taps.a = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>(-2.0, -2.0), 0.0).rgb;
    taps.b = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 0.0, -2.0), 0.0).rgb;
    taps.c = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 2.0, -2.0), 0.0).rgb;

    taps.d = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>(-2.0,  0.0), 0.0).rgb;
    taps.e = textureSampleLevel(t_source, s_source, uv,                                 0.0).rgb;
    taps.f = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 2.0,  0.0), 0.0).rgb;

    taps.g = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>(-2.0,  2.0), 0.0).rgb;
    taps.h = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 0.0,  2.0), 0.0).rgb;
    taps.i = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 2.0,  2.0), 0.0).rgb;

    taps.j = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    taps.k = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 1.0, -1.0), 0.0).rgb;
    taps.l = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>(-1.0,  1.0), 0.0).rgb;
    taps.m = textureSampleLevel(t_source, s_source, uv + texel * vec2<f32>( 1.0,  1.0), 0.0).rgb;

    return taps;
}

// first downsample (hdr scene color -> mip 0): the 13 taps are grouped into five 2x2 blocks
// and each block is weighted with 1 / (1 + luma) (karis average) - a single very bright
// pixel would otherwise flicker through the whole mip chain
@fragment
fn fs_downsample_first(in: VertexOutput) -> @location(0) vec4<f32>
{
    let taps = downsample_taps(in.uv);

    let group0 = (taps.a + taps.b + taps.d + taps.e) * 0.25; // top left block
    let group1 = (taps.b + taps.c + taps.e + taps.f) * 0.25; // top right block
    let group2 = (taps.d + taps.e + taps.g + taps.h) * 0.25; // bottom left block
    let group3 = (taps.e + taps.f + taps.h + taps.i) * 0.25; // bottom right block
    let group4 = (taps.j + taps.k + taps.l + taps.m) * 0.25; // center block

    let weight0 = 0.125 / (1.0 + luma(group0));
    let weight1 = 0.125 / (1.0 + luma(group1));
    let weight2 = 0.125 / (1.0 + luma(group2));
    let weight3 = 0.125 / (1.0 + luma(group3));
    let weight4 = 0.5   / (1.0 + luma(group4));

    // normalized karis average - keeps the overall energy while damping single hot pixels
    let color = (group0 * weight0 + group1 * weight1 + group2 * weight2 + group3 * weight3 + group4 * weight4)
              / (weight0 + weight1 + weight2 + weight3 + weight4);

    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}

// downsample within the mip chain (mip n-1 -> mip n)
@fragment
fn fs_downsample(in: VertexOutput) -> @location(0) vec4<f32>
{
    let taps = downsample_taps(in.uv);

    // center 2x2 block: 0.5, the four overlapping corner blocks: 0.125 each
    var color = taps.e * 0.125;
    color += (taps.a + taps.c + taps.g + taps.i) * 0.03125;
    color += (taps.b + taps.d + taps.f + taps.h) * 0.0625;
    color += (taps.j + taps.k + taps.l + taps.m) * 0.125;

    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}

// upsample (mip n -> mip n-1, additive): 3x3 tent filter
@fragment
fn fs_upsample(in: VertexOutput) -> @location(0) vec4<f32>
{
    let texel = 1.0 / vec2<f32>(textureDimensions(t_source));

    var color = textureSampleLevel(t_source, s_source, in.uv, 0.0).rgb * 4.0;

    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>(-1.0,  0.0), 0.0).rgb * 2.0;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>( 1.0,  0.0), 0.0).rgb * 2.0;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>( 0.0, -1.0), 0.0).rgb * 2.0;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>( 0.0,  1.0), 0.0).rgb * 2.0;

    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>( 1.0, -1.0), 0.0).rgb;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>(-1.0,  1.0), 0.0).rgb;
    color += textureSampleLevel(t_source, s_source, in.uv + texel * vec2<f32>( 1.0,  1.0), 0.0).rgb;

    return vec4<f32>(color / 16.0, 1.0);
}

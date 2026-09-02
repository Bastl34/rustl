@group(0) @binding(0)
var depth_tex : texture_depth_2d;

@group(0) @binding(1)
var sampler_nearest: sampler;

// xy: uv offset of the camera viewport inside the depth texture, zw: uv scale
// (the hzb texture only covers the viewport of the camera - the depth texture covers the full surface)
struct ViewportRemap
{
    offset_scale: vec4<f32>,
};

@group(0) @binding(2)
var<uniform> viewport_remap : ViewportRemap;

struct VertexOutput
{
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx : u32) -> VertexOutput
{
    // fullscreen triangle
    var positions = array<vec2<f32>, 3>
    (
        vec2(-1.0, -3.0),
        vec2( 3.0,  1.0),
        vec2(-1.0,  1.0)
    );

    var uv = (positions[idx] * 0.5 + vec2(0.5)) * vec2(1.0, 1.0);

    var out: VertexOutput;
    out.pos = vec4(positions[idx], 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) f32
{
    let uv = viewport_remap.offset_scale.xy + in.uv * viewport_remap.offset_scale.zw;

    // depth-texture sampling returns float in fragment stage!
    let d = textureSample(depth_tex, sampler_nearest, uv);
    return d;
}

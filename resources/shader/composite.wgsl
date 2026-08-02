// post processing composite: hdr scene color (+ bloom) -> tone mapping -> gamma -> surface
//
// the scene renders linear hdr into a Rgba16Float target - this fullscreen pass brings
// the result into the (ldr) surface format. exposure/gamma match the former base.wgsl
// behavior (0.0 disables the respective step)

// set at pipeline creation (unused here - kept for the shared fullscreen pipeline setup)
override REVERSE_Z: bool = false;

struct CompositeUniform
{
    exposure: f32,        // 0.0 = tone mapping disabled
    gamma: f32,           // 0.0 = gamma correction disabled
    bloom_intensity: f32, // 0.0 = bloom disabled
    _padding: f32,
};

@group(0) @binding(0) var<uniform> composite: CompositeUniform;

@group(0) @binding(1) var t_hdr: texture_2d<f32>;
@group(0) @binding(2) var t_bloom: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    var color = textureSampleLevel(t_hdr, s_linear, in.uv, 0.0).rgb;

    // bloom (mip 0 of the bloom chain, energy preserving mix)
    if (composite.bloom_intensity > 0.0001)
    {
        let bloom = textureSampleLevel(t_bloom, s_linear, in.uv, 0.0).rgb;
        color = mix(color, bloom, composite.bloom_intensity);
    }

    // tone mapping (hdr -> ldr)
    if (composite.exposure > 0.0001)
    {
        color = vec3<f32>(1.0) - exp(-color * composite.exposure);
    }

    // gamma correction
    if (composite.gamma > 0.0001)
    {
        color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / composite.gamma));
    }

    return vec4<f32>(color, 1.0);
}

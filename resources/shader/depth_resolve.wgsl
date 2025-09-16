@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32>
{
    // fullscreen triangle
    var pos = array<vec2<f32>, 3>
    (
        vec2<f32>(-1.0, -3.0),
        vec2<f32>( 3.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    return vec4<f32>(pos[idx], 0.0, 1.0);
}

@group(0) @binding(0)
var depth_tex: texture_multisampled_2d<f32>;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @builtin(frag_depth) f32
{
    let coord = vec2<i32>(i32(pos.x), i32(pos.y));
    var d = 0.0;
    let sample_count: u32 = textureNumSamples(depth_tex);
    for (var i: u32 = 0u; i < sample_count; i = i + 1u)
    {
        d = d + textureLoad(depth_tex, coord, i32(i)).x;
    }
    return d / f32(sample_count);
}

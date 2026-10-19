// set at pipeline creation (reverse z depth buffer: near = 1, far = 0)
override REVERSE_Z: bool = false;

@group(0) @binding(0)
var src_tex: texture_2d<f32>;  // previous mip

@group(0) @binding(1)
var dst_tex: texture_storage_2d<r32float, write>; // next mip

// keep the FARTHEST depth of the region: max in forward z, min in reverse z
fn farthest(a: f32, b: f32) -> f32
{
    if (REVERSE_Z)
    {
        return min(a, b);
    }
    return max(a, b);
}

fn load_farthest(coords: vec2<i32>, src_size: vec2<i32>, current: f32) -> f32
{
    let clamped = clamp(coords, vec2<i32>(0), src_size - 1);
    return farthest(current, textureLoad(src_tex, clamped, 0).r);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>)
{
    let dst_size = textureDimensions(dst_tex);
    if (gid.x >= dst_size.x || gid.y >= dst_size.y)
    {
        return;
    }

    let src_size = vec2<i32>(textureDimensions(src_tex));

    // Each destination pixel corresponds to 2x2 pixels in the source mip
    let base = vec2<i32>(gid.xy * 2u);

    var m = textureLoad(src_tex, base, 0).r;
    m = load_farthest(base + vec2(1, 0), src_size, m);
    m = load_farthest(base + vec2(0, 1), src_size, m);
    m = load_farthest(base + vec2(1, 1), src_size, m);

    // odd source sizes: the last row/column is not covered by any 2x2 footprint
    // -> include a third row/column so the reduction stays conservative
    let odd_x = (src_size.x & 1) == 1;
    let odd_y = (src_size.y & 1) == 1;

    if (odd_x)
    {
        m = load_farthest(base + vec2(2, 0), src_size, m);
        m = load_farthest(base + vec2(2, 1), src_size, m);
    }

    if (odd_y)
    {
        m = load_farthest(base + vec2(0, 2), src_size, m);
        m = load_farthest(base + vec2(1, 2), src_size, m);
    }

    if (odd_x && odd_y)
    {
        m = load_farthest(base + vec2(2, 2), src_size, m);
    }

    textureStore(dst_tex, vec2<i32>(gid.xy), vec4<f32>(m, 0.0, 0.0, 0.0));
}

@group(0) @binding(0)
var src_tex: texture_2d<f32>;  // previous mip

@group(0) @binding(1)
var dst_tex: texture_storage_2d<r32float, write>; // next mip

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>)
{
    let dst_size = textureDimensions(dst_tex);
    if (gid.x >= dst_size.x || gid.y >= dst_size.y)
    {
        return;
    }

    let src_size = textureDimensions(src_tex);

    // Each destination pixel corresponds to 2x2 pixels in the source mip
    let base = vec2<i32>(gid.xy * 2u);

    let a = textureLoad(src_tex, base, 0);
    let b = textureLoad(src_tex, base + vec2(1, 0), 0);
    let c = textureLoad(src_tex, base + vec2(0, 1), 0);
    let d = textureLoad(src_tex, base + vec2(1, 1), 0);

    // Maximum for conservative occlusion culling
    let m = max(max(a, b), max(c, d));

    textureStore(dst_tex, vec2<i32>(gid.xy), m);
}

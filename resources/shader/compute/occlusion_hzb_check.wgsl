// two-pass hzb occlusion culling: test all object bounding boxes against the
// hierarchical z-buffer (built from the depth of pass 1 = the objects visible
// in the previous frame) and write the indirect draw args for the render passes:
//  - args_visible: all currently visible objects (consumed by pass 1 of the NEXT
//    frame and by the transparent/no-depth draws of pass 2 of THIS frame)
//  - args_new: objects which became visible this frame (consumed by pass 2 of
//    THIS frame to correct the false negatives of pass 1)
// the visibility never leaves the gpu - no cpu readback/stall is needed
// (the stats readback is asynchronous and does not block)

const FLAG_OCCLUSION_TEST : u32 = 1u; // bit 0: object takes part in the occlusion test

struct BoundingBox
{
    min : vec4<f32>,
    max : vec4<f32>,
    object_id : u32,
    flags : u32,       // FLAG_* bits
    slot_start : u32,  // first draw slot of the object
    slot_count : u32,  // number of draw slots (one per mesh)
};

struct Visibility
{
    object_id : u32,
    visible : u32,
};

struct SlotMeta
{
    node_index : u32,
    index_count : u32,
    instance_count : u32,
    _pad : u32,
};

struct DrawIndexedArgs
{
    index_count : u32,
    instance_count : u32,
    first_index : u32,
    base_vertex : u32,
    first_instance : u32,
};

struct CameraUniform
{
    view_position : vec4<f32>,
    view : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    viewport_width : u32,
    viewport_height : u32,
};

struct CullParams
{
    num_objects : u32,
    num_slots : u32,
    _pad0 : u32,
    _pad1 : u32,
};

@group(0) @binding(0)
var<storage, read> bounding_boxes : array<BoundingBox>;

@group(0) @binding(1)
var<storage, read_write> visibility_buffer : array<Visibility>; // current frame

@group(0) @binding(2)
var hzb_tex : texture_2d<f32>;

@group(0) @binding(3)
var<uniform> camera : CameraUniform;

@group(0) @binding(4)
var<uniform> cull : CullParams;

@group(0) @binding(5)
var<storage, read> visibility_prev : array<Visibility>; // previous frame

@group(0) @binding(6)
var<storage, read> slots : array<SlotMeta>;

@group(0) @binding(7)
var<storage, read_write> args_visible : array<DrawIndexedArgs>;

@group(0) @binding(8)
var<storage, read_write> args_new : array<DrawIndexedArgs>;

// Convert NDC [-1,1] -> UV [0,1]
// note: no y-flip - the depth export pass stores the hzb with the same
// (flipped) convention, so both cancel out
fn ndc_to_uv(ndc_xy: vec2<f32>) -> vec2<f32>
{
    return ndc_xy * 0.5 + vec2<f32>(0.5, 0.5);
}

fn is_object_visible(bb: BoundingBox) -> bool
{
    // Get all 8 corners of the bounding box
    let corners = array<vec3<f32>, 8>
    (
        vec3<f32>(bb.min.x, bb.min.y, bb.min.z),  // 0
        vec3<f32>(bb.max.x, bb.min.y, bb.min.z),  // 1
        vec3<f32>(bb.min.x, bb.max.y, bb.min.z),  // 2
        vec3<f32>(bb.max.x, bb.max.y, bb.min.z),  // 3
        vec3<f32>(bb.min.x, bb.min.y, bb.max.z),  // 4
        vec3<f32>(bb.max.x, bb.min.y, bb.max.z),  // 5
        vec3<f32>(bb.min.x, bb.max.y, bb.max.z),  // 6
        vec3<f32>(bb.max.x, bb.max.y, bb.max.z)   // 7
    );

    // Project all corners and find screen bounds + closest depth
    var min_ndc = vec2<f32>(1.0, 1.0);
    var max_ndc = vec2<f32>(-1.0, -1.0);
    var closest_depth = 1.0;

    for (var i = 0; i < 8; i++)
    {
        let clip = camera.view_proj * vec4<f32>(corners[i], 1.0);

        // corner behind the camera -> assume visible (conservative)
        if (clip.w <= 0.0)
        {
            return true;
        }

        let ndc = clip.xyz / clip.w;

        min_ndc = min(min_ndc, ndc.xy);
        max_ndc = max(max_ndc, ndc.xy);

        // wgpu ndc depth is already in [0,1] (OPENGL_TO_WGPU_MATRIX is part of the projection)
        closest_depth = min(closest_depth, ndc.z);
    }

    // object in front of the near plane
    if (closest_depth <= 0.0)
    {
        return true;
    }

    // clamp the screen rect to the viewport (partially off-screen objects)
    let min_uv = clamp(ndc_to_uv(min_ndc), vec2<f32>(0.0), vec2<f32>(1.0));
    let max_uv = clamp(ndc_to_uv(max_ndc), vec2<f32>(0.0), vec2<f32>(1.0));

    // fully outside of the viewport -> handled by the cpu frustum culling, keep conservative
    if (min_uv.x >= max_uv.x || min_uv.y >= max_uv.y)
    {
        return true;
    }

    // pick the mip where the rect spans at most ONE texel width per axis: a rect of
    // length <= 1 texel can overlap at most 2 texel cells, so the 4 corner samples
    // below are guaranteed to cover every overlapped texel (conservative).
    // (<= 2 texel widths would not be enough - a 2-wide rect can straddle 3 cells
    // and the middle one would never be sampled -> false culling)
    let px_size = (max_uv - min_uv) * vec2<f32>(f32(camera.viewport_width), f32(camera.viewport_height));
    let extent = max(px_size.x, px_size.y);

    let max_mip = f32(textureNumLevels(hzb_tex) - 1u);
    var mip_f = 0.0;
    if (extent > 1.0)
    {
        mip_f = clamp(ceil(log2(extent)), 0.0, max_mip);
    }
    let mip = i32(mip_f);

    // sample the 4 texels covering the rect (max = farthest depth in the region)
    let dims = vec2<i32>(textureDimensions(hzb_tex, mip));
    let t0 = clamp(vec2<i32>(min_uv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);
    let t1 = clamp(vec2<i32>(max_uv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);

    var hzb_depth = textureLoad(hzb_tex, t0, mip).r;
    hzb_depth = max(hzb_depth, textureLoad(hzb_tex, vec2<i32>(t1.x, t0.y), mip).r);
    hzb_depth = max(hzb_depth, textureLoad(hzb_tex, vec2<i32>(t0.x, t1.y), mip).r);
    hzb_depth = max(hzb_depth, textureLoad(hzb_tex, t1, mip).r);

    // visible if the closest point of the box is closer than the farthest occluder depth
    return closest_depth <= hzb_depth;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>)
{
    let idx = gid.x;
    if (idx >= cull.num_objects)
    {
        return;
    }

    let bb = bounding_boxes[idx];

    // objects without the occlusion test flag (per node setting, no depth test, ...) are always visible
    var visible = true;
    if ((bb.flags & FLAG_OCCLUSION_TEST) != 0u)
    {
        visible = is_object_visible(bb);
    }

    visibility_buffer[idx].object_id = bb.object_id;
    visibility_buffer[idx].visible = select(0u, 1u, visible);

    let was_visible = visibility_prev[idx].visible != 0u;
    let newly_visible = visible && !was_visible;

    // write the indirect draw args for all draw slots (one per mesh) of the object
    let slot_end = min(bb.slot_start + bb.slot_count, cull.num_slots);
    for (var s = bb.slot_start; s < slot_end; s++)
    {
        let slot = slots[s];

        args_visible[s].index_count = slot.index_count;
        args_visible[s].instance_count = select(0u, slot.instance_count, visible);
        args_visible[s].first_index = 0u;
        args_visible[s].base_vertex = 0u;
        args_visible[s].first_instance = 0u;

        args_new[s].index_count = slot.index_count;
        args_new[s].instance_count = select(0u, slot.instance_count, newly_visible);
        args_new[s].first_index = 0u;
        args_new[s].base_vertex = 0u;
        args_new[s].first_instance = 0u;
    }
}

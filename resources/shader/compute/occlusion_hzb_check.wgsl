struct BoundingBox
{
    min : vec4<f32>,
    max : vec4<f32>,
    object_id : u32,
};

struct Visibility
{
    object_id : u32,
    visible : u32,
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
};


@group(0) @binding(0)
var<storage, read> bounding_boxes : array<BoundingBox>;

@group(0) @binding(1)
var<storage, read_write> visibility_buffer : array<Visibility>;

@group(0) @binding(2)
var hzb_tex : texture_2d<f32>;

@group(0) @binding(3)
var<uniform> camera : CameraUniform;

@group(0) @binding(4)
var<uniform> cull : CullParams;

fn project_clip(p: vec3<f32>) -> vec4<f32>
{
    return camera.view_proj * vec4<f32>(p, 1.0);
}

// convert clip -> ndc (x,y) and normalized depth [0..1]
fn clip_to_ndc_xy_depth(c: vec4<f32>) -> vec3<f32>
{
    // if w == 0 this is degenerate; caller should ensure w > 0 where appropriate
    let ndc = c.xyz / c.w;
    // ndc.xy in [-1,1], ndc.z in [-1,1]
    let z01 = ndc.z * 0.5 + 0.5;
    return vec3<f32>(ndc.xy, z01);
}

// clamp a float to [0,1]
fn clamp01(v: f32) -> f32
{
    return clamp(v, 0.0, 1.0);
}

// Convert NDC [-1,1] -> UV [0,1]
fn ndc_to_uv(ndc_xy: vec2<f32>) -> vec2<f32>
{
    return ndc_xy * 0.5 + vec2<f32>(0.5, 0.5);
}

// compute best mip level for a screen rectangle measured in pixels
fn compute_mip_for_rect_px(px_w: f32, px_h: f32, max_dim: f32) -> i32
{
    let extent = max(px_w, px_h);
    if (extent <= 1.0) {
        return 0;
    }
    // desired mip is ceil(log2(extent))
    let m = ceil(log2(extent));
    // max mip = floor(log2(max_dim))
    let max_m = floor(log2(max_dim));
    return i32(clamp(m, 0.0, max_m));
}

// sample center texel at given mip (returns depth in [0..1])
fn sample_hzb_at_mip(uv: vec2<f32>, mip: i32) -> f32
{
    let dims = textureDimensions(hzb_tex, mip);
    // convert uv*size -> integer texel coordinates, clamp inside
    let sx = i32(clamp(floor(uv.x * f32(dims.x)), 0.0, f32(dims.x - 1)));
    let sy = i32(clamp(floor(uv.y * f32(dims.y)), 0.0, f32(dims.y - 1)));
    return textureLoad(hzb_tex, vec2<i32>(sx, sy), mip).r;
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
    var behind_camera = false;

    for (var i = 0; i < 8; i++)
    {
        let clip = project_clip(corners[i]);

        // Check if behind camera (w <= 0)
        if (clip.w <= 0.0)
        {
            behind_camera = true;
            break;
        }

        let ndc = clip_to_ndc_xy_depth(clip);

        // Update screen bounds
        min_ndc = min(min_ndc, ndc.xy);
        max_ndc = max(max_ndc, ndc.xy);

        // Track closest depth
        closest_depth = min(closest_depth, ndc.z);
    }

    // If any corner is behind camera, assume visible (conservative)
    if (behind_camera)
    {
        visibility_buffer[idx].object_id = bb.object_id;
        visibility_buffer[idx].visible = 1u;
        return;
    }

    // Convert NDC bounds to UV space [0,1]
    let min_uv = ndc_to_uv(min_ndc);
    let max_uv = ndc_to_uv(max_ndc);

    // Convert UV bounds to pixel coordinates
    let px_min = min_uv * vec2<f32>(f32(camera.viewport_width), f32(camera.viewport_height));
    let px_max = max_uv * vec2<f32>(f32(camera.viewport_width), f32(camera.viewport_height));
    let px_size = px_max - px_min;

    // Find appropriate mip level for the projected size
    let max_tex_dim = f32(max(camera.viewport_width, camera.viewport_height));
    let mip = compute_mip_for_rect_px(px_size.x, px_size.y, max_tex_dim);

    // Sample HZB texture at multiple points for better coverage
    let center_uv = (min_uv + max_uv) * 0.5;
    let hzb_depth = sample_hzb_at_mip(center_uv, mip);

    // Conservative occlusion test: object is visible if its closest point is closer than HZB
    let is_visible = closest_depth <= hzb_depth;

    visibility_buffer[idx].object_id = bb.object_id;
    visibility_buffer[idx].visible = select(0u, 1u, is_visible);
}

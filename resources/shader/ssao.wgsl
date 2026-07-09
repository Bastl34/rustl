const PI: f32 = 3.141592653589793;

const SSAO_SAMPLES: u32 = 16u;

// hemisphere kernel (z > 0, scaled so the samples concentrate near the origin)
const SSAO_KERNEL = array<vec3<f32>, 16>(
    vec3<f32>(0.010418, -0.064807, 0.075442),
    vec3<f32>(0.070482, -0.056826, 0.050185),
    vec3<f32>(0.104545, -0.037364, 0.026162),
    vec3<f32>(0.082727, 0.061880, 0.081587),
    vec3<f32>(0.050731, 0.134168, 0.061962),
    vec3<f32>(-0.141655, -0.109644, 0.056699),
    vec3<f32>(0.102098, -0.101710, 0.174819),
    vec3<f32>(0.170855, 0.125954, 0.170507),
    vec3<f32>(0.158181, -0.232375, 0.163112),
    vec3<f32>(-0.193840, -0.208889, 0.258527),
    vec3<f32>(0.010240, -0.439739, 0.102144),
    vec3<f32>(0.361148, 0.073478, 0.374445),
    vec3<f32>(0.175082, -0.482472, 0.322654),
    vec3<f32>(-0.585219, 0.001450, 0.373294),
    vec3<f32>(0.486595, 0.381609, 0.490122),
    vec3<f32>(0.482265, 0.072344, 0.745718),
);

// must match SsaoUniform in rendering/bind_groups/ssao.rs
struct SsaoUniform
{
    projection: mat4x4<f32>,     // camera view -> clip
    inv_projection: mat4x4<f32>, // clip -> camera view

    // camera viewport in surface pixels (top-left origin): xy = offset, zw = size
    viewport: vec4<f32>,

    radius: f32,
    bias: f32,

    _padding: vec2<f32>,
};

// ssao pass (fs_main): binding 0 = scene depth (depth pre-pass)
// blur pass (fs_blur): binding 2 = raw ssao result
// (each entry point only uses its own texture binding - the uniform is shared)
@group(0) @binding(0) var t_depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> ssao: SsaoUniform;
@group(0) @binding(2) var t_ssao_raw: texture_2d<f32>;

struct VertexOutput
{
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput
{
    // fullscreen triangle (rasterized only inside the camera viewport via set_viewport)
    var positions = array<vec2<f32>, 3>
    (
        vec2(-1.0, -3.0),
        vec2( 3.0,  1.0),
        vec2(-1.0,  1.0)
    );

    var out: VertexOutput;
    out.pos = vec4(positions[idx], 0.0, 1.0);
    return out;
}

// clamp a pixel position to the camera viewport (avoids bleeding into other camera viewports)
fn clamp_to_viewport(px: vec2<i32>) -> vec2<i32>
{
    let vp_min = vec2<i32>(ssao.viewport.xy);
    let vp_max = vec2<i32>(ssao.viewport.xy + ssao.viewport.zw) - vec2<i32>(1);
    return clamp(px, vp_min, vp_max);
}

fn in_viewport(px: vec2<i32>) -> bool
{
    return all(px == clamp_to_viewport(px));
}

fn load_depth(px: vec2<i32>) -> f32
{
    return textureLoad(t_depth, clamp_to_viewport(px), 0);
}

// pixel position (surface space) -> ndc xy inside the camera viewport
fn px_to_ndc(px: vec2<f32>) -> vec2<f32>
{
    let rel = (px - ssao.viewport.xy) / ssao.viewport.zw; // 0..1, y down
    return vec2<f32>(rel.x * 2.0 - 1.0, (1.0 - rel.y) * 2.0 - 1.0);
}

// reconstruct the camera/view space position (wgpu ndc z is already 0..1 - no remapping)
fn view_pos_from_depth(ndc_xy: vec2<f32>, depth: f32) -> vec3<f32>
{
    let clip = vec4<f32>(ndc_xy, depth, 1.0);
    let view = ssao.inv_projection * clip;
    return view.xyz / view.w;
}

// view space position of a pixel from its already loaded depth
fn view_pos_at(px: vec2<i32>, depth: f32) -> vec3<f32>
{
    let center = vec2<f32>(px) + vec2<f32>(0.5);
    return view_pos_from_depth(px_to_ndc(center), depth);
}

// view space z only - two dot products instead of the full inverse projection multiply
// (wgsl matrices are column major: m[col][row])
fn view_z_from_depth(ndc_xy: vec2<f32>, depth: f32) -> f32
{
    let m = ssao.inv_projection;
    let z = m[0][2] * ndc_xy.x + m[1][2] * ndc_xy.y + m[2][2] * depth + m[3][2];
    let w = m[0][3] * ndc_xy.x + m[1][3] * ndc_xy.y + m[2][3] * depth + m[3][3];
    return z / w;
}

// view space normal from depth: central differences, per axis the neighbor
// with the smaller depth delta is used (reduces artifacts at depth edges)
// neighbors outside the viewport are rejected so the viewport rim uses the
// one-sided difference towards the interior (no seam at screen/viewport edges)
fn reconstruct_normal(px: vec2<i32>, depth_center: f32, center_pos: vec3<f32>) -> vec3<f32>
{
    let px_left  = px + vec2<i32>(-1,  0);
    let px_right = px + vec2<i32>( 1,  0);
    let px_up    = px + vec2<i32>( 0, -1);
    let px_down  = px + vec2<i32>( 0,  1);

    let depth_left  = load_depth(px_left);
    let depth_right = load_depth(px_right);
    let depth_up    = load_depth(px_up);
    let depth_down  = load_depth(px_down);

    var delta_left  = abs(depth_left - depth_center);
    var delta_right = abs(depth_right - depth_center);
    var delta_up    = abs(depth_up - depth_center);
    var delta_down  = abs(depth_down - depth_center);

    const OUTSIDE_PENALTY: f32 = 1.0e9;
    if (!in_viewport(px_left))  { delta_left  = OUTSIDE_PENALTY; }
    if (!in_viewport(px_right)) { delta_right = OUTSIDE_PENALTY; }
    if (!in_viewport(px_up))    { delta_up    = OUTSIDE_PENALTY; }
    if (!in_viewport(px_down))  { delta_down  = OUTSIDE_PENALTY; }

    var ddx: vec3<f32>;
    if (delta_left < delta_right)
    {
        ddx = center_pos - view_pos_at(px_left, depth_left);
    }
    else
    {
        ddx = view_pos_at(px_right, depth_right) - center_pos;
    }

    var ddy: vec3<f32>;
    if (delta_up < delta_down)
    {
        ddy = center_pos - view_pos_at(px_up, depth_up);
    }
    else
    {
        ddy = view_pos_at(px_down, depth_down) - center_pos;
    }

    var normal = normalize(cross(ddx, ddy));

    // orient towards the camera (view dir from fragment to camera is -center_pos)
    if (dot(normal, center_pos) > 0.0)
    {
        normal = -normal;
    }

    return normal;
}

// interleaved gradient noise - stable per pixel random rotation for the kernel
fn interleaved_gradient_noise(px: vec2<f32>) -> f32
{
    return fract(52.9829189 * fract(0.06711056 * px.x + 0.00583715 * px.y));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) f32
{
    let px = vec2<i32>(in.pos.xy);

    let depth = load_depth(px);

    // background: no occlusion
    if (depth >= 1.0)
    {
        return 1.0;
    }

    let view_pos = view_pos_from_depth(px_to_ndc(in.pos.xy), depth);
    let normal = reconstruct_normal(px, depth, view_pos);

    // random rotation around the normal (per pixel) against banding
    let angle = interleaved_gradient_noise(in.pos.xy) * 2.0 * PI;
    let random_vec = vec3<f32>(cos(angle), sin(angle), 0.0);

    var tangent = random_vec - normal * dot(random_vec, normal);
    if (length(tangent) < 0.0001)
    {
        let fallback = vec3<f32>(0.0, 0.0, 1.0);
        tangent = fallback - normal * dot(fallback, normal);
    }
    tangent = normalize(tangent);
    let bitangent = cross(normal, tangent);
    let tbn = mat3x3<f32>(tangent, bitangent, normal);

    var occlusion = 0.0;
    for (var i = 0u; i < SSAO_SAMPLES; i = i + 1u)
    {
        let sample_pos = view_pos + (tbn * SSAO_KERNEL[i]) * ssao.radius;

        // project the sample back to screen space
        let clip = ssao.projection * vec4<f32>(sample_pos, 1.0);
        if (clip.w <= 0.0)
        {
            continue;
        }

        let ndc = clip.xyz / clip.w;
        if (abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0)
        {
            continue;
        }

        let sample_px = ssao.viewport.xy + vec2<f32>(ndc.x * 0.5 + 0.5, 1.0 - (ndc.y * 0.5 + 0.5)) * ssao.viewport.zw;
        let scene_depth = load_depth(vec2<i32>(sample_px));

        // view space z of the geometry at the sample's screen position (only z is needed)
        let scene_z = view_z_from_depth(ndc.xy, scene_depth);

        // camera looks along -z: closer to the camera = larger z
        let occluded = scene_z >= sample_pos.z + ssao.bias;

        // fade out contributions of geometry far away from the shading point
        let range_check = smoothstep(0.0, 1.0, ssao.radius / max(abs(view_pos.z - scene_z), 0.0001));

        occlusion += select(0.0, 1.0, occluded) * range_check;
    }

    return 1.0 - (occlusion / f32(SSAO_SAMPLES));
}

// 4x4 box blur - removes the noise of the per pixel random kernel rotation
@fragment
fn fs_blur(in: VertexOutput) -> @location(0) f32
{
    let px = vec2<i32>(in.pos.xy);

    var sum = 0.0;
    for (var y = -2; y < 2; y = y + 1)
    {
        for (var x = -2; x < 2; x = x + 1)
        {
            let sample_px = clamp_to_viewport(px + vec2<i32>(x, y));
            sum += textureLoad(t_ssao_raw, sample_px, 0).r;
        }
    }

    return sum / 16.0;
}

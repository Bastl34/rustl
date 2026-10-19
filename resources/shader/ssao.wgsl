const PI: f32 = 3.141592653589793;

// set at pipeline creation (reverse z depth buffer: near = 1, far = 0)
override REVERSE_Z: bool = false;

// nothing was rendered at this pixel (depth still at its clear value)
fn is_background(depth: f32) -> bool
{
    if (REVERSE_Z)
    {
        return depth <= 0.0;
    }
    return depth >= 1.0;
}

// maximum screen space footprint of the kernel as a fraction of the viewport height -
// close-up geometry would otherwise project the world space radius onto a huge pixel
// area (the widely scattered depth loads thrash the texture cache and the pass cost
// explodes near the camera)
const SSAO_MAX_FOOTPRINT: f32 = 0.1;

// hemisphere kernel (z > 0, scaled so the samples concentrate near the origin) -
// full res mode uses the first 16 samples, half res mode all 32 (the lower pixel
// count leaves budget for more samples, which reduces noise/shimmer under motion)
const SSAO_KERNEL = array<vec3<f32>, 32>(
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
    vec3<f32>(0.090291, -0.307596, 0.112527),
    vec3<f32>(-0.201898, 0.172489, 0.258593),
    vec3<f32>(0.256005, -0.269636, 0.156578),
    vec3<f32>(-0.335391, -0.200693, 0.197874),
    vec3<f32>(-0.341832, -0.217433, 0.247241),
    vec3<f32>(0.054417, -0.338500, 0.381619),
    vec3<f32>(0.239836, -0.382508, 0.319817),
    vec3<f32>(0.419288, -0.338050, 0.253863),
    vec3<f32>(0.591670, -0.211460, 0.118713),
    vec3<f32>(-0.444354, 0.382883, 0.354436),
    vec3<f32>(0.467487, 0.349680, 0.443398),
    vec3<f32>(0.646956, -0.166097, 0.408068),
    vec3<f32>(0.490251, 0.176392, 0.651528),
    vec3<f32>(0.298743, 0.790080, 0.272746),
    vec3<f32>(-0.723409, -0.559933, 0.228391),
    vec3<f32>(-0.522779, -0.780617, 0.342549),
);

// must match SsaoUniform in rendering/bind_groups/ssao.rs
struct SsaoUniform
{
    projection: mat4x4<f32>,     // camera view -> clip
    inv_projection: mat4x4<f32>, // clip -> camera view

    // camera viewport in surface pixels (top-left origin): xy = offset, zw = size
    viewport: vec4<f32>,

    // camera viewport in the pixels of the ao render targets - equal to `viewport` at
    // full resolution, half of it in half resolution mode
    ao_viewport: vec4<f32>,

    radius: f32,
    bias: f32,

    // kernel sample count: 16 in full res mode, 32 in half res mode
    samples: u32,

    _padding: f32,
};

// ssao pass (fs_main): binding 0 = scene depth (depth pre-pass)
// blur pass (fs_blur): binding 2 = raw ssao result
// upsample pass (fs_upsample, half res mode only): binding 0 = scene depth, binding 2 = blurred half res ssao
// (each entry point only uses its own texture bindings - the uniform is shared)
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

// same clamp in the pixel space of the ao render targets
fn clamp_to_ao_viewport(px: vec2<i32>) -> vec2<i32>
{
    let vp_min = vec2<i32>(ssao.ao_viewport.xy);
    let vp_max = vec2<i32>(ssao.ao_viewport.xy + ssao.ao_viewport.zw) - vec2<i32>(1);
    return clamp(px, vp_min, vp_max);
}

// ao target pixel position -> surface pixel position (identity at full resolution)
fn ao_px_to_full(pos: vec2<f32>) -> vec2<f32>
{
    let scale = ssao.viewport.zw / ssao.ao_viewport.zw;
    return ssao.viewport.xy + (pos - ssao.ao_viewport.xy) * scale;
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
    // the render target may be half resolution - all depth reads and the projection
    // math happen in full resolution surface pixels. snap to the CENTER of the
    // representative full res pixel: the reconstruction ray and the loaded depth must
    // belong to the same position, otherwise flat surfaces self-occlude at grazing
    // angles (false, view dependent occlusion that flickers with camera motion)
    let px = vec2<i32>(ao_px_to_full(in.pos.xy));
    let full_pos = vec2<f32>(px) + vec2<f32>(0.5);

    let depth = load_depth(px);

    // background: no occlusion
    if (is_background(depth))
    {
        return 1.0;
    }

    let ndc_center = px_to_ndc(full_pos);
    let view_pos = view_pos_from_depth(ndc_center, depth);
    let normal = reconstruct_normal(px, depth, view_pos);

    // clamp the kernel radius so its screen space footprint stays bounded
    // (projecting a view space x offset handles perspective and orthographic cameras)
    let clip_offset = ssao.projection * vec4<f32>(view_pos + vec3<f32>(ssao.radius, 0.0, 0.0), 1.0);
    let px_radius = abs(clip_offset.x / clip_offset.w - ndc_center.x) * 0.5 * ssao.viewport.z;
    let max_px_radius = SSAO_MAX_FOOTPRINT * ssao.viewport.w;
    let radius = ssao.radius * min(1.0, max_px_radius / max(px_radius, 0.0001));

    // random rotation around the normal against banding - anchored at the full res
    // pixel so the pattern is identical in full and half res mode
    let angle = interleaved_gradient_noise(full_pos) * 2.0 * PI;
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
    for (var i = 0u; i < ssao.samples; i = i + 1u)
    {
        let sample_pos = view_pos + (tbn * SSAO_KERNEL[i]) * radius;

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
        let range_check = smoothstep(0.0, 1.0, radius / max(abs(view_pos.z - scene_z), 0.0001));

        occlusion += select(0.0, 1.0, occluded) * range_check;
    }

    return 1.0 - (occlusion / f32(ssao.samples));
}

// 4x4 box blur - removes the noise of the per pixel random kernel rotation
// (runs in the pixel space of the ao render targets)
@fragment
fn fs_blur(in: VertexOutput) -> @location(0) f32
{
    let px = vec2<i32>(in.pos.xy);

    var sum = 0.0;
    for (var y = -2; y < 2; y = y + 1)
    {
        for (var x = -2; x < 2; x = x + 1)
        {
            let sample_px = clamp_to_ao_viewport(px + vec2<i32>(x, y));
            sum += textureLoad(t_ssao_raw, sample_px, 0).r;
        }
    }

    return sum / 16.0;
}

// bilateral upsample (half res mode only): lifts the blurred half res ao to full
// resolution - the 4 nearest half res texels are mixed with bilinear weights, each
// additionally weighted by depth similarity so the ao does not bleed across depth
// edges (no dark halos around objects)
@fragment
fn fs_upsample(in: VertexOutput) -> @location(0) f32
{
    let full_pos = in.pos.xy;
    let px = vec2<i32>(full_pos);

    let center_depth = load_depth(px);

    // background: no occlusion
    if (is_background(center_depth))
    {
        return 1.0;
    }

    let center_z = view_z_from_depth(px_to_ndc(full_pos), center_depth);

    // position in the half res ao target
    let scale = ssao.ao_viewport.zw / ssao.viewport.zw;
    let ao_pos = ssao.ao_viewport.xy + (full_pos - ssao.viewport.xy) * scale;

    let base = ao_pos - vec2<f32>(0.5);
    let base_floor = floor(base);
    let frac = base - base_floor;

    var sum = 0.0;
    var weight_sum = 0.0;

    for (var i = 0; i < 4; i = i + 1)
    {
        let offset = vec2<i32>(i % 2, i / 2);
        let texel = clamp_to_ao_viewport(vec2<i32>(base_floor) + offset);

        // depth of the full res pixel this ao texel was computed from
        // (snapped to the pixel center - matches the position fs_main used)
        let texel_full_px = vec2<i32>(ao_px_to_full(vec2<f32>(texel) + vec2<f32>(0.5)));
        let texel_full_pos = vec2<f32>(texel_full_px) + vec2<f32>(0.5);
        let texel_depth = load_depth(texel_full_px);
        let texel_z = view_z_from_depth(px_to_ndc(texel_full_pos), texel_depth);

        let weight_x = select(1.0 - frac.x, frac.x, offset.x == 1);
        let weight_y = select(1.0 - frac.y, frac.y, offset.y == 1);
        let weight_depth = 1.0 / (0.001 + abs(texel_z - center_z));

        let weight = weight_x * weight_y * weight_depth;

        sum += textureLoad(t_ssao_raw, texel, 0).r * weight;
        weight_sum += weight;
    }

    return sum / max(weight_sum, 0.0001);
}

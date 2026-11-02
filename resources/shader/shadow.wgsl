// Depth-only shadow pass.
// Renders scene geometry from a light's point of view into one layer of the shadow atlas.
// The per-view light-space matrix is provided as a dynamic-offset uniform (group 0), so the
// same pipeline/bind group renders every shadow view (directional cascades, spot, point faces).
// Skeleton + morph target bindings (group 1) mirror SkeletonMorphTargetBindGroup so skinned /
// morphed casters deform correctly.
//
// Two pipelines share this module:
// - opaque casters: vertex stage only (vs_main)
// - alpha textured casters (e.g. leaves): vs_main + fs_main, which alpha-tests the material
//   textures (group 2, subset of the color pass material bind group) and discards cutout pixels

const MAX_JOINTS = [MAX_JOINTS];
const MAX_MORPH_TARGETS: u32 = [MAX_MORPH_TARGETS]u;

const PI: f32 = 3.141592653589793;

// shadow pipelines of material shaders: vertex hook in vs_main + fs_material
override MATERIAL_SHADER: bool = false;

// ****************************** structs ******************************

struct ShadowViewUniform
{
    view_proj: mat4x4<f32>,
};

struct SkeletonUniform
{
    joint_transforms: array<mat4x4<f32>, MAX_JOINTS>,
    joints_amount: u32,
};

struct MorphTargetUniform
{
    weights: array<vec4<f32>, MAX_MORPH_TARGETS>,
    amount: u32,
};

// must match the layout in rendering/material.rs (same uniform as base.wgsl)
const TEXTURE_AMOUNT = 14;

struct TextureTransform
{
    offset: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
    uv_index: u32,

    _padding: vec2<f32>,
};

struct MaterialUniform
{
    ambient_color: vec4<f32>,
    base_color: vec4<f32>,
    specular_color: vec4<f32>,
    highlight_color: vec4<f32>,
    locked_color: vec4<f32>,

    blend_mode: u32,
    alpha: f32,
    alpha_cutoff: f32,

    shininess: f32,
    reflectivity: f32,
    refraction_index: f32,

    normal_map_strength: f32,
    roughness: f32,
    receive_shadow: u32,

    unlit_shading: u32,

    ibl_diffuse_intensity: f32,

    allow_xray: u32,

    texture_transforms: array<TextureTransform, TEXTURE_AMOUNT>,
    textures_used: u32,

    mapping_mode: u32,
    mapping_space: u32,
    mapping_axis: u32,
    mapping_scale: f32,
    mapping_sharpness: f32,

    shadow_softness: f32,

    no_fog: u32,

    vertex_colors: u32,

    time: f32,

    custom_params: array<vec4<f32>, 8>,
};

struct VertexInput
{
    @builtin(vertex_index) index: u32,
    @location(0) position: vec3<f32>,
    @location(1) tex_coords_0_1: vec4<f32>,
    @location(2) tex_coords_2_3: vec4<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec3<f32>,
    @location(5) bitangent: vec3<f32>,

    @location(6) joints: vec4<u32>,
    @location(7) weights: vec4<f32>,

    @location(8) color: vec4<f32>,
};

struct InstanceInput
{
    @location(9) model_matrix_0: vec4<f32>,
    @location(10) model_matrix_1: vec4<f32>,
    @location(11) model_matrix_2: vec4<f32>,
    @location(12) model_matrix_3: vec4<f32>,

    @location(13) color: vec4<f32>,
    @location(14) highlight: f32,
    @location(15) locked: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,

    // only used by the alpha test in fs_main (cutout pipeline)
    @location(0) tex_coords_0_1: vec4<f32>,
    @location(1) tex_coords_2_3: vec4<f32>,
    @location(2) instance_alpha: f32,

    // material shaders only
    @location(3) position: vec3<f32>,
    @location(4) normal: vec3<f32>,
    @location(5) object_position: vec3<f32>,
    @location(6) color: vec4<f32>,
    @location(7) object_origin: vec3<f32>, // world position of the object origin
};

// ****************************** inputs / bindings ******************************

@group(0) @binding(0)
var<uniform> shadow_view: ShadowViewUniform;

@group(1) @binding(0)
var<uniform> skeleton: SkeletonUniform;

@group(1) @binding(1)
var<uniform> morpth_target: MorphTargetUniform;

@group(1) @binding(2) var t_morpth_targets: texture_2d_array<f32>;

// material (cutout pipeline only) - subset of the color pass material bind group,
// the binding indices must match base.wgsl / rendering/material.rs
@group(2) @binding(0) var<uniform> material: MaterialUniform;

@group(2) @binding(3) var tex_base: texture_2d<f32>;
@group(2) @binding(4) var tex_base_sampler: sampler;

@group(2) @binding(9) var tex_alpha: texture_2d<f32>;
@group(2) @binding(10) var tex_alpha_sampler: sampler;

// for material shaders
@group(2) @binding(21) var tex_custom0: texture_2d<f32>;
@group(2) @binding(22) var tex_custom0_sampler: sampler;
@group(2) @binding(23) var tex_custom1: texture_2d<f32>;
@group(2) @binding(24) var tex_custom1_sampler: sampler;
@group(2) @binding(25) var tex_custom2: texture_2d<f32>;
@group(2) @binding(26) var tex_custom2_sampler: sampler;
@group(2) @binding(27) var tex_custom3: texture_2d<f32>;
@group(2) @binding(28) var tex_custom3_sampler: sampler;

const TEXTURE_INDEX_BASE: u32 = 1u;
const TEXTURE_INDEX_ALPHA: u32 = 4u;

fn has_base_texture() -> bool  { return (material.textures_used & (1u << 2u)) != 0u; }
fn has_alpha_texture() -> bool { return (material.textures_used & (1u << 5u)) != 0u; }

// ****************************** helper ******************************

const items: u32 = 4u;
fn read_vec_from_texture_array(vertex_index: u32, tex_id: u32, offset: u32, texture: texture_2d_array<f32>) -> vec4<f32>
{
    let dimensions = textureDimensions(texture);
    let pos = (vertex_index * items) + offset;
    let x = pos % dimensions.x;
    let y = pos / dimensions.x;

    return textureLoad(texture, vec2<u32>(x, y), tex_id, 0);
}

// uv selection + transform - mirrors get_uv/transform_uv in base.wgsl
fn get_uv(in: VertexOutput, texture_index: u32) -> vec2<f32>
{
    let transform = material.texture_transforms[texture_index];
    let uv_index = transform.uv_index;

    let uv = in.tex_coords_0_1.xy;

    if (uv_index == 1)
    {
        return in.tex_coords_0_1.zw;
    }
    else if (uv_index == 2)
    {
        return in.tex_coords_2_3.xy;
    }
    else if (uv_index == 3)
    {
        return in.tex_coords_2_3.zw;
    }

    return transform_uv(uv, texture_index);
}

fn transform_uv(uv: vec2<f32>, texture_index: u32) -> vec2<f32>
{
    let transform = material.texture_transforms[texture_index];

    let translation = mat3x3<f32>
    (
        1.0, 0.0, 0.0,
        0.0, 1.0, 0.0,
        transform.offset.x, transform.offset.y, 1.0
    );

    let rotation = mat3x3<f32>
    (
        cos(transform.rotation), sin(transform.rotation), 0.0,
       -sin(transform.rotation), cos(transform.rotation), 0.0,
        0.0, 0.0, 1.0
    );

    let scale = mat3x3<f32>
    (
        transform.scale.x, 0.0, 0.0,
        0.0, transform.scale.y, 0.0,
        0.0, 0.0, 1.0
    );

    let matrix = translation * rotation * scale;

    return (matrix * vec3<f32>(uv, 1.0)).xy;
}

// ****************************** vertex ******************************

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput
{
    let model_matrix = mat4x4<f32>
    (
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    var model_pos = vec4<f32>(model.position, 1.0);

    // morph targets (position only - shadows do not need normals/tangents)
    if (morpth_target.amount > 0u)
    {
        let vertex_id = model.index;
        for (var i: u32 = 0u; i < min(morpth_target.amount, MAX_MORPH_TARGETS); i = i + 1u)
        {
            let weight = morpth_target.weights[i].x;

            let pos = read_vec_from_texture_array(vertex_id, i, 0u, t_morpth_targets);
            model_pos.x += pos.x * weight;
            model_pos.y += pos.y * weight;
            model_pos.z += pos.z * weight;
        }
    }

    var world_position = vec4<f32>(0.0);

    if (skeleton.joints_amount > 0u)
    {
        for (var i: u32 = 0u; i < 4u; i = i + 1u)
        {
            let joint_transform = skeleton.joint_transforms[model.joints[i]];
            world_position += joint_transform * model_pos * model.weights[i];
        }

        world_position = model_matrix * world_position;
    }
    else
    {
        world_position = model_matrix * model_pos;
    }

    var out: VertexOutput;
    out.tex_coords_0_1 = model.tex_coords_0_1;
    out.tex_coords_2_3 = model.tex_coords_2_3;
    out.instance_alpha = instance.color.a;

    // material shader hook (normal without skinning - good enough for the shadow)
    if (MATERIAL_SHADER)
    {
        var vertex = CustomVertex(world_position.xyz / world_position.w, normalize((model_matrix * vec4<f32>(model.normal, 0.0)).xyz), model_pos.xyz, model.tex_coords_0_1.xy, model.tex_coords_0_1.zw, model.color, model_matrix[3].xyz, model_matrix);
        custom_vertex(&vertex);
        world_position = vec4<f32>(vertex.position, 1.0);

        out.position = vertex.position;
        out.normal = vertex.normal;
        out.object_position = vertex.local_position;
        out.tex_coords_0_1 = vec4<f32>(vertex.uv, vertex.uv1);
        out.color = instance.color;
        out.object_origin = vertex.object_position;
    }

    out.clip_position = shadow_view.view_proj * world_position;

    // hidden instances (visible = false gives alpha 0) - opaque materials ignore the alpha, so they leave the view volume
    if (instance.color.a <= 0.0)
    {
        out.clip_position = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    }

    return out;
}

// ****************************** material shader hooks ******************************
// same block in base.wgsl - a material shader replaces the default hooks (each one has to stay a single line)

struct CustomVertex
{
    position: vec3<f32>, // world space
    normal: vec3<f32>, // world space
    local_position: vec3<f32>, // object space (read only)
    uv: vec2<f32>,
    uv1: vec2<f32>,
    color: vec4<f32>, // vertex color (used when the material uses vertex colors)
    object_position: vec3<f32>, // world position of the object origin (read only)
    model: mat4x4<f32>, // object -> world (read only)
};

struct CustomSurfaceInput
{
    position: vec3<f32>, // world space
    local_position: vec3<f32>, // object space
    normal: vec3<f32>, // world space geometry normal
    uv: vec2<f32>,
    color: vec4<f32>, // instance * vertex color
    view_dir: vec3<f32>, // to the camera (zero in the shadow pass)
    object_position: vec3<f32>, // world position of the object origin
};

struct CustomSurface
{
    base_color: vec4<f32>, // alpha in a
    ambient_color: vec4<f32>,
    normal: vec3<f32>,
};

fn custom_param(index: u32) -> f32 { return material.custom_params[index / 4u][index % 4u]; }
fn get_time() -> f32 { return material.time; }
// 0..1 per object from its position (e.g. in.object_position) - changes when it moves
fn instance_seed(origin: vec3<f32>) -> f32 { return fract(sin(dot(origin, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453); }

fn custom_vertex(vertex: ptr<function, CustomVertex>) {}
fn custom_surface(in: CustomSurfaceInput, surface: ptr<function, CustomSurface>) {}
fn custom_fragment(in: CustomSurfaceInput, color: vec3<f32>) -> vec3<f32> { return color; }

// alpha test of material shaders: the surface hook can discard or change the alpha (opaque only by discard)
@fragment
fn fs_material(in: VertexOutput)
{
    var surface = CustomSurface(material.base_color * in.color, material.ambient_color, normalize(in.normal));
    if (has_base_texture())
    {
        surface.base_color *= textureSample(tex_base, tex_base_sampler, get_uv(in, TEXTURE_INDEX_BASE));
    }

    custom_surface(CustomSurfaceInput(in.position, in.object_position, normalize(in.normal), in.tex_coords_0_1.xy, in.color, vec3<f32>(0.0), in.object_origin), &surface);

    if (material.blend_mode == 0u)
    {
        return;
    }

    var alpha = surface.base_color.a * material.alpha;
    if (has_alpha_texture())
    {
        alpha *= textureSample(tex_alpha, tex_alpha_sampler, get_uv(in, TEXTURE_INDEX_ALPHA)).x;
    }

    var cutoff = 0.5;
    if (material.blend_mode == 1u) // Mask
    {
        cutoff = material.alpha_cutoff;
    }

    if (alpha <= cutoff)
    {
        discard;
    }
}

// ****************************** fragment (cutout pipeline only) ******************************

// alpha test for alpha textured shadow casters (e.g. leaves):
// pixels below the cutoff do not cast a shadow - mirrors the alpha computation in base.wgsl
@fragment
fn fs_main(in: VertexOutput)
{
    // opaque materials always cast a full shadow (the color pass forces alpha to 1.0 as well)
    if (material.blend_mode == 0u)
    {
        return;
    }

    var alpha = in.instance_alpha * material.alpha;

    if (has_base_texture())
    {
        let uv = get_uv(in, TEXTURE_INDEX_BASE);
        alpha *= textureSample(tex_base, tex_base_sampler, uv).a;
    }

    if (has_alpha_texture())
    {
        let uv = get_uv(in, TEXTURE_INDEX_ALPHA);
        alpha *= textureSample(tex_alpha, tex_alpha_sampler, uv).x;
    }

    // mask materials use their own cutoff (like the color pass), blend materials a fixed one
    var cutoff = 0.5;
    if (material.blend_mode == 1u) // Mask
    {
        cutoff = material.alpha_cutoff;
    }

    if (alpha <= cutoff)
    {
        discard;
    }
}

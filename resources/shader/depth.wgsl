const MAX_LIGHTS = [MAX_LIGHTS];
const MAX_JOINTS = [MAX_JOINTS];
const MAX_MORPH_TARGETS: u32 = [MAX_MORPH_TARGETS]u;

// ****************************** structs ******************************
struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
};

struct LightUniform
{
    position: vec4<f32>,
    dir: vec4<f32>,
    color: vec4<f32>,
    ground_color: vec4<f32>,
    intensity: f32,
    light_type: u32,
    max_angle: f32,
    distance_based_intensity: u32,
};

struct SceneUniform
{
    gamma: f32,
    exposure: f32
};

struct SkeletonUniform
{
    joint_transforms: array<mat4x4<f32>, MAX_JOINTS>,
    joints_amount: u32,
};

struct MorphTargetUniform
{
    weights: array<vec4<f32>, MAX_MORPH_TARGETS>, // array stride must be 16 - so we use vec4
    amount: u32,
};

struct VertexInput
{
    @builtin(vertex_index) index: u32,
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,

    @location(5) joints: vec4<u32>,
    @location(6) weights: vec4<f32>,
};

struct InstanceInput
{
    @location(7) model_matrix_0: vec4<f32>,
    @location(8) model_matrix_1: vec4<f32>,
    @location(9) model_matrix_2: vec4<f32>,
    @location(10) model_matrix_3: vec4<f32>,

    @location(11) color: vec4<f32>,
    @location(12) highlight: f32,
    @location(13) locked: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

// ****************************** inputs / bindings ******************************

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(1)
var<uniform> light_amount: i32;

@group(1) @binding(2)
var<uniform> lights: array<LightUniform, MAX_LIGHTS>;

@group(2) @binding(0)
var<uniform> skeleton: SkeletonUniform;

@group(2) @binding(1)
var<uniform> morpth_target: MorphTargetUniform;

@group(2) @binding(2) var t_morpth_targets: texture_2d_array<f32>;

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

    // morph targets
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

    out.tex_coords = model.tex_coords;
    out.clip_position = camera.view_proj * world_position;

    return out;
}


// ****************************** fragment ******************************

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
const MAX_LIGHTS = [MAX_LIGHTS];

// ********** vertex **********

struct CameraUniform
{
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
};
@group(1) @binding(0)
var<uniform> camera: CameraUniform;

struct LightUniform
{
    position: vec4<f32>,
    color: vec4<f32>,
    lintensity: f32,
};

@group(1) @binding(1)
var<uniform> light_amount: i32;

@group(1) @binding(2)
var<uniform> lights: array<LightUniform, MAX_LIGHTS>;

struct VertexInput
{
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct InstanceInput
{
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,

    @location(9) alpha: f32,
    @location(10) highlight: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

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

    var out: VertexOutput;

    out.tex_coords = model.tex_coords;
    out.clip_position = camera.view_proj * model_matrix * vec4<f32>(model.position, 1.0);

    return out;
}


// ********** fragment **********

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
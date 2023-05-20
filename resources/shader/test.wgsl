// ********** vertex **********

struct CameraUniform
{
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
@group(2) @binding(0)
var<uniform> light: LightUniform;

struct VertexInput
{
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>
};

struct InstanceInput
{
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
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

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@group(0) @binding(2)
var t_depth: texture_depth_2d; //texture_depth_2d_array
@group(0) @binding(3)
var s_depth: sampler_comparison;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    var uvs = in.tex_coords;

    let light_color = light.color.xyz;
    let ambient_strength = 0.1;
    let ambient_color = light_color * ambient_strength;

    let object_color = textureSample(t_diffuse, s_diffuse, uvs);

    let result = light.color.xyz * object_color.xyz;

    return vec4<f32>(result, object_color.a);

    //return textureSample(t_diffuse, s_diffuse, uvs);

    //return vec4<f32>(1.0, 0.0, 0.0, 1.0);

    //let res = textureSampleCompare(t_depth, s_depth, in.tex_coords, 0.0);
    //return vec4<f32>(res, res, res, 1.0);
}
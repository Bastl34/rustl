const MAX_LIGHTS = [MAX_LIGHTS];

// ****************************** inputs ******************************

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
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

struct InstanceInput
{
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,

    @location(9) normal_matrix_0: vec3<f32>,
    @location(10) normal_matrix_1: vec3<f32>,
    @location(11) normal_matrix_2: vec3<f32>,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) tangent_position: vec3<f32>,
    @location(2) tangent_view_position: vec3<f32>,
    //@location(3) tangent_light_position: vec3<f32>,

    @location(3) world_tangent: vec3<f32>,
    @location(4) world_bitangent: vec3<f32>,
    @location(5) world_normal: vec3<f32>,
};

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

    let normal_matrix = mat3x3<f32>(
        instance.normal_matrix_0,
        instance.normal_matrix_1,
        instance.normal_matrix_2,
    );

    // Construct the tangent matrix
    let world_normal = normalize(normal_matrix * model.normal);
    let world_tangent = normalize(normal_matrix * model.tangent);
    let world_bitangent = normalize(normal_matrix * model.bitangent);
    let tangent_matrix = transpose(mat3x3<f32>
    (
        world_tangent,
        world_bitangent,
        world_normal,
    ));

    let world_position = model_matrix * vec4<f32>(model.position, 1.0);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;

    out.tangent_position = tangent_matrix * world_position.xyz;
    out.tangent_view_position = tangent_matrix * camera.view_pos.xyz;
    //out.tangent_light_position = tangent_matrix * lights[0].position.xyz;

    out.world_tangent = world_tangent;
    out.world_bitangent = world_bitangent;
    out.world_normal = world_normal;

    return out;
}


// ****************************** fragment ******************************

struct MaterialUniform
{
    ambient_color: vec4<f32>,
    base_color: vec4<f32>,
    specular_color: vec4<f32>,

    alpha: f32,
    shininess: f32,
    reflectivity: f32,
    refraction_index: f32,

    normal_map_strength: f32,
    roughness: f32,
    receive_shadow: u32,

    textures_used: u32,
};

@group(0) @binding(0)
var<uniform> material: MaterialUniform;

@group(0) @binding(1) var t_ambient: texture_2d<f32>;
@group(0) @binding(2) var s_ambient: sampler;
@group(0) @binding(3) var t_base: texture_2d<f32>;
@group(0) @binding(4) var s_base: sampler;
@group(0) @binding(5) var t_specular: texture_2d<f32>;
@group(0) @binding(6) var s_specular: sampler;
@group(0) @binding(7) var t_normal: texture_2d<f32>;
@group(0) @binding(8) var s_normal: sampler;
@group(0) @binding(9) var t_alpha: texture_2d<f32>;
@group(0) @binding(10) var s_alpha: sampler;
@group(0) @binding(11) var t_roughness: texture_2d<f32>;
@group(0) @binding(12) var s_roughness: sampler;
@group(0) @binding(13) var t_ambient_occlusion: texture_2d<f32>;
@group(0) @binding(14) var s_ambient_occlusion: sampler;
@group(0) @binding(15) var t_reflectivity: texture_2d<f32>;
@group(0) @binding(16) var s_reflectivity: sampler;
@group(0) @binding(17) var t_shininess: texture_2d<f32>;
@group(0) @binding(18) var s_shininess: sampler;

@group(0) @binding(19) var t_custom0: texture_2d<f32>;
@group(0) @binding(20) var s_custom0: sampler;
@group(0) @binding(21) var t_custom1: texture_2d<f32>;
@group(0) @binding(22) var s_custom1: sampler;
@group(0) @binding(23) var t_custom2: texture_2d<f32>;
@group(0) @binding(24) var s_custom2: sampler;
@group(0) @binding(25) var t_custom3: texture_2d<f32>;
@group(0) @binding(26) var s_custom3: sampler;
@group(0) @binding(27) var t_custom4: texture_2d<f32>;
@group(0) @binding(28) var s_custom4: sampler;
@group(0) @binding(29) var t_custom5: texture_2d<f32>;
@group(0) @binding(30) var s_custom5: sampler;
@group(0) @binding(31) var t_custom6: texture_2d<f32>;
@group(0) @binding(32) var s_custom6: sampler;
@group(0) @binding(33) var t_custom7: texture_2d<f32>;
@group(0) @binding(34) var s_custom7: sampler;
@group(0) @binding(35) var t_custom8: texture_2d<f32>;
@group(0) @binding(36) var s_custom8: sampler;
@group(0) @binding(37) var t_custom9: texture_2d<f32>;
@group(0) @binding(38) var s_custom9: sampler;

@group(0) @binding(39) var t_depth: texture_2d<f32>;
@group(0) @binding(40) var s_depth: sampler;

/*
@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@group(0) @binding(2)
var t_normal: texture_2d<f32>;
@group(0) @binding(3)
var s_normal: sampler;
*/

/*
@group(0) @binding(2)
var t_depth: texture_depth_2d; //texture_depth_2d_array
@group(0) @binding(3)
var s_depth: sampler_comparison;
*/

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    var uvs = in.tex_coords;

    let tangent_matrix = transpose(mat3x3<f32>
    (
        in.world_tangent,
        in.world_bitangent,
        in.world_normal,
    ));

    let object_color = textureSample(t_base, s_base, uvs);
    let object_normal = textureSample(t_normal, s_normal, uvs);

    let tangent_normal = object_normal.xyz * 2.0 - 1.0;
    //let tangent_normal = in.world_normal;

    let shininess = 32.0;

    var res = vec3<f32>(0.0, 0.0, 0.0);

    let view_dir = normalize(in.tangent_view_position - in.tangent_position);

    for(var i = 0; i < min(light_amount, MAX_LIGHTS); i += 1)
    {
        let light_color = lights[i].color.xyz;
        let ambient_strength = 0.1;
        let ambient_color = (light_color * ambient_strength).xyz;

        //let light_dir = normalize(light.position.xyz - in.world_position);
        //let view_dir = normalize(camera.view_pos.xyz - in.world_position);
        let light_dir = normalize((tangent_matrix * lights[i].position.xyz) - in.tangent_position);

        let half_dir = normalize(view_dir + light_dir);

        let diffuse_strength = max(dot(tangent_normal, light_dir), 0.0);
        let diffuse_color = (lights[i].color * diffuse_strength).xyz;

        let specular_strength = pow(max(dot(tangent_normal, half_dir), 0.0), shininess);
        let specular_color = (specular_strength * lights[i].color).xyz;

        res += (ambient_color + diffuse_color + specular_color) * object_color.xyz;
    }

    return vec4<f32>(res, object_color.a);

    //return textureSample(t_diffuse, s_diffuse, uvs);

    //return vec4<f32>(1.0, 0.0, 0.0, 1.0);

    //let res = textureSampleCompare(t_depth, s_depth, in.tex_coords, 0.0);
    //return vec4<f32>(res, res, res, 1.0);
}
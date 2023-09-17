const MAX_LIGHTS = [MAX_LIGHTS];

// ****************************** inputs ******************************

struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
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

    @location(9) alpha: f32,
    @location(10) highlight: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) position: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) view_dir: vec3<f32>,

    @location(4) alpha: f32,
    @location(5) highlight: f32,
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

    let world_position = model_matrix * vec4<f32>(model.position, 1.0);

    // https://lxjk.github.io/2017/10/01/Stop-Using-Normal-Matrix.html
    let scale_squared = vec3<f32>
    (
        dot(model_matrix[0].xyz, model_matrix[0].xyz),
        dot(model_matrix[1].xyz, model_matrix[1].xyz),
        dot(model_matrix[2].xyz, model_matrix[2].xyz)
    );

    var normal =
    (
        model_matrix * vec4<f32>
        (
            model.normal.x / scale_squared.x,
            model.normal.y / scale_squared.y,
            model.normal.z / scale_squared.z,
            0.0
        )
    ).xyz;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;

    out.position = world_position.xyz / world_position.w;
    out.normal = normal;
    out.view_dir = (camera.view_pos - world_position).xyz;

    out.alpha = instance.alpha;
    out.highlight = instance.highlight;

    return out;
}


// ****************************** fragment ******************************

struct MaterialUniform
{
    ambient_color: vec4<f32>,
    base_color: vec4<f32>,
    specular_color: vec4<f32>,
    highlight_color: vec4<f32>,

    alpha: f32,
    shininess: f32,
    reflectivity: f32,
    refraction_index: f32,

    normal_map_strength: f32,
    roughness: f32,
    receive_shadow: u32,

    unlit_shading: u32,

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

@group(0) @binding(27) var t_depth: texture_2d<f32>;
@group(0) @binding(38) var s_depth: sampler;


fn has_ambient_texture() -> bool            { return (material.textures_used & (1u << 1u)) != 0u; }
fn has_base_texture() -> bool               { return (material.textures_used & (1u << 2u)) != 0u; }
fn has_specular_texture() -> bool           { return (material.textures_used & (1u << 3u)) != 0u; }
fn has_normal_texture() -> bool             { return (material.textures_used & (1u << 4u)) != 0u; }
fn has_alpha_texture() -> bool              { return (material.textures_used & (1u << 5u)) != 0u; }
fn has_roughness_texture() -> bool          { return (material.textures_used & (1u << 6u)) != 0u; }
fn has_ambient_occlusion_texture() -> bool  { return (material.textures_used & (1u << 7u)) != 0u; }
fn has_reflectivity_texture() -> bool       { return (material.textures_used & (1u << 8u)) != 0u; }
fn has_shininess_texture() -> bool          { return (material.textures_used & (1u << 9u)) != 0u; }

fn has_custom0_texture() -> bool            { return (material.textures_used & (1u << 10u)) != 0u; }
fn has_custom1_texture() -> bool            { return (material.textures_used & (1u << 11u)) != 0u; }
fn has_custom2_texture() -> bool            { return (material.textures_used & (1u << 12u)) != 0u; }
fn has_custom3_texture() -> bool            { return (material.textures_used & (1u << 13u)) != 0u; }

fn has_depth_texture() -> bool              { return (material.textures_used & (1u << 14u)) != 0u; }


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    var uvs = in.tex_coords;

    // base color
    var object_color = material.base_color;
    if (has_base_texture())
    {
        let tex_color = textureSample(t_base, s_base, uvs);
        object_color *= tex_color;
    }

    // normal
    var normal = in.normal;

    //TODO: check
    if (has_normal_texture())
    {
        let object_normal = textureSample(t_normal, s_normal, uvs);
        normal = object_normal.xyz * 2.0 - 1.0;

        normal.x *= material.normal_map_strength;
        normal.y *= material.normal_map_strength;
    }

    normal = normalize(normal);

    var color = vec3<f32>(0.0, 0.0, 0.0);

    if (material.unlit_shading != 0u || light_amount == 0)
    {
        color = object_color.rgb;
    }
    else
    {
        let view_dir = normalize(in.view_dir);

        for(var i = 0; i < min(light_amount, MAX_LIGHTS); i += 1)
        {
            let light_color = lights[i].color.rgb;
            let ambient_color = (light_color * material.ambient_color.rgb).rgb;

            var light_pos = lights[i].position.xyz;

            var light_dir = lights[i].position.xyz - in.position;
            //var distance = length(light_dir);
            //distance = distance * distance;
            light_dir = normalize(light_dir);

            let half_dir = normalize(view_dir + light_dir);

            let diffuse_strength = max(dot(normal, light_dir), 0.0);
            let diffuse_color = (lights[i].color * object_color * diffuse_strength).rgb;

            let specular_strength = pow(max(dot(normal, half_dir), 0.0), material.shininess);
            let specular_color = (lights[i].color * material.specular_color * specular_strength).rgb;

            color += ambient_color + diffuse_color + specular_color;
        }

        // ambient occlusion
        if (has_ambient_occlusion_texture())
        {
            let ambient_occlusion = textureSample(t_ambient_occlusion, s_ambient_occlusion, uvs);
            color.x *= ambient_occlusion.x;
            color.y *= ambient_occlusion.x;
            color.z *= ambient_occlusion.x;
        }
    }

    // highlight color
    if (in.highlight > 0.0001)
    {
        color = (color * 0.5) + (material.highlight_color.rgb * 0.5);
    }

    let alpha = in.alpha * object_color.a * material.alpha;

    if (alpha < 0.000001)
    {
        discard;
    }

    //return vec4<f32>(normal, alpha);
    return vec4<f32>(color, alpha);
    //return vec4<f32>(1.0, 1.0, 1.0, alpha);
    //return vec4<f32>(object_color.r, object_color.g, object_color.b, alpha);

    //return textureSample(t_diffuse, s_diffuse, uvs);

    //return vec4<f32>(1.0, 0.0, 0.0, 1.0);

    //let res = textureSampleCompare(t_depth, s_depth, in.tex_coords, 0.0);
    //return vec4<f32>(res, res, res, 1.0);
}
const PI: f32 = 3.141592653589793;

const MAX_LIGHTS = [MAX_LIGHTS];
const MAX_JOINTS = [MAX_JOINTS];
const MAX_MORPH_TARGETS: u32 = [MAX_MORPH_TARGETS]u;

const LIGHT_TYPE_DIRECTIONAL: u32 = 0u;
const LIGHT_TYPE_POINT: u32 = 1u;
const LIGHT_TYPE_SPOT: u32 = 2u;

// ****************************** inputs ******************************

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

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(1)
var<uniform> scene: SceneUniform;

@group(1) @binding(2)
var<uniform> light_amount: i32;

@group(1) @binding(3)
var<uniform> lights: array<LightUniform, MAX_LIGHTS>;

@group(2) @binding(0)
var<uniform> skeleton: SkeletonUniform;

@group(2) @binding(1)
var<uniform> morpth_target: MorphTargetUniform;

@group(2) @binding(2) var t_morpth_targets: texture_2d_array<f32>;

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

    @location(11) alpha: f32,
    @location(12) highlight: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) position: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) bitangent: vec3<f32>,
    @location(4) tangent: vec3<f32>,

    @location(5) view_dir: vec3<f32>,

    @location(6) alpha: f32,
    @location(7) highlight: f32,

    @location(8) weights: vec4<f32>, // just for debugging
};

// ****************************** vertex ******************************

const items: u32 = 4u;
fn read_vec_from_texture_array(vertex_index: u32, tex_id: u32, offset: u32, texture: texture_2d_array<f32>) -> vec4<f32>
{
    let dimensions = textureDimensions(texture);
    let pos = (vertex_index * items) + offset;
    let x = pos % dimensions.x;
    let y = pos / dimensions.x;

    return textureLoad(texture, vec2<u32>(x, y), tex_id, 0);
}


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

    let identityMatrix = mat4x4<f32>
    (
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );

    //let world_position = model_matrix * vec4<f32>(model.position, 1.0);

    //let model_pos = model_matrix * vec4<f32>(model.position, 1.0);
    var model_pos = vec4<f32>(model.position, 1.0);
    var model_normal = vec4<f32>(model.normal, 0.0);
    var model_tangent = vec4<f32>(model.tangent, 0.0);
    var model_bitangent = vec4<f32>(model.bitangent, 0.0);

    // morph targets
    if (morpth_target.amount > 0u)
    {
        let vertex_id = model.index;
        for (var i: u32 = 0u; i < min(morpth_target.amount, MAX_MORPH_TARGETS); i = i + 1u)
        //let i: u32 = 1u;
        {
            let weight = morpth_target.weights[i].x;

            // position
            let pos = read_vec_from_texture_array(vertex_id, i, 0u, t_morpth_targets);
            model_pos.x += pos.x * weight;
            model_pos.y += pos.y * weight;
            model_pos.z += pos.z * weight;

            // normal
            let normal = read_vec_from_texture_array(vertex_id, i, 1u, t_morpth_targets);
            model_normal.x += normal.x * weight;
            model_normal.y += normal.y * weight;
            model_normal.z += normal.z * weight;

            // tangent
            let tangent = read_vec_from_texture_array(vertex_id, i, 2u, t_morpth_targets);
            model_tangent.x += tangent.x * weight;
            model_tangent.y += tangent.y * weight;
            model_tangent.z += tangent.z * weight;

            // bitangent
            let bitangent = read_vec_from_texture_array(vertex_id, i, 2u, t_morpth_targets);
            model_bitangent.x += bitangent.x * weight;
            model_bitangent.y += bitangent.y * weight;
            model_bitangent.z += bitangent.z * weight;
        }
    }

    var world_position = vec4<f32>(0.0);

    var world_normal = vec4<f32>(0.0);
    var world_tangent = vec4<f32>(0.0);
    var world_bitangent = vec4<f32>(0.0);

    if (skeleton.joints_amount > 0u)
    {
        for (var i: u32 = 0u; i < 4u; i = i + 1u)
        {
            //let joint_transform = transpose(skeleton.joint_transforms[model.joints[i]]);
            let joint_transform = skeleton.joint_transforms[model.joints[i]];
            //let joint_transform = skeleton.joint_transforms[1];
            world_position += joint_transform * model_pos * model.weights[i];
            //world_position += identityMatrix * model_pos * model.weights[i];

            // normal / tangent / bitangent
            let normal = joint_transform * model_normal;
            world_normal += normal * model.weights[i];

            let tangent = joint_transform * model_tangent;
            world_tangent += tangent * model.weights[i];

            let bitangent = joint_transform * model_bitangent;
            world_bitangent += bitangent * model.weights[i];
        }

        /*
        world_position =
        (
            skeleton.joint_transforms[model.joints[0]] * model_pos * model.weights[0] +
            skeleton.joint_transforms[model.joints[1]] * model_pos * model.weights[1] +
            skeleton.joint_transforms[model.joints[2]] * model_pos * model.weights[2] +
            skeleton.joint_transforms[model.joints[3]] * model_pos * model.weights[3]
        );
        */


        //world_position.w = 1.0;
        world_position = model_matrix * world_position;
        //world_position = world_position;


        /*
        var influence =  skeleton.joint_transforms[model.joints[0]] * model.weights[0];
        influence     += skeleton.joint_transforms[model.joints[1]] * model.weights[1];
        influence     += skeleton.joint_transforms[model.joints[2]] * model.weights[2];
        influence     += skeleton.joint_transforms[model.joints[3]] * model.weights[3];

        var world_position = model_matrix * vec4<f32>(model.position, 1.0);
        world_position = influence * world_position;
        */


        /*
        let skinMatrix =    skeleton.joint_transforms[model.joints[0]] * model.weights[0] +
                            skeleton.joint_transforms[model.joints[1]] * model.weights[1] +
                            skeleton.joint_transforms[model.joints[2]] * model.weights[2] +
                            skeleton.joint_transforms[model.joints[3]] * model.weights[3];
        let world = model_matrix * skinMatrix;
        world_position = world * model_pos;
        */
    }
    else
    {
        world_position = model_matrix * model_pos;

        world_normal = model_normal;
        world_tangent = model_tangent;
        world_bitangent = model_bitangent;
    }


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
            world_normal.x / scale_squared.x,
            world_normal.y / scale_squared.y,
            world_normal.z / scale_squared.z,
            0.0
        )
    ).xyz;


    var tangent =
    (
        model_matrix * vec4<f32>
        (
            world_tangent.x / scale_squared.x,
            world_tangent.y / scale_squared.y,
            world_tangent.z / scale_squared.z,
            0.0
        )
    ).xyz;

    var bitangent =
    (
        model_matrix * vec4<f32>
        (
            world_bitangent.x / scale_squared.x,
            world_bitangent.y / scale_squared.y,
            world_bitangent.z / scale_squared.z,
            0.0
        )
    ).xyz;


    /*
    var tangent = cross(normal, vec3<f32>(0.0, 1.0, 0.0));

    if length(tangent)  <= 0.0001
    {
        tangent = cross(normal, vec3<f32>(0.0, 0.0, 1.0));
    }

    tangent = normalize(tangent);
    let bitangent = normalize(cross(normal, tangent));
    */

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;

    out.position = world_position.xyz / world_position.w;
    out.normal = normal;
    out.tangent = tangent;
    out.bitangent = bitangent;
    out.view_dir = camera.view_pos.xyz - out.position;

    out.alpha = instance.alpha;
    out.highlight = instance.highlight;

    out.weights = model.weights;

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
@group(0) @binding(19) var t_environment: texture_2d<f32>;
@group(0) @binding(20) var s_environment: sampler;

@group(0) @binding(21) var t_custom0: texture_2d<f32>;
@group(0) @binding(22) var s_custom0: sampler;
@group(0) @binding(23) var t_custom1: texture_2d<f32>;
@group(0) @binding(24) var s_custom1: sampler;
@group(0) @binding(25) var t_custom2: texture_2d<f32>;
@group(0) @binding(26) var s_custom2: sampler;
@group(0) @binding(27) var t_custom3: texture_2d<f32>;
@group(0) @binding(28) var s_custom3: sampler;

@group(0) @binding(29) var t_depth: texture_2d<f32>;
@group(0) @binding(30) var s_depth: sampler;


fn has_ambient_texture() -> bool            { return (material.textures_used & (1u << 1u)) != 0u; }
fn has_base_texture() -> bool               { return (material.textures_used & (1u << 2u)) != 0u; }
fn has_specular_texture() -> bool           { return (material.textures_used & (1u << 3u)) != 0u; }
fn has_normal_texture() -> bool             { return (material.textures_used & (1u << 4u)) != 0u; }
fn has_alpha_texture() -> bool              { return (material.textures_used & (1u << 5u)) != 0u; }
fn has_roughness_texture() -> bool          { return (material.textures_used & (1u << 6u)) != 0u; }
fn has_ambient_occlusion_texture() -> bool  { return (material.textures_used & (1u << 7u)) != 0u; }
fn has_reflectivity_texture() -> bool       { return (material.textures_used & (1u << 8u)) != 0u; }
fn has_shininess_texture() -> bool          { return (material.textures_used & (1u << 9u)) != 0u; }
fn has_environment_texture() -> bool        { return (material.textures_used & (1u << 10u)) != 0u; }

fn has_custom0_texture() -> bool            { return (material.textures_used & (1u << 11u)) != 0u; }
fn has_custom1_texture() -> bool            { return (material.textures_used & (1u << 12u)) != 0u; }
fn has_custom2_texture() -> bool            { return (material.textures_used & (1u << 13u)) != 0u; }
fn has_custom3_texture() -> bool            { return (material.textures_used & (1u << 14u)) != 0u; }

fn has_depth_texture() -> bool              { return (material.textures_used & (1u << 15u)) != 0u; }

// https://learnopengl.com/PBR/IBL/Diffuse-irradiance
const inv_atan: vec2<f32> = vec2<f32>(0.1591, 0.3183);
fn sphericalCoords(direction: vec3<f32>) -> vec2<f32>
{
    var uv = vec2<f32>(atan2(direction.z, direction.x), asin(direction.y));
    uv *= inv_atan;
    uv += 0.5;
    uv.y = 1.0 - uv.y;
    return uv;
}

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

    // ambient color
    var ambient_color = material.ambient_color;
    if (has_ambient_texture())
    {
        let tex_color = textureSample(t_ambient, s_ambient, uvs);
        ambient_color *= tex_color;
    }

    // normal
    var normal = in.normal;
    var tangent = in.tangent;
    var bitangent = in.bitangent;

    // normal mapping
    if (has_normal_texture())
    {
        var normal_map = textureSample(t_normal, s_normal, uvs).xyz;
        normal_map = normal_map * 2.0 - 1.0;

        normal_map.x *= material.normal_map_strength;
        normal_map.y *= material.normal_map_strength;

        // todo: check if normalize is needed here
        let T = normalize(tangent);
        let B = normalize(bitangent);
        let N = normalize(normal);

        // https://lettier.github.io/3d-game-shaders-for-beginners/normal-mapping.html
        normal = normalize(T * normal_map.x + B * normal_map.y + N * normal_map.z);
        //normal = normalize(mat3x3<f32>(T, B, N) * normal_map);
    }
    else
    {
        normal = normalize(normal);
    }

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
            var light_pos = lights[i].position.xyz;
            var direction_to_light = lights[i].position.xyz - in.position;

            // light intensity
            var intensity = 1.0;
            if lights[i].distance_based_intensity == 1u
            {
                switch lights[i].light_type
                {
                    case 0u //LIGHT_TYPE_DIRECTIONAL
                    {
                        intensity = lights[i].intensity;
                    }
                    case 1u //LIGHT_TYPE_POINT
                    {
                        var distance = length(direction_to_light);
                        //distance = distance * distance;
                        intensity = lights[i].intensity / (4.0 * PI * distance);
                    }
                    case 2u //LIGHT_TYPE_SPOT
                    {
                        var distance = length(direction_to_light);
                        //distance = distance * distance;
                        intensity = lights[i].intensity / (4.0 * PI * distance);

                        let dir_from_light = -normalize(direction_to_light);
                        let dot = dot(dir_from_light, lights[i].dir.xyz);
                        let angle = acos(dot);

                        if angle > lights[i].max_angle
                        {
                            intensity = 0.0;
                        }
                    }
                    default {}
                }
            }

            intensity = min(intensity, 1.0);

            // phong light dir
            switch lights[i].light_type
            {
                case 0u //LIGHT_TYPE_DIRECTIONAL
                {
                    direction_to_light = -lights[i].dir.xyz;
                }
                default {}
            }

            direction_to_light = normalize(direction_to_light);

            let half_dir = normalize(view_dir + direction_to_light);

            let diffuse_strength = max(dot(normal, direction_to_light), 0.0);
            let diffuse_color = (lights[i].color * object_color * diffuse_strength).rgb;

            let specular_strength = pow(max(dot(normal, half_dir), 0.0), material.shininess);

            /*
            let reflect_dir = reflect(-direction_to_light, normal);
            let spec_dot = max(dot(reflect_dir, view_dir), 0.0);
            let specular_strength = pow(spec_dot, material.shininess);
            */

            let specular_color = (lights[i].color * material.specular_color * specular_strength).rgb;

            color += (diffuse_color + specular_color) * intensity;
        }

        // ambient occlusion
        if (has_ambient_occlusion_texture())
        {
            let ambient_occlusion = textureSample(t_ambient_occlusion, s_ambient_occlusion, uvs);
            color.x *= ambient_occlusion.x;
            color.y *= ambient_occlusion.x;
            color.z *= ambient_occlusion.x;
        }

        // reflection with env map
        if (has_environment_texture() && material.reflectivity > 0.001)
        {
            var reflectivity = material.reflectivity;
            if (has_reflectivity_texture())
            {
                let reflectivity_value = textureSample(t_reflectivity, s_reflectivity, uvs);
                reflectivity *= reflectivity_value.x;
            }

            var roughness = material.roughness;
            if (has_roughness_texture())
            {
                let roughness_value = textureSample(t_roughness, s_roughness, uvs);
                roughness *= roughness_value.x;
            }

            let reflection = reflect(-view_dir, normal);
            let sphere_coords = sphericalCoords(reflection);

            let environment_map_levels = textureNumLevels(t_environment) - 1u;
            let mipmap_level = roughness * f32(environment_map_levels);

            let reflection_color = textureSampleLevel(t_environment, s_environment, sphere_coords, mipmap_level);
            color.x += reflection_color.x * reflectivity;
            color.y += reflection_color.y * reflectivity;
            color.z += reflection_color.z * reflectivity;
        }
    }

    // ambient color
    color.x += ambient_color.x;
    color.y += ambient_color.y;
    color.z += ambient_color.z;

    // TODO: tone mapping and gamma can be done in post

    // tone mapping (HDR -> LDR)
    if (scene.exposure > 0.0001)
    {
        let mapped = vec3<f32>(1.0) - exp(-color * scene.exposure);
        color.x = mapped.x;
        color.y = mapped.y;
        color.z = mapped.z;
    }

    // gamma correction
    if (scene.gamma > 0.0001)
    {
        let mapped = pow(color, vec3<f32>(1.0 / scene.gamma));
        color.x = mapped.x;
        color.y = mapped.y;
        color.z = mapped.z;
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
    //return vec4<f32>(in.weights.r, in.weights.g, in.weights.b, alpha);

    //return textureSample(t_diffuse, s_diffuse, uvs);

    //return vec4<f32>(1.0, 0.0, 0.0, 1.0);

    //let res = textureSampleCompare(t_depth, s_depth, in.tex_coords, 0.0);
    //return vec4<f32>(res, res, res, 1.0);
}
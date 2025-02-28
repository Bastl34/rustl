const PI: f32 = 3.141592653589793;

const MAX_LIGHTS = [MAX_LIGHTS];
const MAX_JOINTS = [MAX_JOINTS];
const MAX_MORPH_TARGETS: u32 = [MAX_MORPH_TARGETS]u;

const LIGHT_TYPE_DIRECTIONAL: u32 = 0u;
const LIGHT_TYPE_POINT: u32 = 1u;
const LIGHT_TYPE_SPOT: u32 = 2u;

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
    weights: array<vec4<f32>, MAX_MORPH_TARGETS>, // array stride must be 16 - so we use vec4 - but its just the first coordinate which matters
    amount: u32,
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
};

struct InstanceInput
{
    @location(8) model_matrix_0: vec4<f32>,
    @location(9) model_matrix_1: vec4<f32>,
    @location(10) model_matrix_2: vec4<f32>,
    @location(11) model_matrix_3: vec4<f32>,

    @location(12) color: vec4<f32>,
    @location(13) highlight: f32,
    @location(14) locked: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords_0_1: vec4<f32>,
    @location(1) tex_coords_2_3: vec4<f32>,
    @location(2) position: vec3<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
    @location(5) tangent: vec3<f32>,

    @location(6) view_dir: vec3<f32>,

    @location(7) color: vec4<f32>,
    @location(8) highlight: f32,
    @location(9) locked: f32,

    @location(10) weights: vec4<f32>, // just for debugging
};

// ****************************** inputs / bindings ******************************

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

    let identityMatrix = mat4x4<f32>
    (
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );

    var model_pos = vec4<f32>(model.position, 1.0);
    var model_normal = vec4<f32>(model.normal, 0.0);
    var model_tangent = vec4<f32>(model.tangent, 0.0);
    var model_bitangent = vec4<f32>(model.bitangent, 0.0);

    // morph targets
    if (morpth_target.amount > 0u)
    {
        let vertex_id = model.index;
        for (var i: u32 = 0u; i < min(morpth_target.amount, MAX_MORPH_TARGETS); i = i + 1u)
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
            let joint_transform = skeleton.joint_transforms[model.joints[i]];
            world_position += joint_transform * model_pos * model.weights[i];

            // normal / tangent / bitangent
            let normal = joint_transform * model_normal;
            world_normal += normal * model.weights[i];

            let tangent = joint_transform * model_tangent;
            world_tangent += tangent * model.weights[i];

            let bitangent = joint_transform * model_bitangent;
            world_bitangent += bitangent * model.weights[i];
        }

        world_position = model_matrix * world_position;
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
    let scale = sqrt(scale_squared);

    //let scale = vec3<f32>(length(model_matrix[0].xyz), length(model_matrix[1].xyz), length(model_matrix[2].xyz));

    var normal =
    (
        model_matrix * vec4<f32>
        (
            world_normal.x / scale.x, // scale_squared
            world_normal.y / scale.y, // scale_squared
            world_normal.z / scale.z, // scale_squared
            0.0
        )
    ).xyz;

    /*
    var tangent =
    (
        model_matrix * vec4<f32>
        (
            world_tangent.x / scale.x, // scale_squared
            world_tangent.y / scale.y, // scale_squared
            world_tangent.z / scale.z, // scale_squared
            0.0
        )
    ).xyz;

    //tangent = world_tangent.xyz;

    var bitangent =
    (
        model_matrix * vec4<f32>
        (
            world_bitangent.x / scale.x, // scale_squared
            world_bitangent.y / scale.y, // scale_squared
            world_bitangent.z / scale.z, // scale_squared
            0.0
        )
    ).xyz;
    */

    var tangent = cross(normal, vec3<f32>(0.0, 1.0, 0.0));

    if length(tangent) <= 0.0001
    {
        tangent = cross(normal, vec3<f32>(0.0, 0.0, 1.0));
    }

    var bitangent = cross(normal, tangent);


    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords_0_1 = model.tex_coords_0_1;
    out.tex_coords_2_3 = model.tex_coords_2_3;

    out.position = world_position.xyz / world_position.w;
    out.normal = normal;
    out.tangent = tangent;
    out.bitangent = bitangent;
    out.view_dir = camera.view_pos.xyz - out.position;

    out.color = instance.color;
    out.highlight = instance.highlight;
    out.locked = instance.locked;

    out.weights = model.weights;

    return out;
}


// ****************************** fragment ******************************

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

    _padding1: vec2<u32>,

    texture_transforms: array<TextureTransform, TEXTURE_AMOUNT>,
    textures_used: u32
};

@group(0) @binding(0) var<uniform> material: MaterialUniform;

@group(0) @binding(1) var tex_ambient: texture_2d<f32>;
@group(0) @binding(2) var tex_ambient_sampler: sampler;

@group(0) @binding(3) var tex_base: texture_2d<f32>;
@group(0) @binding(4) var tex_base_sampler: sampler;

@group(0) @binding(5) var tex_specular: texture_2d<f32>;
@group(0) @binding(6) var tex_specular_sampler: sampler;

@group(0) @binding(7) var tex_normal: texture_2d<f32>;
@group(0) @binding(8) var tex_normal_Sampler: sampler;

@group(0) @binding(9) var tex_alpha: texture_2d<f32>;
@group(0) @binding(10) var tex_alpha_sampler: sampler;

@group(0) @binding(11) var tex_roughness: texture_2d<f32>;
@group(0) @binding(12) var tex_roughness_sampler: sampler;

@group(0) @binding(13) var tex_ambient_occlusion: texture_2d<f32>;
@group(0) @binding(14) var tex_ambient_occlusion_sampler: sampler;

@group(0) @binding(15) var tex_reflectivity: texture_2d<f32>;
@group(0) @binding(16) var tex_reflectivity_sampler: sampler;

@group(0) @binding(17) var tex_shininess: texture_2d<f32>;
@group(0) @binding(18) var tex_shininess_sampler: sampler;

@group(0) @binding(19) var tex_environment: texture_2d<f32>;
@group(0) @binding(20) var tex_environment_sampler: sampler;

@group(0) @binding(21) var tex_custom0: texture_2d<f32>;
@group(0) @binding(22) var tex_custom0_sampler: sampler;

@group(0) @binding(24) var tex_custom1: texture_2d<f32>;
@group(0) @binding(24) var tex_custom1_sampler: sampler;

@group(0) @binding(25) var tex_custom2: texture_2d<f32>;
@group(0) @binding(26) var tex_custom2_sampler: sampler;

@group(0) @binding(27) var tex_custom3: texture_2d<f32>;
@group(0) @binding(28) var tex_custom3_sampler: sampler;


// additional textures
@group(0) @binding(30) var tex_depth: texture_2d<f32>;
@group(0) @binding(31) var tex_depth_sampler: sampler;

const TEXTURE_INDEX_AMBIENT: u32 = 0u;
const TEXTURE_INDEX_BASE: u32 = 1u;
const TEXTURE_INDEX_SPECULAR: u32 = 2u;
const TEXTURE_INDEX_NORMAL: u32 = 3u;
const TEXTURE_INDEX_ALPHA: u32 = 4u;
const TEXTURE_INDEX_ROUGHNESS: u32 = 5u;
const TEXTURE_INDEX_AMBIENT_OCCLUSION: u32 = 6u;
const TEXTURE_INDEX_REFLECTIVITY: u32 = 7u;
const TEXTURE_INDEX_SHININESS: u32 = 8u;
const TEXTURE_INDEX_ENVIRONMENT: u32 = 9u;

const TEXTURE_INDEX_CUSTOM0: u32 = 10u;
const TEXTURE_INDEX_CUSTOM1: u32 = 11u;
const TEXTURE_INDEX_CUSTOM2: u32 = 12u;
const TEXTURE_INDEX_CUSTOM3: u32 = 13u;

const TEXTURE_INDEX_DEPTH: u32 = 14u;


fn has_ambient_texture() -> bool            { return (material.textures_used & (1u << 1u)) != 0u; }
fn has_base_texture() -> bool               { return (material.textures_used & (1u << 2u)) != 0u; }
fn has_specular_texture() -> bool           { return (material.textures_used & (1u << 3u)) != 0u; }
fn has_normal_texture() -> bool             { return (material.textures_used & (1u << 4u)) != 0u; }
fn has_alpha_texture() -> bool              { return (material.textures_used & (1u << 5u)) != 0u; }
fn has_roughness_texture() -> bool          { return (material.textures_used & (1u << 6u)) != 0u; }
fn has_ambient_occlusion_texture() -> bool  { return (material.textures_used & (1u << 7u)) != 0u; }
fn has_reflectivity_texture() -> bool       { return (material.textures_used & (1u << 8u)) != 0u; }
fn has_shininess_texture() -> bool          { return (material.textures_used & (1u << 9u)) != 0u; }
fn has_environment_sampler_texture() -> bool        { return (material.textures_used & (1u << 10u)) != 0u; }

fn has_custom0_sampler_texture() -> bool            { return (material.textures_used & (1u << 11u)) != 0u; }
fn has_custom1_sampler_texture() -> bool            { return (material.textures_used & (1u << 12u)) != 0u; }
fn has_custom2_sampler_texture() -> bool            { return (material.textures_used & (1u << 13u)) != 0u; }
fn has_custom3_sampler_texture() -> bool            { return (material.textures_used & (1u << 14u)) != 0u; }

fn has_depth_sampler_texture() -> bool              { return (material.textures_used & (1u << 15u)) != 0u; }

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

fn easeInExpo(x: f32) -> f32
{
    if (x <= 0.00001)
    {
        return 0.0;
    }

    return pow(2.0, 10.0 * x - 10.0);
}

fn easeInQuint(x: f32) -> f32
{
    return x * x * x * x * x;
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


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    var uv = in.tex_coords_0_1.xy;

    // base color
    var object_color = material.base_color * in.color;
    if (has_base_texture())
    {
        let uv = transform_uv(uv, TEXTURE_INDEX_BASE);
        let tex_color = textureSample(tex_base, tex_base_sampler, uv);
        object_color *= tex_color;
    }

    // ambient color
    var ambient_color = material.ambient_color;
    if (has_ambient_texture())
    {
        let uv = transform_uv(uv, TEXTURE_INDEX_AMBIENT);
        let tex_color = textureSample(tex_ambient, tex_ambient_sampler, uv);
        ambient_color *= tex_color;
    }

    // normal
    var normal = in.normal;
    var tangent = in.tangent;
    var bitangent = in.bitangent;

    // normal mapping
    if (has_normal_texture())
    {
        let uv = transform_uv(uv, TEXTURE_INDEX_NORMAL);
        var normal_map = textureSample(tex_normal, tex_normal_Sampler, uv).xyz;
        normal_map = normal_map * 2.0 - 1.0;

        normal_map.x *= material.normal_map_strength;
        normal_map.y *= material.normal_map_strength;

        let T = tangent;
        let B = bitangent;
        let N = normal;

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

        var specular = material.specular_color;
        if (has_specular_texture())
        {
            let uv = transform_uv(uv, TEXTURE_INDEX_SPECULAR);
            let tex_color = textureSample(tex_specular, tex_specular_sampler, uv);
            specular *= tex_color;
        }

        for(var i = 0; i < min(light_amount, MAX_LIGHTS); i += 1)
        {
            // light_type == 0 --> disabled
            if (lights[i].light_type == 0)
            {
                continue;
            }

            let light_color = lights[i].color.rgb;
            var light_pos = lights[i].position.xyz;
            var direction_to_light = lights[i].position.xyz - in.position;

            // light intensity
            var intensity = 1.0;
            if lights[i].distance_based_intensity == 1u
            {
                switch lights[i].light_type
                {
                    case 1u //LIGHT_TYPE_DIRECTIONAL
                    {
                        intensity = lights[i].intensity;
                    }
                    case 2u //LIGHT_TYPE_POINT
                    {
                        var distance = length(direction_to_light);
                        //distance = distance * distance;
                        intensity = lights[i].intensity / (4.0 * PI * distance);
                    }
                    case 3u //LIGHT_TYPE_SPOT
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
                    case 4u //LIGHT_TYPE_HEMISPHERIC
                    {
                        intensity = lights[i].intensity;
                    }
                    default {}
                }
            }
            else
            {
                intensity = lights[i].intensity;
            }

            intensity = min(intensity, 1.0);

            if (lights[i].light_type == 4u) //LIGHT_TYPE_HEMISPHERIC
            {
                let dir = normalize(lights[i].dir.xyz);
                let normal_dot_light_dir = dot(normal, dir);

                let light_contrib = clamp(normal_dot_light_dir, -1.0, 1.0) * 0.5 + 0.5;
                let light_color = mix(lights[i].ground_color, lights[i].color, light_contrib);

                color += (light_color * object_color * intensity).rgb;
            }
            else
            {
                // phong light dir
                switch lights[i].light_type
                {
                    case 1u //LIGHT_TYPE_DIRECTIONAL
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

                let specular_color = (lights[i].color * specular * specular_strength).rgb;

                color += (diffuse_color + specular_color) * intensity;
            }
        }

        // ambient occlusion
        if (has_ambient_occlusion_texture())
        {
            let uv = transform_uv(uv, TEXTURE_INDEX_AMBIENT_OCCLUSION);
            let ambient_occlusion = textureSample(tex_ambient_occlusion, tex_ambient_occlusion_sampler, uv);
            color.x *= ambient_occlusion.x;
            color.y *= ambient_occlusion.x;
            color.z *= ambient_occlusion.x;
        }

        // reflection with env map
        if (has_environment_sampler_texture() && material.reflectivity > 0.001)
        {
            var reflectivity = material.reflectivity;
            if (has_reflectivity_texture())
            {
                let uv = transform_uv(uv, TEXTURE_INDEX_REFLECTIVITY);
                let reflectivity_value = textureSample(tex_reflectivity, tex_reflectivity_sampler, uv);
                reflectivity *= reflectivity_value.x;
            }

            var roughness = material.roughness;
            if (has_roughness_texture())
            {
                let uv = transform_uv(uv, TEXTURE_INDEX_ROUGHNESS);
                let roughness_value = textureSample(tex_roughness, tex_roughness_sampler, uv);
                roughness *= roughness_value.x;
            }

            let reflection = reflect(-view_dir, normal);
            let sphere_coords = sphericalCoords(reflection);

            let environment_map_levels = textureNumLevels(tex_environment) - 1u;
            let mipmap_level = roughness * f32(environment_map_levels);

            let sphere_coords_transformed = transform_uv(sphere_coords, TEXTURE_INDEX_ENVIRONMENT);
            let reflection_color = textureSampleLevel(tex_environment, tex_environment_sampler, sphere_coords_transformed, mipmap_level);
            color.x += reflection_color.x * reflectivity;
            color.y += reflection_color.y * reflectivity;
            color.z += reflection_color.z * reflectivity;
        }

        /*
        // IBL
        if (has_environment_sampler_texture())
        {
            let sphere_coords = sphericalCoords(normal);

            let environment_map_levels = textureNumLevels(tex_environment) - 1u;
            //let mipmap_level = roughness * f32(environment_map_levels);
            //let mipmap_level = 0.0;
            let mipmap_level = f32(environment_map_levels) - 2.0;

            let sphere_coords_transformed = transform_uv(sphere_coords, TEXTURE_INDEX_ENVIRONMENT);
            let ibl_color = textureSampleLevel(tex_environment, tex_environment_sampler, sphere_coords_transformed, mipmap_level);
            color.x += ibl_color.x;
            color.y += ibl_color.y;
            color.z += ibl_color.z;
        }
        */
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

    // locked color
    if (in.highlight > 0.0001 && in.locked > 0.0001)
    {
        color = (color * 0.5) + (material.locked_color.rgb * 0.5);
    }
    // highlight color
    else if (in.highlight > 0.0001)
    {
        color = (color * 0.5) + (material.highlight_color.rgb * 0.5);
    }

    // alpha
    var alpha = in.color.a * object_color.a * material.alpha;
    if (has_alpha_texture())
    {
        let uv = transform_uv(uv, TEXTURE_INDEX_ALPHA);
        let tex_color = textureSample(tex_alpha, tex_alpha_sampler, uv);
        alpha *= tex_color.x;
    }

    if (material.blend_mode == 0u) // Opaque
    {
        alpha = 1.0;
    }
    else if (material.blend_mode == 1u) // Mask
    {
        if (alpha <= material.alpha_cutoff)
        {
            discard;
        }
        else
        {
            alpha = 1.0;
        }
    }

    // distance based blending out
    /*
    let max_distance: f32 = 50.0;
    let view_dir = camera.view_pos.xyz - in.position;

    let distance = length(view_dir);
    let dist_scaled = distance / max_distance;
    //let distance_fading_factor = 1.0 - easeInExpo(dist_scaled);
    let distance_fading_factor = 1.0 - easeInQuint(dist_scaled);

    alpha *= distance_fading_factor;
        */

    if (alpha < 0.000001)
    {
        discard;
    }

    //return vec4<f32>(normalize(in.tangent.xyz), alpha);
    return vec4<f32>(color, alpha);
    //return vec4<f32>(1.0, 1.0, 1.0, alpha);
    //return vec4<f32>(object_color.r, object_color.g, object_color.b, alpha);
    //return vec4<f32>(in.weights.r, in.weights.g, in.weights.b, alpha);

    //return textureSample(t_diffuse, s_diffuse, uv);

    //return vec4<f32>(1.0, 0.0, 0.0, 1.0);

    //let res = textureSampleCompare(tex_depth, tex_depth_sampler, in.tex_coords, 0.0);
    //return vec4<f32>(res, res, res, 1.0);
}
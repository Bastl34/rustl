const PI: f32 = 3.141592653589793;

const MAX_LIGHTS = [MAX_LIGHTS];
const MAX_JOINTS = [MAX_JOINTS];
const MAX_MORPH_TARGETS: u32 = [MAX_MORPH_TARGETS]u;

const LIGHT_TYPE_DIRECTIONAL: u32 = 0u;
const LIGHT_TYPE_POINT: u32 = 1u;
const LIGHT_TYPE_SPOT: u32 = 2u;
const JOINTS_LIMIT: u32 = 4u;

// ****************************** structs ******************************

struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    viewport_width: u32,
    viewport_height: u32,
};

struct LightUniform
{
    position: vec4<f32>,
    dir: vec4<f32>,
    color: vec4<f32>,
    ground_color: vec4<f32>,
    intensity: f32,
    range: f32,
    light_type: u32,
    max_angle: f32,
    distance_based_intensity: u32,
};

struct SceneUniform
{
    gamma: f32,
    exposure: f32,
    ibl_diffuse_intensity: f32,
    xray_alpha: f32,
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

    @location(11) object_position: vec3<f32>, // object/local space (pre model matrix) - for object space texture mapping
    @location(12) object_normal: vec3<f32>,   // object/local space geometric normal - for object space texture mapping
    @location(13) model_rotation: vec4<f32>,  // object -> world rotation (quaternion) - for object space mapped normals
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

// converts an orthonormal rotation (given as its 3 column vectors) to a quaternion (x, y, z, w)
// assumes a proper rotation (uniform scale, no reflection) - same assumption as the normal matrix handling
fn rotation_to_quat(c0: vec3<f32>, c1: vec3<f32>, c2: vec3<f32>) -> vec4<f32>
{
    let m00 = c0.x; let m10 = c0.y; let m20 = c0.z;
    let m01 = c1.x; let m11 = c1.y; let m21 = c1.z;
    let m02 = c2.x; let m12 = c2.y; let m22 = c2.z;

    let trace = m00 + m11 + m22;

    var q: vec4<f32>;
    if (trace > 0.0)
    {
        let s = sqrt(trace + 1.0) * 2.0; // s = 4 * w
        q.w = 0.25 * s;
        q.x = (m21 - m12) / s;
        q.y = (m02 - m20) / s;
        q.z = (m10 - m01) / s;
    }
    else if (m00 > m11 && m00 > m22)
    {
        let s = sqrt(1.0 + m00 - m11 - m22) * 2.0; // s = 4 * x
        q.w = (m21 - m12) / s;
        q.x = 0.25 * s;
        q.y = (m01 + m10) / s;
        q.z = (m02 + m20) / s;
    }
    else if (m11 > m22)
    {
        let s = sqrt(1.0 + m11 - m00 - m22) * 2.0; // s = 4 * y
        q.w = (m02 - m20) / s;
        q.x = (m01 + m10) / s;
        q.y = 0.25 * s;
        q.z = (m12 + m21) / s;
    }
    else
    {
        let s = sqrt(1.0 + m22 - m00 - m11) * 2.0; // s = 4 * z
        q.w = (m10 - m01) / s;
        q.x = (m02 + m20) / s;
        q.y = (m12 + m21) / s;
        q.z = 0.25 * s;
    }

    return normalize(q);
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

    // object/local space position and normal (pre model matrix) - used for object space texture mapping
    var object_position = vec4<f32>(0.0);
    var object_normal = vec4<f32>(0.0);

    if (skeleton.joints_amount > 0u)
    {
        for (var i: u32 = 0u; i < JOINTS_LIMIT; i = i + 1u)
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

        // skinned position/normal in object space (before model matrix is applied)
        object_position = world_position;
        object_normal = world_normal;

        world_position = model_matrix * world_position;
    }
    else
    {
        world_position = model_matrix * model_pos;

        world_normal = model_normal;
        world_tangent = model_tangent;
        world_bitangent = model_bitangent;

        object_position = model_pos;
        object_normal = model_normal;
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

    // object space position/normal (raw, not normalized - fragment shader normalizes the normal for the mapping projection)
    out.object_position = object_position.xyz;
    out.object_normal = object_normal.xyz;

    // object -> world rotation (scale removed) as a quaternion - brings object space mapped normals into world space
    out.model_rotation = rotation_to_quat(normalize(model_matrix[0].xyz), normalize(model_matrix[1].xyz), normalize(model_matrix[2].xyz));

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

    ibl_diffuse_intensity: f32,

    allow_xray: u32,

    texture_transforms: array<TextureTransform, TEXTURE_AMOUNT>,
    textures_used: u32,

    mapping_mode: u32,
    mapping_space: u32,
    mapping_axis: u32,
    mapping_scale: f32,
    mapping_sharpness: f32,
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

fn get_uv(in: VertexOutput, texture_index: u32) -> vec2<f32>
{
    let transform = material.texture_transforms[texture_index];
    let uv_index = transform.uv_index;

    // UV 0
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


// ****************************** texture mapping (triplanar / cube / planar / cylindrical / spherical) ******************************

const MAPPING_MODE_UV: u32 = 0u;
const MAPPING_MODE_TRIPLANAR: u32 = 1u;
const MAPPING_MODE_CUBE: u32 = 2u;
const MAPPING_MODE_PLANAR: u32 = 3u;
const MAPPING_MODE_CYLINDRICAL: u32 = 4u;
const MAPPING_MODE_SPHERICAL: u32 = 5u;

const MAPPING_SPACE_OBJECT: u32 = 0u;
const MAPPING_SPACE_WORLD: u32 = 1u;

// position in the chosen projection space (object or world)
fn mapping_position(in: VertexOutput) -> vec3<f32>
{
    if (material.mapping_space == MAPPING_SPACE_WORLD)
    {
        return in.position;
    }
    return in.object_position;
}

// geometric normal in the chosen projection space (object or world)
fn mapping_normal(in: VertexOutput) -> vec3<f32>
{
    if (material.mapping_space == MAPPING_SPACE_WORLD)
    {
        return in.normal;
    }
    return in.object_normal;
}

// swizzles p so that the configured mapping axis becomes the y ("up") axis
fn axis_align(p: vec3<f32>) -> vec3<f32>
{
    if (material.mapping_axis == 0u) { return vec3<f32>(p.z, p.x, p.y); } // x
    if (material.mapping_axis == 2u) { return vec3<f32>(p.x, p.z, p.y); } // z
    return p; // y (default)
}

// inverse of axis_align - brings an axis aligned vector back into the projection space
fn axis_unalign(p: vec3<f32>) -> vec3<f32>
{
    if (material.mapping_axis == 0u) { return vec3<f32>(p.y, p.z, p.x); } // x
    if (material.mapping_axis == 2u) { return vec3<f32>(p.x, p.z, p.y); } // z
    return p; // y (default)
}

// blend weights from a geometric normal (in the chosen projection space)
fn triplanar_blend(n: vec3<f32>, sharpness: f32) -> vec3<f32>
{
    var blend = pow(abs(normalize(n)), vec3<f32>(sharpness));
    let sum = blend.x + blend.y + blend.z;
    return blend / max(sum, 0.0001);
}

// rotates a vector by a (unit) quaternion (x, y, z, w)
fn rotate_by_quat(q: vec4<f32>, v: vec3<f32>) -> vec3<f32>
{
    let u = q.xyz;
    return v + 2.0 * cross(u, cross(u, v) + q.w * v);
}

// linear part (rotation * scale) of the texture transform - used to bring uv gradients into the transformed uv space
fn transform_uv_dir(v: vec2<f32>, texture_index: u32) -> vec2<f32>
{
    let transform = material.texture_transforms[texture_index];
    let scaled = v * transform.scale;
    let c = cos(transform.rotation);
    let s = sin(transform.rotation);
    return vec2<f32>(c * scaled.x - s * scaled.y, s * scaled.x + c * scaled.y);
}

// unscaled projected uv for cube / planar / cylindrical / spherical mapping (p / n in the chosen projection space).
// the angle based u of cylindrical/spherical is in [0,1] - its seam jump is exactly 1 (see sample_projected)
fn projected_uv(p: vec3<f32>, n: vec3<f32>) -> vec2<f32>
{
    if (material.mapping_mode == MAPPING_MODE_CUBE)
    {
        // dominant axis of the normal picks the projection plane (like triplanar without blending)
        let an = abs(n);
        if (an.x >= an.y && an.x >= an.z) { return p.zy; }
        if (an.y >= an.z) { return p.xz; }
        return p.xy;
    }

    let pa = axis_align(p);

    if (material.mapping_mode == MAPPING_MODE_CYLINDRICAL)
    {
        let u = atan2(pa.x, pa.z) / (2.0 * PI) + 0.5;
        return vec2<f32>(u, pa.y);
    }

    if (material.mapping_mode == MAPPING_MODE_SPHERICAL)
    {
        let dir = normalize(pa);
        let u = atan2(dir.x, dir.z) / (2.0 * PI) + 0.5;
        let v = acos(clamp(dir.y, -1.0, 1.0)) / PI;
        return vec2<f32>(u, v);
    }

    // planar
    return pa.xz;
}

// samples a projected uv with explicit gradients: the angle based uvs jump at the atan2 seam which
// would break the mip level selection there - wrapping the gradients removes the seam line.
// mapping scale and the per texture 2d transform are applied on top of the projected uv.
fn sample_projected(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>, texture_index: u32) -> vec4<f32>
{
    let scale = material.mapping_scale;

    var gx = dpdx(uv);
    var gy = dpdy(uv);
    gx.x = gx.x - round(gx.x);
    gy.x = gy.x - round(gy.x);

    return textureSampleGrad(tex, samp, transform_uv(uv * scale, texture_index), transform_uv_dir(gx * scale, texture_index), transform_uv_dir(gy * scale, texture_index));
}

// triplanar color sample (p / n in the chosen projection space - world or object)
fn sample_triplanar(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, n: vec3<f32>, scale: f32, sharpness: f32, texture_index: u32) -> vec4<f32>
{
    let blend = triplanar_blend(n, sharpness);

    let cx = textureSample(tex, samp, transform_uv(p.zy * scale, texture_index));
    let cy = textureSample(tex, samp, transform_uv(p.xz * scale, texture_index));
    let cz = textureSample(tex, samp, transform_uv(p.xy * scale, texture_index));

    return cx * blend.x + cy * blend.y + cz * blend.z;
}

// samples a material texture with the active mapping mode (uv / triplanar / cube / planar / cylindrical / spherical)
fn sample_material_texture(tex: texture_2d<f32>, samp: sampler, in: VertexOutput, texture_index: u32) -> vec4<f32>
{
    if (material.mapping_mode == MAPPING_MODE_TRIPLANAR)
    {
        return sample_triplanar(tex, samp, mapping_position(in), mapping_normal(in), material.mapping_scale, material.mapping_sharpness, texture_index);
    }
    if (material.mapping_mode != MAPPING_MODE_UV)
    {
        return sample_projected(tex, samp, projected_uv(mapping_position(in), mapping_normal(in)), texture_index);
    }

    return textureSample(tex, samp, get_uv(in, texture_index));
}

// samples a tangent space normal at the given (already scaled) plane uv, applying the texture transform;
// the sampled xy is rotated back by the transform rotation so the normal stays aligned with the plane axes
fn sample_plane_normal(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>, strength: f32, texture_index: u32) -> vec3<f32>
{
    var tn = textureSample(tex, samp, transform_uv(uv, texture_index)).xyz * 2.0 - 1.0;
    tn = vec3<f32>(tn.xy * strength, tn.z);

    let rot = material.texture_transforms[texture_index].rotation;
    let c = cos(rot);
    let s = sin(rot);
    return vec3<f32>(c * tn.x + s * tn.y, -s * tn.x + c * tn.y, tn.z);
}

// like sample_plane_normal but for an unscaled projected uv with seam wrapped gradients (see sample_projected)
fn sample_projected_normal(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>, strength: f32, texture_index: u32) -> vec3<f32>
{
    let scale = material.mapping_scale;

    var gx = dpdx(uv);
    var gy = dpdy(uv);
    gx.x = gx.x - round(gx.x);
    gy.x = gy.x - round(gy.x);

    var tn = textureSampleGrad(tex, samp, transform_uv(uv * scale, texture_index), transform_uv_dir(gx * scale, texture_index), transform_uv_dir(gy * scale, texture_index)).xyz * 2.0 - 1.0;
    tn = vec3<f32>(tn.xy * strength, tn.z);

    let rot = material.texture_transforms[texture_index].rotation;
    let c = cos(rot);
    let s = sin(rot);
    return vec3<f32>(c * tn.x + s * tn.y, -s * tn.x + c * tn.y, tn.z);
}

// triplanar normal mapping (whiteout blend). p / n in the chosen projection space; result is a normal in that same space.
fn sample_triplanar_normal(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, n: vec3<f32>, scale: f32, sharpness: f32, strength: f32, texture_index: u32) -> vec3<f32>
{
    let gn = normalize(n);
    let blend = triplanar_blend(gn, sharpness);

    var tnx = sample_plane_normal(tex, samp, p.zy * scale, strength, texture_index);
    var tny = sample_plane_normal(tex, samp, p.xz * scale, strength, texture_index);
    var tnz = sample_plane_normal(tex, samp, p.xy * scale, strength, texture_index);

    // whiteout blend - reorient each projected normal by the geometric normal
    tnx = vec3<f32>(tnx.xy + gn.zy, gn.x);
    tny = vec3<f32>(tny.xy + gn.xz, gn.y);
    tnz = vec3<f32>(tnz.xy + gn.xy, gn.z);

    // swizzle to world/object orientation and blend
    let result = tnx.zyx * blend.x + tny.xzy * blend.y + tnz.xyz * blend.z;
    return normalize(result);
}

// cube mapping normal - hard pick of the dominant plane, whiteout reorientation like triplanar (no blend).
// p / n in the chosen projection space; result is a normal in that same space.
fn sample_cube_normal(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, n: vec3<f32>, strength: f32, texture_index: u32) -> vec3<f32>
{
    let gn = normalize(n);
    let an = abs(gn);

    let tn = sample_projected_normal(tex, samp, projected_uv(p, gn), strength, texture_index);

    if (an.x >= an.y && an.x >= an.z)
    {
        return normalize(vec3<f32>(tn.xy + gn.zy, gn.x).zyx);
    }
    if (an.y >= an.z)
    {
        return normalize(vec3<f32>(tn.xy + gn.xz, gn.y).xzy);
    }
    return normalize(vec3<f32>(tn.xy + gn.xy, gn.z));
}

// planar mapping normal - fixed projection plane selected by the mapping axis (whiteout reorientation).
// p / n in the chosen projection space; result is a normal in that same space.
fn sample_planar_normal(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, n: vec3<f32>, strength: f32, texture_index: u32) -> vec3<f32>
{
    let gn = normalize(n);
    let tn = sample_projected_normal(tex, samp, projected_uv(p, gn), strength, texture_index);

    if (material.mapping_axis == 0u) // x
    {
        return normalize(vec3<f32>(tn.xy + gn.zy, gn.x).zyx);
    }
    if (material.mapping_axis == 2u) // z
    {
        return normalize(vec3<f32>(tn.xy + gn.xy, gn.z));
    }
    // y (default)
    return normalize(vec3<f32>(tn.xy + gn.xz, gn.y).xzy);
}

// cylindrical / spherical normal mapping - analytic tangent frame from the parametrization.
// p / n in the chosen projection space; result is a normal in that same space.
fn sample_wrapped_normal(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, n: vec3<f32>, strength: f32, texture_index: u32) -> vec3<f32>
{
    let gn = normalize(n);
    let tn = sample_projected_normal(tex, samp, projected_uv(p, gn), strength, texture_index);

    let pa = axis_align(p);

    // tangent along the angle (u) direction
    let flat_len = max(length(pa.xz), 0.0001);
    let sin_theta = pa.x / flat_len;
    let cos_theta = pa.z / flat_len;
    let tangent = axis_unalign(vec3<f32>(cos_theta, 0.0, -sin_theta));

    // bitangent along the v direction
    var bitangent_aligned = vec3<f32>(0.0, 1.0, 0.0); // cylindrical: v runs along the axis
    if (material.mapping_mode == MAPPING_MODE_SPHERICAL)
    {
        // spherical: v runs from the +axis pole to the -axis pole
        let dir = normalize(pa);
        let sin_phi = max(sqrt(max(1.0 - dir.y * dir.y, 0.0)), 0.0001);
        bitangent_aligned = normalize(vec3<f32>(dir.y * sin_theta, -sin_phi, dir.y * cos_theta));
    }
    let bitangent = axis_unalign(bitangent_aligned);

    return normalize(tangent * tn.x + bitangent * tn.y + gn * tn.z);
}


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    // base color
    var object_color = material.base_color * in.color;
    if (has_base_texture())
    {
        let tex_color = sample_material_texture(tex_base, tex_base_sampler, in, TEXTURE_INDEX_BASE);
        object_color *= tex_color;
    }

    // ambient color
    var ambient_color = material.ambient_color;
    if (has_ambient_texture())
    {
        let tex_color = sample_material_texture(tex_ambient, tex_ambient_sampler, in, TEXTURE_INDEX_AMBIENT);
        ambient_color *= tex_color;
    }

    // normal
    var normal = in.normal;
    var tangent = in.tangent;
    var bitangent = in.bitangent;

    // normal mapping
    if (has_normal_texture())
    {
        if (material.mapping_mode != MAPPING_MODE_UV) // projected normal mapping
        {
            let p = mapping_position(in);
            let n = mapping_normal(in);

            var mapped_normal: vec3<f32>;
            if (material.mapping_mode == MAPPING_MODE_TRIPLANAR)
            {
                mapped_normal = sample_triplanar_normal(tex_normal, tex_normal_Sampler, p, n, material.mapping_scale, material.mapping_sharpness, material.normal_map_strength, TEXTURE_INDEX_NORMAL);
            }
            else if (material.mapping_mode == MAPPING_MODE_CUBE)
            {
                mapped_normal = sample_cube_normal(tex_normal, tex_normal_Sampler, p, n, material.normal_map_strength, TEXTURE_INDEX_NORMAL);
            }
            else if (material.mapping_mode == MAPPING_MODE_PLANAR)
            {
                mapped_normal = sample_planar_normal(tex_normal, tex_normal_Sampler, p, n, material.normal_map_strength, TEXTURE_INDEX_NORMAL);
            }
            else // cylindrical / spherical
            {
                mapped_normal = sample_wrapped_normal(tex_normal, tex_normal_Sampler, p, n, material.normal_map_strength, TEXTURE_INDEX_NORMAL);
            }

            if (material.mapping_space == MAPPING_SPACE_OBJECT)
            {
                // bring the object space normal into world space (handles object rotation incl. twist) for correct lighting
                mapped_normal = rotate_by_quat(normalize(in.model_rotation), mapped_normal);
            }

            normal = normalize(mapped_normal);
        }
        else // UV normal mapping
        {
            let uv = get_uv(in, TEXTURE_INDEX_NORMAL);
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
            let tex_color = sample_material_texture(tex_specular, tex_specular_sampler, in, TEXTURE_INDEX_SPECULAR);
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

            // range attenuation (point and spot lights only, glTF punctual lights spec)
            // formula: max(min(1 - (d/range)^4, 1), 0) -- smooth fade to zero at range boundary
            // NOTE: intentionally NO division by d² here, even though the glTF spec includes it.
            // Reason: the distance-based falloff (1/d) is already handled above via distance_based_intensity.
            // Adding /d² here would double-attenuate the intensity and make everything too dark.
            // range == 0 means infinite range (e.g. loaded from glTF where range was undefined/None).
            let range = lights[i].range;
            if range > 0.0 && (lights[i].light_type == 2u || lights[i].light_type == 3u)
            {
                let current_distance = length(lights[i].position.xyz - in.position);
                if current_distance >= range
                {
                    continue; // hard cutoff -- no contribution beyond range (glTF spec requirement)
                }
                let attenuation = max(min(1.0 - pow(current_distance / range, 4.0), 1.0), 0.0);
                intensity *= attenuation;
            }

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
            let ambient_occlusion = sample_material_texture(tex_ambient_occlusion, tex_ambient_occlusion_sampler, in, TEXTURE_INDEX_AMBIENT_OCCLUSION);
            color.x *= ambient_occlusion.x;
            color.y *= ambient_occlusion.x;
            color.z *= ambient_occlusion.x;
        }

        // reflection with env map (specular IBL)
        if (has_environment_sampler_texture() && material.reflectivity > 0.001)
        {
            var reflectivity = material.reflectivity;
            if (has_reflectivity_texture())
            {
                let reflectivity_value = sample_material_texture(tex_reflectivity, tex_reflectivity_sampler, in, TEXTURE_INDEX_REFLECTIVITY);
                reflectivity *= reflectivity_value.x;
            }

            var roughness = material.roughness;
            if (has_roughness_texture())
            {
                let roughness_value = sample_material_texture(tex_roughness, tex_roughness_sampler, in, TEXTURE_INDEX_ROUGHNESS);
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

        // Diffuse IBL (Irradiance)
        let ibl_diffuse_intensity = material.ibl_diffuse_intensity * scene.ibl_diffuse_intensity;
        if (has_environment_sampler_texture() && ibl_diffuse_intensity > 0.001)
        {
            let sphere_coords = sphericalCoords(normal);
            let environment_map_levels = textureNumLevels(tex_environment) - 1u;

            // use big mipmap level for diffuse IBL
            let mipmap_level = f32(environment_map_levels) - 2.0;
            let sphere_coords_transformed = transform_uv(sphere_coords, TEXTURE_INDEX_ENVIRONMENT);
            let ibl_color = textureSampleLevel(tex_environment, tex_environment_sampler, sphere_coords_transformed, mipmap_level);

            color += ibl_color.rgb * object_color.rgb * ibl_diffuse_intensity;
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
        let tex_color = sample_material_texture(tex_alpha, tex_alpha_sampler, in, TEXTURE_INDEX_ALPHA);
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
    let max_distance: f32 = 100.0;
    let view_dir = camera.view_pos.xyz - in.position;

    let distance = length(view_dir);
    let dist_scaled = distance / max_distance;
    //let distance_fading_factor = 1.0 - easeInExpo(dist_scaled);
    let distance_fading_factor = 1.0 - easeInQuint(dist_scaled);

    alpha *= distance_fading_factor;
    */

    // x-ray mode: clamp alpha to xray_alpha so opaque objects become see-through
    // materials with allow_xray=0 (gizmos, grid, etc.) are excluded
    if (material.allow_xray != 0u)
    {
        alpha = min(alpha, scene.xray_alpha);
    }

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
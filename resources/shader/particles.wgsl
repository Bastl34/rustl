// particles as quads - one instance per particle, the corners come from the vertex index

// keep in sync with rendering/particles.rs
const FLAG_TEXTURED: u32 = 1u;
const FLAG_FOG: u32 = 2u;
const FLAG_ADDITIVE: u32 = 4u;
const FLAG_LIT: u32 = 8u;
const FLAG_SHADOWED: u32 = 16u;
const ALIGN_SHIFT: u32 = 4u;

const PI: f32 = 3.141592653589793;
const MAX_LIGHTS = [MAX_LIGHTS];
const MAX_SHADOW_VIEWS = [MAX_SHADOW_VIEWS];

// lit particles: a quad has no real surface - lit from the front (the light behind the camera) up to this much more than from behind
const BACK_LIT: f32 = 0.45;
// the hemispheric light: share of the sky color (the rest is the ground's)
const SKY_SHARE: f32 = 0.6;
// m toward the light the shadow is looked up from (dust lies on the ground it would shadow itself with)
const SHADOW_LIFT: f32 = 0.2;

const ALIGN_VELOCITY: u32 = 1u;
const ALIGN_NODE: u32 = 2u;

// the depth pre-pass can be read (not on WebGL) - replaced when the shader is loaded
const SOFT_PARTICLES: bool = [SOFT_PARTICLES];

struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    viewport_width: u32,
    viewport_height: u32,
};

// like the fog of the material shader
struct FogUniform
{
    color: vec4<f32>,  // rgb, density
    params: vec4<f32>, // start, end
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> fog: FogUniform;

// m22, m23, m32, m33 of the projection: the depth back to view space
struct DepthUniform
{
    projection: vec4<f32>,
};

@group(0) @binding(2) var<uniform> depth_params: DepthUniform;
@group(0) @binding(3) var t_scene_depth: texture_depth_2d;

@group(1) @binding(0) var t_particle: texture_2d<f32>;
@group(1) @binding(1) var s_particle: sampler;

// ****************************** lighting (as in base.wgsl) ******************************

struct SceneUniform
{
    gamma: f32,
    exposure: f32,
    ibl_diffuse_intensity: f32,
    xray_alpha: f32,
    shadow_max_distance: f32,
    ssao_strength: f32,
    fog_density: f32,
    fog_start: f32,
    fog_color: vec4<f32>,
    fog_end: f32,
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
    shadow_index: i32,
    shadow_views: u32,
    shadow_bias: f32,
    shadow_strength: f32,
    softness: f32,
};

struct ShadowView
{
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
};

@group(2) @binding(0) var<uniform> scene: SceneUniform;
@group(2) @binding(1) var<uniform> light_amount: i32;
@group(2) @binding(2) var<uniform> lights: array<LightUniform, MAX_LIGHTS>;
@group(2) @binding(3) var<uniform> shadow_views: array<ShadowView, MAX_SHADOW_VIEWS>;
@group(2) @binding(4) var t_shadow: texture_depth_2d_array;
@group(2) @binding(5) var s_shadow: sampler_comparison;

// one shadow atlas layer, 4 taps (the comparison sampler filters each 2x2): 1 lit, 0 shadowed
fn shadow_sample_view(view_index: u32, world_pos: vec3<f32>, bias: f32) -> f32
{
    let clip = shadow_views[view_index].view_proj * vec4<f32>(world_pos, 1.0);
    if (clip.w <= 0.0)
    {
        return 1.0;
    }

    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 1.0 - (ndc.y * 0.5 + 0.5));
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z <= 0.0 || ndc.z >= 1.0)
    {
        return 1.0;
    }

    let texel = 1.0 / f32(textureDimensions(t_shadow).x);
    let depth_ref = ndc.z - bias;
    var shadow = 0.0;
    shadow += textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(-texel, -texel), view_index, depth_ref);
    shadow += textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(texel, -texel), view_index, depth_ref);
    shadow += textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(-texel, texel), view_index, depth_ref);
    shadow += textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(texel, texel), view_index, depth_ref);
    return shadow * 0.25;
}

// the shadow of one light at a position: cascades of a directional light, the cube face of a point light, a spot light's view
fn shadow_factor(light_index: i32, world_pos: vec3<f32>) -> f32
{
    let shadow_index = lights[light_index].shadow_index;
    if (shadow_index < 0)
    {
        return 1.0;
    }

    let first_view = u32(shadow_index);
    let bias = lights[light_index].shadow_bias;

    switch lights[light_index].light_type
    {
        case 1u // directional
        {
            var fade = 0.0;
            if (scene.shadow_max_distance > 0.0)
            {
                let fade_start = scene.shadow_max_distance * 0.85;
                let view_distance = length(camera.view_pos.xyz - world_pos);
                fade = clamp((view_distance - fade_start) / max(scene.shadow_max_distance - fade_start, 0.0001), 0.0, 1.0);
            }
            if (fade >= 1.0)
            {
                return 1.0;
            }

            for (var cascade = 0u; cascade < lights[light_index].shadow_views; cascade = cascade + 1u)
            {
                let clip = shadow_views[first_view + cascade].view_proj * vec4<f32>(world_pos, 1.0);
                if (clip.w <= 0.0)
                {
                    continue;
                }
                let ndc = clip.xyz / clip.w;
                if (abs(ndc.x) < 0.99 && abs(ndc.y) < 0.99 && ndc.z > 0.0 && ndc.z < 1.0)
                {
                    return mix(shadow_sample_view(first_view + cascade, world_pos, bias), 1.0, fade);
                }
            }
            return 1.0;
        }
        case 2u // point
        {
            let to_particle = world_pos - lights[light_index].position.xyz;
            let a = abs(to_particle);
            var face = 0u;
            if (a.x >= a.y && a.x >= a.z) { face = select(1u, 0u, to_particle.x > 0.0); }
            else if (a.y >= a.z) { face = select(3u, 2u, to_particle.y > 0.0); }
            else { face = select(5u, 4u, to_particle.z > 0.0); }
            return shadow_sample_view(first_view + face, world_pos, bias);
        }
        case 3u // spot
        {
            return shadow_sample_view(first_view, world_pos, bias);
        }
        default
        {
            return 1.0;
        }
    }
}

// the light that reaches a particle (its center): like a lit surface, but without a normal - the hemispheric light mixed,
// the others brighter from the front than from behind
fn particle_light(p: vec3<f32>, shadowed: bool) -> vec3<f32>
{
    if (light_amount == 0)
    {
        return vec3<f32>(1.0);
    }

    let to_camera = normalize(camera.view_pos.xyz - p);
    var sum = vec3<f32>(0.0);

    for (var i = 0; i < min(light_amount, MAX_LIGHTS); i += 1)
    {
        let light_type = lights[i].light_type;
        if (light_type == 0u)
        {
            continue;
        }

        var to_light = lights[i].position.xyz - p;
        let distance = length(to_light);

        var intensity = lights[i].intensity;
        if (lights[i].distance_based_intensity == 1u && (light_type == 2u || light_type == 3u))
        {
            intensity = lights[i].intensity / (4.0 * PI * max(distance, 0.0001));
        }
        if (light_type == 3u)
        {
            let angle = acos(clamp(dot(-to_light / max(distance, 0.0001), lights[i].dir.xyz), -1.0, 1.0));
            let soft = clamp(lights[i].softness, 0.0, 1.0);
            if (soft > 0.001)
            {
                intensity *= 1.0 - smoothstep(lights[i].max_angle * (1.0 - soft), lights[i].max_angle, angle);
            }
            else if (angle > lights[i].max_angle)
            {
                intensity = 0.0;
            }
        }
        intensity = min(intensity, 1.0);

        let range = lights[i].range;
        if (range > 0.0 && (light_type == 2u || light_type == 3u))
        {
            if (distance >= range)
            {
                continue;
            }
            intensity *= max(min(1.0 - pow(distance / range, 4.0), 1.0), 0.0);
        }

        if (intensity <= 0.0)
        {
            continue;
        }

        if (light_type == 4u)
        {
            sum += mix(lights[i].ground_color.rgb, lights[i].color.rgb, SKY_SHARE) * intensity;
            continue;
        }

        if (light_type == 1u)
        {
            to_light = -lights[i].dir.xyz;
        }
        let direction = normalize(to_light);
        let facing = mix(BACK_LIT, 1.0, dot(to_camera, direction) * 0.5 + 0.5);

        var shadow = 1.0;
        if (shadowed && lights[i].shadow_index >= 0)
        {
            shadow = mix(1.0, shadow_factor(i, p + direction * SHADOW_LIFT), lights[i].shadow_strength);
        }

        sum += lights[i].color.rgb * intensity * facing * shadow;
    }

    return sum;
}

struct ParticleInput
{
    @location(0) position: vec3<f32>,  // world space
    @location(1) size: f32,            // width of the quad
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,        // rad, around the quad normal
    @location(4) frame: u32,           // tile of the flipbook, row by row
    @location(5) flipbook: u32,        // columns | rows << 16
    @location(6) flags: u32,
    @location(7) direction: vec3<f32>, // velocity (align velocity) or plane normal (align node)
    @location(8) stretch: f32,         // s of the way drawn behind (align velocity)
    @location(9) softness: f32,        // m, 0: off
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) flags: u32,
    @location(4) fog_factor: f32,
    @location(5) softness: f32,
    @location(6) @interpolate(flat) light: vec3<f32>, // lit: the light at the particle (the same at every corner)
};

fn fog_factor(distance: f32) -> f32
{
    let density = fog.color.w;
    let start = fog.params.x;
    let end = fog.params.y;

    if (end > start + 0.0001)
    {
        return clamp((distance - start) / (end - start), 0.0, 1.0);
    }

    if (density > 0.0001)
    {
        return 1.0 - exp(-density * distance);
    }

    return 0.0;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, particle: ParticleInput) -> VertexOutput
{
    var corners = array<vec2<f32>, 6>
    (
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vertex_index];

    let c = cos(particle.rotation);
    let s = sin(particle.rotation);
    var turned = vec2<f32>(corner.x * c - corner.y * s, corner.x * s + corner.y * c);

    // billboard: the rows of the view matrix are the camera axes in world space
    var axis_x = vec3<f32>(camera.view[0].x, camera.view[1].x, camera.view[2].x);
    var axis_y = vec3<f32>(camera.view[0].y, camera.view[1].y, camera.view[2].y);
    var half_size = vec2<f32>(particle.size * 0.5, particle.size * 0.5);
    var center = particle.position;

    let align = (particle.flags >> ALIGN_SHIFT) & 3u;

    if (align == ALIGN_VELOCITY)
    {
        // long side along the way, turned around it to the camera - the stretch lies behind the particle
        let speed = length(particle.direction);
        let way = particle.direction / max(speed, 0.0001);
        let side = cross(way, camera.view_pos.xyz - particle.position);

        if (speed > 0.0001 && length(side) > 0.0001)
        {
            axis_x = normalize(side);
            axis_y = way;
            half_size.y = half_size.y + speed * particle.stretch * 0.5;
            center = particle.position - way * speed * particle.stretch * 0.5;
            turned = corner;
        }
    }
    else if (align == ALIGN_NODE)
    {
        let normal = normalize(particle.direction);
        let reference = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(normal.x) > 0.9);
        axis_x = normalize(cross(reference, normal));
        axis_y = cross(normal, axis_x);
    }

    let world = center + axis_x * turned.x * half_size.x + axis_y * turned.y * half_size.y;

    let columns = max(particle.flipbook & 0xffffu, 1u);
    let rows = max(particle.flipbook >> 16u, 1u);
    let cell = vec2<f32>(f32(particle.frame % columns), f32(particle.frame / columns));
    let uv = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
    out.uv = (cell + uv) / vec2<f32>(f32(columns), f32(rows));
    out.local = corner;
    out.color = particle.color;
    out.flags = particle.flags;
    out.fog_factor = select(0.0, fog_factor(length(camera.view_pos.xyz - world)), (particle.flags & FLAG_FOG) != 0u);
    out.softness = particle.softness;
    out.light = vec3<f32>(1.0);
    if ((particle.flags & FLAG_LIT) != 0u)
    {
        out.light = particle_light(particle.position, (particle.flags & FLAG_SHADOWED) != 0u);
    }

    return out;
}

// distance in front of the camera of a depth buffer value
fn view_distance(depth: f32) -> f32
{
    let p = depth_params.projection;
    return abs((p.y - depth * p.w) / (depth * p.z - p.x));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    // sampled outside of the branch (uniform control flow)
    let texel = textureSample(t_particle, s_particle, in.uv);

    // without a texture a soft round dot
    let dot = 1.0 - smoothstep(0.4, 1.0, length(in.local));
    let shape = select(vec4<f32>(1.0, 1.0, 1.0, dot), texel, (in.flags & FLAG_TEXTURED) != 0u);

    var color = in.color * shape;
    color = vec4<f32>(color.rgb * in.light, color.a);

    // soft particles: fade out close to the scene behind (both blend modes scale by the alpha)
    if (SOFT_PARTICLES && in.softness > 0.0)
    {
        let scene_depth = textureLoad(t_scene_depth, vec2<i32>(in.clip_position.xy), 0);
        let gap = view_distance(scene_depth) - view_distance(in.clip_position.z);
        color.a = color.a * clamp(gap / in.softness, 0.0, 1.0);
    }

    if (color.a <= 0.001)
    {
        discard;
    }

    // additive fades out instead of adding the fog color
    if ((in.flags & FLAG_ADDITIVE) != 0u)
    {
        color = vec4<f32>(color.rgb * (1.0 - in.fog_factor), color.a);
    }
    else
    {
        color = vec4<f32>(mix(color.rgb, fog.color.rgb, in.fog_factor), color.a);
    }

    // lit particles: the tone mapping and gamma of the lit surfaces
    if ((in.flags & FLAG_LIT) != 0u)
    {
        var rgb = color.rgb;
        if (scene.exposure > 0.0001)
        {
            rgb = vec3<f32>(1.0) - exp(-rgb * scene.exposure);
        }
        if (scene.gamma > 0.0001)
        {
            rgb = pow(rgb, vec3<f32>(1.0 / scene.gamma));
        }
        color = vec4<f32>(rgb, color.a);
    }

    return color;
}

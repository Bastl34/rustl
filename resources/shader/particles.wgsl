// particles as quads - one instance per particle, the corners come from the vertex index

// keep in sync with rendering/particles.rs
const FLAG_TEXTURED: u32 = 1u;
const FLAG_FOG: u32 = 2u;
const FLAG_ADDITIVE: u32 = 4u;
const ALIGN_SHIFT: u32 = 4u;

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

    return color;
}

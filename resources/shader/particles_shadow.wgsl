// particles of emitters with cast_shadows into the shadow atlas (inside the shadow pass) - quads turned to the light, alpha tested

// keep in sync with rendering/particles.rs and particles.wgsl
const FLAG_TEXTURED: u32 = 1u;
const ALIGN_SHIFT: u32 = 4u;
const ALIGN_VELOCITY: u32 = 1u;
const ALIGN_NODE: u32 = 2u;

// below this alpha no shadow
const ALPHA_CUTOFF: f32 = 0.35;

// the light matrix of the shadow view (dynamic offset, like shadow.wgsl)
struct ShadowViewUniform
{
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> shadow_view: ShadowViewUniform;

@group(1) @binding(0) var t_particle: texture_2d<f32>;
@group(1) @binding(1) var s_particle: sampler;

struct ParticleInput
{
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
    @location(4) frame: u32,
    @location(5) flipbook: u32,
    @location(6) flags: u32,
    @location(7) direction: vec3<f32>,
    @location(8) stretch: f32,
    @location(9) softness: f32,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) alpha: f32,
    @location(3) @interpolate(flat) flags: u32,
    @location(4) color: vec3<f32>,
};

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

    // the first two rows of the light matrix point along its right and up axes (symmetric projections)
    let vp = shadow_view.view_proj;
    var axis_x = normalize(vec3<f32>(vp[0].x, vp[1].x, vp[2].x));
    var axis_y = normalize(vec3<f32>(vp[0].y, vp[1].y, vp[2].y));
    var half_size = vec2<f32>(particle.size * 0.5, particle.size * 0.5);
    var center = particle.position;

    let align = (particle.flags >> ALIGN_SHIFT) & 3u;

    if (align == ALIGN_VELOCITY)
    {
        let speed = length(particle.direction);
        let way = particle.direction / max(speed, 0.0001);
        let side = cross(way, cross(axis_x, axis_y));

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
    out.clip_position = vp * vec4<f32>(world, 1.0);
    out.uv = (cell + uv) / vec2<f32>(f32(columns), f32(rows));
    out.local = corner;
    out.alpha = particle.color.a;
    out.color = particle.color.rgb;
    out.flags = particle.flags;

    return out;
}

// translucent shadows: rgb multiplies down the light that passes, a keeps the nearest depth (min blend)
// grey only darkens, the hue of a colored particle passes through (like stained glass)
@fragment
fn fs_translucent(in: VertexOutput) -> @location(0) vec4<f32>
{
    let texel = textureSample(t_particle, s_particle, in.uv);

    let dot = 1.0 - smoothstep(0.4, 1.0, length(in.local));
    let shape_alpha = select(dot, texel.a, (in.flags & FLAG_TEXTURED) != 0u);
    let opacity = clamp(in.alpha * shape_alpha, 0.0, 1.0);

    if (opacity <= 0.001)
    {
        discard;
    }

    let color = in.color * select(vec3<f32>(1.0), texel.rgb, (in.flags & FLAG_TEXTURED) != 0u);
    let chroma = color / max(max(color.r, color.g), max(color.b, 0.0001));
    let hue = chroma - vec3<f32>(min(min(chroma.r, chroma.g), chroma.b));

    return vec4<f32>(vec3<f32>(1.0) - opacity * (vec3<f32>(1.0) - hue), in.clip_position.z);
}

@fragment
fn fs_main(in: VertexOutput)
{
    // sampled outside of the branch (uniform control flow)
    let texel = textureSample(t_particle, s_particle, in.uv);

    let dot = 1.0 - smoothstep(0.4, 1.0, length(in.local));
    let shape_alpha = select(dot, texel.a, (in.flags & FLAG_TEXTURED) != 0u);

    if (in.alpha * shape_alpha < ALPHA_CUTOFF)
    {
        discard;
    }
}

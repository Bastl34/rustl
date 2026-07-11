// debug rendering of the culling bounding volumes (boxes and spheres) as lines
// all vertices are generated from the vertex/instance index (vertex pulling)
// -> no vertex buffers needed (one instance per volume)
//
// line width is not supported by WebGPU (always 1px) -> every line segment is
// extruded into a screen space quad (2 triangles) in the vertex shader instead

const LINE_WIDTH_PX: f32 = 2.0;

const SPHERE_SEGMENTS: u32 = 48u; // lines per circle (keep in sync with debug_volumes.rs)

const BOX_COLOR: vec4<f32> = vec4<f32>(1.0, 0.6, 0.1, 1.0);
const SPHERE_COLOR: vec4<f32> = vec4<f32>(0.2, 0.8, 1.0, 1.0);

const PI: f32 = 3.14159265359;

struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    viewport_width: u32,
    viewport_height: u32,
};

struct DebugVolume
{
    min: vec4<f32>,           // xyz = world space aabb min
    max: vec4<f32>,           // xyz = world space aabb max
    center_radius: vec4<f32>, // xyz = sphere center, w = sphere radius
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<storage, read> volumes: array<DebugVolume>;

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// extrudes the line p0-p1 (world space) into a screen space quad and
// returns the quad vertex for quad_index (two triangles: 0,1,2 / 2,1,3)
fn line_vertex(p0_world: vec3<f32>, p1_world: vec3<f32>, quad_index: u32, color: vec4<f32>) -> VertexOutput
{
    var out: VertexOutput;
    out.color = color;

    var p0 = camera.view_proj * vec4<f32>(p0_world, 1.0);
    var p1 = camera.view_proj * vec4<f32>(p1_world, 1.0);

    // clip the segment against the near plane - the projection flips behind the camera
    // (happens all the time: the camera is often inside a bounding volume)
    let near_eps = 0.0001;
    if (p0.w < near_eps && p1.w < near_eps)
    {
        out.clip_position = vec4<f32>(0.0, 0.0, 2.0, 1.0); // fully behind: degenerate -> clipped
        return out;
    }
    if (p0.w < near_eps)
    {
        p0 = mix(p0, p1, (near_eps - p0.w) / (p1.w - p0.w));
    }
    else if (p1.w < near_eps)
    {
        p1 = mix(p1, p0, (near_eps - p1.w) / (p0.w - p1.w));
    }

    let viewport = vec2<f32>(f32(camera.viewport_width), f32(camera.viewport_height));

    // screen space direction of the line
    var dir = (p1.xy / p1.w - p0.xy / p0.w) * viewport;
    let len = length(dir);
    if (len < 0.0001) { dir = vec2<f32>(1.0, 0.0); }
    else { dir = dir / len; }

    let normal = vec2<f32>(-dir.y, dir.x);

    // quad corners: 0/1 = p0 +/- normal, 2/3 = p1 +/- normal
    var corners = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = corners[quad_index];

    let p = select(p0, p1, (corner & 2u) != 0u);
    let side = select(1.0, -1.0, (corner & 1u) != 0u);

    // pixel offset in ndc units (* p.w so it survives the perspective divide)
    let offset = normal * (LINE_WIDTH_PX / viewport) * side * p.w; // 0.5 * width * (2.0 / viewport)

    out.clip_position = vec4<f32>(p.xy + offset, p.z, p.w);

    return out;
}

// corner bits: 1 = x max, 2 = z max, 4 = y max
fn box_corner(volume_index: u32, corner: u32) -> vec3<f32>
{
    let b_min = volumes[volume_index].min.xyz;
    let b_max = volumes[volume_index].max.xyz;

    return vec3<f32>
    (
        select(b_min.x, b_max.x, (corner & 1u) != 0u),
        select(b_min.y, b_max.y, (corner & 4u) != 0u),
        select(b_min.z, b_max.z, (corner & 2u) != 0u)
    );
}

// 72 vertices per box: 12 edges as quads (2 triangles each)
@vertex
fn vs_box(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    // the 12 edges as pairs of corner indices (see box_corner)
    var edges = array<vec2<u32>, 12>
    (
        vec2<u32>(0u, 1u), vec2<u32>(1u, 3u), vec2<u32>(3u, 2u), vec2<u32>(2u, 0u), // bottom (y min)
        vec2<u32>(4u, 5u), vec2<u32>(5u, 7u), vec2<u32>(7u, 6u), vec2<u32>(6u, 4u), // top (y max)
        vec2<u32>(0u, 4u), vec2<u32>(1u, 5u), vec2<u32>(3u, 7u), vec2<u32>(2u, 6u)  // sides
    );

    let edge = edges[vertex_index / 6u];

    let p0 = box_corner(instance_index, edge.x);
    let p1 = box_corner(instance_index, edge.y);

    return line_vertex(p0, p1, vertex_index % 6u, BOX_COLOR);
}

// 3 great circles (x-z, x-y, y-z plane) with SPHERE_SEGMENTS quads each
@vertex
fn vs_sphere(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let center = volumes[instance_index].center_radius.xyz;
    let radius = volumes[instance_index].center_radius.w;

    let circle = vertex_index / (SPHERE_SEGMENTS * 6u);
    let in_circle = vertex_index % (SPHERE_SEGMENTS * 6u);
    let segment = in_circle / 6u;

    let angle0 = (f32(segment) / f32(SPHERE_SEGMENTS)) * 2.0 * PI;
    let angle1 = (f32(segment + 1u) / f32(SPHERE_SEGMENTS)) * 2.0 * PI;

    let c0 = cos(angle0) * radius;
    let s0 = sin(angle0) * radius;
    let c1 = cos(angle1) * radius;
    let s1 = sin(angle1) * radius;

    var offset0 = vec3<f32>(c0, 0.0, s0);                                 // x-z
    var offset1 = vec3<f32>(c1, 0.0, s1);
    if (circle == 1u) { offset0 = vec3<f32>(c0, s0, 0.0); offset1 = vec3<f32>(c1, s1, 0.0); }      // x-y
    else if (circle == 2u) { offset0 = vec3<f32>(0.0, c0, s0); offset1 = vec3<f32>(0.0, c1, s1); } // y-z

    return line_vertex(center + offset0, center + offset1, in_circle % 6u, SPHERE_COLOR);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    return in.color;
}

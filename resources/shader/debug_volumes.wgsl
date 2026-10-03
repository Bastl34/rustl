// debug rendering of volumes (culling bounds, physics colliders, lights, cameras) as lines
// all vertices are generated from the vertex/instance index (vertex pulling)
// -> no vertex buffers needed (one instance per volume)
//
// line width is not supported by WebGPU (always 1px) -> every line segment is
// extruded into a screen space quad (2 triangles) in the vertex shader instead

const LINE_WIDTH_PX: f32 = 2.0;

const SPHERE_SEGMENTS: u32 = 48u;  // lines per circle (keep in sync with debug_volumes.rs)
const CAPSULE_SEGMENTS: u32 = 32u; // lines per circle (keep in sync with debug_volumes.rs)
const LIGHT_SEGMENTS: u32 = 24u;   // lines per circle (keep in sync with debug_volumes.rs)

// light and camera icons keep their size on screen
const LIGHT_RADIUS_PX: f32 = 9.0;
const LIGHT_RAY_START_PX: f32 = 13.0;
const LIGHT_RAY_END_PX: f32 = 19.0;
const LIGHT_ARROW_PX: f32 = 45.0;
const LIGHT_ARROW_HEAD_PX: f32 = 8.0;
const LIGHT_CONE_PX: f32 = 60.0;
const CAMERA_DEPTH_PX: f32 = 60.0;

// params.z of a light icon (keep in sync with debug_volumes.rs)
const LIGHT_POINT: u32 = 0u;
const LIGHT_DIRECTIONAL: u32 = 1u;
const LIGHT_SPOT: u32 = 2u;
const LIGHT_SUN: u32 = 3u;
const LIGHT_HEMI_COLOR: u32 = 4u;
const LIGHT_HEMI_GROUND: u32 = 5u;

const HIDDEN_ALPHA: f32 = 0.25; // alpha share of lines behind geometry

const PI: f32 = 3.14159265359;

// set per pipeline: this pass only draws the see through volumes where they lie behind the scene depth
override HIDDEN_PASS: bool = false;

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
    transform: mat4x4<f32>, // world from local (rigid)
    params: vec4<f32>,      // box: xyz half extents / sphere: x radius / capsule: x radius, y half height / light: x spot angle, z kind / camera: xy half fov tangents or ortho half extents, z ortho / w: see through
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<storage, read> volumes: array<DebugVolume>;

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

fn culled() -> VertexOutput
{
    var out: VertexOutput;
    out.clip_position = vec4<f32>(0.0, 0.0, 2.0, 1.0); // degenerate -> clipped
    out.color = vec4<f32>(0.0);
    return out;
}

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
        return culled(); // fully behind
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

// one line of a volume in world space
fn world_line(volume_index: u32, p0: vec3<f32>, p1: vec3<f32>, quad_index: u32) -> VertexOutput
{
    let volume = volumes[volume_index];

    if (HIDDEN_PASS && volume.params.w <= 0.0)
    {
        return culled();
    }

    var color = volume.color;
    if (HIDDEN_PASS)
    {
        color.a = color.a * HIDDEN_ALPHA;
    }

    return line_vertex(p0, p1, quad_index, color);
}

// one line of a volume, given in the local space of the volume
fn volume_line(volume_index: u32, p0_local: vec3<f32>, p1_local: vec3<f32>, quad_index: u32) -> VertexOutput
{
    let transform = volumes[volume_index].transform;

    return world_line(volume_index, (transform * vec4<f32>(p0_local, 1.0)).xyz, (transform * vec4<f32>(p1_local, 1.0)).xyz, quad_index);
}

fn camera_right() -> vec3<f32>
{
    return vec3<f32>(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
}

fn camera_up() -> vec3<f32>
{
    return vec3<f32>(camera.view[0][1], camera.view[1][1], camera.view[2][1]);
}

// world units per screen pixel at a point, for perspective and orthographic views alike (0 behind the camera)
fn world_per_pixel(point: vec3<f32>) -> f32
{
    let c0 = camera.view_proj * vec4<f32>(point, 1.0);
    let c1 = camera.view_proj * vec4<f32>(point + camera_right(), 1.0);

    if (c0.w < 0.0001 || c1.w < 0.0001)
    {
        return 0.0;
    }

    let pixels = abs(c1.x / c1.w - c0.x / c0.w) * 0.5 * f32(camera.viewport_width);

    return 1.0 / max(pixels, 0.0001);
}

fn circle_point(segment: u32, segments: u32, radius: f32) -> vec2<f32>
{
    let angle = (f32(segment) / f32(segments)) * 2.0 * PI;
    return vec2<f32>(cos(angle), sin(angle)) * radius;
}

// corner bits: 1 = x max, 2 = z max, 4 = y max
fn box_corner(half: vec3<f32>, corner: u32) -> vec3<f32>
{
    return vec3<f32>
    (
        select(-half.x, half.x, (corner & 1u) != 0u),
        select(-half.y, half.y, (corner & 4u) != 0u),
        select(-half.z, half.z, (corner & 2u) != 0u)
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
    let half = volumes[instance_index].params.xyz;

    return volume_line(instance_index, box_corner(half, edge.x), box_corner(half, edge.y), vertex_index % 6u);
}

// 3 great circles (x-z, x-y, y-z plane) with SPHERE_SEGMENTS quads each
@vertex
fn vs_sphere(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let radius = volumes[instance_index].params.x;

    let circle = vertex_index / (SPHERE_SEGMENTS * 6u);
    let in_circle = vertex_index % (SPHERE_SEGMENTS * 6u);
    let segment = in_circle / 6u;

    let c0 = circle_point(segment, SPHERE_SEGMENTS, radius);
    let c1 = circle_point(segment + 1u, SPHERE_SEGMENTS, radius);

    var offset0 = vec3<f32>(c0.x, 0.0, c0.y);                                                           // x-z
    var offset1 = vec3<f32>(c1.x, 0.0, c1.y);
    if (circle == 1u) { offset0 = vec3<f32>(c0.x, c0.y, 0.0); offset1 = vec3<f32>(c1.x, c1.y, 0.0); }      // x-y
    else if (circle == 2u) { offset0 = vec3<f32>(0.0, c0.x, c0.y); offset1 = vec3<f32>(0.0, c1.x, c1.y); } // y-z

    return volume_line(instance_index, offset0, offset1, in_circle % 6u);
}

// capsule along y: 2 rings at the cap centers, then 2 profiles (x-y, z-y) of a split circle plus 2 side lines
@vertex
fn vs_capsule(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let radius = volumes[instance_index].params.x;
    let half_height = volumes[instance_index].params.y;

    let n = CAPSULE_SEGMENTS;
    let segment = vertex_index / 6u;
    let quad = vertex_index % 6u;

    // rings: top first, then bottom
    if (segment < 2u * n)
    {
        let y = select(half_height, -half_height, segment >= n);
        let c0 = circle_point(segment % n, n, radius);
        let c1 = circle_point(segment % n + 1u, n, radius);

        return volume_line(instance_index, vec3<f32>(c0.x, y, c0.y), vec3<f32>(c1.x, y, c1.y), quad);
    }

    let t = segment - 2u * n;
    let profile = t / (n + 2u);
    let k = t % (n + 2u);

    var q0: vec2<f32>; // x = across, y = up
    var q1: vec2<f32>;

    if (k < n)
    {
        // upper half circle around the top cap center, lower half around the bottom one
        let y = select(-half_height, half_height, k < n / 2u);
        q0 = circle_point(k, n, radius) + vec2<f32>(0.0, y);
        q1 = circle_point(k + 1u, n, radius) + vec2<f32>(0.0, y);
    }
    else
    {
        let x = select(-radius, radius, k == n);
        q0 = vec2<f32>(x, half_height);
        q1 = vec2<f32>(x, -half_height);
    }

    if (profile == 0u)
    {
        return volume_line(instance_index, vec3<f32>(q0.x, q0.y, 0.0), vec3<f32>(q1.x, q1.y, 0.0), quad);
    }

    return volume_line(instance_index, vec3<f32>(0.0, q0.y, q0.x), vec3<f32>(0.0, q1.y, q1.x), quad);
}

// light icon: circle + rays (point, sun), arrow along local z (directional, sun), cone along local z (spot), two half circles split by local z (hemispheric)
@vertex
fn vs_light(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let volume = volumes[instance_index];
    let center = volume.transform[3].xyz;
    let kind = u32(volume.params.z + 0.5);
    let px = world_per_pixel(center);

    let n = LIGHT_SEGMENTS;
    let segment = vertex_index / 6u;
    let quad = vertex_index % 6u;

    let right = camera_right();
    let up = camera_up();
    let hemispheric = kind == LIGHT_HEMI_COLOR || kind == LIGHT_HEMI_GROUND;

    // circle
    if (segment < n)
    {
        if (hemispheric) { return culled(); }

        let c0 = circle_point(segment, n, LIGHT_RADIUS_PX * px);
        let c1 = circle_point(segment + 1u, n, LIGHT_RADIUS_PX * px);

        return world_line(instance_index, center + right * c0.x + up * c0.y, center + right * c1.x + up * c1.y, quad);
    }

    // 8 rays
    if (segment < n + 8u)
    {
        if (kind != LIGHT_POINT && kind != LIGHT_SUN) { return culled(); }

        let ray = circle_point(segment - n, 8u, 1.0);
        let ray_dir = right * ray.x + up * ray.y;

        return world_line(instance_index, center + ray_dir * LIGHT_RAY_START_PX * px, center + ray_dir * LIGHT_RAY_END_PX * px, quad);
    }

    // arrow: shaft + 4 head lines
    if (segment < n + 13u)
    {
        if (kind != LIGHT_DIRECTIONAL && kind != LIGHT_SUN && kind != LIGHT_HEMI_COLOR) { return culled(); }

        let arrow_length = LIGHT_ARROW_PX * px;
        let head = LIGHT_ARROW_HEAD_PX * px;
        let tip = vec3<f32>(0.0, 0.0, arrow_length);
        let k = segment - n - 8u;

        if (k == 0u)
        {
            return volume_line(instance_index, vec3<f32>(0.0), tip, quad);
        }

        let head_side = circle_point(k - 1u, 4u, head);

        return volume_line(instance_index, tip, vec3<f32>(head_side.x, head_side.y, arrow_length - head), quad);
    }

    // cone: 8 side lines + ring, the slant stays the same length for every angle
    if (segment < 2u * n + 21u)
    {
        if (kind != LIGHT_SPOT) { return culled(); }

        let slant = LIGHT_CONE_PX * px;
        let cone_length = cos(volume.params.x) * slant;
        let cone_radius = sin(volume.params.x) * slant;
        let k = segment - n - 13u;

        if (k < 8u)
        {
            let cone_side = circle_point(k, 8u, cone_radius);

            return volume_line(instance_index, vec3<f32>(0.0), vec3<f32>(cone_side.x, cone_side.y, cone_length), quad);
        }

        let c0 = circle_point(k - 8u, n, cone_radius);
        let c1 = circle_point(k - 7u, n, cone_radius);

        return volume_line(instance_index, vec3<f32>(c0.x, c0.y, cone_length), vec3<f32>(c1.x, c1.y, cone_length), quad);
    }

    // hemispheric: half circle against local z (color, where the light comes from) or along it (ground color), the color half adds the divider
    if (!hemispheric) { return culled(); }

    // local z as seen on screen, straight up when it points at the camera
    let axis = volume.transform[2].xyz;
    var toward = vec2<f32>(dot(axis, right), dot(axis, up));
    if (length(toward) < 0.001) { toward = vec2<f32>(0.0, 1.0); }
    else { toward = normalize(toward); }

    if (kind == LIGHT_HEMI_COLOR) { toward = -toward; }

    let across = vec2<f32>(-toward.y, toward.x);
    let radius = LIGHT_RADIUS_PX * px;
    let half_segments = n / 2u;
    let k = segment - 2u * n - 21u;

    if (k < half_segments)
    {
        let a0 = f32(k) / f32(half_segments) * PI;
        let a1 = f32(k + 1u) / f32(half_segments) * PI;
        let q0 = (across * cos(a0) + toward * sin(a0)) * radius;
        let q1 = (across * cos(a1) + toward * sin(a1)) * radius;

        return world_line(instance_index, center + right * q0.x + up * q0.y, center + right * q1.x + up * q1.y, quad);
    }

    if (kind != LIGHT_HEMI_COLOR) { return culled(); }

    let divider = across * radius;

    return world_line(instance_index, center - right * divider.x - up * divider.y, center + right * divider.x + up * divider.y, quad);
}

// camera frustum in camera space (looking along -z): 4 side lines, far rectangle, near rectangle (orthographic only), up triangle
@vertex
fn vs_camera(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let volume = volumes[instance_index];
    let orthographic = volume.params.z > 0.5;
    let depth = CAMERA_DEPTH_PX * world_per_pixel(volume.transform[3].xyz);

    // perspective: a pyramid from the eye, orthographic: a box of the view extents
    var near_half = vec2<f32>(0.0);
    var far_half = volume.params.xy * depth;
    if (orthographic)
    {
        near_half = volume.params.xy;
        far_half = volume.params.xy;
    }

    let segment = vertex_index / 6u;
    let quad = vertex_index % 6u;

    var signs = array<vec2<f32>, 4>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));

    if (segment < 4u)
    {
        return volume_line(instance_index, vec3<f32>(near_half * signs[segment], 0.0), vec3<f32>(far_half * signs[segment], -depth), quad);
    }

    if (segment < 8u)
    {
        let k = segment - 4u;
        return volume_line(instance_index, vec3<f32>(far_half * signs[k], -depth), vec3<f32>(far_half * signs[(k + 1u) % 4u], -depth), quad);
    }

    if (segment < 12u)
    {
        if (!orthographic) { return culled(); }

        let k = segment - 8u;
        return volume_line(instance_index, vec3<f32>(near_half * signs[k], 0.0), vec3<f32>(near_half * signs[(k + 1u) % 4u], 0.0), quad);
    }

    // up triangle above the far rectangle
    let size = min(far_half.x, far_half.y) * 0.5;
    let base = far_half.y + size * 0.3;

    var triangle = array<vec2<f32>, 3>(vec2<f32>(-size, base), vec2<f32>(size, base), vec2<f32>(0.0, base + size));
    let k = segment - 12u;

    return volume_line(instance_index, vec3<f32>(triangle[k], -depth), vec3<f32>(triangle[(k + 1u) % 3u], -depth), quad);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
{
    return in.color;
}

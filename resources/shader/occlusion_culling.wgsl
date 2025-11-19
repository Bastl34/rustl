// TODO DELETE ME (FILE NO LONGER USED)

struct CameraUniform
{
    view_pos: vec4<f32>,
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
};

struct BoundingBox
{
    min: vec4<f32>,
    max: vec4<f32>,
    // model_transform: mat4x4<f32>,
};

struct VertexOutput
{
    @builtin(position) clip_position: vec4<f32>,
};

@group(0) @binding(0)
var<storage, read> boxes: array<BoundingBox>;

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput
{
    let box = boxes[instance_index];
    let corner = vec3<f32>
    (
        select(box.min.x, box.max.x, (vertex_index & 1u) != 0u),
        select(box.min.y, box.max.y, (vertex_index & 2u) != 0u),
        select(box.min.z, box.max.z, (vertex_index & 4u) != 0u),
    );

    // let world = box.model_transform * vec4<f32>(corner, 1.0);
    let world = vec4<f32>(corner, 1.0);
    let clip = camera.view_proj * world;

    var out: VertexOutput;
    out.clip_position = clip;
    return out;
}

// no fragment output needed - using just depth test
@fragment
fn fs_main()
{}
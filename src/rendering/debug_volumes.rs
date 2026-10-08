use std::{borrow::Cow, ops::Range};

use nalgebra::{Matrix4, Point3, Vector3};

use crate::{render_item_impl_default, rendering::{bind_groups::debug_volumes::DebugVolumesBindGroup, helper::buffer::create_empty_buffer, texture::Texture, wgpu::WGpu}, state::{helper::render_item::RenderItem, scene::{camera::{Camera, CameraProjectionType}, light::{Light, LightType}, physics::physics_world::{PhysicsDebugShape, PhysicsDebugState, PhysicsDebugVolume}}}};

const MIN_SIZE: usize = 1024; // entries (buffer grows on demand)

// every line segment is a screen space quad (2 triangles) - see debug_volumes.wgsl
pub const BOX_VERTICES: u32 = 12 * 6;                             // 12 edges
pub const SPHERE_SEGMENTS: u32 = 48;                              // lines per circle (keep in sync with debug_volumes.wgsl)
pub const SPHERE_VERTICES: u32 = 3 * SPHERE_SEGMENTS * 6;         // 3 great circles
pub const CAPSULE_SEGMENTS: u32 = 32;                             // lines per circle (keep in sync with debug_volumes.wgsl)
pub const CAPSULE_VERTICES: u32 = (4 * CAPSULE_SEGMENTS + 4) * 6; // 2 rings + 2 profiles (circle + 2 side lines each)
pub const LIGHT_SEGMENTS: u32 = 24;                               // lines per circle (keep in sync with debug_volumes.wgsl)
pub const LIGHT_VERTICES: u32 = (2 * LIGHT_SEGMENTS + LIGHT_SEGMENTS / 2 + 22) * 6; // circle, 8 rays, arrow (5 lines), cone (8 sides + ring), half circle + divider
pub const CAMERA_VERTICES: u32 = 15 * 6;                          // 4 sides, far + near rectangle, up triangle
pub const SEGMENT_VERTICES: u32 = 6;                              // one line

// groups in buffer order: boxes, spheres, capsules, lights, cameras, segments
const GROUP_VERTICES: [u32; 6] = [BOX_VERTICES, SPHERE_VERTICES, CAPSULE_VERTICES, LIGHT_VERTICES, CAMERA_VERTICES, SEGMENT_VERTICES];
const GROUP_ENTRY_POINTS: [&str; 6] = ["vs_box", "vs_sphere", "vs_capsule", "vs_light", "vs_camera", "vs_segment"];
const CAMERA_GROUP: usize = 4;

pub const BOUNDING_BOX_COLOR: [f32; 4] = [1.0, 0.6, 0.1, 1.0];
pub const BOUNDING_SPHERE_COLOR: [f32; 4] = [0.2, 0.8, 1.0, 1.0];
const CAMERA_COLOR: [f32; 4] = [0.9, 0.9, 0.9, 1.0];

// mesh colliders are only drawn as their bounds, so they stay fainter than the exact shapes
const PHYSICS_BOUNDS_ALPHA: f32 = 0.5;

// params.z of a light icon (keep in sync with vs_light in debug_volumes.wgsl)
const LIGHT_POINT: f32 = 0.0;
const LIGHT_DIRECTIONAL: f32 = 1.0;
const LIGHT_SPOT: f32 = 2.0;
const LIGHT_SUN: f32 = 3.0;
const LIGHT_HEMI_COLOR: f32 = 4.0;
const LIGHT_HEMI_GROUND: f32 = 5.0;

// a spot cone of 90 degrees or more has no tip left to draw
const MAX_SPOT_ANGLE: f32 = 89.0f32.to_radians();

// params.w: the volume is also drawn (faded) where it is hidden behind geometry
const SEE_THROUGH: f32 = 1.0;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DebugVolume
{
    pub transform: [[f32; 4]; 4], // world from local (rigid)
    pub params: [f32; 4],         // box: xyz half extents / sphere: x radius / capsule: x radius, y half height / light: x spot angle, z kind / camera: xy half fov tangents or ortho half extents, z ortho / segment: xyz to its end / w: see through
    pub color: [f32; 4],
}

impl DebugVolume
{
    pub fn new(transform: &Matrix4<f32>, params: [f32; 4], color: [f32; 4]) -> Self
    {
        Self
        {
            transform: (*transform).into(),
            params,
            color,
        }
    }

    pub fn aabb(min: &Point3<f32>, max: &Point3<f32>, color: [f32; 4]) -> Self
    {
        let center = nalgebra::center(min, max);
        let half = (max - min) * 0.5;

        Self::new(&Matrix4::new_translation(&center.coords), [half.x, half.y, half.z, 0.0], color)
    }

    pub fn sphere(center: &Point3<f32>, radius: f32, color: [f32; 4]) -> Self
    {
        Self::new(&Matrix4::new_translation(&center.coords), [radius, 0.0, 0.0, 0.0], color)
    }

    pub fn is_see_through(&self) -> bool
    {
        self.params[3] > 0.0
    }
}

pub fn physics_color(state: PhysicsDebugState) -> [f32; 4]
{
    match state
    {
        PhysicsDebugState::Static => [0.3, 0.85, 0.35, 1.0],
        PhysicsDebugState::Kinematic => [0.45, 0.45, 1.0, 1.0],
        PhysicsDebugState::Dynamic => [1.0, 0.3, 0.75, 1.0],
        PhysicsDebugState::Sleeping => [0.6, 0.35, 0.55, 1.0],
        PhysicsDebugState::Waiting => [0.75, 0.55, 1.0, 1.0],
        PhysicsDebugState::Character => [1.0, 0.95, 0.25, 1.0],
    }
}

// rotation whose local z axis points along dir (identity for a zero direction)
fn direction_basis(dir: &Vector3<f32>) -> Matrix4<f32>
{
    let Some(z) = dir.try_normalize(1.0e-6) else { return Matrix4::identity(); };

    let helper = if z.y.abs() < 0.99 { Vector3::y() } else { Vector3::x() };
    let x = helper.cross(&z).normalize();
    let y = z.cross(&x);

    Matrix4::new
    (
        x.x, y.x, z.x, 0.0,
        x.y, y.y, z.y, 0.0,
        x.z, y.z, z.z, 0.0,
        0.0, 0.0, 0.0, 1.0
    )
}

// a light color at full brightness, so a dim light still reads - black becomes grey
fn light_icon_color(color: &Vector3<f32>) -> [f32; 4]
{
    let brightest = color.max();

    if brightest <= 0.0
    {
        return [0.5, 0.5, 0.5, 1.0];
    }

    [color.x / brightest, color.y / brightest, color.z / brightest, 1.0]
}

// the volumes of one frame, grouped by the pipeline that draws them
#[derive(Default)]
pub struct DebugVolumeList
{
    pub boxes: Vec<DebugVolume>,
    pub spheres: Vec<DebugVolume>,
    pub capsules: Vec<DebugVolume>,
    pub lights: Vec<DebugVolume>,
    pub cameras: Vec<DebugVolume>,
    pub segments: Vec<DebugVolume>,

    pub camera_ids: Vec<u32>, // one per camera volume, so a camera can leave out its own frustum
}

impl DebugVolumeList
{
    pub fn add_physics(&mut self, volume: &PhysicsDebugVolume)
    {
        let color = physics_color(volume.state);

        match volume.shape
        {
            PhysicsDebugShape::Box { half_extents } =>
            {
                self.boxes.push(DebugVolume::new(&volume.transform, [half_extents.x, half_extents.y, half_extents.z, SEE_THROUGH], color));
            }
            PhysicsDebugShape::Bounds { half_extents } =>
            {
                let faint = [color[0], color[1], color[2], color[3] * PHYSICS_BOUNDS_ALPHA];
                self.boxes.push(DebugVolume::new(&volume.transform, [half_extents.x, half_extents.y, half_extents.z, SEE_THROUGH], faint));
            }
            PhysicsDebugShape::Sphere { radius } =>
            {
                self.spheres.push(DebugVolume::new(&volume.transform, [radius, 0.0, 0.0, SEE_THROUGH], color));
            }
            PhysicsDebugShape::Capsule { half_height, radius } =>
            {
                self.capsules.push(DebugVolume::new(&volume.transform, [radius, half_height, 0.0, SEE_THROUGH], color));
            }
            PhysicsDebugShape::Segment { a, b } =>
            {
                let to_end = b - a;
                self.segments.push(DebugVolume::new(&(volume.transform * Matrix4::new_translation(&a)), [to_end.x, to_end.y, to_end.z, SEE_THROUGH], color));
            }
        }
    }

    // an icon in the light color at the light position, nothing for a disabled light
    pub fn add_light(&mut self, light: &Light)
    {
        if !light.enabled
        {
            return;
        }

        // local z points along the light direction
        let transform = Matrix4::new_translation(&light.pos.coords) * direction_basis(&light.dir);
        let spot_angle = light.max_angle.clamp(0.0, MAX_SPOT_ANGLE);

        let mut push = |kind: f32, color: &Vector3<f32>|
        {
            self.lights.push(DebugVolume::new(&transform, [spot_angle, 0.0, kind, SEE_THROUGH], light_icon_color(color)));
        };

        match light.light_type
        {
            LightType::Point => push(LIGHT_POINT, &light.color),
            LightType::Directional => push(LIGHT_DIRECTIONAL, &light.color),
            LightType::Spot => push(LIGHT_SPOT, &light.color),
            LightType::Sun => push(LIGHT_SUN, &light.color),
            // two half circles: the color side faces where the light comes from (against dir), the ground color side the other way
            LightType::Hemispheric =>
            {
                push(LIGHT_HEMI_COLOR, &light.color);
                push(LIGHT_HEMI_GROUND, &light.ground_color);
            }
        }
    }

    // the frustum a camera renders, read from its projection so fov and aspect are exact - nothing for a disabled camera
    pub fn add_camera(&mut self, camera: &Camera)
    {
        if !camera.enabled
        {
            return;
        }

        let data = camera.get_data();
        let projection = &data.projection;

        if projection[(0, 0)].abs() < f32::EPSILON || projection[(1, 1)].abs() < f32::EPSILON
        {
            return;
        }

        // perspective: tangents of the half fov, orthographic: half extents in world units
        let half_x = 1.0 / projection[(0, 0)];
        let half_y = 1.0 / projection[(1, 1)];

        let orthographic = data.projection_type == CameraProjectionType::Orthogonal;

        // an off center orthographic view is shifted within the camera plane
        let center = if orthographic { Vector3::new(-projection[(0, 3)] * half_x, -projection[(1, 3)] * half_y, 0.0) } else { Vector3::zeros() };

        let transform = data.view_inverse * Matrix4::new_translation(&center);

        self.cameras.push(DebugVolume::new(&transform, [half_x.abs(), half_y.abs(), if orthographic { 1.0 } else { 0.0 }, SEE_THROUGH], CAMERA_COLOR));
        self.camera_ids.push(camera.id);
    }
}

pub struct DebugVolumesBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize, // capacity (entries)
    pub count: usize,       // used entries

    groups: [(Range<u32>, bool); 6],                                      // instance range + any see through volume, per group
    pipelines: [Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>; 6], // (visible, hidden behind geometry) per group
    camera_ids: Vec<u32>,                                                 // camera id per instance of the camera group
}

crate::render_item_send_sync!(DebugVolumesBuffer);

impl RenderItem for DebugVolumesBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.buffer.size()
    }
}

impl DebugVolumesBuffer
{
    pub fn new(wgpu: &mut WGpu) -> Self
    {
        let mut volumes_buffer = Self
        {
            buffer: create_empty_buffer(wgpu),
            buffer_size: 0,
            count: 0,

            groups: Default::default(),
            pipelines: [None, None, None, None, None, None],
            camera_ids: vec![],
        };

        volumes_buffer.update(wgpu, &DebugVolumeList::default());

        volumes_buffer
    }

    // returns true if the gpu buffer was recreated (bind groups have to be recreated)
    pub fn update(&mut self, wgpu: &mut WGpu, list: &DebugVolumeList) -> bool
    {
        let groups = [&list.boxes, &list.spheres, &list.capsules, &list.lights, &list.cameras, &list.segments];
        let total: usize = groups.iter().map(|group| group.len()).sum();

        let new_buffer_size = total.next_power_of_two().max(MIN_SIZE);

        let recreated = new_buffer_size > self.buffer_size;

        if recreated
        {
            // recreate buffer
            self.buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
            {
                label: Some("Debug Volumes Buffer"),
                size: (std::mem::size_of::<DebugVolume>() * new_buffer_size) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.buffer_size = new_buffer_size;
        }

        // the groups are stored one after another, every draw reads its own instance range
        let mut offset = 0;

        for (index, group) in groups.iter().enumerate()
        {
            // only write the used entries - the draws never read past them
            if !group.is_empty()
            {
                wgpu.queue_mut().write_buffer(&self.buffer, (offset * std::mem::size_of::<DebugVolume>()) as u64, bytemuck::cast_slice(group.as_slice()));
            }

            let see_through = group.iter().any(|volume| volume.is_see_through());
            self.groups[index] = (offset as u32..(offset + group.len()) as u32, see_through);

            offset += group.len();
        }

        self.camera_ids.clone_from(&list.camera_ids);
        self.count = total;

        recreated
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    // (vertices per volume, instance range, see through, (visible, hidden) pipelines) of every non empty group, as seen by one camera
    pub fn draws(&self, camera_id: u32) -> Vec<(u32, Range<u32>, bool, &(wgpu::RenderPipeline, wgpu::RenderPipeline))>
    {
        let mut draws = vec![];

        for (index, ((range, see_through), pipelines)) in self.groups.iter().zip(self.pipelines.iter()).enumerate()
        {
            let Some(pipelines) = pipelines.as_ref() else { continue; };

            let mut ranges = vec![range.clone()];

            // a camera never draws its own frustum
            if index == CAMERA_GROUP
            {
                if let Some(own) = self.camera_ids.iter().position(|id| *id == camera_id)
                {
                    let own = range.start + own as u32;
                    ranges = vec![range.start..own, own + 1..range.end];
                }
            }

            for range in ranges.into_iter().filter(|range| !range.is_empty())
            {
                draws.push((GROUP_VERTICES[index], range, *see_through, pipelines));
            }
        }

        draws
    }

    // pipelines per shape group (vertex pulling - no vertex buffers)
    // lines are rendered as screen space quads (WebGPU has no line width support)
    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, shader_source: &String, samples: u32, reverse_z: bool)
    {
        let bind_group_layout = DebugVolumesBindGroup::bind_layout(wgpu);

        let device = wgpu.device();
        let surface_format = wgpu.surface_config().format;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some("debug volumes"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)).into(),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some("debug volumes layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            ..Default::default()
        });

        let fragment_targets = [Some(wgpu::ColorTargetState
        {
            format: surface_format,
            blend: Some(wgpu::BlendState
            {
                color: wgpu::BlendComponent
                {
                    operation: wgpu::BlendOperation::Add,
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                },
                alpha: wgpu::BlendComponent
                {
                    operation: wgpu::BlendOperation::Add,
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let create_pipeline = |name: &str, vertex_entry_point: &str, hidden: bool| -> wgpu::RenderPipeline
        {
            // the hidden pass only draws what lies behind the scene depth (HIDDEN_PASS in the shader)
            let depth_compare = match (reverse_z, hidden)
            {
                (false, false) => wgpu::CompareFunction::LessEqual,
                (false, true) => wgpu::CompareFunction::Greater,
                (true, false) => wgpu::CompareFunction::GreaterEqual,
                (true, true) => wgpu::CompareFunction::Less,
            };

            let constants = [("HIDDEN_PASS", if hidden { 1.0 } else { 0.0 })];

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
            {
                label: Some(name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState
                {
                    module: &shader,
                    entry_point: Some(vertex_entry_point),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions { constants: &constants, ..Default::default() },
                },
                fragment: Some(wgpu::FragmentState
                {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &fragment_targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState
                {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                // tested against the scene depth, never written
                depth_stencil: Some(wgpu::DepthStencilState
                {
                    format: Texture::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(depth_compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState
                {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };

        for (index, entry_point) in GROUP_ENTRY_POINTS.iter().enumerate()
        {
            let visible = create_pipeline(&format!("debug volumes {}", entry_point), entry_point, false);
            let hidden = create_pipeline(&format!("debug volumes {} hidden", entry_point), entry_point, true);

            self.pipelines[index] = Some((visible, hidden));
        }
    }
}

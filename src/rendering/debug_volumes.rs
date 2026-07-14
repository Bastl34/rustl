use std::borrow::Cow;

use nalgebra::Point3;

use crate::{render_item_impl_default, rendering::{bind_groups::debug_volumes::DebugVolumesBindGroup, helper::buffer::create_empty_buffer, texture::Texture, wgpu::WGpu}, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 1024; // entries (buffer grows on demand)

// every line segment is a screen space quad (2 triangles) - see debug_volumes.wgsl
pub const BOX_VERTICES: u32 = 12 * 6;                     // 12 edges
pub const SPHERE_SEGMENTS: u32 = 48;                      // lines per circle (keep in sync with debug_volumes.wgsl)
pub const SPHERE_VERTICES: u32 = 3 * SPHERE_SEGMENTS * 6; // 3 great circles

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DebugVolume
{
    pub min: [f32; 4],           // xyz = world space aabb min
    pub max: [f32; 4],           // xyz = world space aabb max
    pub center_radius: [f32; 4], // xyz = sphere center, w = sphere radius
}

impl DebugVolume
{
    pub fn new(min: &Point3<f32>, max: &Point3<f32>, sphere_center: &Point3<f32>, sphere_radius: f32) -> Self
    {
        Self
        {
            min: [min.x, min.y, min.z, 0.0],
            max: [max.x, max.y, max.z, 0.0],
            center_radius: [sphere_center.x, sphere_center.y, sphere_center.z, sphere_radius],
        }
    }
}

pub struct DebugVolumesBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize, // capacity (entries)
    pub count: usize,       // used entries

    box_pipeline: Option<wgpu::RenderPipeline>,
    sphere_pipeline: Option<wgpu::RenderPipeline>,
}

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

            box_pipeline: None,
            sphere_pipeline: None,
        };

        volumes_buffer.update(wgpu, &vec![]);

        volumes_buffer
    }

    // returns true if the gpu buffer was recreated (bind groups have to be recreated)
    pub fn update(&mut self, wgpu: &mut WGpu, buffer_data: &Vec<DebugVolume>) -> bool
    {
        let new_buffer_size = buffer_data.len().next_power_of_two().max(MIN_SIZE);

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

        // only write the used entries - the draws never read past count
        if !buffer_data.is_empty()
        {
            wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(buffer_data));
        }

        self.count = buffer_data.len();

        recreated
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn box_pipeline(&self) -> Option<&wgpu::RenderPipeline>
    {
        self.box_pipeline.as_ref()
    }

    pub fn sphere_pipeline(&self) -> Option<&wgpu::RenderPipeline>
    {
        self.sphere_pipeline.as_ref()
    }

    // pipelines for boxes and spheres (vertex pulling - no vertex buffers)
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

        let create_pipeline = |name: &str, vertex_entry_point: &str| -> wgpu::RenderPipeline
        {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
            {
                label: Some(name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState
                {
                    module: &shader,
                    entry_point: Some(vertex_entry_point),
                    buffers: &[],
                    compilation_options: Default::default(),
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
                // tested against the scene depth (lines behind geometry are hidden), never written
                depth_stencil: Some(wgpu::DepthStencilState
                {
                    format: Texture::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(if reverse_z { wgpu::CompareFunction::GreaterEqual } else { wgpu::CompareFunction::LessEqual }),
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

        self.box_pipeline = Some(create_pipeline("debug volumes boxes", "vs_box"));
        self.sphere_pipeline = Some(create_pipeline("debug volumes spheres", "vs_sphere"));
    }
}

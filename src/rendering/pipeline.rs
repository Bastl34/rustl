use std::{fs, borrow::Cow};

use wgpu::BindGroup;
use wgpu::util::DeviceExt;

use super::{wgpu::WGpu, buffer::Buffer, buffer::Vertex, texture::Texture, camera::{CameraUniform}};

pub struct Pipeline
{
    pub name: String,
    pipe: wgpu::RenderPipeline,

    diffuse_bind_group: BindGroup,

    camera_buffer: wgpu::Buffer,
    camera_bind_group: BindGroup
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, buffer: &Buffer, name: &str, shader_path: &str, texture: &Texture, cam: &CameraUniform) -> Pipeline
    {
        let shader_source = fs::read_to_string(shader_path).unwrap();

        let device = wgpu.device();
        let config = wgpu.surface_config();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&shader_source)).into(),
        });

        // ******************** texture ********************
        let texture_bind_group_layout_name = format!("{} {} texture_bind_group_layout ", name, texture.name);
        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // This should match the filterable field of the
                    // corresponding Texture entry above.
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some(texture_bind_group_layout_name.as_str()),
        });

        let diffuse_bind_group_layout_name = format!("{} {} diffuse_bind_group ", name, texture.name);
        let diffuse_bind_group = device.create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &texture_bind_group_layout,
                entries:
                &[
                    wgpu::BindGroupEntry
                    {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture.get_view()),
                    },
                    wgpu::BindGroupEntry
                    {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&texture.get_sampler()),
                    }
                ],
                label: Some(diffuse_bind_group_layout_name.as_str()),
            }
        );

        // ******************** camera ********************
        let camera_buffer_name = format!("{} Camera Buffer", name);
        let camera_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(camera_buffer_name.as_str()),
                //contents: bytemuck::cast_slice(&[cam]),
                contents: bytemuck::cast_slice(&cam.view_proj),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let camera_bind_group_layout_name = format!("{} camera_bind_group_layout", name);
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some(camera_bind_group_layout_name.as_str()),
        });

        let camera_bind_group_name = format!("{} camera_bind_group", name);
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &camera_bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }
            ],
            label: Some(camera_bind_group_name.as_str()),
        });



        let layout_name = format!("{} Layout", name);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts:
            &[
                &texture_bind_group_layout,
                &camera_bind_group_layout,

            ],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some(name),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &shader,
                entry_point: "vs_main",
                buffers:
                &[
                    Vertex::desc()
                ],
            },
            fragment: Some(wgpu::FragmentState
            {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState
                {
                    format: config.format,
                    blend: Some(wgpu::BlendState
                    {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState
            {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // indicates how many array layers the attachments will have.
            multiview: None,
        });

        Self
        {
            name: name.to_string(),
            pipe: render_pipeline,
            diffuse_bind_group,

            camera_buffer,
            camera_bind_group
        }
    }

    pub fn update_camera(&self, wgpu: &mut WGpu, cam: &CameraUniform)
    {
        wgpu.queue_mut().write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[cam.view_proj]));
    }

    pub fn get(&self) -> &wgpu::RenderPipeline
    {
        &self.pipe
    }

    pub fn get_diffuse_bind_group(&self) -> &BindGroup
    {
        &self.diffuse_bind_group
    }

    pub fn get_camera_bind_group(&self) -> &BindGroup
    {
        &self.camera_bind_group
    }

}

use std::{fs, borrow::Cow};

use wgpu::BindGroup;
use wgpu::util::DeviceExt;

use super::{wgpu::WGpu, buffer::Buffer, buffer::Vertex, texture::{Texture, self}, camera::{CameraUniform}, uniform, instance::InstanceRaw};

pub struct Pipeline
{
    pub name: String,
    pipe: wgpu::RenderPipeline,

    textures_bind_group: BindGroup,

    camera_buffer: wgpu::Buffer,
    camera_bind_group: BindGroup
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, buffer: &Buffer, name: &str, shader_path: &str, textures: &Vec<&Texture>, cam: &CameraUniform, depth_stencil: bool) -> Pipeline
    {
        let shader_source = fs::read_to_string(shader_path).unwrap();

        let device = wgpu.device();
        let config = wgpu.surface_config();

        // ******************** shader ********************
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&shader_source)).into(),
        });

        // ******************** textures ********************
        let mut textures_layout_group_vec = vec![];
        let mut textures_group_vec = vec![];

        let mut i = 0;
        for texture in textures
        {
            let textures_layout_group = texture.get_bind_group_layout_entries(i);
            let textures_group = texture.get_bind_group_entries(i);

            textures_layout_group_vec.append(&mut textures_layout_group.to_vec());
            textures_group_vec.append(&mut textures_group.to_vec());

            i += 1;
        }

        let textures_bind_group_layout_name = format!("{} texture_bind_group_layout ", name);
        let textures_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &textures_layout_group_vec.as_slice(),
            label: Some(textures_bind_group_layout_name.as_str()),
        });

        let textures_bind_group_name = format!("{} texturediffuse_bind_group ", name);
        let textures_bind_group = device.create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &textures_bind_group_layout,
                entries:
                &textures_group_vec.as_slice(),
                label: Some(textures_bind_group_name.as_str()),
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
                uniform::uniform_bind_group_layout_entry(0, true, false)
            ],
            label: Some(camera_bind_group_layout_name.as_str()),
        });

        let camera_bind_group_name = format!("{} camera_bind_group", name);
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &camera_bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &camera_buffer)
            ],
            label: Some(camera_bind_group_name.as_str()),
        });

        // ******************** render pipeline ********************
        let layout_name = format!("{} Layout", name);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts:
            &[
                &textures_bind_group_layout,
                &camera_bind_group_layout,

            ],
            push_constant_ranges: &[],
        });

        let mut depth_stencil_state = None;
        if depth_stencil
        {
            depth_stencil_state = Some(wgpu::DepthStencilState
            {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less, // front to back
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            });
        }

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
                    Vertex::desc(),
                    InstanceRaw::desc()
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
            depth_stencil: depth_stencil_state,
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

            textures_bind_group,

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

    pub fn get_textures_bind_group(&self) -> &BindGroup
    {
        &self.textures_bind_group
    }

    pub fn get_camera_bind_group(&self) -> &BindGroup
    {
        &self.camera_bind_group
    }

}

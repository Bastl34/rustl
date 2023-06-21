use std::{borrow::Cow};

use wgpu::{BindGroup, TextureView, ShaderModule, Device, RenderPipeline};
use wgpu::util::DeviceExt;

use super::camera::CameraBuffer;
use super::light::LightBuffer;
use super::{wgpu::WGpu, vertex_buffer::{Vertex, VertexBuffer}, texture::{Texture, self}, camera::CameraUniform, uniform, instance::Instance, light::LightUniform};

pub struct Pipeline
{
    pub name: String,
    pub fragment_attachment: bool,

    shader: ShaderModule,
    pipe: wgpu::RenderPipeline,

    textures_bind_group: BindGroup,

    camera_bind_group: BindGroup,

    light_bind_group: BindGroup
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, name: &str, shader_source: &String, textures: &Vec<&Texture>, cam: &CameraBuffer, light: &LightBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32) -> Pipeline
    {
        let device = wgpu.device();

        // ******************** shader ********************
        let shader = Pipeline::create_shader(device, name, shader_source);

        let (render_pipeline, fragment_attachment, textures_bind_group, camera_bind_group, light_bind_group) = Pipeline::create(wgpu, name, &shader, textures, cam, light, depth_stencil, fragment_attachment, samples);

        Self
        {
            name: name.to_string(),
            shader,
            pipe: render_pipeline,

            fragment_attachment,

            textures_bind_group,

            camera_bind_group,

            //light_buffer,
            light_bind_group,
        }
    }

    pub fn create(wgpu: &mut WGpu, name: &str, shader: &ShaderModule, textures: &Vec<&Texture>, cam: &CameraBuffer, light: &LightBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32) -> (RenderPipeline, bool, BindGroup, BindGroup, BindGroup)
    {
        let device = wgpu.device();
        let config = wgpu.surface_config();

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

        let textures_bind_group_layout_name = format!("{} texture_bind_group_layout", name);
        let textures_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries: &textures_layout_group_vec.as_slice(),
            label: Some(textures_bind_group_layout_name.as_str()),
        });

        let textures_bind_group_name = format!("{} texture__bind_group", name);
        let textures_bind_group = device.create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &textures_bind_group_layout,
                entries: &textures_group_vec.as_slice(),
                label: Some(textures_bind_group_name.as_str()),
            }
        );

        // ******************** camera ********************
        let camera_bind_group_layout_name = format!("{} camera_bind_group_layout", name);
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true)
            ],
            label: Some(camera_bind_group_layout_name.as_str()),
        });

        let camera_bind_group_name = format!("{} camera_bind_group", name);
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &camera_bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &cam.get_buffer())
            ],
            label: Some(camera_bind_group_name.as_str()),
        });

        // ******************** light ********************
        let light_bind_group_layout_name = format!("{} light_bind_group_layout", name);
        let light_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true)
            ],
            label: Some(light_bind_group_layout_name.as_str()),
        });

        let light_bind_group_name = format!("{} light_bind_group", name);
        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &light_bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &light.get_buffer())
            ],
            label: Some(light_bind_group_name.as_str()),
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
                &light_bind_group_layout,
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

        let fragment_targets = &[Some(wgpu::ColorTargetState
        {
            format: config.format,
            blend: Some(wgpu::BlendState
            {
                color: wgpu::BlendComponent::REPLACE,
                alpha: wgpu::BlendComponent::REPLACE,
            }),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let mut fragment_state = None;
        if fragment_attachment
        {
            fragment_state = Some(wgpu::FragmentState
            {
                module: &shader,
                entry_point: "fs_main",
                targets: fragment_targets
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
                    Instance::desc()
                ],
            },
            fragment: fragment_state,
            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                //cull_mode: Some(wgpu::Face::Back), // backface culling
                cull_mode: None,
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
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // indicates how many array layers the attachments will have.
            multiview: None,
        });

        (render_pipeline, fragment_attachment, textures_bind_group, camera_bind_group, light_bind_group)
    }

    pub fn re_create(&mut self, wgpu: &mut WGpu, textures: &Vec<&Texture>, cam: &CameraBuffer, light: &LightBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32)
    {
        let (render_pipeline, fragment_attachment, textures_bind_group, camera_bind_group, light_bind_group) = Pipeline::create(wgpu, &self.name, &self.shader, textures, cam, light, depth_stencil, fragment_attachment, samples);

        self.pipe = render_pipeline;

        self.fragment_attachment = fragment_attachment;

        self.textures_bind_group = textures_bind_group;

        self.camera_bind_group = camera_bind_group;

        self.light_bind_group = light_bind_group;
    }

    pub fn create_shader(device: &Device, name: &str, shader_source: &String) -> ShaderModule
    {
        device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&shader_source)).into(),
        })
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

    pub fn get_light_bind_group(&self) -> &BindGroup
    {
        &self.light_bind_group
    }

}

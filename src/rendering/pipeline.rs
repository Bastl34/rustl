use std::{borrow::Cow};

use wgpu::util::DeviceExt;
use wgpu::{BindGroup, ShaderModule, Device, BindGroupLayout, Buffer};

use super::camera::CameraBuffer;
use super::light::LightBuffer;
use super::{wgpu::WGpu, vertex_buffer::{Vertex}, texture::{Texture, self}, uniform, instance::Instance};

pub struct Pipeline
{
    pub name: String,
    pub fragment_attachment: bool,

    shader: ShaderModule,
    pipeline: Option<wgpu::RenderPipeline>,

    textures_bind_group: Option<BindGroup>,
    light_bind_group: Option<BindGroup>,

    textures_bind_group_layout: Option<BindGroupLayout>,
    light_bind_group_layout: Option<BindGroupLayout>,
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, name: &str, shader_source: &String, textures: &Vec<&Texture>, cam: &CameraBuffer, lights: &LightBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader
            shader = Pipeline::create_shader(device, name, shader_source);
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment,

            shader,
            pipeline: None,

            textures_bind_group: None,
            light_bind_group: None,

            textures_bind_group_layout: None,
            light_bind_group_layout: None,
        };

        pipe.create_binding_groups(wgpu, textures, lights);
        pipe.create(wgpu, cam, depth_stencil, fragment_attachment, samples);

        pipe
    }

    pub fn create_binding_groups(&mut self, wgpu: &mut WGpu, textures: &Vec<&Texture>, lights: &LightBuffer)
    {
        let device = wgpu.device();

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

        let textures_bind_group_layout_name = format!("{} texture_bind_group_layout", self.name);
        let textures_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries: &textures_layout_group_vec.as_slice(),
            label: Some(textures_bind_group_layout_name.as_str()),
        });

        let textures_bind_group_name = format!("{} texture__bind_group", self.name);
        let textures_bind_group = device.create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &textures_bind_group_layout,
                entries: &textures_group_vec.as_slice(),
                label: Some(textures_bind_group_name.as_str()),
            }
        );

        // ******************** lights ********************
        let light_bind_group_layout_name = format!("{} light_bind_group_layout", self.name);
        let light_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true),
                uniform::uniform_bind_group_layout_entry(1, true, true),
            ],
            label: Some(light_bind_group_layout_name.as_str()),
        });

        let light_bind_group_name = format!("{} light_bind_group", self.name);
        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &light_bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &lights.get_amount_buffer()),
                uniform::uniform_bind_group(1, &lights.get_lights_buffer()),
            ],
            label: Some(light_bind_group_name.as_str()),
        });

        self.textures_bind_group = Some(textures_bind_group);
        self.light_bind_group = Some(light_bind_group);

        self.textures_bind_group_layout = Some(textures_bind_group_layout);
        self.light_bind_group_layout = Some(light_bind_group_layout);
    }

    pub fn create(&mut self, wgpu: &mut WGpu, cam: &CameraBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32)
    {
        let device = wgpu.device();
        let config = wgpu.surface_config();

        let layout_name = format!("{} Layout", self.name);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts:
            &[
                self.textures_bind_group_layout.as_ref().unwrap(),
                cam.get_bind_group_layout(),
                self.light_bind_group_layout.as_ref().unwrap(),
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
                module: &self.shader,
                entry_point: "fs_main",
                targets: fragment_targets
            });
        }

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some(&self.name),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &self.shader,
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
                cull_mode: Some(wgpu::Face::Back), // backface culling
                //cull_mode: None,
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
            multiview: None,
        });

        self.pipeline = Some(render_pipeline);
    }

    pub fn re_create(&mut self, wgpu: &mut WGpu, textures: &Vec<&Texture>, cam: &CameraBuffer, lights: &LightBuffer, depth_stencil: bool, fragment_attachment: bool, samples: u32)
    {
        dbg!("recreating pipeline");

        self.create_binding_groups(wgpu, textures, lights);
        self.create(wgpu, cam, depth_stencil, fragment_attachment, samples);
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
        self.pipeline.as_ref().unwrap()
    }

    pub fn get_textures_bind_group(&self) -> &BindGroup
    {
        self.textures_bind_group.as_ref().unwrap()
    }

    pub fn get_light_bind_group(&self) -> &BindGroup
    {
        self.light_bind_group.as_ref().unwrap()
    }

}

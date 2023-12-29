use std::borrow::Cow;

use wgpu::{ShaderModule, Device, BindGroupLayout};

use super::{wgpu::WGpu, vertex_buffer::Vertex, texture::{self}, instance::Instance, skeleton::MAX_JOINTS};

pub struct Pipeline
{
    pub name: String,
    pub fragment_attachment: bool,

    max_lights: u32,

    shader: ShaderModule,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout], max_lights: u32, depth_stencil: bool, fragment_attachment: bool, samples: u32) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader
            let prepared_shader = Self::prepare_shader(shader_source, max_lights);
            shader = Pipeline::create_shader(device, name, &prepared_shader);
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment,

            max_lights: max_lights,

            shader,
            pipeline: None,
        };

        pipe.create(wgpu, bind_group_layouts, depth_stencil, fragment_attachment, samples);

        pipe
    }

    pub fn prepare_shader(shader_source: &String, max_lights: u32) -> String
    {
        let mut shader = shader_source.clone();

        shader = shader.replace("[MAX_LIGHTS]", format!("{}", max_lights).as_str());
        shader = shader.replace("[MAX_JOINTS]", format!("{}", MAX_JOINTS).as_str());

        shader
    }

    pub fn create(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], depth_stencil: bool, fragment_attachment: bool, samples: u32)
    {
        let device = wgpu.device();
        let config = wgpu.surface_config();

        let layout_name = format!("{} Layout", self.name);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts: bind_group_layouts,
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
            /*
            blend: Some(wgpu::BlendState
            {
                color: wgpu::BlendComponent::REPLACE,
                alpha: wgpu::BlendComponent::REPLACE,
            }),
            */
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
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                },
                //alpha: wgpu::BlendComponent::REPLACE,
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

    pub fn re_create(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], depth_stencil: bool, fragment_attachment: bool, samples: u32)
    {
        dbg!("recreating pipeline");

        self.create(wgpu, bind_group_layouts, depth_stencil, fragment_attachment, samples);
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
}

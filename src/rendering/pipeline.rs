use std::{fs, borrow::Cow};

use super::{wgpu::WGpu, buffer::Buffer, buffer::Vertex};

pub struct Pipeline
{
    pipe: wgpu::RenderPipeline,
}

impl Pipeline
{
    pub fn new(wgpu: &mut WGpu, buffer: &Buffer, name: &str, shader_path: &str) -> Pipeline
    {
        let shader_source = fs::read_to_string(shader_path).unwrap();

        let device = wgpu.device();
        let config = wgpu.surface_config();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&shader_source)).into(),
        });

        let layout_name = format!("{} Layout", name);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts: &[],
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
            pipe: render_pipeline
        }
    }

    pub fn get(&self) -> &wgpu::RenderPipeline
    {
        &self.pipe
    }
}

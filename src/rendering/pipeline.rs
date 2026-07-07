use std::borrow::Cow;

use wgpu::{ShaderModule, BindGroupLayout};

use crate::{console_log, render_item_impl_default, state::helper::render_item::RenderItem};

use super::{wgpu::WGpu, vertex_buffer::Vertex, texture::{self}, instance::Instance, skeleton::MAX_JOINTS, morph_target::MAX_MORPH_TARGETS, shadow::MAX_SHADOW_VIEWS};

pub struct Pipeline
{
    pub name: String,
    pub fragment_attachment: bool, // TODO: currently not implemented

    shader: ShaderModule,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl RenderItem for Pipeline
{
    render_item_impl_default!();
}

impl Pipeline
{
    pub fn new_std(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout], max_lights: u32, depth_stencil: bool, depth_compare: bool, depth_write: bool, fragment_attachment: bool, samples: u32, polygon_mode: wgpu::PolygonMode) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader
            let prepared_shader = Self::prepare_shader(shader_source, max_lights);
            shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
            {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&prepared_shader)).into(),
            });
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment,

            shader,
            pipeline: None,
        };

        pipe.create_std(wgpu, bind_group_layouts, depth_stencil, depth_compare, depth_write, fragment_attachment, samples, polygon_mode);

        pipe
    }

    /*
    pub fn new_occlusion_culling(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout]) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader
            shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
            {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)).into(),
            });
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment: false,

            shader,
            pipeline: None,
        };

        pipe.create_occlusion_culling(wgpu, bind_group_layouts);

        pipe
    }
    */

    pub fn new_shadow(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout], alpha_test: bool) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader (no lights needed - only joints/morph target amounts are replaced)
            let prepared_shader = Self::prepare_shader(shader_source, 0);
            shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
            {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&prepared_shader)).into(),
            });
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment: false,

            shader,
            pipeline: None,
        };

        pipe.create_shadow(wgpu, bind_group_layouts, alpha_test);

        pipe
    }

    pub fn create_shadow(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], alpha_test: bool)
    {
        // depth-only pass rendering into one shadow atlas layer
        // alpha_test: adds a fragment stage without color targets which discards cutout pixels
        // (alpha textured casters like leaves) - opaque casters stay vertex-only

        let device = wgpu.device();

        let layout_name = format!("{} Layout", self.name);
        let bind_group_layouts: Vec<Option<&BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(*l)).collect();
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts: &bind_group_layouts,
            ..Default::default()
        });

        let mut fragment_state = None;
        if alpha_test
        {
            fragment_state = Some(wgpu::FragmentState
            {
                module: &self.shader,
                entry_point: Some("fs_main"),
                targets: &[],
                compilation_options: Default::default(),
            });
        }

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some(&self.name),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers:
                &[
                    Vertex::desc(),
                    Instance::desc()
                ],
                compilation_options: Default::default(),
            },
            fragment: fragment_state,
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
            depth_stencil: Some(wgpu::DepthStencilState
            {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),

                // constant + slope-scaled bias against shadow acne
                bias: wgpu::DepthBiasState
                {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState
            {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(render_pipeline);
    }

    pub fn re_create_shadow(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], alpha_test: bool)
    {
        console_log!("recreating shadow pipeline");

        self.create_shadow(wgpu, bind_group_layouts, alpha_test);
    }

    pub fn new_depth_export(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout]) -> Pipeline
    {
        let shader;
        {
            let device = wgpu.device();

            // shader
            shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
            {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)).into(),
            });
        }

        // create pipe
        let mut pipe = Self
        {
            name: name.to_string(),
            fragment_attachment: false,

            shader,
            pipeline: None,
        };

        pipe.create_depth_export(wgpu, bind_group_layouts);

        pipe
    }

    pub fn prepare_shader(shader_source: &String, max_lights: u32) -> String
    {
        let mut shader = shader_source.clone();

        shader = shader.replace("[MAX_LIGHTS]", format!("{}", max_lights).as_str());
        shader = shader.replace("[MAX_JOINTS]", format!("{}", MAX_JOINTS).as_str());
        shader = shader.replace("[MAX_MORPH_TARGETS]", format!("{}", MAX_MORPH_TARGETS).as_str());
        shader = shader.replace("[MAX_SHADOW_VIEWS]", format!("{}", MAX_SHADOW_VIEWS).as_str());

        shader
    }

    pub fn get(&self) -> &wgpu::RenderPipeline
    {
        self.pipeline.as_ref().unwrap()
    }

    pub fn create_std(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], depth_stencil: bool, depth_compare: bool, depth_write: bool, fragment_attachment: bool, samples: u32, polygon_mode: wgpu::PolygonMode)
    {
        let device = wgpu.device();
        let config = wgpu.surface_config();

        let layout_name = format!("{} Layout", self.name);
        let bind_group_layouts: Vec<Option<&BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(*l)).collect();
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts: &bind_group_layouts,
            ..Default::default()
        });

        // front to back is the default
        let depth_compare_func = if depth_compare { wgpu::CompareFunction::Less } else { wgpu::CompareFunction::Always };

        let mut depth_stencil_state = None;
        if depth_stencil
        {
            depth_stencil_state = Some(wgpu::DepthStencilState
            {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(depth_compare_func),
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
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
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
                entry_point: Some("fs_main"),
                targets: fragment_targets,
                compilation_options: Default::default(),
            });
        }

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some(&self.name),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers:
                &[
                    Vertex::desc(),
                    Instance::desc()
                ],
                compilation_options: Default::default(),
            },
            fragment: fragment_state,
            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                //cull_mode: Some(wgpu::Face::Back), // backface culling
                cull_mode: None,
                polygon_mode,
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
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(render_pipeline);
    }

    pub fn re_create_std(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], depth_stencil: bool, depth_compare: bool, depth_write: bool, fragment_attachment: bool, samples: u32, polygon_mode: wgpu::PolygonMode)
    {
        console_log!("recreating pipeline");

        self.create_std(wgpu, bind_group_layouts, depth_stencil, depth_compare, depth_write, fragment_attachment, samples, polygon_mode);
    }

    /*
    pub fn re_create_occlusion_culling(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        console_log!("recreating occlusion culling pipeline");

        self.create_occlusion_culling(wgpu, bind_group_layouts);
    }

    pub fn create_occlusion_culling(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        let device = wgpu.device();

        let layout_name = format!("{} Layout", self.name);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts,
            push_constant_ranges: &[],
        });

        // Depth-only pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some(&self.name),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers: &[], // no vertex buffer, corners created in the shader
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState
            {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState
            {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            fragment: None, // kein Fragment-Shader = depth-only
            cache: None,
        });

        self.pipeline = Some(render_pipeline);
    }
    */

    pub fn create_depth_export(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        // for converting depth buffer R32Float texture

        let device = wgpu.device();

        let layout_name = format!("{} Layout", self.name);

        let bind_group_layouts: Vec<Option<&BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(*l)).collect();
        let depth_export_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some(layout_name.as_str()),
            bind_group_layouts: &bind_group_layouts,
            ..Default::default()
        });

        let depth_export_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some("depth_export_pipeline"),
            layout: Some(&depth_export_pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState
            {
                module: &self.shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState
                {
                    format: wgpu::TextureFormat::R32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),

            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },

            depth_stencil: None, // no depth test here
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(depth_export_pipeline);
    }

    pub fn re_create_depth_export(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        console_log!("recreating depth export pipeline");

        self.create_depth_export(wgpu, bind_group_layouts);
    }
}

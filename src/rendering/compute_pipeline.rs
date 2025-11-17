use std::borrow::Cow;

use wgpu::{BindGroupLayout, ShaderModule};

use crate::{console_log, render_item_impl_default, state::helper::render_item::RenderItem};

use super::wgpu::WGpu;

pub struct ComputePipeline
{
    pub name: String,

    shader: ShaderModule,
    pipeline: Option<wgpu::ComputePipeline>,
}

impl RenderItem for ComputePipeline
{
    render_item_impl_default!();
}

impl ComputePipeline
{
    pub fn new_hzb_downsample_compute(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout]) -> ComputePipeline
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

            shader,
            pipeline: None,
        };

        pipe.create_hzb_downsample_compute(wgpu, bind_group_layouts);

        pipe
    }

    pub fn get(&self) -> &wgpu::ComputePipeline
    {
        self.pipeline.as_ref().unwrap()
    }

    pub fn create_hzb_downsample_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        // for creating the hierarchical Z-buffer mip chain

        let device = wgpu.device();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            bind_group_layouts: bind_group_layouts,
            push_constant_ranges: &[],
            label: Some("HZB Downsample Pipeline Layout"),
        });

        let hzb_compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor
        {
            label: Some("HZB Downsample Pipeline"),
            layout: Some(&pipeline_layout),
            module: &self.shader,
            entry_point: Some("cs_downsample"),
            compilation_options: Default::default(),
            cache: None,
        });

        self.pipeline = Some(hzb_compute_pipeline);
    }

    pub fn re_create_hzb_downsample_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout])
    {
        console_log!("recreating hzb downsample compute pipeline");

        self.create_hzb_downsample_compute(wgpu, bind_group_layouts);
    }

}

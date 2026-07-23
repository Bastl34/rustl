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
    pub fn new_hzb_downsample_compute(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool) -> ComputePipeline
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

        pipe.create_hzb_downsample_compute(wgpu, bind_group_layouts, reverse_z);

        pipe
    }

    pub fn new_hzb_occlusion_check_compute(wgpu: &mut WGpu, name: &str, shader_source: &String, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool) -> ComputePipeline
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

        pipe.create_hzb_occlusion_check_compute(wgpu, bind_group_layouts, reverse_z);

        pipe
    }

    pub fn get(&self) -> &wgpu::ComputePipeline
    {
        self.pipeline.as_ref().unwrap()
    }

    pub fn create_hzb_downsample_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool)
    {
        // for creating the hierarchical Z-buffer mip chain

        let device = wgpu.device();

        let bind_group_layouts: Vec<Option<&BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(*l)).collect();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            bind_group_layouts: &bind_group_layouts,
            label: Some("HZB Downsample Pipeline Layout"),
            ..Default::default()
        });

        let hzb_compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor
        {
            label: Some("HZB Downsample Pipeline"),
            layout: Some(&pipeline_layout),
            module: &self.shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions
            {
                constants: &[("REVERSE_Z", if reverse_z { 1.0 } else { 0.0 })],
                ..Default::default()
            },
            cache: None,
        });

        self.pipeline = Some(hzb_compute_pipeline);
    }

    pub fn re_create_hzb_downsample_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool)
    {
        console_log!("recreating hzb downsample compute pipeline");

        self.create_hzb_downsample_compute(wgpu, bind_group_layouts, reverse_z);
    }

    pub fn create_hzb_occlusion_check_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool)
    {
        // tests the object bounding boxes against the hierarchical Z-buffer

        let device = wgpu.device();

        let bind_group_layouts: Vec<Option<&BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(*l)).collect();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            bind_group_layouts: &bind_group_layouts,
            label: Some("HZB Occlusion Check Pipeline Layout"),
            ..Default::default()
        });

        let hzb_compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor
        {
            label: Some("HZB Occlusion Check Pipeline"),
            layout: Some(&pipeline_layout),
            module: &self.shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions
            {
                constants: &[("REVERSE_Z", if reverse_z { 1.0 } else { 0.0 })],
                ..Default::default()
            },
            cache: None,
        });

        self.pipeline = Some(hzb_compute_pipeline);
    }

    pub fn re_create_hzb_occlusion_check_compute(&mut self, wgpu: &mut WGpu, bind_group_layouts: &[&BindGroupLayout], reverse_z: bool)
    {
        console_log!("recreating hzb occlusion check compute pipeline");

        self.create_hzb_occlusion_check_compute(wgpu, bind_group_layouts, reverse_z);
    }

}

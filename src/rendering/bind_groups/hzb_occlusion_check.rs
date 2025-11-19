#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bounding_boxes::BoundingBoxesBuffer, camera::CameraBuffer, hzb_cull_buffer::HZBCullBuffer, texture::Texture, visibility::VisibilityBuffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct HZBOcclusionCheckBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for HZBOcclusionCheckBindGroup
{
    render_item_impl_default!();
}

impl HZBOcclusionCheckBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("HZB Occlusion Check"),
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None, // optional
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry
                {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry
                {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                 wgpu::BindGroupLayoutEntry
                 {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, cam_buffer: &CameraBuffer, visibility: &VisibilityBuffer, bounding_boxes: &BoundingBoxesBuffer, hzb_cull_buffer: &HZBCullBuffer, hzb_texture: &Texture) -> HZBOcclusionCheckBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        let bind_group_name = format!("{}_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: bounding_boxes.get_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 1,
                    resource: visibility.get_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&hzb_texture.get_view()),
                },
                wgpu::BindGroupEntry
                {
                    binding: 3,
                    resource: cam_buffer.get_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 4,
                    resource: hzb_cull_buffer.get_buffer().as_entire_binding(),
                },
            ],
            label: Some(bind_group_name.as_str()),
        });

        HZBOcclusionCheckBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}
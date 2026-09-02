#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bounding_boxes::BoundingBoxesBuffer, camera::CameraBuffer, draw_slots::{DrawSlotsBuffer, IndirectArgsBuffers}, hzb_cull_buffer::HZBCullBuffer, texture::Texture, visibility::VisibilityBuffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct HZBOcclusionCheckBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for HZBOcclusionCheckBindGroup
{
    render_item_impl_default!();
}

fn storage_buffer_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry
{
    wgpu::BindGroupLayoutEntry
    {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer
        {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_buffer_entry(binding: u32) -> wgpu::BindGroupLayoutEntry
{
    wgpu::BindGroupLayoutEntry
    {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer
        {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
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
                // bounding boxes
                storage_buffer_entry(0, true),

                // visibility (current frame)
                storage_buffer_entry(1, false),

                // hzb texture
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

                // camera
                uniform_buffer_entry(3),

                // cull params
                uniform_buffer_entry(4),

                // visibility (previous frame)
                storage_buffer_entry(5, true),

                // draw slot metadata
                storage_buffer_entry(6, true),

                // indirect args (visible)
                storage_buffer_entry(7, false),

                // indirect args (newly visible)
                storage_buffer_entry(8, false),
            ],
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, cam_buffer: &CameraBuffer, visibility: &VisibilityBuffer, bounding_boxes: &BoundingBoxesBuffer, hzb_cull_buffer: &HZBCullBuffer, hzb_texture: &Texture, draw_slots: &DrawSlotsBuffer, indirect_args: &IndirectArgsBuffers) -> HZBOcclusionCheckBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        // the per-mip views of the hzb texture are for the downsample passes - the occlusion
        // check needs the whole mip chain (textureNumLevels/mip sampling in the shader)
        let hzb_full_view = hzb_texture.get_texture().create_view(&wgpu::TextureViewDescriptor::default());

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
                    resource: visibility.get_storage_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&hzb_full_view),
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
                wgpu::BindGroupEntry
                {
                    binding: 5,
                    resource: visibility.get_prev_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 6,
                    resource: draw_slots.get_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 7,
                    resource: indirect_args.args_visible.as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 8,
                    resource: indirect_args.args_new.as_entire_binding(),
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

#![allow(dead_code)]

use wgpu::{BindGroup, BindGroupLayout};

use crate::{render_item_impl_default, rendering::{texture::Texture, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct HZBDownsampleBindGroup
{
    pub bind_groups: Vec<BindGroup>
}

impl RenderItem for HZBDownsampleBindGroup
{
    render_item_impl_default!();
}

impl HZBDownsampleBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("HZB bind group layout"),
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
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
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture
                    {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        //format: wgpu::TextureFormat::Rgba32Float,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, hzb_depth_texture: &Texture) -> HZBDownsampleBindGroup
    {
        let mips = hzb_depth_texture.get_views();

        let mut hzb_bind_groups = vec![];

        let bind_group_layout = Self::bind_layout(wgpu);

        for level in 1..mips.iter().count()
        {
            let src_view = &mips[level - 1]; // previous
            let dst_view = &mips[level];     // next

            let bg = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
            {
                label: Some(name),
                layout: &bind_group_layout,
                entries:
                &[
                    wgpu::BindGroupEntry
                    {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src_view),
                    },
                    wgpu::BindGroupEntry
                    {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(dst_view),
                    },
                ],
            });

            hzb_bind_groups.push(bg);
        }

        HZBDownsampleBindGroup
        {
            bind_groups: hzb_bind_groups
        }
    }
}
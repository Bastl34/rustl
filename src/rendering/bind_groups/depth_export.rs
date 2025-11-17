#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bind_groups::uniform, morph_target::MorphTarget, skeleton::SkeletonBuffer, texture::Texture, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct DepthExportBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for DepthExportBindGroup
{
    render_item_impl_default!();
}

impl DepthExportBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("depth_export_binding_group_layout"),
            entries:
            &[
                // Binding 0: depth texture
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },

                // Binding 1: sampler
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        bind_group_layout
    }

    pub fn new(wgpu: &mut WGpu, name: &str, depth_texture: &Texture) -> DepthExportBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        // TODO: find better place for the sampler
        let sampler = wgpu.device().create_sampler(&wgpu::SamplerDescriptor
        {
            label: Some("depth_export_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: None, // important: no compare
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            border_color: None,
            ..Default::default()
        });

        let bind_group_name = format!("{} depth_texture_export_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_texture.get_view()),
                },
                wgpu::BindGroupEntry
                {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some(bind_group_name.as_str()),
        });

        DepthExportBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}
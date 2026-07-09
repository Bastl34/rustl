#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bind_groups::uniform, camera::CameraBuffer, light::LightBuffer, scene::Scene, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct LightCamSceneBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for LightCamSceneBindGroup
{
    render_item_impl_default!();
}

impl LightCamSceneBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true),
                uniform::uniform_bind_group_layout_entry(1, true, true),
                uniform::uniform_bind_group_layout_entry(2, true, true),
                uniform::uniform_bind_group_layout_entry(3, true, true),

                // shadow view matrices
                uniform::uniform_bind_group_layout_entry(4, false, true),

                // shadow atlas (depth texture array)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },

                // shadow comparison sampler (hardware PCF)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },

                // blurred ssao result (read 1:1 per pixel via textureLoad - no sampler needed)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
            ],
            label: Some("light_cam_scene_bind_group_layout"),
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, cam_buffer: &CameraBuffer, light_buffer: &LightBuffer, scene_buffer: &Scene) -> LightCamSceneBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        let bind_group_name = format!("{} light_camera_scene_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry { binding: 0, resource: cam_buffer.get_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: scene_buffer.get_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: light_buffer.get_amount_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: light_buffer.get_lights_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: scene_buffer.shadow.get_views_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(scene_buffer.shadow.get_atlas_view()) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(scene_buffer.shadow.get_sampler()) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(scene_buffer.ssao_blur_texture.get_view()) },
            ],
            label: Some(bind_group_name.as_str()),
        });

        LightCamSceneBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}
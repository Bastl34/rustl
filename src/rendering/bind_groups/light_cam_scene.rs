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
        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true),
                uniform::uniform_bind_group_layout_entry(1, true, true),
                uniform::uniform_bind_group_layout_entry(2, true, true),
                uniform::uniform_bind_group_layout_entry(3, true, true),
            ],
            label: Some("light_cam_scene_bind_group_layout"),
        });

        bind_group_layout
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
                wgpu::BindGroupEntry { binding: 3, resource: light_buffer.get_lights_buffer().as_entire_binding() }
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
use wgpu::{BindGroupLayout, BindGroup};

use crate::{rendering::{light::LightBuffer, camera::CameraBuffer, wgpu::WGpu, uniform, scene::Scene}, state::helper::render_item::RenderItem, render_item_impl_default};

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
                uniform::uniform_bind_group(0, &cam_buffer.get_buffer()),
                uniform::uniform_bind_group(1, &scene_buffer.get_buffer()),
                uniform::uniform_bind_group(2, &light_buffer.get_amount_buffer()),
                uniform::uniform_bind_group(3, &light_buffer.get_lights_buffer()),
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
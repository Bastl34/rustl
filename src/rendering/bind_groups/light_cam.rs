use wgpu::{BindGroupLayout, BindGroup};

use crate::{rendering::{light::LightBuffer, camera::CameraBuffer, wgpu::WGpu, uniform}, state::helper::render_item::RenderItem, render_item_impl_default};

pub struct LightCamBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for LightCamBindGroup
{
    render_item_impl_default!();
}

impl LightCamBindGroup
{
    pub fn new(wgpu: &mut WGpu, name: &str, cam_buffer: &CameraBuffer, light_buffer: &LightBuffer) -> LightCamBindGroup
    {
        let bind_group_layout_name = format!("{} light_cam_bind_group_layout", name);
        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true),
                uniform::uniform_bind_group_layout_entry(1, true, true),
                uniform::uniform_bind_group_layout_entry(2, true, true),
            ],
            label: Some(bind_group_layout_name.as_str()),
        });

        let bind_group_name = format!("{} light_camera_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &cam_buffer.get_buffer()),
                uniform::uniform_bind_group(1, &light_buffer.get_amount_buffer()),
                uniform::uniform_bind_group(2, &light_buffer.get_lights_buffer()),
            ],
            label: Some(bind_group_name.as_str()),
        });

        LightCamBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}
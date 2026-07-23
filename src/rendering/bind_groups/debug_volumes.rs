use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bind_groups::{storage, uniform}, camera::CameraBuffer, debug_volumes::DebugVolumesBuffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct DebugVolumesBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for DebugVolumesBindGroup
{
    render_item_impl_default!();
}

impl DebugVolumesBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                // camera
                uniform::uniform_bind_group_layout_entry(0, true, false),

                // debug volumes (vertex pulling)
                storage::storage_bind_group_layout_entry(1, true, false, true),
            ],
            label: Some("debug_volumes_bind_group_layout"),
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, cam_buffer: &CameraBuffer, volumes_buffer: &DebugVolumesBuffer) -> DebugVolumesBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        let bind_group_name = format!("{} debug_volumes_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry { binding: 0, resource: cam_buffer.get_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: volumes_buffer.get_buffer().as_entire_binding() },
            ],
            label: Some(bind_group_name.as_str()),
        });

        DebugVolumesBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}

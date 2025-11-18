use wgpu::{BindGroup, util::DeviceExt};

use crate::{render_item_impl_default, rendering::{bind_groups::single_binding_group::SingleBindingBindGroup, wgpu::WGpu}, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 64 * 1024; // 64k entries

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Visibility
{
    pub object_id: u64,
    pub visible: u32,
    pub _padding: u32,
}


pub struct VisibilityBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize,
}

impl RenderItem for VisibilityBuffer
{
    render_item_impl_default!();
}

impl VisibilityBuffer
{
    pub fn new(wgpu: &mut WGpu, num_objects: usize) -> Self
    {
        let buffer_size = num_objects.next_power_of_two().max(MIN_SIZE);

        let visibility_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("visibility buffer"),
            size: (std::mem::size_of::<Visibility>() * buffer_size) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        //let bind_group = SingleBindingBindGroup::new(wgpu, "occlusion culling", &buffer, true, false, true, true);

        Self
        {
            buffer: visibility_buffer,
            buffer_size,
        }
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}

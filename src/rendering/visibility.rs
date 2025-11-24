
use crate::{render_item_impl_default, rendering::wgpu::WGpu, state::{helper::render_item::RenderItem, scene::camera::Visibility}};

const MIN_SIZE: usize = 64 * 1024; // 64k entries

pub struct VisibilityBuffer
{
    pub storage_buffer: wgpu::Buffer,
    pub readback_buffer: wgpu::Buffer,
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

        let readback_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("visibility readback buffer"),
            size: (std::mem::size_of::<Visibility>() * buffer_size) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self
        {
            storage_buffer: visibility_buffer,
            readback_buffer,
            buffer_size,
        }
    }

    pub fn get_storage_buffer(&self) -> &wgpu::Buffer
    {
        &self.storage_buffer
    }

    pub fn get_readback_buffer(&self) -> &wgpu::Buffer
    {
        &self.readback_buffer
    }

    pub fn copy_to_readback_buffer(&self, encoder: &mut wgpu::CommandEncoder)
    {
        encoder.copy_buffer_to_buffer(&self.storage_buffer, 0, &self.readback_buffer, 0, (std::mem::size_of::<Visibility>() * self.buffer_size) as u64);
    }
}

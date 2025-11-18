use gltf::buffer;
use nalgebra::Point3;
use wgpu::{BindGroup, util::DeviceExt};

use crate::{render_item_impl_default, rendering::{bind_groups::single_binding_group::SingleBindingBindGroup, helper::buffer::create_empty_buffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 64 * 1024; // 64k entries

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoundingBox
{
    pub object_id: u64,
    pub min: [f32; 4],
    pub max: [f32; 4],
    // pub model_transform: [[f32; 4]; 4],
}

pub struct BoundingBoxesBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize
}

impl RenderItem for BoundingBoxesBuffer
{
    render_item_impl_default!();
}

impl BoundingBoxesBuffer
{
    pub fn new(wgpu: &mut WGpu) -> Self
    {
        let mut bbox_buffers =  Self
        {
            buffer: create_empty_buffer(wgpu),
            buffer_size: 0
        };

        bbox_buffers.update(wgpu, &vec![]);

        bbox_buffers
    }

    pub fn update(&mut self, wgpu: &mut WGpu, buffer_data: &Vec<BoundingBox>) -> bool
    {
        let new_buffer_size = buffer_data.len().next_power_of_two().max(MIN_SIZE);

        let buffer_size_changed = new_buffer_size != self.buffer_size;

        if new_buffer_size > self.buffer_size
        {
            // recreate buffer
            self.buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
            {
                label: Some("Bounding Boxes Buffer"),
                size: (std::mem::size_of::<BoundingBox>() * new_buffer_size) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.buffer_size = new_buffer_size;

             // TODO bind group
        }

        let mut padded_data = Vec::with_capacity(new_buffer_size);
        padded_data.extend_from_slice(buffer_data);

        // fill with dummy padding data
        let dummy_bbox = BoundingBox
        {
            object_id: 0,
            min: [0.0, 0.0, 0.0, 0.0],
            max: [0.0, 0.0, 0.0, 0.0],
        };

        for _ in padded_data.len()..new_buffer_size
        {
            padded_data.push(dummy_bbox);
        }

        wgpu.queue_mut().write_buffer
        (
            &self.buffer,
            0,
            bytemuck::cast_slice(&padded_data),
        );

        buffer_size_changed
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}

use nalgebra::Point3;

use crate::{render_item_impl_default, rendering::{helper::buffer::create_empty_buffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 1024; // entries (buffer grows on demand)

// bit 0: object takes part in the occlusion test (see occlusion_hzb_check.wgsl)
pub const BOUNDING_BOX_FLAG_OCCLUSION_TEST: u32 = 1;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoundingBox
{
    pub min: [f32; 4],
    pub max: [f32; 4],
    pub object_id: u32,
    pub flags: u32,
    pub slot_start: u32, // first draw slot of the object
    pub slot_count: u32, // number of draw slots (one per mesh)
}

impl BoundingBox
{
    pub fn new(object_id: u32, min: &Point3<f32>, max: &Point3<f32>, flags: u32, slot_start: u32, slot_count: u32) -> Self
    {
        Self
        {
            object_id: object_id as u32,
            min: [min.x, min.y, min.z, 0.0],
            max: [max.x, max.y, max.z, 0.0],
            flags,
            slot_start,
            slot_count,
        }
    }
}

pub struct BoundingBoxesBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize
}

impl RenderItem for BoundingBoxesBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.buffer.size()
    }
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

    // returns true if the gpu buffer was recreated (bind groups have to be recreated)
    pub fn update(&mut self, wgpu: &mut WGpu, buffer_data: &Vec<BoundingBox>) -> bool
    {
        let new_buffer_size = buffer_data.len().next_power_of_two().max(MIN_SIZE);

        let recreated = new_buffer_size > self.buffer_size;

        if recreated
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
        }

        // only write the used entries - the compute shader never reads past num_objects
        // (this runs on every instance change, so avoid uploading the full padded buffer)
        if !buffer_data.is_empty()
        {
            wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(buffer_data));
        }

        recreated
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}

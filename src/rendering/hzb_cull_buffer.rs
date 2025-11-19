use wgpu::util::DeviceExt;

use crate::{render_item_impl_default, rendering::wgpu::WGpu, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 64 * 1024; // 64k entries

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HZBCullParams
{
    num_objects : u32,
    _pad0 : u32,
    _pad1 : u32,
    _pad2 : u32,
}

pub struct HZBCullBuffer
{
    pub buffer: wgpu::Buffer,
    pub num_objects: usize,
}

impl RenderItem for HZBCullBuffer
{
    render_item_impl_default!();
}

impl HZBCullBuffer
{
    pub fn new(wgpu: &mut WGpu) -> Self
    {
        let cull_params = HZBCullParams
        {
            num_objects: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };

        let buffer = wgpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor
        {
            label: Some("HZB Cull Params"),
            contents: bytemuck::bytes_of(&cull_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self
        {
            buffer,
            num_objects: 0,
        }
    }

    pub fn update(&mut self, wgpu: &mut WGpu, num_objects: u32)
    {
        let cull_params = HZBCullParams
        {
            num_objects,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::bytes_of(&cull_params));
        self.num_objects = num_objects as usize;
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}

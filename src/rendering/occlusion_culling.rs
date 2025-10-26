#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoundingBoxInstance
{
    pub min: [f32; 4],
    pub max: [f32; 4],
    // pub model_transform: [[f32; 4]; 4],
}

use nalgebra::Point3;
use wgpu::util::DeviceExt;

use crate::{render_item_impl_default, rendering::wgpu::WGpu, state::helper::render_item::RenderItem};

pub struct OcclusionCullingBuffer
{
    pub buffer: wgpu::Buffer
}

impl RenderItem for OcclusionCullingBuffer
{
    render_item_impl_default!();
}

impl OcclusionCullingBuffer
{
    pub fn new(wgpu: &WGpu, min: Point3<f32>, max: Point3<f32>) -> Self
    {
        let buffer = wgpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor
        {
            label: Some("Occlusion Culling Buffer"),
            contents: bytemuck::cast_slice(&[BoundingBoxInstance
            {
                min: [min.x, min.y, min.z, 0.0],
                max: [max.x, max.y, max.z, 0.0],
            }]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        Self
        {
            buffer
        }
    }
}

use wgpu::Buffer;
use wgpu::util::DeviceExt;

use crate::state::scene::instance::Instance;

use super::wgpu::WGpu;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw
{
    pub model: [[f32; 4]; 4],
}

impl InstanceRaw
{
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a>
    {
        use std::mem;
        wgpu::VertexBufferLayout
        {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes:
            &[
                // matrix needs to be split into 4 times float32x4
                wgpu::VertexAttribute
                {
                    offset: 0,
                    shader_location: 5, //TODO proper shader location
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub fn instances_to_buffer(wgpu: &mut WGpu, instances: &Vec<Instance>) -> Buffer
{
    let instance_data = instances.iter().map(Instance::get_transform).collect::<Vec<_>>();

    wgpu.device().create_buffer_init
    (
        &wgpu::util::BufferInitDescriptor
        {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        }
    )
}

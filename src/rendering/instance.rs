use wgpu::Buffer;
use wgpu::util::DeviceExt;

use crate::state::scene::instance::Instance;

use super::wgpu::WGpu;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw
{
    pub transform: [[f32; 4]; 4],
    pub normal: [[f32; 3]; 3],
}

impl InstanceRaw
{
    const SHADER_LOCATION_START: u32 = 5;

    pub fn desc() -> wgpu::VertexBufferLayout<'static>
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
                    shader_location: Self::SHADER_LOCATION_START,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 4,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 19]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 5,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 22]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub fn instances_to_buffer(wgpu: &mut WGpu, instances: &Vec<Box<Instance>>) -> Buffer
{
    //let instance_data = instances.iter().map(Instance::get_transform).collect::<Vec<_>>();

    let instance_data = instances.iter().map(|instance|
    {
        let (transform, normal) = instance.get_transform();

        InstanceRaw
        {
            transform: transform.into(),
            normal: normal.into()
        }
    }).collect::<Vec<_>>();

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

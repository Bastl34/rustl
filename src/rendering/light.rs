
// Due to uniforms requiring 16 byte (4 float) spacing, its needed to use pading
// position: [f32; 3]
// __padding: [f32; 1]
// --> 16
// https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-blinn-phong-model
// https://www.w3.org/TR/WGSL/#alignment-and-size

use nalgebra::{Vector3, Point3};
use wgpu::util::DeviceExt;

use crate::{state::{helper::render_item::RenderItem, scene::light::Light}, render_item_impl_default};

use super::wgpu::WGpu;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform
{
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub intensity: f32,
    _padding: [f32; 3],
}

/*
impl RenderItem for LightUniform
{
    render_item_impl_default!();
}
*/

impl LightUniform
{
    pub fn new(position: Point3<f32>, color: Vector3<f32>, intensity: f32) -> Self
    {
        Self
        {
            position: [position.x, position.y, position.z, 1.0],
            color: [color.x, color.y, color.z, 1.0],
            intensity,
            _padding: [0.0, 0.0, 0.0]
        }
    }
}

pub struct LightBuffer
{
    pub name: String,
    buffer: wgpu::Buffer,
}

impl RenderItem for LightBuffer
{
    render_item_impl_default!();
}

impl LightBuffer
{
    pub fn new(wgpu: &mut WGpu, light: &Light) -> LightBuffer
    {
        let empty_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("Empty Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut buffer = LightBuffer
        {
            name: light.name(),
            buffer: empty_buffer
        };

        buffer.to_buffer(wgpu, light);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, light: &Light)
    {
        let light_uniform = LightUniform::new(light.pos, light.color, light.intensity);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[light_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, light: &Light)
    {
        let light_uniform = LightUniform::new(light.pos, light.color, light.intensity);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[light_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}
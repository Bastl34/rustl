
// Due to uniforms requiring 16 byte (4 float) spacing, its needed to use pading
// position: [f32; 3]
// __padding: [f32; 1]
// --> 16
// https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-blinn-phong-model
// https://www.w3.org/TR/WGSL/#alignment-and-size

use std::mem;

use nalgebra::{Vector3, Point3};
use wgpu::util::DeviceExt;

use crate::{state::{helper::render_item::RenderItem, scene::light::{Light, LightItem}}, render_item_impl_default};

use super::{wgpu::WGpu, helper::buffer::create_empty_buffer};

const MAX_LIGHTS: usize = 10;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform
{
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub intensity: f32,
    _padding: [f32; 3],
}

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

pub struct LightsUniform
{
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub intensity: f32,
    _padding: [f32; 3],
}

impl LightsUniform
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

    lights_amount: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
}

impl RenderItem for LightBuffer
{
    render_item_impl_default!();
}

impl LightBuffer
{
    pub fn new(wgpu: &mut WGpu, name: String, lights: &Vec<LightItem>) -> LightBuffer
    {
        let mut buffer = LightBuffer
        {
            name: name,
            lights_amount: create_empty_buffer(wgpu),
            lights_buffer: create_empty_buffer(wgpu),
        };

        buffer.to_buffer(wgpu, lights);

        buffer
    }

    fn uniform_size() -> wgpu::BufferAddress
    {
        (MAX_LIGHTS * mem::size_of::<LightsUniform>()) as wgpu::BufferAddress
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, lights: &Vec<LightItem>)
    {
        /*
        let lights_buffer_data = vec![];
        for light in lights
        {
            lights_buffer_data.push(LightUniform::new(light.pos, light.color, light.intensity));
        }
        */

        let amount = lights.len() as u32;
        self.lights_amount = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some("lights amount buffer"),
                contents: bytemuck::bytes_of(&amount),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        self.lights_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some(&self.name),
            size: Self::uniform_size(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        for (i, light) in lights.iter().enumerate()
        {
            let data = LightUniform::new(light.pos, light.color, light.intensity);

            wgpu.queue_mut().write_buffer
            (
                &self.lights_buffer,
                (i * mem::size_of::<LightsUniform>()) as wgpu::BufferAddress,
                bytemuck::bytes_of(&data),
            );
        }

        /*
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
        */
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, light: &Light, index: usize)
    {
        let data = LightUniform::new(light.pos, light.color, light.intensity);

        wgpu.queue_mut().write_buffer
        (
            &self.lights_buffer,
            (index * mem::size_of::<LightsUniform>()) as wgpu::BufferAddress,
            bytemuck::bytes_of(&data),
        );
    }

    pub fn get_amount_buffer(&self) -> &wgpu::Buffer
    {
        &self.lights_amount
    }

    pub fn get_lights_buffer(&self) -> &wgpu::Buffer
    {
        &self.lights_buffer
    }
}
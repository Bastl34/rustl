
// Due to uniforms requiring 16 byte (4 float) spacing, its needed to use pading
// position: [f32; 3]
// __padding: [f32; 1]
// --> 16
// https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-blinn-phong-model
// https://www.w3.org/TR/WGSL/#alignment-and-size

use std::{mem, cell::RefCell};

use colored::Colorize;
use nalgebra::{Vector3, Point3};

use crate::{state::{helper::render_item::RenderItem, scene::light::{Light, LightItem, LightType}}, render_item_impl_default, helper::{change_tracker::ChangeTracker, math::approx_zero_vec3}};

use super::{wgpu::WGpu, helper::buffer::create_empty_buffer};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform
{
    pub position: [f32; 4],
    pub dir: [f32; 4],
    pub color: [f32; 4],
    pub intensity: f32,
    pub light_type: u32,
    pub max_angle: f32,
    pub distance_based_intensity: u32,
    _padding: [f32; 0],
}

impl LightUniform
{
    pub fn new(light_type: LightType, position: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32, max_angle: f32, distance_based_intensity: bool) -> Self
    {
        let l_type;
        match light_type
        {
            LightType::Directional => l_type = 0,
            LightType::Point => l_type = 1,
            LightType::Spot => l_type = 2,
        };

        let dist_based_intensity; if distance_based_intensity { dist_based_intensity = 1; } else { dist_based_intensity = 0; }

        let dir_normalized;
        if approx_zero_vec3(&dir)
        {
            dir_normalized = Vector3::new(0.0, -1.0, 0.0);
        }
        else
        {
            dir_normalized = dir.normalize();
        }

        Self
        {
            position: [position.x, position.y, position.z, 1.0],
            dir: [dir_normalized.x, dir_normalized.y, dir_normalized.z, 1.0],
            color: [color.x, color.y, color.z, 1.0],
            intensity,
            light_type: l_type,
            max_angle,
            distance_based_intensity: dist_based_intensity,
            _padding: []
        }
    }
}

pub struct LightBuffer
{
    pub name: String,

    max_lights: usize,

    lights_amount: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
}

impl RenderItem for LightBuffer
{
    render_item_impl_default!();
}

impl LightBuffer
{
    pub fn new(wgpu: &mut WGpu, name: String, lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: u32) -> LightBuffer
    {
        let mut buffer = LightBuffer
        {
            name: name,
            max_lights: max_lights as usize,
            lights_amount: create_empty_buffer(wgpu),
            lights_buffer: create_empty_buffer(wgpu),
        };


        buffer.create_buffer(wgpu);
        buffer.to_buffer(wgpu, lights);

        buffer
    }

    fn uniform_size(max_lights: usize) -> wgpu::BufferAddress
    {
        (max_lights * mem::size_of::<LightUniform>()) as wgpu::BufferAddress
    }

    pub fn create_buffer(&mut self, wgpu: &mut WGpu)
    {
        self.lights_amount = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("lights amount buffer"),
            size: mem::size_of::<u32>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.lights_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some(&self.name),
            size: Self::uniform_size(self.max_lights),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, lights: &Vec<RefCell<ChangeTracker<LightItem>>>)
    {
        let amount = lights.len().min(self.max_lights) as u32;

        wgpu.queue_mut().write_buffer
        (
            &self.lights_amount,
            0,
            bytemuck::bytes_of(&amount),
        );

        for (i, light) in lights.iter().enumerate()
        {
            if i + 1 > self.max_lights
            {
                let warning = format!("only {} lights are supported", self.max_lights);
                println!("{}", warning.bright_yellow());
                break;
            }

            let light = light.borrow();
            let light = light.get_ref();
            let data = LightUniform::new(light.light_type, light.pos, light.dir, light.color, light.intensity, light.max_angle, light.distance_based_intensity);

            wgpu.queue_mut().write_buffer
            (
                &self.lights_buffer,
                (i * mem::size_of::<LightUniform>()) as wgpu::BufferAddress,
                bytemuck::bytes_of(&data),
            );
        }
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, light: &Light, index: usize)
    {
        if index + 1 > self.max_lights
        {
            let warning = format!("only {} lights are supported", self.max_lights);
            println!("{}", warning.bright_yellow());
            return;
        }

        let data = LightUniform::new(light.light_type, light.pos, light.dir, light.color, light.intensity, light.max_angle, light.distance_based_intensity);

        wgpu.queue_mut().write_buffer
        (
            &self.lights_buffer,
            (index * mem::size_of::<LightUniform>()) as wgpu::BufferAddress,
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
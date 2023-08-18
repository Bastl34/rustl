use std::borrow::Borrow;
use std::cell::{Cell, RefCell};
use std::mem;

use colored::Colorize;
use wgpu::util::DeviceExt;

use crate::helper::change_tracker::ChangeTracker;
use crate::render_item_impl_default;
use crate::state::helper::render_item::RenderItem;
use crate::state::scene::instance::InstanceItem;

use super::helper::buffer::create_empty_buffer;
use super::wgpu::WGpu;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance
{
    pub transform: [[f32; 4]; 4],
    pub normal: [[f32; 3]; 3],
    pub alpha: f32,
    pub highlight: f32,
}

impl Instance
{
    const SHADER_LOCATION_START: u32 = 5; // based on vertex input

    pub fn desc() -> wgpu::VertexBufferLayout<'static>
    {
        wgpu::VertexBufferLayout
        {
            array_stride: mem::size_of::<Instance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes:
            &[
                // matrix needs to be split into 4 times float32x4
                // ***** transformation *****
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

                // ***** normal *****
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

                // ***** alpha *****
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 25]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 7,
                    format: wgpu::VertexFormat::Float32,
                },

                // ***** highlight *****
                wgpu::VertexAttribute
                {
                    offset: mem::size_of::<[f32; 26]>() as wgpu::BufferAddress,
                    shader_location: Self::SHADER_LOCATION_START + 8,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

pub struct InstanceBuffer
{
    pub name: String,
    count: u32,
    buffer: wgpu::Buffer,
}

impl RenderItem for InstanceBuffer
{
    render_item_impl_default!();
}

impl InstanceBuffer
{
    pub fn new(wgpu: &mut WGpu, name: &str, instances: &Vec<RefCell<ChangeTracker<InstanceItem>>>) -> InstanceBuffer
    {
        let mut instance_buffer = InstanceBuffer
        {
            name: name.to_string(),
            count: instances.len() as u32,
            buffer: create_empty_buffer(wgpu)
        };

        instance_buffer.to_buffer(wgpu, instances);

        instance_buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, instances: &Vec<RefCell<ChangeTracker<InstanceItem>>>)
    {
        let instance_data = instances.iter().map(|instance|
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();
            let (transform, normal) = instance.get_transform();
            let alpha = instance.get_alpha();

            Instance
            {
                transform: transform.into(),
                normal: normal.into(),
                alpha: alpha,
                highlight: f32::from(instance.highlight)
            }
        }).collect::<Vec<_>>();

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&instance_data),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }
        );

        self.count = instances.len() as u32;
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, instance: &InstanceItem, index: usize)
    {
        if index + 1 > self.count as usize
        {
            let warning = format!("index {} out of range {} lights are supported", index, self.count);
            println!("{}", warning.bright_yellow());
            return;
        }

        let (transform, normal) = instance.get_transform();
        let alpha = instance.get_alpha();

        let data = Instance
        {
            transform: transform.into(),
            normal: normal.into(),
            alpha: alpha,
            highlight: f32::from(instance.highlight)
        };

        wgpu.queue_mut().write_buffer
        (
            &self.buffer,
            (index * mem::size_of::<Instance>()) as wgpu::BufferAddress,
            bytemuck::bytes_of(&data),
        );
    }

    pub fn update_buffer_range(&mut self, wgpu: &mut WGpu, instances: &Vec<RefCell<ChangeTracker<InstanceItem>>>, range: std::ops::Range<usize>)
    {
        if range.start + 1 > self.count as usize
        {
            let warning = format!("index {} out of range {} lights are supported", range.start, self.count);
            println!("{}", warning.bright_yellow());
            return;
        }

        let slice = &instances[range.clone()];

        let instance_data = slice.iter().map(|instance|
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();
            let (transform, normal) = instance.get_transform();
            let alpha = instance.get_alpha();

            Instance
            {
                transform: transform.into(),
                normal: normal.into(),
                alpha: alpha,
                highlight: f32::from(instance.highlight)
            }
        }).collect::<Vec<_>>();

        wgpu.queue_mut().write_buffer
        (
            &self.buffer,
            (range.start * mem::size_of::<Instance>()) as wgpu::BufferAddress,
            bytemuck::cast_slice(&instance_data),
        );
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn get_count(&self) -> u32
    {
        self.count
    }
}

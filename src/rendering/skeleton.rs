use nalgebra::Matrix4;
use wgpu::{BindGroup, util::DeviceExt};
use colored::Colorize;

use crate::{state::helper::render_item::RenderItem, render_item_impl_default};

use super::{wgpu::WGpu, helper::buffer::create_empty_buffer};


pub const MAX_JOINTS: usize = 256; // 128?!

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkeletonUniform
{
    pub transform: [[[f32; 4]; 4]; MAX_JOINTS],
    pub joints_amount: u32,

    _padding: [f32; 3],
}

impl SkeletonUniform
{
    pub fn new(joint_matrices: &Vec<Matrix4<f32>>) -> Self
    {
        let mut transform = [[[0.0; 4]; 4]; MAX_JOINTS];

        for (i, joint) in joint_matrices.iter().enumerate()
        {
            if i + 1 > MAX_JOINTS
            {
                let error = "MAX_JOINTS reached - try to increase MAX_JOINTS or reduce joints in skeleton".to_string();
                println!("{}", error.red());
                break;
            }

            transform[i] = joint.clone().into();
        }

        SkeletonUniform
        {
            transform: transform,
            joints_amount: joint_matrices.len() as u32,

            _padding: [0.0, 0.0, 0.0]
        }
    }
}

pub struct SkeletonBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,

    pub bind_group: Option<BindGroup>
}

impl RenderItem for SkeletonBuffer
{
    render_item_impl_default!();
}

impl SkeletonBuffer
{
    pub fn new(wgpu: &mut WGpu, name: &str, joint_matrices: &Vec<Matrix4<f32>>) -> SkeletonBuffer
    {
        let empty_buffer = create_empty_buffer(wgpu);

        let mut buffer = SkeletonBuffer
        {
            name: name.to_string(),
            buffer: empty_buffer,
            bind_group: None
        };

        buffer.to_buffer(wgpu, joint_matrices);

        buffer
    }

    pub fn empty(wgpu: &mut WGpu, ) -> SkeletonBuffer
    {
        let empty_buffer = create_empty_buffer(wgpu);

        let mut buffer = SkeletonBuffer
        {
            name: "empty".to_string(),
            buffer: empty_buffer,
            bind_group: None
        };

        let joint_matrices: Vec<Matrix4<f32>> = vec![];

        buffer.to_buffer(wgpu, &joint_matrices);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, joint_matrices: &Vec<Matrix4<f32>>)
    {
        let skeleton_uniform = SkeletonUniform::new(joint_matrices);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[skeleton_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, joint_matrices: &Vec<Matrix4<f32>>)
    {
        let skeleton_uniform = SkeletonUniform::new(joint_matrices);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[skeleton_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}
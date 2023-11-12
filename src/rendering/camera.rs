use nalgebra::{Point3, Matrix4};
use wgpu::util::DeviceExt;

use crate::{state::{helper::render_item::RenderItem, scene::camera::Camera}, render_item_impl_default};

use super::wgpu::WGpu;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform
{
    pub view_position: [f32; 4],
    pub view: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform
{
    pub fn new() -> Self
    {
        Self
        {
            view_position: [0.0; 4],
            view: nalgebra::Matrix4::<f32>::identity().into(),
            view_proj: nalgebra::Matrix4::<f32>::identity().into()
        }
    }

    pub fn update_view_proj(&mut self, pos:Point3::<f32>, projection: Matrix4<f32>, view: Matrix4<f32>)
    {
        self.view_position = pos.to_homogeneous().into();
        self.view = view.into();
        self.view_proj = (projection * view).into();
    }
}

pub struct CameraBuffer
{
    pub name: String,
    buffer: wgpu::Buffer,
}

impl RenderItem for CameraBuffer
{
    render_item_impl_default!();
}

impl CameraBuffer
{
    pub fn new(wgpu: &mut WGpu, cam: &Camera) -> CameraBuffer
    {
        let empty_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("Empty Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut buffer = CameraBuffer
        {
            name: cam.name.clone(),
            buffer: empty_buffer,
        };

        buffer.to_buffer(wgpu, cam);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, cam: &Camera)
    {
        let data = cam.get_data();

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(data.eye_pos, cam.webgpu_projection(), data.view);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, cam: &Camera)
    {
        let data = cam.get_data();

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(data.eye_pos, cam.webgpu_projection(), data.view);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}
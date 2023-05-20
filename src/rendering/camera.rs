use crate::state::scene::camera::Camera;

use super::wgpu::WGpu;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform
{
    pub view_position: [f32; 4],
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform
{
    pub fn new() -> Self
    {
        Self
        {
            view_position: [0.0; 4],
            view_proj: nalgebra::Matrix4::<f32>::identity().into()
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera)
    {
        self.view_position = camera.eye_pos.to_homogeneous().into();
        self.view_proj = (camera.webgpu_projection() * camera.view).into();
    }
}
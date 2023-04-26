use crate::state::scene::camera::Camera;

use super::wgpu::WGpu;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform
{
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform
{
    pub fn new() -> Self
    {
        Self
        {
            view_proj: nalgebra::Matrix4::<f32>::identity().into()
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera)
    {
        self.view_proj = (camera.webgpu_projection() * camera.view).into();
    }
}

/*
pub struct CameraBuffer
{
    pub name: String,
    uniform: CameraUniform
}

impl CameraBuffer
{
    pub fn new(wgpu: &mut WGpu, name: &str, camera: &Camera) -> CameraBuffer
    {
        let device = wgpu.device();

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let camera_buffer_name = format!("{} Camera Buffer", name);
        let camera_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(camera_buffer_name.as_str()),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let camera_bind_group_layout_name = format!("{} camera_bind_group_layout", name);
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some(camera_bind_group_layout_name.as_str()),
        });

        let camera_bind_group_name = format!("{} camera_bind_group", name);
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &camera_bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }
            ],
            label: Some(camera_bind_group_name.as_str()),
        });


        Self
        {
            name: name.to_string(),
            uniform: camera_uniform,
        }
    }

    pub fn get_vertex_buffer(&self) -> &wgpu::Buffer
    {
        &self.vertex_buffer
    }

    pub fn get_index_buffer(&self) -> &wgpu::Buffer
    {
        &self.index_buffer
    }

    pub fn get_vertex_count(&self) -> u32
    {
        self.vertex_count
    }

    pub fn get_index_count(&self) -> u32
    {
        self.index_count
    }
}
*/
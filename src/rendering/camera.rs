use nalgebra::{Point3, Matrix4};
use wgpu::{util::DeviceExt, BindGroup, BindGroupLayout};

use crate::{state::{helper::render_item::RenderItem, scene::camera::Camera}, render_item_impl_default};

use super::{wgpu::WGpu, uniform};

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

    pub fn update_view_proj(&mut self, pos:Point3::<f32>, projection: Matrix4<f32>, view: Matrix4<f32>)
    {
        self.view_position = pos.to_homogeneous().into();
        self.view_proj = (projection * view).into();
    }
}

pub struct CameraBuffer
{
    pub name: String,
    buffer: wgpu::Buffer,

    //bind_group_layout: Option<BindGroupLayout>,
    //bind_group: Option<BindGroup>,
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

            //bind_group_layout: None,
            //bind_group: None
        };

        buffer.to_buffer(wgpu, cam);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, cam: &Camera)
    {
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(cam.eye_pos, cam.webgpu_projection(), cam.view);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        //self.create_binding_group(wgpu);
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, cam: &Camera)
    {
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(cam.eye_pos, cam.webgpu_projection(), cam.view);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    /*
    pub fn create_binding_group(&mut self, wgpu: &mut WGpu)
    {
        let bind_group_layout_name = format!("{} camera_bind_group_layout", self.name);
        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, true)
            ],
            label: Some(bind_group_layout_name.as_str()),
        });

        let bind_group_name = format!("{} camera_bind_group", self.name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                uniform::uniform_bind_group(0, &self.get_buffer())
            ],
            label: Some(bind_group_name.as_str()),
        });

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);
    }

    pub fn get_bind_group_layout(&self) -> &BindGroupLayout
    {
        self.bind_group_layout.as_ref().unwrap()
    }

    pub fn get_bind_group(&self) -> &BindGroup
    {
        self.bind_group.as_ref().unwrap()
    }
    */
}
use wgpu::util::DeviceExt;

use crate::{state::{helper::render_item::RenderItem, scene::components::{material::Material, component::Component}}, render_item_impl_default};

use super::wgpu::WGpu;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform
{
    pub ambient_color: [f32; 4],
    pub base_color: [f32; 4],
    pub specular_color: [f32; 4],

    pub alpha: f32,
    pub shininess: f32,
    pub reflectivity: f32,
    pub refraction_index: f32,

    pub normal_map_strength: f32,
    pub roughness: f32,
    pub receive_shadow: u32,

    _padding: f32,
}

impl MaterialUniform
{
    pub fn new(material: &Material) -> Self
    {
        let material_data = material.get_data();

        MaterialUniform
        {
            ambient_color:
            [
                material_data.ambient_color.x,
                material_data.ambient_color.y,
                material_data.ambient_color.z,
                1.0,
            ],
            base_color:
            [
                material_data.base_color.x,
                material_data.base_color.y,
                material_data.base_color.z,
                1.0,
            ],
            specular_color:
            [
                material_data.specular_color.x,
                material_data.specular_color.y,
                material_data.specular_color.z,
                1.0,
            ],
            alpha: material_data.alpha,
            shininess: material_data.shininess,
            reflectivity: material_data.reflectivity,
            refraction_index: material_data.refraction_index,
            normal_map_strength: material_data.normal_map_strength,
            roughness: material_data.roughness,
            receive_shadow: material_data.receive_shadow as u32,
            _padding: 0.0,
        }
    }
}

pub struct MaterialBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,
}

impl RenderItem for MaterialBuffer
{
    render_item_impl_default!();
}

impl MaterialBuffer
{
    pub fn new(wgpu: &mut WGpu, material: &Material) -> MaterialBuffer
    {
        let empty_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("Empty Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut buffer = MaterialBuffer
        {
            name: material.get_base().name.clone(),
            buffer: empty_buffer,
        };

        buffer.to_buffer(wgpu, material);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, material: &Material)
    {
        let mut material_uniform = MaterialUniform::new(material);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[material_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, material: &Material)
    {
        let mut material_uniform = MaterialUniform::new(material);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[material_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}
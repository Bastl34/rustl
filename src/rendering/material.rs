use crate::{state::helper::render_item::RenderItem, render_item_impl_default};

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

pub struct MaterialBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,
}

impl RenderItem for MaterialBuffer
{
    render_item_impl_default!();
}
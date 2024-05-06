use wgpu::{BindGroupLayoutEntry, BindGroupEntry, Buffer};

/*
 Bind Group layout:

 - Materials + Textures (node)
 - Lights, Camera, Scene Properties (Tonemapping/HDR/Gamma) (scene)
 - Skeleton (node)
 - Custom (node)
 */

pub fn uniform_bind_group_layout_entry(index: u32, vertex: bool, fragment: bool) -> BindGroupLayoutEntry
{
    let mut shader_visibility = wgpu::ShaderStages::NONE;
    if vertex { shader_visibility |= wgpu::ShaderStages::VERTEX }
    if fragment { shader_visibility |= wgpu::ShaderStages::FRAGMENT }

    wgpu::BindGroupLayoutEntry
    {
        binding: index,
        visibility: shader_visibility,
        ty: wgpu::BindingType::Buffer
        {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn uniform_bind_group<'a>(index: u32, buffer: &'a Buffer) -> BindGroupEntry<'a>
{
    wgpu::BindGroupEntry
    {
        binding: index,
        resource: buffer.as_entire_binding(),
    }
}
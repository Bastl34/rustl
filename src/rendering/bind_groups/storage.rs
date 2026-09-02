use wgpu::BindGroupLayoutEntry;

pub fn storage_bind_group_layout_entry(index: u32, vertex: bool, fragment: bool, read_only: bool) -> BindGroupLayoutEntry
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
            ty: wgpu::BufferBindingType::Storage { read_only: read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
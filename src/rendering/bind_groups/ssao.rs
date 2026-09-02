#![allow(dead_code)]

use nalgebra::Matrix4;
use wgpu::{BindGroupLayout, BindGroup};
use wgpu::util::DeviceExt;

use crate::{render_item_impl_default, rendering::{texture::Texture, wgpu::WGpu}, state::helper::render_item::RenderItem};

// must match SsaoUniform in shader/ssao.wgsl
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SsaoUniform
{
    pub projection: [[f32; 4]; 4],
    pub inv_projection: [[f32; 4]; 4],

    // camera viewport in surface pixels (top-left origin): xy = offset, zw = size
    pub viewport: [f32; 4],

    // camera viewport in the pixels of the ao render targets
    // (equal to `viewport` at full resolution, half of it in half resolution mode)
    pub ao_viewport: [f32; 4],

    pub radius: f32,
    pub bias: f32,

    // kernel sample count: 16 in full res mode, 32 in half res mode
    pub samples: u32,

    pub _padding: f32,
}

impl SsaoUniform
{
    pub fn new(projection: Matrix4<f32>, viewport: [f32; 4], ao_viewport: [f32; 4], radius: f32, bias: f32, samples: u32) -> SsaoUniform
    {
        let inv_projection = projection.try_inverse().unwrap_or_else(Matrix4::identity);

        SsaoUniform
        {
            projection: projection.into(),
            inv_projection: inv_projection.into(),
            viewport,
            ao_viewport,
            radius,
            bias,
            samples,
            _padding: 0.0,
        }
    }
}

// per camera ssao resources: one uniform buffer shared by all passes, one bind group
// for the ssao pass (scene depth), one for the blur pass (raw ssao result) and one for
// the upsample pass (scene depth + blurred half res ssao, used in half res mode only)
pub struct SsaoBindGroup
{
    pub ssao_layout: BindGroupLayout,
    pub blur_layout: BindGroupLayout,
    pub upsample_layout: BindGroupLayout,

    pub ssao_bind_group: BindGroup,
    pub blur_bind_group: BindGroup,
    pub upsample_bind_group: BindGroup,

    pub uniform_buffer: wgpu::Buffer,
}

impl RenderItem for SsaoBindGroup
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.uniform_buffer.size()
    }
}

impl SsaoBindGroup
{
    pub fn ssao_bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("ssao_bind_group_layout"),
            entries:
            &[
                // Binding 0: scene depth texture (depth pre-pass)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },

                // Binding 1: ssao uniform (matrices, viewport, params)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn blur_bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("ssao_blur_bind_group_layout"),
            entries:
            &[
                // Binding 1: ssao uniform (only the viewport is read in the blur pass)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },

                // Binding 2: raw (unblurred) ssao texture
                // (2 instead of 0: both entry points live in one shader module - the depth
                // texture occupies binding 0 there and bindings must not overlap)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn upsample_bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            label: Some("ssao_upsample_bind_group_layout"),
            entries:
            &[
                // Binding 0: scene depth texture (bilateral weights)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },

                // Binding 1: ssao uniform
                wgpu::BindGroupLayoutEntry
                {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },

                // Binding 2: blurred half res ssao texture
                wgpu::BindGroupLayoutEntry
                {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn new(wgpu: &mut WGpu, name: &str, depth_texture: &Texture, ssao_raw_texture: &Texture, ssao_blur_half_texture: &Texture, uniform: SsaoUniform) -> SsaoBindGroup
    {
        let ssao_layout = Self::ssao_bind_layout(wgpu);
        let blur_layout = Self::blur_bind_layout(wgpu);
        let upsample_layout = Self::upsample_bind_layout(wgpu);

        let uniform_buffer = wgpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor
        {
            label: Some(format!("{} ssao_uniform_buffer", name).as_str()),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let ssao_bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &ssao_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_texture.get_view()),
                },
                wgpu::BindGroupEntry
                {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
            label: Some(format!("{} ssao_bind_group", name).as_str()),
        });

        let blur_bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &blur_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&ssao_raw_texture.get_view()),
                },
            ],
            label: Some(format!("{} ssao_blur_bind_group", name).as_str()),
        });

        let upsample_bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &upsample_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_texture.get_view()),
                },
                wgpu::BindGroupEntry
                {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry
                {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&ssao_blur_half_texture.get_view()),
                },
            ],
            label: Some(format!("{} ssao_upsample_bind_group", name).as_str()),
        });

        SsaoBindGroup
        {
            ssao_layout,
            blur_layout,
            upsample_layout,

            ssao_bind_group,
            blur_bind_group,
            upsample_bind_group,

            uniform_buffer,
        }
    }

    // camera or ssao params changed - update the uniform in place
    pub fn update_uniform(&self, wgpu: &mut WGpu, uniform: SsaoUniform)
    {
        wgpu.queue_mut().write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

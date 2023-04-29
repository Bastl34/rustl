use std::{fs};

use wgpu::{BindGroupEntry, BindGroupLayoutEntry};

use super::{wgpu::WGpu};

pub struct Texture
{
    pub name: String,

    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl Texture
{
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new_from_image(wgpu: &mut WGpu, name: &str, path: &str) -> Texture
    {
        let image_bytes = fs::read(path).unwrap();

        let device = wgpu.device();
        let queue = wgpu.queue_mut();

        let image = image::load_from_memory(image_bytes.as_slice()).unwrap();
        let rgba = image.to_rgba8();

        use image::GenericImageView;
        let dimensions = image.dimensions();

        let texture_size = wgpu::Extent3d
        {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture_name = format!("{} Texture", name);
        let texture = device.create_texture
        (
            &wgpu::TextureDescriptor
            {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some(texture_name.as_str()),

                // Rgba8UnormSrgb is allowed for WebGL2
                view_formats: &[],
            }
        );

        // upload
        queue.write_texture
        (
            wgpu::ImageCopyTexture
            {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::ImageDataLayout
            {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
                rows_per_image: std::num::NonZeroU32::new(dimensions.1),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor
        {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self
        {
            name: name.to_string(),
            texture: texture,
            view: texture_view,
            sampler: sampler,
        }
    }

    pub fn new_depth_texture(wgpu: &mut WGpu) -> Texture
    {
        let config = wgpu.surface_config();
        let device = wgpu.device();

        let size = wgpu::Extent3d
        {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor
        {
            label: Some("depth texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[Self::DEPTH_FORMAT],
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler
        (
            &wgpu::SamplerDescriptor
            {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                compare: Some(wgpu::CompareFunction::LessEqual),
                lod_min_clamp: 0.0,
                lod_max_clamp: 100.0,
                ..Default::default()
            }
        );

        Self
        {
            name: "depth texture".to_string(),
            texture,
            view,
            sampler
        }

    }

    pub fn get_texture(&self) -> &wgpu::Texture
    {
        &self.texture
    }

    pub fn get_view(&self) -> &wgpu::TextureView
    {
        &self.view
    }

    pub fn get_sampler(&self) -> &wgpu::Sampler
    {
        &self.sampler
    }

    pub fn get_bind_group_layout_entries(&self, index: u32) -> [BindGroupLayoutEntry; 2]
    {
        [
            wgpu::BindGroupLayoutEntry
            {
                binding: index * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture
                {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry
            {
                binding: (index * 2) + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                // This should match the filterable field of the
                // corresponding Texture entry above.
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }
        ]
    }

    pub fn get_bind_group_entries(&self, index: u32) -> [BindGroupEntry; 2]
    {
        [
            wgpu::BindGroupEntry
            {
                binding: index * 2,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry
            {
                binding: (index * 2) + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            }
        ]
    }
}

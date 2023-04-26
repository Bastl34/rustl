use std::{fs};

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
    pub fn new(wgpu: &mut WGpu, name: &str, path: &str) -> Texture
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
}

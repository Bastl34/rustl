use image::{DynamicImage, ImageBuffer, Rgba};
use wgpu::{BindGroupEntry, BindGroupLayoutEntry, Device, Sampler};

use crate::{state::helper::render_item::RenderItem, render_item_impl_default};

use super::{wgpu::WGpu, helper::buffer::{BufferDimensions, remove_padding}};

pub struct Texture
{
    pub name: String,

    pub width: u32,
    pub height: u32,

    format: wgpu::TextureFormat,
    is_depth_texture: bool,

    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl RenderItem for Texture
{
    render_item_impl_default!();
}

impl Texture
{
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const SRGBA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    pub const RGBA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn new_from_texture(wgpu: &mut WGpu, name: &str, scene_texture: &crate::state::scene::texture::Texture, srgb: bool) -> Texture
    {
        let device = wgpu.device();
        let queue = wgpu.queue_mut();

        let format;
        if srgb
        {
            format = Self::SRGBA_FORMAT;
        }
        else
        {
            format = Self::RGBA_FORMAT;
        }

        let texture_size = wgpu::Extent3d
        {
            width: scene_texture.width(),
            height: scene_texture.height(),
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
                format: format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST| wgpu::TextureUsages::COPY_SRC, // COPY_SRC just to read again
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
            scene_texture.rgba_data(),
            wgpu::ImageDataLayout
            {
                offset: 0,
                bytes_per_row: Some(4 * scene_texture.width()), // 4 = RGBA
                rows_per_image: Some(scene_texture.height()),
                //rows_per_image: Some(scene_texture.width()),
            },
            texture_size,
        );

        let sampler = Self::create_sampler(device, scene_texture);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self
        {
            name: name.to_string(),

            width: scene_texture.width(),
            height: scene_texture.height(),

            format: format,
            is_depth_texture: false,

            texture: texture,
            view: texture_view,
            sampler: sampler,
        }
    }

    pub fn new_empty_texture(wgpu: &mut WGpu, name: &str, srgb: bool) -> Texture
    {
        let device = wgpu.device();

        let width: u32 = 1;
        let height = 1;

        let format;
        if srgb
        {
            format = Self::SRGBA_FORMAT;
        }
        else
        {
            format = Self::RGBA_FORMAT;
        }

        let texture_size = wgpu::Extent3d
        {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        };

        let texture_name = format!("{} Empty Texture", name);
        let texture = device.create_texture
        (
            &wgpu::TextureDescriptor
            {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST| wgpu::TextureUsages::COPY_SRC, // COPY_SRC just to read again
                label: Some(texture_name.as_str()),

                // Rgba8UnormSrgb is allowed for WebGL2
                view_formats: &[],
            }
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

            width,
            height,

            format: format,
            is_depth_texture: false,

            texture: texture,
            view: texture_view,
            sampler: sampler,
        }
    }

    pub fn new_depth_texture(wgpu: &mut WGpu, samples: u32) -> Texture
    {
        // shadow
        // https://github.com/gfx-rs/wgpu/blob/trunk/wgpu/examples/shadow/shader.wgsl
        // https://github.com/gfx-rs/wgpu/blob/trunk/wgpu/examples/shadow/main.rs
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
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
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
                ..Default::default()
            }
        );

        Self
        {
            name: "depth texture".to_string(),

            width: config.width,
            height: config.height,

            format: Self::DEPTH_FORMAT,
            is_depth_texture: true,

            texture,
            view,
            sampler
        }

    }

    pub fn create_sampler(device: &Device, scene_texture: &crate::state::scene::texture::Texture) -> Sampler
    {
        let tex_data = scene_texture.get_data();

        let address_mode_u;
        match tex_data.address_mode_u
        {
            crate::state::scene::texture::TextureAddressMode::ClampToEdge => address_mode_u = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::texture::TextureAddressMode::Repeat => address_mode_u = wgpu::AddressMode::Repeat,
            crate::state::scene::texture::TextureAddressMode::MirrorRepeat => address_mode_u = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::texture::TextureAddressMode::ClampToBorder => address_mode_u = wgpu::AddressMode::ClampToBorder,
        }

        let address_mode_v;
        match tex_data.address_mode_v
        {
            crate::state::scene::texture::TextureAddressMode::ClampToEdge => address_mode_v = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::texture::TextureAddressMode::Repeat => address_mode_v = wgpu::AddressMode::Repeat,
            crate::state::scene::texture::TextureAddressMode::MirrorRepeat => address_mode_v = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::texture::TextureAddressMode::ClampToBorder => address_mode_v = wgpu::AddressMode::ClampToBorder,
        }

        let address_mode_w;
        match tex_data.address_mode_w
        {
            crate::state::scene::texture::TextureAddressMode::ClampToEdge => address_mode_w = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::texture::TextureAddressMode::Repeat => address_mode_w = wgpu::AddressMode::Repeat,
            crate::state::scene::texture::TextureAddressMode::MirrorRepeat => address_mode_w = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::texture::TextureAddressMode::ClampToBorder => address_mode_w = wgpu::AddressMode::ClampToBorder,
        }

        let mag_filter;
        match tex_data.mag_filter
        {
            crate::state::scene::texture::TextureFilterMode::Nearest => mag_filter = wgpu::FilterMode::Nearest,
            crate::state::scene::texture::TextureFilterMode::Linear => mag_filter = wgpu::FilterMode::Linear,
        }

        let min_filter;
        match tex_data.min_filter
        {
            crate::state::scene::texture::TextureFilterMode::Nearest => min_filter = wgpu::FilterMode::Nearest,
            crate::state::scene::texture::TextureFilterMode::Linear => min_filter = wgpu::FilterMode::Linear,
        }

        let mipmap_filter;
        match tex_data.mipmap_filter
        {
            crate::state::scene::texture::TextureFilterMode::Nearest => mipmap_filter = wgpu::FilterMode::Nearest,
            crate::state::scene::texture::TextureFilterMode::Linear => mipmap_filter = wgpu::FilterMode::Linear,
        }

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor
        {
            address_mode_u: address_mode_u,
            address_mode_v: address_mode_v,
            address_mode_w: address_mode_w,
            mag_filter: mag_filter,
            min_filter: min_filter,
            mipmap_filter: mipmap_filter,
            ..Default::default()
        });

        sampler
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, scene_texture: &crate::state::scene::texture::Texture)
    {
        let device = wgpu.device();
        let queue = wgpu.queue_mut();

        let texture_size = wgpu::Extent3d
        {
            width: scene_texture.width(),
            height: scene_texture.height(),
            depth_or_array_layers: 1,
        };

        // TODO: performance bottle neck if there was no texture data change

        // upload
        queue.write_texture
        (
            wgpu::ImageCopyTexture
            {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            scene_texture.rgba_data(),
            wgpu::ImageDataLayout
            {
                offset: 0,
                bytes_per_row: Some(4 * scene_texture.width()), // 4 = RGBA
                rows_per_image: Some(scene_texture.height())
            },
            texture_size,
        );

        self.sampler = Self::create_sampler(device, scene_texture);
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

    pub fn get_bind_group_layout_entries(&self, index_start: u32) -> [BindGroupLayoutEntry; 2]
    {
        let mut sample_type = wgpu::TextureSampleType::Float { filterable: true };
        if self.is_depth_texture
        {
            //sample_type = wgpu::TextureSampleType::Float { filterable: false };
            sample_type = wgpu::TextureSampleType::Depth
        }

        let mut binding_type = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        if self.is_depth_texture
        {
            //binding_type = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering);
            binding_type = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison);
        }

        [
            wgpu::BindGroupLayoutEntry
            {
                binding: index_start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture
                {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: sample_type,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry
            {
                binding: index_start + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                // This should match the filterable field of the
                // corresponding Texture entry above.
                ty: binding_type,
                count: None,
            }
        ]
    }

    pub fn get_bind_group_entries(&self, index_start: u32) -> [BindGroupEntry; 2]
    {
        [
            wgpu::BindGroupEntry
            {
                binding: index_start,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry
            {
                binding: index_start + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            }
        ]
    }

    pub fn to_image(&self, wgpu: &mut WGpu) -> DynamicImage
    {
        // https://sotrh.github.io/learn-wgpu/showcase/gifs/#how-do-we-make-the-frames
        // https://github.com/gfx-rs/wgpu/blob/trunk/wgpu/tests/write_texture.rs

        // ********** create texture buffer **********
        let buffer_dimensions = BufferDimensions::new(self.width as usize, self.height as usize);

        let buffer_desc = wgpu::BufferDescriptor
        {
            size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Output Buffer"),
            mapped_at_creation: false,
        };
        let output_buffer = wgpu.device().create_buffer(&buffer_desc);

        // ********** copy to buffer **********
        let mut encoder = wgpu.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        encoder.copy_texture_to_buffer
        (
            wgpu::ImageCopyTexture
            {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer
            {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout
                {
                    offset: 0,
                    bytes_per_row: Some(buffer_dimensions.padded_bytes_per_row as u32),
                    rows_per_image: Some(self.height),
                }
            },
            wgpu::Extent3d
            {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        wgpu.queue_mut().submit(Some(encoder.finish()));

        // ********** read buffer **********
        let slice = output_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| ());
        wgpu.device().poll(wgpu::Maintain::Wait);

        // ********** remove padding **********
        let padded_data = slice.get_mapped_range();
        let data = remove_padding(&padded_data, &buffer_dimensions);
        drop(padded_data);
        output_buffer.unmap();

        DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::from_raw(self.width, self.height, data).unwrap())

    }
}

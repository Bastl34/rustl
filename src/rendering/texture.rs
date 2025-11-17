#![allow(dead_code)]

use image::{DynamicImage, ImageBuffer, Rgba};
use wgpu::{BindGroupEntry, BindGroupLayoutEntry};

use crate::{render_item_impl_default, state::helper::render_item::RenderItem};

use super::{wgpu::WGpu, helper::buffer::{BufferDimensions, remove_padding}};

#[derive(Debug, PartialEq, Eq)]
pub enum TextureFormat
{
    Srgba,
    Rgba,
    Gray,
    Depth,
    R32Float,
}

pub struct Texture
{
    pub name: String,

    pub width: u32,
    pub height: u32,

    format: TextureFormat,
    is_depth_texture: bool,

    texture: wgpu::Texture,
    views: Vec<wgpu::TextureView>,
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
    pub const GRAY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
    pub const R32_FLOAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

    pub fn new_from_texture(wgpu: &mut WGpu, name: &str, scene_texture: &crate::state::resources::texture::Texture, format: TextureFormat) -> Texture
    {
        let device = wgpu.device();
        let queue = wgpu.queue_mut();

        let mut mipmaps = vec![];
        if scene_texture.get_data().mipmapping
        {
            if let Some(mipmap_cache) = &scene_texture.get_data().mipmap_cache
            {
                mipmaps = mipmap_cache.clone();
            }
            else
            {
                mipmaps = scene_texture.create_mipmap_levels();
            }
        }

        let wgpu_format = Self::get_wgpu_format(&format);

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
                mip_level_count: 1 + mipmaps.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST| wgpu::TextureUsages::COPY_SRC, // COPY_SRC just to read again
                label: Some(texture_name.as_str()),

                // Rgba8UnormSrgb is allowed for WebGL2
                view_formats: &[],
            }
        );

        let channels = scene_texture.channels();

        // upload texture
        queue.write_texture
        (
            wgpu::TexelCopyTextureInfo
            {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            scene_texture.raw_data(),
            wgpu::TexelCopyBufferLayout
            {
                offset: 0,
                bytes_per_row: Some(scene_texture.width() * channels),
                rows_per_image: Some(scene_texture.height()),
            },
            texture_size,
        );

        // upload mipmaps
        for (i, mipmap) in mipmaps.iter().enumerate()
        {
            let texture_size = wgpu::Extent3d
            {
                width: mipmap.width(),
                height: mipmap.height(),
                depth_or_array_layers: 1,
            };

            queue.write_texture
            (
                wgpu::TexelCopyTextureInfo
                {
                    texture: &texture,
                    mip_level: i as u32 + 1,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                mipmap.as_bytes(),
                wgpu::TexelCopyBufferLayout
                {
                    offset: 0,
                    bytes_per_row: Some(mipmap.width() * channels),
                    rows_per_image: Some(mipmap.height()),
                },
                texture_size,
            );
        }

        //let sampler = Self::create_sampler(device, scene_texture);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self
        {
            name: name.to_string(),

            width: scene_texture.width(),
            height: scene_texture.height(),

            format: format,
            is_depth_texture: false,

            texture: texture,
            views: vec![texture_view],
            //sampler: sampler
        }
    }

    pub fn new_empty_texture(wgpu: &mut WGpu, name: &str, format: TextureFormat) -> Texture
    {
        let device = wgpu.device();

        let width: u32 = 1;
        let height = 1;

        let wgpu_format;
        match format
        {
            TextureFormat::Srgba => wgpu_format = Self::SRGBA_FORMAT,
            TextureFormat::Rgba => wgpu_format = Self::RGBA_FORMAT,
            TextureFormat::Gray => wgpu_format = Self::GRAY_FORMAT,
            TextureFormat::Depth => wgpu_format = Self::DEPTH_FORMAT,
            TextureFormat::R32Float => wgpu_format = Self::R32_FLOAT,
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
                format: wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST| wgpu::TextureUsages::COPY_SRC, // COPY_SRC just to read again
                label: Some(texture_name.as_str()),

                // Rgba8UnormSrgb is allowed for WebGL2
                view_formats: &[],
            }
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self
        {
            name: name.to_string(),

            width,
            height,

            format: format,
            is_depth_texture: false,

            texture: texture,
            views: vec![texture_view],
            //sampler: sampler
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

        Self
        {
            name: "depth texture".to_string(),

            width: config.width,
            height: config.height,

            format: TextureFormat::Depth,
            is_depth_texture: true,

            texture,
            views: vec![view]
        }
    }

    pub fn new_hzb_texture(wgpu: &mut WGpu) -> Texture
    {
        let device = wgpu.device();
        let config = wgpu.surface_config();

        let mip_count = (config.width.max(config.height) as f32).log2().floor() as u32 + 1;

        let size = wgpu::Extent3d
        {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor
        {
            label: Some("HZB texture"),
            size,
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::R32_FLOAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[Self::R32_FLOAT],
        };
        let texture = device.create_texture(&desc);

        let views: Vec<wgpu::TextureView> = (0..mip_count).map(|mip| texture.create_view(&wgpu::TextureViewDescriptor
        {
            label: Some("HZB Mip View"),
            format: Some(wgpu::TextureFormat::R32Float),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            ..Default::default()
        })).collect();

        Self
        {
            name: "HZB texture".to_string(),

            width: config.width,
            height: config.height,

            format: TextureFormat::R32Float,
            is_depth_texture: false,

            texture,
            views
        }
    }

    pub fn get_wgpu_format(format: &TextureFormat) -> wgpu::TextureFormat
    {
        match format
        {
            TextureFormat::Srgba => Self::SRGBA_FORMAT,
            TextureFormat::Rgba => Self::RGBA_FORMAT,
            TextureFormat::Gray => Self::GRAY_FORMAT,
            TextureFormat::Depth => Self::DEPTH_FORMAT,
            TextureFormat::R32Float => Self::R32_FLOAT,
        }
    }

    pub fn get_texture(&self) -> &wgpu::Texture
    {
        &self.texture
    }

    pub fn get_view(&self) -> &wgpu::TextureView
    {
        &self.views[0]
    }

    pub fn get_views(&self) -> &Vec<wgpu::TextureView>
    {
        &self.views
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

    pub fn get_bind_group_entries<'a>(index_start: u32, view: &'a wgpu::TextureView, sampler: &'a wgpu::Sampler) -> [BindGroupEntry<'a>; 2]
    {
        [
            wgpu::BindGroupEntry
            {
                binding: index_start,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry
            {
                binding: index_start + 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            }
        ]
    }

    pub fn to_image(&self, wgpu: &mut WGpu) -> DynamicImage
    {
        // https://sotrh.github.io/learn-wgpu/showcase/gifs/#how-do-we-make-the-frames
        // https://github.com/gfx-rs/wgpu/blob/trunk/wgpu/tests/write_texture.rs

        let mut src_texture = &self.texture;

        // ********** Multisample-Textures needs to be resolved **********
        // if multisample --> resolve in a single sample texture
        let resolve_texture;
        let resolve_view;
        if self.texture.sample_count() > 1
        {
            let device = wgpu.device();
            let resolve_desc = wgpu::TextureDescriptor
            {
                label: Some("Resolved Texture"),
                size: wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1},
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: Self::get_wgpu_format(&self.format),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            };
            resolve_texture = device.create_texture(&resolve_desc);
            resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Renderpass
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor
            {
                label: Some("Resolve Encoder"),
            });

            let depth_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
            {
                label: Some("Depth Resolve BGL"),
                entries: &
                [
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture
                        {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: true,
                        },
                        count: None,
                    },
                ],
            });

            let depth_bg = device.create_bind_group(&wgpu::BindGroupDescriptor
            {
                label: Some("Depth Resolve BG"),
                layout: &depth_bind_group_layout,
                entries: &
                [
                    wgpu::BindGroupEntry
                    {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.views[0]), // <- multisample view
                    },
                ],
            });

            // Pipeline + Shader for depth resolve
            let shader_module = device.create_shader_module(wgpu::include_wgsl!("../../resources/shader/depth_resolve.wgsl"));

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
            {
                label: Some("Depth Resolve PipelineLayout"),
                bind_group_layouts: &[&depth_bind_group_layout],
                push_constant_ranges: &[],
            });

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
            {
                label: Some("Depth Resolve Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState
                {
                    module: &shader_module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState
                {
                    module: &shader_module,
                    entry_point: Some("fs_main"),
                    targets: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState
                {
                    format: Self::get_wgpu_format(&self.format),
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            // Fullscreen quad without vertex buffer: draw(3)
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
                {
                    label: Some("Resolve RenderPass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
                    {
                        view: &resolve_view,
                        depth_ops: Some(wgpu::Operations
                        {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(&pipeline);
                render_pass.set_bind_group(0, &depth_bg, &[]);
                render_pass.draw(0..3, 0..1);
            }

            wgpu.queue_mut().submit(Some(encoder.finish()));
            src_texture = &resolve_texture;
        }

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
            wgpu::TexelCopyTextureInfo
            {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo
            {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout
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
        wgpu.device().poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();

        // ********** remove padding **********
        let padded_data = slice.get_mapped_range();
        let data = remove_padding(&padded_data, &buffer_dimensions);
        drop(padded_data);
        output_buffer.unmap();

        DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::from_raw(self.width, self.height, data).unwrap())
    }
}

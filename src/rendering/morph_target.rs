use image::{DynamicImage, ImageBuffer, Rgba, EncodableLayout};
use nalgebra::{Point3, Vector4, Point4};
use wgpu::{BindGroupEntry, BindGroupLayoutEntry, Device, Sampler, util::DeviceExt};

use crate::{state::{helper::render_item::RenderItem, scene::components::mesh::MeshData}, render_item_impl_default};

use super::{wgpu::WGpu, helper::buffer::{BufferDimensions, remove_padding}};

const ITEMS_PER_VERTEX: usize = 4; //pos, normal, tangent, bitangent

pub struct MorpthTarget
{
    pub width: u32,
    pub height: u32,

    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl RenderItem for MorpthTarget
{
    render_item_impl_default!();
}

impl MorpthTarget
{
    pub fn new_from_texture(wgpu: &mut WGpu, name: &str, mesh_data: &MeshData) -> MorpthTarget
    {
        let device = wgpu.device();

        let max_tex_size = wgpu.device().limits().max_texture_dimension_2d;
        let vertices = mesh_data.vertices.len() as u32 * ITEMS_PER_VERTEX as u32;

        let width = vertices.min(max_tex_size);
        let height = ((vertices as f64) / (width as f64)).ceil() as u32;

        let texture_size = wgpu::Extent3d
        {
            width: width,
            height: height,
            depth_or_array_layers: Self::get_morph_targets(mesh_data),
        };

        let texture_name = format!("{} Morph Texture Array", name);
        let texture = device.create_texture
        (
            &wgpu::TextureDescriptor
            {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, // COPY_SRC just to read again
                label: Some(texture_name.as_str()),

                view_formats: &[],
            }
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor
        {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());


        let mut morph_target = Self
        {
            width: width,
            height: height,

            texture: texture,
            view: texture_view,
            sampler: sampler,
        };

        // upload data
        morph_target.update_buffer(wgpu, mesh_data);

        morph_target
    }

    fn get_morph_targets(mesh_data: &MeshData) -> u32
    {
        let pos_amount = mesh_data.morph_target_positions.len();
        let normal_amount = mesh_data.morph_target_normals.len();
        let tangent_amount = mesh_data.morph_target_tangents.len();

        pos_amount.max(normal_amount).max(tangent_amount) as u32
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, mesh_data: &MeshData)
    {
        let device = wgpu.device();
        let queue = wgpu.queue_mut();

        let targets = Self::get_morph_targets(mesh_data);

        for morpth_target_id in 0..targets
        {
            // prepare data
            //let mut data: Vec<f32> = Vec::with_capacity(mesh_data.vertices.len() * ITEMS_PER_VERTEX);
            let mut data: Vec<f32> = Vec::with_capacity(self.width as usize * self.height as usize * ITEMS_PER_VERTEX);

            let positions = mesh_data.morph_target_positions.get(morpth_target_id as usize);
            let normals = mesh_data.morph_target_normals.get(morpth_target_id as usize);
            let tangents = mesh_data.morph_target_tangents.get(morpth_target_id as usize);

            for i in 0..mesh_data.vertices.len()
            {
                let mut position = vec![0.0, 0.0, 0.0, 0.0];
                let mut normal = vec![0.0, 0.0, 0.0, 0.0];
                let mut tanget = vec![0.0, 0.0, 0.0, 0.0];
                let mut bitangent = vec![0.0, 0.0, 0.0, 0.0];

                if let Some(positions) = positions
                {
                    if let Some(pos) = positions.get(i)
                    {
                        position = vec![pos.x, pos.y, pos.z, 0.0];
                    }
                }

                if let Some(normals) = normals
                {
                    if let Some(norm) = normals.get(i)
                    {
                        normal = vec![norm.x, norm.y, norm.z, 0.0];
                    }
                }

                if let Some(tangents) = tangents
                {
                    if let Some(tang) = tangents.get(i)
                    {
                        tanget = vec![tang.x, tang.y, tang.z, 0.0];
                    }
                }

                if tangents.is_some() && normals.is_some()
                {
                    let normal = normals.unwrap().get(i).unwrap();
                    let tangent = tangents.unwrap().get(i).unwrap();
                    let bitang = normal.cross(&tangent).normalize();

                    bitangent = vec![bitang.x, bitang.y, bitang.z, 0.0];
                }

                let pos = i * ITEMS_PER_VERTEX * ITEMS_PER_VERTEX;
                data.splice(pos..(pos + ITEMS_PER_VERTEX), position);

                let pos = (i * ITEMS_PER_VERTEX * ITEMS_PER_VERTEX) + ITEMS_PER_VERTEX;
                data.splice(pos..(pos + ITEMS_PER_VERTEX), normal);

                let pos = (i * ITEMS_PER_VERTEX * ITEMS_PER_VERTEX) + (ITEMS_PER_VERTEX * 2);
                data.splice(pos..(pos + ITEMS_PER_VERTEX), tanget);

                let pos = (i * ITEMS_PER_VERTEX * ITEMS_PER_VERTEX) + (ITEMS_PER_VERTEX * 3);
                data.splice(pos..(pos + ITEMS_PER_VERTEX), bitangent);
            }

            let texture_size = wgpu::Extent3d
            {
                width: self.width,
                height: self.height,
                depth_or_array_layers: targets,
            };

            /*
            TODO (check https://github.com/sotrh/learn-wgpu/blob/677300a5e96fa0b06ee7c3b4e09eb9bd6fee2e88/docs/intermediate/tutorial13-hdr/readme.md?plain=1#L591)
            queue.write_texture
            (
                wgpu::ImageCopyTexture
                {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::ImageDataLayout
                {
                    offset: 0,
                    bytes_per_row: std::num::NonZeroU32::new(4 * self.width as u32),
                    rows_per_image: std::num::NonZeroU32::new(self.height as u32),
                },
                texture_size,
            );
            */
        }
    }

    /*
    pub fn update_buffer(&mut self, wgpu: &mut WGpu, scene_texture: &crate::state::scene::texture::Texture)
    {
        dbg!("update texture");

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
                bytes_per_row: Some(scene_texture.channels() * scene_texture.width()),
                rows_per_image: Some(scene_texture.height())
            },
            texture_size,
        );

        self.sampler = Self::create_sampler(device, scene_texture);
    }
     */

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

    pub fn get_bind_group_layout_entries(&self) -> [BindGroupLayoutEntry; 2]
    {
        [
            wgpu::BindGroupLayoutEntry
            {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture
                {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry
            {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                // This should match the filterable field of the
                // corresponding Texture entry above.
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
}

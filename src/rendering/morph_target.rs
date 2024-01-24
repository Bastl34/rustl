use image::{DynamicImage, ImageBuffer, Rgba, EncodableLayout};
use nalgebra::{Point3, Vector4, Point4};
use wgpu::{BindGroupEntry, BindGroupLayoutEntry, Device, Sampler, util::DeviceExt};

use crate::{state::{helper::render_item::RenderItem, scene::components::mesh::MeshData}, render_item_impl_default};

use super::{wgpu::WGpu, helper::buffer::{BufferDimensions, remove_padding}};

const FLOATS_PER_PIXEL: usize = 4;
const ITEMS_PER_VERTEX: usize = 4; //pos, normal, tangent, bitangent

pub const MAX_MORPH_TARGETS: usize = 128;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MorphTargetUniform
{
    // just the first index is used - but must be 4 because or 16byte array stride of WGPU
    // https://www.w3.org/TR/WGSL/#example-67da5de6
    pub weights: [[f32; 4]; MAX_MORPH_TARGETS],
    pub amount: u32,

    _padding: [f32; 3],
}

impl MorphTargetUniform
{
    pub fn new(targets: u32) -> Self
    {
        Self
        {
            weights: [[0.0; 4]; MAX_MORPH_TARGETS],
            amount: targets,

            _padding: [0.0; 3],
        }
    }

    pub fn empty() -> Self
    {
        Self
        {
            weights: [[0.0; 4]; MAX_MORPH_TARGETS],
            amount: 0,

            _padding: [0.0; 3],
        }
    }
}

pub struct MorphTarget
{
    pub width: u32,
    pub height: u32,

    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,

    buffer: wgpu::Buffer,
}

impl RenderItem for MorphTarget
{
    render_item_impl_default!();
}

impl MorphTarget
{
    pub fn new(wgpu: &mut WGpu, name: &str, mesh_data: &MeshData) -> MorphTarget
    {
        let device = wgpu.device();

        let max_tex_size = wgpu.device().limits().max_texture_dimension_2d;
        let data_len = mesh_data.vertices.len() as u32 * ITEMS_PER_VERTEX as u32;

        let width = data_len.min(max_tex_size);
        let height = ((data_len as f64) / (width as f64)).ceil() as u32;

        let targets = Self::get_morph_targets(mesh_data);

        /*
        dbg!(mesh_data.vertices.len());
        dbg!(data_len);
        dbg!(width);
        dbg!(height);
         */

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

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor
        {
            label: Some(texture_name.as_str()),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(Self::get_morph_targets(mesh_data)),
            ..Default::default()
        });
        //let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = MorphTargetUniform::new(targets);
        let buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&format!("{} Morph Target Buffer", name)),
                contents: bytemuck::cast_slice(&[uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let mut morph_target = Self
        {
            width: width,
            height: height,

            texture: texture,
            view: texture_view,
            sampler: sampler,
            buffer
        };

        // upload data
        morph_target.update_texture_buffer(wgpu, mesh_data);

        morph_target
    }

    pub fn empty(wgpu: &mut WGpu) -> MorphTarget
    {
        let device = wgpu.device();

        let width = 1;
        let height = 1;

        let texture_size = wgpu::Extent3d
        {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture
        (
            &wgpu::TextureDescriptor
            {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("Empty Morph Texture Array"),

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
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor
        {
            label: Some("empty morph target"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(1),
            ..Default::default()
        });
        //let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = MorphTargetUniform::empty();
        let buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some("Empty Morph Target Buffer"),
                contents: bytemuck::cast_slice(&[uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        Self
        {
            width: width,
            height: height,

            texture: texture,
            view: texture_view,
            sampler: sampler,
            buffer
        }
    }

    pub fn get_morph_targets(mesh_data: &MeshData) -> u32
    {
        let pos_amount = mesh_data.morph_target_positions.len();
        let normal_amount = mesh_data.morph_target_normals.len();
        let tangent_amount = mesh_data.morph_target_tangents.len();

        pos_amount.max(normal_amount).max(tangent_amount) as u32
    }

    pub fn update_texture_buffer(&mut self, wgpu: &mut WGpu, mesh_data: &MeshData)
    {
        let queue = wgpu.queue_mut();

        let targets = Self::get_morph_targets(mesh_data);

        let len = (self.width * self.height * targets) as usize * FLOATS_PER_PIXEL;
        let mut morph_data: Vec<f32> = Vec::with_capacity(len);
        morph_data.extend(vec![0.0; len]);

        for morpth_target_id in 0..targets
        {
            // prepare data
            //let mut data: Vec<f32> = Vec::with_capacity(mesh_data.vertices.len() * ITEMS_PER_VERTEX);
            //let mut morph_data: Vec<f32> = Vec::with_capacity(self.width as usize * self.height as usize * ITEMS_PER_VERTEX);

            let pos_start = (self.width * self.height * morpth_target_id) as usize;

            let positions = mesh_data.morph_target_positions.get(morpth_target_id as usize);
            let normals = mesh_data.morph_target_normals.get(morpth_target_id as usize);
            let tangents = mesh_data.morph_target_tangents.get(morpth_target_id as usize);

            for i in 0..mesh_data.vertices.len()
            {
                let mut position = vec![0.0, 0.0, 0.0, 0.0];
                let mut normal = vec![0.0, 0.0, 0.0, 0.0];
                let mut tangent = vec![0.0, 0.0, 0.0, 0.0];
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
                        tangent = vec![tang.x, tang.y, tang.z, 0.0];
                    }
                }

                if normals.is_some() && tangents.is_some() && normals.unwrap().len() > 0 && tangents.unwrap().len() > 0
                {
                    let normal = normals.unwrap().get(i).unwrap();
                    let tangent = tangents.unwrap().get(i).unwrap();
                    let bitang = normal.cross(&tangent).normalize();

                    bitangent = vec![bitang.x, bitang.y, bitang.z, 0.0];
                }

                let pos = pos_start + (i * ITEMS_PER_VERTEX * FLOATS_PER_PIXEL);
                morph_data.splice(pos..(pos + FLOATS_PER_PIXEL), position);

                let pos = pos_start + (i * ITEMS_PER_VERTEX * FLOATS_PER_PIXEL) + FLOATS_PER_PIXEL;
                morph_data.splice(pos..(pos + FLOATS_PER_PIXEL), normal);

                let pos = pos_start + (i * ITEMS_PER_VERTEX * FLOATS_PER_PIXEL) + (FLOATS_PER_PIXEL * 2);
                morph_data.splice(pos..(pos + FLOATS_PER_PIXEL), tangent);

                let pos = pos_start + (i * ITEMS_PER_VERTEX * FLOATS_PER_PIXEL) + (FLOATS_PER_PIXEL * 3);
                morph_data.splice(pos..(pos + FLOATS_PER_PIXEL), bitangent);
            }
        }

        let texture_size = wgpu::Extent3d
        {
            width: self.width,
            height: self.height,
            depth_or_array_layers: targets,
        };

        queue.write_texture
        (
            wgpu::ImageCopyTexture
            {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            morph_data.as_bytes(),
            wgpu::ImageDataLayout
            {
                offset: 0,
                bytes_per_row: Some((FLOATS_PER_PIXEL * std::mem::size_of::<f32>()) as u32 * self.width),
                rows_per_image: Some(self.height as u32),
            },
            texture_size,
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, weights: &Vec<f32>)
    {
        let mut uniform = MorphTargetUniform::new(weights.len() as u32);
        for i in 0..weights.len()
        {
            uniform.weights[i][0] = weights[i];
        }

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
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

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn get_bind_group_layout_entries(index_start: u32) -> [BindGroupLayoutEntry; 2]
    {
        [
            wgpu::BindGroupLayoutEntry
            {
                binding: index_start,
                visibility: wgpu::ShaderStages::VERTEX,
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
                binding: index_start + 1,
                visibility: wgpu::ShaderStages::VERTEX,
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

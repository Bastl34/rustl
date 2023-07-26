use std::mem::swap;

use wgpu::{util::DeviceExt, BindGroupLayout, BindGroup};

use crate::{state::{helper::render_item::{RenderItem, get_render_item}, scene::components::{material::{Material, TextureType, ALL_TEXTURE_TYPES}, component::Component}}, render_item_impl_default};

use super::{wgpu::WGpu, uniform, texture::Texture};

//TODO: future: compile shaders for each texture combination to prevent branching/if statements

/*
    textures:

    0: reserved (to match bind group id)

    1: ambient
    2: base (albedo)
    3: specular
    4: normal
    5: alpha
    6: roughness
    7: ambient occlusion
    8: reflectivity
    9: shininess

    10: custom 0
    11: custom 1
    12: custom 2
    13: custom 3
    14: custom 4
    15: custom 5
    16: custom 6
    17: custom 7
    18: custom 8
    19: custom 9

    20: depth
*/

pub const ADDITIONAL_START_INDEX: u32 = 20;

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

    pub textures_used: u32,
}

impl MaterialUniform
{
    pub fn new(material: &Material) -> Self
    {
        let material_data = material.get_data();

        let mut textures_used: u32 = 0;
        if material.has_texture(TextureType::AmbientEmissive)   { textures_used |= 0x2; }
        if material.has_texture(TextureType::Base)              { textures_used |= 0x3; }
        if material.has_texture(TextureType::Specular)          { textures_used |= 0x4; }
        if material.has_texture(TextureType::Normal)            { textures_used |= 0x5; }
        if material.has_texture(TextureType::Alpha)             { textures_used |= 0x6; }
        if material.has_texture(TextureType::Roughness)         { textures_used |= 0x7; }
        if material.has_texture(TextureType::AmbientOcclusion)  { textures_used |= 0x8; }
        if material.has_texture(TextureType::Reflectivity)      { textures_used |= 0x9; }
        if material.has_texture(TextureType::Shininess)         { textures_used |= 0x10; }

        if material.has_texture(TextureType::Custom0)           { textures_used |= 0x11; }
        if material.has_texture(TextureType::Custom1)           { textures_used |= 0x12; }
        if material.has_texture(TextureType::Custom2)           { textures_used |= 0x13; }
        if material.has_texture(TextureType::Custom3)           { textures_used |= 0x14; }
        if material.has_texture(TextureType::Custom4)           { textures_used |= 0x15; }
        if material.has_texture(TextureType::Custom5)           { textures_used |= 0x16; }
        if material.has_texture(TextureType::Custom6)           { textures_used |= 0x17; }
        if material.has_texture(TextureType::Custom7)           { textures_used |= 0x18; }
        if material.has_texture(TextureType::Custom8)           { textures_used |= 0x19; }
        if material.has_texture(TextureType::Custom9)           { textures_used |= 0x20; }

        MaterialUniform
        {
            ambient_color:
            [
                material_data.ambient_color.x,
                material_data.ambient_color.y,
                material_data.ambient_color.z,
                1.0,
            ],
            base_color:
            [
                material_data.base_color.x,
                material_data.base_color.y,
                material_data.base_color.z,
                1.0,
            ],
            specular_color:
            [
                material_data.specular_color.x,
                material_data.specular_color.y,
                material_data.specular_color.z,
                1.0,
            ],
            alpha: material_data.alpha,
            shininess: material_data.shininess,
            reflectivity: material_data.reflectivity,
            refraction_index: material_data.refraction_index,
            normal_map_strength: material_data.normal_map_strength,
            roughness: material_data.roughness,
            receive_shadow: material_data.receive_shadow as u32,
            textures_used: textures_used
        }
    }
}

pub struct MaterialBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,

    pub bind_group_layout: Option<BindGroupLayout>,
    pub bind_group: Option<BindGroup>
}

impl RenderItem for MaterialBuffer
{
    render_item_impl_default!();
}

impl MaterialBuffer
{
    pub fn new(wgpu: &mut WGpu, material: &Material, additional_textures: Option<&Vec<(&Texture, u32)>>) -> MaterialBuffer
    {
        let empty_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("Empty Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut buffer = MaterialBuffer
        {
            name: material.get_base().name.clone(),
            buffer: empty_buffer,
            bind_group_layout: None,
            bind_group: None
        };

        buffer.to_buffer(wgpu, material, additional_textures);
        buffer.create_binding_groups(wgpu, material, additional_textures);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, material: &Material, additional_textures: Option<&Vec<(&Texture, u32)>>)
    {
        let mut material_uniform = MaterialUniform::new(material);

        if let Some(additional_textures) = additional_textures
        {
            for (_texture, texture_id) in additional_textures
            {
                material_uniform.textures_used |= 0x1 << texture_id;
            }
        }

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&self.name),
                contents: bytemuck::cast_slice(&[material_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, material: &Material)
    {
        let material_uniform = MaterialUniform::new(material);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[material_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn create_binding_groups(&mut self, wgpu: &mut WGpu, material: &Material, additional_textures: Option<&Vec<(&Texture, u32)>>)
    {
        let device = wgpu.device();

        let mut layout_group_vec: Vec<wgpu::BindGroupLayoutEntry> = vec![];
        let mut group_vec: Vec<wgpu::BindGroupEntry<'_>> = vec![];

        let mut bind_id = 0;

        // ********* material buffer *********
        layout_group_vec.push(uniform::uniform_bind_group_layout_entry(bind_id, true, true));
        group_vec.push(uniform::uniform_bind_group(bind_id, &self.get_buffer()));
        bind_id += 1;

        // ********* textures *********
        let mut render_items = vec![];

        for texture_type in ALL_TEXTURE_TYPES
        {
            if material.has_texture(texture_type)
            {
                //let texture = material.get_data().texture_ambient.as_ref().unwrap().read().unwrap();
                //let render_item = texture.render_item.as_ref();
                let texture = material.get_texture_by_type(texture_type);
                let texture = texture.unwrap().clone();
                let mut texture = texture.write().unwrap();

                let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;

                swap(&mut texture.render_item, &mut render_item);

                render_items.push((render_item, bind_id));
            }
            else
            {
                render_items.push((None, bind_id));
            }

            bind_id += 1;
        }

        for (render_item, id) in &render_items
        {
            if let Some(render_item) = render_item
            {
                let render_item = get_render_item::<Texture>(render_item);

                let textures_layout_group = render_item.get_bind_group_layout_entries(*id);
                let textures_group = render_item.get_bind_group_entries(*id);

                layout_group_vec.append(&mut textures_layout_group.to_vec());
                group_vec.append(&mut textures_group.to_vec());
            }
        }

        // additional textures
        if let Some(additional_textures) = additional_textures
        {
            for (texture, id) in additional_textures
            {
                let textures_layout_group = texture.get_bind_group_layout_entries(*id);
                let textures_group = texture.get_bind_group_entries(*id);

                layout_group_vec.append(&mut textures_layout_group.to_vec());
                group_vec.append(&mut textures_group.to_vec());
            }
        }

        // ********* bind group *********
        let bind_group_layout_name = format!("{} material_bind_group_layout", self.name);
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries: &layout_group_vec.as_slice(),
            label: Some(bind_group_layout_name.as_str()),
        });

        let bind_group_name = format!("{} material_bind_group", self.name);
        let bind_group = device.create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &bind_group_layout,
                entries: &group_vec.as_slice(),
                label: Some(bind_group_name.as_str()),
            }
        );

        // ********* swap back *********
        let mut i = 0;
        for texture_type in ALL_TEXTURE_TYPES
        {
            if material.has_texture(texture_type)
            {
                let texture = material.get_texture_by_type(texture_type);
                let texture = texture.unwrap().clone();
                let mut texture = texture.write().unwrap();

                swap(&mut render_items[i].0, &mut texture.render_item);
            }

            i += 1;
        }

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);
    }
}
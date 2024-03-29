use std::{mem::swap, collections::HashMap};

use wgpu::{util::DeviceExt, BindGroupLayout, BindGroup};

use crate::{state::{helper::render_item::{RenderItem, get_render_item, RenderItemType}, scene::{components::{material::{Material, TextureType, ALL_TEXTURE_TYPES, TextureState}, component::Component}, texture::TextureItem}}, render_item_impl_default};

use super::{wgpu::WGpu, uniform, texture::{Texture, TextureFormat}};

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
    10: environment

    11: custom 0
    12: custom 1
    13: custom 2
    14: custom 3

    15: depth
*/

//pub const ADDITIONAL_START_INDEX: u32 = 20;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform
{
    pub ambient_color: [f32; 4],
    pub base_color: [f32; 4],
    pub specular_color: [f32; 4],

    pub highlight_color: [f32; 4],

    pub alpha: f32,
    pub shininess: f32,
    pub reflectivity: f32,
    pub refraction_index: f32,

    pub normal_map_strength: f32,
    pub roughness: f32,
    pub receive_shadow: u32,

    pub unlit: u32,

    pub textures_used: u32,

    pub __padding: [u32; 3]
}

impl MaterialUniform
{
    pub fn new(material: &Material, has_default_env_map: bool) -> Self
    {
        let material_data = material.get_data();

        let mut textures_used: u32 = 0;
        if material.is_texture_enabled(TextureType::AmbientEmissive)                    { textures_used |= 1 << 1; }
        if material.is_texture_enabled(TextureType::Base)                               { textures_used |= 1 << 2; }
        if material.is_texture_enabled(TextureType::Specular)                           { textures_used |= 1 << 3; }
        if material.is_texture_enabled(TextureType::Normal)                             { textures_used |= 1 << 4; }
        if material.is_texture_enabled(TextureType::Alpha)                              { textures_used |= 1 << 5; }
        if material.is_texture_enabled(TextureType::Roughness)                          { textures_used |= 1 << 6; }
        if material.is_texture_enabled(TextureType::AmbientOcclusion)                   { textures_used |= 1 << 7; }
        if material.is_texture_enabled(TextureType::Reflectivity)                       { textures_used |= 1 << 8; }
        if material.is_texture_enabled(TextureType::Shininess)                          { textures_used |= 1 << 9; }
        if material.is_texture_enabled(TextureType::Environment) || has_default_env_map { textures_used |= 1 << 10; }

        if material.is_texture_enabled(TextureType::Custom0)                            { textures_used |= 1 << 11; }
        if material.is_texture_enabled(TextureType::Custom1)                            { textures_used |= 1 << 12; }
        if material.is_texture_enabled(TextureType::Custom2)                            { textures_used |= 1 << 13; }
        if material.is_texture_enabled(TextureType::Custom3)                            { textures_used |= 1 << 14; }

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
            highlight_color:
            [
                material_data.highlight_color.x,
                material_data.highlight_color.y,
                material_data.highlight_color.z,
                1.0,
            ],
            alpha: material_data.alpha,
            shininess: material_data.shininess,
            reflectivity: material_data.reflectivity,
            refraction_index: material_data.refraction_index,
            normal_map_strength: material_data.normal_map_strength,
            roughness: material_data.roughness,
            receive_shadow: material_data.receive_shadow as u32,
            unlit: material_data.unlit_shading as u32,
            textures_used: textures_used,

            __padding: [0, 0, 0]
        }
    }
}

pub struct MaterialBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,

    empty_texture: Texture,

    pub bind_group_layout: Option<BindGroupLayout>,
    pub bind_group: Option<BindGroup>
}

impl RenderItem for MaterialBuffer
{
    render_item_impl_default!();
}

impl MaterialBuffer
{
    pub fn new(wgpu: &mut WGpu, material: &Material, default_env_map: Option<TextureState>, additional_textures: Option<&Vec<(&Texture, u32)>>) -> MaterialBuffer
    {
        let empty_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("Empty Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let empty_texture = Texture::new_empty_texture(wgpu, format!("empty material {} texture", material.get_base().name).as_str(), TextureFormat::Srgba);

        let mut buffer = MaterialBuffer
        {
            name: material.get_base().name.clone(),
            buffer: empty_buffer,
            empty_texture,
            bind_group_layout: None,
            bind_group: None
        };

        buffer.to_buffer(wgpu, material, default_env_map.clone(), additional_textures);
        buffer.create_binding_groups(wgpu, material, default_env_map, additional_textures);

        buffer
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, material: &Material, default_env_map: Option<TextureState>, additional_textures: Option<&Vec<(&Texture, u32)>>)
    {
        let mut material_uniform = MaterialUniform::new(material, default_env_map.is_some());

        if let Some(additional_textures) = additional_textures
        {
            for (_texture, texture_id) in additional_textures
            {
                //material_uniform.textures_used |= 0x1 << texture_id;
                material_uniform.textures_used |= 1 << texture_id;
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

    /*
    pub fn update_buffer(&mut self, wgpu: &mut WGpu, material: &Material, has_default_env_tex: bool)
    {
        let material_uniform = MaterialUniform::new(material, has_default_env_tex);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[material_uniform]));
    }
    */

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn create_binding_groups(&mut self, wgpu: &mut WGpu, material: &Material, default_env_map: Option<TextureState>, additional_textures: Option<&Vec<(&Texture, u32)>>)
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
        let mut texture_render_items: HashMap<u64, (RenderItemType, TextureItem)> = HashMap::new();
        let mut texture_render_items_dir = vec![];

        for texture_type in ALL_TEXTURE_TYPES
        {
            let mut texture = None;
            if texture_type == TextureType::Environment && default_env_map.is_some()
            {
                texture = default_env_map.clone();
            }

            if material.has_texture(texture_type)
            {
                texture = material.get_texture_by_type(texture_type).clone();
            }

            if let Some(texture) = texture
            {
                let enabled = texture.enabled;

                if enabled
                {
                    let texture_arc = texture.get();
                    let mut texture = texture_arc.write().unwrap();

                    if !texture_render_items.contains_key(&texture.id) && texture.render_item.is_some()
                    {
                        let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
                        swap(&mut texture.render_item, &mut render_item);

                        texture_render_items.insert(texture.id, (render_item.unwrap(), texture_arc.clone()));
                    }

                    texture_render_items_dir.push((Some(texture.id), bind_id));
                }
                else
                {
                    texture_render_items_dir.push((None, bind_id));
                }
            }
            else
            {
                texture_render_items_dir.push((None, bind_id));
            }

            bind_id += 2;
        }

        for (texture_id, bind_id) in &texture_render_items_dir
        {
            if let Some(texture_id) = texture_id
            {
                let render_item = texture_render_items.get(texture_id).unwrap();
                let render_item = get_render_item::<Texture>(&render_item.0);

                let textures_layout_group = render_item.get_bind_group_layout_entries(*bind_id);
                let textures_group = render_item.get_bind_group_entries(*bind_id);

                layout_group_vec.append(&mut textures_layout_group.to_vec());
                group_vec.append(&mut textures_group.to_vec());
            }
            else
            {
                let textures_layout_group = self.empty_texture.get_bind_group_layout_entries(*bind_id);
                let textures_group = self.empty_texture.get_bind_group_entries(*bind_id);

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
        for (_tex_id, (render_item, texture)) in texture_render_items
        {
            texture.write().unwrap().render_item = Some(render_item);
        }

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);
    }
}
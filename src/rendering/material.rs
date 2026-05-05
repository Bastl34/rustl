use std::{mem::swap, collections::HashMap};

use wgpu::{BindGroup, BindGroupLayout, Sampler, util::DeviceExt};

use crate::{render_item_impl_default, rendering::bind_groups::uniform, state::{helper::render_item::{get_render_item, RenderItem, RenderItemType}, resources::texture::TextureItem, scene::components::{component::Component, material::{Material, TextureState, TextureType, ALL_TEXTURE_TYPES, TEXTURE_AMOUNT}}}};

use super::{texture::{Texture, TextureFormat}, wgpu::WGpu};

//TODO: future: compile shaders for each texture combination to prevent branching/if statements

/*
    textures:

    0: reserved (material buffer)
    1: reserved (texture transform array)

    2: ambient
    3: base (albedo)
    4: specular
    5: normal
    6: alpha
    7: roughness
    8: ambient occlusion
    9: reflectivity
    10: shininess
    11: environment

    12: custom 0
    13: custom 1
    14: custom 2
    15: custom 3

    // additional textures
    16: depth
*/

//pub const ADDITIONAL_START_INDEX: u32 = 20;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextureTransform
{
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,

    pub uv_index: u32,

    pub _padding: [f32; 2]
}

impl TextureTransform
{
    pub fn new(texture_state: &TextureState) -> Self
    {
        Self
        {
            offset: [texture_state.transform.offset.x, texture_state.transform.offset.y],
            scale: [texture_state.transform.scale.x, texture_state.transform.scale.y],
            rotation: texture_state.transform.rotation,
            uv_index: texture_state.transform.uv_index,
            _padding: [0.0, 0.0],
        }
    }

    pub fn default() -> Self
    {
        Self
        {
            offset: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            uv_index: 0,
            _padding: [0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform
{
    pub ambient_color: [f32; 4],
    pub base_color: [f32; 4],
    pub specular_color: [f32; 4],

    pub highlight_color: [f32; 4],
    pub locked_color: [f32; 4],

    pub blend_mode: u32,

    pub alpha: f32,
    pub alpha_cutoff: f32,

    pub shininess: f32,
    pub reflectivity: f32,
    pub refraction_index: f32,

    pub normal_map_strength: f32,
    pub roughness: f32,
    pub receive_shadow: u32,

    pub unlit: u32,

    pub ibl_diffuse_intensity: f32,

    //pub _padding1: [u32; 2],
    pub _padding1: u32,

    pub texture_transforms: [TextureTransform; TEXTURE_AMOUNT],
    pub textures_used: u32,

    pub _padding2: [u32; 3]
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

        let mut texture_transforms = [TextureTransform::default(); TEXTURE_AMOUNT];

        for (i, texture_type) in ALL_TEXTURE_TYPES.iter().enumerate()
        {
            let texture_state = material.get_texture_by_type(*texture_type);

            if let Some(texture_state) = texture_state.as_ref()
            {
                texture_transforms[i] = TextureTransform::new(texture_state);
            }
        }

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
            locked_color:
            [
                material_data.locked_color.x,
                material_data.locked_color.y,
                material_data.locked_color.z,
                1.0,
            ],

            blend_mode: material_data.blend_mode as u32,
            alpha: material_data.alpha,
            alpha_cutoff: material_data.alpha_cutoff.unwrap_or(0.0),

            shininess: material_data.shininess,
            reflectivity: material_data.reflectivity,
            refraction_index: material_data.refraction_index,
            normal_map_strength: material_data.normal_map_strength,
            roughness: material_data.roughness,
            receive_shadow: material_data.receive_shadow as u32,
            unlit: material_data.unlit_shading as u32,
            ibl_diffuse_intensity: material_data.ibl_diffuse_intensity,

            texture_transforms,
            textures_used: textures_used,

            //_padding1: [0, 0],
            _padding1: 0,
            _padding2: [0, 0, 0],
        }
    }
}

pub struct MaterialBuffer
{
    pub name: String,

    buffer: wgpu::Buffer,

    empty_texture: Texture,

    default_texture_sampler: wgpu::Sampler,

    pub bind_group_layout: Option<BindGroupLayout>,
    pub bind_group: Option<BindGroup>,
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
        let default_texture_sampler = Self::create_default_sampler(wgpu);

        let mut buffer = MaterialBuffer
        {
            name: material.get_base().name.clone(),
            buffer: empty_buffer,
            empty_texture,
            default_texture_sampler,
            bind_group_layout: None,
            bind_group: None
        };

        buffer.to_buffers(wgpu, material, default_env_map.clone(), additional_textures);
        buffer.create_binding_groups(wgpu, material, default_env_map, additional_textures);

        buffer
    }

    pub fn to_buffers(&mut self, wgpu: &mut WGpu, material: &Material, default_env_map: Option<TextureState>, additional_textures: Option<&Vec<(&Texture, u32)>>)
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

    pub fn create_binding_groups(&mut self, wgpu: &mut WGpu, material: &Material, default_env_map: Option<TextureState>, additional_textures: Option<&Vec<(&Texture, u32)>>)
    {
        let mut layout_group_vec: Vec<wgpu::BindGroupLayoutEntry> = vec![];
        let mut group_vec: Vec<wgpu::BindGroupEntry<'_>> = vec![];

        let mut bind_id = 0;

        // ********* material buffer *********
        layout_group_vec.push(uniform::uniform_bind_group_layout_entry(bind_id, false, true));
        group_vec.push(wgpu::BindGroupEntry { binding: bind_id, resource: self.buffer.as_entire_binding() });

        bind_id += 1;

        // ********* textures *********
        let mut texture_render_items: HashMap<u32, (RenderItemType, TextureItem, wgpu::Sampler)> = HashMap::new();
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

            if let Some(texture_state) = texture
            {
                let enabled = texture_state.enabled;

                if let Some(texture_arc) = texture_state.get()
                {
                    if enabled
                    {
                        let mut texture = texture_arc.write().unwrap();

                        if !texture_render_items.contains_key(&texture.id) && texture.render_item.is_some()
                        {
                            let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
                            swap(&mut texture.render_item, &mut render_item);

                            let sampler = Self::create_sampler(wgpu, &texture_state);

                            texture_render_items.insert(texture.id, (render_item.unwrap(), texture_arc.clone(), sampler));
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
                let render_item_obj = texture_render_items.get(texture_id).unwrap();
                let render_item = get_render_item::<Texture>(&render_item_obj.0);

                let textures_layout_group = render_item.get_bind_group_layout_entries(*bind_id);
                let textures_group = Texture::get_bind_group_entries(*bind_id, render_item.get_view(), &render_item_obj.2);

                layout_group_vec.append(&mut textures_layout_group.to_vec());
                group_vec.append(&mut textures_group.to_vec());
            }
            else
            {
                let textures_layout_group = self.empty_texture.get_bind_group_layout_entries(*bind_id);
                let textures_group = Texture::get_bind_group_entries(*bind_id, self.empty_texture.get_view(), &self.default_texture_sampler);

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

                // TODO: additional textures texture state????
                let textures_group = Texture::get_bind_group_entries(*id, texture.get_view(), &self.default_texture_sampler);

                layout_group_vec.append(&mut textures_layout_group.to_vec());
                group_vec.append(&mut textures_group.to_vec());
            }
        }

        // ********* bind group *********
        let bind_group_layout_name = format!("{} material_bind_group_layout", self.name);
        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries: &layout_group_vec.as_slice(),
            label: Some(bind_group_layout_name.as_str()),
        });

        let bind_group_name = format!("{} material_bind_group", self.name);
        let bind_group = wgpu.device().create_bind_group
        (
            &wgpu::BindGroupDescriptor
            {
                layout: &bind_group_layout,
                entries: &group_vec.as_slice(),
                label: Some(bind_group_name.as_str()),
            }
        );

        // ********* swap back *********
        for (_tex_id, (render_item, texture, _)) in texture_render_items
        {
            texture.write().unwrap().render_item = Some(render_item);
        }

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);
    }

    pub fn create_default_sampler(wgpu: &mut WGpu) -> Sampler
    {
        wgpu.device().create_sampler(&wgpu::SamplerDescriptor
        {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        })
    }

    pub fn create_sampler(wgpu: &mut WGpu, texture_state: &TextureState) -> Sampler
    {
        let address_mode_u;
        match texture_state.sampler.address_mode_u
        {
            crate::state::scene::components::material::TextureAddressMode::ClampToEdge => address_mode_u = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::components::material::TextureAddressMode::Repeat => address_mode_u = wgpu::AddressMode::Repeat,
            crate::state::scene::components::material::TextureAddressMode::MirrorRepeat => address_mode_u = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::components::material::TextureAddressMode::ClampToBorder => address_mode_u = wgpu::AddressMode::ClampToBorder,
        }

        let address_mode_v;
        match texture_state.sampler.address_mode_v
        {
            crate::state::scene::components::material::TextureAddressMode::ClampToEdge => address_mode_v = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::components::material::TextureAddressMode::Repeat => address_mode_v = wgpu::AddressMode::Repeat,
            crate::state::scene::components::material::TextureAddressMode::MirrorRepeat => address_mode_v = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::components::material::TextureAddressMode::ClampToBorder => address_mode_v = wgpu::AddressMode::ClampToBorder,
        }

        let address_mode_w;
        match texture_state.sampler.address_mode_w
        {
            crate::state::scene::components::material::TextureAddressMode::ClampToEdge => address_mode_w = wgpu::AddressMode::ClampToEdge,
            crate::state::scene::components::material::TextureAddressMode::Repeat => address_mode_w = wgpu::AddressMode::Repeat,
            crate::state::scene::components::material::TextureAddressMode::MirrorRepeat => address_mode_w = wgpu::AddressMode::MirrorRepeat,
            crate::state::scene::components::material::TextureAddressMode::ClampToBorder => address_mode_w = wgpu::AddressMode::ClampToBorder,
        }

        let mag_filter;
        match texture_state.sampler.mag_filter
        {
            crate::state::scene::components::material::TextureFilterMode::Nearest => mag_filter = wgpu::FilterMode::Nearest,
            crate::state::scene::components::material::TextureFilterMode::Linear => mag_filter = wgpu::FilterMode::Linear,
        }

        let min_filter;
        match texture_state.sampler.min_filter
        {
            crate::state::scene::components::material::TextureFilterMode::Nearest => min_filter = wgpu::FilterMode::Nearest,
            crate::state::scene::components::material::TextureFilterMode::Linear => min_filter = wgpu::FilterMode::Linear,
        }

        let mipmap_filter;
        match texture_state.sampler.mipmap_filter
        {
            crate::state::scene::components::material::TextureFilterMode::Nearest => mipmap_filter = wgpu::MipmapFilterMode::Nearest,
            crate::state::scene::components::material::TextureFilterMode::Linear => mipmap_filter = wgpu::MipmapFilterMode::Linear,
        }

        let sampler = wgpu.device().create_sampler(&wgpu::SamplerDescriptor
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
}
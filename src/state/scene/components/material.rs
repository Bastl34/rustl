#![allow(dead_code)]

use std::str::FromStr;

use nalgebra::{Vector2, Vector3, Vector4};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, FromRepr, EnumString};

use crate::helper::change_tracker::ChangeTracker;
use crate::helper::math::approx_equal;
use crate::helper::option_or_id::OptionOrId;
use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update, component_impl_set_enabled};
use crate::state::scene::node::NodeItem;
use crate::{state::resources::texture::TextureItem, helper};
use crate::state::scene::exporter::serialization_helper;

use super::component::{Component, ComponentItem, ComponentBase};

//pub type MaterialItem = Arc<RwLock<Box<Material>>>;
//pub type MaterialItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;
pub type MaterialItem = ComponentItem;

//pub type MaterialBoxItem = Box<dyn Any + Send + Sync>;
//pub type MaterialItem = Arc<RwLock<MaterialBoxItem>>;

#[derive(Clone, Copy, PartialEq, Debug, Display, EnumIter, FromRepr, EnumString, Serialize, Deserialize)]
pub enum TextureType
{
    AmbientEmissive,
    Base,
    Specular,
    Normal,
    Alpha,
    Roughness,
    AmbientOcclusion,
    Reflectivity,
    Shininess,
    Environment,

    Custom0,
    Custom1,
    Custom2,
    Custom3
}

#[derive(Clone, Copy, PartialEq, Debug, Display, EnumIter, Serialize, Deserialize)]
pub enum BlendMode
{
    Opaque,
    Mask,
    Blend
}

pub const TEXTURE_AMOUNT: usize = 14; // without additional textures
pub const ALL_TEXTURE_TYPES: [TextureType; TEXTURE_AMOUNT] =
[
    TextureType::AmbientEmissive,
    TextureType::Base,
    TextureType::Specular,
    TextureType::Normal,
    TextureType::Alpha,
    TextureType::Roughness,
    TextureType::AmbientOcclusion,
    TextureType::Reflectivity,
    TextureType::Shininess,
    TextureType::Environment,

    TextureType::Custom0,
    TextureType::Custom1,
    TextureType::Custom2,
    TextureType::Custom3
];

#[derive(PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum TextureAddressMode
{
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder
}

#[derive(PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum TextureFilterMode
{
    Nearest,
    Linear
}


#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TextureTransform
{
    pub offset: Vector2::<f32>,
    pub scale: Vector2::<f32>,
    pub rotation: f32,

    pub uv_index: u32,
}

impl TextureTransform
{
    pub fn default() -> TextureTransform
    {
        TextureTransform
        {
            offset: Vector2::<f32>::new(0.0, 0.0),
            scale: Vector2::<f32>::new(1.0, 1.0),
            rotation: 0.0,

            uv_index: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TextureSampler
{
    pub address_mode_u: TextureAddressMode,
    pub address_mode_v: TextureAddressMode,
    pub address_mode_w: TextureAddressMode,
    pub mag_filter: TextureFilterMode,
    pub min_filter: TextureFilterMode,
    pub mipmap_filter: TextureFilterMode,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TextureState
{
    #[serde(serialize_with = "serialization_helper::serialize_texture", deserialize_with = "serialization_helper::deserialize_texture")]
    pub item: OptionOrId<TextureItem>,
    pub enabled: bool,

    pub sampler: TextureSampler,
    pub transform: TextureTransform
}

impl TextureState
{
    pub fn new(item: TextureItem) -> TextureState
    {
        TextureState
        {
            item: OptionOrId::Some(item),
            enabled: true,

            sampler: TextureSampler
            {
                address_mode_u: TextureAddressMode::ClampToEdge,
                address_mode_v: TextureAddressMode::ClampToEdge,
                address_mode_w: TextureAddressMode::ClampToEdge,
                mag_filter: TextureFilterMode::Linear,
                min_filter: TextureFilterMode::Linear,
                mipmap_filter: TextureFilterMode::Linear
            },

            transform: TextureTransform::default(),
        }
    }

    pub fn get(&self) -> Option<&TextureItem>
    {
        self.item.as_ref()
    }
}

#[derive(Serialize, Deserialize)]
pub struct MaterialData
{
    pub ambient_color: Vector3<f32>,
    pub base_color: Vector3<f32>,
    pub specular_color: Vector3<f32>,

    pub highlight_color: Vector3<f32>,
    pub locked_color: Vector3<f32>,

    pub texture_ambient: Option<TextureState>,
    pub texture_base: Option<TextureState>,
    pub texture_specular: Option<TextureState>,
    pub texture_normal: Option<TextureState>,
    pub texture_alpha: Option<TextureState>,
    pub texture_roughness: Option<TextureState>,
    pub texture_ambient_occlusion: Option<TextureState>,
    pub texture_reflectivity: Option<TextureState>,
    pub texture_shininess: Option<TextureState>,
    pub texture_environment: Option<TextureState>,

    pub texture_custom0: Option<TextureState>,
    pub texture_custom1: Option<TextureState>,
    pub texture_custom2: Option<TextureState>,
    pub texture_custom3: Option<TextureState>,

    pub blend_mode: BlendMode,

    pub alpha: f32,
    pub alpha_cutoff: Option<f32>,
    pub shininess: f32,
    pub reflectivity: f32,
    pub refraction_index: f32,

    pub normal_map_strength: f32,

    pub unlit_shading: bool, // todo (no shading at all - just base color and base texture)
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub shadow_softness: f32,

    pub roughness: f32, //degree in rad (max PI/2)

    pub smooth_shading: bool,

    pub reflection_only: bool,
    pub backface_culling: bool
}

#[derive(Serialize, Deserialize)]
pub struct Material
{
    base: ComponentBase,
    data: ChangeTracker<MaterialData>,
}

impl Material
{
    pub fn new(name: &str) -> Material
    {
        let material_data = MaterialData
        {
            ambient_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            base_color: Vector3::<f32>::new(1.0, 1.0, 1.0),
            specular_color: Vector3::<f32>::new(0.8, 0.8, 0.8),

            highlight_color: Vector3::<f32>::new(0.0, 1.0, 0.0),
            locked_color: Vector3::<f32>::new(1.0, 0.0, 0.0),

            texture_ambient: None,
            texture_base: None,
            texture_specular: None,
            texture_normal: None,
            texture_alpha: None,
            texture_roughness: None,
            texture_ambient_occlusion: None,
            texture_reflectivity: None,
            texture_shininess: None,
            texture_environment: None,

            texture_custom0: None,
            texture_custom1: None,
            texture_custom2: None,
            texture_custom3: None,

            blend_mode: BlendMode::Blend,

            alpha: 1.0,
            alpha_cutoff: None,
            shininess: 150.0,
            reflectivity: 0.0,
            refraction_index: 1.0,

            normal_map_strength: 1.0,

            unlit_shading: false,
            cast_shadow: true,
            receive_shadow: true,
            shadow_softness: 0.01,

            roughness: 0.0,

            smooth_shading: true,

            reflection_only: false,
            backface_culling: true,
        };

        Material
        {
            base: ComponentBase::new(name.to_string(), "Material".to_string(), "🎨".to_string()),
            data: ChangeTracker::new(material_data),
        }
    }

    pub fn get_data(&self) -> &MaterialData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MaterialData>
    {
        &mut self.data
    }

    pub fn apply_diff_without_textures(&mut self, new_mat: &Material)
    {
        let default_material = Material::new("");
        let default_material_data = default_material.get_data();

        let new_mat_data = new_mat.get_data();

        let data = self.data.get_mut();

        // ********** colors **********

        // ambient
        if
            !helper::math::approx_equal(default_material_data.ambient_color.x, new_mat_data.ambient_color.x)
            ||
            !helper::math::approx_equal(default_material_data.ambient_color.y, new_mat_data.ambient_color.y)
            ||
            !helper::math::approx_equal(default_material_data.ambient_color.z, new_mat_data.ambient_color.z)
        {
            data.ambient_color = new_mat_data.ambient_color;
        }

        // base
        if
            !helper::math::approx_equal(default_material_data.base_color.x, new_mat_data.base_color.x)
            ||
            !helper::math::approx_equal(default_material_data.base_color.y, new_mat_data.base_color.y)
            ||
            !helper::math::approx_equal(default_material_data.base_color.z, new_mat_data.base_color.z)
        {
            data.base_color = new_mat_data.base_color;
        }

        // specular
        if
            !helper::math::approx_equal(default_material_data.specular_color.x, new_mat_data.specular_color.x)
            ||
            !helper::math::approx_equal(default_material_data.specular_color.y, new_mat_data.specular_color.y)
            ||
            !helper::math::approx_equal(default_material_data.specular_color.z, new_mat_data.specular_color.z)
        {
            data.specular_color = new_mat_data.specular_color;
        }

        // ********** other attributes **********
        if !helper::math::approx_equal(default_material_data.alpha, new_mat_data.alpha) { data.alpha = new_mat_data.alpha; }
        if !helper::math::approx_equal(default_material_data.shininess, new_mat_data.shininess) { data.shininess = new_mat_data.shininess; }
        if !helper::math::approx_equal(default_material_data.reflectivity, new_mat_data.reflectivity) { data.reflectivity = new_mat_data.reflectivity; }
        if !helper::math::approx_equal(default_material_data.refraction_index, new_mat_data.refraction_index) { data.refraction_index = new_mat_data.refraction_index; }

        if !helper::math::approx_equal(default_material_data.normal_map_strength, new_mat_data.normal_map_strength) { data.normal_map_strength = new_mat_data.normal_map_strength; }

        if default_material_data.cast_shadow != new_mat_data.cast_shadow { data.cast_shadow = new_mat_data.cast_shadow; }
        if default_material_data.receive_shadow != new_mat_data.receive_shadow { data.receive_shadow = new_mat_data.receive_shadow; }
        if !helper::math::approx_equal(default_material_data.shadow_softness, new_mat_data.shadow_softness) { data.shadow_softness = new_mat_data.shadow_softness; }

        if !helper::math::approx_equal(default_material_data.roughness, new_mat_data.roughness) { data.roughness = new_mat_data.roughness; }

        if default_material_data.smooth_shading != new_mat_data.smooth_shading { data.smooth_shading = new_mat_data.smooth_shading; }

        if default_material_data.reflection_only != new_mat_data.reflection_only { data.reflection_only = new_mat_data.reflection_only; }
        if default_material_data.backface_culling != new_mat_data.backface_culling { data.backface_culling = new_mat_data.backface_culling; }
    }

    pub fn apply_diff(&mut self, new_mat: &Material)
    {
        // ********** default settings **********
        self.apply_diff_without_textures(new_mat);

        // ********** textures **********
        let default_material = Material::new("");
        let default_material_data = default_material.data.get_ref();

        let new_mat_data = new_mat.get_data();

        macro_rules! compare_and_apply_texture_diff
        {
            ($self_tex:expr, $default_material_tex:expr, $new_mat_tex:expr) =>
            {
                if $default_material_tex.is_some() != $new_mat_tex.is_some()
                    ||
                    (
                        $default_material_tex.is_some() && $new_mat_tex.is_some() && $new_mat_tex.unwrap().get().is_some()
                        &&
                        $default_material_tex.unwrap().get().is_some()
                        &&
                        $default_material_tex.unwrap().get().unwrap().read().unwrap().hash != $new_mat_tex.unwrap().get().unwrap().read().unwrap().hash
                    )
                {
                    $self_tex = $new_mat_tex.clone();
                }
            };
        }

        let data = self.data.get_mut();

        compare_and_apply_texture_diff!(data.texture_ambient, default_material_data.texture_ambient.as_ref(), new_mat_data.texture_ambient.clone());
        compare_and_apply_texture_diff!(data.texture_base, default_material_data.texture_base.as_ref(), new_mat_data.texture_base.clone());
        compare_and_apply_texture_diff!(data.texture_specular, default_material_data.texture_specular.as_ref(), new_mat_data.texture_specular.clone());
        compare_and_apply_texture_diff!(data.texture_normal, default_material_data.texture_normal.as_ref(), new_mat_data.texture_normal.clone());
        compare_and_apply_texture_diff!(data.texture_alpha, default_material_data.texture_alpha.as_ref(), new_mat_data.texture_alpha.clone());
        compare_and_apply_texture_diff!(data.texture_roughness, default_material_data.texture_roughness.as_ref(), new_mat_data.texture_roughness.clone());
        compare_and_apply_texture_diff!(data.texture_ambient_occlusion, default_material_data.texture_ambient_occlusion.as_ref(), new_mat_data.texture_ambient_occlusion.clone());
        compare_and_apply_texture_diff!(data.texture_reflectivity, default_material_data.texture_reflectivity.as_ref(), new_mat_data.texture_reflectivity.clone());
        compare_and_apply_texture_diff!(data.texture_shininess, default_material_data.texture_shininess.as_ref(), new_mat_data.texture_shininess.clone());
        compare_and_apply_texture_diff!(data.texture_environment, default_material_data.texture_environment.as_ref(), new_mat_data.texture_environment.clone());

        compare_and_apply_texture_diff!(data.texture_custom0, default_material_data.texture_custom0.as_ref(), new_mat_data.texture_custom0.clone());
        compare_and_apply_texture_diff!(data.texture_custom1, default_material_data.texture_custom1.as_ref(), new_mat_data.texture_custom1.clone());
        compare_and_apply_texture_diff!(data.texture_custom2, default_material_data.texture_custom2.as_ref(), new_mat_data.texture_custom2.clone());
        compare_and_apply_texture_diff!(data.texture_custom3, default_material_data.texture_custom3.as_ref(), new_mat_data.texture_custom3.clone());
    }

    pub fn print(&self)
    {
        let data = self.data.get_ref();

        println!("ambient_color: {:?}", data.ambient_color);
        println!("base_color: {:?}", data.base_color);
        println!("specular_color: {:?}", data.specular_color);

        println!("texture_base: {:?}", data.texture_base.is_some());
        println!("texture_specular: {:?}", data.texture_specular.is_some());
        println!("texture_normal: {:?}", data.texture_normal.is_some());
        println!("texture_alpha: {:?}", data.texture_alpha.is_some());
        println!("texture_roughness: {:?}", data.texture_roughness.is_some());
        println!("texture_ambient_occlusion: {:?}", data.texture_ambient_occlusion.is_some());
        println!("texture_reflectivity: {:?}", data.texture_reflectivity.is_some());
        println!("texture_shininess: {:?}", data.texture_shininess.is_some());
        println!("texture_environment: {:?}", data.texture_environment.is_some());

        println!("texture_custom0: {:?}", data.texture_custom0.is_some());
        println!("texture_custom1: {:?}", data.texture_custom1.is_some());
        println!("texture_custom2: {:?}", data.texture_custom2.is_some());
        println!("texture_custom3: {:?}", data.texture_custom3.is_some());

        println!("alpha: {:?}", data.alpha);
        println!("shininess: {:?}", data.shininess);
        println!("reflectivity: {:?}", data.reflectivity);
        println!("refraction_index: {:?}", data.refraction_index);

        println!("normal_map_strength: {:?}", data.normal_map_strength);

        println!("cast_shadow: {:?}", data.cast_shadow);
        println!("receive_shadow: {:?}", data.receive_shadow);
        println!("shadow_softness: {:?}", data.shadow_softness);

        println!("roughness: {:?}", data.roughness);

        println!("smooth_shading: {:?}", data.smooth_shading);

        println!("reflection_only: {:?}", data.reflection_only);
        println!("backface_culling: {:?}", data.backface_culling);
    }

    pub fn remove_texture(&mut self, tex_type: TextureType)
    {
        let data = self.data.get_mut();

        match tex_type
        {
            TextureType::Base => { data.texture_base = None; },
            TextureType::AmbientEmissive => { data.texture_ambient = None; },
            TextureType::Specular => { data.texture_specular = None; },
            TextureType::Normal => { data.texture_normal = None; },
            TextureType::Alpha => { data.texture_alpha = None; },
            TextureType::Roughness => { data.texture_roughness = None; },
            TextureType::AmbientOcclusion => { data.texture_ambient_occlusion = None; },
            TextureType::Reflectivity => { data.texture_reflectivity = None; },
            TextureType::Shininess => { data.texture_shininess = None; },
            TextureType::Environment => { data.texture_environment = None; },

            TextureType::Custom0 => { data.texture_custom0 = None; },
            TextureType::Custom1 => { data.texture_custom1 = None; },
            TextureType::Custom2 => { data.texture_custom2 = None; },
            TextureType::Custom3 => { data.texture_custom3 = None; },
        }
    }

    pub fn remove_all_textures(&mut self)
    {
        for texture_type in ALL_TEXTURE_TYPES
        {
            self.remove_texture(texture_type);
        }
    }

    pub fn set_texture(&mut self, tex: TextureItem, tex_type: TextureType)
    {
        let data = self.data.get_mut();

        match tex_type
        {
            TextureType::Base => { data.texture_base = Some(TextureState::new(tex.clone())); },
            TextureType::AmbientEmissive => { data.texture_ambient = Some(TextureState::new(tex.clone())); },
            TextureType::Specular => { data.texture_specular = Some(TextureState::new(tex.clone())); },
            TextureType::Normal => { data.texture_normal = Some(TextureState::new(tex.clone())); },
            TextureType::Alpha => { data.texture_alpha = Some(TextureState::new(tex.clone())); },
            TextureType::Roughness => { data.texture_roughness = Some(TextureState::new(tex.clone())); },
            TextureType::AmbientOcclusion => { data.texture_ambient_occlusion = Some(TextureState::new(tex.clone())); },
            TextureType::Reflectivity => { data.texture_reflectivity = Some(TextureState::new(tex.clone())); },
            TextureType::Shininess => { data.texture_shininess = Some(TextureState::new(tex.clone())); },
            TextureType::Environment => { data.texture_environment = Some(TextureState::new(tex.clone())); },

            TextureType::Custom0 => { data.texture_custom0 = Some(TextureState::new(tex.clone())); },
            TextureType::Custom1 => { data.texture_custom1 = Some(TextureState::new(tex.clone())); },
            TextureType::Custom2 => { data.texture_custom2 = Some(TextureState::new(tex.clone())); },
            TextureType::Custom3 => { data.texture_custom3 = Some(TextureState::new(tex.clone())); },
        }
    }

    pub fn set_texture_from_string_type(&mut self, tex: TextureItem, tex_type: &str)
    {
        let tex_type = TextureType::from_str(tex_type);

        if tex_type.is_err()
        {
            dbg!("Invalid texture type: {}", tex_type.unwrap_err());
            return;
        }

        let texture_type = tex_type.unwrap();
        self.set_texture(tex, texture_type);
    }

    pub fn set_texture_state(&mut self, tex_type: TextureType, state: bool)
    {
        if !self.has_texture(tex_type)
        {
            return;
        }

        self.get_texture_by_type_mut(tex_type).unwrap().enabled = state;
    }

    pub fn has_texture_id(&self, texture_id: u64) -> bool
    {
        for texture_type in ALL_TEXTURE_TYPES
        {
            if let Some(texture) = self.get_texture_by_type(texture_type)
            {
                if texture.get().is_some() && texture.get().unwrap().read().unwrap().id == texture_id
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn has_any_texture(&self) -> bool
    {
        for texture_type in ALL_TEXTURE_TYPES
        {
            if self.get_texture_by_type(texture_type).is_some()
            {
                return true;
            }
        }

        false
    }

    pub fn has_transparency(&self) -> bool
    {
        let data = self.get_data();

        // alpha texture
        if data.texture_alpha.is_some()
        {
            return true;
        }

        // base texture alpha channel
        if let Some(texture_base) = &data.texture_base
        {
            if texture_base.get().is_some() && texture_base.get().unwrap().read().unwrap().get_data().has_transparency
            {
                return true;
            }
        }

        if !approx_equal(data.alpha, 1.0)
        {
            return true;
        }

        false
    }

    pub fn get_texture_by_type(&self, tex_type: TextureType) -> Option<TextureState>
    {
        let tex;

        let data = self.data.get_ref();

        match tex_type
        {
            TextureType::Base => { tex = data.texture_base.clone() },
            TextureType::AmbientEmissive => { tex = data.texture_ambient.clone() },
            TextureType::Specular => { tex = data.texture_specular.clone() },
            TextureType::Normal => { tex = data.texture_normal.clone() },
            TextureType::Alpha => { tex = data.texture_alpha.clone() },
            TextureType::Roughness => { tex = data.texture_roughness.clone() },
            TextureType::AmbientOcclusion => { tex = data.texture_ambient_occlusion.clone() },
            TextureType::Reflectivity => { tex = data.texture_reflectivity.clone() },
            TextureType::Shininess => { tex = data.texture_shininess.clone() },
            TextureType::Environment => { tex = data.texture_environment.clone() },

            TextureType::Custom0 => { tex = data.texture_custom0.clone() },
            TextureType::Custom1 => { tex = data.texture_custom1.clone() },
            TextureType::Custom2 => { tex = data.texture_custom2.clone() },
            TextureType::Custom3 => { tex = data.texture_custom3.clone() },
        }

        tex
    }

    pub fn get_texture_by_type_mut(&mut self, tex_type: TextureType) -> Option<&mut TextureState>
    {
        let tex: Option<&mut TextureState>;

        let data = self.data.get_mut();

        match tex_type
        {
            TextureType::Base => { if let Some(tex_state) = data.texture_base.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::AmbientEmissive => { if let Some(tex_state) = data.texture_ambient.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Specular => { if let Some(tex_state) = data.texture_specular.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Normal => { if let Some(tex_state) = data.texture_normal.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Alpha => { if let Some(tex_state) = data.texture_alpha.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Roughness => { if let Some(tex_state) = data.texture_roughness.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::AmbientOcclusion => { if let Some(tex_state) = data.texture_ambient_occlusion.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Reflectivity => { if let Some(tex_state) = data.texture_reflectivity.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Shininess => { if let Some(tex_state) = data.texture_shininess.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Environment => { if let Some(tex_state) = data.texture_environment.as_mut() { tex = Some(tex_state) } else { tex = None; } },

            TextureType::Custom0 => { if let Some(tex_state) = data.texture_custom0.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Custom1 => { if let Some(tex_state) = data.texture_custom1.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Custom2 => { if let Some(tex_state) = data.texture_custom2.as_mut() { tex = Some(tex_state) } else { tex = None; } },
            TextureType::Custom3 => { if let Some(tex_state) = data.texture_custom3.as_mut() { tex = Some(tex_state) } else { tex = None; } },
        }

        tex
    }

    pub fn get_all_textures(&self) -> Vec<TextureItem>
    {
        let mut textures = vec![];
        for texture_type in ALL_TEXTURE_TYPES
        {
            if let Some(texture) = self.get_texture_by_type(texture_type)
            {
                if let Some(tex_item) = texture.get()
                {
                    textures.push(tex_item.clone());
                }
            }
        }

        textures
    }

    pub fn has_texture(&self, tex_type: TextureType) -> bool
    {
        let tex = self.get_texture_by_type(tex_type);

        tex.is_some()
    }

    pub fn is_texture_enabled(&self, tex_type: TextureType) -> bool
    {
        let tex = self.get_texture_by_type(tex_type);

        tex.is_some() && tex.unwrap().enabled
    }

    pub fn remove_texture_by_id(&mut self, id: u64) -> bool
    {
        let mut removed = false;
        for texture_type in ALL_TEXTURE_TYPES
        {
            if let Some(texture) = self.get_texture_by_type(texture_type)
            {
                if texture.get().is_some() && texture.get().unwrap().read().unwrap().id == id
                {
                    self.remove_texture(texture_type);
                    removed = true;
                }
            }
        }

        removed
    }

    pub fn texture_dimension(&self, tex_type: TextureType) -> (u32, u32)
    {
        if let Some(tex_state) = self.get_texture_by_type(tex_type).as_ref()
        {
            if let Some(texture) = tex_state.get().as_ref()
            {
                return texture.read().unwrap().dimensions().clone();
            }
        }

        (0,0)
    }

    pub fn get_texture_pixel(&self, x: u32, y: u32, tex_type: TextureType) -> Vector4<f32>
    {
        if !self.has_texture(tex_type)
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        if let Some(tex_state) = self.get_texture_by_type(tex_type).as_ref()
        {
            if let Some(texture) = tex_state.get().as_ref()
            {
                return texture.read().unwrap().get_pixel_as_float_vec(x, y).clone();
            }
        }

        Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0)
    }

    pub fn get_texture_pixel_float(&self, x: f32, y: f32, tex_type: TextureType) -> Vector4<f32>
    {
        if !self.has_texture(tex_type)
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        let tex = self.get_texture_by_type(tex_type);
        if tex.is_none() || tex.clone().unwrap().get().is_none()
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        let tex = tex.as_ref().unwrap();
        let tex = tex.get().unwrap();
        let tex = tex.read().unwrap();

        let width = tex.width();
        let height = tex.height();

        let mut x = x * width as f32;
        let mut y = y * height as f32;
        if x < 0.0 { x = x + width as f32; }
        if y < 0.0 { y = y + height as f32; }

        let mut x0: u32 = x.floor() as u32;
        let mut x1: u32 = x.ceil() as u32;

        let mut y0: u32 = y.floor() as u32;
        let mut y1: u32 = y.ceil() as u32;

        // out of bounds check
        if x0 >= width { x0 = width - 1; }
        if y0 >= height { y0 = height - 1; }
        if x1 >= width { x1 = width - 1; }
        if y1 >= height { y1 = height - 1; }

        let x_f = x - x0 as f32;
        let y_f = y - y0 as f32;

        let p0 = tex.get_pixel_as_float_vec(x0, y0);
        let p1 = tex.get_pixel_as_float_vec(x1, y0);
        let p2 = tex.get_pixel_as_float_vec(x0, y1);
        let p3 = tex.get_pixel_as_float_vec(x1, y1);

        let p_res_1 = helper::math::interpolate_vec4(&p0, &p1, x_f);
        let p_res_2 = helper::math::interpolate_vec4(&p2, &p3, x_f);

        let res = helper::math::interpolate_vec4(&p_res_1, &p_res_2, y_f);

        res
    }

    pub fn ui_texture_state(&mut self, ui: &mut egui::Ui, tex_type: TextureType)
    {
        if self.get_texture_by_type(tex_type).is_none()
        {
            return;
        }

        let tex_id;

        let mut address_mode_u;
        let mut address_mode_v;
        let mut address_mode_w;
        let mut mag_filter;
        let mut min_filter;
        let mut mipmap_filter;

        let mut uv_offset;
        let mut uv_scale;
        let mut uv_rotation_deg;
        let mut uv_index;

        {
            let tex = self.get_texture_by_type(tex_type).unwrap();

            {
                let tex = tex.get().unwrap();
                let tex = tex.read().unwrap();
                tex_id = tex.id;
            }

            address_mode_u = tex.sampler.address_mode_u;
            address_mode_v = tex.sampler.address_mode_v;
            address_mode_w = tex.sampler.address_mode_w;
            mag_filter = tex.sampler.mag_filter;
            min_filter = tex.sampler.min_filter;
            mipmap_filter = tex.sampler.mipmap_filter;

            uv_offset = tex.transform.offset;
            uv_scale = tex.transform.scale;
            uv_rotation_deg = tex.transform.rotation.to_degrees();
            uv_index = tex.transform.uv_index;
        }

        let mut changed = false;

        // ********** sampler **********
        let sampler_id = ui.make_persistent_id(format!("material_tex_sampler_{}",tex_id));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), sampler_id, false).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                ui.label("Sampler")
            });
        }).body(|ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("Address Mode U:");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("address_mode_u")).selected_text(format!("{address_mode_u:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut address_mode_u, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_u, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_u, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_u, TextureAddressMode::Repeat, "Repeat").changed() || changed;
                });
            });

            ui.horizontal(|ui|
            {
                ui.label("Address Mode V:");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("address_mode_v")).selected_text(format!("{address_mode_v:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut address_mode_v, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_v, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_v, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_v, TextureAddressMode::Repeat, "Repeat").changed() || changed;
                });
            });

            ui.horizontal(|ui|
            {
                ui.label("Address Mode W:");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("address_mode_w")).selected_text(format!("{address_mode_w:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut address_mode_w, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_w, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_w, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || changed;
                    changed = ui.selectable_value(& mut address_mode_w, TextureAddressMode::Repeat, "Repeat").changed() || changed;
                });
            });

            ui.horizontal(|ui|
            {
                ui.label("Mag Filter: ");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("mag_filter")).selected_text(format!("{mag_filter:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut mag_filter, TextureFilterMode::Linear, "Linear").changed() || changed;
                    changed = ui.selectable_value(& mut mag_filter, TextureFilterMode::Nearest, "Nearest").changed() || changed;
                });
            });

            ui.horizontal(|ui|
            {
                ui.label("Min Filter: ");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("min_filter")).selected_text(format!("{min_filter:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut min_filter, TextureFilterMode::Linear, "Linear").changed() || changed;
                    changed = ui.selectable_value(& mut min_filter, TextureFilterMode::Nearest, "Nearest").changed() || changed;
                });
            });

            ui.horizontal(|ui|
            {
                ui.label("Mipmap Filter: ");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("mipmap_filter")).selected_text(format!("{mipmap_filter:?}")).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(& mut mipmap_filter, TextureFilterMode::Linear, "Linear").changed() || changed;
                    changed = ui.selectable_value(& mut mipmap_filter, TextureFilterMode::Nearest, "Nearest").changed() || changed;
                });
            });
        });


        // ********** transform **********
        let transform_id = ui.make_persistent_id(format!("material_tex_transform_{}",tex_id));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), transform_id, false).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                ui.label("Transform")
            });
        }).body(|ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("UV Offset:");
                changed = ui.add(egui::DragValue::new(&mut uv_offset.x).speed(0.001).prefix("u: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut uv_offset.y).speed(0.001).prefix("v: ")).changed() || changed;
            });

            ui.horizontal(|ui|
            {
                ui.label("UV Scale:");
                changed = ui.add(egui::DragValue::new(&mut uv_scale.x).speed(0.001).prefix("u: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut uv_scale.y).speed(0.001).prefix("v: ")).changed() || changed;
            });

            changed = ui.add(egui::Slider::new(&mut uv_rotation_deg, 0.0..=359.9999).suffix(" °").text("UV Rotation (in deg)")).changed() || changed;

            changed = ui.add(egui::Slider::new(&mut uv_index, 0..=3).text("UV Index")).changed() || changed;
        });

        if changed
        {
            let tex = self.get_texture_by_type_mut(tex_type).unwrap();

            tex.sampler.address_mode_u = address_mode_u;
            tex.sampler.address_mode_v = address_mode_v;
            tex.sampler.address_mode_w = address_mode_w;
            tex.sampler.mag_filter = mag_filter;
            tex.sampler.min_filter = min_filter;
            tex.sampler.mipmap_filter = mipmap_filter;

            tex.transform.offset = uv_offset;
            tex.transform.scale = uv_scale;
            tex.transform.rotation = uv_rotation_deg.to_radians();
            tex.transform.uv_index = uv_index;
        }

    }
}

#[typetag::serde]
impl Component for Material
{
    component_impl_default!();
    component_impl_no_update!();
    component_impl_set_enabled!();
    component_impl_no_cleanup_node!();

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        // textures
        for texture_type in ALL_TEXTURE_TYPES
        {
            if let Some(texture) = self.get_texture_by_type_mut(texture_type)
            {
                if texture.item.is_ref()
                {
                    let texture_found = context.textures.iter().find(|tex| tex.read().unwrap().uuid == texture.item.id().unwrap());
                    if let Some(tex) = texture_found
                    {
                        texture.item = OptionOrId::Some(tex.clone());
                    }
                    else
                    {
                        texture.item = OptionOrId::None;
                        println!("Material: Texture with id {} not found", texture.item.id().unwrap());
                    }
                }
            }
        }
    }

    fn instantiable() -> bool
    {
        false
    }

    fn duplicatable(&self) -> bool
    {
        false
    }

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        None
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        // material settings
        let mut blend_mode;
        let mut alpha;
        let mut alpha_cutoff;

        let mut shininess;
        let mut reflectivity;
        let mut refraction_index;
        let mut normal_map_strength;

        let mut unlit_shading;
        let mut cast_shadow;
        let mut receive_shadow;

        let mut shadow_softness;
        let mut roughness;
        let mut smooth_shading;
        let mut reflection_only;
        let mut backface_culling;

        let mut ambient_color;
        let mut base_color;
        let mut specular_color;
        let mut highlight_color;
        let mut locked_color;

        {
            let data = self.data.get_ref();

            blend_mode = data.blend_mode;
            alpha = data.alpha;
            alpha_cutoff = data.alpha_cutoff.unwrap_or(0.0);

            shininess = data.shininess;
            reflectivity = data.reflectivity;
            refraction_index = data.refraction_index;
            normal_map_strength = data.normal_map_strength;

            unlit_shading = data.unlit_shading;
            cast_shadow = data.cast_shadow;
            receive_shadow = data.receive_shadow;

            shadow_softness = data.shadow_softness;
            roughness = data.roughness;
            smooth_shading = data.smooth_shading;
            reflection_only = data.reflection_only;
            backface_culling = data.backface_culling;

            let r = (data.ambient_color.x * 255.0) as u8;
            let g = (data.ambient_color.y * 255.0) as u8;
            let b = (data.ambient_color.z * 255.0) as u8;
            ambient_color = egui::Color32::from_rgb(r, g, b);

            let r = (data.base_color.x * 255.0) as u8;
            let g = (data.base_color.y * 255.0) as u8;
            let b = (data.base_color.z * 255.0) as u8;
            base_color = egui::Color32::from_rgb(r, g, b);

            let r = (data.specular_color.x * 255.0) as u8;
            let g = (data.specular_color.y * 255.0) as u8;
            let b = (data.specular_color.z * 255.0) as u8;
            specular_color = egui::Color32::from_rgb(r, g, b);

            let r = (data.highlight_color.x * 255.0) as u8;
            let g = (data.highlight_color.y * 255.0) as u8;
            let b = (data.highlight_color.z * 255.0) as u8;
            highlight_color = egui::Color32::from_rgb(r, g, b);

            let r = (data.locked_color.x * 255.0) as u8;
            let g = (data.locked_color.y * 255.0) as u8;
            let b = (data.locked_color.z * 255.0) as u8;
            locked_color = egui::Color32::from_rgb(r, g, b);
        }

        let mut apply_settings = false;

        ui.label(format!("has transparency: {}", self.has_transparency()));

        ui.horizontal(|ui|
        {
            ui.label("Blend Mode:");
            apply_settings = ui.selectable_value(& mut blend_mode, BlendMode::Blend, "Blend").changed() || apply_settings;
            apply_settings = ui.selectable_value(& mut blend_mode, BlendMode::Mask, "Mask").changed() || apply_settings;
            apply_settings = ui.selectable_value(& mut blend_mode, BlendMode::Opaque, "Opaque").changed() || apply_settings;
        });

        apply_settings = ui.add(egui::Slider::new(&mut alpha, 0.0..=1.0).text("alpha")).changed() || apply_settings;
        if blend_mode == BlendMode::Mask
        {
            apply_settings = ui.add(egui::Slider::new(&mut alpha_cutoff, 0.0..=1.0).text("alpha cutoff")).changed() || apply_settings;
        }

        apply_settings = ui.add(egui::Slider::new(&mut shininess, 0.0..=1000.0).text("shininess")).changed() || apply_settings;
        apply_settings = ui.add(egui::Slider::new(&mut reflectivity, 0.0..=1.0).text("reflectivity")).changed() || apply_settings;
        apply_settings = ui.add(egui::Slider::new(&mut refraction_index, 1.0..=5.0).text("refraction index")).changed() || apply_settings;
        apply_settings = ui.add(egui::Slider::new(&mut normal_map_strength, 0.0..=100.0).text("normal map strength").step_by(0.1)).changed() || apply_settings;

        apply_settings = ui.checkbox(&mut unlit_shading, "unlit shading (just base color and base texture)").changed() || apply_settings;
        apply_settings = ui.checkbox(&mut cast_shadow, "cast shadow").changed() || apply_settings;
        apply_settings = ui.checkbox(&mut receive_shadow, "receive shadow").changed() || apply_settings;

        apply_settings = ui.add(egui::Slider::new(&mut shadow_softness, 0.0..=100.0).text("shadow softness")).changed() || apply_settings;
        apply_settings = ui.add(egui::Slider::new(&mut roughness, 0.0..=5.0).text("roughness")).changed() || apply_settings;
        apply_settings = ui.checkbox(&mut smooth_shading, "smooth shading").changed() || apply_settings;
        apply_settings = ui.checkbox(&mut reflection_only, "reflection only").changed() || apply_settings;
        apply_settings = ui.checkbox(&mut backface_culling, "backface cullig").changed() || apply_settings;

        ui.horizontal(|ui|
        {
            ui.label("ambient color:");
            apply_settings = ui.color_edit_button_srgba(&mut ambient_color).changed() || apply_settings;
        });

        ui.horizontal(|ui|
        {
            ui.label("base color:");
            apply_settings = ui.color_edit_button_srgba(&mut base_color).changed() || apply_settings;
        });

        ui.horizontal(|ui|
        {
            ui.label("specular color:");
            apply_settings = ui.color_edit_button_srgba(&mut specular_color).changed() || apply_settings;
        });

        ui.horizontal(|ui|
        {
            ui.label("highlight color:");
            apply_settings = ui.color_edit_button_srgba(&mut highlight_color).changed() || apply_settings;
        });

        ui.horizontal(|ui|
        {
            ui.label("lock color:");
            apply_settings = ui.color_edit_button_srgba(&mut locked_color).changed() || apply_settings;
        });

        if apply_settings
        {
            let data = self.get_data_mut().get_mut();

            data.blend_mode = blend_mode;
            data.alpha = alpha;

            if blend_mode == BlendMode::Mask
            {
                data.alpha_cutoff = Some(alpha_cutoff);
            }
            else
            {
                data.alpha_cutoff = None;
            }

            data.shininess = shininess;
            data.reflectivity = reflectivity;
            data.refraction_index = refraction_index;
            data.normal_map_strength = normal_map_strength;

            data.unlit_shading = unlit_shading;
            data.cast_shadow = cast_shadow;
            data.receive_shadow = receive_shadow;

            data.shadow_softness = shadow_softness;
            data.roughness = roughness;
            data.smooth_shading = smooth_shading;
            data.reflection_only = reflection_only;
            data.backface_culling = backface_culling;

            let r = ((ambient_color.r() as f32) / 255.0).clamp(0.0, 1.0);
            let g = ((ambient_color.g() as f32) / 255.0).clamp(0.0, 1.0);
            let b = ((ambient_color.b() as f32) / 255.0).clamp(0.0, 1.0);
            data.ambient_color = Vector3::<f32>::new(r, g, b);

            let r = ((base_color.r() as f32) / 255.0).clamp(0.0, 1.0);
            let g = ((base_color.g() as f32) / 255.0).clamp(0.0, 1.0);
            let b = ((base_color.b() as f32) / 255.0).clamp(0.0, 1.0);
            data.base_color = Vector3::<f32>::new(r, g, b);

            let r = ((specular_color.r() as f32) / 255.0).clamp(0.0, 1.0);
            let g = ((specular_color.g() as f32) / 255.0).clamp(0.0, 1.0);
            let b = ((specular_color.b() as f32) / 255.0).clamp(0.0, 1.0);
            data.specular_color = Vector3::<f32>::new(r, g, b);

            let r = ((highlight_color.r() as f32) / 255.0).clamp(0.0, 1.0);
            let g = ((highlight_color.g() as f32) / 255.0).clamp(0.0, 1.0);
            let b = ((highlight_color.b() as f32) / 255.0).clamp(0.0, 1.0);
            data.highlight_color = Vector3::<f32>::new(r, g, b);
        }
    }
}
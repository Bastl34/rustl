use std::sync::{RwLock, Arc};

use nalgebra::{Vector3, Vector4};

use crate::{state::scene::texture::{TextureItem, Texture}, helper};

#[derive(Clone, Copy)]
pub enum TextureType
{
    Base,
    AmbientEmissive,
    Specular,
    Normal,
    Alpha,
    Roughness,
    AmbientOcclusion,
    Reflectivity,
}

#[derive(Debug)]
pub struct Material
{
    pub id: u32,
    pub name: String,

    pub ambient_color: Vector3<f32>,
    pub base_color: Vector3<f32>,
    pub specular_color: Vector3<f32>,

    pub texture_ambient: Option<TextureItem>,
    pub texture_base: Option<TextureItem>,
    pub texture_specular: Option<TextureItem>,
    pub texture_normal: Option<TextureItem>,
    pub texture_alpha: Option<TextureItem>,
    pub texture_roughness: Option<TextureItem>,
    pub texture_ambient_occlusion: Option<TextureItem>,
    pub texture_reflectivity: Option<TextureItem>,

    pub alpha: f32,
    pub shininess: f32,
    pub reflectivity: f32,
    pub refraction_index: f32,

    pub normal_map_strength: f32,

    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub shadow_softness: f32,

    pub monte_carlo: bool,

    pub roughness: f32, //degree in rad (max PI/2)

    pub smooth_shading: bool,

    pub reflection_only: bool,
    pub backface_cullig: bool
}

impl Material
{
    pub fn new(id: u32, name: &str) -> Material
    {
        Material
        {
            id: id,
            name: name.to_string(),

            ambient_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            base_color: Vector3::<f32>::new(1.0, 1.0, 1.0),
            specular_color: Vector3::<f32>::new(0.8, 0.8, 0.8),

            texture_ambient: None,
            texture_base: None,
            texture_specular: None,
            texture_normal: None,
            texture_alpha: None,
            texture_roughness: None,
            texture_ambient_occlusion: None,
            texture_reflectivity: None,

            alpha: 1.0,
            shininess: 150.0,
            reflectivity: 0.0,
            refraction_index: 1.0,

            normal_map_strength: 1.0,

            cast_shadow: true,
            receive_shadow: true,
            shadow_softness: 0.01,

            roughness: 0.0,

            monte_carlo: true,

            smooth_shading: true,

            reflection_only: false,
            backface_cullig: true,
        }
    }

    pub fn apply_diff_without_textures(&mut self, new_mat: &Material)
    {
        let default_material = Material::new(0, "");

        // ********** colors **********

        // ambient
        if
            !helper::math::approx_equal(default_material.ambient_color.x, new_mat.ambient_color.x)
            ||
            !helper::math::approx_equal(default_material.ambient_color.y, new_mat.ambient_color.y)
            ||
            !helper::math::approx_equal(default_material.ambient_color.z, new_mat.ambient_color.z)
        {
            self.ambient_color = new_mat.ambient_color;
        }

        // base
        if
            !helper::math::approx_equal(default_material.base_color.x, new_mat.base_color.x)
            ||
            !helper::math::approx_equal(default_material.base_color.y, new_mat.base_color.y)
            ||
            !helper::math::approx_equal(default_material.base_color.z, new_mat.base_color.z)
        {
            self.base_color = new_mat.base_color;
        }

        // specular
        if
            !helper::math::approx_equal(default_material.specular_color.x, new_mat.specular_color.x)
            ||
            !helper::math::approx_equal(default_material.specular_color.y, new_mat.specular_color.y)
            ||
            !helper::math::approx_equal(default_material.specular_color.z, new_mat.specular_color.z)
        {
            self.specular_color = new_mat.specular_color;
        }

        // ********** other attributes **********
        if !helper::math::approx_equal(default_material.alpha, new_mat.alpha) { self.alpha = new_mat.alpha; }
        if !helper::math::approx_equal(default_material.shininess, new_mat.shininess) { self.shininess = new_mat.shininess; }
        if !helper::math::approx_equal(default_material.reflectivity, new_mat.reflectivity) { self.reflectivity = new_mat.reflectivity; }
        if !helper::math::approx_equal(default_material.refraction_index, new_mat.refraction_index) { self.refraction_index = new_mat.refraction_index; }

        if !helper::math::approx_equal(default_material.normal_map_strength, new_mat.normal_map_strength) { self.normal_map_strength = new_mat.normal_map_strength; }

        if default_material.cast_shadow != new_mat.cast_shadow { self.cast_shadow = new_mat.cast_shadow; }
        if default_material.receive_shadow != new_mat.receive_shadow { self.receive_shadow = new_mat.receive_shadow; }
        if !helper::math::approx_equal(default_material.shadow_softness, new_mat.shadow_softness) { self.shadow_softness = new_mat.shadow_softness; }

        if !helper::math::approx_equal(default_material.roughness, new_mat.roughness) { self.roughness = new_mat.roughness; }

        if default_material.monte_carlo != new_mat.monte_carlo { self.monte_carlo = new_mat.monte_carlo; }

        if default_material.smooth_shading != new_mat.smooth_shading { self.smooth_shading = new_mat.smooth_shading; }

        if default_material.reflection_only != new_mat.reflection_only { self.reflection_only = new_mat.reflection_only; }
        if default_material.backface_cullig != new_mat.backface_cullig { self.backface_cullig = new_mat.backface_cullig; }
    }

    pub fn apply_diff(&mut self, new_mat: &Material)
    {
        // ********** default settings **********
        self.apply_diff_without_textures(new_mat);

        // ********** textures **********
        let default_material = Material::new(0, "");

        macro_rules! compare_and_apply_texture_diff
        {
            ($self_tex:expr, $default_material_tex:expr, $new_mat_tex:expr) =>
            {
                if $default_material_tex.is_some() != $new_mat_tex.is_some()
                    ||
                    (
                        $default_material_tex.is_some() && $new_mat_tex.is_some()
                        &&
                        $default_material_tex.unwrap().read().unwrap().hash != $new_mat_tex.unwrap().read().unwrap().hash
                    )
                {
                    $self_tex = $new_mat_tex.clone();
                }
            };
        }

        compare_and_apply_texture_diff!(self.texture_ambient, default_material.texture_ambient, new_mat.texture_ambient.clone());
        compare_and_apply_texture_diff!(self.texture_base, default_material.texture_base, new_mat.texture_base.clone());
        compare_and_apply_texture_diff!(self.texture_specular, default_material.texture_specular, new_mat.texture_specular.clone());
        compare_and_apply_texture_diff!(self.texture_normal, default_material.texture_normal, new_mat.texture_normal.clone());
        compare_and_apply_texture_diff!(self.texture_alpha, default_material.texture_alpha, new_mat.texture_alpha.clone());
        compare_and_apply_texture_diff!(self.texture_roughness, default_material.texture_roughness, new_mat.texture_roughness.clone());
        compare_and_apply_texture_diff!(self.texture_ambient_occlusion, default_material.texture_ambient_occlusion, new_mat.texture_ambient_occlusion.clone());
        compare_and_apply_texture_diff!(self.texture_reflectivity, default_material.texture_reflectivity, new_mat.texture_reflectivity.clone());

    }

    pub fn print(&self)
    {
        println!("ambient_color: {:?}", self.ambient_color);
        println!("base_color: {:?}", self.base_color);
        println!("specular_color: {:?}", self.specular_color);

        println!("texture_base: {:?}", self.texture_base.is_some());
        println!("texture_specular: {:?}", self.texture_specular.is_some());
        println!("texture_normal: {:?}", self.texture_normal.is_some());
        println!("texture_alpha: {:?}", self.texture_alpha.is_some());
        println!("texture_roughness: {:?}", self.texture_roughness.is_some());
        println!("texture_ambient_occlusion: {:?}", self.texture_ambient_occlusion.is_some());
        println!("texture_reflectivity: {:?}", self.texture_reflectivity.is_some());

        println!("alpha: {:?}", self.alpha);
        println!("shininess: {:?}", self.shininess);
        println!("reflectivity: {:?}", self.reflectivity);
        println!("refraction_index: {:?}", self.refraction_index);

        println!("normal_map_strength: {:?}", self.normal_map_strength);

        println!("cast_shadow: {:?}", self.cast_shadow);
        println!("receive_shadow: {:?}", self.receive_shadow);
        println!("shadow_softness: {:?}", self.shadow_softness);

        println!("roughness: {:?}", self.roughness);

        println!("monte_carlo: {:?}", self.monte_carlo);

        println!("smooth_shading: {:?}", self.smooth_shading);

        println!("reflection_only: {:?}", self.reflection_only);
        println!("backface_cullig: {:?}", self.backface_cullig);
    }

    pub fn remove_texture(&mut self, tex_type: TextureType)
    {
        match tex_type
        {
            TextureType::Base =>
            {
                self.texture_base = None;
            },
            TextureType::AmbientEmissive =>
            {
                self.texture_ambient = None;
            },
            TextureType::Specular =>
            {
                self.texture_specular = None;
            },
            TextureType::Normal =>
            {
                self.texture_normal = None;
            },
            TextureType::Alpha =>
            {
                self.texture_alpha = None;
            },
            TextureType::Roughness =>
            {
                self.texture_roughness = None;
            },
            TextureType::AmbientOcclusion =>
            {
                self.texture_ambient_occlusion = None;
            },
            TextureType::Reflectivity =>
            {
                self.texture_reflectivity = None;
            },
        }
    }

    pub fn set_texture(&mut self, tex: TextureItem, tex_type: TextureType)
    {
        match tex_type
        {
            TextureType::Base =>
            {
                self.texture_base = Some(tex.clone());
            },
            TextureType::AmbientEmissive =>
            {
                self.texture_ambient = Some(tex.clone());
            },
            TextureType::Specular =>
            {
                self.texture_specular = Some(tex.clone());
            },
            TextureType::Normal =>
            {
                self.texture_normal = Some(tex.clone());
            },
            TextureType::Alpha =>
            {
                self.texture_alpha = Some(tex.clone());
            },
            TextureType::Roughness =>
            {
                self.texture_roughness = Some(tex.clone());
            },
            TextureType::AmbientOcclusion =>
            {
                self.texture_ambient_occlusion = Some(tex.clone());
            },
            TextureType::Reflectivity =>
            {
                self.texture_reflectivity = Some(tex.clone());
            },
        }
    }

    pub fn has_any_texture(&self) -> bool
    {
        self.texture_base.is_some()
        ||
        self.texture_ambient.is_some()
        ||
        self.texture_specular.is_some()
        ||
        self.texture_normal.is_some()
        ||
        self.texture_alpha.is_some()
        ||
        self.texture_roughness.is_some()
        ||
        self.texture_ambient_occlusion.is_some()
        ||
        self.texture_reflectivity.is_some()
    }

    pub fn has_texture(&self, tex_type: TextureType) -> bool
    {
        match tex_type
        {
            TextureType::Base => self.texture_base.is_some(),
            TextureType::AmbientEmissive => self.texture_ambient.is_some(),
            TextureType::Specular => self.texture_specular.is_some(),
            TextureType::Normal => self.texture_normal.is_some(),
            TextureType::Alpha => self.texture_alpha.is_some(),
            TextureType::Roughness => self.texture_roughness.is_some(),
            TextureType::AmbientOcclusion => self.texture_ambient_occlusion.is_some(),
            TextureType::Reflectivity => self.texture_reflectivity.is_some()
        }
    }

    pub fn texture_dimension(&self, tex_type: TextureType) -> (u32, u32)
    {
        let tex;

        match tex_type
        {
            TextureType::Base => { tex = self.texture_base.clone() },
            TextureType::AmbientEmissive => { tex = self.texture_ambient.clone() },
            TextureType::Specular => { tex = self.texture_specular.clone() },
            TextureType::Normal => { tex = self.texture_normal.clone() },
            TextureType::Alpha => { tex = self.texture_alpha.clone() },
            TextureType::Roughness => { tex = self.texture_roughness.clone() },
            TextureType::AmbientOcclusion => { tex = self.texture_ambient_occlusion.clone() },
            TextureType::Reflectivity => { tex = self.texture_reflectivity.clone() },
        }

        if tex.is_some()
        {
            return tex.unwrap().read().unwrap().dimensions()
        }

        (0,0)
    }

    pub fn get_texture_pixel(&self, x: u32, y: u32, tex_type: TextureType) -> Vector4<f32>
    {
        if !self.has_texture(tex_type)
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        let tex;

        match tex_type
        {
            TextureType::Base => { tex = self.texture_base.clone() },
            TextureType::AmbientEmissive => { tex = self.texture_ambient.clone() },
            TextureType::Specular => { tex = self.texture_specular.clone() },
            TextureType::Normal => { tex = self.texture_normal.clone() },
            TextureType::Alpha => { tex = self.texture_alpha.clone() },
            TextureType::Roughness => { tex = self.texture_roughness.clone() },
            TextureType::AmbientOcclusion => { tex = self.texture_ambient_occlusion.clone() },
            TextureType::Reflectivity => { tex = self.texture_reflectivity.clone() },
        }

        if tex.is_some()
        {
            return tex.unwrap().read().unwrap().get_pixel_as_float_vec(x, y);
        }

        Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0)
    }
}
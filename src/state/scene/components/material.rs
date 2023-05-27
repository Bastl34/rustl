use std::sync::{RwLock, Arc};
use std::any::Any;

use nalgebra::{Vector3, Vector4};

use crate::{state::scene::texture::{TextureItem, Texture}, helper};

use super::component::{Component, SharedComponentItem};

//pub type MaterialItem = Arc<RwLock<Box<Material>>>;
//pub type MaterialItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;
pub type MaterialItem = SharedComponentItem;

//pub type MaterialBoxItem = Box<dyn Any + Send + Sync>;
//pub type MaterialItem = Arc<RwLock<MaterialBoxItem>>;

#[derive(Clone, Copy, PartialEq, Debug)]
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
    Shininess
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextureFiltering
{
    Nearest,
    Linear
}

#[derive(Debug)]
pub struct MaterialData
{
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
    pub texture_shininess: Option<TextureItem>,

    pub filtering_mode: TextureFiltering,

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

pub struct Material
{
    id: u32,
    name: String,

    data: MaterialData,
}

impl Material
{
    pub fn new(id: u32, name: &str) -> Material
    {
        let material_data = MaterialData
        {
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
            texture_shininess: None,

            filtering_mode: TextureFiltering::Linear,

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
        };

        Material
        {
            id: id,
            name: name.to_string(),

            data: material_data
        }
    }

    pub fn get_name(&self) -> &String
    {
        &self.name
    }

    pub fn set_name(&mut self, name: String)
    {
        self.name = name;
    }

    pub fn get_data(&self) -> &MaterialData
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut MaterialData
    {
        &mut self.data
    }

    pub fn apply_diff_without_textures(&mut self, new_mat: &Material)
    {
        let default_material = Material::new(0, "");
        let default_material_data = new_mat.get_data();

        let new_mat_data = new_mat.get_data();

        // ********** colors **********

        // ambient
        if
            !helper::math::approx_equal(default_material_data.ambient_color.x, new_mat_data.ambient_color.x)
            ||
            !helper::math::approx_equal(default_material_data.ambient_color.y, new_mat_data.ambient_color.y)
            ||
            !helper::math::approx_equal(default_material_data.ambient_color.z, new_mat_data.ambient_color.z)
        {
            self.data.ambient_color = new_mat_data.ambient_color;
        }

        // base
        if
            !helper::math::approx_equal(default_material_data.base_color.x, new_mat_data.base_color.x)
            ||
            !helper::math::approx_equal(default_material_data.base_color.y, new_mat_data.base_color.y)
            ||
            !helper::math::approx_equal(default_material_data.base_color.z, new_mat_data.base_color.z)
        {
            self.data.base_color = new_mat_data.base_color;
        }

        // specular
        if
            !helper::math::approx_equal(default_material_data.specular_color.x, new_mat_data.specular_color.x)
            ||
            !helper::math::approx_equal(default_material_data.specular_color.y, new_mat_data.specular_color.y)
            ||
            !helper::math::approx_equal(default_material_data.specular_color.z, new_mat_data.specular_color.z)
        {
            self.data.specular_color = new_mat_data.specular_color;
        }

        // ********** other attributes **********
        if default_material_data.filtering_mode != new_mat_data.filtering_mode { self.data.filtering_mode = new_mat_data.filtering_mode; }

        if !helper::math::approx_equal(default_material_data.alpha, new_mat_data.alpha) { self.data.alpha = new_mat_data.alpha; }
        if !helper::math::approx_equal(default_material_data.shininess, new_mat_data.shininess) { self.data.shininess = new_mat_data.shininess; }
        if !helper::math::approx_equal(default_material_data.reflectivity, new_mat_data.reflectivity) { self.data.reflectivity = new_mat_data.reflectivity; }
        if !helper::math::approx_equal(default_material_data.refraction_index, new_mat_data.refraction_index) { self.data.refraction_index = new_mat_data.refraction_index; }

        if !helper::math::approx_equal(default_material_data.normal_map_strength, new_mat_data.normal_map_strength) { self.data.normal_map_strength = new_mat_data.normal_map_strength; }

        if default_material_data.cast_shadow != new_mat_data.cast_shadow { self.data.cast_shadow = new_mat_data.cast_shadow; }
        if default_material_data.receive_shadow != new_mat_data.receive_shadow { self.data.receive_shadow = new_mat_data.receive_shadow; }
        if !helper::math::approx_equal(default_material_data.shadow_softness, new_mat_data.shadow_softness) { self.data.shadow_softness = new_mat_data.shadow_softness; }

        if !helper::math::approx_equal(default_material_data.roughness, new_mat_data.roughness) { self.data.roughness = new_mat_data.roughness; }

        if default_material_data.monte_carlo != new_mat_data.monte_carlo { self.data.monte_carlo = new_mat_data.monte_carlo; }

        if default_material_data.smooth_shading != new_mat_data.smooth_shading { self.data.smooth_shading = new_mat_data.smooth_shading; }

        if default_material_data.reflection_only != new_mat_data.reflection_only { self.data.reflection_only = new_mat_data.reflection_only; }
        if default_material_data.backface_cullig != new_mat_data.backface_cullig { self.data.backface_cullig = new_mat_data.backface_cullig; }
    }

    pub fn apply_diff(&mut self, new_mat: &Material)
    {
        // ********** default settings **********
        self.apply_diff_without_textures(new_mat);

        // ********** textures **********
        let default_material = Material::new(0, "");
        let default_material_data = default_material.data;

        let new_mat_data = new_mat.get_data();

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

        compare_and_apply_texture_diff!(self.data.texture_ambient, default_material_data.texture_ambient, new_mat_data.texture_ambient.clone());
        compare_and_apply_texture_diff!(self.data.texture_base, default_material_data.texture_base, new_mat_data.texture_base.clone());
        compare_and_apply_texture_diff!(self.data.texture_specular, default_material_data.texture_specular, new_mat_data.texture_specular.clone());
        compare_and_apply_texture_diff!(self.data.texture_normal, default_material_data.texture_normal, new_mat_data.texture_normal.clone());
        compare_and_apply_texture_diff!(self.data.texture_alpha, default_material_data.texture_alpha, new_mat_data.texture_alpha.clone());
        compare_and_apply_texture_diff!(self.data.texture_roughness, default_material_data.texture_roughness, new_mat_data.texture_roughness.clone());
        compare_and_apply_texture_diff!(self.data.texture_ambient_occlusion, default_material_data.texture_ambient_occlusion, new_mat_data.texture_ambient_occlusion.clone());
        compare_and_apply_texture_diff!(self.data.texture_reflectivity, default_material_data.texture_reflectivity, new_mat_data.texture_reflectivity.clone());
        compare_and_apply_texture_diff!(self.data.texture_shininess, default_material_data.texture_shininess, new_mat_data.texture_shininess.clone());

    }

    pub fn print(&self)
    {
        println!("ambient_color: {:?}", self.data.ambient_color);
        println!("base_color: {:?}", self.data.base_color);
        println!("specular_color: {:?}", self.data.specular_color);

        println!("texture_base: {:?}", self.data.texture_base.is_some());
        println!("texture_specular: {:?}", self.data.texture_specular.is_some());
        println!("texture_normal: {:?}", self.data.texture_normal.is_some());
        println!("texture_alpha: {:?}", self.data.texture_alpha.is_some());
        println!("texture_roughness: {:?}", self.data.texture_roughness.is_some());
        println!("texture_ambient_occlusion: {:?}", self.data.texture_ambient_occlusion.is_some());
        println!("texture_reflectivity: {:?}", self.data.texture_reflectivity.is_some());
        println!("texture_shininess: {:?}", self.data.texture_shininess.is_some());

        println!("filtering_mode: {:?}", self.data.filtering_mode);

        println!("alpha: {:?}", self.data.alpha);
        println!("shininess: {:?}", self.data.shininess);
        println!("reflectivity: {:?}", self.data.reflectivity);
        println!("refraction_index: {:?}", self.data.refraction_index);

        println!("normal_map_strength: {:?}", self.data.normal_map_strength);

        println!("cast_shadow: {:?}", self.data.cast_shadow);
        println!("receive_shadow: {:?}", self.data.receive_shadow);
        println!("shadow_softness: {:?}", self.data.shadow_softness);

        println!("roughness: {:?}", self.data.roughness);

        println!("monte_carlo: {:?}", self.data.monte_carlo);

        println!("smooth_shading: {:?}", self.data.smooth_shading);

        println!("reflection_only: {:?}", self.data.reflection_only);
        println!("backface_cullig: {:?}", self.data.backface_cullig);
    }

    pub fn remove_texture(&mut self, tex_type: TextureType)
    {
        match tex_type
        {
            TextureType::Base =>
            {
                self.data.texture_base = None;
            },
            TextureType::AmbientEmissive =>
            {
                self.data.texture_ambient = None;
            },
            TextureType::Specular =>
            {
                self.data.texture_specular = None;
            },
            TextureType::Normal =>
            {
                self.data.texture_normal = None;
            },
            TextureType::Alpha =>
            {
                self.data.texture_alpha = None;
            },
            TextureType::Roughness =>
            {
                self.data.texture_roughness = None;
            },
            TextureType::AmbientOcclusion =>
            {
                self.data.texture_ambient_occlusion = None;
            },
            TextureType::Reflectivity =>
            {
                self.data.texture_reflectivity = None;
            },
            TextureType::Shininess =>
            {
                self.data.texture_shininess = None;
            },
        }
    }

    pub fn set_texture(&mut self, tex: TextureItem, tex_type: TextureType)
    {
        match tex_type
        {
            TextureType::Base =>
            {
                self.data.texture_base = Some(tex.clone());
            },
            TextureType::AmbientEmissive =>
            {
                self.data.texture_ambient = Some(tex.clone());
            },
            TextureType::Specular =>
            {
                self.data.texture_specular = Some(tex.clone());
            },
            TextureType::Normal =>
            {
                self.data.texture_normal = Some(tex.clone());
            },
            TextureType::Alpha =>
            {
                self.data.texture_alpha = Some(tex.clone());
            },
            TextureType::Roughness =>
            {
                self.data.texture_roughness = Some(tex.clone());
            },
            TextureType::AmbientOcclusion =>
            {
                self.data.texture_ambient_occlusion = Some(tex.clone());
            },
            TextureType::Reflectivity =>
            {
                self.data.texture_reflectivity = Some(tex.clone());
            },
            TextureType::Shininess =>
            {
                self.data.texture_shininess = Some(tex.clone());
            },
        }
    }

    pub fn has_any_texture(&self) -> bool
    {
        self.data.texture_base.is_some()
        ||
        self.data.texture_ambient.is_some()
        ||
        self.data.texture_specular.is_some()
        ||
        self.data.texture_normal.is_some()
        ||
        self.data.texture_alpha.is_some()
        ||
        self.data.texture_roughness.is_some()
        ||
        self.data.texture_ambient_occlusion.is_some()
        ||
        self.data.texture_reflectivity.is_some()
        ||
        self.data.texture_shininess.is_some()
    }

    fn get_texture_by_type(&self, tex_type: TextureType) -> Option<Arc<RwLock<Box<Texture>>>>
    {
        let tex;

        match tex_type
        {
            TextureType::Base => { tex = self.data.texture_base.clone() },
            TextureType::AmbientEmissive => { tex = self.data.texture_ambient.clone() },
            TextureType::Specular => { tex = self.data.texture_specular.clone() },
            TextureType::Normal => { tex = self.data.texture_normal.clone() },
            TextureType::Alpha => { tex = self.data.texture_alpha.clone() },
            TextureType::Roughness => { tex = self.data.texture_roughness.clone() },
            TextureType::AmbientOcclusion => { tex = self.data.texture_ambient_occlusion.clone() },
            TextureType::Reflectivity => { tex = self.data.texture_reflectivity.clone() },
            TextureType::Shininess => { tex = self.data.texture_shininess.clone() },
        }

        tex
    }

    pub fn has_texture(&self, tex_type: TextureType) -> bool
    {
        let tex = self.get_texture_by_type(tex_type);

        tex.is_some()
    }

    pub fn texture_dimension(&self, tex_type: TextureType) -> (u32, u32)
    {
        let tex = self.get_texture_by_type(tex_type);

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

        let tex = self.get_texture_by_type(tex_type);

        if tex.is_some()
        {
            return tex.unwrap().read().unwrap().get_pixel_as_float_vec(x, y);
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

        let tex_arc = tex.unwrap();
        let tex = tex_arc.read().unwrap();

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

        let p_res_1 = helper::math::interpolate_vec4(p0, p1, x_f);
        let p_res_2 = helper::math::interpolate_vec4(p2, p3, x_f);

        let res = helper::math::interpolate_vec4(p_res_1, p_res_2, y_f);

        res
    }
}

impl Component for Material
{
    fn is_enabled(&self) -> bool
    {
        true
    }

    fn component_name(&self) -> &'static str
    {
        "Material"
    }

    fn update(&mut self, time_delta: f32)
    {
        // TODO
    }

    fn as_any(&self) -> &dyn Any
    {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any
    {
        self
    }
}
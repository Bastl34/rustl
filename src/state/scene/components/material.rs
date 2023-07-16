use std::sync::{RwLock, Arc};
use std::any::Any;

use nalgebra::{Vector3, Vector4};

use crate::helper::change_tracker::ChangeTracker;
use crate::{component_impl_default};
use crate::{state::scene::texture::{TextureItem, Texture}, helper};

use super::component::{Component, SharedComponentItem, ComponentBase};

//pub type MaterialItem = Arc<RwLock<Box<Material>>>;
//pub type MaterialItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;
pub type MaterialItem = SharedComponentItem;

//pub type MaterialBoxItem = Box<dyn Any + Send + Sync>;
//pub type MaterialItem = Arc<RwLock<MaterialBoxItem>>;

#[derive(Clone, Copy, PartialEq, Debug)]
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
    Shininess
}

pub const ALL_TEXTURE_TYPES: [TextureType; 9] =
[
    TextureType::AmbientEmissive,
    TextureType::Base,
    TextureType::Specular,
    TextureType::Normal,
    TextureType::Alpha,
    TextureType::Roughness,
    TextureType::AmbientOcclusion,
    TextureType::Reflectivity,
    TextureType::Shininess
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextureFiltering
{
    Nearest,
    Linear
}

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

    pub alpha_inheritance: bool,
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
    base: ComponentBase,
    data: ChangeTracker<MaterialData>,
}

impl Material
{
    pub fn new(id: u64, name: &str) -> Material
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

            alpha_inheritance: true,
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
            base: ComponentBase::new(id, name.to_string(), "Transformation".to_string()),
            data: ChangeTracker::new(material_data)
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
        let default_material = Material::new(0, "");
        let default_material_data = new_mat.get_data();

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
        if default_material_data.filtering_mode != new_mat_data.filtering_mode { data.filtering_mode = new_mat_data.filtering_mode; }

        if !helper::math::approx_equal(default_material_data.alpha, new_mat_data.alpha) { data.alpha = new_mat_data.alpha; }
        if !helper::math::approx_equal(default_material_data.shininess, new_mat_data.shininess) { data.shininess = new_mat_data.shininess; }
        if !helper::math::approx_equal(default_material_data.reflectivity, new_mat_data.reflectivity) { data.reflectivity = new_mat_data.reflectivity; }
        if !helper::math::approx_equal(default_material_data.refraction_index, new_mat_data.refraction_index) { data.refraction_index = new_mat_data.refraction_index; }

        if !helper::math::approx_equal(default_material_data.normal_map_strength, new_mat_data.normal_map_strength) { data.normal_map_strength = new_mat_data.normal_map_strength; }

        if default_material_data.alpha_inheritance != new_mat_data.alpha_inheritance { data.alpha_inheritance = new_mat_data.alpha_inheritance; }
        if default_material_data.cast_shadow != new_mat_data.cast_shadow { data.cast_shadow = new_mat_data.cast_shadow; }
        if default_material_data.receive_shadow != new_mat_data.receive_shadow { data.receive_shadow = new_mat_data.receive_shadow; }
        if !helper::math::approx_equal(default_material_data.shadow_softness, new_mat_data.shadow_softness) { data.shadow_softness = new_mat_data.shadow_softness; }

        if !helper::math::approx_equal(default_material_data.roughness, new_mat_data.roughness) { data.roughness = new_mat_data.roughness; }

        if default_material_data.monte_carlo != new_mat_data.monte_carlo { data.monte_carlo = new_mat_data.monte_carlo; }

        if default_material_data.smooth_shading != new_mat_data.smooth_shading { data.smooth_shading = new_mat_data.smooth_shading; }

        if default_material_data.reflection_only != new_mat_data.reflection_only { data.reflection_only = new_mat_data.reflection_only; }
        if default_material_data.backface_cullig != new_mat_data.backface_cullig { data.backface_cullig = new_mat_data.backface_cullig; }
    }

    pub fn apply_diff(&mut self, new_mat: &Material)
    {
        // ********** default settings **********
        self.apply_diff_without_textures(new_mat);

        // ********** textures **********
        let default_material = Material::new(0, "");
        let default_material_data = default_material.data.get_ref();

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

        println!("filtering_mode: {:?}", data.filtering_mode);

        println!("alpha: {:?}", data.alpha);
        println!("shininess: {:?}", data.shininess);
        println!("reflectivity: {:?}", data.reflectivity);
        println!("refraction_index: {:?}", data.refraction_index);

        println!("normal_map_strength: {:?}", data.normal_map_strength);

        println!("alpha_inheritance: {:?}", data.alpha_inheritance);
        println!("cast_shadow: {:?}", data.cast_shadow);
        println!("receive_shadow: {:?}", data.receive_shadow);
        println!("shadow_softness: {:?}", data.shadow_softness);

        println!("roughness: {:?}", data.roughness);

        println!("monte_carlo: {:?}", data.monte_carlo);

        println!("smooth_shading: {:?}", data.smooth_shading);

        println!("reflection_only: {:?}", data.reflection_only);
        println!("backface_cullig: {:?}", data.backface_cullig);
    }

    pub fn remove_texture(&mut self, tex_type: TextureType)
    {
        let data = self.data.get_mut();

        match tex_type
        {
            TextureType::Base =>
            {
                data.texture_base = None;
            },
            TextureType::AmbientEmissive =>
            {
                data.texture_ambient = None;
            },
            TextureType::Specular =>
            {
                data.texture_specular = None;
            },
            TextureType::Normal =>
            {
                data.texture_normal = None;
            },
            TextureType::Alpha =>
            {
                data.texture_alpha = None;
            },
            TextureType::Roughness =>
            {
                data.texture_roughness = None;
            },
            TextureType::AmbientOcclusion =>
            {
                data.texture_ambient_occlusion = None;
            },
            TextureType::Reflectivity =>
            {
                data.texture_reflectivity = None;
            },
            TextureType::Shininess =>
            {
                data.texture_shininess = None;
            },
        }
    }

    pub fn set_texture(&mut self, tex: TextureItem, tex_type: TextureType)
    {
        let data = self.data.get_mut();

        match tex_type
        {
            TextureType::Base =>
            {
                data.texture_base = Some(tex.clone());
            },
            TextureType::AmbientEmissive =>
            {
                data.texture_ambient = Some(tex.clone());
            },
            TextureType::Specular =>
            {
                data.texture_specular = Some(tex.clone());
            },
            TextureType::Normal =>
            {
                data.texture_normal = Some(tex.clone());
            },
            TextureType::Alpha =>
            {
                data.texture_alpha = Some(tex.clone());
            },
            TextureType::Roughness =>
            {
                data.texture_roughness = Some(tex.clone());
            },
            TextureType::AmbientOcclusion =>
            {
                data.texture_ambient_occlusion = Some(tex.clone());
            },
            TextureType::Reflectivity =>
            {
                data.texture_reflectivity = Some(tex.clone());
            },
            TextureType::Shininess =>
            {
                data.texture_shininess = Some(tex.clone());
            },
        }
    }

    pub fn has_any_texture(&self) -> bool
    {
        let data = self.data.get_ref();

        data.texture_base.is_some()
        ||
        data.texture_ambient.is_some()
        ||
        data.texture_specular.is_some()
        ||
        data.texture_normal.is_some()
        ||
        data.texture_alpha.is_some()
        ||
        data.texture_roughness.is_some()
        ||
        data.texture_ambient_occlusion.is_some()
        ||
        data.texture_reflectivity.is_some()
        ||
        data.texture_shininess.is_some()
    }

    pub fn get_texture_by_type(&self, tex_type: TextureType) -> Option<Arc<RwLock<Box<Texture>>>>
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
    component_impl_default!();

    fn update(&mut self, _frame_scale: f32)
    {
    }
}
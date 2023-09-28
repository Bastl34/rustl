use std::{f32::consts::PI, cell::RefCell};

use nalgebra::{Point3, Vector3};

use crate::{state::helper::render_item::{RenderItemOption}, helper::change_tracker::ChangeTracker};

pub type LightItem = Box<Light>;

// ******************** LightType ********************

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum LightType
{
    Directional,
    Point,
    Spot
}

// ******************** Light ********************

pub struct Light
{
    pub enabled: bool,
    pub name: String,
    pub id: u64,
    pub pos: Point3<f32>,
    pub dir: Vector3<f32>,
    pub color: Vector3<f32>,
    pub intensity: f32,
    pub distance_based_intensity: bool,
    pub max_angle: f32, //in rad
    pub light_type: LightType,
}

impl Light
{
    pub fn new_point(id: u64, name: String, pos: Point3<f32>, color: Vector3<f32>, intensity: f32) -> Light
    {
        Self
        {
            enabled: true,
            id: id,
            name: name,
            pos: pos,
            dir: Vector3::<f32>::new(0.0, -1.0, 0.0),
            color: color,
            intensity: intensity,
            distance_based_intensity: false,
            max_angle: 0.0,
            light_type: LightType::Point,
        }
    }

    pub fn new_directional(id: u64, name: String, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32) -> Light
    {
        Self
        {
            enabled: true,
            id: id,
            name: name,
            pos: pos,
            dir: dir,
            color: color,
            intensity: intensity,
            distance_based_intensity: false,
            max_angle: 0.0,
            light_type: LightType::Directional,
        }
    }

    pub fn new_spot(id: u64, name: String, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, max_angle: f32, intensity: f32) -> Light
    {
        Self
        {
            enabled: true,
            id: id,
            name: name,
            pos: pos,
            dir: dir,
            color: color,
            intensity: intensity,
            distance_based_intensity: false,
            max_angle: max_angle,
            light_type: LightType::Spot,
        }
    }

    pub fn ui(light: &RefCell<ChangeTracker<Box<Light>>>, ui: &mut egui::Ui)
    {
        let mut enabled;

        let mut pos;
        let mut dir;
        let mut color;
        let mut intensity;
        let mut max_angle;
        let mut light_type;
        let mut distance_based_intensity;

        {
            let light = light.borrow();
            let light = light.get_ref();

            enabled = light.enabled;

            pos = light.pos;
            dir = light.dir;

            let r = (light.color.x * 255.0) as u8;
            let g = (light.color.y * 255.0) as u8;
            let b = (light.color.z * 255.0) as u8;
            color = egui::Color32::from_rgb(r, g, b);

            intensity = light.intensity;
            max_angle = light.max_angle.to_degrees();
            light_type = light.light_type;
            distance_based_intensity = light.distance_based_intensity;
        }

        let mut apply_settings = false;

        ui.vertical(|ui|
        {
            apply_settings = ui.checkbox(&mut enabled, "Enabled").changed() || apply_settings;

            ui.horizontal(|ui|
            {
                ui.label("pos:");
                apply_settings = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || apply_settings;
                apply_settings = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || apply_settings;
                apply_settings = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || apply_settings;
            });

            ui.horizontal(|ui|
            {
                ui.label("dir:");
                apply_settings = ui.add(egui::DragValue::new(&mut dir.x).speed(0.1).prefix("x: ")).changed() || apply_settings;
                apply_settings = ui.add(egui::DragValue::new(&mut dir.y).speed(0.1).prefix("y: ")).changed() || apply_settings;
                apply_settings = ui.add(egui::DragValue::new(&mut dir.z).speed(0.1).prefix("z: ")).changed() || apply_settings;
            });

            ui.horizontal(|ui|
            {
                ui.label("color:");
                apply_settings = ui.color_edit_button_srgba(&mut color).changed() || apply_settings;
            });

            if light_type == LightType::Directional
            {
                apply_settings = ui.add(egui::Slider::new(&mut intensity, 0.0..=1.0).text("intensity")).changed() || apply_settings;
            }
            else
            {
                apply_settings = ui.add(egui::Slider::new(&mut intensity, 0.0..=10000.0).text("intensity")).changed() || apply_settings;
            }
            apply_settings = ui.add(egui::Slider::new(&mut max_angle, 0.0..=180.0).text("max_angle").suffix("°")).changed() || apply_settings;

            ui.horizontal(|ui|
            {
                apply_settings = ui.selectable_value(& mut light_type, LightType::Directional, "Directional").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut light_type, LightType::Point, "Point").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut light_type, LightType::Spot, "Spot").changed() || apply_settings;
            });

            apply_settings = ui.checkbox(&mut distance_based_intensity, "Distance based intensity").changed() || apply_settings;
        });

        if apply_settings
        {
            let mut light = light.borrow_mut();
            let light = light.get_mut();

            light.enabled = enabled;

            light.pos = pos;
            light.dir = dir;

            let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
            let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
            let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
            light.color = Vector3::<f32>::new(r, g, b);

            light.intensity = intensity;
            light.max_angle = max_angle.to_radians();
            light.light_type = light_type;
            light.distance_based_intensity = distance_based_intensity;
        }
    }

    pub fn print(&self)
    {
        println!("id: {:?}", self.id);
        println!("name: {:?}", self.name);
        println!("enabled: {:?}", self.enabled);

        println!("pos: {:?}", self.pos);
        println!("dir: {:?}", self.dir);
        println!("color: {:?}", self.color);

        println!("intensity: {:?}", self.intensity);
        println!("max_angle: {:?}", self.max_angle);
        println!("light_type: {:?}", self.light_type);
    }

    pub fn print_short(&self)
    {
        println!(" - (LIGHT): id={} name={} enabled={} pos=[x={}, y={}, z={}], dir=[x={}, y={}, z={}], color=[r={}, g={}, b={}], intensity={} max_angle={} light_type={:?}", self.id, self.name, self.enabled, self.pos.x, self.pos.y, self.pos.z, self.dir.x, self.dir.y, self.dir.z, self.color.x, self.color.y, self.color.z, self.intensity, self.max_angle, self.light_type);
    }
}
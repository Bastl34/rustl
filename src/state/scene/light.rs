#![allow(dead_code)]

use std::cell::RefCell;

use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};

use crate::{console_log, helper::{change_tracker::ChangeTracker}, state::scene::utilities::tags::Tags};

use super::manager::id_manager;

pub type LightItem = Box<Light>;

// ******************** LightType ********************

#[derive(PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum LightType
{
    Directional,
    Point,
    Spot,
    Hemispheric
}

// ******************** Light ********************

#[derive(Serialize, Deserialize)]
pub struct Light
{
    pub id: u32,
    pub uuid: String,

    pub name: String,

    pub enabled: bool,

    pub tags: Tags,

    pub pos: Point3<f32>,
    pub dir: Vector3<f32>,
    pub color: Vector3<f32>,
    pub ground_color: Vector3<f32>,
    pub intensity: f32,
    pub range: f32, // 0.0 == undefined (infinity)
    pub distance_based_intensity: bool,
    pub max_angle: f32, //in rad
    pub light_type: LightType,
}

impl Light
{
    pub fn new_point(name: String, pos: Point3<f32>, color: Vector3<f32>, intensity: f32, range: f32) -> Light
    {
        Self
        {
            id: id_manager::get_next_light_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name,

            enabled: true,

            tags: Tags::new(),

            pos: pos,
            dir: Vector3::<f32>::new(0.0, -1.0, 0.0),
            color: color,
            ground_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            intensity: intensity,
            range: range,
            distance_based_intensity: false,
            max_angle: 0.0,
            light_type: LightType::Point,
        }
    }

    pub fn new_directional(name: String, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32) -> Light
    {
        Self
        {
            id: id_manager::get_next_light_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name,

            enabled: true,

            tags: Tags::new(),

            pos: pos,
            dir: dir,
            color: color,
            ground_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            intensity: intensity,
            range: 0.0,
            distance_based_intensity: false,
            max_angle: 0.0,
            light_type: LightType::Directional,
        }
    }

    pub fn new_spot(name: String, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, max_angle: f32, intensity: f32, range: f32) -> Light
    {
        Self
        {
            id: id_manager::get_next_light_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name,

            enabled: true,

            tags: Tags::new(),

            pos: pos,
            dir: dir,
            color: color,
            ground_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            intensity: intensity,
            range: range,
            distance_based_intensity: false,
            max_angle: max_angle,
            light_type: LightType::Spot,
        }
    }

    pub fn new_hemi(name: String, dir: Vector3<f32>, color: Vector3<f32>, ground_color: Vector3<f32>, intensity: f32) -> Light
    {
        Self
        {
            id: id_manager::get_next_light_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name,

            enabled: true,

            tags: Tags::new(),

            pos: Point3::<f32>::new(0.0, 0.0, 0.0),
            dir: dir,
            color: color,
            ground_color: ground_color,
            intensity: intensity,
            range: 0.0,
            distance_based_intensity: false,
            max_angle: 0.0,
            light_type: LightType::Hemispheric,
        }
    }

    pub fn ui(light: &RefCell<ChangeTracker<Box<Light>>>, ui: &mut egui::Ui)
    {
        let mut enabled;

        let mut pos;
        let mut dir;
        let mut color;
        let mut ground_color;
        let mut intensity;
        let mut range;
        let mut max_angle;
        let mut light_type;
        let mut distance_based_intensity;

        {
            let light = light.borrow();
            let light = light.get_ref();

            enabled = light.enabled;

            pos = light.pos;
            dir = light.dir;

            {
                let r = (light.color.x * 255.0) as u8;
                let g = (light.color.y * 255.0) as u8;
                let b = (light.color.z * 255.0) as u8;
                color = egui::Color32::from_rgb(r, g, b);
            }

            {
                let r = (light.ground_color.x * 255.0) as u8;
                let g = (light.ground_color.y * 255.0) as u8;
                let b = (light.ground_color.z * 255.0) as u8;
                ground_color = egui::Color32::from_rgb(r, g, b);
            }

            intensity = light.intensity;
            range = light.range;
            max_angle = light.max_angle.to_degrees();
            light_type = light.light_type;
            distance_based_intensity = light.distance_based_intensity;
        }

        let mut changed = false;

        ui.vertical(|ui|
        {
            changed = ui.checkbox(&mut enabled, "Enabled").changed() || changed;

            ui.horizontal(|ui|
            {
                ui.label("pos:");
                changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
            });

            ui.horizontal(|ui|
            {
                ui.label("dir:");
                changed = ui.add(egui::DragValue::new(&mut dir.x).speed(0.1).prefix("x: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut dir.y).speed(0.1).prefix("y: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut dir.z).speed(0.1).prefix("z: ")).changed() || changed;
            });

            ui.horizontal(|ui|
            {
                ui.label("Color:");
                changed = ui.color_edit_button_srgba(&mut color).changed() || changed;
            });

            if light_type == LightType::Hemispheric
            {
                ui.horizontal(|ui|
                {
                    ui.label("Ground Color:");
                    changed = ui.color_edit_button_srgba(&mut ground_color).changed() || changed;
                });
            }

            if light_type == LightType::Directional || light_type == LightType::Hemispheric
            {
                changed = ui.add(egui::Slider::new(&mut intensity, 0.0..=1.0).text("intensity")).changed() || changed;
            }
            else
            {
                changed = ui.add(egui::Slider::new(&mut intensity, 0.0..=10000.0).text("intensity")).changed() || changed;
            }
            changed = ui.add(egui::Slider::new(&mut max_angle, 0.0..=180.0).text("max_angle").suffix("°")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut range, 0.0..=1000.0).text("range")).changed() || changed;

            ui.horizontal(|ui|
            {
                ui.label("Type:");
                changed = ui.selectable_value(& mut light_type, LightType::Directional, "Directional").changed() || changed;
                changed = ui.selectable_value(& mut light_type, LightType::Point, "Point").changed() || changed;
                changed = ui.selectable_value(& mut light_type, LightType::Spot, "Spot").changed() || changed;
                changed = ui.selectable_value(& mut light_type, LightType::Hemispheric, "Hemispheric").changed() || changed;
            });

            changed = ui.checkbox(&mut distance_based_intensity, "Distance based intensity").changed() || changed;
        });

        if changed
        {
            let mut light = light.borrow_mut();
            let light = light.get_mut();

            light.enabled = enabled;

            light.pos = pos;
            light.dir = dir;

            {
                let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                light.color = Vector3::<f32>::new(r, g, b);
            }

            {
                let r = ((ground_color.r() as f32) / 255.0).clamp(0.0, 1.0);
                let g = ((ground_color.g() as f32) / 255.0).clamp(0.0, 1.0);
                let b = ((ground_color.b() as f32) / 255.0).clamp(0.0, 1.0);
                light.ground_color = Vector3::<f32>::new(r, g, b);
            }

            light.range = range;
            light.intensity = intensity;
            light.max_angle = max_angle.to_radians();
            light.light_type = light_type;
            light.distance_based_intensity = distance_based_intensity;
        }
    }

    pub fn print(&self)
    {
        console_log!("id: {:?}", self.id);
        console_log!("name: {:?}", self.name);
        console_log!("enabled: {:?}", self.enabled);

        console_log!("pos: {:?}", self.pos);
        console_log!("dir: {:?}", self.dir);
        console_log!("color: {:?}", self.color);

        console_log!("intensity: {:?}", self.intensity);
        console_log!("max_angle: {:?}", self.max_angle);
        console_log!("light_type: {:?}", self.light_type);
    }

    pub fn print_short(&self)
    {
        console_log!(" - (LIGHT): id={} name={} enabled={} pos=[x={}, y={}, z={}], dir=[x={}, y={}, z={}], color=[r={}, g={}, b={}], intensity={} max_angle={} light_type={:?}", self.id, self.name, self.enabled, self.pos.x, self.pos.y, self.pos.z, self.dir.x, self.dir.y, self.dir.z, self.color.x, self.color.y, self.color.z, self.intensity, self.max_angle, self.light_type);
    }
}
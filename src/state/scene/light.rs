use nalgebra::{Point3, Vector3};

use crate::state::helper::render_item::{RenderItemOption};

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
            dir: Vector3::<f32>::zeros(),
            color: color,
            intensity: intensity,
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
            max_angle: max_angle,
            light_type: LightType::Spot,
        }
    }
}
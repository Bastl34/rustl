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
    pub id: u64,
    pub pos: Point3<f32>,
    pub dir: Vector3<f32>,
    pub color: Vector3<f32>,
    pub intensity: f32,
    pub max_angle: f32, //in rad
    pub light_type: LightType,

    pub render_item: RenderItemOption
}

impl Light
{
    pub fn new_point(id: u64, pos: Point3<f32>, color: Vector3<f32>, intensity: f32) -> Light
    {
        Self
        {
            enabled: true,
            id: id,
            pos: pos,
            dir: Vector3::<f32>::zeros(),
            color: color,
            intensity: intensity,
            max_angle: 0.0,
            light_type: LightType::Point,

            render_item: None
        }
    }

    pub fn name(&self) -> String
    {
        match self.light_type
        {
            LightType::Point => return "Point".to_string(),
            LightType::Directional => return "Directional".to_string(),
            LightType::Spot => return "Spot".to_string()
        }
    }
}
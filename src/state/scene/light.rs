use nalgebra::{Point3, Vector3};

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
    pub id: u32,
    pub pos: Point3<f32>,
    pub dir: Vector3<f32>,
    pub color: Vector3<f32>,
    pub intensity: f32,
    pub max_angle: f32, //in rad
    pub light_type: LightType
}

impl Light
{
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
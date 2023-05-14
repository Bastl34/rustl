use std::any::Any;

use nalgebra::{Vector3, Matrix4, Rotation3};

use super::component::Component;

pub struct Transformation
{
    position: Vector3<f32>,
    rotation: Vector3<f32>,
    scale: Vector3<f32>,
}

impl Transformation
{
    pub fn new(position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
    {
        Transformation
        {
            position,
            rotation,
            scale
        }
    }

    pub fn get_transform(&self) -> Matrix4::<f32>
    {
        let translation = nalgebra::Isometry3::translation(self.position.x, self.position.y, self.position.z).to_homogeneous();

        let scale = Matrix4::new_nonuniform_scaling(&self.scale);

        let rotation_x  = Rotation3::from_euler_angles(self.rotation.x, 0.0, 0.0).to_homogeneous();
        let rotation_y  = Rotation3::from_euler_angles(0.0, self.rotation.y, 0.0).to_homogeneous();
        let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, self.rotation.z).to_homogeneous();

        let mut trans = Matrix4::<f32>::identity();
        trans = trans * translation;
        trans = trans * scale;
        trans = trans * rotation_z;
        trans = trans * rotation_y;
        trans = trans * rotation_x;

        trans
    }
}

impl Component for Transformation
{
    fn is_enabled(&self) -> bool
    {
        true
    }

    fn name(&self) -> &'static str
    {
        "Transformation"
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
use std::convert::identity;

use nalgebra::{Rotation3, Matrix4, Vector3};

use crate::rendering::instance::InstanceRaw;

pub struct Instance
{
    position: Vector3<f32>,
    rotation: Vector3<f32>,
    scale: Vector3<f32>,
}

impl Instance
{
    pub fn new(position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Instance
    {
        Instance
        {
            position,
            rotation,
            scale
        }
    }

    pub fn get_transform(&self) -> InstanceRaw
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

        InstanceRaw
        {
            model: trans.into(),
        }
    }
}
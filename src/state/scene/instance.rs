use nalgebra::{Rotation3, Matrix3, Matrix4, Vector3};

use crate::rendering::instance::InstanceRaw;

pub struct Instance
{
    pub position: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scale: Vector3<f32>,
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

        let mut rotation = rotation_x;
        rotation = rotation * rotation_y;
        rotation = rotation * rotation_z;

        let mut trans = Matrix4::<f32>::identity();
        trans = trans * translation;
        trans = trans * scale;
        trans = trans * rotation;

        let col0 = rotation.column(0).xyz();
        let col1 = rotation.column(1).xyz();
        let col2 = rotation.column(2).xyz();

        let normal_matrix = Matrix3::from_columns
        (&[
            col0,
            col1,
            col2
        ]);

        InstanceRaw
        {
            model: trans.into(),
            normal: normal_matrix.into()
        }
    }
}
use std::{any::Any};

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3};

use crate::{component_impl_default, helper::change_tracker::ChangeTracker};

use super::component::{Component, ComponentBase};

pub struct TransformationData
{
    pub parent_inheritance: bool,

    pub position: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scale: Vector3<f32>,

    trans: Matrix4<f32>,
    tran_inverse: Matrix4<f32>,

    normal: Matrix3<f32>,
}

pub struct Transformation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationData>
}

impl Transformation
{
    pub fn new(id: u64, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,

            position,
            rotation,
            scale,

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity(),

            normal: Matrix3::<f32>::identity(),
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(id, "".to_string(), "Transformation".to_string()),
            data: ChangeTracker::new(data)
        };
        transform.calc_transform();

        transform
    }

    pub fn identity(id: u64) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,

            position: Vector3::<f32>::new(0.0, 0.0, 0.0),
            rotation: Vector3::<f32>::new(0.0, 0.0, 0.0),
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity(),

            normal: Matrix3::<f32>::identity(),
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(id, "".to_string(), "Transformation".to_string()),
            data: ChangeTracker::new(data)
        };
        transform.calc_transform();

        transform
    }

    pub fn get_data(&self) -> &TransformationData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<TransformationData>
    {
        &mut self.data
    }

    pub fn has_parent_inheritance(&self) -> bool
    {
        self.data.get_ref().parent_inheritance
    }

    pub fn calc_transform(&mut self)
    {
        let data = self.data.get_mut();

        let translation = nalgebra::Isometry3::translation(data.position.x, data.position.y, data.position.z).to_homogeneous();

        let scale = Matrix4::new_nonuniform_scaling(&data.scale);

        let rotation_x  = Rotation3::from_euler_angles(data.rotation.x, 0.0, 0.0).to_homogeneous();
        let rotation_y  = Rotation3::from_euler_angles(0.0, data.rotation.y, 0.0).to_homogeneous();
        let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, data.rotation.z).to_homogeneous();

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

        data.trans = trans;
        data.normal = normal_matrix;
        data.tran_inverse = data.trans.try_inverse().unwrap();
    }

    pub fn get_transform(&self) -> &Matrix4::<f32>
    {
        &self.data.get_ref().trans
    }

    pub fn get_transform_inverse(&self) -> &Matrix4::<f32>
    {
        &self.data.get_ref().tran_inverse
    }

    pub fn get_normal_matrix(&self) -> &Matrix3::<f32>
    {
        &self.data.get_ref().normal
    }

    pub fn apply_transformation(&mut self, translation: Vector3<f32>, scale: Vector3<f32>, rotation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.position += translation;
        data.scale.x *= scale.x;
        data.scale.y *= scale.y;
        data.scale.z *= scale.z;
        data.rotation += rotation;

        self.calc_transform();
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.position += translation;

        self.calc_transform();
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.scale.x *= scale.x;
        data.scale.y *= scale.y;
        data.scale.z *= scale.z;

        self.calc_transform();
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.rotation += rotation;

        self.calc_transform();
    }
}

impl Component for Transformation
{
    component_impl_default!();

    fn update(&mut self, _frame_scale: f32)
    {
    }
}
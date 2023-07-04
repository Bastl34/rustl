use std::{any::Any};

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3};

use crate::{component_impl_default};

use super::component::{Component, ComponentBase};

pub struct TransformationData
{
    parent_inheritance: bool,

    position: Vector3<f32>,
    rotation: Vector3<f32>,
    scale: Vector3<f32>,

    trans: Matrix4<f32>,
    tran_inverse: Matrix4<f32>,

    normal: Matrix3<f32>,
}

pub struct Transformation
{
    base: ComponentBase,
    data: TransformationData
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
            data: data
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
            data: data
        };
        transform.calc_transform();

        transform
    }

    /*
    pub fn get_data(&self) -> &TransformationData
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut TransformationData
    {
        &mut self.data
    }
    */

    pub fn has_parent_inheritance(&self) -> bool
    {
        self.data.parent_inheritance
    }

    pub fn calc_transform(&mut self)
    {
        let translation = nalgebra::Isometry3::translation(self.data.position.x, self.data.position.y, self.data.position.z).to_homogeneous();

        let scale = Matrix4::new_nonuniform_scaling(&self.data.scale);

        let rotation_x  = Rotation3::from_euler_angles(self.data.rotation.x, 0.0, 0.0).to_homogeneous();
        let rotation_y  = Rotation3::from_euler_angles(0.0, self.data.rotation.y, 0.0).to_homogeneous();
        let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, self.data.rotation.z).to_homogeneous();

        let mut rotation = rotation_x;
        rotation = rotation * rotation_y;
        rotation = rotation * rotation_z;

        let mut trans = Matrix4::<f32>::identity();
        trans = trans * translation;
        trans = trans * scale;
        trans = trans * rotation;
        /*
        trans = trans * rotation_z;
        trans = trans * rotation_y;
        trans = trans * rotation_x;
         */

        let col0 = rotation.column(0).xyz();
        let col1 = rotation.column(1).xyz();
        let col2 = rotation.column(2).xyz();

        let normal_matrix = Matrix3::from_columns
        (&[
            col0,
            col1,
            col2
        ]);

        self.data.trans = trans;
        self.data.normal = normal_matrix;
        self.data.tran_inverse = self.data.trans.try_inverse().unwrap();
    }

    pub fn get_transform(&self) -> &Matrix4::<f32>
    {
        &self.data.trans
    }

    pub fn get_transform_inverse(&self) -> &Matrix4::<f32>
    {
        &self.data.tran_inverse
    }

    pub fn get_normal_matrix(&self) -> &Matrix3::<f32>
    {
        &self.data.normal
    }

    pub fn apply_transformation(&mut self, translation: Vector3<f32>, scale: Vector3<f32>, rotation: Vector3<f32>)
    {
        self.data.position += translation;
        self.data.scale.x *= scale.x;
        self.data.scale.y *= scale.y;
        self.data.scale.z *= scale.z;
        self.data.rotation += rotation;

        self.calc_transform();
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        self.data.position += translation;

        self.calc_transform();
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>)
    {
        self.data.scale.x *= scale.x;
        self.data.scale.y *= scale.y;
        self.data.scale.z *= scale.z;

        self.calc_transform();
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        self.data.rotation += rotation;

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
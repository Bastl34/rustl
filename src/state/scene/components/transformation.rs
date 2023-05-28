use std::{any::Any, ops::Mul};

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3};

use crate::state::scene::node::{NodeItem, Node};

use super::component::Component;

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
    pub is_enabled: bool,
    data: TransformationData
}

impl Transformation
{
    pub fn new(position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
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
            is_enabled: true,
            data: data
        };
        transform.calc_transform();

        transform
    }

    pub fn identity() -> Transformation
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
            is_enabled: true,
            data: data
        };
        transform.calc_transform();

        transform
    }

    pub fn get_data(&self) -> &TransformationData
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut TransformationData
    {
        &mut self.data
    }

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
    fn is_enabled(&self) -> bool
    {
        self.is_enabled
    }

    fn component_name(&self) -> &'static str
    {
        "Transformation"
    }

    fn update(&mut self, time_delta: f32)
    {
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
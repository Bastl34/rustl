use std::{any::Any};

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3};

use crate::{component_impl_default, helper::change_tracker::ChangeTracker};

use super::component::{Component, ComponentBase};

pub struct TransformationData
{
    pub parent_inheritance: bool,
    pub transform_vectors: bool, // if disabled - only trans matrix is used (position, rotation, scale vectors are ignored)

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
            transform_vectors: true,

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

    pub fn new_transformation_only(id: u64, trans: Matrix4::<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: false,

            position: Vector3::<f32>::zeros(),
            rotation: Vector3::<f32>::zeros(),
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            trans: trans,
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
            transform_vectors: true,

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

    pub fn get_data_tracker(&self) -> &ChangeTracker<TransformationData>
    {
        &self.data
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

        if data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(data.position.x, data.position.y, data.position.z).to_homogeneous();

            let scale = Matrix4::new_nonuniform_scaling(&data.scale);

            let rotation_x  = Rotation3::from_euler_angles(data.rotation.x, 0.0, 0.0).to_homogeneous();
            let rotation_y  = Rotation3::from_euler_angles(0.0, data.rotation.y, 0.0).to_homogeneous();
            let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, data.rotation.z).to_homogeneous();

            let mut rotation = rotation_z;
            rotation = rotation * rotation_y;
            rotation = rotation * rotation_x;

            let mut trans = Matrix4::<f32>::identity();
            trans = trans * translation;
            trans = trans * rotation;
            trans = trans * scale;

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
        }
        else
        {
            //https://stackoverflow.com/questions/21079623/how-to-calculate-the-normal-matrix
            let upper_left_3x3 = data.trans.fixed_view::<3, 3>(0, 0);
            let normal_matrix = upper_left_3x3.try_inverse().map(|inv| inv.transpose()).unwrap_or_else(Matrix3::identity);

            data.normal = normal_matrix;
        }

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

        if !data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();

            let scale = Matrix4::new_nonuniform_scaling(&scale);

            let rotation_x  = Rotation3::from_euler_angles(rotation.x, 0.0, 0.0).to_homogeneous();
            let rotation_y  = Rotation3::from_euler_angles(0.0, rotation.y, 0.0).to_homogeneous();
            let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, rotation.z).to_homogeneous();

            let mut rotation = rotation_z;
            rotation = rotation * rotation_y;
            rotation = rotation * rotation_x;

            let mut trans = Matrix4::<f32>::identity();
            trans = trans * translation;
            trans = trans * rotation;
            trans = trans * scale;

            data.trans = data.trans * rotation;
        }

        self.calc_transform();
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.position += translation;

        if !data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
            data.trans = data.trans * translation;
        }

        self.calc_transform();
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.scale.x *= scale.x;
        data.scale.y *= scale.y;
        data.scale.z *= scale.z;

        if !data.transform_vectors
        {
            let scale = Matrix4::new_nonuniform_scaling(&data.scale);
            data.trans = data.trans * scale;
        }

        self.calc_transform();
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.rotation += rotation;

        if !data.transform_vectors
        {
            let rotation_x  = Rotation3::from_euler_angles(rotation.x, 0.0, 0.0).to_homogeneous();
            let rotation_y  = Rotation3::from_euler_angles(0.0, rotation.y, 0.0).to_homogeneous();
            let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, rotation.z).to_homogeneous();

            let mut rotation = rotation_z;
            rotation = rotation * rotation_y;
            rotation = rotation * rotation_x;

            data.trans = data.trans * rotation;
        }

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
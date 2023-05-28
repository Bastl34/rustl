use std::any::Any;

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3};

use crate::state::scene::node::NodeItem;

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

        Transformation { data: data }
    }

    pub fn get_data(&self) -> &TransformationData
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut TransformationData
    {
        &mut self.data
    }

    pub fn calc_transform(&self) -> (Matrix4::<f32>, Matrix3::<f32>)
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

        (trans, normal_matrix)
    }

    fn get_full_transform(&self, node: NodeItem) -> (Matrix4::<f32>, Matrix3::<f32>)
    {
        let transform = self.calc_transform();

        let mut parent_transform = (Matrix4::<f32>::identity(), Matrix3::<f32>::identity());

        let node = node.read().unwrap();
        if node.parent.is_some()
        {
            let parent_node = node.parent.clone().unwrap();
            let parent_read = parent_node.read().unwrap();

            let parent_transform_component = parent_read.find_component::<Transformation>();

            if let Some(parent_transform_component) = parent_transform_component
            {
                let data = parent_transform_component.get_data();

                if data.parent_inheritance
                {
                    parent_transform = parent_transform_component.get_full_transform(parent_node.clone()).clone();
                }
            }
        }

        // 0 = transformation, 1 = normal matrix
        (
            parent_transform.0 * transform.0,
            parent_transform.1 * transform.1,
        )
    }

    pub fn calc_full_transform(&mut self, node: NodeItem)
    {
        let trans = self.get_full_transform(node);
        self.data.trans = trans.0;
        self.data.normal = trans.1;
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
}

impl Component for Transformation
{
    fn is_enabled(&self) -> bool
    {
        true
    }

    fn component_name(&self) -> &'static str
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
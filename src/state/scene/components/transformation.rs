use std::any::Any;

use nalgebra::{Vector3, Matrix4, Rotation3};

use crate::state::scene::node::NodeItem;

use super::component::Component;

pub struct Transformation
{
    position: Vector3<f32>,
    rotation: Vector3<f32>,
    scale: Vector3<f32>,

    pub trans: Matrix4<f32>,
    tran_inverse: Matrix4<f32>,
}

impl Transformation
{
    pub fn new(position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
    {
        Transformation
        {
            position,
            rotation,
            scale,

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity(),
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

    pub fn get_full_transform(&self, node: NodeItem) -> Matrix4::<f32>
    {
        let transform = self.get_transform();

        let mut parent_transform = Matrix4::<f32>::identity();

        let node = node.read().unwrap();
        if node.parent.is_some()
        {
            let parent_node = node.parent.clone().unwrap();
            let parent_read = parent_node.read().unwrap();

            let parent_transform_component = parent_read.find_component::<Transformation>();

            if parent_transform_component.is_some()
            {
                parent_transform = parent_transform_component.unwrap().get_full_transform(parent_node.clone()).clone();
            }
        }

        parent_transform * transform
    }

    pub fn calc_full_transform(&mut self, node: NodeItem)
    {
        self.trans = self.get_full_transform(node);
        self.tran_inverse = self.trans.try_inverse().unwrap();
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
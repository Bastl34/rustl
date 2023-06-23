use nalgebra::{Matrix3, Matrix4, Vector3};

use super::{node::{NodeItem, Node}, components::{transformation::{Transformation}}};

pub type InstanceItem = Box<Instance>;

pub struct Instance
{
    id: u64,
    name: String,

    node: NodeItem,

    transform: Transformation,
}

impl Instance
{
    pub fn new(id: u64, name: String, node: NodeItem) -> Instance
    {
        let instance = Instance
        {
            id: id,
            name: name,
            node: node,
            transform: Transformation::new
            (
                0,
                Vector3::<f32>::zeros(),
                Vector3::<f32>::zeros(),
                Vector3::<f32>::new(1.0, 1.0, 1.0)
            )
        };

        instance
    }

    pub fn new_with_transform(id: u64, name: String, node: NodeItem, transform: Transformation) -> Instance
    {
        let instance = Instance
        {
            id: id,
            name: name,
            node: node,
            transform: transform
        };

        instance
    }

    pub fn new_with_data(id: u64, name: String, node: NodeItem, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Instance
    {
        let instance = Instance
        {
            id: id,
            name: name,
            node: node,
            transform: Transformation::new(0, position, rotation, scale)
        };

        instance
    }

    pub fn get_transform(&self) -> (Matrix4::<f32>, Matrix3::<f32>)
    {
        let (trans, normal) = Node::get_full_transform(self.node.clone());

        (
            trans * self.transform.get_transform(),
            normal * self.transform.get_normal_matrix(),
        )
    }

    pub fn apply_transform(&mut self, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>)
    {
        self.transform.apply_transformation(position, scale, rotation);
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        self.transform.apply_translation(translation);
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>)
    {
        self.transform.apply_scale(scale);
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        self.transform.apply_rotation(rotation);
        self.transform.calc_transform();
    }
}
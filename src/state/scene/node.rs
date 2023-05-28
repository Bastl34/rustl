use std::{sync::{Arc, RwLock}};
use bvh::aabb::Bounded;
use bvh::bounding_hierarchy::BHShape;
use nalgebra::{Matrix4, Matrix3};

use super::{components::{component::{ComponentItem, SharedComponentItem, Component}, mesh::Mesh, transformation::Transformation}, instance::InstanceItem};

pub type NodeItem = Arc<RwLock<Box<Node>>>;

pub struct Node
{
    id: u64,
    name: String,
    pub visible: bool,

    pub parent: Option<NodeItem>,

    pub nodes: Vec<NodeItem>,
    pub instances: Vec<InstanceItem>,

    pub components: Vec<ComponentItem>,
    pub shared_components: Vec<SharedComponentItem>,

    // bounding box
    b_box_node_index: usize,
}

impl Node
{
    pub fn new(id: u64, name: &str) -> NodeItem
    {
        let node = Self
        {
            id: id,
            name: name.to_string(),
            visible: true,

            components: vec![],
            shared_components: vec![],

            parent: None,
            nodes: vec![],
            instances: vec![],

            b_box_node_index: 0
        };

        Arc::new(RwLock::new(Box::new(node)))
    }

    pub fn add_node(node: NodeItem, child_node: NodeItem)
    {
        {
            let mut node = node.write().unwrap();
            node.nodes.push(child_node.clone());
        }

        {
            let mut child_node = child_node.write().unwrap();
            child_node.parent = Some(node.clone());
        }
    }

    pub fn add_component(&mut self, component: ComponentItem)
    {
        self.components.push(component);
    }

    pub fn remove_component_by_type<T>(&mut self) where T: 'static
    {
        let index = self.components.iter().position
        (
            |c|
            {
                let component_item = c.as_any();
                component_item.is::<T>()
            }
        );

        if let Some(index) = index
        {
            self.components.remove(index);
        }
    }

    pub fn remove_component_by_id(&mut self, id: u64)
    {
        let index = self.components.iter().position
        (
            |c|
            {
                c.id() == id
            }
        );

        if let Some(index) = index
        {
            self.components.remove(index);
        }
    }

    pub fn find_component<'a, T>(&'a self) -> Option<Box<&'a T>> where T: 'static
    {
        let value = self.components.iter().find
        (
            |c|
            {
                let component_item = c.as_any();
                component_item.is::<T>()
            }
        );

        if !value.is_some()
        {
            return None;
        }

        let any = value.unwrap().as_any();
        let downcast = Box::new(any.downcast_ref::<T>().unwrap());

        Some(downcast)
    }

    pub fn find_component_mut<'a, T>(&'a mut self) -> Option<Box<&'a mut T>> where T: 'static
    {
        let value = self.components.iter_mut().find
        (
            |c|
            {
                let component_item = c.as_any();
                component_item.is::<T>()
            }
        );

        if !value.is_some()
        {
            return None;
        }

        let any = value.unwrap().as_any_mut();
        let downcast = Box::new(any.downcast_mut::<T>().unwrap());

        Some(downcast)
    }

    pub fn find_shared_component<T>(&self) -> Option<SharedComponentItem> where T: 'static
    {
        let value = self.shared_components.iter().find
        (
            |c|
            {
                let component = c.read().unwrap();
                let component_item = component.as_any();
                component_item.is::<T>()
            }
        );

        if !value.is_some()
        {
            return None;
        }

        Some(value.unwrap().clone())
    }

    pub fn find_shared_component_mut<T>(&mut self) -> Option<SharedComponentItem> where T: 'static
    {
        let value = self.shared_components.iter_mut().find
        (
            |c|
            {
                let component = c.read().unwrap();
                let component_item = component.as_any();
                component_item.is::<T>()
            }
        );

        if !value.is_some()
        {
            return None;
        }

        Some(value.unwrap().clone())
    }

    pub fn remove_shared_component_by_type<T>(&mut self) where T: 'static
    {
        let index = self.shared_components.iter().position
        (
            |c|
            {
                let component = c.read().unwrap();
                let component_item = component.as_any();
                component_item.is::<T>()
            }
        );

        if let Some(index) = index
        {
            self.shared_components.remove(index);
        }
    }

    pub fn remove_shared_component_by_id(&mut self, id: u64)
    {
        let index = self.shared_components.iter().position
        (
            |c|
            {
                let component = c.read().unwrap();
                component.id() == id
            }
        );

        if let Some(index) = index
        {
            self.components.remove(index);
        }
    }

    pub fn add_shared_component(&mut self, component: SharedComponentItem)
    {
        self.shared_components.push(component);
    }

    fn get_mesh(&self) -> Option<Box<&Mesh>>
    {
        self.find_component::<Mesh>()
    }

    pub fn get_transform(&self) -> (Matrix4<f32>, Matrix3<f32>)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            if transform_component.get_base().is_enabled
            {
                return
                (
                    transform_component.get_transform().clone(),
                    transform_component.get_normal_matrix().clone()
                );
            }
        }

        (
            Matrix4::<f32>::identity(),
            Matrix3::<f32>::identity()
        )
    }

    pub fn get_full_transform(node: NodeItem) -> (Matrix4<f32>, Matrix3<f32>)
    {
        let node = node.read().unwrap();

        let node_transform = node.get_transform();
        let mut parent_transform = (Matrix4::<f32>::identity(), Matrix3::<f32>::identity());

        if let Some(parent_node) = &node.parent
        {
            let parent = parent_node.read().unwrap();

            parent_transform = parent.get_transform();
        }

        // 0 = transformation, 1 = normal matrix
        (
            parent_transform.0 * node_transform.0,
            parent_transform.1 * node_transform.1,
        )
    }

    pub fn add_instance(&mut self, instance: InstanceItem)
    {
        self.instances.push(instance);
    }

    pub fn update(&mut self, frame_scale: f32)
    {
        // update components
        for component in &mut self.components
        {
            component.update(frame_scale);
        }

        for component in &mut self.shared_components
        {
            let mut component_write = component.write().unwrap();
            component_write.update(frame_scale);
        }

        // update instances
        for instance in &mut self.instances
        {
            instance.update(frame_scale);
        }

        // update nodes
        for node in &mut self.nodes
        {
            node.write().unwrap().update(frame_scale);
        }

    }

    pub fn print(&self, level: usize)
    {
        let spaces = " ".repeat(level * 2);
        println!("{} - (NODE) id={} name={} visible={} components={}, shared_components={}", spaces, self.id, self.name, self.visible, self.components.len(), self.shared_components.len());

        for node in &self.nodes
        {
            node.read().unwrap().print(level + 1);
        }
    }
}

// ******************** bounding box ********************
impl Bounded for Node
{
    fn aabb(&self) -> bvh::aabb::AABB
    {
        let mesh = self.get_mesh();

        if mesh.is_none()
        {
            return bvh::aabb::AABB::empty();
        }

        let (trans, _) = self.get_transform();

        let mesh_data = mesh.unwrap().get_data();

        let aabb = mesh_data.b_box;
        let verts = aabb.vertices();

        let mut min = verts[0];
        let mut max = verts[0];

        for vert in &verts
        {
            let transformed = trans * vert.to_homogeneous();

            min.x = min.x.min(transformed.x);
            min.y = min.y.min(transformed.y);
            min.z = min.z.min(transformed.z);

            max.x = max.x.max(transformed.x);
            max.y = max.y.max(transformed.y);
            max.z = max.z.max(transformed.z);
        }

        let min = bvh::Point3::new(min.x, min.y, min.z);
        let max = bvh::Point3::new(max.x, max.y, max.z);

        bvh::aabb::AABB::with_bounds(min, max)
    }
}

impl BHShape for Node
{
    fn set_bh_node_index(&mut self, index: usize)
    {
        self.b_box_node_index = index;
    }

    fn bh_node_index(&self) -> usize
    {
        self.b_box_node_index
    }
}

// ******************** macros ********************
#[macro_export]
macro_rules! shared_component_write
{
    ($component:ident, $component_type:ty, $result:ident) =>
    {
        let mut writable = $component.write().unwrap();
        let $result = writable.as_any_mut().downcast_mut::<$component_type>().unwrap();
    };
}

macro_rules! shared_component_read
{
    ($component:ident, $component_type:ty, $result:ident) =>
    {
        let mut readable = $component.read().unwrap();
        let $result = readable.as_any_mut().downcast_ref::<$component_type>().unwrap();
    };
}
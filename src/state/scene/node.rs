use std::{sync::{Arc, RwLock}, cell::{RefCell}};
use bvh::aabb::Bounded;
use bvh::bounding_hierarchy::BHShape;
use nalgebra::{Matrix4, Matrix3, Vector3};

use crate::{state::helper::render_item::RenderItemOption, helper::change_tracker::ChangeTracker};

use super::{components::{component::{ComponentItem, SharedComponentItem, Component}, mesh::Mesh, transformation::Transformation, material::Material}, instance::{InstanceItem, Instance}};

pub type NodeItem = Arc<RwLock<Box<Node>>>;
pub type InstanceItemChangeTrack = RefCell<ChangeTracker<InstanceItem>>;

pub struct Node
{
    pub id: u64,
    pub name: String,
    pub visible: bool,

    pub render_children_first: bool,

    pub parent: Option<NodeItem>,

    pub nodes: Vec<NodeItem>,
    pub instances: ChangeTracker<Vec<InstanceItemChangeTrack>>,

    pub components: Vec<ComponentItem>,
    pub shared_components: Vec<SharedComponentItem>,

    pub instance_render_item: RenderItemOption,

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

            render_children_first: false,

            components: vec![],
            shared_components: vec![],

            parent: None,
            nodes: vec![],
            instances: ChangeTracker::new(vec![]),

            instance_render_item: None,

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

    pub fn find_components<'a, T>(&'a self) -> Option<Vec<Box<&'a T>>> where T: 'static
    {
        let values:Vec<_>  = self.components.iter().filter
        (
            |c|
            {
                let component_item = c.as_any();
                component_item.is::<T>()
            }
        ) .collect();

        if values.len() == 0
        {
            return None;
        }

        let res = values.iter().map(|component|
        {
            let any = component.as_any();
            Box::new(any.downcast_ref::<T>().unwrap())
        }).collect::<Vec<Box<&'a T>>>();

        Some(res)
    }

    pub fn find_components_mut<'a, T>(&'a mut self) -> Option<Vec<Box<&'a mut T>>> where T: 'static
    {
        let values:Vec<_>  = self.components.iter_mut().filter
        (
            |c|
            {
                let component_item = c.as_any();
                component_item.is::<T>()
            }
        ) .collect();

        if values.len() == 0
        {
            return None;
        }

        let mut res: Vec<Box<&mut T>> = vec![];
        for component in values
        {
            let any = component.as_any_mut();
            res.push(Box::new(any.downcast_mut::<T>().unwrap()))
        }

        Some(res)
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

    pub fn get_mesh(&self) -> Option<Box<&Mesh>>
    {
        self.find_component::<Mesh>()
    }

    pub fn get_meshes(&self) -> Option<Vec<Box<&Mesh>>>
    {
        self.find_components::<Mesh>()
    }

    pub fn get_transform(&self) -> (Matrix4<f32>, Matrix3<f32>, bool)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            if transform_component.get_base().is_enabled
            {
                return
                (
                    transform_component.get_transform().clone(),
                    transform_component.get_normal_matrix().clone(),
                    transform_component.has_parent_inheritance()
                );
            }
        }

        (
            Matrix4::<f32>::identity(),
            Matrix3::<f32>::identity(),
            true
        )
    }

    pub fn get_full_transform(node: NodeItem) -> (Matrix4<f32>, Matrix3<f32>)
    {
        let node = node.read().unwrap();

        let (node_transform, node_normal_matrix, node_parent_inheritance) = node.get_transform();
        let (mut parent_trans, mut parent_normal_matrix) = (Matrix4::<f32>::identity(), Matrix3::<f32>::identity());

        if let Some(parent_node) = &node.parent
        {
            (parent_trans, parent_normal_matrix) = Self::get_full_transform(parent_node.clone());
        }

        if node_parent_inheritance
        {
            (
                parent_trans * node_transform,
                parent_normal_matrix * node_normal_matrix,
            )
        }
        else
        {
            (
                node_transform,
                node_normal_matrix,
            )
        }
    }

    pub fn get_alpha(node: NodeItem) -> f32
    {
        let node = node.read().unwrap();

        let mat = node.find_shared_component::<Material>();

        if let Some(mat) = mat
        {
            let mat = mat.read().unwrap();
            let mat = mat.as_any().downcast_ref::<Material>().unwrap();
            let mat_data = mat.get_data();
            let alpha = mat_data.alpha;

            if mat_data.alpha_inheritance && node.parent.is_some()
            {
                return Self::get_alpha(node.parent.as_ref().unwrap().clone()) * alpha;
            }
            else
            {
                return alpha;
            }
        }

        1.0
    }

    pub fn is_empty(&self) -> bool
    {
        let has_meshes = self.get_mesh().is_some();

        if has_meshes
        {
            return false;
        }
        else if !has_meshes && self.nodes.len() == 0
        {
            return true;
        }

        let mut is_not_empty = false;
        for node in &self.nodes
        {
            let node = node.read().unwrap();
            is_not_empty = is_not_empty || !node.is_empty();
        }

        !is_not_empty
    }

    pub fn create_default_instance(&mut self, self_node_item: NodeItem, instance_id: u64)
    {
        let instance = Instance::new_with_data
        (
            instance_id,
            "instance".to_string(),
            self_node_item,
            Vector3::<f32>::new(0.0, 0.0, 0.0),
            Vector3::<f32>::new(0.0, 0.0, 0.0),
            Vector3::<f32>::new(1.0, 1.0, 1.0)
        );

        self.add_instance(Box::new(instance));
    }

    pub fn add_instance(&mut self, instance: InstanceItem)
    {
        self.instances.get_mut().push(RefCell::new(ChangeTracker::new(instance)));
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

        // update nodes
        for node in &mut self.nodes
        {
            node.write().unwrap().update(frame_scale);
        }
    }

    pub fn merge_mesh(&mut self, node: &NodeItem) -> bool
    {
        let merge_read = node.read().unwrap();
        let merge_mesh = merge_read.find_component::<Mesh>();
        let current_mesh = self.find_component_mut::<Mesh>();

        if current_mesh.is_none() || merge_mesh.is_none()
        {
            println!("can not merge node -> can not merge empty mesh");
            return false;
        }

        let mesh_data = merge_mesh.unwrap().get_data();
        current_mesh.unwrap().merge(mesh_data);

        true
    }

    pub fn find_instance_by_id(&self, id: u64) -> Option<&InstanceItemChangeTrack>
    {
        for instance in self.instances.get_ref()
        {
            if instance.borrow().get_ref().id == id
            {
                return Some(instance);
            }
        }

        None
    }

    pub fn delete_instance_by_id(&mut self, id: u64) -> bool
    {
        let len = self.instances.get_ref().len();
        self.instances.get_mut().retain(|instance|
        {
            instance.borrow().get_ref().id != id
        });

        self.instances.get_ref().len() != len
    }

    pub fn print(&self, level: usize)
    {
        let spaces = " ".repeat(level * 2);
        println!("{} - (NODE) id={} name={} visible={} components={}, shared_components={} instances={}", spaces, self.id, self.name, self.visible, self.components.len(), self.shared_components.len(), self.instances.get_ref().len());

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

        let (trans, _, _) = self.get_transform();

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
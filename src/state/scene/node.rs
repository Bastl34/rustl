use std::{sync::{Arc, RwLock}, cell::RefCell};
use bvh::aabb::Bounded;
use bvh::bounding_hierarchy::BHShape;
use nalgebra::{Matrix4, Point3};

use crate::{state::helper::render_item::RenderItemOption, helper::change_tracker::ChangeTracker, component_downcast, component_downcast_mut, input::input_manager::InputManager};

use super::{components::{component::{ComponentItem, Component, find_component, find_components, remove_component_by_type, remove_component_by_id, find_component_by_id}, mesh::Mesh, transformation::Transformation, alpha::Alpha}, instance::{InstanceItem, Instance}};

pub type NodeItem = Arc<RwLock<Box<Node>>>;
pub type InstanceItemArc = Arc<RwLock<InstanceItem>>;

const UPDATE_ALL_INSTANCES_THRESHOLD: u32 = 10; // if more than 10 instances got an update -> update all instances at once to save performance

pub struct Node
{
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub root_node: bool,

    pub render_children_first: bool,
    pub alpha_index: u64, // this can be used to influence the sorting (for rendering)

    pub parent: Option<NodeItem>,

    pub nodes: Vec<NodeItem>,
    //pub instances: ChangeTracker<Vec<RefCell<ChangeTracker<InstanceItem>>>>,
    //pub instances: ChangeTracker<Vec<RefCell<InstanceItem>>>,
    pub instances: ChangeTracker<Vec<Arc<RwLock<InstanceItem>>>>,

    pub components: Vec<ComponentItem>,

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
            root_node: false,

            render_children_first: false,
            alpha_index: 0,

            components: vec![],

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

    pub fn find_component<T>(&self) -> Option<ComponentItem> where T: 'static
    {
        find_component::<T>(&self.components)
    }

    pub fn find_component_by_id(&self, id: u64) -> Option<ComponentItem>
    {
        find_component_by_id(&self.components, id)
    }

    pub fn find_components<T: Component>(&self) -> Vec<ComponentItem> where T: 'static
    {
        find_components::<T>(&self.components)
    }

    pub fn remove_component_by_type<T>(&mut self) where T: 'static
    {
        if remove_component_by_type::<T>(&mut self.components)
        {
            self.force_instances_update();
        }
    }

    pub fn remove_component_by_id(&mut self, id: u64)
    {
        if remove_component_by_id(&mut self.components, id)
        {
            self.force_instances_update();
        }
    }

    pub fn get_mesh(&self) -> Option<ComponentItem>
    {
        self.find_component::<Mesh>()
    }

    pub fn get_meshes(&self) -> Vec<ComponentItem>
    {
        self.find_components::<Mesh>()
    }

    pub fn get_bounding_info(&self, recursive: bool) -> Option<(Point3<f32>, Point3<f32>)>
    {
        let meshes = self.get_meshes();

        let mut min = Point3::<f32>::new(std::f32::MAX, std::f32::MAX, std::f32::MAX);
        let mut max = Point3::<f32>::new(std::f32::MIN, std::f32::MIN, std::f32::MIN);

        let mut found = false;

        // own meshes
        for instance in self.instances.get_ref()
        {
            let instance = instance.read().unwrap();
            let transform = instance.calculate_transform();

            for mesh in &meshes
            {
                component_downcast!(mesh, Mesh);
                let bbox = mesh.get_data().b_box;

                let transformed_min = transform * Point3::<f32>::new(bbox.mins.x, bbox.mins.y, bbox.mins.z).to_homogeneous();
                let transformed_max = transform * Point3::<f32>::new(bbox.maxs.x, bbox.maxs.y, bbox.maxs.z).to_homogeneous();

                // sometimes coordinates are flipped because of the transformation -> check for min and max points

                min.x = min.x.min(transformed_min.x);
                min.y = min.y.min(transformed_min.y);
                min.z = min.z.min(transformed_min.z);

                min.x = min.x.min(transformed_max.x);
                min.y = min.y.min(transformed_max.y);
                min.z = min.z.min(transformed_max.z);


                max.x = max.x.max(transformed_min.x);
                max.y = max.y.max(transformed_min.y);
                max.z = max.z.max(transformed_min.z);

                max.x = max.x.max(transformed_max.x);
                max.y = max.y.max(transformed_max.y);
                max.z = max.z.max(transformed_max.z);

                found = true;
            }
        }

        // meshes of child nodes
        if recursive
        {
            for node in &self.nodes
            {
                let node = node.read().unwrap();
                let child_min_max = node.get_bounding_info(recursive);

                if let Some(child_min_max) = child_min_max
                {
                    let (child_min, child_max) = child_min_max;

                    min.x = min.x.min(child_min.x);
                    min.y = min.y.min(child_min.y);
                    min.z = min.z.min(child_min.z);

                    max.x = max.x.max(child_max.x);
                    max.y = max.y.max(child_max.y);
                    max.z = max.z.max(child_max.z);

                    found = true;
                }
            }
        }

        if found
        {
            return Some((min, max));
        }

        None
    }

    pub fn get_center(&self, recursive: bool) -> Option<Point3<f32>>
    {
        let bounding_info = self.get_bounding_info(recursive);

        if let Some(bounding_info) = bounding_info
        {
            let (min, max) = bounding_info;

            return Some(min + (max - min) / 2.0);
        }

        None
    }

    pub fn has_changed_instance_data(&self) -> bool
    {
        for instance in self.instances.get_ref()
        {
            let instance = instance.read().unwrap();
            if instance.get_data_tracker().changed()
            {
                return true;
            }
        }

        false
    }

    pub fn get_transform(&self) -> (Matrix4<f32>, bool)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            if transform_component.get_base().is_enabled
            {
                return
                (
                    transform_component.get_transform().clone(),
                    transform_component.has_parent_inheritance()
                );
            }
        }

        (
            Matrix4::<f32>::identity(),
            true
        )
    }

    pub fn get_full_transform(&self) -> Matrix4<f32>
    {
        let (node_transform, node_parent_inheritance) = self.get_transform();
        let mut parent_trans = Matrix4::<f32>::identity();

        if let Some(parent_node) = &self.parent
        {
            let parent_node = parent_node.read().unwrap();
            parent_trans = parent_node.get_full_transform();
        }

        if node_parent_inheritance
        {
            parent_trans * node_transform
        }
        else
        {
            node_transform
        }
    }

    pub fn get_alpha(&self) -> (f32, bool)
    {
        let alpha_component = self.find_component::<Alpha>();

        if let Some(alpha_component) = alpha_component
        {
            component_downcast!(alpha_component, Alpha);

            if alpha_component.get_base().is_enabled
            {
                return
                (
                    alpha_component.get_alpha(),
                    alpha_component.has_alpha_inheritance()
                );
            }
        }

        (1.0, true)
    }

    pub fn get_full_alpha(node: NodeItem) -> f32
    {
        let node = node.read().unwrap();

        let (node_alpha, node_parent_inheritance) = node.get_alpha();
        let mut parent_alpha = 1.0;

        if let Some(parent_node) = &node.parent
        {
            parent_alpha = Self::get_full_alpha(parent_node.clone());
        }

        if node_parent_inheritance
        {
            parent_alpha * node_alpha
        }
        else
        {
            node_alpha
        }
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
        let instance = Instance::new
        (
            instance_id,
            "instance".to_string(),
            self_node_item
        );

        self.add_instance(Box::new(instance));
    }

    pub fn add_instance(&mut self, instance: InstanceItem)
    {
        self.instances.get_mut().push(Arc::new(RwLock::new(instance)));
    }

    pub fn update(node: NodeItem, input_manager: &mut InputManager, frame_scale: f32)
    {
        // ***** copy all components *****
        let all_components;
        {
            let node = node.write().unwrap();
            all_components = node.components.clone();
        }

        for (component_id, component) in all_components.clone().iter_mut().enumerate()
        {
            {
                if !component.read().unwrap().is_enabled()
                {
                    continue;
                }
            }

            // remove the component itself  for the component update
            {
                let mut node = node.write().unwrap();
                node.components = all_components.clone();
                node.components.remove(component_id);
            }

            let mut component_write = component.write().unwrap();
            component_write.update(node.clone(), input_manager, frame_scale);
        }

        // ***** reassign components *****
        {
            let mut node = node.write().unwrap();
            node.components = all_components;
        }

        // ***** update instances *****
        {
            let mut updates = 0;
            {
                let node_read = node.read().unwrap();
                for instance in node_read.instances.get_ref()
                {
                    if Instance::update(&instance, input_manager, frame_scale)
                    {
                        updates += 1;
                    }
                }
            }

            // if more than UPDATE_ALL_INSTANCES_THRESHOLD instances got an update -> update all instances at once to save performance
            if updates >= UPDATE_ALL_INSTANCES_THRESHOLD
            {
                let mut node = node.write().unwrap();
                node.instances.force_change();
            }

            // consume alpha and transform manually (not prevent useless updates)
            /*
            let node_read = node.read().unwrap();
            let transform_component = node_read.find_component::<Transformation>();
            let alpha_component = node_read.find_component::<Alpha>();

            if let Some(transform_component) = transform_component
            {
                component_downcast_mut!(transform_component, Transformation);
                transform_component.get_data_mut().consume();
            }

            if let Some(alpha_component) = alpha_component
            {
                component_downcast_mut!(alpha_component, Alpha);
                alpha_component.get_data_mut().consume();
            }
             */
        }

        // ***** update childs *****
        let node_read = node.read().unwrap();
        for child_node in &node_read.nodes
        {
            Self::update(child_node.clone(), input_manager, frame_scale);
        }
    }

    pub fn merge_mesh(&mut self, node: &NodeItem) -> bool
    {
        let merge_read = node.read().unwrap();
        let merge_mesh = merge_read.find_component::<Mesh>();
        let current_mesh = self.find_component::<Mesh>();

        if current_mesh.is_none() || merge_mesh.is_none()
        {
            println!("can not merge node -> can not merge empty mesh");
            return false;
        }

        let merge_mesh = merge_mesh.unwrap();
        let current_mesh = current_mesh.unwrap();

        component_downcast!(merge_mesh, Mesh);
        component_downcast_mut!(current_mesh, Mesh);

        let mesh_data = merge_mesh.get_data();
        current_mesh.merge(mesh_data);

        true
    }

    pub fn merge_instances(&mut self) -> bool
    {
        let meshes = self.get_meshes();

        if meshes.len() == 0
        {
            return false;
        }

        if self.instances.get_ref().len() == 0
        {
            return false;
        }

        // get all transformations
        let mut transformations = vec![];

        let instances = self.instances.get_ref();
        for instance in instances
        {
            let instance = instance.read().unwrap();

            let mut matrix = Matrix4::<f32>::identity();

            let transform_component: Option<Arc<RwLock<Box<dyn Component + Send + Sync>>>> = instance.find_component::<Transformation>();

            if let Some(transform_component) = transform_component
            {
                component_downcast_mut!(transform_component, Transformation);

                // force update
                transform_component.calc_transform();
                matrix = transform_component.get_transform().clone();
            }

            transformations.push(matrix);
        }

        // apply all transformations
        for mesh in meshes
        {
            component_downcast_mut!(mesh, Mesh);
            mesh.merge_by_transformations(&transformations);
        }

        // clear and create new single instance
        let instance_id;
        let node;
        {
            let first_instance = self.instances.get_ref().first().unwrap();
            let first_instance = first_instance.read().unwrap();

            instance_id = first_instance.id;
            node = first_instance.node.clone();
        }

        self.clear_instances();
        self.create_default_instance(node, instance_id);

        true
    }

    pub fn force_instances_update(&mut self)
    {
        for instance in self.instances.get_ref()
        {
            let mut instance = instance.write().unwrap();
            instance.set_force_update();
        }
    }

    pub fn find_instance_by_id(&self, id: u64) -> Option<&InstanceItemArc>
    {
        for instance in self.instances.get_ref()
        {
            if instance.read().unwrap().id == id
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
            instance.read().unwrap().id != id
        });

        self.instances.get_ref().len() != len
    }

    pub fn clear_instances(&mut self)
    {
        self.instances.get_mut().clear();
    }

    pub fn delete_node_by_id(&mut self, id: u64) -> bool
    {
        let len = self.nodes.len();
        self.nodes.retain(|node|
        {
            node.read().unwrap().id != id
        });

        if self.nodes.len() != len
        {
            return true;
        }

        // if not found -> check children
        for node in &self.nodes
        {
            let deleted = node.write().unwrap().delete_node_by_id(id);

            if deleted
            {
                return true;
            }
        }

        false
    }

    pub fn find_root_node(node: NodeItem) -> Option<NodeItem>
    {
        if node.read().unwrap().root_node
        {
            return Some(node);
        }

        if let Some(parent) = &node.read().unwrap().parent
        {
            return Self::find_root_node(parent.clone());
        }

        None
    }

    pub fn print(&self, level: usize)
    {
        let spaces = " ".repeat(level * 2);
        println!("{} - (NODE) id={} name={} visible={} components={}, instances={}", spaces, self.id, self.name, self.visible, self.components.len(), self.instances.get_ref().len());

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

        let mesh = mesh.unwrap();
        component_downcast!(mesh, Mesh);
        let mesh_data = mesh.get_data();

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
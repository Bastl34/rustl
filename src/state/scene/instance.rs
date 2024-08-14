#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Matrix4, Vector3, Vector4};

use crate::{component_downcast, component_downcast_mut, input::input_manager::InputManager, helper::change_tracker::ChangeTracker};

use super::{components::{alpha::Alpha, component::{find_component, find_component_by_id, find_components, remove_component_by_id, remove_component_by_type, remove_components_by_ids, Component, ComponentItem}, joint::Joint, transformation::Transformation}, node::{InstanceItemArc, Node, NodeItem}};

pub type InstanceItem = Box<Instance>;


pub struct ComputedInstanceData
{
    pub world_matrix: Matrix4::<f32>,
    pub alpha: f32,
    pub locked: bool,
}

pub struct InstanceData
{
    pub computed: ComputedInstanceData,

    pub visible: bool,
    pub highlight: bool,
    pub collision: bool,
    pub locked: bool,
}


pub struct Instance
{
    pub id: u64,
    pub name: String,
    pub pickable: bool,

    pub node: NodeItem,
    pub components: Vec<ComponentItem>,

    force_update: bool,

    data: ChangeTracker<InstanceData>
}

impl Instance
{
    pub fn new(id: u64, name: String, node: NodeItem) -> Instance
    {
        let instance = Instance
        {
            id: id,
            name: name,
            pickable: true,

            node: node,
            components: vec![],

            force_update: true, // force update on creation

            data: ChangeTracker::new(InstanceData
            {
                computed: ComputedInstanceData
                {
                    world_matrix: Matrix4::<f32>::identity(),
                    alpha: 1.0,
                    locked: false
                },

                visible: true,
                highlight: false,
                collision: true,
                locked: false,
            })
        };

        instance
    }

    pub fn new_with_transform(id: u64, name: String, node: NodeItem, transform: Transformation) -> Instance
    {
        let mut instance = Instance
        {
            id: id,
            name: name,
            pickable: true,

            node: node,
            components: vec![],

            force_update: true, // force update on creation

            data: ChangeTracker::new(InstanceData
            {
                computed: ComputedInstanceData
                {
                    world_matrix: Matrix4::<f32>::identity(),
                    alpha: 1.0,
                    locked: false
                },

                visible: true,
                highlight: false,
                collision: true,
                locked: false,
            })
        };

        instance.add_component(Arc::new(RwLock::new(Box::new(transform))));

        instance
    }

    pub fn get_data(&self) -> &InstanceData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<InstanceData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<InstanceData>
    {
        &mut self.data
    }

    pub fn set_force_update(&mut self)
    {
        self.force_update = true;
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
            self.force_update = true;
        }
    }

    pub fn remove_component_by_id(&mut self, id: u64)
    {
        if remove_component_by_id(&mut self.components, id)
        {
            self.force_update = true;
        }
    }

    pub fn remove_components_by_ids(&mut self, ids: &Vec<u64>)
    {
        if remove_components_by_ids(&mut self.components, &ids)
        {
            self.force_update = true;
        }
    }

    pub fn update(instance: &InstanceItemArc, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64) -> bool
    {
        let node;
        {
            let instance = instance.read().unwrap();
            node = instance.node.clone();
        }

        // ***** copy all components *****
        let all_components;
        {
            let instance = instance.read().unwrap();
            all_components = instance.components.clone();
        }

        let mut delete_components = vec![];

        for (component_id, component) in all_components.clone().iter_mut().enumerate()
        {
            if component.read().unwrap().get_base().delete_later_request
            {
                delete_components.push(component.read().unwrap().id());
            }

            if !component.read().unwrap().is_enabled()
            {
                continue;
            }

            // remove the component itself  for the component update
            {
                let mut instance = instance.write().unwrap();
                instance.components = all_components.clone();
                instance.components.remove(component_id);
            }

            let mut component_write = component.write().unwrap();
            component_write.update_instance(node.clone(), instance, input_manager, time, frame_scale, frame);
        }

        // ***** reassign components *****
        {
            let mut instance = instance.write().unwrap();
            instance.components = all_components;
        }

        // ***** delete components *****
        {
            let mut instance = instance.write().unwrap();
            instance.remove_components_by_ids(&delete_components);
        }

        // ***** update computed data *****
        let has_changed_data;
        {
            let instance = instance.read().unwrap();
            has_changed_data = instance.find_changed_data() || instance.force_update;
        }

        if has_changed_data
        {
            let mut instance = instance.write().unwrap();

            let world_matrix = instance.calculate_transform();
            let alpha = instance.calculate_alpha();
            let locked = instance.accumulate_locked();

            let data = instance.get_data_mut().get_mut();
            data.computed.world_matrix = world_matrix;
            data.computed.alpha = alpha;
            data.computed.locked = locked;
        }

        {
            let mut instance = instance.write().unwrap();
            instance.force_update = false;
        }

        has_changed_data
    }

    pub fn find_changed_data(&self) -> bool
    {
        let mut changed = false;

        // ********** check self **********
        // transformation check
        let trans_component = self.find_component::<Transformation>();
        if let Some(trans_component) = trans_component
        {
            component_downcast_mut!(trans_component, Transformation);
            if trans_component.get_data_mut().consume_change()
            {
                changed = true;
            }
        }

        // alpha check
        let alpha_components = self.find_components::<Alpha>();
        for alpha_component in alpha_components
        {
            component_downcast_mut!(alpha_component, Alpha);

            if alpha_component.get_data_mut().consume_change()
            {
                changed = true;
            }
        }

        // locked check
        if self.accumulate_locked() != self.get_data().computed.locked
        {
            changed = true;
        }

        // ********** check node and parents **********
        changed = Instance::find_changed_parent_data(self.node.clone()) || changed;

        changed
    }

    pub fn find_changed_parent_data(node: Arc<RwLock<Box<Node>>>) -> bool
    {
        let node_read = node.read().unwrap();

        // check transformation
        let trans_component = node_read.find_component::<Transformation>();
        if let Some(trans_component) = trans_component
        {
            component_downcast!(trans_component, Transformation);
            if trans_component.get_data_tracker().changed()
            {
                return true;
            }
        }

        // check joints
        let joint_component = node_read.find_component::<Joint>();
        if let Some(joint_component) = joint_component
        {
            component_downcast!(joint_component, Joint);
            if joint_component.get_data_tracker().changed()
            {
                return true;
            }
        }

        // check alpha
        let alpha_components = node_read.find_components::<Alpha>();
        for alpha_component in alpha_components
        {
            component_downcast!(alpha_component, Alpha);

            if alpha_component.get_data_tracker().changed()
            {
                return true;
            }
        }

        if let Some(parent) = &node_read.parent
        {
            return Instance::find_changed_parent_data(parent.clone());
        }

        false
    }

    pub fn calculate_transform(&self) -> Matrix4::<f32>
    {
        let node_trans;
        {
            let node = self.node.read().unwrap();
            node_trans = node.get_full_transform();
        }
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            let instance_trans = transform_component.get_transform();

            if transform_component.has_parent_inheritance()
            {
                node_trans * instance_trans
            }
            else
            {
                *instance_trans
            }
        }
        else
        {
            node_trans
        }
    }

    pub fn calculate_inverse_transform(&self) -> Matrix4<f32>
    {
        let full_transform = self.calculate_transform();

        full_transform.try_inverse().unwrap()
    }

    pub fn transform_global_to_local(&self, vec: &Vector4<f32>) -> Vector4<f32>
    {
        let trans = self.calculate_inverse_transform();

        trans * vec
    }

    pub fn transform_local_to_global(&self, vec: &Vector4<f32>) -> Vector4<f32>
    {
        let trans = self.calculate_transform();

        trans * vec
    }

    pub fn transform_from_instance_to_local(&self, vec: &Vector4<f32>, instance: Arc<RwLock<Box<Instance>>>) -> Vector4<f32>
    {
        let instance = instance.read().unwrap();
        let global_vec = instance.transform_local_to_global(vec);

        self.transform_global_to_local(&global_vec)
    }

    pub fn calculate_alpha(&self) -> f32
    {
        let node_alpha = Node::get_full_alpha(self.node.clone());

        let alpha_components = self.find_components::<Alpha>();

        if alpha_components.len() == 0
        {
            return node_alpha;
        }

        let mut alpha = 1.0;
        let mut inheritance = true;
        for alpha_component in alpha_components
        {
            component_downcast!(alpha_component, Alpha);

            inheritance = alpha_component.has_alpha_inheritance();
            alpha *= alpha_component.get_alpha();
        }

        if inheritance
        {
            alpha * node_alpha
        }
        else
        {
            alpha
        }
    }

    pub fn accumulate_locked(&self) -> bool
    {
        if self.get_data().locked
        {
            return true;
        }

        let node = self.node.read().unwrap();
        node.is_locked()
    }

    pub fn get_cached_world_transform(&self) -> Matrix4::<f32>
    {
        self.get_data().computed.world_matrix
    }

    pub fn get_cached_alpha(&self) -> f32
    {
        if self.get_data().visible == false
        {
            return 0.0;
        }

        self.get_data().computed.alpha
    }

    pub fn get_cached_is_locked(&self) -> bool
    {
        if self.get_data().locked
        {
            return true;
        }

        self.get_data().computed.locked
    }

    pub fn apply_transform(&mut self, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            transform_component.apply_transformation(Some(position), Some(scale), Some(rotation));
        }
        else
        {
            panic!("trnasform component not found");
        }
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            transform_component.apply_translation(translation);
        }
        else
        {
            panic!("trnasform component not found");
        }
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            transform_component.apply_scale(scale, true);
        }
        else
        {
            panic!("trnasform component not found");
        }
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            transform_component.apply_rotation(rotation);
        }
        else
        {
            panic!("trnasform component not found");
        }
    }
}
use std::sync::{Arc, RwLock};

use nalgebra::{Matrix3, Matrix4, Vector3};

use crate::{component_downcast, component_downcast_mut, input::input_manager::InputManager};

use super::{node::{NodeItem, Node, InstanceItemChangeTracker}, components::{transformation::{Transformation}, alpha::Alpha, component::{ComponentItem, find_component, Component, find_components, remove_component_by_type, remove_component_by_id}}};

pub type InstanceItem = Box<Instance>;

pub struct Instance
{
    pub id: u64,
    pub name: String,

    node: NodeItem,
    pub components: Vec<ComponentItem>,

    pub visible: bool,
    pub highlight: bool
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
            components: vec![],
            visible: true,
            highlight: false
        };

        instance
    }

    pub fn new_with_transform(id: u64, name: String, node: NodeItem, transform: Transformation) -> Instance
    {
        let mut instance = Instance
        {
            id: id,
            name: name,
            node: node,
            components: vec![],
            visible: true,
            highlight: false
        };

        instance.add_component(Arc::new(RwLock::new(Box::new(transform))));

        instance
    }

    /*
    pub fn new_with_data(id: u64, name: String, node: NodeItem, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Instance
    {
        let instance = Instance
        {
            id: id,
            name: name,
            node: node,
            transform: Transformation::new(0, position, rotation, scale),
            alpha: Alpha::new(0, 1.0),
            visible: true,
            highlight: false
        };

        instance
    }
    */

    pub fn add_component(&mut self, component: ComponentItem)
    {
        self.components.push(component);
    }

    pub fn find_component<T>(&self) -> Option<ComponentItem> where T: 'static
    {
        find_component::<T>(&self.components)
    }

    pub fn find_components<T: Component>(&self) -> Vec<ComponentItem> where T: 'static
    {
        find_components::<T>(&self.components)
    }

    pub fn remove_component_by_type<T>(&mut self) where T: 'static
    {
        remove_component_by_type::<T>(&mut self.components)
    }

    pub fn remove_component_by_id(&mut self, id: u64)
    {
        remove_component_by_id(&mut self.components, id)
    }

    pub fn update(node: NodeItem, instance: &InstanceItemChangeTracker, input_manager: &mut InputManager, frame_scale: f32)
    {
        // ***** copy all components *****
        let all_components;
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();
            all_components = instance.components.clone();
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
                let mut instance: std::cell::RefMut<'_, crate::helper::change_tracker::ChangeTracker<Box<Instance>>> = instance.borrow_mut();
                let instance = instance.get_unmarked_mut();
                instance.components = all_components.clone();
                instance.components.remove(component_id);
            }

            let mut component_write = component.write().unwrap();
            component_write.update_instance(node.clone(), instance, input_manager, frame_scale);
        }

        // ***** reassign components *****
        {
            let mut instance: std::cell::RefMut<'_, crate::helper::change_tracker::ChangeTracker<Box<Instance>>> = instance.borrow_mut();
            let instance = instance.get_unmarked_mut();
            instance.components = all_components;
        }
    }

    pub fn get_transform(&self) -> (Matrix4::<f32>, Matrix3::<f32>)
    {
        let (node_trans, node_normal) = Node::get_full_transform(self.node.clone());
        let transform_component = self.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            let instance_trans = transform_component.get_transform();
            let instance_normal = transform_component.get_normal_matrix();

            if transform_component.has_parent_inheritance()
            {
                (
                    node_trans * instance_trans,
                    node_normal * instance_normal,
                )
            }
            else
            {
                (
                    *instance_trans,
                    *instance_normal,
                )
            }
        }
        else
        {
            (
                node_trans,
                node_normal,
            )
        }
    }

    pub fn get_alpha(&self) -> f32
    {
        if self.visible == false
        {
            return 0.0;
        }

        let node_alpha = Node::get_full_alpha(self.node.clone());

        let alpha_component = self.find_component::<Alpha>();

        if let Some(alpha_component) = alpha_component
        {
            component_downcast!(alpha_component, Alpha);

            if alpha_component.has_alpha_inheritance()
            {
                alpha_component.get_alpha() * node_alpha
            }
            else
            {
                alpha_component.get_alpha()
            }
        }
        else
        {
            node_alpha
        }
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
            panic!("trnasform component nout found");
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
            panic!("trnasform component nout found");
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
            panic!("trnasform component nout found");
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
            panic!("trnasform component nout found");
        }
    }
}
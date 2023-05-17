use std::sync::RwLockReadGuard;

use crate::{find_shared_component_mut, shared_component_downcast_mut};

use super::components::component::{ComponentItem, SharedComponentItem, Component};

pub type NodeItem = Box<Node>;

pub struct Node
{
    id: u32,
    name: String,

    pub nodes: Vec<NodeItem>,

    components: Vec<ComponentItem>,
    shared_components: Vec<SharedComponentItem>
}

impl Node
{
    pub fn new(id: u32, name: &str) -> Node
    {
        Self
        {
            id: id,
            name: name.to_string(),
            components: vec![],
            shared_components: vec![],

            nodes: vec![]
        }
    }

    pub fn add_component(&mut self, component: ComponentItem)
    {
        self.components.push(component);
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

    pub fn find_shared_component<'a, T>(&'a self) -> Option<Box<&'a T>> where T: 'static
    {
        let value = self.shared_components.iter().find
        (
            |c|
            {
                let component_item = c.read().unwrap();
                let component_item = component_item.as_any();
                component_item.is::<T>()
            }
        );
        if !value.is_some()
        {
            return None;
        }

        /*
        let any = value.unwrap().read().unwrap().as_any();
        let downcast: Box<&'a T> = Box::new(any.downcast_ref::<T>().unwrap());

        Some(downcast)
        */

        let read: RwLockReadGuard<'a, Box<dyn Component + Send + Sync>> = value.unwrap().read().unwrap();
        let any = read.as_any();
        let downcast: &T = any.downcast_ref::<T>().unwrap();
        let boxed_downcast: Box<&'a T> = Box::new(downcast);

        Some(boxed_downcast)
    }

    pub fn add_shared_component(&mut self, component: SharedComponentItem)
    {
        self.shared_components.push(component);
    }

    pub fn update(&mut self, time_delta: f32)
    {
        // update components
        for component in &mut self.components
        {
            component.update(time_delta);
        }

        for component in &mut self.shared_components
        {
            let mut component_write = component.write().unwrap();
            component_write.update(time_delta);
        }

        // update nodes
        for node in &mut self.nodes
        {
            node.update(time_delta);
        }
    }
}
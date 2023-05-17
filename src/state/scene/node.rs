use std::{sync::{RwLockReadGuard, Arc, RwLock}, any::Any};

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

#[macro_export]
macro_rules! get_component_mut
{
    ($node:ident, $component:ty, $result:ident) =>
    {
        let $result = $node.find_shared_component_mut::<$component>().unwrap();
        let mut writable = $result.write().unwrap();
        let $result = writable.as_any_mut().downcast_mut::<$component>().unwrap();
    };
}

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
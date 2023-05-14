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
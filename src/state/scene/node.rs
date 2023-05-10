use super::components::component::{ComponentItem};

pub type NodeItem = Box<Node>;

pub struct Node
{
    id: u32,
    name: String,

    components: Vec<ComponentItem>
}

impl Node
{
    pub fn new(id: u32, name: &str, components: Option<Vec<ComponentItem>>) -> Node
    {
        let mut component_items = vec![];
        if components.is_some()
        {
            component_items = components.unwrap();
        }

        Self
        {
            id: id,
            name: name.to_string(),
            components: component_items
        }
    }

    pub fn add_component(&mut self, component: ComponentItem)
    {
        self.components.push(component);
    }

    pub fn update(&mut self, time_delta: f32)
    {
        for component in &mut self.components
        {
            component.update(time_delta);
        }
    }
}
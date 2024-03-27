use std::sync::{RwLock, Arc};
use std::any::Any;

use crate::input::input_manager::InputManager;
use crate::state::helper::render_item::RenderItemOption;
use crate::state::scene::node::{NodeItem, InstanceItemArc};

pub type ComponentBox = Box<dyn Component + Send + Sync>;
pub type ComponentItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;

pub trait Component: Any
{
    fn get_base(&self) -> &ComponentBase;
    fn get_base_mut(&mut self) -> &mut ComponentBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>);

    fn update(&mut self, node: NodeItem, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64);
    fn update_instance(&mut self, node: NodeItem, instance: &InstanceItemArc, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64);

    fn set_enabled(&mut self, state: bool);

    fn instantiable(&self) -> bool;

    fn id(&self) -> u64
    {
        self.get_base().id
    }

    fn is_enabled(&self) -> bool
    {
        self.get_base().is_enabled
    }

    fn component_name(&self) -> &str
    {
        self.get_base().name.as_str()
    }
}

pub struct ComponentBase
{
    pub id: u64,
    pub is_enabled: bool,
    pub name: String,
    pub component_name: String,
    pub icon: String,
    pub info: Option<String>,

    pub post_update_request: bool,

    pub render_item: RenderItemOption
}

impl ComponentBase
{
    pub fn new(id: u64, name: String, component_name: String, icon: String) -> ComponentBase
    {
        ComponentBase
        {
            id,
            name,
            component_name,
            icon,
            is_enabled: true,
            render_item: None,
            info: None,
            post_update_request: false
        }
    }
}

// ******************** default implementations ********************

//https://stackoverflow.com/questions/65380698/trait-with-default-implementation-and-required-struct-member
#[macro_export]
macro_rules! component_impl_default
{
    () =>
    {
        fn as_any(&self) -> &dyn std::any::Any
        {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any
        {
            self
        }

        fn get_base(&self) -> &ComponentBase
        {
            &self.base
        }

        fn get_base_mut(&mut self) -> &mut ComponentBase
        {
            &mut self.base
        }
    };
}

#[macro_export]
macro_rules! component_impl_no_update
{
    () =>
    {
        fn update(&mut self, _node: NodeItem, _input_manager: &mut crate::input::input_manager::InputManager, _time: u128, _frame_scale: f32, _frame: u64)
        {
        }

        fn update_instance(&mut self, _node: NodeItem, _instance: &crate::state::scene::node::InstanceItemArc, _input_manager: &mut crate::input::input_manager::InputManager, _time: u128, _frame_scale: f32, _frame: u64)
        {
        }
    };
}

#[macro_export]
macro_rules! component_impl_no_update_instance
{
    () =>
    {
        fn update_instance(&mut self, _node: NodeItem, _instance: &crate::state::scene::node::InstanceItemArc, _input_manager: &mut crate::input::input_manager::InputManager, _time: u128, _frame_scale: f32, _frame: u64)
        {
        }
    };
}

#[macro_export]
macro_rules! component_impl_set_enabled
{
    () =>
    {
        fn set_enabled(&mut self, state: bool)
        {
            self.get_base_mut().is_enabled = state;
        }
    };
}

// ******************** helper ********************

pub fn find_component<T>(components: &Vec<ComponentItem>) -> Option<ComponentItem> where T: 'static
{
    if components.len() == 0
    {
        return None;
    }

    let value = components.iter().find
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

pub fn find_component_by_id(components: &Vec<ComponentItem>, id: u64) -> Option<ComponentItem>
{
    if components.len() == 0
    {
        return None;
    }

    let value = components.iter().find
    (
        |c|
        {
            let component = c.read().unwrap();
            component.id() == id
        }
    );

    if !value.is_some()
    {
        return None;
    }

    Some(value.unwrap().clone())
}

pub fn find_components<T: Component>(components: &Vec<ComponentItem>) -> Vec<ComponentItem> where T: 'static
{
    if components.len() == 0
    {
        return vec![];
    }

    let values: Vec<_> = components.iter().filter
    (
        |c|
        {
            let component = c.read().unwrap();
            let component_item = component.as_any();
            component_item.is::<T>()
        }
    ).collect();

    if values.len() == 0
    {
        return vec![];
    }

    values.iter().map(|component| Arc::clone(component)).collect()
}

pub fn remove_component_by_type<T>(components: &mut Vec<ComponentItem>) -> bool where T: 'static
{
    let index = components.iter().position
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
        components.remove(index);
        return true;
    }

    false
}

pub fn remove_component_by_id(components: &mut Vec<ComponentItem>, id: u64) -> bool
{
    let index = components.iter().position
    (
        |c|
        {
            let component = c.read().unwrap();
            component.id() == id
        }
    );

    if let Some(index) = index
    {
        components.remove(index);
        return true;
    }

    false
}

// ******************** macros ********************

#[macro_export]
macro_rules! new_component
{
    ($component:expr) =>
    {
        {
            Arc::new(RwLock::new(Box::new($component)))
        }
    };
}

#[macro_export]
macro_rules! component_downcast
{
    ($component:ident, $type:ty) =>
    {
        let read = $component.read().unwrap();
        let $component = read.as_any().downcast_ref::<$type>().unwrap();
    };
}

#[macro_export]
macro_rules! component_downcast_mut
{
    ($component:ident, $type:ty) =>
    {
        let mut write = $component.write().unwrap();
        let $component = write.as_any_mut().downcast_mut::<$type>().unwrap();
    };
}
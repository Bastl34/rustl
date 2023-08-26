use std::sync::{RwLock, Arc};
use std::any::Any;

use crate::state::helper::render_item::RenderItemOption;
use crate::state::scene::node::NodeItem;

pub type ComponentBox = Box<dyn Component + Send + Sync>;
pub type ComponentItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;

pub trait Component: Any
{
    fn get_base(&self) -> &ComponentBase;
    fn get_base_mut(&mut self) -> &mut ComponentBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn update(&mut self, node: NodeItem, frame_scale: f32);

    fn ui(&mut self, ui: &mut egui::Ui);

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
            render_item: None
        }
    }
}


//https://stackoverflow.com/questions/65380698/trait-with-default-implementation-and-required-struct-member
#[macro_export]
macro_rules! component_impl_default
{
    () =>
    {
        fn as_any(&self) -> &dyn Any
        {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any
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
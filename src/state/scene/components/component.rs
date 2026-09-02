#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::{RwLock, Arc};
use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::state::helper::render_item::RenderItemOption;
use crate::state::resources::mesh_resource::MeshResourceItem;
use crate::state::scene::manager::id_manager;
use crate::state::scene::node::{NodeItem, InstanceItemArc};
use crate::state::scene::utilities::extras::Extras;
use crate::state::scene::utilities::tags::Tags;
use crate::state::state::InputOutput;

pub type ComponentBox = Box<dyn Component>;
pub type ComponentItem = Arc<RwLock<Box<dyn Component>>>;

#[typetag::serde(tag = "type")]
pub trait Component: Any + Send + Sync
{
    fn get_base(&self) -> &ComponentBase;
    fn get_base_mut(&mut self) -> &mut ComponentBase;

    fn get_extras(&self) -> &Extras;
    fn get_extras_mut(&mut self) -> &mut Extras;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn is_serializable(&self) -> bool { true }
    fn run_after_deserialize(&mut self, context: &mut DeserializationContext);

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>);

    fn update(&mut self, node: NodeItem, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64);
    fn update_instance(&mut self, node: Option<NodeItem>, instance: &InstanceItemArc, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64);

    fn duplicate(&self) -> Option<ComponentItem>;
    fn cleanup_node(&mut self, node: NodeItem) -> bool; // node was deleted and should be removed from component

    fn set_enabled(&mut self, state: bool);

    fn instantiable() -> bool
    where
        Self: Sized;

    fn duplicatable(&self) -> bool;

    fn id(&self) -> u32
    {
        self.get_base().id
    }

    fn uuid(&self) -> &String
    {
        &self.get_base().uuid
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

pub struct DeserializationContext<'a>
{
    // resources
    pub textures: Vec<crate::state::resources::texture::TextureItem>,
    pub mesh_resources: Vec<MeshResourceItem>,
    pub sound_sources: Vec<crate::state::resources::sound_source::SoundSourceItem>,

    // scene
    pub scene: &'a mut crate::state::scene::scene::Scene,
    pub nodes: Vec<NodeItem>,
    pub instances: Vec<InstanceItemArc>,
    pub components: Vec<ComponentItem>,

    // io
    pub io: &'a mut InputOutput,
}

#[derive(Serialize, Deserialize)]
pub struct ComponentBase
{
    #[serde(skip, default)]
    pub id: u32,
    pub uuid: String,

    pub is_enabled: bool,

    pub name: String,

    #[serde(skip, default)]
    pub component_name: String,

    #[serde(skip, default)]
    pub icon: String,

    pub info: Option<String>,

    pub extras: Extras,
    pub tags: Tags,

    pub from_file: bool,

    #[serde(skip, default)]
    pub delete_later_request: bool,

    #[serde(skip, default)]
    pub render_item: RenderItemOption,

    pub export: bool
}

impl ComponentBase
{
    pub fn new(name: String, component_name: String, icon: String) -> ComponentBase
    {
        ComponentBase
        {
            id: id_manager::get_next_component_id(),
            uuid: uuid::Uuid::new_v4().to_string(),
            is_enabled: true,

            name,
            component_name,
            icon,
            info: None,

            extras: Extras::new(),
            tags: Tags::new(),

            from_file: false,

            delete_later_request: false,

            render_item: None,

            export: true
        }
    }

    pub fn duplicate(from: &ComponentBase) -> ComponentBase
    {
        ComponentBase
        {
            id: id_manager::get_next_component_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            is_enabled: from.is_enabled,

            name: from.name.clone(),
            component_name: from.component_name.clone(),
            icon: from.icon.clone(),
            info: from.info.clone(),

            extras: from.extras.clone(),
            tags: from.tags.clone(),

            from_file: false,

            delete_later_request: false,

            render_item: None,

            export: from.export
        }
    }

    pub fn delete_later(&mut self)
    {
        self.delete_later_request = true;
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

        fn get_extras(&self) -> &crate::state::scene::utilities::extras::Extras
        {
            &self.base.extras
        }

        fn get_extras_mut(&mut self) -> &mut crate::state::scene::utilities::extras::Extras
        {
            &mut self.base.extras
        }
    };
}

#[macro_export]
macro_rules! component_impl_no_update
{
    () =>
    {
        fn update(&mut self, _node: NodeItem, _io: &mut crate::state::state::InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
        {
        }

        fn update_instance(&mut self, _node: Option<NodeItem>, _instance: &crate::state::scene::node::InstanceItemArc, _io: &mut crate::state::state::InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
        {
        }
    };
}

#[macro_export]
macro_rules! component_impl_no_update_instance
{
    () =>
    {
        fn update_instance(&mut self, _node: Option<NodeItem>, _instance: &crate::state::scene::node::InstanceItemArc, _io: &mut crate::state::state::InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
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

#[macro_export]
macro_rules! component_impl_no_cleanup_node
{
    () =>
    {
        fn cleanup_node(&mut self, _node: NodeItem) -> bool
        {
            false
        }
    };
}

#[macro_export]
macro_rules! component_impl_no_post_deserialization
{
    () =>
    {
        fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
        {

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

pub fn find_component_by_id(components: &Vec<ComponentItem>, id: u32) -> Option<ComponentItem>
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

pub fn remove_components_by_type<T>(components: &mut Vec<ComponentItem>) -> bool where T: 'static
{
    let prev_len = components.len();
    components.retain
    (
        |c|
        {
            let component = c.read().unwrap();
            let component_item = component.as_any();
            !component_item.is::<T>()
        }
    );

    components.len() != prev_len
}

pub fn remove_component_by_id(components: &mut Vec<ComponentItem>, id: u32) -> bool
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

pub fn remove_components_by_ids(components: &mut Vec<ComponentItem>, ids: &Vec<u32>) -> bool
{
    let set: HashSet<u32> = ids.iter().cloned().collect();
    let prev_len = components.len();

    components.retain(|component|
    {
        let component = component.read().unwrap();
        !set.contains(&component.id())
    });

    components.len() != prev_len
}

pub fn find_new_components_with_position(old_list: &Vec<ComponentItem>, new_list: &Vec<ComponentItem>) -> Vec<(ComponentItem, bool)>
{
    let old_ids: Vec<u32> = old_list.iter()
        .map(|c| c.read().unwrap().id())
        .collect();

    let mut result = vec![];

    for (index, c) in new_list.iter().enumerate()
    {
        let id = c.read().unwrap().id();
        if !old_ids.contains(&id)
        {
            // Determine if the component was added at the front or back
            // Simple heuristic: if index < old_list.len() / 2 => front, else back
            let add_to_front = index < old_list.len() / 2;
            result.push((c.clone(), add_to_front));
        }
    }

    result
}

pub fn find_and_add_new_components(components_target: &mut Vec<ComponentItem>, maybe_new_components: &Vec<ComponentItem>)
{
    // after each update, check if new components were added during the update --> add
    let new_components_with_position = find_new_components_with_position(&components_target, maybe_new_components);
    for (component, add_to_front) in new_components_with_position
    {
        if add_to_front
        {
            components_target.insert(0, component);
        }
        else
        {
            components_target.push(component);
        }
    }
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
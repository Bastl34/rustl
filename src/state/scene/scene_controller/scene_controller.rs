#![allow(dead_code)]

use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::{state::scene::node::NodeItem, input::input_manager::InputManager};

pub type SceneControllerBox = Box<dyn SceneController + Send + Sync>;

pub trait SceneController: Any
{
    fn get_base(&self) -> &SceneControllerBase;
    fn get_base_mut(&mut self) -> &mut SceneControllerBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn serializable(&self) -> bool;
    fn deserializable(&self) -> bool;

    fn init_after_deserialize(&mut self, scene: &mut crate::state::scene::scene::Scene);

    fn cleanup(&mut self);
    fn cleanup_node(&mut self, node: NodeItem) -> bool; // node was deleted and should be removed from component

    fn ui(&mut self, ui: &mut egui::Ui, scene: &mut crate::state::scene::scene::Scene);

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, input_manager: &mut InputManager, frame_scale: f32) -> bool;
}

#[derive(Serialize, Deserialize)]
pub struct SceneControllerBase
{
    pub is_enabled: bool,
    pub name: String,
    pub icon: String,
}

impl SceneControllerBase
{
    pub fn new(name: String, icon: String) -> SceneControllerBase
    {
        SceneControllerBase
        {
            name,
            icon,
            is_enabled: true
        }
    }
}

// ******************** default implementations ********************

#[macro_export]
macro_rules! scene_controller_impl_default
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

        fn get_base(&self) -> &SceneControllerBase
        {
            &self.base
        }

        fn get_base_mut(&mut self) -> &mut SceneControllerBase
        {
            &mut self.base
        }
    };
}

#[macro_export]
macro_rules! scene_controller_impl_no_serialization
{
    () =>
    {
        fn serializable(&self) -> bool
        {
            false
        }

        fn deserializable(&self) -> bool
        {
            false
        }

        fn init_after_deserialize(&mut self, scene: &mut crate::state::scene::scene::Scene)
        {

        }
    };
}
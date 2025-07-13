#![allow(dead_code)]

use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::{state::{scene::node::NodeItem, state::InputOutput}};

pub type SceneControllerBox = Box<dyn SceneController>;

#[typetag::serde(tag = "type")]
pub trait SceneController: Any + Send + Sync
{
    fn get_base(&self) -> &SceneControllerBase;
    fn get_base_mut(&mut self) -> &mut SceneControllerBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn is_serializable(&self) -> bool { true }

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext);

    fn cleanup(&mut self);
    fn cleanup_node(&mut self, node: NodeItem) -> bool; // node was deleted and should be removed from component

    fn ui(&mut self, ui: &mut egui::Ui, scene: &mut crate::state::scene::scene::Scene);

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, io: &mut InputOutput, frame_scale: f32) -> bool;
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
        fn is_serializable(&self) -> bool
        {
            false
        }

        fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
        {

        }
    };
}
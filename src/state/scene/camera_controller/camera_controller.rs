use std::any::Any;

use crate::{state::scene::node::NodeItem, input::input_manager::InputManager};

pub type CameraControllerBox = Box<dyn CameraController + Send + Sync>;

pub trait CameraController: Any
{
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn ui(&mut self, ui: &mut egui::Ui);

    fn update(&mut self, node: Option<NodeItem>, scene: &mut crate::state::scene::scene::Scene, input_manager: &mut InputManager, cam_data: &mut crate::helper::change_tracker::ChangeTracker<crate::state::scene::camera::CameraData>, frame_scale: f32);
}

// ******************** default implementations ********************

#[macro_export]
macro_rules! camera_controller_impl_default
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
    };
}
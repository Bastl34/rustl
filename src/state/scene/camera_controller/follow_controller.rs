use std::f32::consts::PI;

use nalgebra::{Point3, Vector2, Vector3};

use crate::{camera_controller_impl_default, component_downcast, helper::{change_tracker::ChangeTracker, math::{self, approx_zero_vec2}}, input::{input_manager::InputManager, keyboard::{Key, Modifier}}, state::scene::{camera::CameraData, components::transformation::Transformation, node::NodeItem, scene::Scene}};

use super::camera_controller::{CameraController, CameraControllerBase};

pub struct FollowControllerControllerData
{
    pub offset: Vector3::<f32>,
}

pub struct FollowController
{
    base: CameraControllerBase,

    pub data: ChangeTracker<FollowControllerControllerData>,
}

impl FollowController
{
    pub fn new() -> FollowController
    {
        FollowController
        {
            base: CameraControllerBase::new("Follow Controller".to_string(), "👣".to_string()),

            data: ChangeTracker::new(FollowControllerControllerData
            {
                offset: Vector3::<f32>::zeros()
            }),
        }
    }
}

impl CameraController for FollowController
{
    camera_controller_impl_default!();

    fn update(&mut self, node: Option<NodeItem>, _scene: &mut Scene, _input_manager: &mut InputManager, cam_data: &mut ChangeTracker<CameraData>, _frame_scale: f32) -> bool
    {
        let mut change = false;

        if let Some(node) = node
        {
            let node = node.read().unwrap();

            if let Some(transform_component) = node.find_component::<Transformation>()
            {
                component_downcast!(transform_component, Transformation);
                let transform_data = transform_component.get_data_tracker();

                if transform_data.changed()
                {
                    let transform_data = transform_data.get_ref();
                    let cam_data = cam_data.get_mut();

                    let dir = math::yaw_pitch_to_direction(transform_data.rotation.y, transform_data.rotation.x).normalize();

                    cam_data.eye_pos = Point3::<f32>::new(transform_data.position.x, transform_data.position.y, transform_data.position.z) + self.data.get_ref().offset;
                    cam_data.dir = dir;

                    change = true;
                }
            }
        }

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("Offset:");

            let mut offset = self.data.get_ref().offset;
            let mut changed = false;

            changed = ui.add(egui::DragValue::new(&mut offset.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut offset.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut offset.z).speed(0.1).prefix("z: ")).changed() || changed;

            if changed
            {
                self.data.get_mut().offset = offset;
            }
        });
    }
}
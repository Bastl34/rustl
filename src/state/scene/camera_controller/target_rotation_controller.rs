use std::{f32::consts::PI, ops::Mul};

use nalgebra::{Vector2, Vector3, Isometry, Isometry3, Point3};
use parry3d::{shape::Ball, query};

use crate::{camera_controller_impl_default, state::scene::{node::NodeItem, scene::Scene, camera::CameraData, components::mesh::Mesh}, input::{input_manager::InputManager, keyboard::{Key, Modifier}}, helper::{change_tracker::ChangeTracker, math::{approx_zero_vec2, self, approx_zero}}, component_downcast};

use super::camera_controller::{CameraController, CameraControllerBase};

const DEFAULT_TARGET_POS: Point3::<f32> = Point3::new(0.0, 0.0, 0.0);
const ANGLE_OFFSET: f32 = 0.01;

pub struct TargetRotationController
{
    base: CameraControllerBase,

    run_initial_update: bool,

    radius: f32,
    alpha: f32,
    beta: f32,

    mouse_sensitivity: Vector2::<f32>,
    mouse_wheel_sensitivity: f32,
}

impl TargetRotationController
{
    pub fn new(radius: f32, alpha: f32, beta: f32, mouse_sensitivity: Vector2::<f32>, mouse_wheel_sensitivity: f32) -> TargetRotationController
    {
        TargetRotationController
        {
            base: CameraControllerBase::new("Target Rotation Controller".to_string(), "⟲".to_string()),

            run_initial_update: true,

            radius,
            alpha,
            beta,

            mouse_sensitivity,
            mouse_wheel_sensitivity,
        }
    }
}

impl CameraController for TargetRotationController
{
    camera_controller_impl_default!();

    fn update(&mut self, node: Option<NodeItem>, scene: &mut Scene, input_manager: &mut InputManager, cam_data: &mut ChangeTracker<CameraData>, frame_scale: f32) -> bool
    {
        let mut change = false;

        let velocity = &input_manager.mouse.point.velocity;

        if self.run_initial_update || !math::approx_zero(input_manager.mouse.wheel_delta_y) || (input_manager.mouse.is_any_button_holding() && !approx_zero_vec2(*velocity))
        {
            let mut target_pos = DEFAULT_TARGET_POS;

            if let Some(node) = node
            {
                let node = node.read().unwrap();

                if let Some(center) = node.get_center(true)
                {
                    target_pos = center;
                }
            }

            let delta_x = velocity.x * self.mouse_sensitivity.x;
            let delta_y = velocity.y * self.mouse_sensitivity.y;

            self.alpha -= delta_x;
            self.beta -= delta_y;

            if self.beta > PI / 2.0 - ANGLE_OFFSET
            {
                self.beta = (PI / 2.0) - ANGLE_OFFSET;
            }
            else if self.beta < -PI / 2.0 + ANGLE_OFFSET
            {
                self.beta = -(PI / 2.0) + ANGLE_OFFSET;
            }

            let cam_data = cam_data.get_mut();

            self.radius += self.mouse_wheel_sensitivity * -input_manager.mouse.wheel_delta_y;
            let dir = math::yaw_pitch_to_direction(self.alpha, self.beta).normalize();

            cam_data.dir = -dir;
            let dir = dir * self.radius;
            cam_data.eye_pos = target_pos + dir;

            self.run_initial_update = false;
            change = true;
        }

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("Alpha (Longitude): ");
            let mut alpha = self.alpha.to_degrees();
            if ui.add(egui::DragValue::new(&mut alpha).speed(0.1).suffix("°")).changed()
            {
                self.alpha = alpha.to_radians();
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Beta (Latitude): ");
            let mut beta = self.beta.to_degrees();
            if ui.add(egui::DragValue::new(&mut beta).speed(0.1).suffix("°")).changed()
            {
                self.beta = beta.to_radians();
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Radius:");
            ui.add(egui::DragValue::new(&mut self.radius).speed(0.1));
        });

        ui.horizontal(|ui|
        {
            ui.label("Sensitivity (rad): ");
            ui.add(egui::DragValue::new(&mut self.mouse_sensitivity.x).speed(0.01).prefix("x: "));
            ui.add(egui::DragValue::new(&mut self.mouse_sensitivity.y).speed(0.01).prefix("y: "));
        });

        ui.horizontal(|ui|
        {
            ui.label("Mouse Wheel Sensitivity: ");
            ui.add(egui::DragValue::new(&mut self.mouse_wheel_sensitivity).speed(0.01));
        });
    }
}
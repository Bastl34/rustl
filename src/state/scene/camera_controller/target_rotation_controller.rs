use std::f32::consts::PI;

use nalgebra::{Vector2, Vector3, Point3};

use crate::{camera_controller_impl_default, state::scene::{node::NodeItem, scene::Scene, camera::CameraData}, input::{input_manager::InputManager, mouse::MouseButton}, helper::{change_tracker::ChangeTracker, math::{approx_zero_vec2, self}}};

use super::camera_controller::{CameraController, CameraControllerBase};

const DEFAULT_TARGET_POS: Point3::<f32> = Point3::new(0.0, 0.0, 0.0);
const ANGLE_OFFSET: f32 = 0.01;

pub struct TargetRotationControllerData
{
    pub offset: Vector3::<f32>,

    pub radius: f32,
    pub alpha: f32,
    pub beta: f32,
}

pub struct TargetRotationController
{
    base: CameraControllerBase,

    run_initial_update: bool,

    pub data: ChangeTracker<TargetRotationControllerData>,

    pub mouse_sensitivity: Vector2::<f32>,
    pub mouse_wheel_sensitivity: f32,
}

impl TargetRotationController
{
    pub fn new(radius: f32, alpha: f32, beta: f32, mouse_sensitivity: Vector2::<f32>, mouse_wheel_sensitivity: f32) -> TargetRotationController
    {
        TargetRotationController
        {
            base: CameraControllerBase::new("Target Rotation Controller".to_string(), "⟲".to_string()),

            run_initial_update: true,

            data: ChangeTracker::new(TargetRotationControllerData
            {
                offset: Vector3::<f32>::zeros(),

                radius,
                alpha,
                beta,
            }),

            mouse_sensitivity,
            mouse_wheel_sensitivity,
        }
    }
}

impl CameraController for TargetRotationController
{
    camera_controller_impl_default!();

    fn update(&mut self, node: Option<NodeItem>, _scene: &mut Scene, input_manager: &mut InputManager, cam_data: &mut ChangeTracker<CameraData>, _frame_scale: f32) -> bool
    {
        let mut change = false;

        let velocity = &input_manager.mouse.point.velocity;

        let mut update_needed = false;
        if let Some(node) = &node
        {
            update_needed = node.read().unwrap().has_changed_instance_data();
        }

        // offset
        if input_manager.mouse.is_holding(MouseButton::Right) && !approx_zero_vec2(*velocity)
        {
            let delta_x = velocity.x * self.mouse_sensitivity.x;
            let delta_y = velocity.y * self.mouse_sensitivity.y;

            let offset_movement = Vector3::<f32>::new(delta_x, delta_y, 0.0);

            let cam_inverse = &cam_data.get_ref().view_inverse;
            let transformed = cam_inverse * offset_movement.to_homogeneous();

            let data = self.data.get_mut();

            data.offset.x -= transformed.x;
            data.offset.y -= transformed.y;
            data.offset.z -= transformed.z;

            update_needed = true;
        }

        // rotation
        if input_manager.mouse.is_holding(MouseButton::Left) && !approx_zero_vec2(*velocity)
        {
            let delta_x = velocity.x * self.mouse_sensitivity.x;
            let delta_y = velocity.y * self.mouse_sensitivity.y;

            let data = self.data.get_mut();
            data.alpha -= delta_x;
            data.beta -= delta_y;

            if data.beta > PI / 2.0 - ANGLE_OFFSET
            {
                data.beta = (PI / 2.0) - ANGLE_OFFSET;
            }
            else if data.beta < -PI / 2.0 + ANGLE_OFFSET
            {
                data.beta = -(PI / 2.0) + ANGLE_OFFSET;
            }

            update_needed = true;
        }

        // distance
        if !math::approx_zero(input_manager.mouse.wheel_delta_y)
        {
            let data = self.data.get_mut();
            data.radius += self.mouse_wheel_sensitivity * -input_manager.mouse.wheel_delta_y;

            update_needed = true;
        }

        // apply
        let (data, controller_data_change) = self.data.consume_borrow();
        if self.run_initial_update || update_needed || controller_data_change
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

            let cam_data = cam_data.get_mut();

            let dir = math::yaw_pitch_to_direction(data.alpha, data.beta).normalize();

            cam_data.dir = -dir;
            let dir = dir * data.radius;
            cam_data.eye_pos = target_pos + data.offset + dir;

            self.run_initial_update = false;

            change = true;
        }

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("Alpha (Yaw/Longitude): ");
            let mut alpha = self.data.get_ref().alpha.to_degrees();
            if ui.add(egui::DragValue::new(&mut alpha).speed(0.1).suffix("°")).changed()
            {
                self.data.get_mut().alpha = alpha.to_radians();
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Beta (Pitch/Latitude): ");
            let mut beta = self.data.get_ref().beta.to_degrees();
            if ui.add(egui::DragValue::new(&mut beta).speed(0.1).suffix("°")).changed()
            {
                self.data.get_mut().beta = beta.to_radians();
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Radius:");
            let mut radius = self.data.get_ref().radius;
            if ui.add(egui::DragValue::new(&mut radius).speed(0.1)).changed()
            {
                self.data.get_mut().radius = radius;
            }
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
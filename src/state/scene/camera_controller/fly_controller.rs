use std::f32::consts::PI;

use nalgebra::{Vector2, Vector3};
use parry3d::{query::Ray, shape::Ball};

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, math::{self, approx_equal, approx_zero_vec2, approx_zero_vec3}}, input::{gamepad::{GamepadAxis, GamepadButton}, input_manager::InputManager, keyboard::{Key, Modifier}}, state::scene::{camera::CameraData, node::NodeItem, scene::Scene}};

use super::camera_controller::{CameraController, CameraControllerBase};

const ANGLE_OFFSET_UP: f32 = 0.01;
const ANGLE_OFFSET_DOWN: f32 = 0.1;

const DEFAULT_COLLISION_DISTANCE: f32 = 0.5;
const DEFAULT_MOUSE_SENSIVITY: f32 = 0.0015;
const DEFAULT_GAMEPAD_SENSIVITY: f32 = 0.03;

pub struct FlyController
{
    base: CameraControllerBase,

    pub mouse_movement: bool,
    pub keyboard_movement: bool,
    pub gamepad_movement: bool,

    collision: bool,
    collision_distance: f32,

    move_speed: f32,
    move_speed_shift: f32,
    mouse_sensitivity: Vector2::<f32>,
    gamepad_sensitivity: f32,
}

impl FlyController
{
    pub fn new(collision: bool, mouse_sensitivity: Vector2::<f32>, move_speed: f32, move_speed_shift: f32) -> FlyController
    {
        FlyController
        {
            base: CameraControllerBase::new("Fly Controller".to_string(), "✈".to_string()),

            mouse_movement: true,
            keyboard_movement: true,
            gamepad_movement: true,

            collision,
            collision_distance: DEFAULT_COLLISION_DISTANCE,

            move_speed,
            move_speed_shift,
            mouse_sensitivity,

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,
        }
    }

    pub fn default() -> Self
    {
        FlyController
        {
            base: CameraControllerBase::new("Fly Controller".to_string(), "✈".to_string()),

            mouse_movement: true,
            keyboard_movement: true,
            gamepad_movement: true,

            collision: false,
            collision_distance: DEFAULT_COLLISION_DISTANCE,

            move_speed: 0.1,
            move_speed_shift: 0.2,
            mouse_sensitivity: Vector2::<f32>::new(DEFAULT_MOUSE_SENSIVITY, DEFAULT_MOUSE_SENSIVITY),

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,
        }
    }
}

impl CameraController for FlyController
{
    camera_controller_impl_default!();

    fn update(&mut self, _node: Option<NodeItem>, scene: &mut Scene, input_manager: &mut InputManager, cam_data: &mut ChangeTracker<CameraData>, frame_scale: f32) -> bool
    {
        let mut change = false;
        let mut last_eye_pos = None;

        // ******************** angle/rotation ********************
        let mut angle_velocity = Vector2::<f32>::zeros();

        // mouse
        if self.mouse_movement
        {
            if
            (
                input_manager.mouse.is_any_button_holding() && *input_manager.mouse.visible.get_ref()
            )
            ||
                !*input_manager.mouse.visible.get_ref()
            {
                angle_velocity = input_manager.mouse.point.velocity;
                angle_velocity.x *= self.mouse_sensitivity.x;
                angle_velocity.y *= self.mouse_sensitivity.y;
            }
        }

        // gamepad
        if self.gamepad_movement
        {
            for (_, gamepad) in &mut input_manager.gamepads
            {
                if gamepad.is_axis_active(GamepadAxis::RightStickX)
                {
                    angle_velocity.x = gamepad.get_axis_value(GamepadAxis::RightStickX) * self.gamepad_sensitivity;
                }
                if gamepad.is_axis_active(GamepadAxis::RightStickY)
                {
                    angle_velocity.y = gamepad.get_axis_value(GamepadAxis::RightStickY) * self.gamepad_sensitivity;
                }
            }
        }

        // apply rotation
        if approx_zero_vec2(&angle_velocity) == false
        {
            let cam_data = cam_data.get_mut();

            let dir: Vector3::<f32> = cam_data.dir.normalize();

            let delta_x = angle_velocity.x;
            let delta_y = angle_velocity.y;

            let (mut yaw, mut pitch) = math::yaw_pitch_from_direction(dir);

            pitch += delta_y;
            yaw -= delta_x;

            // check that you can not look up/down to 90°
            if pitch > (PI/2.0) - ANGLE_OFFSET_UP
            {
                pitch = (PI/2.0) - ANGLE_OFFSET_UP;
            }
            else if pitch < (-PI/2.0) + ANGLE_OFFSET_DOWN
            {
                pitch = (-PI / 2.0) + ANGLE_OFFSET_DOWN;
            }

            let dir = math::yaw_pitch_to_direction(yaw, pitch);

            cam_data.dir = dir;

            change = true;
        }

        // ******************** movement ********************
        let mut movement = Vector3::<f32>::zeros();
        let mut fast_movement = false;

        // keyboard
        if self.keyboard_movement
        {
            let keys = vec![Key::W, Key::A, Key::S, Key::D, Key::Space, Key::C];
            if input_manager.keyboard.is_holding_by_keys(&keys) || input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl)
            {
                if input_manager.keyboard.is_holding(Key::W)
                {
                    movement.z = 1.0;
                }
                if input_manager.keyboard.is_holding(Key::S)
                {
                    movement.z = -1.0;
                }
                if input_manager.keyboard.is_holding(Key::D)
                {
                    movement.x = -1.0;
                }
                if input_manager.keyboard.is_holding(Key::A)
                {
                    movement.x = 1.0;
                }
                if input_manager.keyboard.is_holding(Key::Space)
                {
                    movement.y = 1.0;
                }
                //if input_manager.keyboard.is_holding(Key::C) || input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)
                if input_manager.keyboard.is_holding(Key::C)
                {
                    movement.y = -1.0;
                }
                if input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                {
                    fast_movement = true;
                }
            }
        }

        // gamepad
        if self.gamepad_movement
        {
            for (_, gamepad) in &mut input_manager.gamepads
            {
                if gamepad.is_holding(GamepadButton::DPadLeft)
                {
                    movement.x = 1.0;
                }
                if gamepad.is_holding(GamepadButton::DPadRight)
                {
                    movement.x = -1.0;
                }
                if gamepad.is_holding(GamepadButton::DPadUp)
                {
                    movement.z = 1.0;
                }
                if gamepad.is_holding(GamepadButton::DPadDown)
                {
                    movement.z = -1.0;
                }
                if gamepad.is_axis_active(GamepadAxis::LeftStickX)
                {
                    movement.x -= gamepad.get_axis_value(GamepadAxis::LeftStickX);
                }
                if gamepad.is_axis_active(GamepadAxis::LeftStickY)
                {
                    movement.z = gamepad.get_axis_value(GamepadAxis::LeftStickY);
                }
                if gamepad.is_holding(GamepadButton::South)
                {
                    movement.y = 1.0;
                }
                if gamepad.is_holding(GamepadButton::East)
                {
                    movement.y = -1.0;
                }
                if gamepad.is_holding(GamepadButton::LeftThumb)
                {
                    fast_movement = true;
                }
            }
        }

        // update movement
        let mut movement_vec = Vector3::<f32>::zeros();

        if !approx_zero_vec3(&movement)
        {
            let cam_data = cam_data.get_ref();
            last_eye_pos = Some(cam_data.eye_pos.clone());

            let dir = cam_data.dir.normalize();
            let up = cam_data.up.normalize();
            let right = up.cross(&dir);

            let mut factor = self.move_speed;
            if fast_movement
            {
                factor = self.move_speed_shift;
            }

            let sensitivity = frame_scale * factor;

            movement_vec += movement.z * dir * sensitivity;
            movement_vec += movement.x * right * sensitivity;
            movement_vec += movement.y * up * sensitivity;
        }

        // collision check
        if self.collision && !approx_zero_vec3(&movement_vec)
        {
            let cam_data = cam_data.get_ref();

            let origin = cam_data.eye_pos;
            let dir = movement_vec.normalize();

            let ray = Ray::new(origin, dir);

            let hit = scene.pick(&ray, false, false, None);

            if let Some(hit) = hit
            {
                if hit.time_of_impact < self.collision_distance
                {
                    let delta = self.collision_distance - hit.time_of_impact;
                    movement_vec = (dir * -1.0) * delta;
                }
                else if approx_equal(self.collision_distance, hit.time_of_impact)
                {
                    movement_vec = Vector3::<f32>::zeros();
                }
            }
        }

        if !approx_zero_vec3(&movement_vec)
        {
            let cam_data = cam_data.get_mut();

            cam_data.eye_pos += movement_vec;

            change = true;
        }

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.checkbox(&mut self.mouse_movement, "Mouse movement");
        ui.checkbox(&mut self.keyboard_movement, "Keyboard movement");
        ui.checkbox(&mut self.gamepad_movement, "Gamepad movement");

        ui.checkbox(&mut self.collision, "collision");

        ui.horizontal(|ui|
        {
            ui.label("Collision distance: ");
            ui.add(egui::DragValue::new(&mut self.collision_distance).speed(0.01));
        });

        ui.horizontal(|ui|
        {
            ui.label("Mouse sensitivity (rad): ");
            ui.add(egui::DragValue::new(&mut self.mouse_sensitivity.x).speed(0.01).prefix("x: "));
            ui.add(egui::DragValue::new(&mut self.mouse_sensitivity.y).speed(0.01).prefix("y: "));
        });

        ui.horizontal(|ui|
        {
            ui.label("Gamepad sensitivity (rad): ");
            ui.add(egui::DragValue::new(&mut self.gamepad_sensitivity).speed(0.01));
        });

        ui.horizontal(|ui|
        {
            ui.label("Movement Speed: ");
            ui.add(egui::DragValue::new(&mut self.move_speed).speed(0.1).prefix("normal: "));
            ui.add(egui::DragValue::new(&mut self.move_speed_shift).speed(0.1).prefix("shift: "));
        });
    }
}
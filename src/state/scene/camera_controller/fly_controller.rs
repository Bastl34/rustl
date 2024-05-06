use std::f32::consts::PI;

use nalgebra::{Vector2, Vector3};
use parry3d::shape::Ball;

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, math::{self, approx_zero_vec2, approx_zero_vec3}}, input::{gamepad::{GamepadAxis, GamepadButton}, input_manager::InputManager, keyboard::{Key, Modifier}}, state::scene::{camera::CameraData, node::NodeItem, scene::Scene}};

use super::camera_controller::{CameraController, CameraControllerBase};

const ANGLE_OFFSET_UP: f32 = 0.01;
const ANGLE_OFFSET_DOWN: f32 = 0.1;

const DEFAULT_SPHERE_RADIUS: f32 = 2.0;
const DEFAULT_MOUSE_SENSIVITY: f32 = 0.0015;
const DEFAULT_GAMEPAD_SENSIVITY: f32 = 0.03;

pub struct FlyController
{
    base: CameraControllerBase,

    collision: bool,

    move_speed: f32,
    move_speed_shift: f32,
    mouse_sensitivity: Vector2::<f32>,
    gamepad_sensitivity: f32,

    sphere_shape: Ball
}

impl FlyController
{
    pub fn new(collision: bool, mouse_sensitivity: Vector2::<f32>, move_speed: f32, move_speed_shift: f32) -> FlyController
    {
        FlyController
        {
            base: CameraControllerBase::new("Fly Controller".to_string(), "✈".to_string()),

            collision,

            move_speed,
            move_speed_shift,
            mouse_sensitivity,

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,

            sphere_shape: Ball::new(DEFAULT_SPHERE_RADIUS)
        }
    }

    pub fn default() -> Self
    {
        FlyController
        {
            base: CameraControllerBase::new("Fly Controller".to_string(), "✈".to_string()),

            collision: true,

            move_speed: 0.1,
            move_speed_shift: 0.2,
            mouse_sensitivity: Vector2::<f32>::new(DEFAULT_MOUSE_SENSIVITY, DEFAULT_MOUSE_SENSIVITY),

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,

            sphere_shape: Ball::new(DEFAULT_SPHERE_RADIUS)
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

        // gamepad
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
        let keys = vec![Key::W, Key::A, Key::S, Key::D, Key::Space, Key::C];
        if input_manager.keyboard.is_holding_by_keys(&keys) || input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)
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
            if input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                fast_movement = true;
            }
        }

        // gamepad
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

        // update movement
        if !approx_zero_vec3(&movement)
        {
            let cam_data = cam_data.get_mut();
            last_eye_pos = Some(cam_data.eye_pos.clone());

            let dir = cam_data.dir.normalize();
            let up = cam_data.up.normalize();
            let right = up.cross(&dir);

            let mut vec = Vector3::<f32>::zeros();

            let mut factor = self.move_speed;
            if fast_movement
            {
                factor = self.move_speed_shift;
            }

            let sensitivity = frame_scale * factor;

            vec += movement.z * dir * sensitivity;
            vec += movement.x * right * sensitivity;
            vec += movement.y * up * sensitivity;

            cam_data.eye_pos += vec;

            change = true;
        }

        /*
        let cam_data = cam_data.get_mut();
            last_eye_pos = Some(cam_data.eye_pos.clone());

            let dir = cam_data.dir.normalize();
            let up = cam_data.up.normalize();
            let right = up.cross(&dir);

            let mut vec = Vector3::<f32>::zeros();

            let mut factor = self.move_speed;
            if input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                factor = self.move_speed_shift;
            }

            let sensitivity = frame_scale * factor;

            if input_manager.keyboard.is_holding(Key::W)
            {
                vec += dir * sensitivity;
            }
            if input_manager.keyboard.is_holding(Key::S)
            {
                vec -= dir * sensitivity;
            }
            if input_manager.keyboard.is_holding(Key::D)
            {
                vec -= right * sensitivity;
            }
            if input_manager.keyboard.is_holding(Key::A)
            {
                vec += right * sensitivity;
            }
            if input_manager.keyboard.is_holding(Key::Space)
            {
                vec += up * sensitivity;
            }
            //if input_manager.keyboard.is_holding(Key::C) || input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)
            if input_manager.keyboard.is_holding(Key::C)
            {
                vec -= up * sensitivity;
            }

            cam_data.eye_pos += vec;

            change = true;
         */

        // collision check

        /*
        if change
        {
            let nodes = Scene::list_all_child_nodes_with_mesh(&scene.nodes);

            for node_arc in &nodes
            {
                let node = node_arc.read().unwrap();

                for instance in node.instances.get_ref()
                {
                    let instance = instance.borrow();
                    let instance = instance.get_ref();

                    let alpha = instance.get_alpha();

                    if approx_zero(alpha)
                    {
                        continue;
                    }

                    let transform = instance.get_transform();

                    let mesh = node.find_component::<Mesh>().unwrap();
                    component_downcast!(mesh, Mesh);

                    let mesh_data = mesh.get_data();

                    Isometry3::from_parts(translation, rotation)

                    //mesh_data.mesh.coll

                    //parry3d::
                    //query::contact(pos1, g1, pos2, g2, prediction)
                    //mesh_data.mesh.

                }
            }
        }
         */

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.checkbox(&mut self.collision, "collision");

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
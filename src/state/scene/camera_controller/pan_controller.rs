use nalgebra::{Point2, Vector3};
use serde::{Deserialize, Serialize};

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, math::{approx_zero, approx_zero_vec2, approx_zero_vec3}}, input::{gamepad::{GamepadAxis, GamepadButton}, keyboard::{Key, Modifier}, mouse::MouseButton}, state::{scene::{camera::{Camera, CameraData, CameraProjectionType}, node::NodeItem, scene::Scene}, state::InputOutput}};

use super::camera_controller::{gesture_owns_viewport, CameraController, CameraControllerBase};

const DEFAULT_MOUSE_WHEEL_SENSIVITY: f32 = 1.5;
const DEFAULT_GAMEPAD_SENSIVITY: f32 = 0.03;

// smallest allowed vertical extent (top - bottom), so a single zoom step can never collapse or invert the window
const ORTHO_MIN_EXTENT: f32 = 0.001;

// largest allowed vertical extent (top - bottom) - zoom out stops here (blender style)
const DEFAULT_ORTHO_MAX_EXTENT: f32 = 200.0;

fn default_ortho_max_extent() -> f32 { DEFAULT_ORTHO_MAX_EXTENT }

#[derive(Serialize, Deserialize)]
pub struct PanController
{
    base: CameraControllerBase,

    pub viewport_only: bool,

    pub mouse_wheel_zoom: bool,
    pub keyboard_movement: bool,
    pub gamepad_movement: bool,

    move_speed: f32,
    move_speed_shift: f32,
    mouse_wheel_sensitivity: f32,
    gamepad_sensitivity: f32,

    #[serde(default = "default_ortho_max_extent")]
    pub ortho_max_extent: f32,
}

impl PanController
{
    pub fn new(mouse_wheel_sensitivity: f32, move_speed: f32, move_speed_shift: f32, viewport_only: bool) -> PanController
    {
        PanController
        {
            base: CameraControllerBase::new("Pan Controller".to_string(), "↔".to_string()),

            viewport_only,

            mouse_wheel_zoom: true,
            keyboard_movement: true,
            gamepad_movement: true,

            move_speed,
            move_speed_shift,
            mouse_wheel_sensitivity,

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,

            ortho_max_extent: DEFAULT_ORTHO_MAX_EXTENT,
        }
    }

    pub fn default() -> Self
    {
        PanController
        {
            base: CameraControllerBase::new("Pan Controller".to_string(), "↔".to_string()),

            viewport_only: true,

            mouse_wheel_zoom: true,
            keyboard_movement: true,
            gamepad_movement: true,

            move_speed: 0.1,
            move_speed_shift: 0.2,
            mouse_wheel_sensitivity: DEFAULT_MOUSE_WHEEL_SENSIVITY,

            gamepad_sensitivity: DEFAULT_GAMEPAD_SENSIVITY,

            ortho_max_extent: DEFAULT_ORTHO_MAX_EXTENT,
        }
    }
}

#[typetag::serde]
impl CameraController for PanController
{
    camera_controller_impl_default!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
    }

    fn update(&mut self, _node: Option<NodeItem>, _scene: &mut Scene, io: &mut InputOutput, cam_data: &mut ChangeTracker<CameraData>, frame_scale: f32) -> bool
    {
        let mut change = false;

        if self.viewport_only && !gesture_owns_viewport(cam_data.get_ref(), &io.input_manager.mouse)
        {
            return false;
        }

        // ******************** movement ********************
        let mut movement = Vector3::<f32>::zeros();
        let mut fast_movement = false;

        // mouse
        if self.mouse_wheel_zoom
        {
            movement.z = io.input_manager.mouse.wheel_delta_y;
        }

        // keyboard
        if self.keyboard_movement
        {
            let keys = vec![Key::W, Key::A, Key::S, Key::D, Key::Space, Key::C];
            if io.input_manager.keyboard.is_holding_by_keys(&keys) || io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl)
            {
                if io.input_manager.keyboard.is_holding_and_not_consumed(Key::W)
                {
                    movement.y = 1.0;
                }
                if io.input_manager.keyboard.is_holding_and_not_consumed(Key::S)
                {
                    movement.y = -1.0;
                }
                if io.input_manager.keyboard.is_holding_and_not_consumed(Key::D)
                {
                    movement.x = -1.0;
                }
                if io.input_manager.keyboard.is_holding_and_not_consumed(Key::A)
                {
                    movement.x = 1.0;
                }
                if io.input_manager.keyboard.is_holding(Key::Space)
                {
                    movement.z = -1.0;
                }
                //if io.input_manager.keyboard.is_holding_and_not_consumed(Key::C) || io.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)
                if io.input_manager.keyboard.is_holding_and_not_consumed(Key::C)
                {
                    movement.z = 1.0;
                }
                if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                {
                    fast_movement = true;
                }
            }
        }

        // gamepad
        if self.gamepad_movement
        {
            for (_, gamepad) in &mut io.input_manager.gamepads
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
                    movement.y = 1.0;
                }
                if gamepad.is_holding(GamepadButton::DPadDown)
                {
                    movement.y = -1.0;
                }
                if gamepad.is_axis_active(GamepadAxis::LeftStickX)
                {
                    movement.x = -gamepad.get_axis_value(GamepadAxis::LeftStickX);
                }
                if gamepad.is_axis_active(GamepadAxis::LeftStickY)
                {
                    movement.y = gamepad.get_axis_value(GamepadAxis::LeftStickY);
                }
                if gamepad.is_holding(GamepadButton::LeftTrigger)
                {
                    movement.z = 1.0;
                }
                if gamepad.is_holding(GamepadButton::RightTrigger)
                {
                    movement.z = -1.0;
                }
                if gamepad.is_holding(GamepadButton::LeftThumb)
                {
                    fast_movement = true;
                }
            }
        }

        // middle mouse button drag
        if io.input_manager.mouse.is_holding(MouseButton::Middle) && !io.input_manager.mouse.is_holding(MouseButton::Right)
        {
            let velocity = io.input_manager.mouse.point.velocity;

            if let Some(pos) = io.input_manager.mouse.point.pos
            {
                if !approx_zero_vec2(&velocity)
                {
                    // world position under the cursor this frame vs. last frame (current matrices)
                    let delta =
                    {
                        let data = cam_data.get_ref();
                        let prev = Point2::new(pos.x - velocity.x, pos.y - velocity.y);
                        Camera::screen_to_world_data(data, &pos) - Camera::screen_to_world_data(data, &prev)
                    };

                    // move the camera opposite to the drag so the grabbed point stays under the cursor
                    cam_data.get_mut().eye_pos -= delta;
                    change = true;
                }
            }
        }

        // update movement
        let mut movement_vec = Vector3::<f32>::zeros();

        if !approx_zero_vec3(&movement)
        {
            let cam_data = cam_data.get_ref();

            let dir = cam_data.dir.normalize();
            let up = cam_data.up.normalize();
            let right = up.cross(&dir);

            let mut factor = self.move_speed;
            if fast_movement
            {
                factor = self.move_speed_shift;
            }

            let sensitivity = frame_scale * factor;

            //movement_vec += movement.z * dir * sensitivity;
            movement_vec += movement.x * right * sensitivity;
            movement_vec += movement.y * up * sensitivity;
        }

        if !approx_zero_vec3(&movement_vec)
        {
            let cam_data = cam_data.get_mut();

            cam_data.eye_pos += movement_vec;

            change = true;
        }

        // ******************** zoom (orthographic) ********************
        if !approx_zero(movement.z) && cam_data.get_ref().projection_type == CameraProjectionType::Orthogonal
        {
            let (old_half_h, center_x, center_y, half_w_old, cursor_offset) =
            {
                let data = cam_data.get_ref();

                let old_half_h = (data.top - data.bottom) * 0.5;
                let center_x = (data.left + data.right) * 0.5;
                let center_y = (data.top + data.bottom) * 0.5;
                let half_w_old = (data.right - data.left) * 0.5;

                let cursor_offset = if let Some(cursor) = io.input_manager.mouse.point.pos
                {
                    let viewport = data.get_viewport();
                    let center_px = Point2::new
                    (
                        (viewport.x + viewport.width  * 0.5) * data.resolution_width  as f32,
                        (viewport.y + viewport.height * 0.5) * data.resolution_height as f32,
                    );

                    Camera::screen_to_world_data(data, &cursor) - Camera::screen_to_world_data(data, &center_px)
                }
                else
                {
                    Vector3::<f32>::zeros()
                };

                (old_half_h, center_x, center_y, half_w_old, cursor_offset)
            };

            // symmetric shrink/grow around the center; > 0 zooms in
            let delta = movement.z * frame_scale * self.mouse_wheel_sensitivity;

            // max: keep already-larger legacy extents zoomable-in without snapping
            let max_half_h = (self.ortho_max_extent * 0.5).max(old_half_h);
            let new_half_h = (old_half_h - delta).clamp(ORTHO_MIN_EXTENT * 0.5, max_half_h);
            let scale = new_half_h / old_half_h;

            let cam_data = cam_data.get_mut();

            cam_data.left   = center_x - half_w_old * scale;
            cam_data.right  = center_x + half_w_old * scale;
            cam_data.bottom = center_y - new_half_h;
            cam_data.top    = center_y + new_half_h;

            // zoom-to-cursor: keep the point under the cursor stationary
            cam_data.eye_pos += (1.0 - scale) * cursor_offset;

            change = true;
        }

        change
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.checkbox(&mut self.viewport_only, "Viewport only");

        ui.checkbox(&mut self.mouse_wheel_zoom, "Mouse wheel zoom");
        ui.checkbox(&mut self.keyboard_movement, "Keyboard movement");
        ui.checkbox(&mut self.gamepad_movement, "Gamepad movement");

        ui.horizontal(|ui|
        {
            ui.label("Mouse wheel sensitivity (rad): ");
            ui.add(egui::DragValue::new(&mut self.mouse_wheel_sensitivity).speed(0.01));
        });

        ui.horizontal(|ui|
        {
            ui.label("Gamepad sensitivity (rad): ");
            ui.add(egui::DragValue::new(&mut self.gamepad_sensitivity).speed(0.01));
        });

        ui.horizontal(|ui|
        {
            ui.label("Max zoom out extent: ");
            ui.add(egui::DragValue::new(&mut self.ortho_max_extent).speed(1.0).range(ORTHO_MIN_EXTENT..=f32::MAX));
        });

        ui.horizontal(|ui|
        {
            ui.label("Movement Speed: ");
            ui.add(egui::DragValue::new(&mut self.move_speed).speed(0.1).prefix("normal: "));
            ui.add(egui::DragValue::new(&mut self.move_speed_shift).speed(0.1).prefix("shift: "));
        });
    }
}
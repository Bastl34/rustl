#![allow(dead_code)]

use std::{f32::consts::PI, sync::Arc};

use nalgebra::{Point3, Vector2, Vector3};
use parry3d::query::Ray;
use serde::{Deserialize, Serialize};

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, generic::get_millis, math::{self, approx_zero, approx_zero_vec2, interpolate}}, input::mouse::MouseButton, state::{scene::{camera::CameraData, node::NodeItem, scene::Scene}, state::{get_delta_t, InputOutput}}};

use crate::state::scene::exporter::serialization_helper::default_true;

use super::camera_controller::{CameraController, CameraControllerBase};

const DEFAULT_TARGET_POS: Point3::<f32> = Point3::new(0.0, 0.0, 0.0);
const ANGLE_OFFSET: f32 = 0.01;
const DEFAULT_AUTO_ROTATE_TIMEOUT: u64 = 2000;
const DEFAULT_ZOOM_SPEED: f32 = 0.05;
const LATERAL_OFFSET_FADE_RADIUS: f32 = 2.0;
const DEFAULT_MOUSE_SENSITIVITY: Vector2::<f32> = Vector2::<f32>::new(0.0015, 0.0015);
const DEFAULT_MOUSE_WHEEL_SENSITIVITY: f32 = 0.2;

pub fn default_max_radius() -> f32 { 1000.0 }

#[derive(Serialize, Deserialize)]
pub struct TargetRotationControllerData
{
    pub offset: Vector3::<f32>,

    pub radius: f32,
    pub alpha: f32, // y-achis
    pub beta: f32, // x-achis
}

#[derive(Serialize, Deserialize)]
pub struct TargetRotationController
{
    base: CameraControllerBase,

    #[serde(skip, default = "default_true")]
    run_initial_update: bool,

    pub data: ChangeTracker<TargetRotationControllerData>,

    pub mouse_sensitivity: Vector2::<f32>,
    pub mouse_wheel_sensitivity: f32,

    pub auto_rotate: Option<f32>,
    pub auto_rotate_timeout: u64,

    #[serde(skip, default)]
    pub object_center_predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>, // TODO -> find a better way to handle this

    // off = follow the node transform - an animated bbox center makes the camera tremble
    #[serde(default = "default_true")]
    pub use_bbox_center: bool,

    pub collision_check: bool,
    pub collision_check_offset: f32,
    pub collision_zoom_speed: f32,

    // moves the pivot along the camera right axis, so the target sits off center on screen
    #[serde(default)]
    pub lateral_offset: f32,

    // zoom limits - collision may still pull the camera closer
    #[serde(default)]
    pub min_radius: f32,
    #[serde(default = "default_max_radius")]
    pub max_radius: f32,

    // seconds the pivot needs to catch up with a moving target, 0 = rigid
    #[serde(default)]
    pub follow_smoothing: f32,
    #[serde(default)]
    pub follow_smoothing_vertical: f32,

    #[serde(skip, default)]
    last_manual_move: u64, // time in millis after the last movement
    #[serde(skip, default)]
    last_radius: Option<f32>, // the last radius which was maybe overwritten by collision

    #[serde(skip, default)]
    smoothed_target: Option<Point3::<f32>>,
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

            auto_rotate: None,
            auto_rotate_timeout: DEFAULT_AUTO_ROTATE_TIMEOUT,

            object_center_predicate: None,

            use_bbox_center: true,

            collision_check: false,
            collision_check_offset: 0.1,
            collision_zoom_speed: DEFAULT_ZOOM_SPEED,

            lateral_offset: 0.0,
            min_radius: 0.0,
            max_radius: default_max_radius(),
            follow_smoothing: 0.0,
            follow_smoothing_vertical: 0.0,

            last_manual_move: 0,
            last_radius: None,

            smoothed_target: None,
        }
    }

    pub fn default() -> Self
    {
        TargetRotationController
        {
            base: CameraControllerBase::new("Target Rotation Controller".to_string(), "⟲".to_string()),

            run_initial_update: true,

            data: ChangeTracker::new(TargetRotationControllerData
            {
                offset: Vector3::<f32>::zeros(),

                radius: 3.0,
                alpha: 0.0,
                beta: PI / 8.0,
            }),

            mouse_sensitivity: DEFAULT_MOUSE_SENSITIVITY,
            mouse_wheel_sensitivity: DEFAULT_MOUSE_WHEEL_SENSITIVITY,

            auto_rotate: None,
            auto_rotate_timeout: DEFAULT_AUTO_ROTATE_TIMEOUT,

            object_center_predicate: None,

            use_bbox_center: true,

            collision_check: false,
            collision_check_offset: 0.1,
            collision_zoom_speed: DEFAULT_ZOOM_SPEED,

            lateral_offset: 0.0,
            min_radius: 0.0,
            max_radius: default_max_radius(),
            follow_smoothing: 0.0,
            follow_smoothing_vertical: 0.0,

            last_manual_move: 0,
            last_radius: None,

            smoothed_target: None,
        }
    }

    pub fn get_target_pos(&self, node: Option<NodeItem>) -> Point3::<f32>
    {
        let mut target_pos = DEFAULT_TARGET_POS;

        if let Some(node) = node
        {
            let node = node.read().unwrap();

            if self.use_bbox_center
            {
                if let Some(center) = node.get_world_bbox_center(None, true, self.object_center_predicate.clone())
                {
                    target_pos = center;
                }
            }
            else
            {
                let transform = node.get_full_transform();
                let translation = math::extract_translation_from_transform(&transform);

                target_pos = Point3::new(translation.x, translation.y, translation.z);
            }
        }

        let controller_data = self.data.get_ref();
        target_pos + controller_data.offset
    }

    // sets the radius and forgets the one collision wanted to return to
    pub fn set_radius(&mut self, radius: f32)
    {
        self.data.get_mut().radius = radius;
        self.last_radius = None;
    }

    // scales the wanted distance like the mouse wheel does, from the one before a collision shortened it
    pub fn zoom_by(&mut self, factor: f32, min_radius: f32)
    {
        let radius = self.last_radius.unwrap_or(self.data.get_ref().radius) * factor;
        let min = self.min_radius.max(min_radius).max(0.0);
        self.set_radius(radius.clamp(min, self.max_radius.max(min)));
        self.last_manual_move = get_millis();
    }

    // places the camera around the target - also usable without an update (e.g. in the editor)
    pub fn apply_to_camera(&mut self, node: Option<NodeItem>, cam_data: &mut ChangeTracker<CameraData>)
    {
        let target_pos = self.get_target_pos(node);

        // snap - smoothing starts over from here
        self.smoothed_target = Some(target_pos);

        self.place(target_pos, cam_data);
    }

    // fades out while zooming in, so the zoom ends in the pivot and not beside it
    fn offset_pivot(&self, target_pos: Point3::<f32>) -> Point3::<f32>
    {
        let controller_data = self.data.get_ref();

        let fade = (controller_data.radius / LATERAL_OFFSET_FADE_RADIUS).clamp(0.0, 1.0);

        let right = Vector3::new(controller_data.alpha.cos(), 0.0, -controller_data.alpha.sin());
        target_pos + right * self.lateral_offset * fade
    }

    fn place(&mut self, target_pos: Point3::<f32>, cam_data: &mut ChangeTracker<CameraData>)
    {
        let target_pos = self.offset_pivot(target_pos);

        let cam_data = cam_data.get_mut();
        let (alpha, beta, radius) = { let data = self.data.get_ref(); (data.alpha, data.beta, data.radius) };

        let dir = math::yaw_pitch_to_direction(alpha, beta).normalize();

        cam_data.dir = -dir;
        cam_data.eye_pos = target_pos + dir * radius;
    }

    // moves the smoothed pivot one step toward the target, returns it and whether it still trails
    fn step_smoothing(&mut self, target_pos: Point3::<f32>, frame_scale: f32) -> (Point3::<f32>, bool)
    {
        let first_person = approx_zero(self.data.get_ref().radius);

        let Some(smoothed) = self.smoothed_target.filter(|_| !first_person) else
        {
            self.smoothed_target = Some(target_pos);
            return (target_pos, false);
        };

        let dt = get_delta_t(frame_scale);
        let factor = |smoothing: f32| if smoothing > 0.0 { 1.0 - (-dt / smoothing).exp() } else { 1.0 };

        let horizontal = factor(self.follow_smoothing);
        let vertical = factor(self.follow_smoothing_vertical);

        let mut pivot = Point3::new
        (
            interpolate(smoothed.x, target_pos.x, horizontal),
            interpolate(smoothed.y, target_pos.y, vertical),
            interpolate(smoothed.z, target_pos.z, horizontal)
        );

        let trailing = (pivot - target_pos).norm() > 0.0005;
        if !trailing
        {
            pivot = target_pos;
        }

        self.smoothed_target = Some(pivot);

        (pivot, trailing)
    }

}

#[typetag::serde]
impl CameraController for TargetRotationController
{
    camera_controller_impl_default!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
    }

    fn update(&mut self, node: Option<NodeItem>, scene: &mut Scene, io: &mut InputOutput, cam_data: &mut ChangeTracker<CameraData>, frame_scale: f32) -> bool
    {
        let mut change = false;

        let mut velocity = io.input_manager.mouse.point.velocity.clone();

        if !*io.input_manager.mouse.visible.get_ref()
        {
            velocity = io.input_manager.mouse.raw_velocity.velocity;
        }

        let mut update_needed = false;
        if let Some(node) = &node
        {
            update_needed = node.read().unwrap().has_changed_data();
        }

        // offset
        if io.input_manager.mouse.is_holding(MouseButton::Right) && !approx_zero_vec2(&velocity)
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
            self.last_manual_move = get_millis()
        }

        // rotation
        if (io.input_manager.mouse.is_holding(MouseButton::Left) || !*io.input_manager.mouse.visible.get_ref()) && !approx_zero_vec2(&velocity)
        {
            let delta_x = velocity.x * self.mouse_sensitivity.x;
            let delta_y = velocity.y * self.mouse_sensitivity.y;

            let data = self.data.get_mut();
            data.alpha -= delta_x;
            data.beta -= delta_y;

            data.alpha = data.alpha % (PI * 2.0);

            if data.beta > PI / 2.0 - ANGLE_OFFSET
            {
                data.beta = (PI / 2.0) - ANGLE_OFFSET;
            }
            else if data.beta < -PI / 2.0 + ANGLE_OFFSET
            {
                data.beta = -(PI / 2.0) + ANGLE_OFFSET;
            }

            update_needed = true;
            self.last_manual_move = get_millis()
        }

        // auto rotate
        if !io.input_manager.mouse.is_any_button_holding() && self.last_manual_move + self.auto_rotate_timeout < get_millis()
        {
            if let Some(auto_rotate) = self.auto_rotate
            {
                let mut alpha = self.data.get_ref().alpha + (auto_rotate * frame_scale);
                alpha = alpha % (PI * 2.0);

                self.data.get_mut().alpha = alpha;
            }
        }

        // distance
        if !math::approx_zero(io.input_manager.mouse.wheel_delta_y)
        {
            // from the wanted distance, not the one a collision pulled the camera to
            let radius = self.last_radius.unwrap_or(self.data.get_ref().radius) + self.mouse_wheel_sensitivity * -io.input_manager.mouse.wheel_delta_y;
            self.data.get_mut().radius = radius.clamp(self.min_radius.max(0.0), self.max_radius.max(self.min_radius.max(0.0)));

            update_needed = true;
            self.last_manual_move = get_millis();
            self.last_radius = None;
        }

        // smoothing
        if self.run_initial_update
        {
            self.smoothed_target = None;
        }

        let raw_target_pos = self.get_target_pos(node.clone());
        let (target_pos, trailing) = self.step_smoothing(raw_target_pos, frame_scale);

        // apply
        let controller_data_change = self.data.consume_change();
        if self.run_initial_update || update_needed || controller_data_change || trailing
        {
            self.place(target_pos, cam_data);

            self.run_initial_update = false;

            change = true;
        }

        // collision - while pulled in it keeps checking, even without a change, until it is back out
        if (change || self.last_radius.is_some()) && self.collision_check && node.is_some()
        {
            let ray_origin = self.offset_pivot(target_pos);
            let radius = self.data.get_ref().radius;
            let wanted = self.last_radius.unwrap_or(radius);

            // a unit direction, so the time of impact is a distance - the pick itself has no length limit
            let dir = -cam_data.get_ref().dir.normalize();
            let ray = Ray::new(ray_origin.into(), dir.into());

            let target_node = node.clone().unwrap();
            let pick_res = scene.pick(&ray, false, false, false, false, Some(Arc::new(move |node, _instance|
            {
                let node = node.read().unwrap();
                let has_currect_parent = node.has_parent_or_is_equal(target_node.clone());
                let camera_collision_allowed = node.has_camera_collision();

                !has_currect_parent && camera_collision_allowed
            })));

            // only what lies between the target and the wanted camera position is in the way
            let obstacle = pick_res.map(|pick_res| pick_res.time_of_impact).filter(|distance| *distance < wanted);
            let allowed = obstacle.map_or(wanted, |distance| (distance - self.collision_check_offset).max(self.collision_check_offset));

            if obstacle.is_some() && self.last_radius.is_none()
            {
                self.last_radius = Some(wanted);
            }

            // in front of an obstacle at once, back out smoothly
            let new_radius = if allowed < radius
            {
                allowed
            }
            else
            {
                let eased = interpolate(radius, allowed, (frame_scale * self.collision_zoom_speed).min(1.0));
                if (allowed - eased).abs() < 0.05 { allowed } else { eased }
            };

            if new_radius != radius
            {
                self.data.get_mut().radius = new_radius;
                self.place(target_pos, cam_data);
                change = true;
            }

            if obstacle.is_none() && new_radius == wanted
            {
                self.last_radius = None;
            }
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

        let mut auto_rotate = 0.0;
        if let Some(auto_rotate_value) = self.auto_rotate
        {
            auto_rotate = auto_rotate_value;
        }

        ui.horizontal(|ui|
        {
            ui.label("Auto Rotate:");

            if ui.add(egui::DragValue::new(&mut auto_rotate).speed(0.001)).changed()
            {
                if approx_zero(auto_rotate)
                {
                    self.auto_rotate = None;
                }
                else
                {
                    self.auto_rotate = Some(auto_rotate);
                }
            }
        });

        ui.add(egui::Slider::new(&mut self.auto_rotate_timeout, 0..=5000).text("auto rotate timeout"));

        ui.checkbox(&mut self.collision_check, "Collision check");

        ui.horizontal(|ui|
        {
            ui.label("Collision check offset: ");
            ui.add(egui::DragValue::new(&mut self.collision_check_offset).speed(0.01))
        });

        ui.horizontal(|ui|
        {
            ui.label("Collision zoom speed: ");
            ui.add(egui::DragValue::new(&mut self.collision_zoom_speed).speed(0.1))
        });

        ui.horizontal(|ui|
        {
            ui.label("Lateral offset: ");
            ui.label("ℹ").on_hover_text("moves the pivot along the camera right axis, so the target sits off center (e.g. over the shoulder)");
            ui.add(egui::DragValue::new(&mut self.lateral_offset).speed(0.01))
        });

        ui.horizontal(|ui|
        {
            ui.label("Zoom limits: ");
            ui.add(egui::DragValue::new(&mut self.min_radius).speed(0.05).range(0.0..=1000.0).prefix("min: "));
            ui.add(egui::DragValue::new(&mut self.max_radius).speed(0.05).range(0.0..=1000.0).prefix("max: "));
        });

        ui.horizontal(|ui|
        {
            ui.label("Follow smoothing (s): ");
            ui.label("ℹ").on_hover_text("how long the pivot takes to catch up with a moving target, 0 = rigid");
            ui.add(egui::DragValue::new(&mut self.follow_smoothing).speed(0.005).range(0.0..=2.0).prefix("h: "));
            ui.add(egui::DragValue::new(&mut self.follow_smoothing_vertical).speed(0.005).range(0.0..=2.0).prefix("v: "));
        });

    }
}
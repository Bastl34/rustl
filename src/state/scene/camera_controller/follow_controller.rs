#![allow(dead_code)]

use std::f32::consts::PI;

use nalgebra::{Point3, UnitQuaternion, Vector2, Vector3};
use serde::{Deserialize, Serialize};

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, math::{self, approx_equal_vec, approx_zero_vec2}}, input::mouse::MouseButton, state::{scene::{camera::CameraData, node::NodeItem, scene::Scene}, state::InputOutput}};

use super::camera_controller::{CameraController, CameraControllerBase};

const DEFAULT_MOUSE_SENSITIVITY: Vector2::<f32> = Vector2::<f32>::new(0.0015, 0.0015);
pub const FOLLOW_PITCH_LIMIT: f32 = PI / 2.0 - 0.01;

fn default_basis() -> UnitQuaternion<f32> { UnitQuaternion::identity() }
fn default_mouse_sensitivity() -> Vector2::<f32> { DEFAULT_MOUSE_SENSITIVITY }

#[derive(Serialize, Deserialize)]
pub struct FollowControllerData
{
    pub offset: Vector3::<f32>, // node space - turns with the node, not scaled

    // looking around, relative to the node
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub pitch: f32,
}

// the camera sits on the node and turns, pitches and rolls with it - e.g. a cockpit view
#[derive(Serialize, Deserialize)]
pub struct FollowController
{
    base: CameraControllerBase,

    pub data: ChangeTracker<FollowControllerData>,

    // the view axes in node space: +z forward, +y up
    #[serde(default = "default_basis")]
    pub basis: UnitQuaternion<f32>,

    #[serde(default)]
    pub mouse_look: bool, // left mouse button or a hidden cursor
    #[serde(default = "default_mouse_sensitivity")]
    pub mouse_sensitivity: Vector2::<f32>,
}

impl FollowController
{
    pub fn new() -> FollowController
    {
        FollowController
        {
            base: CameraControllerBase::new("Follow Controller".to_string(), "👣".to_string()),

            data: ChangeTracker::new(FollowControllerData
            {
                offset: Vector3::<f32>::zeros(),
                yaw: 0.0,
                pitch: 0.0,
            }),

            basis: default_basis(),

            mouse_look: false,
            mouse_sensitivity: DEFAULT_MOUSE_SENSITIVITY,
        }
    }

    pub fn look_by(&mut self, yaw: f32, pitch: f32)
    {
        let data = self.data.get_mut();
        data.yaw = (data.yaw + yaw) % (PI * 2.0);
        data.pitch = (data.pitch + pitch).clamp(-FOLLOW_PITCH_LIMIT, FOLLOW_PITCH_LIMIT);
    }
}

#[typetag::serde]
impl CameraController for FollowController
{
    camera_controller_impl_default!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
    }

    fn update(&mut self, node: Option<NodeItem>, _scene: &mut Scene, io: &mut InputOutput, cam_data: &mut ChangeTracker<CameraData>, _frame_scale: f32) -> bool
    {
        let Some(node) = node else { return false; };

        if self.mouse_look
        {
            let mouse = &io.input_manager.mouse;
            let hidden = !*mouse.visible.get_ref();
            let velocity = if hidden { mouse.raw_velocity.velocity } else { mouse.point.velocity };

            if (hidden || mouse.is_holding(MouseButton::Left)) && !approx_zero_vec2(&velocity)
            {
                self.look_by(-velocity.x * self.mouse_sensitivity.x, velocity.y * self.mouse_sensitivity.y);
            }
        }

        let transform = node.read().unwrap().get_full_transform();
        let node_rotation = math::extract_rotation_quat_from_transform(&transform);
        let view_rotation = node_rotation * self.basis;

        let data = self.data.get_ref();
        let eye_pos = Point3::from(math::extract_translation_from_transform(&transform) + node_rotation * data.offset);
        let dir = view_rotation * math::yaw_pitch_to_direction(data.yaw, data.pitch).normalize();
        let up = view_rotation * Vector3::y();

        let cam = cam_data.get_ref();
        if approx_equal_vec(&cam.eye_pos.coords, &eye_pos.coords) && approx_equal_vec(&cam.dir, &dir) && approx_equal_vec(&cam.up, &up)
        {
            return false;
        }

        let cam = cam_data.get_mut();
        cam.eye_pos = eye_pos;
        cam.dir = dir;
        cam.up = up;

        true
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
        }).response.on_hover_text("node space - turns with the node");

        ui.horizontal(|ui|
        {
            let (mut yaw, mut pitch) = { let data = self.data.get_ref(); (data.yaw.to_degrees(), data.pitch.to_degrees()) };

            ui.label("Look:");
            let yaw_changed = ui.add(egui::DragValue::new(&mut yaw).speed(0.5).prefix("yaw: ").suffix("°")).changed();
            let pitch_changed = ui.add(egui::DragValue::new(&mut pitch).speed(0.5).range(-89.0..=89.0).prefix("pitch: ").suffix("°")).changed();

            if yaw_changed || pitch_changed
            {
                let data = self.data.get_mut();
                data.yaw = yaw.to_radians();
                data.pitch = pitch.to_radians();
            }
        });

        ui.checkbox(&mut self.mouse_look, "Mouse look").on_hover_text("left mouse button or a hidden cursor turns the view");
    }
}

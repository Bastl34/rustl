#![allow(dead_code)]

use std::{collections::{HashMap, HashSet}, f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{Matrix3, Matrix4, Rotation3, Unit, UnitQuaternion, Vector3};
use rapier3d::prelude::{ColliderHandle, Pose, Rotation, SharedShape, Vector};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_downcast_mut, console_error, console_log, console_warning, helper::{math::{approx_zero, extract_rotation_quat_from_transform, extract_translation_from_transform, shortest_angle_dist, yaw_pitch_from_direction}, option_or_id::OptionOrId}, input::{gamepad::{GamepadAxis, GamepadButton}, input_binding::{gamepad_select_ui, input_action_ui, AxisDirection, GamepadSelect, InputAction, InputSource}, keyboard::Key, mouse::MouseButton}, scene_controller_impl_default, state::{scene::{camera_controller::{camera_controller::CameraControllerBox, follow_controller::FollowController, target_rotation_controller::TargetRotationController}, components::{animation::Animation, component::ComponentItem, mesh::Mesh, transformation::Transformation}, node::NodeItem, physics::{contacts::ContactTarget, physics_world::{HitchDesc, HitchState, PhysicsWorld, VehicleChassisDesc, VehicleWheelDesc}}, scene::Scene, scene_controller::scene_controller::SceneControllerBase}, state::{get_delta_t, InputOutput, RunMode}}};

use rapier3d::control::WheelTuning;

use crate::state::scene::exporter::serialization_helper::{default_true, deserialize_node, serialize_node};

use egui::{Color32, RichText};
use crate::gui::helper::property_items::{combo, slider, vector_edit};
use crate::state::scene::components::sound::Sound;
use crate::state::resources::sound_source::SoundSourceItem;
use crate::state::scene::components::sound::SoundType;
use super::{scene_controller::{ControllerUiContext, SceneController}, vehicle::{engine::{EngineSettings, EngineState}, engine_sound::{VehicleSoundInput, VehicleSoundPlayer, VehicleSoundSettings}, tire_marks::{TireMarkInput, TireMarkSettings, TireMarks}}};

const EARTH_GRAVITY: f32 = 9.81;

// the wheel name search - the exclude keeps the steering wheel and the spare wheel out
const WHEEL_NAME_REGEX: &str = r"(?i)(wheel|tire|tyre|reifen|felge|rim\b|(^|[ _.:-])rad([ _.:-]|$))";
const WHEEL_NAME_EXCLUDE_REGEX: &str = r"(?i)(steer|lenk|spare|reserve|ersatz)";
const FRONT_NAME_REGEX: &str = r"(?i)(front|vorn|(^|[ _.:-])f[lr]([ _.:-]|$))";
const BACK_NAME_REGEX: &str = r"(?i)(back|rear|hinten|(^|[ _.:-])r[lr]([ _.:-]|$))";
const STEERING_NAME_REGEX: &str = r"(?i)(steering|lenkrad|handlebar|lenker)";

// the coupling part of a trailer: the drawbar eye of a trailer, the kingpin of a semi trailer
const HITCH_NAME_REGEX: &str = r"(?i)(hitch|coupling|kingpin|king pin|kupplung|zugöse|zugoese|königszapfen|koenigszapfen)";

// without a coupling part the trailer couples at the middle of its front tip, this deep, m
const HITCH_TIP_DEPTH: f32 = 0.03;

// a node counts as a wheel by shape when it is round seen from the side and flat seen from the front
const WHEEL_ROUNDNESS_MIN: f32 = 0.75;
const WHEEL_ROUNDNESS_MAX: f32 = 1.33;
const WHEEL_MAX_WIDTH_RATIO: f32 = 0.85;
const WHEEL_BOTTOM_ZONE: f32 = 0.35; // share of the vehicle height, measured from its lowest point

// wheels closer than this along the vehicle belong to the same axle, m
const AXLE_TOLERANCE: f32 = 0.25;

// below this the vehicle counts as standing - the brake input turns into reverse, m/s
const STANDSTILL_SPEED: f32 = 0.5;

// no counter steering or leaning below this, m/s
const ASSIST_MIN_SPEED: f32 = 3.0;

// slip angles below this are normal cornering, not a slide - degrees
const COUNTER_STEER_DEADZONE: f32 = 6.0;

// wheel bumpers: share of the wheel radius, and how far above flat ground they stay, m
const BUMPER_RADIUS: f32 = 0.8;
const BUMPER_LIFT: f32 = 0.03;

// the recover check - up vector below this means lying on the side or the roof
const UPSIDE_DOWN_DOT: f32 = 0.3;

// 1/s the wanted lean closes the rest of its way with, below the roll rate
const LEAN_GOAL_RESPONSE: f32 = 3.0;

// share of the wheel radius the collision bottom rises above the axle at the very front and back - bench_loop_entry, 12 entries at 50-80 km/h:
// flat and hard 4 through, rounded 6, rounded + 0.5 8 with the smallest jolts, 1.0 lets low rails and cones slip under the nose
const SLOPED_END_LIFT: f32 = 0.5;

// how far the automatic center of mass may sit off the middle of the wheels, share of the wheelbase and track - 60/40 at most
const COM_REACH: f32 = 0.1;

// share of the pull that would lift the front wheels - half leaves the front grip to steer with, and the squat overshoot stays below a lift
const WHEELIE_MARGIN: f32 = 0.5;

// m from the top of the driver's head down to below its chin
const HEAD_SIZE: f32 = 0.25;

const DEFAULT_CAM_PITCH: f32 = 12.0;
const COCKPIT_PITCH: f32 = 0.05;

// zooming in stops here, closer the chase camera would sit inside the vehicle
const CHASE_MIN_DISTANCE: f32 = 2.0;

// m/s - below it the travel direction is too shaky for the camera, twice as fast it counts fully
const SLIDE_FOLLOW_MIN_SPEED: f32 = 2.0;

// slip angle of a wheel, degrees: the normal few degrees of a fast corner are not sliding yet - from here it squeals and marks, fully at the second
const SLIDE_MIN_ANGLE: f32 = 10.0;
const SLIDE_FULL_ANGLE: f32 = 30.0;

// m/s sideways - slower, a big angle is just maneuvering, twice as fast it counts fully
const SLIDE_MIN_SPEED: f32 = 1.0;

// tire width as a share of the radius, for wheels the auto setup has not measured yet
const UNMEASURED_TIRE_WIDTH: f32 = 0.6;

// ********** enums **********

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum VehicleType
{
    #[default]
    Car,
    SportsCar,
    ElectricCar,
    Bus,
    Truck,
    MultiAxle, // military trucks, many driven and several steered axles
    Motorcycle,
    Bicycle,
    Scooter,
    Trike,
    Tank, // tracks: skid steering over many road wheels
    Kart, // light, stiff, no differential feel - quick steering and a high revving small engine
    Trailer, // no engine and no driver - coupled to the hitch of another vehicle, brakes with it
}

impl VehicleType
{
    pub fn all() -> [VehicleType; 13]
    {
        [VehicleType::Car, VehicleType::SportsCar, VehicleType::ElectricCar, VehicleType::Bus, VehicleType::Truck, VehicleType::MultiAxle, VehicleType::Motorcycle, VehicleType::Bicycle, VehicleType::Scooter, VehicleType::Trike, VehicleType::Tank, VehicleType::Kart, VehicleType::Trailer]
    }

    // a typical length of the type, m - the presets are made for it
    pub fn reference_length(&self) -> f32
    {
        match self
        {
            VehicleType::Car => 4.3,
            VehicleType::SportsCar => 4.5,
            VehicleType::ElectricCar => 4.7,
            VehicleType::Bus => 12.0,
            VehicleType::Truck => 8.0,
            VehicleType::MultiAxle => 9.0,
            VehicleType::Motorcycle => 2.1,
            VehicleType::Bicycle => 1.8,
            VehicleType::Scooter => 1.8,
            VehicleType::Trike => 2.5,
            VehicleType::Tank => 7.5,
            VehicleType::Kart => 1.9,
            VehicleType::Trailer => 4.0,
        }
    }

    pub fn is_two_wheeler(&self) -> bool
    {
        matches!(self, VehicleType::Motorcycle | VehicleType::Bicycle | VehicleType::Scooter)
    }

    pub fn is_trailer(&self) -> bool
    {
        *self == VehicleType::Trailer
    }
}

// Where the vehicle faces, as it is placed in the editor when the setup runs.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum VehicleForward
{
    #[default]
    Auto, // from wheel names like "front left", otherwise world -z
    WorldNegZ,
    WorldPosZ,
    WorldNegX,
    WorldPosX,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum VehicleDrive
{
    Front,
    #[default]
    Rear,
    All,
    Tracked, // no steering angle - the sides are driven against each other
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum ChassisShape
{
    Box,
    #[default]
    ConvexHull, // rounded like the body - a box catches with its corners, e.g. in a loop
    Compound, // one hull per mesh - an open frame or a car carrier keeps its gaps, e.g. for a load
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum SteeringAxis
{
    #[default]
    Column, // a steering wheel - turns around its column
    Handlebar, // turns around the vehicle up axis, like the front wheel
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum VehicleCameraMode
{
    #[default]
    Chase,
    Cockpit,
}

// ********** settings **********

// The vehicle axes in chassis space: the rigid part of the vehicle node's world transform.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleFrame
{
    pub forward: Vector3<f32>,
    pub up: Vector3<f32>,
}

impl VehicleFrame
{
    pub fn right(&self) -> Vector3<f32>
    {
        self.forward.cross(&self.up).normalize()
    }

    // turns the camera orbit axes into chassis space: +y to up, +z to forward
    pub fn camera_basis(&self) -> UnitQuaternion<f32>
    {
        let up = self.up.normalize();
        let forward = (self.forward - up * self.forward.dot(&up)).normalize();

        UnitQuaternion::from_basis_unchecked(&[up.cross(&forward), up, forward])
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VehicleWheel
{
    pub node_name: String,

    // names are not unique - the loader names a mesh node after its mesh, so four wheels can share one
    #[serde(default)]
    pub node_uuid: String,

    pub center: Vector3<f32>, // chassis space at rest, measured by the setup
    pub radius: f32,
    #[serde(default)]
    pub width: f32, // m across the tire, measured by the setup - 0 = not measured yet

    pub steer: f32, // share of the steering angle, negative steers the other way (rear axle steering)
    pub driven: bool,
    pub handbrake: bool,

    #[serde(skip, default)]
    runtime: Option<PivotRuntime>,
}

impl VehicleWheel
{
    pub fn new(node_name: &str, center: Vector3<f32>, radius: f32) -> Self
    {
        Self
        {
            node_name: node_name.to_string(),
            node_uuid: String::new(),
            center,
            radius,
            width: 0.0,
            steer: 0.0,
            driven: false,
            handbrake: false,
            runtime: None
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleChassisSettings
{
    pub shape: ChassisShape,
    pub mass: f32, // kg

    pub center_of_mass_auto: bool,
    pub center_of_mass_height: f32, // auto: share of the chassis height, from its bottom
    pub center_of_mass: Vector3<f32>, // chassis space, m

    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,

    // the collision bottom stays at least this share of the wheel radius above the wheel bottom
    #[serde(default = "default_min_clearance")]
    pub min_clearance: f32,

    // balls at the wheels that let the vehicle slide up over curbs and ramp edges - rigid, so they launch it over bumps at speed
    #[serde(default)]
    pub wheel_bumpers: bool,

    // the bottom rises to the axle height in front of the front and behind the rear axle
    #[serde(default = "default_true")]
    pub sloped_ends: bool,

    // m the edges of the collision body are rounded by - a hard edge catches on every ramp start and loop entry
    #[serde(default = "default_rounding")]
    pub rounding: f32,
}

fn default_min_clearance() -> f32 { 1.0 }
fn default_rounding() -> f32 { 0.2 }

impl Default for VehicleChassisSettings
{
    fn default() -> Self
    {
        Self
        {
            shape: ChassisShape::ConvexHull,
            mass: 1300.0,

            center_of_mass_auto: true,
            center_of_mass_height: 0.3,
            center_of_mass: Vector3::zeros(),

            friction: 0.5,
            restitution: 0.1,
            linear_damping: 0.0,
            angular_damping: 0.3,

            min_clearance: default_min_clearance(),
            wheel_bumpers: false,
            sloped_ends: true,
            rounding: default_rounding(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleSuspensionSettings
{
    pub rest_length: f32, // m
    pub travel: f32, // m, up and down from the rest length
    pub stiffness: f32, // per unit of chassis mass - sag = g / (wheels * stiffness)
    pub compression: f32, // damping while compressing
    pub relaxation: f32, // damping while extending
    pub max_force_factor: f32, // times the chassis weight, per wheel
    pub keep_ride_height: bool, // mount the springs so the loaded vehicle sits where it was modelled
}

impl Default for VehicleSuspensionSettings
{
    fn default() -> Self
    {
        Self
        {
            rest_length: 0.3,
            travel: 0.2,
            stiffness: 35.0,
            compression: 2.2,
            relaxation: 3.2,
            max_force_factor: 3.0,
            keep_ride_height: true
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleTireSettings
{
    pub grip: f32, // roughly the friction coefficient of the tire
    pub side_grip: f32,

    // scale the grip with the friction of the ground collider - different surfaces grip differently
    pub surface_grip: bool,
}

impl Default for VehicleTireSettings
{
    fn default() -> Self
    {
        Self { grip: 2.0, side_grip: 1.0, surface_grip: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VehicleSteeringSettings
{
    pub max_angle: f32, // degrees
    pub speed: f32, // keyboard: how fast the full lock is reached, 1/s
    pub return_speed: f32, // keyboard: how fast it centers again, 1/s
    pub high_speed: f32, // km/h at which only high_speed_factor of the lock is left
    pub high_speed_factor: f32,

    pub node_name: String, // steering wheel or handlebar, empty = none
    #[serde(default)]
    pub node_uuid: String,
    pub axis: SteeringAxis,
    pub ratio: f32, // steering wheel turn per wheel turn
}

impl Default for VehicleSteeringSettings
{
    fn default() -> Self
    {
        Self
        {
            max_angle: 35.0,
            speed: 3.0,
            return_speed: 5.0,
            high_speed: 120.0,
            high_speed_factor: 0.35,
            node_name: String::new(),
            node_uuid: String::new(),
            axis: SteeringAxis::Column,
            ratio: 12.0
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleBrakeSettings
{
    pub brake_force: f32, // N, all wheels together
    pub handbrake_force: f32, // N, the handbrake wheels together
    pub rolling_resistance: f32, // N, all wheels together
    pub air_drag: f32, // force = air_drag * speed^2
}

impl Default for VehicleBrakeSettings
{
    fn default() -> Self
    {
        Self
        {
            brake_force: 13000.0,
            handbrake_force: 12000.0,
            rolling_resistance: 200.0,
            air_drag: 0.9
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleDriftSettings
{
    pub handbrake_grip: f32, // grip along the wheel while it is pulled, share - a locked wheel still brakes
    pub handbrake_side_grip: f32,
    pub grip_recovery: f32, // 1/s after the handbrake is released
    pub throttle_hold: f32, // share of the recovery that is left while on throttle and sliding - keeps a drift going
    pub counter_steer: f32, // steers into the slide by this share of the slip angle
}

impl Default for VehicleDriftSettings
{
    fn default() -> Self
    {
        Self
        {
            handbrake_grip: 0.8,
            handbrake_side_grip: 0.3,
            grip_recovery: 1.5,
            throttle_hold: 0.3,
            counter_steer: 0.4
        }
    }
}

// Keeps a two wheeler upright and leans it into curves.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleBalanceSettings
{
    pub enabled: bool,
    pub max_lean: f32, // degrees
    pub stiffness: f32, // 1/s^2
    pub damping: f32, // 1/s
    pub lean_factor: f32, // share of the physically right lean angle

    // degrees/s the wanted lean changes by at most - a quick flick, not a snap that lifts the wheels
    #[serde(default = "default_roll_rate")]
    pub roll_rate: f32,
}

fn default_roll_rate() -> f32 { 120.0 }

impl Default for VehicleBalanceSettings
{
    fn default() -> Self
    {
        Self
        {
            enabled: false,
            max_lean: 45.0,
            stiffness: 60.0,
            damping: 12.0,
            lean_factor: 1.0,
            roll_rate: default_roll_rate()
        }
    }
}

// Skid steering of a tracked vehicle.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleTrackSettings
{
    pub turn_force: f32, // share of the first gear drive force pushing the sides against each other
    pub turn_rate: f32, // degrees/s the yaw assist aims for at full lock
    pub turn_assist: f32, // how hard the yaw assist pulls, 1/s
    pub turn_side_grip: f32, // side grip while turning - tracks have to slide sideways to turn
}

impl Default for VehicleTrackSettings
{
    fn default() -> Self
    {
        Self
        {
            turn_force: 0.5,
            turn_rate: 35.0,
            turn_assist: 4.0,
            turn_side_grip: 0.4
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleCameraSettings
{
    pub mode: VehicleCameraMode,

    pub distance_auto: bool,
    pub distance: f32,
    pub height: f32, // above the chassis center

    pub follow: bool, // swing in behind while driving
    pub follow_speed: f32, // 1/s
    pub follow_delay: f32, // s after orbiting with the mouse

    // 0 = behind the nose, 1 = behind where the vehicle actually goes - a drift then shows the car sliding out
    #[serde(default = "default_slide_follow")]
    pub slide_follow: f32,

    pub cockpit_auto: bool, // eye point from the seat
    pub cockpit_offset: Vector3<f32>, // chassis space, m
}

fn default_slide_follow() -> f32 { 0.8 }

impl Default for VehicleCameraSettings
{
    fn default() -> Self
    {
        Self
        {
            mode: VehicleCameraMode::Chase,

            distance_auto: true,
            distance: 7.0,
            height: 1.0,

            follow: true,
            follow_speed: 3.0,
            follow_delay: 1.5,
            slide_follow: default_slide_follow(),

            cockpit_auto: true,
            cockpit_offset: Vector3::new(0.0, 1.2, 0.0),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum SeatRole
{
    Driver, // the first one gives the cockpit view its head
    #[default]
    Passenger,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VehicleSeat
{
    #[serde(default)]
    pub role: SeatRole,
    pub node_name: String,
    pub animation: String, // regex of the sitting clip

    pub seat_auto: bool, // somebody already inside the vehicle keeps where it sits
    pub seat_position: Vector3<f32>, // chassis space, m - where the node origin goes
    pub seat_rotation: f32, // degrees around the vehicle up axis

    pub hide_in_cockpit: bool,
}

impl VehicleSeat
{
    pub fn new(role: SeatRole) -> Self
    {
        Self
        {
            role,
            node_name: String::new(),
            animation: "(?i)(sit|driv)".to_string(),
            seat_auto: true,
            seat_position: Vector3::zeros(),
            seat_rotation: 0.0,
            hide_in_cockpit: role == SeatRole::Driver,
        }
    }
}

// the node on a seat, by the index of the seat
struct SeatRuntime
{
    node: NodeItem,
    animation: Option<ComponentItem>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VehicleRecoverSettings
{
    pub auto: bool,
    pub delay: f32, // s on the side or the roof before the auto recover
    pub lift: f32, // m
}

impl Default for VehicleRecoverSettings
{
    fn default() -> Self
    {
        Self { auto: false, delay: 3.0, lift: 1.0 }
    }
}

// The trailer on the hitch - a scene node outside the vehicle with its own vehicle controller of type Trailer.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct VehicleHitchSettings
{
    pub trailer_name: String, // empty = no trailer

    // a part of the trailer named like 'hitch' or 'kingpin', otherwise the middle of its front tip - where the two stand at the run start
    pub point_auto: bool,
    pub point: Vector3<f32>, // the ball, chassis space, m

    // degrees to each side from straight - 180 = free
    pub yaw_limit: f32,
    pub pitch_limit: f32,
    pub roll_limit: f32,

    pub break_roll: f32, // degrees the trailer rolls against the vehicle before it tears off, 0 = never
    pub break_force: f32, // kN at the ball that tear the trailer off, 0 = never

    pub trailer_brakes: bool, // the trailer brakes along with the vehicle
}

impl Default for VehicleHitchSettings
{
    fn default() -> Self
    {
        Self
        {
            trailer_name: String::new(),
            point_auto: true,
            point: Vector3::zeros(),
            yaw_limit: 85.0,
            pitch_limit: 30.0,
            roll_limit: 40.0,
            break_roll: 0.0,
            break_force: 0.0,
            trailer_brakes: true,
        }
    }
}

// which keys and gamepad drive this vehicle - two vehicles with different ones make split screen
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct VehicleControls
{
    pub gamepad: GamepadSelect,
    pub throttle: InputAction,
    pub brake: InputAction,
    pub steer_left: InputAction,
    pub steer_right: InputAction,
    pub handbrake: InputAction,
    pub recover: InputAction,
    pub camera: InputAction,
    pub look_left: InputAction,
    pub look_right: InputAction,
    pub look_up: InputAction,
    pub look_down: InputAction,
    pub look_speed: f32, // deg/s at full stick
    pub zoom_in: InputAction,
    pub zoom_out: InputAction,
    pub zoom_speed: f32, // 1/s, the distance changes by e^(speed * t)
}

impl Default for VehicleControls
{
    fn default() -> Self
    {
        use InputSource::{GamepadAxis as Axis, GamepadButton as Button, Key as K};

        Self
        {
            gamepad: GamepadSelect::Any,
            throttle: InputAction::new(&[K(Key::W), K(Key::ArrowUp), Button(GamepadButton::RightTrigger), Axis(GamepadAxis::RightTrigger, AxisDirection::Positive)]),
            brake: InputAction::new(&[K(Key::S), K(Key::ArrowDown), Button(GamepadButton::LeftTrigger), Axis(GamepadAxis::LeftTrigger, AxisDirection::Positive)]),
            steer_left: InputAction::new(&[K(Key::A), K(Key::ArrowLeft), Axis(GamepadAxis::LeftStickX, AxisDirection::Negative)]),
            steer_right: InputAction::new(&[K(Key::D), K(Key::ArrowRight), Axis(GamepadAxis::LeftStickX, AxisDirection::Positive)]),
            handbrake: InputAction::new(&[K(Key::Space), Button(GamepadButton::South)]),
            recover: InputAction::new(&[K(Key::R), Button(GamepadButton::North)]),
            camera: InputAction::new(&[K(Key::C), Button(GamepadButton::RightThumb)]),
            look_left: InputAction::new(&[Axis(GamepadAxis::RightStickX, AxisDirection::Negative)]),
            look_right: InputAction::new(&[Axis(GamepadAxis::RightStickX, AxisDirection::Positive)]),
            look_up: InputAction::new(&[Axis(GamepadAxis::RightStickY, AxisDirection::Positive)]),
            look_down: InputAction::new(&[Axis(GamepadAxis::RightStickY, AxisDirection::Negative)]),
            look_speed: 150.0,
            zoom_in: InputAction::new(&[Button(GamepadButton::DPadUp)]),
            zoom_out: InputAction::new(&[Button(GamepadButton::DPadDown)]),
            zoom_speed: 1.2,
        }
    }
}

// ********** runtime **********

// A node the controller turns and moves around a center, relative to its rest transform.
#[derive(Clone)]
struct PivotRuntime
{
    node: NodeItem,
    transformation: ComponentItem,
    base_local: Matrix4<f32>,

    center: Vector3<f32>, // parent space
    chassis_to_parent: Matrix3<f32>, // maps chassis space directions into parent space, scale included
    parent_to_chassis: Matrix3<f32>,

    axis: Vector3<f32>, // chassis space, only used by the steering node
}

impl PivotRuntime
{
    // offset and rotation are in chassis space, the rotation turns around the center
    fn apply(&self, offset: Vector3<f32>, rotation: &Rotation3<f32>)
    {
        let rotation_parent = self.chassis_to_parent * rotation.matrix() * self.parent_to_chassis;

        let mut rotation_4 = Matrix4::<f32>::identity();
        rotation_4.fixed_view_mut::<3, 3>(0, 0).copy_from(&rotation_parent);

        let moved = self.center + self.chassis_to_parent * offset;
        let local = Matrix4::new_translation(&moved) * rotation_4 * Matrix4::new_translation(&-self.center) * self.base_local;

        let transformation = &self.transformation;
        component_downcast_mut!(transformation, Transformation);
        transformation.set_local_transform(local);
    }

    fn reset(&self)
    {
        let transformation = &self.transformation;
        component_downcast_mut!(transformation, Transformation);
        transformation.set_local_transform(self.base_local);
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct VehicleWheelTelemetry
{
    pub contact: bool,
    pub compression: f32, // m, positive = compressed beyond the ride height
    pub grip: f32, // surface grip factor
    pub engine_force: f32,
    pub brake: f32,
}

// Current state for debugging, shown in the Telemetry section of the controller.
#[derive(Default, Clone, Debug)]
pub struct VehicleTelemetry
{
    pub speed_kmh: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub drift: f32,
    pub wheelspin: f32,
    pub skid: f32,
    pub lean: f32, // degrees
    pub wheels: Vec<VehicleWheelTelemetry>,
    pub touching: Vec<ContactTarget>, // what the body touches right now, the wheels are rays and never count
    pub last_hit: Option<(ContactTarget, f32)>, // with its impact speed in m/s
    pub hitch: Option<VehicleHitchTelemetry>, // towing or towed
}

#[derive(Clone, Debug)]
pub struct VehicleHitchTelemetry
{
    pub towing: bool, // false: this is the trailer
    pub other: u32, // node id of the trailer or the tow vehicle
    pub state: HitchState,
    pub force: f32, // kN
    pub angles: Vector3<f32>, // degrees: yaw, pitch, roll of the trailer against the tow vehicle
    pub brake: f32,
    pub load: f32, // what the springs carry against the own weight
}

// what the last physics step found under a wheel
#[derive(Clone, Copy, Default)]
struct WheelContact
{
    contact: bool,
    ground: Option<ColliderHandle>,
}

// what drive() hands to rapier for one wheel
#[derive(Clone, Copy)]
struct WheelCommand
{
    engine_force: f32, // N
    brake: f32, // impulse per step
    steering: f32, // rad
    grip: f32,
    side_grip: f32,
}

// one wheel at the pose the node shows
#[derive(Clone, Copy)]
struct WheelVisual
{
    contact: bool,
    suspension_length: f32,
    spin: f32,
    steering: f32,
    ground: Option<(Vector3<f32>, Vector3<f32>)>, // under the wheel as shown, and the ground normal - world space
}

// what drive() leaves for the visuals, the camera and the sound
struct DriveState
{
    speed: f32,
    lateral_left: f32,
    steer_angle: f32, // of the steered wheels, as they are turned
    brake: f32,
    wheelspin: f32,
    lean: f32,
    lean_target: f32,
    telemetry_wheels: Vec<VehicleWheelTelemetry>,
}

// update() drives before the physics step, update_after_physics() shows the result
struct PendingFrame
{
    input: VehicleInput,
    state: DriveState,
}

#[derive(Default, Clone, Copy)]
struct VehicleInput
{
    throttle: f32, // 0..1
    brake: f32, // 0..1
    steer: f32, // -1..1, positive = left
    steer_analog: bool,
    handbrake: bool,
    recover: bool,
    camera: bool,
    look: (f32, f32), // -1..1 each, right and up turn the view right and up
    zoom: f32, // -1..1, positive = closer
}

// ********** controller **********

#[derive(Serialize, Deserialize)]
pub struct VehicleController
{
    base: SceneControllerBase,

    pub node_name: String,
    pub cam_name: String,

    pub vehicle_type: VehicleType,
    pub forward_axis: VehicleForward,
    pub drive: VehicleDrive,

    // measured by the auto setup - None means it never ran
    pub frame: Option<VehicleFrame>,

    pub wheels: Vec<VehicleWheel>,

    pub chassis: VehicleChassisSettings,
    pub suspension: VehicleSuspensionSettings,
    pub tires: VehicleTireSettings,
    pub steering: VehicleSteeringSettings,
    pub brakes: VehicleBrakeSettings,
    pub drift: VehicleDriftSettings,
    pub balance: VehicleBalanceSettings,
    pub tracks: VehicleTrackSettings,
    pub engine: EngineSettings,
    pub camera: VehicleCameraSettings,
    #[serde(default)]
    pub seats: Vec<VehicleSeat>,
    // older scenes had one driver entry - moved into the seats after loading
    #[serde(default, skip_serializing, rename = "driver")]
    legacy_driver: Option<VehicleSeat>,
    pub recover: VehicleRecoverSettings,
    pub sound: VehicleSoundSettings,
    #[serde(default)]
    pub tire_marks: TireMarkSettings,
    #[serde(default)]
    pub controls: VehicleControls,
    #[serde(default)]
    pub hitch: VehicleHitchSettings,

    #[serde(serialize_with = "serialize_node", deserialize_with = "deserialize_node")]
    pub node: OptionOrId<NodeItem>,

    #[serde(default, serialize_with = "serialize_node", deserialize_with = "deserialize_node")]
    pub trailer: OptionOrId<NodeItem>,

    // ********** runtime **********

    // chassis space vertices of everything but the wheels and the driver
    #[serde(skip, default)]
    chassis_points: Vec<Vector3<f32>>,
    // the same points by mesh, for the compound shape
    #[serde(skip, default)]
    chassis_parts: Vec<Vec<Vector3<f32>>>,
    #[serde(skip, default)]
    chassis_bounds: Option<(Vector3<f32>, Vector3<f32>)>,
    #[serde(skip, default)]
    principal_inertia: Vector3<f32>,

    // excluded from the scene colliders by this controller, and not by somebody else before
    #[serde(skip, default)]
    excluded_node_ids: HashSet<u32>,

    #[serde(skip, default)]
    physics_dirty: bool,
    #[serde(skip, default)]
    hitch_dirty: bool, // the coupling is measured and handed to the physics again

    // how far the springs compress under the vehicle's own weight, m
    #[serde(skip, default)]
    sag: f32,

    #[serde(skip, default)]
    engine_state: EngineState,
    #[serde(skip, default)]
    steer: f32,
    #[serde(skip, default)]
    drift_amount: f32, // 1 = handbrake just pulled, falls back to 0 as the grip recovers
    #[serde(skip, default)]
    wheelspin: f32, // 1 = the engine turns the driven wheels against the held handbrake, falls back like the drift
    #[serde(skip, default)]
    lean_goal: f32, // the wanted lean, following the steering at the roll rate
    #[serde(skip, default)]
    upside_down_time: f32,
    #[serde(skip, default)]
    orbit_idle_time: f32, // since the mouse last orbited the camera
    #[serde(skip, default)]
    chase_controller: Option<CameraControllerBox>, // waits here while the cockpit controller is on the camera

    #[serde(skip, default)]
    steering_runtime: Option<PivotRuntime>,
    #[serde(skip, default)]
    seat_runtime: Vec<Option<SeatRuntime>>,

    #[serde(skip, default)]
    sound_player: Option<VehicleSoundPlayer>,

    #[serde(skip, default)]
    tire_mark_pieces: TireMarks,
    #[serde(skip, default)]
    previous_ground: Vec<Option<Vector3<f32>>>, // per wheel, for its speed over the ground

    #[serde(skip, default)]
    pending: Option<PendingFrame>,

    #[serde(skip, default)]
    telemetry: VehicleTelemetry,
}

impl Drop for VehicleController
{
    fn drop(&mut self)
    {
        self.cleanup();
    }
}

impl VehicleController
{
    pub fn default() -> Self
    {
        let mut controller = VehicleController
        {
            base: SceneControllerBase::new("Vehicle Controller".to_string(), "🚗".to_string()),

            node_name: "".to_string(),
            cam_name: "".to_string(),

            vehicle_type: VehicleType::Car,
            forward_axis: VehicleForward::Auto,
            drive: VehicleDrive::Rear,

            frame: None,
            wheels: vec![],

            chassis: VehicleChassisSettings::default(),
            suspension: VehicleSuspensionSettings::default(),
            tires: VehicleTireSettings::default(),
            steering: VehicleSteeringSettings::default(),
            brakes: VehicleBrakeSettings::default(),
            drift: VehicleDriftSettings::default(),
            balance: VehicleBalanceSettings::default(),
            tracks: VehicleTrackSettings::default(),
            engine: EngineSettings::default(),
            camera: VehicleCameraSettings::default(),
            seats: vec![],
            legacy_driver: None,
            recover: VehicleRecoverSettings::default(),
            sound: VehicleSoundSettings::default(),
            tire_marks: TireMarkSettings::default(),
            controls: VehicleControls::default(),
            hitch: VehicleHitchSettings::default(),

            node: OptionOrId::None,
            trailer: OptionOrId::None,

            chassis_points: vec![],
            chassis_parts: vec![],
            chassis_bounds: None,
            principal_inertia: Vector3::zeros(),
            excluded_node_ids: HashSet::new(),
            physics_dirty: false,
            hitch_dirty: false,
            sag: 0.0,

            engine_state: EngineState::default(),
            steer: 0.0,
            drift_amount: 0.0,
            wheelspin: 0.0,
            lean_goal: 0.0,
            upside_down_time: 0.0,
            orbit_idle_time: 0.0,
            chase_controller: None,

            steering_runtime: None,
            seat_runtime: vec![],

            sound_player: None,

            tire_mark_pieces: TireMarks::default(),
            previous_ground: vec![],

            pending: None,

            telemetry: VehicleTelemetry::default(),
        };

        controller.apply_preset();
        controller
    }

    fn node_id(&self) -> Option<u32>
    {
        self.node.as_ref().map(|node| node.read().unwrap().id)
    }

    // ********** setup **********

    pub fn auto_setup(&mut self, scene: &mut Scene, vehicle_node: &str, cam_name: &str) -> Option<String>
    {
        let Some(node) = scene.find_node_by_name(vehicle_node) else
        {
            console_error!("vehicle auto setup failed - node not found");
            return Some("auto setup failed - node not found".to_string());
        };

        // an earlier setup may have turned the wheels
        self.reset_visuals();

        self.node = OptionOrId::Some(node.clone());
        self.node_name = node.read().unwrap().name.clone();

        let world = node.read().unwrap().get_full_transform();
        let rotation = extract_rotation_quat_from_transform(&world);
        let to_chassis = Self::chassis_inverse(&world);

        // ********** seats **********
        let seat_nodes = self.find_seat_nodes(scene);
        let driver = self.driver_seat().and_then(|index| seat_nodes[index].clone());

        // everybody on board is left out of the vehicle geometry
        let driver_ids: HashSet<u32> = seat_nodes.iter().flatten().flat_map(|rider| Self::subtree_ids(rider)).collect();

        // ********** geometry **********
        let up = (rotation.inverse() * Vector3::y()).normalize();
        let mesh_bounds = Self::mesh_bounds(&node, &to_chassis, &driver_ids);

        let Some(vehicle_bounds) = Self::union_bounds(mesh_bounds.values().map(|(bounds, _)| *bounds)) else
        {
            console_error!("vehicle auto setup failed - the vehicle has no meshes");
            return Some("auto setup failed - the vehicle has no meshes".to_string());
        };

        // ********** wheels **********
        let wheel_nodes = self.detect_wheels(&node, &mesh_bounds, &vehicle_bounds, &up);

        let mut wheels: Vec<(NodeItem, Vector3<f32>, f32)> = vec![];
        let mut extents = vec![];
        for wheel_node in &wheel_nodes
        {
            let ids = Self::subtree_ids(wheel_node);
            let Some((min, max)) = Self::union_bounds(mesh_bounds.iter().filter(|(id, _)| ids.contains(id)).map(|(_, (bounds, _))| *bounds)) else { continue; };

            let extent = max - min;
            let radius = extent.dot(&up.abs()) * 0.5;

            wheels.push((wheel_node.clone(), (min + max) * 0.5, radius.max(0.01)));
            extents.push(extent);
        }

        // ********** forward **********
        let forward = self.detect_forward(&rotation, &up, &wheels);
        let frame = VehicleFrame { forward, up };
        self.frame = Some(frame);

        self.wheels = wheels.iter().zip(extents.iter()).map(|((wheel_node, center, radius), extent)|
        {
            let mut wheel = VehicleWheel::new(&wheel_node.read().unwrap().name, *center, *radius);
            wheel.node_uuid = wheel_node.read().unwrap().uuid.clone();
            wheel.width = extent.dot(&frame.right().abs());
            wheel
        }).collect();
        self.assign_wheel_roles();

        console_log!("vehicle setup: {} wheels found", self.wheels.len());
        if self.wheels.is_empty()
        {
            console_warning!("vehicle setup: no wheels found - add them in the wheel list");
        }

        // ********** chassis **********
        let wheel_ids: HashSet<u32> = wheel_nodes.iter().flat_map(|wheel_node| Self::subtree_ids(wheel_node)).collect();
        self.collect_chassis_points(&node, &to_chassis, &wheel_ids, &driver_ids);

        // ********** steering node **********
        if self.steering.node_name.is_empty()
        {
            let regex = Regex::new(STEERING_NAME_REGEX).unwrap();

            if let Some(steering_node) = Scene::list_all_child_nodes(&node.read().unwrap().nodes).into_iter().find(|child| Self::names_of(child).iter().any(|name| regex.is_match(name)))
            {
                self.steering.node_name = steering_node.read().unwrap().name.clone();
                self.steering.node_uuid = steering_node.read().unwrap().uuid.clone();
            }
        }

        // ********** seat positions **********
        for (seat, rider) in self.seats.iter_mut().zip(seat_nodes.iter())
        {
            if let (Some(rider), true) = (rider.as_ref(), seat.seat_auto)
            {
                let world_rider = rider.read().unwrap().get_full_transform();
                seat.seat_position = (to_chassis * world_rider.column(3)).xyz();
            }
        }

        // ********** camera **********
        let (min, max) = self.chassis_bounds.unwrap_or((vehicle_bounds.0, vehicle_bounds.1));
        let points_of = |node: &NodeItem| -> Vec<Vector3<f32>> { Self::mesh_bounds(node, &to_chassis, &HashSet::new()).into_values().flat_map(|(_, points)| points).collect() };
        let driver_points: Vec<Vector3<f32>> = driver.as_ref().map(&points_of).unwrap_or_default();
        let rider_points: Vec<Vector3<f32>> = seat_nodes.iter().flatten().flat_map(&points_of).collect();

        // a rider on top counts - the chase cam looks over the helmet like over the roof of a car
        let (top_min, top_max) = Self::bounds_of(&rider_points).map(|(low, high)| (min.inf(&low), max.sup(&high))).unwrap_or((min, max));
        let height = (top_max - top_min).dot(&up.abs());
        let mut length = (max - min).dot(&forward.abs());

        // the chase camera looks over the trailer too
        if let Some(trailer) = self.trailer.as_ref()
        {
            let along: Vec<f32> = PhysicsWorld::collect_points(&[trailer.clone()], &to_chassis).iter().map(|point| point.dot(&forward)).collect();
            let rear = along.iter().copied().fold(min.dot(&forward).min(max.dot(&forward)), f32::min);
            length = length.max(min.dot(&forward).max(max.dot(&forward)) - rear);
        }

        if self.camera.distance_auto
        {
            self.camera.distance = (length * 1.3 + 2.5).max(3.0);
            self.camera.height = height * 0.6;
        }

        if self.camera.cockpit_auto
        {
            // the eyes of the driver: in front of the top of its head, without a driver mesh above the seat
            let top = driver_points.iter().map(|point| point.dot(&up)).fold(f32::MIN, f32::max);
            let head: Vec<Vector3<f32>> = driver_points.iter().filter(|point| point.dot(&up) > top - HEAD_SIZE).copied().collect();

            self.camera.cockpit_offset = match Self::bounds_of(&head)
            {
                Some((low, high)) => (low + high) * 0.5 + forward * HEAD_SIZE * 0.5,
                None => self.driver_seat().map(|index| self.seats[index].seat_position).unwrap_or_default() + up * (height * 0.45).max(0.6),
            };
        }

        // a trailer has no camera of its own - an empty name would take the main camera
        if !self.vehicle_type.is_trailer()
        {
            match self.setup_camera(scene, &node, cam_name)
            {
                Some(error) => return Some(error),
                None => {}
            }
        }

        self.setup_runtime(scene);

        None
    }

    fn setup_camera(&mut self, scene: &mut Scene, node: &NodeItem, cam_name: &str) -> Option<String>
    {
        // never the editor cam - its controller must stay untouched
        let Some(cam) = scene.get_game_camera_mut(cam_name) else
        {
            console_error!("vehicle auto setup failed - camera not found");
            return Some("auto setup failed - camera not found".to_string());
        };

        cam.node = OptionOrId::Some(node.clone());
        self.cam_name = cam.name.clone();

        cam.controller = Some(Box::new(self.new_chase_controller(node)));

        self.chase_controller = None;

        None
    }

    // starts behind the vehicle
    fn new_chase_controller(&self, node: &NodeItem) -> TargetRotationController
    {
        let alpha = self.frame.map(|frame|
        {
            let rotation = extract_rotation_quat_from_transform(&node.read().unwrap().get_full_transform());
            yaw_pitch_from_direction(rotation * frame.forward).0 + PI
        }).unwrap_or(0.0);

        let mut controller = TargetRotationController::default();
        controller.data.get_mut().alpha = alpha;
        controller.data.get_mut().beta = DEFAULT_CAM_PITCH.to_radians();
        controller.data.get_mut().radius = self.camera.distance;
        controller.follow_smoothing = 0.03;
        controller.follow_smoothing_vertical = 0.08;
        controller.collision_check = true;
        controller.use_bbox_center = false;
        controller.min_radius = 0.0;

        controller
    }

    // the first seat with the driver role
    fn driver_seat(&self) -> Option<usize>
    {
        self.seats.iter().position(|seat| seat.role == SeatRole::Driver)
    }

    fn seated_nodes(&self) -> impl Iterator<Item = &NodeItem>
    {
        self.seat_runtime.iter().flatten().map(|runtime| &runtime.node)
    }

    // by the index of the seat
    fn find_seat_nodes(&self, scene: &Scene) -> Vec<Option<NodeItem>>
    {
        self.seats.iter().map(|seat| if seat.node_name.is_empty() { None } else { scene.find_node_by_name(&seat.node_name) }).collect()
    }

    // who sits there hides while the camera is in the cockpit
    fn set_cockpit_visibility(&self, cockpit: bool)
    {
        for (seat, runtime) in self.seats.iter().zip(self.seat_runtime.iter())
        {
            if let (true, Some(runtime)) = (seat.hide_in_cockpit, runtime.as_ref())
            {
                runtime.node.write().unwrap().settings.visible = !cockpit;
            }
        }
    }

    fn find_sit_animation(node: &NodeItem, regex: &str) -> Option<ComponentItem>
    {
        if regex.is_empty() || Regex::new(regex).is_err()
        {
            return None;
        }

        node.read().unwrap().find_animation_by_regex(regex)
    }

    // Everything that is not saved: node lookups, the pivots, the physics body.
    fn setup_runtime(&mut self, scene: &mut Scene)
    {
        let Some(node) = self.node.as_ref().cloned() else { return; };
        let Some(frame) = self.frame else { return; };

        // the pivots take the current transforms as their rest pose - a turned or sprung wheel must not become it
        self.reset_visuals();

        let world = node.read().unwrap().get_full_transform();
        let chassis_world = Self::chassis_matrix(&world);
        let to_chassis = Self::chassis_inverse(&world);

        let children = Scene::list_all_child_nodes(&node.read().unwrap().nodes);

        // ********** wheels **********
        for wheel in &mut self.wheels
        {
            let wheel_node = Self::find_part(&children, &wheel.node_uuid, &wheel.node_name, Some((&to_chassis, wheel.center)));

            if let Some(wheel_node) = wheel_node.as_ref()
            {
                wheel.node_uuid = wheel_node.read().unwrap().uuid.clone();
            }

            wheel.runtime = wheel_node.and_then(|wheel_node| Self::pivot_runtime(&wheel_node, &chassis_world, &wheel.center, Vector3::zeros()));

            if wheel.runtime.is_none()
            {
                console_warning!("vehicle: wheel node '{}' not found below '{}'", wheel.node_name, self.node_name);
            }
        }

        // ********** steering node **********
        self.steering_runtime = None;
        if !self.steering.node_name.is_empty()
        {
            if let Some(steering_node) = Self::find_part(&children, &self.steering.node_uuid, &self.steering.node_name, None)
            {
                self.steering.node_uuid = steering_node.read().unwrap().uuid.clone();

                let center = Self::subtree_bounds(&steering_node, &to_chassis).map(|(min, max)| (min + max) * 0.5).unwrap_or_else(|| (to_chassis * steering_node.read().unwrap().get_full_transform().column(3)).xyz());

                let axis = match self.steering.axis
                {
                    SteeringAxis::Handlebar => frame.up,
                    SteeringAxis::Column => Self::column_axis(&steering_node, &world, &frame),
                };

                self.steering_runtime = Self::pivot_runtime(&steering_node, &chassis_world, &center, axis);
            }
        }

        // ********** seats **********
        let seat_nodes = self.find_seat_nodes(scene);
        self.seat_runtime = seat_nodes.into_iter().zip(self.seats.iter()).map(|(node, seat)|
        {
            if node.is_none() && !seat.node_name.is_empty()
            {
                console_warning!("vehicle: seat node '{}' not found", seat.node_name);
            }

            node.map(|node| SeatRuntime { animation: Self::find_sit_animation(&node, &seat.animation), node })
        }).collect();

        // ********** colliders **********
        let mut ids = Self::subtree_ids(&node);
        for rider in self.seated_nodes()
        {
            ids.extend(Self::subtree_ids(rider));
        }

        // what an earlier setup excluded and no longer belongs to the vehicle - e.g. a former driver - collides again
        let released: HashSet<u32> = self.excluded_node_ids.difference(&ids).copied().collect();
        scene.physics.include_nodes(&released);

        let new_ids: HashSet<u32> = ids.into_iter().filter(|id| !scene.physics.is_excluded(*id) || self.excluded_node_ids.contains(id)).collect();
        scene.physics.exclude_nodes(&new_ids);
        self.excluded_node_ids = new_ids;

        // the chassis points are not saved - measured again from the rest pose
        if self.chassis_points.is_empty()
        {
            let wheel_ids: HashSet<u32> = self.wheels.iter().filter_map(|wheel| wheel.runtime.as_ref()).flat_map(|runtime| Self::subtree_ids(&runtime.node)).collect();
            let driver_ids: HashSet<u32> = self.seated_nodes().flat_map(|rider| Self::subtree_ids(rider)).collect();

            self.collect_chassis_points(&node, &to_chassis, &wheel_ids, &driver_ids);
        }

        // without colliders the wheels find no ground
        if scene.physics.is_empty()
        {
            let colliders = scene.build_physics();
            console_log!("vehicle controller: built {} scene colliders", colliders);
        }

        self.engine_state.reset(&self.engine);
        self.hitch_dirty = true;
        self.build_physics(scene);
    }

    fn pivot_runtime(pivot_node: &NodeItem, chassis_world: &Matrix4<f32>, center_chassis: &Vector3<f32>, axis: Vector3<f32>) -> Option<PivotRuntime>
    {
        let transformation =
        {
            let mut pivot = pivot_node.write().unwrap();

            if pivot.find_component::<Transformation>().is_none()
            {
                pivot.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Transformation")))));
            }

            pivot.find_component::<Transformation>()?
        };

        let base_local =
        {
            component_downcast!(transformation, Transformation);
            *transformation.get_transform()
        };

        let parent_world = pivot_node.read().unwrap().parent.as_ref().map(|parent| parent.read().unwrap().get_full_transform()).unwrap_or_else(Matrix4::identity);
        let parent_inverse = parent_world.try_inverse()?;

        let to_parent = parent_inverse * chassis_world;

        let chassis_to_parent: Matrix3<f32> = to_parent.fixed_view::<3, 3>(0, 0).into_owned();
        let parent_to_chassis = chassis_to_parent.try_inverse()?;

        let center = (to_parent * center_chassis.push(1.0)).xyz();

        Some(PivotRuntime { node: pivot_node.clone(), transformation, base_local, center, chassis_to_parent, parent_to_chassis, axis })
    }

    // the local axis of the steering wheel that points along the vehicle, turned toward the driver
    fn column_axis(steering_node: &NodeItem, vehicle_world: &Matrix4<f32>, frame: &VehicleFrame) -> Vector3<f32>
    {
        let steering_world = steering_node.read().unwrap().get_full_transform();
        let to_chassis = extract_rotation_quat_from_transform(vehicle_world).inverse() * extract_rotation_quat_from_transform(&steering_world);

        let axes = [to_chassis * Vector3::x(), to_chassis * Vector3::y(), to_chassis * Vector3::z()];
        let axis = axes.iter().max_by(|a, b| a.dot(&frame.forward).abs().total_cmp(&b.dot(&frame.forward).abs())).copied().unwrap_or(-frame.forward);

        if axis.dot(&frame.forward) > 0.0 { -axis } else { axis }
    }

    // ********** geometry helpers **********

    // the world transform without its scale
    fn chassis_matrix(world: &Matrix4<f32>) -> Matrix4<f32>
    {
        let rotation = extract_rotation_quat_from_transform(world);
        let translation = extract_translation_from_transform(world);

        Matrix4::new_translation(&translation) * rotation.to_homogeneous()
    }

    fn chassis_inverse(world: &Matrix4<f32>) -> Matrix4<f32>
    {
        Self::chassis_matrix(world).try_inverse().unwrap_or_else(Matrix4::identity)
    }

    fn subtree_ids(node: &NodeItem) -> HashSet<u32>
    {
        let mut ids: HashSet<u32> = Scene::list_all_child_nodes(&node.read().unwrap().nodes).iter().map(|child| child.read().unwrap().id).collect();
        ids.insert(node.read().unwrap().id);
        ids
    }

    // chassis space bounds of all meshes in the subtree
    fn subtree_bounds(node: &NodeItem, to_chassis: &Matrix4<f32>) -> Option<(Vector3<f32>, Vector3<f32>)>
    {
        Self::union_bounds(Self::mesh_bounds(node, to_chassis, &HashSet::new()).into_values().map(|(bounds, _)| bounds))
    }

    // A part of the vehicle by its uuid, else by its name - of several with that name the one whose meshes sit closest to the hint (chassis space).
    fn find_part(children: &[NodeItem], uuid: &str, name: &str, hint: Option<(&Matrix4<f32>, Vector3<f32>)>) -> Option<NodeItem>
    {
        if !uuid.is_empty()
        {
            if let Some(part) = children.iter().find(|child| child.read().unwrap().uuid == uuid)
            {
                return Some(part.clone());
            }
        }

        let named: Vec<&NodeItem> = children.iter().filter(|child| Self::names_of(child).iter().any(|child_name| child_name == name)).collect();

        let Some((to_chassis, center)) = hint.filter(|_| named.len() > 1) else { return named.first().map(|part| (*part).clone()); };

        let distance = |part: &NodeItem| Self::subtree_bounds(part, to_chassis).map(|(min, max)| ((min + max) * 0.5 - center).norm()).unwrap_or(f32::MAX);
        named.into_iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).cloned()
    }

    // chassis space bounds and vertices of every mesh node below the vehicle, by node id
    fn mesh_bounds(node: &NodeItem, to_chassis: &Matrix4<f32>, skip: &HashSet<u32>) -> HashMap<u32, ((Vector3<f32>, Vector3<f32>), Vec<Vector3<f32>>)>
    {
        let mut nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes);
        nodes.push(node.clone());

        let mut result = HashMap::new();

        for mesh_node in nodes
        {
            let id = mesh_node.read().unwrap().id;
            if skip.contains(&id)
            {
                continue;
            }

            let points = PhysicsWorld::collect_points(&[mesh_node], to_chassis);

            if let Some(bounds) = Self::bounds_of(&points)
            {
                result.insert(id, (bounds, points));
            }
        }

        result
    }

    fn bounds_of(points: &[Vector3<f32>]) -> Option<(Vector3<f32>, Vector3<f32>)>
    {
        let first = points.first()?;
        let mut min = *first;
        let mut max = *first;

        for point in points
        {
            min = min.inf(point);
            max = max.sup(point);
        }

        Some((min, max))
    }

    fn union_bounds(bounds: impl Iterator<Item = (Vector3<f32>, Vector3<f32>)>) -> Option<(Vector3<f32>, Vector3<f32>)>
    {
        bounds.reduce(|(min_a, max_a), (min_b, max_b)| (min_a.inf(&min_b), max_a.sup(&max_b)))
    }

    fn collect_chassis_points(&mut self, node: &NodeItem, to_chassis: &Matrix4<f32>, wheel_ids: &HashSet<u32>, driver_ids: &HashSet<u32>)
    {
        let skip: HashSet<u32> = wheel_ids.union(driver_ids).copied().collect();
        let meshes = Self::mesh_bounds(node, to_chassis, &skip);

        self.chassis_parts = meshes.into_values().map(|(_, points)| points).collect();
        self.chassis_points = self.chassis_parts.concat();
        self.chassis_bounds = Self::bounds_of(&self.chassis_points);
    }

    // Wheels by name first, by shape when the names say nothing. Only the top-most node of a wheel counts.
    fn detect_wheels(&self, node: &NodeItem, mesh_bounds: &HashMap<u32, ((Vector3<f32>, Vector3<f32>), Vec<Vector3<f32>>)>, vehicle_bounds: &(Vector3<f32>, Vector3<f32>), up: &Vector3<f32>) -> Vec<NodeItem>
    {
        let include = Regex::new(WHEEL_NAME_REGEX).unwrap();
        let exclude = Regex::new(WHEEL_NAME_EXCLUDE_REGEX).unwrap();

        let subtree_bounds = |candidate: &NodeItem| -> Option<(Vector3<f32>, Vector3<f32>)>
        {
            let ids = Self::subtree_ids(candidate);
            Self::union_bounds(mesh_bounds.iter().filter(|(id, _)| ids.contains(id)).map(|(_, (bounds, _))| *bounds))
        };

        let by_name = |candidate: &NodeItem| -> bool
        {
            let names = Self::names_of(candidate);
            names.iter().any(|name| include.is_match(name)) && !names.iter().any(|name| exclude.is_match(name)) && subtree_bounds(candidate).is_some()
        };

        let vehicle_height = (vehicle_bounds.1 - vehicle_bounds.0).dot(&up.abs());
        let vehicle_bottom = vehicle_bounds.0.dot(&up.abs()).min(vehicle_bounds.1.dot(&up.abs()));
        let up_index = up.iamax();

        let by_shape = |candidate: &NodeItem| -> bool
        {
            let Some((min, max)) = subtree_bounds(candidate) else { return false; };
            let extent = max - min;

            let diameter = extent[up_index];
            let horizontal: Vec<f32> = (0..3).filter(|i| *i != up_index).map(|i| extent[i]).collect();
            let (long, short) = (horizontal[0].max(horizontal[1]), horizontal[0].min(horizontal[1]));

            let bottom = min[up_index].min(max[up_index]);

            diameter > vehicle_height * 0.1 && diameter < vehicle_height * 0.8
                && long >= diameter * WHEEL_ROUNDNESS_MIN && long <= diameter * WHEEL_ROUNDNESS_MAX
                && short < diameter * WHEEL_MAX_WIDTH_RATIO
                && bottom < vehicle_bottom + vehicle_height * WHEEL_BOTTOM_ZONE
        };

        let top_level = node.read().unwrap().nodes.clone();

        let mut found = vec![];
        Self::collect_top_most(&top_level, &by_name, &mut found);

        if found.len() < 2
        {
            found.clear();
            Self::collect_top_most(&top_level, &by_shape, &mut found);
        }

        found
    }

    // the node name plus its mesh names - scenes saved before the loader named nodes after the gltf node refer to the mesh (e.g. 'Cube.373')
    fn names_of(node: &NodeItem) -> Vec<String>
    {
        let node = node.read().unwrap();
        let mut names = vec![node.name.clone()];

        names.extend(node.find_components::<Mesh>().iter().map(|mesh| mesh.read().unwrap().get_base().name.clone()));

        names
    }

    fn collect_top_most(nodes: &Vec<NodeItem>, predicate: &dyn Fn(&NodeItem) -> bool, found: &mut Vec<NodeItem>)
    {
        for child in nodes
        {
            if predicate(child)
            {
                found.push(child.clone());
                continue;
            }

            let children = child.read().unwrap().nodes.clone();
            Self::collect_top_most(&children, predicate, found);
        }
    }

    fn detect_forward(&self, rotation: &UnitQuaternion<f32>, up: &Vector3<f32>, wheels: &[(NodeItem, Vector3<f32>, f32)]) -> Vector3<f32>
    {
        let horizontal = |v: Vector3<f32>| -> Option<Vector3<f32>>
        {
            let v = v - up * v.dot(up);
            if v.norm() > 0.0001 { Some(v.normalize()) } else { None }
        };

        let world_axis = match self.forward_axis
        {
            VehicleForward::WorldPosZ => Some(Vector3::z()),
            VehicleForward::WorldNegX => Some(-Vector3::x()),
            VehicleForward::WorldPosX => Some(Vector3::x()),
            VehicleForward::WorldNegZ => Some(-Vector3::z()),
            VehicleForward::Auto => None,
        };

        if let Some(axis) = world_axis
        {
            if let Some(forward) = horizontal(rotation.inverse() * axis)
            {
                return forward;
            }
        }

        // ********** from the wheel names **********
        if self.forward_axis == VehicleForward::Auto
        {
            let front = Regex::new(FRONT_NAME_REGEX).unwrap();
            let back = Regex::new(BACK_NAME_REGEX).unwrap();

            let average = |regex: &Regex| -> Option<Vector3<f32>>
            {
                let centers: Vec<Vector3<f32>> = wheels.iter().filter(|(wheel, _, _)| Self::names_of(wheel).iter().any(|name| regex.is_match(name))).map(|(_, center, _)| *center).collect();
                if centers.is_empty() { None } else { Some(centers.iter().sum::<Vector3<f32>>() / centers.len() as f32) }
            };

            if let (Some(front), Some(back)) = (average(&front), average(&back))
            {
                if let Some(forward) = horizontal(front - back)
                {
                    return forward;
                }
            }
        }

        // ********** from the wheel layout **********
        // the axis the wheels spread along, in model space - never the world direction the vehicle is turned to (gltf front: +z)
        let spread = |axis: &Vector3<f32>| -> f32
        {
            let along: Vec<f32> = wheels.iter().map(|(_, center, _)| center.dot(axis)).collect();
            along.iter().cloned().fold(f32::MIN, f32::max) - along.iter().cloned().fold(f32::MAX, f32::min)
        };

        let world_forward = horizontal(rotation.inverse() * -Vector3::z()).unwrap_or(-Vector3::z());
        let model_axes: Vec<Vector3<f32>> = [Vector3::z(), Vector3::x(), Vector3::y()].into_iter().filter_map(horizontal).filter(|axis| axis.dot(up).abs() < 0.5).collect();

        let axis = if wheels.len() >= 2
        {
            model_axes.iter().cloned().max_by(|a, b| spread(a).total_cmp(&spread(b)))
        }
        else
        {
            model_axes.iter().cloned().max_by(|a, b| a.dot(&world_forward).abs().total_cmp(&b.dot(&world_forward).abs()))
        };

        match axis
        {
            Some(axis) if (axis - Vector3::z()).norm() < 0.01 => axis,
            Some(axis) => if axis.dot(&world_forward) < 0.0 { -axis } else { axis },
            None => world_forward,
        }
    }

    // For a vehicle node scaled by factor: lengths follow, mass, forces and engine as for a true model - same speeds and accelerations.
    pub fn scale_vehicle(&mut self, factor: f32)
    {
        for wheel in &mut self.wheels
        {
            wheel.center *= factor;
            wheel.radius *= factor;
        }

        self.chassis.center_of_mass *= factor;
        self.camera.distance *= factor;
        self.camera.height *= factor;
        self.camera.cockpit_offset *= factor;
        for seat in &mut self.seats
        {
            seat.seat_position *= factor;
        }
        self.recover.lift *= factor;
        self.chassis.rounding *= factor;
        self.hitch.point *= factor;
        self.hitch_dirty = true;

        self.scale_physics(factor);
    }

    // Mass, suspension, brakes and engine for a vehicle factor times the size they were made for - the geometry stays.
    pub fn scale_physics(&mut self, factor: f32)
    {
        let volume = factor * factor * factor;

        self.chassis.mass *= volume;

        self.suspension.rest_length *= factor;
        self.suspension.travel *= factor;
        self.suspension.stiffness /= factor;

        // the spring frequency goes with 1 / sqrt(factor), the damping follows so the damping ratio stays
        self.suspension.compression /= factor.sqrt();
        self.suspension.relaxation /= factor.sqrt();

        self.brakes.brake_force *= volume;
        self.brakes.handbrake_force *= volume;
        self.brakes.rolling_resistance *= volume;
        self.brakes.air_drag *= factor * factor;

        // the wheel force keeps its share of the weight, the gearing keeps the speeds
        self.engine.final_drive *= factor;
        self.engine.max_torque *= volume;

        self.mark_physics_dirty();
    }

    // the length of the set up vehicle along its forward axis, m
    pub fn measured_length(&self) -> Option<f32>
    {
        let frame = self.frame?;
        let (min, max) = self.chassis_bounds?;
        let length = (max - min).dot(&frame.forward.abs());
        if length > 0.01 { Some(length) } else { None }
    }

    // the preset of the type, scaled to the measured size - replaces the current values like Apply Preset
    pub fn apply_preset_for_size(&mut self) -> Option<f32>
    {
        let factor = (self.measured_length()? / self.vehicle_type.reference_length()).clamp(0.02, 50.0);
        self.apply_preset();
        self.scale_physics(factor);
        Some(factor)
    }

    // Steering, drive and handbrake by axle, from the vehicle type and the drive.
    pub fn assign_wheel_roles(&mut self)
    {
        let Some(frame) = self.frame else { return; };

        let mut axles: Vec<f32> = vec![];
        for wheel in &self.wheels
        {
            let along = wheel.center.dot(&frame.forward);
            let tolerance = AXLE_TOLERANCE.max(wheel.radius * 0.5);

            if !axles.iter().any(|axle| (axle - along).abs() < tolerance)
            {
                axles.push(along);
            }
        }

        axles.sort_by(|a, b| b.total_cmp(a)); // front first

        let axle_of = |wheel: &VehicleWheel| -> usize
        {
            let along = wheel.center.dot(&frame.forward);
            axles.iter().enumerate().min_by(|(_, a), (_, b)| (*a - along).abs().total_cmp(&(*b - along).abs())).map(|(index, _)| index).unwrap_or(0)
        };

        let axle_amount = axles.len();
        let vehicle_type = self.vehicle_type;
        let drive = self.drive;

        for index in 0..self.wheels.len()
        {
            let axle = axle_of(&self.wheels[index]);
            let front = axle == 0;
            let rear = axle + 1 == axle_amount;

            let wheel = &mut self.wheels[index];

            wheel.steer = match (vehicle_type, drive)
            {
                (_, VehicleDrive::Tracked) | (VehicleType::Tank, _) | (VehicleType::Trailer, _) => 0.0,
                (VehicleType::MultiAxle, _) if axle_amount >= 3 && axle == 1 => 0.6,
                _ if front && axle_amount > 1 => 1.0,
                _ => 0.0,
            };

            wheel.driven = match drive
            {
                _ if vehicle_type.is_trailer() => false,
                _ if axle_amount <= 1 => true,
                VehicleDrive::Front => front,
                VehicleDrive::Rear => rear,
                VehicleDrive::All | VehicleDrive::Tracked => true,
            };

            // a trailer parks on all its wheels
            wheel.handbrake = match drive
            {
                _ if vehicle_type.is_trailer() => true,
                VehicleDrive::Tracked => true,
                _ => rear || axle_amount <= 1,
            };
        }
    }

    // ********** physics **********

    fn build_physics(&mut self, scene: &mut Scene)
    {
        let Some(node) = self.node.as_ref().cloned() else { return; };
        let Some(frame) = self.frame else { return; };

        self.physics_dirty = false;

        let (mut min, mut max) = self.chassis_bounds.unwrap_or((Vector3::new(-0.8, 0.3, -2.0), Vector3::new(0.8, 1.4, 2.0)));

        // a body reaching down to the ground would catch on every edge before the wheel rays see it
        let floor = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.up) - wheel.radius * (1.0 - self.chassis.min_clearance)).fold(f32::MIN, f32::max);
        let floor = if self.wheels.is_empty() { f32::MIN } else { floor };
        let raise = |point: Vector3<f32>| -> Vector3<f32> { point + frame.up * (floor - point.dot(&frame.up)).max(0.0) };

        let (a, b) = (raise(min), raise(max));
        (min, max) = (a.inf(&b), a.sup(&b));

        let center = (min + max) * 0.5;
        let extent = (max - min).map(|e| e.max(0.05));

        // rounded edges slide up a ramp start or a loop entry instead of catching on it - at most half the smallest half extent
        let rounding = self.chassis.rounding.clamp(0.0, extent.min() * 0.25);
        let to_vector = |p: &Vector3<f32>| Vector::new(p.x, p.y, p.z);

        let box_shape = ||
        {
            let half = extent * 0.5 - Vector3::repeat(rounding);
            let cuboid = if rounding > 0.0 { SharedShape::round_cuboid(half.x, half.y, half.z, rounding) } else { SharedShape::cuboid(half.x, half.y, half.z) };
            SharedShape::compound(vec![(Pose::from_translation(to_vector(&center)), cuboid)])
        };

        // the points are pulled in by the radius first, so the rounded body keeps its outer size
        let hull = |points: &[Vector3<f32>]| -> Option<SharedShape>
        {
            if rounding <= 0.0
            {
                return SharedShape::convex_hull(&points.iter().map(to_vector).collect::<Vec<_>>());
            }

            let (low, high) = Self::bounds_of(points)?;
            let middle = (low + high) * 0.5;
            let pull = ((high - low) * 0.5).map(|half| (half - rounding).max(0.0) / half.max(rounding * 2.0));
            let pulled: Vec<Vector> = points.iter().map(|p| middle + (p - middle).component_mul(&pull)).map(|p| to_vector(&p)).collect();

            SharedShape::round_convex_hull(&pulled, rounding)
        };

        // ********** approach and departure angle **********
        // in front of the front axle and behind the rear axle the bottom rises above the axles, like a real body does
        let along: Vec<f32> = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.forward)).collect();
        let axle_height = if self.wheels.is_empty() { f32::MIN } else { self.wheels.iter().map(|wheel| wheel.center.dot(&frame.up)).sum::<f32>() / self.wheels.len() as f32 };
        let (front_axle, rear_axle) = (along.iter().copied().fold(f32::MIN, f32::max), along.iter().copied().fold(f32::MAX, f32::min));

        let bottom = min.dot(&frame.up.abs()).min(max.dot(&frame.up.abs()));
        let (front_end, rear_end) = (max.dot(&frame.forward).max(min.dot(&frame.forward)), max.dot(&frame.forward).min(min.dot(&frame.forward)));

        let wheel_radius = if self.wheels.is_empty() { 0.0 } else { self.wheels.iter().map(|wheel| wheel.radius).sum::<f32>() / self.wheels.len() as f32 };
        let end_bottom = axle_height.max(bottom) + wheel_radius * SLOPED_END_LIFT;

        let bevel = |point: Vector3<f32>| -> Vector3<f32>
        {
            let point = raise(point);

            if !self.chassis.sloped_ends || self.wheels.is_empty() || end_bottom <= bottom
            {
                return point;
            }

            let position = point.dot(&frame.forward);
            let share = if position > front_axle && front_end > front_axle { (position - front_axle) / (front_end - front_axle) }
                else if position < rear_axle && rear_end < rear_axle { (rear_axle - position) / (rear_axle - rear_end) }
                else { 0.0 };

            let lowest = bottom + (end_bottom - bottom) * share.clamp(0.0, 1.0);
            point + frame.up * (lowest - point.dot(&frame.up)).max(0.0)
        };

        let sloped_box = ||
        {
            let mut corners = vec![];
            for x in [min.x, max.x] { for y in [min.y, max.y] { for z in [min.z, max.z] { corners.push(Vector3::new(x, y, z)); } } }

            // the flat bottom ends at the axles
            let bottom_corners: Vec<Vector3<f32>> = corners.iter().filter(|corner| (corner.dot(&frame.up.abs()) - bottom).abs() < 0.0001).copied().collect();
            for corner in &bottom_corners
            {
                for axle in [front_axle, rear_axle]
                {
                    corners.push(corner + frame.forward * (axle - corner.dot(&frame.forward)));
                }
            }

            let points: Vec<Vector3<f32>> = corners.into_iter().map(|p| bevel(p)).collect();
            hull(&points).unwrap_or_else(box_shape)
        };

        let shape = match self.chassis.shape
        {
            ChassisShape::Box if self.chassis.sloped_ends && !self.wheels.is_empty() => sloped_box(),
            ChassisShape::Box => box_shape(),
            ChassisShape::ConvexHull =>
            {
                let points: Vec<Vector3<f32>> = self.chassis_points.iter().map(|p| bevel(*p)).collect();
                hull(&points).unwrap_or_else(||
                {
                    console_warning!("vehicle: convex hull failed, using a box");
                    if self.chassis.sloped_ends && !self.wheels.is_empty() { sloped_box() } else { box_shape() }
                })
            }
            ChassisShape::Compound =>
            {
                // sharp hulls: rounding would flatten thin parts like a deck or a post to nothing
                let parts: Vec<(Pose, SharedShape)> = self.chassis_parts.iter()
                    .filter_map(|points| SharedShape::convex_hull(&points.iter().map(|p| to_vector(&bevel(*p))).collect::<Vec<_>>()))
                    .map(|shape| (Pose::IDENTITY, shape))
                    .collect();

                if parts.is_empty()
                {
                    console_warning!("vehicle: no convex part for the compound, using a box");
                    if self.chassis.sloped_ends && !self.wheels.is_empty() { sloped_box() } else { box_shape() }
                }
                else
                {
                    SharedShape::compound(parts)
                }
            }
        };

        let mass = self.chassis.mass.max(1.0);

        if self.chassis.center_of_mass_auto && self.vehicle_type.is_trailer()
        {
            // a trailer leans on its hitch: along it the middle of the body volume, not of the wheels - a light drawbar barely counts
            // a compound counts its whole outline like a hull - its thin walls alone would shift the middle to the drawbar
            let outline = if self.chassis.shape == ChassisShape::Compound { hull(&self.chassis_points.iter().map(|p| bevel(*p)).collect::<Vec<_>>()) } else { None };
            let centroid = outline.as_ref().unwrap_or(&shape).mass_properties(1.0).local_com;
            let centroid = Vector3::new(centroid.x, centroid.y, centroid.z);
            let height = extent.dot(&frame.up.abs());
            let com = center + frame.up * (height * (self.chassis.center_of_mass_height - 0.5));

            self.chassis.center_of_mass = centroid + frame.up * (com - centroid).dot(&frame.up);
        }
        else if self.chassis.center_of_mass_auto
        {
            let height = extent.dot(&frame.up.abs());
            let com = center + frame.up * (height * (self.chassis.center_of_mass_height - 0.5));

            // near the middle of the wheels - a long gun barrel must not tip the vehicle onto its nose
            let centers: Vec<Vector3<f32>> = self.wheels.iter().map(|wheel| wheel.center).collect();
            self.chassis.center_of_mass = match Self::bounds_of(&centers)
            {
                Some((low, high)) =>
                {
                    let middle = (low + high) * 0.5;
                    let offset = com - middle;

                    // tracks: right over their middle, so the vehicle turns on the spot
                    let share = if self.drive == VehicleDrive::Tracked { 0.0 } else { COM_REACH };
                    let within = |axis: Vector3<f32>| { let reach = (high - low).dot(&axis.abs()) * share; offset.dot(&axis).clamp(-reach, reach) };

                    middle + frame.forward * within(frame.forward) + frame.right() * within(frame.right()) + frame.up * offset.dot(&frame.up)
                }
                None => com,
            };
        }

        // a box of the chassis size is close enough for the inertia
        self.principal_inertia = Vector3::new
        (
            mass / 12.0 * (extent.y * extent.y + extent.z * extent.z),
            mass / 12.0 * (extent.x * extent.x + extent.z * extent.z),
            mass / 12.0 * (extent.x * extent.x + extent.y * extent.y)
        );

        let chassis = VehicleChassisDesc
        {
            shape,
            mass,
            center_of_mass: self.chassis.center_of_mass,
            principal_inertia: self.principal_inertia,
            friction: self.chassis.friction,
            restitution: self.chassis.restitution,
            linear_damping: self.chassis.linear_damping,
            angular_damping: self.chassis.angular_damping,
            bumpers: if self.chassis.wheel_bumpers { self.bumpers(&frame) } else { vec![] },
            forward: frame.forward,
            up: frame.up,
        };

        let wheel_amount = self.wheels.len().max(1) as f32;
        self.sag = if self.suspension.keep_ride_height { (EARTH_GRAVITY / (wheel_amount * self.suspension.stiffness.max(0.01))).min(self.suspension.travel) } else { 0.0 };

        let tuning = self.wheel_tuning(mass);

        let wheels: Vec<VehicleWheelDesc> = self.wheels.iter().map(|wheel| VehicleWheelDesc
        {
            connection: wheel.center + frame.up * (self.suspension.rest_length - self.sag),
            direction: -frame.up,
            axle: frame.right(),
            rest_length: self.suspension.rest_length,
            radius: wheel.radius,
            tuning,
        }).collect();

        scene.physics.set_vehicle(&node, chassis, &wheels);

        let riders = self.riders(&node);
        scene.physics.set_vehicle_riders(node.read().unwrap().id, riders);
    }

    // a bit smaller than the wheel and a bit higher, so it never touches flat ground and only catches edges
    fn bumpers(&self, frame: &VehicleFrame) -> Vec<(Vector3<f32>, f32)>
    {
        self.wheels.iter().map(|wheel|
        {
            let radius = wheel.radius * BUMPER_RADIUS;
            (wheel.center + frame.up * (wheel.radius - radius + BUMPER_LIFT), radius)
        }).collect()
    }

    fn wheel_tuning(&self, mass: f32) -> WheelTuning
    {
        WheelTuning
        {
            suspension_stiffness: self.suspension.stiffness,
            suspension_compression: self.suspension.compression,
            suspension_damping: self.suspension.relaxation,
            max_suspension_travel: self.suspension.travel,
            side_friction_stiffness: self.tires.side_grip,
            friction_slip: self.tires.grip,
            max_suspension_force: mass * EARTH_GRAVITY * self.suspension.max_force_factor,
        }
    }

    // Seated nodes outside the vehicle hierarchy are carried by the physics write back, so they never trail a frame behind.
    fn riders(&self, vehicle: &NodeItem) -> Vec<(NodeItem, Pose)>
    {
        let Some(frame) = self.frame else { return vec![]; };

        self.seats.iter().zip(self.seat_runtime.iter())
            .filter_map(|(seat, runtime)| runtime.as_ref().map(|runtime| (seat, &runtime.node)))
            .filter(|(_, rider)| !rider.read().unwrap().has_parent_or_is_equal(vehicle.clone()))
            .map(|(seat, rider)| (rider.clone(), Self::seat_pose(&frame, &seat.seat_position, seat.seat_rotation)))
            .collect()
    }

    // the rider faces the vehicle forward, a character looks along its -z
    fn seat_pose(frame: &VehicleFrame, position: &Vector3<f32>, rotation_deg: f32) -> Pose
    {
        let back = -frame.forward;
        let right = frame.up.cross(&back).normalize();
        let basis = Matrix3::from_columns(&[right, frame.up, back]);

        // not from_matrix: its iteration starts at identity and stays there for an exact half turn
        let rotation = UnitQuaternion::from_axis_angle(&Unit::new_normalize(frame.up), rotation_deg.to_radians()) * UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(basis));

        Pose::from_parts(Vector::new(position.x, position.y, position.z), Rotation::from_xyzw(rotation.i, rotation.j, rotation.k, rotation.w))
    }

    // ********** hitch **********

    // Where the trailer couples, chassis space: its part named like 'hitch' or 'kingpin', else the middle of its front tip - as the two stand.
    fn measure_hitch_point(&self) -> Option<Vector3<f32>>
    {
        let node = self.node.as_ref()?;
        let trailer = self.trailer.as_ref()?;
        let frame = self.frame?;

        let to_chassis = Self::chassis_inverse(&node.read().unwrap().get_full_transform());
        let regex = Regex::new(HITCH_NAME_REGEX).unwrap();

        let part = Scene::list_all_child_nodes(&trailer.read().unwrap().nodes).into_iter().find(|child| Self::names_of(child).iter().any(|name| regex.is_match(name)));
        if let Some(part) = part
        {
            return Some(Self::subtree_bounds(&part, &to_chassis).map(|(min, max)| (min + max) * 0.5).unwrap_or_else(|| (to_chassis * part.read().unwrap().get_full_transform().column(3)).xyz()));
        }

        // the trailer stands behind the vehicle, so its tip points along the vehicle's forward
        let points = PhysicsWorld::collect_points(&[trailer.clone()], &to_chassis);
        let tip = points.iter().map(|point| point.dot(&frame.forward)).fold(f32::MIN, f32::max);
        let front: Vec<Vector3<f32>> = points.iter().filter(|point| point.dot(&frame.forward) > tip - HITCH_TIP_DEPTH).copied().collect();

        Self::bounds_of(&front).map(|(min, max)| (min + max) * 0.5)
    }

    // Hands the coupling to the physics - after a change, or if the physics has none. No trailer: none.
    fn sync_hitch(&mut self, scene: &mut Scene, node_id: u32)
    {
        if self.hitch_dirty && self.trailer.is_none() && !self.hitch.trailer_name.is_empty()
        {
            self.trailer = scene.find_node_by_name(&self.hitch.trailer_name).map(OptionOrId::Some).unwrap_or(OptionOrId::None);
        }

        let trailer_id = self.trailer.as_ref().map(|trailer| trailer.read().unwrap().id).filter(|id| *id != node_id);

        let Some(trailer_id) = trailer_id else
        {
            self.hitch_dirty = false;
            scene.physics.remove_hitches_of(node_id);
            return;
        };

        if !self.hitch_dirty && scene.physics.has_hitch(node_id, trailer_id)
        {
            return;
        }

        self.hitch_dirty = false;

        // measured where the two stand - not while coupled, a jackknifed trailer has another tip
        if self.hitch.point_auto && scene.physics.hitch_of(trailer_id).map_or(true, |hitch| hitch.state != HitchState::Coupled)
        {
            if let Some(point) = self.measure_hitch_point()
            {
                self.hitch.point = point;
            }
        }

        let degrees = |value: f32| if value >= 180.0 { PI } else { value.max(0.0).to_radians() };

        scene.physics.set_hitch(node_id, trailer_id, HitchDesc
        {
            point: Vector::new(self.hitch.point.x, self.hitch.point.y, self.hitch.point.z),
            yaw_limit: degrees(self.hitch.yaw_limit),
            pitch_limit: degrees(self.hitch.pitch_limit),
            roll_limit: degrees(self.hitch.roll_limit),
            break_roll: degrees(self.hitch.break_roll),
            break_force: self.hitch.break_force.max(0.0) * 1000.0,
        });
    }

    // what the hitch telemetry shows - towing comes first, a trailer in the middle of a train shows its tow vehicle
    fn hitch_telemetry(physics: &PhysicsWorld, node_id: u32) -> Option<VehicleHitchTelemetry>
    {
        let (other, hitch, towing) = match (physics.hitch_from(node_id), physics.hitch_of(node_id))
        {
            (Some((trailer, hitch)), _) => (trailer, hitch, true),
            (None, Some(hitch)) => (hitch.tow, hitch, false),
            (None, None) => return None,
        };

        Some(VehicleHitchTelemetry { towing, other, state: hitch.state, force: hitch.force / 1000.0, angles: hitch.angles.map(|angle| angle.to_degrees()), brake: hitch.brake, load: physics.vehicle_load_factor(node_id) })
    }

    // ********** input **********

    fn read_input(&self, io: &mut InputOutput) -> VehicleInput
    {
        let controls = &self.controls;
        let input = &mut io.input_manager;

        let left = controls.steer_left.value(input, controls.gamepad);
        let right = controls.steer_right.value(input, controls.gamepad);

        VehicleInput
        {
            throttle: controls.throttle.value(input, controls.gamepad).value,
            brake: controls.brake.value(input, controls.gamepad).value,
            steer: left.value - right.value,
            steer_analog: left.analog || right.analog,
            handbrake: controls.handbrake.held(input, controls.gamepad),
            recover: controls.recover.pressed(input, controls.gamepad),
            camera: controls.camera.pressed(input, controls.gamepad),
            look:
            (
                controls.look_right.value(input, controls.gamepad).value - controls.look_left.value(input, controls.gamepad).value,
                controls.look_up.value(input, controls.gamepad).value - controls.look_down.value(input, controls.gamepad).value,
            ),
            zoom: controls.zoom_in.value(input, controls.gamepad).value - controls.zoom_out.value(input, controls.gamepad).value,
        }
    }

    // ********** visuals **********

    fn reset_visuals(&mut self)
    {
        for wheel in &self.wheels
        {
            if let Some(runtime) = wheel.runtime.as_ref()
            {
                runtime.reset();
            }
        }

        if let Some(runtime) = self.steering_runtime.as_ref()
        {
            runtime.reset();
        }
    }

    // Each wheel at the given chassis pose. The ray ran at the pose before the last step, so its hit is taken as a ground plane under the shown pose.
    fn wheel_visuals(physics: &PhysicsWorld, node_id: u32, chassis_world: &Matrix4<f32>) -> Vec<WheelVisual>
    {
        let Some((vehicle, _)) = physics.vehicle(node_id) else { return vec![]; };

        let v = |v: Vector| Vector3::new(v.x, v.y, v.z);
        let chassis_rotation: Matrix3<f32> = chassis_world.fixed_view::<3, 3>(0, 0).into_owned();

        vehicle.controller.wheels().iter().map(|wheel|
        {
            let info = wheel.raycast_info();
            let mut suspension_length = info.suspension_length;
            let mut ground = None;

            if info.is_in_contact
            {
                let hard_point = (chassis_world * v(wheel.chassis_connection_point_cs).push(1.0)).xyz();
                let direction = chassis_rotation * v(wheel.direction_cs);
                let normal = v(info.contact_normal_ws);
                let facing = direction.dot(&normal);

                // like rapier: a ground nearly parallel to the suspension keeps the measured length
                if facing < -0.1
                {
                    let distance = (v(info.contact_point_ws) - hard_point).dot(&normal) / facing;
                    suspension_length = (distance - wheel.radius).clamp(wheel.suspension_rest_length - wheel.max_suspension_travel, wheel.suspension_rest_length + wheel.max_suspension_travel);
                }

                ground = Some((hard_point + direction * (suspension_length + wheel.radius), normal));
            }

            WheelVisual { contact: info.is_in_contact, suspension_length, spin: wheel.rotation, steering: wheel.steering, ground }
        }).collect()
    }

    // drops every node the vehicle runtime holds, e.g. once the vehicle node is deleted
    fn release_nodes(&mut self)
    {
        self.set_cockpit_visibility(false);

        for wheel in &mut self.wheels
        {
            wheel.runtime = None;
        }

        self.steering_runtime = None;
        self.seat_runtime.clear();
        self.chassis_points.clear();
        self.chassis_parts.clear();
        self.pending = None;
    }

    // what this controller excluded from the scene colliders collides again
    fn release_exclusions(&mut self, scene: &mut Scene)
    {
        if !self.excluded_node_ids.is_empty()
        {
            scene.physics.include_nodes(&self.excluded_node_ids);
            self.excluded_node_ids.clear();
        }
    }

    fn start_sit_animations(&self)
    {
        for animation in self.seat_runtime.iter().flatten().filter_map(|runtime| runtime.animation.as_ref())
        {
            component_downcast_mut!(animation, Animation);

            if !animation.running()
            {
                animation.looped = true;
                animation.weight = 1.0;
                animation.start();
            }
        }
    }

    // ********** camera **********

    // the cockpit controller only lives at runtime - the chase one waits in chase_controller meanwhile
    fn put_cockpit_controller(&mut self, scene: &mut Scene)
    {
        let Some(cam) = scene.get_game_camera_mut(&self.cam_name) else { return; };
        if cam.controller.as_ref().is_some_and(|controller| controller.as_any().is::<FollowController>()) { return; }

        let mut controller = FollowController::new();
        controller.mouse_look = true;
        controller.data.get_mut().pitch = -COCKPIT_PITCH;

        self.chase_controller = cam.controller.replace(Box::new(controller));
    }

    // true if it was swapped back - a scene saved in the cockpit has no chase controller, so it is made again
    fn put_chase_controller(&mut self, scene: &mut Scene) -> bool
    {
        let Some(node) = self.node.as_ref().cloned() else { return false; };
        let Some(cam) = scene.get_game_camera_mut(&self.cam_name) else { return false; };
        if !cam.controller.as_ref().is_some_and(|controller| controller.as_any().is::<FollowController>()) { return false; }

        cam.controller = Some(self.chase_controller.take().unwrap_or_else(|| Box::new(self.new_chase_controller(&node))));
        cam.data.get_mut().up = Vector3::y(); // the cockpit rolled it

        true
    }

    // rotation: the pose the node shows this frame, the camera must not use the last physics step instead
    fn update_camera(&mut self, scene: &mut Scene, io: &mut InputOutput, rotation: &UnitQuaternion<f32>, speed: f32, lateral_left: f32, input: &VehicleInput, dt: f32)
    {
        let Some(frame) = self.frame else { return; };

        if input.camera
        {
            self.camera.mode = match self.camera.mode { VehicleCameraMode::Chase => VehicleCameraMode::Cockpit, VehicleCameraMode::Cockpit => VehicleCameraMode::Chase };
        }

        // orbiting by mouse or stick pauses the swing in behind the vehicle
        let mouse = &io.input_manager.mouse;
        let (look, zoom) = (input.look, input.zoom);
        let looking = look.0 != 0.0 || look.1 != 0.0;
        let orbiting = looking || mouse.is_holding(MouseButton::Left) || (!*mouse.visible.get_ref() && mouse.raw_velocity.velocity.norm() > 0.0);
        self.orbit_idle_time = if orbiting { 0.0 } else { self.orbit_idle_time + dt };

        let forward_world = rotation * frame.forward;
        let (yaw, _) = yaw_pitch_from_direction(forward_world);

        // where the vehicle goes instead of where it points - fades in with the speed, standing still the direction means nothing
        let travel = (speed * speed + lateral_left * lateral_left).sqrt();
        let travel_world = rotation * (frame.forward * speed - frame.right() * lateral_left);
        let (travel_yaw, _) = yaw_pitch_from_direction(travel_world);
        let slide_weight = self.camera.slide_follow * ((travel - SLIDE_FOLLOW_MIN_SPEED) / SLIDE_FOLLOW_MIN_SPEED).clamp(0.0, 1.0);
        let behind = yaw + shortest_angle_dist(yaw, travel_yaw) * slide_weight + PI;

        // the heading only means something while the vehicle points sideways and stands upright - not in a loop
        let heading_weight = if (rotation * frame.up).y > 0.0 { (1.0 - forward_world.y * forward_world.y).max(0.0).sqrt() } else { 0.0 };

        let cockpit = self.camera.mode == VehicleCameraMode::Cockpit;
        let turn = self.controls.look_speed.to_radians() * dt;

        if cockpit
        {
            self.put_cockpit_controller(scene);

            let Some(controller) = scene.get_game_camera_mut(&self.cam_name).and_then(|cam| cam.controller.as_mut()?.as_any_mut().downcast_mut::<FollowController>()) else { return; };

            // sits on the vehicle node and looks ahead, pitching and rolling with it
            controller.basis = frame.camera_basis();
            if controller.data.get_ref().offset != self.camera.cockpit_offset
            {
                controller.data.get_mut().offset = self.camera.cockpit_offset;
            }

            if looking
            {
                controller.look_by(-look.0 * turn, look.1 * turn);
            }

            // the view turns back to straight ahead like the chase camera swings in behind
            if self.camera.follow && speed > 1.0 && self.orbit_idle_time > self.camera.follow_delay
            {
                let yaw = controller.data.get_ref().yaw;
                controller.data.get_mut().yaw = yaw + shortest_angle_dist(yaw, 0.0) * (1.0 - (-self.camera.follow_speed * dt).exp());
            }
        }
        else
        {
            let restored = self.put_chase_controller(scene);

            let node = self.node.as_ref().cloned();
            let Some(cam) = scene.get_game_camera_mut(&self.cam_name) else { return; };
            let Some(controller) = cam.controller.as_mut().and_then(|controller| controller.as_any_mut().downcast_mut::<TargetRotationController>()) else { return; };

            let center = self.chassis_bounds.map(|(min, max)| (min + max) * 0.5).unwrap_or_else(Vector3::zeros);
            controller.data.get_mut().offset = rotation * (center + frame.up * self.camera.height);

            if restored
            {
                let data = controller.data.get_mut();
                data.alpha = behind;
                data.beta = DEFAULT_CAM_PITCH.to_radians();

                // its smoothing still trails where the vehicle was when the cockpit came on
                controller.apply_to_camera(node, &mut cam.data);
            }

            if looking
            {
                let data = controller.data.get_mut();
                data.alpha = (data.alpha - look.0 * turn) % (PI * 2.0);
                data.beta = (data.beta - look.1 * turn).clamp(-PI / 2.0 + 0.05, PI / 2.0 - 0.05); // beta is the camera height, stick up lowers it to look up
            }

            if zoom != 0.0
            {
                controller.zoom_by((-zoom * self.controls.zoom_speed * dt).exp(), CHASE_MIN_DISTANCE);
            }

            // reversing keeps the view, so it does not swing around - sliding sideways still counts as moving
            if self.camera.follow && travel > 1.0 && speed > -1.0 && self.orbit_idle_time > self.camera.follow_delay
            {
                let alpha = controller.data.get_ref().alpha;
                let diff = shortest_angle_dist(alpha, behind);
                controller.data.get_mut().alpha = alpha + diff * (1.0 - (-self.camera.follow_speed * heading_weight * dt).exp());
            }
        }

        self.set_cockpit_visibility(cockpit);
    }

    // ********** sound **********

    fn update_sound(&mut self, node: &NodeItem, squeal: f32, speed: f32, dt: f32)
    {
        self.release_detached_sounds(node);

        if !self.sound.enabled || !self.sound.has_sounds()
        {
            self.stop_sound();
            return;
        }

        if self.sound_player.is_none()
        {
            let seed = self.node_id().unwrap_or(1).wrapping_mul(2654435761);
            self.sound_player = Some(VehicleSoundPlayer::new(seed));
        }

        // idling is steady combustion, not overrun - the overrun loops only belong above it
        let idle = self.engine.idle_rpm.max(1.0);
        let idling = 1.0 - ((self.engine_state.rpm - idle * 1.1) / (idle * 0.8)).clamp(0.0, 1.0);

        let input = VehicleSoundInput
        {
            rpm: self.engine_state.rpm,
            engine_load: self.engine_state.load,
            idling,
            squeal,
            speed,
            limiter: self.engine_state.rpm >= self.engine.max_rpm * 0.99,
            electric: self.engine.engine_type == super::vehicle::engine::EngineType::Electric,
        };

        if let Some(player) = self.sound_player.as_mut()
        {
            player.update(&self.sound, &input, dt);
        }
    }

    // ********** tires **********

    // How hard each wheel slides sideways, 0..1 - by its slip angle, so a fast clean corner stays quiet and clean. 0 in the air.
    fn wheel_slides(&mut self, rotation: &UnitQuaternion<f32>, frame: &VehicleFrame, wheel_visuals: &[WheelVisual], dt: f32) -> Vec<f32>
    {
        self.previous_ground.resize(self.wheels.len(), None);

        let mut slides = Vec::with_capacity(self.wheels.len());
        for index in 0..self.wheels.len()
        {
            let visual = wheel_visuals.get(index);
            let ground = visual.and_then(|visual| visual.ground);
            let previous = std::mem::replace(&mut self.previous_ground[index], ground.map(|(point, _)| point));

            let (Some((point, normal)), Some(previous), Some(visual)) = (ground, previous, visual) else
            {
                slides.push(0.0);
                continue;
            };

            // over the ground, against its axle turned by the steering
            let velocity = (point - previous) / dt.max(1e-4);
            let velocity = velocity - normal * velocity.dot(&normal);
            let axle = rotation * (Rotation3::from_axis_angle(&Unit::new_normalize(frame.up), visual.steering) * frame.right());
            let sideways = velocity.dot(&axle).abs();
            let along = (velocity.norm_squared() - sideways * sideways).max(0.0).sqrt();

            let angle = sideways.atan2(along).to_degrees();
            let by_angle = ((angle - SLIDE_MIN_ANGLE) / (SLIDE_FULL_ANGLE - SLIDE_MIN_ANGLE)).clamp(0.0, 1.0);
            let by_speed = ((sideways - SLIDE_MIN_SPEED) / SLIDE_MIN_SPEED).clamp(0.0, 1.0);

            slides.push(by_angle * by_speed);
        }

        slides
    }

    // each wheel leaves rubber as far as it slides - sideways over the ground, locked or spinning
    fn update_tire_marks(&mut self, scene: &mut Scene, wheel_visuals: &[WheelVisual], slides: &[f32], input: &VehicleInput, speed: f32, brake: f32, wheelspin: f32)
    {
        let mut marks = Vec::with_capacity(self.wheels.len());
        for (index, wheel) in self.wheels.iter().enumerate()
        {
            let ground = wheel_visuals.get(index).and_then(|visual| visual.ground);
            let slide = slides.get(index).copied().unwrap_or(0.0);

            let locked = if input.handbrake && wheel.handbrake && speed.abs() > 1.0 { (speed.abs() / 10.0).min(1.0) } else if brake > 0.8 && speed.abs() > 8.0 { 0.4 } else { 0.0 };
            let spinning = if wheel.driven { wheelspin } else { 0.0 };

            // scenes set up before the width was measured: a share of the radius
            let width = if !self.tire_marks.width_auto { self.tire_marks.width } else if wheel.width > 0.0 { wheel.width } else { wheel.radius * UNMEASURED_TIRE_WIDTH };

            marks.push(TireMarkInput { ground, strength: slide.max(locked).max(spinning), width });
        }

        self.tire_mark_pieces.apply_color(self.tire_marks.color);
        self.tire_mark_pieces.update(scene, &self.tire_marks, &marks);
    }

    fn stop_sound(&mut self)
    {
        if let Some(mut player) = self.sound_player.take()
        {
            player.stop();
        }
    }

    // sound components deleted from the vehicle node (or moved away) are no longer played
    fn release_detached_sounds(&mut self, node: &NodeItem)
    {
        let attached = node.read().unwrap().components.clone();

        for component in self.sound.release_detached(&attached)
        {
            VehicleSoundPlayer::silence(&component);
        }
    }

    // the vehicle node is gone and its sound components with it
    fn release_sounds(&mut self)
    {
        self.stop_sound();

        for component in self.sound.release_all()
        {
            VehicleSoundPlayer::silence(&component);
        }
    }
}

// picks one of the sound components of the vehicle node
// a sound resource picked here becomes a new looped Sound component of the vehicle node
fn sound_component_combo(ui: &mut egui::Ui, id: String, sound: &mut OptionOrId<ComponentItem>, components: &[ComponentItem], sources: &[SoundSourceItem], node: Option<&NodeItem>, name_prefix: &str)
{
    let text = match sound
    {
        OptionOrId::Some(component) => component.read().unwrap().get_base().name.clone(),
        OptionOrId::Id(uuid) => format!("missing {}", uuid),
        OptionOrId::None => "none".to_string(),
    };

    // an engine or tire sound that ends falls silent until it is started again
    let not_looped = sound.as_ref().is_some_and(|component| { component_downcast!(component, Sound); !component.get_data().looped });

    let text = if sound.is_ref() || not_looped { RichText::new(format!("⚠ {}", text)).color(Color32::LIGHT_RED) } else { RichText::new(text) };

    let response = egui::ComboBox::from_id_salt(id).width(220.0).selected_text(text).show_ui(ui, |ui|
    {
        if ui.selectable_label(sound.is_none(), "none").clicked()
        {
            *sound = OptionOrId::None;
        }

        for component in components
        {
            let name = component.read().unwrap().get_base().name.clone();
            let selected = sound.as_ref().is_some_and(|current| Arc::ptr_eq(current, component));

            if ui.selectable_label(selected, name).clicked()
            {
                *sound = OptionOrId::Some(component.clone());
            }
        }

        let Some(node) = node.filter(|_| !sources.is_empty()) else { return; };

        ui.separator();
        ui.label(RichText::new("new from a sound resource:").color(Color32::GRAY));

        for source in sources
        {
            let (name, path) = { let source = source.read().unwrap(); (source.name.clone(), source.origin_path().unwrap_or("").to_string()) };

            if ui.selectable_label(false, format!("+ {}", name)).on_hover_text(path).clicked()
            {
                let component: ComponentItem = Arc::new(RwLock::new(Box::new(Sound::new(&format!("{}{}", name_prefix, name), source.clone(), SoundType::Spatial, true))));
                node.write().unwrap().add_component(component.clone());
                *sound = OptionOrId::Some(component);
            }
        }
    });

    if sound.is_ref()
    {
        response.response.on_hover_text("the vehicle node has no sound component with this uuid");
    }
    else if not_looped
    {
        response.response.on_hover_text("the sound component does not loop - turn on 'Loop' in its settings");
    }
}

#[typetag::serde]
impl SceneController for VehicleController
{
    scene_controller_impl_default!();

    fn runs_in_mode(&self, run_mode: RunMode) -> bool
    {
        run_mode.runs_game_logic()
    }

    fn on_run_mode_changed(&mut self, scene: &mut Scene, old: RunMode, new: RunMode)
    {
        if old.runs_game_logic() && !new.runs_game_logic()
        {
            self.stop_sound();
            self.reset_visuals();

            self.engine_state.reset(&self.engine);
            self.steer = 0.0;
            self.drift_amount = 0.0;
            self.wheelspin = 0.0;
            self.lean_goal = 0.0;
            self.upside_down_time = 0.0;
            self.pending = None;

            // the editor shows the scene as it was built
            self.tire_mark_pieces.clear(scene);
            self.previous_ground.clear();

            self.set_cockpit_visibility(false);

            // the editor gets the upright chase camera back, play puts the cockpit on again
            if !self.vehicle_type.is_trailer()
            {
                self.put_chase_controller(scene);
            }
        }

        // the trailer is coupled where it stands when the run starts
        if !old.runs_game_logic() && new.runs_game_logic()
        {
            self.hitch_dirty = true;
        }
    }

    fn cleanup(&mut self)
    {
        self.release_sounds();
        self.reset_visuals();
        self.release_nodes();
        self.tire_mark_pieces.clear_later();

        self.chassis_bounds = None;
        self.node = OptionOrId::None;
    }

    fn cleanup_node(&mut self, node: NodeItem) -> bool
    {
        let id = node.read().unwrap().id;
        let is_or_below = |part: &NodeItem| { let part = part.read().unwrap(); part.id == id || part.has_parent_id(id) };

        // the vehicle goes with any of its parents
        if self.node.as_ref().is_some_and(is_or_below)
        {
            // the scene colliders it excluded are released with the next scene access
            self.release_sounds();
            self.release_nodes();
            self.tire_mark_pieces.clear_later();
            self.node = OptionOrId::None;
            return true;
        }

        for runtime in &mut self.seat_runtime
        {
            if runtime.as_ref().is_some_and(|runtime| is_or_below(&runtime.node))
            {
                *runtime = None;
            }
        }

        // the hitch in the physics goes with the next update
        if self.trailer.as_ref().is_some_and(is_or_below)
        {
            self.trailer = OptionOrId::None;
        }

        false
    }

    fn on_remove(&mut self, scene: &mut Scene)
    {
        if let Some(id) = self.node_id()
        {
            scene.physics.remove_hitches_of(id);
            scene.physics.remove_vehicle(id);
        }

        self.release_exclusions(scene);

        if !self.vehicle_type.is_trailer()
        {
            self.put_chase_controller(scene);
        }

        self.stop_sound();
        self.reset_visuals();
        self.tire_mark_pieces.clear(scene);
    }

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        self.sound.resolve(&context.components);

        // an older scene: its driver becomes the first seat
        if let Some(mut driver) = self.legacy_driver.take()
        {
            if !driver.node_name.is_empty() && self.driver_seat().is_none()
            {
                driver.role = SeatRole::Driver;
                self.seats.insert(0, driver);
            }
        }

        if self.node.is_ref()
        {
            let node_found = context.nodes.iter().find(|node| node.read().unwrap().uuid == self.node.id().unwrap());

            if let Some(node) = node_found
            {
                self.node = OptionOrId::Some(node.clone());
            }
            else
            {
                console_warning!("VehicleController: node with id {} not found, falling back to the name", self.node.id().unwrap());
                self.node = OptionOrId::None;
            }
        }

        if self.node.is_none() && !self.node_name.is_empty()
        {
            if let Some(node) = context.scene.find_node_by_name(&self.node_name)
            {
                self.node = OptionOrId::Some(node);
            }
        }

        // the trailer by its uuid, else by its name
        if self.trailer.is_ref()
        {
            let uuid = self.trailer.id().unwrap().to_string();
            self.trailer = context.nodes.iter().find(|node| node.read().unwrap().uuid == uuid).cloned().map(OptionOrId::Some).unwrap_or(OptionOrId::None);
        }

        if self.trailer.is_none() && !self.hitch.trailer_name.is_empty()
        {
            self.trailer = context.scene.find_node_by_name(&self.hitch.trailer_name).map(OptionOrId::Some).unwrap_or(OptionOrId::None);
        }

        // the saved wheel list and settings win - only the runtime is rebuilt
        if self.node.is_some() && self.frame.is_some()
        {
            self.setup_runtime(&mut context.scene);
        }
    }

    fn update(&mut self, scene: &mut Scene, io: &mut InputOutput, frame_scale: f32) -> bool
    {
        self.pending = None;

        let Some(node) = self.node.as_ref().cloned() else
        {
            self.release_exclusions(scene);
            return false;
        };

        let Some(frame) = self.frame else { return false; };
        let node_id = node.read().unwrap().id;

        let dt = get_delta_t(frame_scale);

        // never registered, or its setup changed
        if self.physics_dirty || !scene.physics.has_vehicle(node_id)
        {
            if self.wheels.iter().any(|wheel| wheel.runtime.is_none()) || self.chassis_points.is_empty()
            {
                self.setup_runtime(scene);
            }
            else
            {
                self.build_physics(scene);
            }
        }

        self.sync_hitch(scene, node_id);

        // a trailer has no driver - it brakes with its tow vehicle
        let input = if self.vehicle_type.is_trailer() { VehicleInput::default() } else { self.read_input(io) };

        let Some(state) = self.drive(&mut scene.physics, node_id, &frame, &input, dt) else { return false; };
        self.pending = Some(PendingFrame { input, state });

        true
    }

    // Wheels, camera and sound follow the pose the physics write back just showed - interpolated between the steps, so never the raw body pose.
    fn update_after_physics(&mut self, scene: &mut Scene, io: &mut InputOutput, frame_scale: f32)
    {
        let Some(PendingFrame { input, state }) = self.pending.take() else { return; };
        let Some(node) = self.node.as_ref().cloned() else { return; };
        let Some(frame) = self.frame else { return; };
        let node_id = node.read().unwrap().id;

        let dt = get_delta_t(frame_scale);

        let chassis_world = Self::chassis_matrix(&node.read().unwrap().get_full_transform());
        let rotation = extract_rotation_quat_from_transform(&chassis_world);
        let wheel_visuals = Self::wheel_visuals(&scene.physics, node_id, &chassis_world);

        let DriveState { speed, lateral_left, steer_angle, brake, wheelspin, lean, lean_target: _, mut telemetry_wheels } = state;

        // ********** visuals **********
        for (index, wheel) in self.wheels.iter().enumerate()
        {
            let Some(runtime) = wheel.runtime.as_ref() else { continue; };
            let visual = wheel_visuals.get(index).copied().unwrap_or(WheelVisual { contact: false, suspension_length: self.suspension.rest_length, spin: 0.0, steering: 0.0, ground: None });

            let compression = if visual.contact { self.suspension.rest_length - self.sag - visual.suspension_length } else { -self.sag };

            let steer_rotation = Rotation3::from_axis_angle(&Unit::new_normalize(frame.up), visual.steering);
            let spin_rotation = Rotation3::from_axis_angle(&Unit::new_normalize(frame.right()), -visual.spin);

            runtime.apply(frame.up * compression, &(steer_rotation * spin_rotation));

            if let Some(telemetry) = telemetry_wheels.get_mut(index)
            {
                telemetry.compression = compression;
            }
        }

        if let Some(runtime) = self.steering_runtime.as_ref()
        {
            let ratio = if self.steering.axis == SteeringAxis::Handlebar { 1.0 } else { self.steering.ratio };
            runtime.apply(Vector3::zeros(), &Rotation3::from_axis_angle(&Unit::new_normalize(runtime.axis), steer_angle * ratio));
        }

        self.start_sit_animations();

        // ********** camera **********
        if !self.vehicle_type.is_trailer()
        {
            self.update_camera(scene, io, &rotation, speed, lateral_left, &input, dt);
        }

        // ********** sound **********
        // the tires squeal sliding sideways (the wheel sliding most), locked (the handbrake, or a full stop from speed) or spinning - only as far as they touch the ground
        let slides = self.wheel_slides(&rotation, &frame, &wheel_visuals, dt);
        let grounded = wheel_visuals.iter().filter(|visual| visual.contact).count() as f32 / wheel_visuals.len().max(1) as f32;
        let skid = slides.iter().copied().fold(0.0, f32::max);
        let locked = grounded * if input.handbrake && speed.abs() > 1.0 { (speed.abs() / 10.0).min(1.0) } else if brake > 0.8 && speed.abs() > 8.0 { 0.4 } else { 0.0 };

        self.update_sound(&node, skid.max(locked).max(wheelspin * grounded), speed, dt);

        // ********** tire marks **********
        self.update_tire_marks(scene, &wheel_visuals, &slides, &input, speed, brake, wheelspin);

        // ********** contacts **********
        let mut last_hit = self.telemetry.last_hit;
        let mut touching = vec![];

        for contact in scene.physics.contacts_of(node_id)
        {
            if contact.started()
            {
                last_hit = Some((contact.other, contact.impact_speed));
            }

            if !contact.stopped()
            {
                touching.push(contact.other);
            }
        }

        // ********** telemetry **********
        self.telemetry = VehicleTelemetry
        {
            speed_kmh: speed * 3.6,
            rpm: self.engine_state.rpm,
            gear: self.engine_state.gear,
            throttle: self.engine_state.throttle,
            brake,
            steer: self.steer,
            drift: self.drift_amount,
            wheelspin,
            skid,
            lean: lean.to_degrees(),
            wheels: telemetry_wheels,
            touching,
            last_hit,
            hitch: Self::hitch_telemetry(&scene.physics, node_id),
        };
    }

    fn ui(&mut self, ui: &mut egui::Ui, scene: &mut Scene, context: &ControllerUiContext)
    {
        self.ui_settings(ui, scene, context);
    }
}

impl VehicleController
{
    // Input to forces: steering, engine, brakes, drift, balance, tracks, recover. Everything that is not camera, sound or visuals.
    fn drive(&mut self, physics: &mut PhysicsWorld, node_id: u32, frame: &VehicleFrame, input: &VehicleInput, dt: f32) -> Option<DriveState>
    {
        let frame = *frame;
        let fixed_dt = physics.settings.fixed_timestep.max(0.0001);

        // ********** read the body **********
        let (rotation, linvel, angvel, contacts, side_torque) =
        {
            let Some((vehicle, body)) = physics.vehicle(node_id) else { return None; };

            let pose = body.position();
            let rotation = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(pose.rotation.w, pose.rotation.x, pose.rotation.y, pose.rotation.z));
            let linvel = Vector3::new(body.linvel().x, body.linvel().y, body.linvel().z);
            let angvel = Vector3::new(body.angvel().x, body.angvel().y, body.angvel().z);

            let contacts: Vec<WheelContact> = vehicle.controller.wheels().iter().map(|wheel|
            {
                let info = wheel.raycast_info();
                WheelContact { contact: info.is_in_contact, ground: info.ground_object }
            }).collect();

            // the torque the side grip of the wheels put on the body in the last step
            let com = body.center_of_mass();
            let side_torque: Vector3<f32> = vehicle.controller.wheels().iter().filter(|wheel| wheel.raycast_info().is_in_contact).map(|wheel|
            {
                let info = wheel.raycast_info();
                let side = (wheel.axle() - info.contact_normal_ws * wheel.axle().dot(info.contact_normal_ws)).normalize_or_zero();
                let torque = (info.contact_point_ws - com).cross(side * wheel.side_impulse) / fixed_dt;
                Vector3::new(torque.x, torque.y, torque.z)
            }).sum();

            (rotation, linvel, angvel, contacts, side_torque)
        };

        let forward_world = rotation * frame.forward;
        let up_world = rotation * frame.up;
        let right_world = rotation * frame.right();

        let speed = linvel.dot(&forward_world);
        let lateral_left = -linvel.dot(&right_world);
        let speed_kmh = speed.abs() * 3.6;

        let tracked = self.drive == VehicleDrive::Tracked;

        // ********** steering **********
        let high_speed = (speed_kmh / self.steering.high_speed.max(1.0)).clamp(0.0, 1.0);
        let mut max_angle = self.steering.max_angle.to_radians() * (1.0 + (self.steering.high_speed_factor - 1.0) * high_speed);

        // two wheelers: at speed the lock is what the lean limit allows - full lock is the steepest lean, not a slide
        if self.balance.enabled && speed.abs() > ASSIST_MIN_SPEED
        {
            let wheelbase = self.wheelbase(&frame).max(0.3);
            max_angle = max_angle.min((wheelbase * EARTH_GRAVITY * self.balance.max_lean.to_radians().tan() / (speed * speed)).atan());
        }

        if input.steer_analog
        {
            self.steer = input.steer;
        }
        else
        {
            let returning = approx_zero(input.steer) || input.steer.signum() != self.steer.signum();
            let rate = if returning { self.steering.return_speed } else { self.steering.speed };
            let diff = input.steer - self.steer;

            self.steer += diff.signum() * (rate * dt).min(diff.abs());
        }

        let steer_angle = self.steer * max_angle;

        // two wheelers: the steering axis leans with the bike, which turns the wheel further on the ground
        let lean = right_world.y.clamp(-1.0, 1.0).asin();
        let wheel_angle = if self.balance.enabled { (steer_angle.tan() * lean.cos()).atan() } else { steer_angle };

        // steering into a slide - the slip angle, positive when the vehicle slides to its left
        let slip_angle = if speed > ASSIST_MIN_SPEED { lateral_left.atan2(speed) } else { 0.0 };
        let slide = (slip_angle.abs() - COUNTER_STEER_DEADZONE.to_radians()).max(0.0) * slip_angle.signum();
        let counter_steer = (slide * self.drift.counter_steer).clamp(-max_angle, max_angle);

        // ********** throttle / brake / reverse **********
        let trailer = self.vehicle_type.is_trailer();
        let reversing = self.engine_state.gear < 0;
        let (mut throttle, mut brake, mut reverse) = (0.0, 0.0, reversing);

        if trailer
        {
            // coupled it brakes with the tow vehicle, alone it stands - and torn off the breakaway cable pulls the brake
            brake = match physics.hitch_of(node_id)
            {
                Some(hitch) if hitch.state != HitchState::Broken => hitch.brake,
                _ => 1.0,
            };
        }
        else if reversing
        {
            if input.throttle > 0.0
            {
                if speed < -STANDSTILL_SPEED { brake = input.throttle; } else { reverse = false; throttle = input.throttle; }
            }
            else
            {
                throttle = input.brake;
            }
        }
        else if input.brake > 0.0
        {
            if speed > STANDSTILL_SPEED { brake = input.brake; } else { reverse = true; throttle = input.brake; }
        }
        else
        {
            throttle = input.throttle;
        }

        // the trailer behind brakes along - a trailer passes it on down the train
        physics.set_hitch_brake(node_id, if self.hitch.trailer_brakes { brake } else { 0.0 });

        // ********** engine **********
        let driven: Vec<usize> = (0..self.wheels.len()).filter(|index| self.wheels[*index].driven).collect();
        let driven_radius = if driven.is_empty() { 0.35 } else { driven.iter().map(|index| self.wheels[*index].radius).sum::<f32>() / driven.len() as f32 };

        // no driven wheel on the ground: the engine revs freely
        let airborne = !driven.is_empty() && driven.iter().all(|index| !contacts.get(*index).is_some_and(|contact| contact.contact));
        let torque = if trailer { 0.0 } else { self.engine_state.update(&self.engine, throttle, reverse, speed, driven_radius, airborne, dt) };
        let direction = if self.engine_state.gear < 0 { -1.0 } else { 1.0 };
        let mut drive_force = torque / driven_radius * direction;

        // no more pull than the weight behind the center of mass holds down - a short, high bike would loop over backwards
        if drive_force > 0.0 && !self.wheels.is_empty()
        {
            let rear_axle = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.forward)).fold(f32::MAX, f32::min);
            let ground = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.up) - wheel.radius).sum::<f32>() / self.wheels.len() as f32;
            let behind = self.chassis.center_of_mass.dot(&frame.forward) - rear_axle;
            let height = self.chassis.center_of_mass.dot(&frame.up) - ground;

            if behind > 0.0 && height > 0.0
            {
                drive_force = drive_force.min(self.chassis.mass * EARTH_GRAVITY * behind / height * WHEELIE_MARGIN);
            }
        }

        // tracks: the sides pushed against each other, from the first gear force
        let first_gear_force = self.engine.torque_at(self.engine.peak_torque_rpm) * self.engine.gear_ratios.first().copied().unwrap_or(1.0) * self.engine.final_drive / driven_radius;
        let turn_force = if tracked { self.steer * first_gear_force * self.tracks.turn_force } else { 0.0 };

        // ********** drift **********
        if input.handbrake
        {
            self.drift_amount = 1.0;
        }
        else
        {
            let sliding = lateral_left.abs() > 2.0;
            let recovery = if throttle > 0.5 && sliding { self.drift.grip_recovery * self.drift.throttle_hold } else { self.drift.grip_recovery };

            self.drift_amount *= (-recovery * dt).exp();
        }

        // ********** wheelspin **********
        let wheel_amount = self.wheels.len().max(1) as f32;
        let handbrake_amount = self.wheels.iter().filter(|wheel| wheel.handbrake).count().max(1) as f32;

        // handbrake and throttle on the same wheels: more engine than brake spins them anyway - the rear wheel burnout
        let handbrake_hold = self.brakes.handbrake_force / handbrake_amount;
        let wheel_drive = drive_force.abs() / driven.len().max(1) as f32;
        let handbrake_burnout = input.handbrake && !tracked && wheel_drive > handbrake_hold && self.wheels.iter().any(|wheel| wheel.driven && wheel.handbrake);
        let spin_target = if handbrake_burnout { 1.0 } else { 0.0 };

        self.wheelspin = if spin_target > self.wheelspin { spin_target } else { spin_target.max(self.wheelspin * (-self.drift.grip_recovery * dt).exp()) };

        // ********** wheels **********

        let mut commands = vec![];
        let mut telemetry_wheels = vec![];

        for (index, wheel) in self.wheels.iter().enumerate()
        {
            let WheelContact { contact, ground } = contacts.get(index).copied().unwrap_or_default();

            // a surface friction of 0.7 is the default of every scene object
            let surface = match (self.tires.surface_grip, ground)
            {
                (true, Some(ground)) => (physics.collider_friction(ground).unwrap_or(0.7) / 0.7).clamp(0.1, 2.0),
                _ => 1.0,
            };

            let mut engine_force = 0.0;
            if wheel.driven
            {
                engine_force = drive_force / driven.len().max(1) as f32;
            }

            if tracked
            {
                // left side back, right side forward turns left
                let side = if wheel.center.dot(&frame.right()) < 0.0 { -1.0 } else { 1.0 };
                engine_force += side * turn_force / wheel_amount;
            }

            let mut brake_force = brake * self.brakes.brake_force / wheel_amount;

            if input.handbrake && wheel.handbrake
            {
                // the engine overpowers the handbrake: the wheel turns anyway, held back by the brake
                if wheel.driven && !tracked && engine_force.abs() > handbrake_hold
                {
                    engine_force -= handbrake_hold * engine_force.signum();
                }
                else
                {
                    brake_force += handbrake_hold;
                    engine_force = 0.0;
                }
            }

            if approx_zero(engine_force) && approx_zero(brake_force)
            {
                brake_force = self.brakes.rolling_resistance / wheel_amount;
            }

            // rapier only brakes when no engine force is set
            if !approx_zero(brake_force) && brake > 0.0
            {
                engine_force = 0.0;
            }

            let mut grip = self.tires.grip * surface;
            let mut side_grip = self.tires.side_grip;

            if wheel.handbrake && !tracked
            {
                grip *= 1.0 + (self.drift.handbrake_grip - 1.0) * self.drift_amount;
            }

            // a spinning tire slides sideways like a locked one - the stronger of the two counts, they do not add up
            if !tracked
            {
                let locked = if wheel.handbrake { self.drift_amount } else { 0.0 };
                let spinning = if wheel.driven { self.wheelspin } else { 0.0 };
                side_grip *= 1.0 + (self.drift.handbrake_side_grip - 1.0) * locked.max(spinning);
            }

            if tracked
            {
                side_grip *= 1.0 + (self.tracks.turn_side_grip - 1.0) * self.steer.abs();
            }

            let steering = if wheel.steer > 0.0 { (wheel_angle * wheel.steer + counter_steer).clamp(-max_angle, max_angle) } else { wheel_angle * wheel.steer };

            commands.push(WheelCommand { engine_force, brake: brake_force * fixed_dt, steering, grip, side_grip });
            telemetry_wheels.push(VehicleWheelTelemetry { contact, compression: 0.0, grip: surface, engine_force, brake: brake_force });
        }

        // ********** body forces **********
        let mass = self.chassis.mass.max(1.0);
        let inertia_along = |axis: &Vector3<f32>| -> f32
        {
            let local = rotation.inverse() * axis;
            local.x * local.x * self.principal_inertia.x + local.y * local.y * self.principal_inertia.y + local.z * local.z * self.principal_inertia.z
        };

        let force = -linvel * linvel.norm() * self.brakes.air_drag;
        let mut torque_world = Vector3::<f32>::zeros();

        // two wheelers: lean into the curve like a rider would, and stay upright while standing
        let mut lean_target = 0.0;
        if self.balance.enabled
        {
            let wheelbase = self.wheelbase(&frame).max(0.3);
            let target = if speed.abs() > ASSIST_MIN_SPEED { (speed * speed * steer_angle.tan() / (wheelbase * EARTH_GRAVITY)).atan() * self.balance.lean_factor } else { 0.0 };
            let target = target.clamp(-self.balance.max_lean.to_radians(), self.balance.max_lean.to_radians());

            // eases in at the end instead of stopping at once, which would make the lean overshoot
            let most = self.balance.roll_rate.max(1.0).to_radians();
            let goal_rate = ((target - self.lean_goal) * LEAN_GOAL_RESPONSE).clamp(-most, most);
            self.lean_goal += goal_rate * dt;
            lean_target = target;

            let lean_axis = -forward_world;
            let roll_rate = angvel.dot(&lean_axis);

            // on the wheels the weight tips a leaning vehicle further - a rider holds that, so the lean follows the target
            let ground = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.up) - wheel.radius).sum::<f32>() / wheel_amount;
            let height = (self.chassis.center_of_mass.dot(&frame.up) - ground).max(0.0);
            let grounded = contacts.iter().filter(|contact| contact.contact).count() as f32 / wheel_amount;
            let tipping = mass * EARTH_GRAVITY * height * lean.sin() * grounded;

            torque_world += lean_axis * (inertia_along(&lean_axis) * (self.balance.stiffness * (self.lean_goal - lean) + self.balance.damping * (goal_rate - roll_rate)) - tipping);
        }

        // tracks: the yaw assist makes turning on the spot work despite the track grip
        if tracked
        {
            let wanted = self.steer * self.tracks.turn_rate.to_radians() * if speed < -STANDSTILL_SPEED { -1.0 } else { 1.0 };
            let current = angvel.dot(&up_world);

            // the tracks scrub sideways while turning - that drag is taken off, so the assist reaches the turn rate
            let scrub = side_torque.dot(&up_world) * (self.steer.abs() * 4.0).min(1.0);

            torque_world += up_world * (inertia_along(&up_world) * self.tracks.turn_assist * (wanted - current) - scrub);
        }

        // ********** recover **********
        self.upside_down_time = if up_world.y < UPSIDE_DOWN_DOT { self.upside_down_time + dt } else { 0.0 };
        let recover = input.recover || (self.recover.auto && self.upside_down_time > self.recover.delay);

        // ********** write the physics **********
        {
            let tuning = self.wheel_tuning(mass);

            // the springs carry what rests on them through the couplings too - same ride height and damping as alone
            let load = physics.vehicle_load_factor(node_id);

            if let Some((vehicle, body)) = physics.vehicle_mut(node_id)
            {
                for (wheel, command) in vehicle.controller.wheels_mut().iter_mut().zip(commands.iter())
                {
                    wheel.engine_force = command.engine_force;
                    wheel.brake = command.brake;
                    wheel.steering = command.steering;
                    wheel.friction_slip = command.grip;
                    wheel.side_friction_stiffness = command.side_grip;

                    // live tuning, no rebuild needed
                    wheel.suspension_stiffness = tuning.suspension_stiffness * load;
                    wheel.damping_compression = tuning.suspension_compression * load;
                    wheel.damping_relaxation = tuning.suspension_damping * load;
                    wheel.max_suspension_travel = tuning.max_suspension_travel;
                    wheel.max_suspension_force = tuning.max_suspension_force * load;
                }

                // a trailer is woken by its tow vehicle through the hitch - its parking brake must not keep it awake
                let active = !trailer && (throttle > 0.0 || brake > 0.0 || !approx_zero(self.steer) || input.handbrake || recover);

                body.reset_forces(false);
                body.reset_torques(false);

                // gravity is not enough to wake a parked vehicle
                if active
                {
                    body.wake_up(true);
                }

                if !body.is_sleeping()
                {
                    body.add_force(Vector::new(force.x, force.y, force.z), false);
                    body.add_torque(Vector::new(torque_world.x, torque_world.y, torque_world.z), false);
                }
            }
        }

        if recover
        {
            self.recover_vehicle(physics, node_id, &rotation, &frame);
        }

        Some(DriveState { speed, lateral_left, steer_angle: wheel_angle, brake, wheelspin: self.wheelspin, lean, lean_target, telemetry_wheels })
    }

    fn wheelbase(&self, frame: &VehicleFrame) -> f32
    {
        let along: Vec<f32> = self.wheels.iter().map(|wheel| wheel.center.dot(&frame.forward)).collect();
        let max = along.iter().copied().fold(f32::MIN, f32::max);
        let min = along.iter().copied().fold(f32::MAX, f32::min);

        if along.is_empty() { 0.0 } else { max - min }
    }

    // Back on the wheels, facing where it faced, a bit above where it lay.
    fn recover_vehicle(&mut self, physics: &mut PhysicsWorld, node_id: u32, rotation: &UnitQuaternion<f32>, frame: &VehicleFrame)
    {
        let Some((_, body)) = physics.vehicle(node_id) else { return; };
        let position = body.position().translation;

        let heading = rotation * frame.forward;
        let heading = Vector3::new(heading.x, 0.0, heading.z);
        let heading = if heading.norm() > 0.01 { heading.normalize() } else { -Vector3::z() };

        let world_basis = Matrix3::from_columns(&[heading, Vector3::y(), heading.cross(&Vector3::y())]);
        let chassis_basis = Matrix3::from_columns(&[frame.forward, frame.up, frame.right()]);

        let upright = UnitQuaternion::from_matrix(&(world_basis * chassis_basis.transpose()));

        let pose = Pose::from_parts(Vector::new(position.x, position.y + self.recover.lift, position.z), Rotation::from_xyzw(upright.i, upright.j, upright.k, upright.w));
        physics.place_vehicle(node_id, pose);

        self.engine_state.reset(&self.engine);
        self.drift_amount = 0.0;
        self.wheelspin = 0.0;
        self.lean_goal = 0.0;
        self.upside_down_time = 0.0;
    }
}

// ********** ui **********

// a node below the vehicle to pick from - label tells apart nodes with the same name
struct PartChoice
{
    uuid: String,
    name: String,
    label: String,
}

fn part_choices(node: &NodeItem) -> Vec<PartChoice>
{
    let children = Scene::list_all_child_nodes(&node.read().unwrap().nodes);
    let mut seen: HashMap<String, usize> = HashMap::new();

    children.iter().map(|child|
    {
        let child = child.read().unwrap();
        let count = seen.entry(child.name.clone()).or_insert(0);
        *count += 1;

        let label = if *count > 1 { format!("{} #{}", child.name, count) } else { child.name.clone() };
        PartChoice { uuid: child.uuid.clone(), name: child.name.clone(), label }
    }).collect()
}

fn part_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, name: &mut String, uuid: &mut String, choices: &[PartChoice]) -> bool
{
    let before = uuid.clone();
    let before_name = name.clone();

    let selected = choices.iter().find(|choice| !uuid.is_empty() && choice.uuid == *uuid).map(|choice| choice.label.clone()).unwrap_or_else(|| if name.is_empty() { "-".to_string() } else { name.clone() });

    egui::ComboBox::from_id_salt(id).selected_text(selected).width(160.0).show_ui(ui, |ui|
    {
        if ui.selectable_label(name.is_empty(), "-").clicked()
        {
            name.clear();
            uuid.clear();
        }

        for choice in choices
        {
            if ui.selectable_label(*uuid == choice.uuid, &choice.label).clicked()
            {
                *name = choice.name.clone();
                *uuid = choice.uuid.clone();
            }
        }
    });

    before != *uuid || before_name != *name
}

impl VehicleController
{
    pub fn mark_physics_dirty(&mut self)
    {
        self.physics_dirty = true;
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui, scene: &mut Scene, context: &ControllerUiContext)
    {
        if self.node.is_none()
        {
            self.release_exclusions(scene);
        }

        ui.horizontal(|ui|
        {
            ui.label("Vehicle Target Name: ");
            ui.text_edit_singleline(&mut self.node_name);
        });

        let trailer = self.vehicle_type.is_trailer();

        if !trailer
        {
            ui.horizontal(|ui|
            {
                ui.label("Camera Target Name: ");
                ui.label("ℹ").on_hover_text("leave empty for main active camera");
                ui.text_edit_singleline(&mut self.cam_name);
            });
        }

        ui.horizontal(|ui|
        {
            combo(ui, "vehicle_type", "Vehicle Type", "", &mut self.vehicle_type, &VehicleType::all());

            if ui.button("Apply Preset").on_hover_text("mass, engine, suspension, tires and brakes for this type - replaces the current values, the sound settings stay").clicked()
            {
                self.apply_preset();
            }

            let size_hint = match self.measured_length()
            {
                Some(length) => format!("like Apply Preset, with mass, suspension, brakes and engine scaled to the measured size: {:.2} m long, the preset is made for {:.1} m (factor {:.2})", length, self.vehicle_type.reference_length(), length / self.vehicle_type.reference_length()),
                None => "run the auto setup first - the size is measured from it".to_string(),
            };

            if ui.add_enabled(self.measured_length().is_some(), egui::Button::new("Preset For Size")).on_hover_text(size_hint.clone()).on_disabled_hover_text(size_hint).clicked()
            {
                self.apply_preset_for_size();
            }
        });

        combo(ui, "vehicle_forward", "Forward", "where the vehicle faces as it stands in the editor - Auto reads it from wheel names like 'front left', otherwise from the axis the wheels spread along (+z of the model). Re-run the auto setup after a change", &mut self.forward_axis, &[VehicleForward::Auto, VehicleForward::WorldNegZ, VehicleForward::WorldPosZ, VehicleForward::WorldNegX, VehicleForward::WorldPosX]);

        if trailer
        {
            ui.label(RichText::new("a trailer has no engine, driver or camera - couple it in the Trailer Hitch section of the vehicle that tows it").color(Color32::GRAY));
        }
        else if combo(ui, "vehicle_drive", "Drive", "which axles are driven - Tracked drives the sides against each other instead of steering", &mut self.drive, &[VehicleDrive::Front, VehicleDrive::Rear, VehicleDrive::All, VehicleDrive::Tracked])
        {
            self.assign_wheel_roles();
        }

        if ui.button("Run Auto Setup").on_hover_text("finds the wheels (by name, otherwise by shape), the chassis, the steering wheel and sets up the camera").clicked()
        {
            self.auto_setup(scene, self.node_name.clone().as_str(), self.cam_name.clone().as_str());
        }

        if self.frame.is_none()
        {
            ui.colored_label(egui::Color32::from_rgb(220, 160, 60), "not set up yet - run the auto setup");
        }

        ui.separator();

        let parts: Vec<PartChoice> = self.node.as_ref().map(part_choices).unwrap_or_default();

        let mut dirty = false;
        let mut runtime_dirty = false;
        let mut picked_wheels = vec![];

        // ********** wheels **********
        egui::CollapsingHeader::new(format!("Wheels ({})", self.wheels.len())).id_salt("vehicle_wheels").default_open(true).show(ui, |ui|
        {
            let mut remove = None;

            for (index, wheel) in self.wheels.iter_mut().enumerate()
            {
                ui.horizontal(|ui|
                {
                    if part_combo(ui, ("vehicle_wheel_node", index), &mut wheel.node_name, &mut wheel.node_uuid, &parts)
                    {
                        picked_wheels.push(index);
                        runtime_dirty = true;
                    }

                    ui.label("r:");
                    dirty |= ui.add(egui::DragValue::new(&mut wheel.radius).speed(0.005).range(0.01..=5.0)).on_hover_text("radius in m").changed();

                    ui.label("steer:");
                    ui.add(egui::DragValue::new(&mut wheel.steer).speed(0.01).range(-1.0..=1.0)).on_hover_text("share of the steering angle, negative steers the other way");

                    ui.checkbox(&mut wheel.driven, "driven");
                    ui.checkbox(&mut wheel.handbrake, "handbrake");

                    if ui.button("🗑").clicked()
                    {
                        remove = Some(index);
                    }
                });
            }

            if let Some(index) = remove
            {
                if let Some(runtime) = self.wheels[index].runtime.as_ref()
                {
                    runtime.reset();
                }

                self.wheels.remove(index);
                dirty = true;
            }

            ui.horizontal(|ui|
            {
                if ui.button("Add Wheel").on_hover_text("pick the node, its center and radius are measured").clicked()
                {
                    self.wheels.push(VehicleWheel::new("", Vector3::zeros(), 0.35));
                }

                if ui.button("Assign Roles").on_hover_text("steering, drive and handbrake by axle, from the vehicle type and the drive").clicked()
                {
                    self.assign_wheel_roles();
                }
            });
        });

        // ********** chassis **********
        egui::CollapsingHeader::new("Chassis").id_salt("vehicle_chassis").show(ui, |ui|
        {
            dirty |= combo(ui, "vehicle_chassis_shape", "Shape", "the collision shape of the body - the wheels are rays and never collide themselves", &mut self.chassis.shape, &[ChassisShape::Box, ChassisShape::ConvexHull, ChassisShape::Compound]);
            dirty |= slider(ui, "Mass", "kg", &mut self.chassis.mass, 10.0..=80000.0, 0);

            dirty |= ui.checkbox(&mut self.chassis.center_of_mass_auto, "Auto Center Of Mass").changed();

            if self.chassis.center_of_mass_auto
            {
                dirty |= slider(ui, "Center Of Mass Height", "share of the chassis height, from its bottom - lower is harder to roll over", &mut self.chassis.center_of_mass_height, 0.0..=1.0, 2);
            }
            else
            {
                dirty |= vector_edit(ui, "Center Of Mass", "chassis space, m", &mut self.chassis.center_of_mass);
            }

            dirty |= slider(ui, "Friction", "of the body against walls and other objects", &mut self.chassis.friction, 0.0..=2.0, 2);
            dirty |= slider(ui, "Restitution", "", &mut self.chassis.restitution, 0.0..=1.0, 2);
            dirty |= slider(ui, "Linear Damping", "", &mut self.chassis.linear_damping, 0.0..=2.0, 2);
            dirty |= slider(ui, "Angular Damping", "", &mut self.chassis.angular_damping, 0.0..=5.0, 2);
            dirty |= slider(ui, "Min Ground Clearance", "share of the wheel radius the collision body stays above the wheel bottom - a body reaching down to the ground catches on every ramp edge", &mut self.chassis.min_clearance, 0.0..=2.0, 2);
            dirty |= slider(ui, "Edge Rounding", "m the edges of the collision body are rounded by - they slide up a ramp start or a loop entry instead of catching on it. The body keeps its outer size", &mut self.chassis.rounding, 0.0..=1.0, 2);
            dirty |= ui.checkbox(&mut self.chassis.sloped_ends, "Sloped Ends").on_hover_text("the collision bottom rises above the axles in front of the front and behind the rear axle - the approach and departure angle of a real car, so the nose does not catch on ramps and loop entries").changed();
            dirty |= ui.checkbox(&mut self.chassis.wheel_bumpers, "Wheel Bumpers").on_hover_text("frictionless balls at the wheels - the wheel rays only see an edge once the wheel is above it, the balls slide the vehicle up over curbs and ramps").changed();
        });

        // ********** suspension **********
        egui::CollapsingHeader::new("Suspension").id_salt("vehicle_suspension").show(ui, |ui|
        {
            dirty |= slider(ui, "Rest Length", "spring length without load, m", &mut self.suspension.rest_length, 0.01..=1.5, 3);
            dirty |= slider(ui, "Travel", "how far the spring moves from the rest length, m", &mut self.suspension.travel, 0.01..=1.0, 3);
            dirty |= slider(ui, "Stiffness", "per unit of mass - the vehicle sinks by g / (wheels * stiffness)", &mut self.suspension.stiffness, 1.0..=200.0, 1);
            slider(ui, "Compression Damping", "", &mut self.suspension.compression, 0.0..=20.0, 2);
            slider(ui, "Relaxation Damping", "raise this if the vehicle keeps bouncing", &mut self.suspension.relaxation, 0.0..=20.0, 2);
            slider(ui, "Max Force", "times the vehicle weight, per wheel", &mut self.suspension.max_force_factor, 0.5..=10.0, 1);
            dirty |= ui.checkbox(&mut self.suspension.keep_ride_height, "Keep Ride Height").on_hover_text("mounts the springs so the loaded vehicle stands exactly as modelled").changed();
        });

        // ********** tires **********
        egui::CollapsingHeader::new("Tires").id_salt("vehicle_tires").show(ui, |ui|
        {
            slider(ui, "Grip", "roughly the tire friction coefficient", &mut self.tires.grip, 0.1..=5.0, 2);
            slider(ui, "Side Grip", "", &mut self.tires.side_grip, 0.1..=3.0, 2);
            ui.checkbox(&mut self.tires.surface_grip, "Surface Grip").on_hover_text("scales the grip with the friction of the ground (its physics friction, 0.7 = normal)");
        });

        // ********** steering **********
        if !trailer
        {
            egui::CollapsingHeader::new("Steering").id_salt("vehicle_steering").show(ui, |ui|
            {
                slider(ui, "Max Angle", "degrees", &mut self.steering.max_angle, 0.0..=70.0, 1);
                slider(ui, "Steer Speed", "keyboard, 1/s", &mut self.steering.speed, 0.5..=15.0, 1);
                slider(ui, "Return Speed", "keyboard, 1/s", &mut self.steering.return_speed, 0.5..=20.0, 1);
                slider(ui, "High Speed", "km/h at which the lock is reduced to the factor below", &mut self.steering.high_speed, 10.0..=300.0, 0);
                slider(ui, "High Speed Factor", "", &mut self.steering.high_speed_factor, 0.05..=1.0, 2);

                ui.horizontal(|ui|
                {
                    ui.label("Steering Node: ");
                    ui.label("ℹ").on_hover_text("steering wheel or handlebar that turns with the steering");
                    runtime_dirty |= part_combo(ui, "vehicle_steering_node", &mut self.steering.node_name, &mut self.steering.node_uuid, &parts);
                });

                runtime_dirty |= combo(ui, "vehicle_steering_axis", "Steering Axis", "Column: a steering wheel, Handlebar: turns around the vehicle up axis", &mut self.steering.axis, &[SteeringAxis::Column, SteeringAxis::Handlebar]);

                if self.steering.axis == SteeringAxis::Column
                {
                    slider(ui, "Ratio", "steering wheel turn per wheel turn", &mut self.steering.ratio, 1.0..=30.0, 1);
                }
            });
        }

        // ********** brakes **********
        egui::CollapsingHeader::new("Brakes & Drag").id_salt("vehicle_brakes").show(ui, |ui|
        {
            slider(ui, "Brake Force", "N, all wheels", &mut self.brakes.brake_force, 0.0..=500000.0, 0);
            slider(ui, "Handbrake Force", "N, the handbrake wheels", &mut self.brakes.handbrake_force, 0.0..=500000.0, 0);
            slider(ui, "Rolling Resistance", "N, all wheels", &mut self.brakes.rolling_resistance, 0.0..=20000.0, 0);
            slider(ui, "Air Drag", "force = drag * speed²", &mut self.brakes.air_drag, 0.0..=10.0, 2);
        });

        // ********** drift **********
        if !trailer
        {
            egui::CollapsingHeader::new("Drift").id_salt("vehicle_drift").show(ui, |ui|
            {
                slider(ui, "Handbrake Grip", "grip of the handbrake wheels while it is pulled", &mut self.drift.handbrake_grip, 0.0..=1.0, 2);
                slider(ui, "Handbrake Side Grip", "", &mut self.drift.handbrake_side_grip, 0.0..=1.0, 2);
                slider(ui, "Grip Recovery", "1/s after the handbrake is released", &mut self.drift.grip_recovery, 0.1..=10.0, 2);
                slider(ui, "Throttle Hold", "share of the recovery left while on throttle and sliding - keeps the drift going", &mut self.drift.throttle_hold, 0.0..=1.0, 2);
                slider(ui, "Counter Steer", "steers into the slide by this share of the slip angle", &mut self.drift.counter_steer, 0.0..=1.5, 2);        });
        }

        // ********** balance **********
        if !trailer
        {
            egui::CollapsingHeader::new("Balance (Two Wheelers)").id_salt("vehicle_balance").show(ui, |ui|
            {
                ui.checkbox(&mut self.balance.enabled, "Enabled").on_hover_text("keeps the vehicle upright and leans it into curves");
                slider(ui, "Max Lean", "degrees", &mut self.balance.max_lean, 0.0..=70.0, 1);
                slider(ui, "Lean Factor", "share of the physically right lean angle", &mut self.balance.lean_factor, 0.0..=1.5, 2);
                slider(ui, "Stiffness", "1/s²", &mut self.balance.stiffness, 1.0..=300.0, 1);
                slider(ui, "Damping", "1/s", &mut self.balance.damping, 0.0..=60.0, 1);
                slider(ui, "Roll Rate", "degrees/s the wanted lean changes by at most - lower is a calmer flick from one side to the other", &mut self.balance.roll_rate, 10.0..=400.0, 0);
            });
        }

        // ********** tracks **********
        if self.drive == VehicleDrive::Tracked
        {
            egui::CollapsingHeader::new("Tracks").id_salt("vehicle_tracks").show(ui, |ui|
            {
                slider(ui, "Turn Force", "share of the first gear force, pushing the sides against each other", &mut self.tracks.turn_force, 0.0..=2.0, 2);
                slider(ui, "Turn Rate", "degrees/s at full lock", &mut self.tracks.turn_rate, 0.0..=120.0, 1);
                slider(ui, "Turn Assist", "how hard the turn rate is pulled toward, 1/s", &mut self.tracks.turn_assist, 0.0..=20.0, 1);
                slider(ui, "Turn Side Grip", "tracks have to slide sideways to turn", &mut self.tracks.turn_side_grip, 0.0..=1.0, 2);
            });
        }

        // ********** engine **********
        if !trailer
        {
            egui::CollapsingHeader::new("Engine & Gearbox").id_salt("vehicle_engine").show(ui, |ui|
            {
                use super::vehicle::engine::EngineType;

                combo(ui, "vehicle_engine_type", "Engine Type", "Electric: full torque from zero, one gear - Pedal: a rider, the rpm is the cadence", &mut self.engine.engine_type, &[EngineType::Combustion, EngineType::Electric, EngineType::Pedal]);
                slider(ui, "Idle RPM", "", &mut self.engine.idle_rpm, 0.0..=3000.0, 0);
                slider(ui, "Max RPM", "rev limiter", &mut self.engine.max_rpm, 100.0..=20000.0, 0);
                slider(ui, "Max Torque", "Nm", &mut self.engine.max_torque, 1.0..=10000.0, 0);
                slider(ui, "Peak Torque RPM", "electric: above this the power stays constant", &mut self.engine.peak_torque_rpm, 10.0..=15000.0, 0);

                ui.horizontal(|ui|
                {
                    ui.label("Gears: ");
                    ui.label("ℹ").on_hover_text("forward gear ratios, first gear first - automatic gearbox");

                    let mut text = self.engine.gear_ratios.iter().map(|ratio| format!("{}", ratio)).collect::<Vec<_>>().join(", ");
                    if ui.text_edit_singleline(&mut text).changed()
                    {
                        let ratios: Vec<f32> = text.split(',').filter_map(|part| part.trim().parse::<f32>().ok()).filter(|ratio| *ratio > 0.0).collect();
                        if !ratios.is_empty()
                        {
                            self.engine.gear_ratios = ratios;
                        }
                    }
                });

                slider(ui, "Reverse Ratio", "", &mut self.engine.reverse_ratio, 0.1..=20.0, 2);
                slider(ui, "Final Drive", "", &mut self.engine.final_drive, 0.1..=20.0, 2);
                slider(ui, "Efficiency", "", &mut self.engine.efficiency, 0.1..=1.0, 2);
                slider(ui, "Shift Up RPM", "", &mut self.engine.shift_up_rpm, 10.0..=20000.0, 0);
                slider(ui, "Shift Down RPM", "", &mut self.engine.shift_down_rpm, 10.0..=20000.0, 0);
                slider(ui, "Shift Time", "s without drive while shifting", &mut self.engine.shift_time, 0.0..=1.5, 2);
                slider(ui, "Engine Braking", "share of the max torque that drags while off throttle", &mut self.engine.engine_braking, 0.0..=1.0, 2);
                slider(ui, "Top Speed", "km/h, 0 = only drag and gearing limit it", &mut self.engine.top_speed, 0.0..=400.0, 0);
                slider(ui, "Max Reverse Speed", "km/h", &mut self.engine.max_reverse_speed, 0.0..=100.0, 0);
            });
        }

        // ********** camera **********
        if !trailer
        {
            egui::CollapsingHeader::new("Camera").id_salt("vehicle_camera").show(ui, |ui|
            {
                combo(ui, "vehicle_camera_mode", "Mode", "C or the right stick switches while driving", &mut self.camera.mode, &[VehicleCameraMode::Chase, VehicleCameraMode::Cockpit]);
                ui.checkbox(&mut self.camera.distance_auto, "Auto Distance").on_hover_text("from the vehicle size, on the next auto setup");
                slider(ui, "Distance", "chase camera, used by the setup - scroll to change it while driving", &mut self.camera.distance, 1.0..=50.0, 1);
                slider(ui, "Height", "above the chassis center", &mut self.camera.height, -2.0..=10.0, 2);
                ui.checkbox(&mut self.camera.follow, "Swing In Behind").on_hover_text("the camera turns behind the vehicle while driving forward");
                slider(ui, "Follow Speed", "1/s", &mut self.camera.follow_speed, 0.1..=15.0, 1);
                slider(ui, "Follow Delay", "s after orbiting with the mouse or stick", &mut self.camera.follow_delay, 0.0..=10.0, 1);
                slider(ui, "Slide Follow", "0 = behind the nose, 1 = behind where the vehicle actually goes - a drift then shows the car sliding out", &mut self.camera.slide_follow, 0.0..=1.0, 2);
                ui.checkbox(&mut self.camera.cockpit_auto, "Auto Cockpit Position").on_hover_text("above the seat, on the next auto setup");
                vector_edit(ui, "Cockpit Position", "eye point, chassis space, m", &mut self.camera.cockpit_offset);
            });
        }

        // ********** seats **********
        if !trailer
        {
            egui::CollapsingHeader::new(format!("Seats ({})", self.seats.len())).id_salt("vehicle_seats").show(ui, |ui|
            {
                ui.label("the first driver seat gives the cockpit view its head - nodes outside the vehicle are put on their seats and carried along");

                let mut remove = None;

                for (index, seat) in self.seats.iter_mut().enumerate()
                {
                    ui.push_id(("vehicle_seat", index), |ui|
                    {
                        ui.horizontal(|ui|
                        {
                            ui.label(RichText::new(format!("Seat {}", index + 1)).strong());
                            if ui.button("🗑").on_hover_text("remove this seat").clicked()
                            {
                                remove = Some(index);
                            }
                        });

                        combo(ui, &format!("vehicle_seat_role_{}", index), "Role", "", &mut seat.role, &[SeatRole::Driver, SeatRole::Passenger]);

                        ui.horizontal(|ui|
                        {
                            ui.label("Node: ");
                            ui.label("ℹ").on_hover_text("any node in the scene - one outside the vehicle is put on the seat and carried along");
                            runtime_dirty |= ui.text_edit_singleline(&mut seat.node_name).lost_focus();
                        });

                        ui.horizontal(|ui|
                        {
                            ui.label("Sit Animation: ");
                            ui.label("ℹ").on_hover_text("regex of the clip name, played looped");
                            runtime_dirty |= ui.text_edit_singleline(&mut seat.animation).lost_focus();
                        });

                        ui.checkbox(&mut seat.seat_auto, "Auto Seat").on_hover_text("the auto setup takes where the node currently is as the seat");
                        runtime_dirty |= vector_edit(ui, "Seat Position", "chassis space, m - where the node origin goes", &mut seat.seat_position);
                        runtime_dirty |= slider(ui, "Seat Rotation", "degrees around the vehicle up axis - 0 turns the node to look along its -z", &mut seat.seat_rotation, -180.0..=180.0, 1);
                        ui.checkbox(&mut seat.hide_in_cockpit, "Hide In Cockpit View");
                        ui.separator();
                    });
                }

                if let Some(index) = remove
                {
                    self.set_cockpit_visibility(false);
                    self.seats.remove(index);
                    runtime_dirty = true;
                }

                if ui.button("➕ Add Seat").clicked()
                {
                    // the first one drives, the next sits beside the driver
                    let seat = match self.driver_seat().map(|index| self.seats[index].clone())
                    {
                        Some(driver) =>
                        {
                            let mut seat = VehicleSeat::new(SeatRole::Passenger);
                            seat.seat_rotation = driver.seat_rotation;
                            seat.seat_position = driver.seat_position;
                            if let Some(frame) = self.frame
                            {
                                seat.seat_position -= frame.right() * (driver.seat_position.dot(&frame.right()) * 2.0);
                            }
                            seat
                        }
                        None => VehicleSeat::new(SeatRole::Driver),
                    };

                    self.seats.push(seat);
                }
            });
        }

        // ********** recover **********
        egui::CollapsingHeader::new("Recover").id_salt("vehicle_recover").show(ui, |ui|
        {
            ui.label("the recover control puts the vehicle back on its wheels");
            ui.checkbox(&mut self.recover.auto, "Auto Recover").on_hover_text("after lying on the side or the roof for the delay below");
            slider(ui, "Delay", "s", &mut self.recover.delay, 0.5..=20.0, 1);
            slider(ui, "Lift", "m", &mut self.recover.lift, 0.0..=5.0, 2);
        });

        // ********** hitch **********
        let mut hitch_dirty = false;

        egui::CollapsingHeader::new("Trailer Hitch").id_salt("vehicle_hitch").show(ui, |ui|
        {
            ui.label("the trailer is a scene node outside this vehicle with its own vehicle controller of the type Trailer - it is coupled where it stands when the run starts, and a recover puts it back behind the vehicle");

            ui.horizontal(|ui|
            {
                ui.label("Trailer Node: ");
                ui.label("ℹ").on_hover_text("empty = no trailer");

                if ui.text_edit_singleline(&mut self.hitch.trailer_name).lost_focus()
                {
                    self.trailer = if self.hitch.trailer_name.is_empty() { OptionOrId::None } else { scene.find_node_by_name(&self.hitch.trailer_name).map(OptionOrId::Some).unwrap_or(OptionOrId::None) };
                    hitch_dirty = true;
                }
            });

            if !self.hitch.trailer_name.is_empty() && self.trailer.is_none()
            {
                ui.colored_label(Color32::from_rgb(220, 160, 60), "trailer node not found");
            }

            hitch_dirty |= ui.checkbox(&mut self.hitch.point_auto, "Auto Hitch Point").on_hover_text("a part of the trailer named like 'hitch', 'coupling' or 'kingpin', otherwise the middle of its front tip - measured at the run start").changed();

            if self.hitch.point_auto
            {
                ui.label(RichText::new(format!("hitch point: {:.2} / {:.2} / {:.2}", self.hitch.point.x, self.hitch.point.y, self.hitch.point.z)).color(Color32::GRAY));
            }
            else
            {
                hitch_dirty |= vector_edit(ui, "Hitch Point", "the ball, chassis space, m", &mut self.hitch.point);
            }

            hitch_dirty |= slider(ui, "Yaw Limit", "degrees to each side the trailer swings around the ball - 180 = free", &mut self.hitch.yaw_limit, 1.0..=180.0, 0);
            hitch_dirty |= slider(ui, "Pitch Limit", "degrees up and down - over crests and through dips", &mut self.hitch.pitch_limit, 1.0..=180.0, 0);
            hitch_dirty |= slider(ui, "Roll Limit", "degrees the trailer rolls against the vehicle - at the limit a falling trailer pulls the vehicle along", &mut self.hitch.roll_limit, 1.0..=180.0, 0);
            hitch_dirty |= slider(ui, "Tear Off Roll", "degrees of roll against the vehicle that tear the trailer off - 0 = never, keep it below the roll limit", &mut self.hitch.break_roll, 0.0..=180.0, 0);
            hitch_dirty |= slider(ui, "Tear Off Force", "kN at the ball that tear the trailer off, e.g. in a crash - 0 = never", &mut self.hitch.break_force, 0.0..=2000.0, 0);
            ui.checkbox(&mut self.hitch.trailer_brakes, "Trailer Brakes").on_hover_text("the trailer brakes along with the vehicle, a train passes it on");
        });

        if hitch_dirty
        {
            self.hitch_dirty = true;
        }

        // ********** controls **********
        if !trailer
        {
            egui::CollapsingHeader::new("Controls").id_salt("vehicle_controls").show(ui, |ui|
            {
                let id = format!("vehicle_controls_{}", self.node_name);
                gamepad_select_ui(ui, &id, &mut self.controls.gamepad);

                let controls = &mut self.controls;
                for (label, action) in [("Throttle", &mut controls.throttle), ("Brake / Reverse", &mut controls.brake), ("Steer Left", &mut controls.steer_left), ("Steer Right", &mut controls.steer_right), ("Handbrake", &mut controls.handbrake), ("Recover", &mut controls.recover), ("Camera", &mut controls.camera), ("Look Left", &mut controls.look_left), ("Look Right", &mut controls.look_right), ("Look Up", &mut controls.look_up), ("Look Down", &mut controls.look_down), ("Zoom In", &mut controls.zoom_in), ("Zoom Out", &mut controls.zoom_out)]
                {
                    input_action_ui(ui, &id, label, action);
                }
                slider(ui, "Look Speed", "deg/s at full stick, chase camera", &mut self.controls.look_speed, 10.0..=500.0, 0);
                slider(ui, "Zoom Speed", "1/s, chase camera - the distance changes by this factor per second, as e^speed", &mut self.controls.zoom_speed, 0.1..=5.0, 1);

                if ui.button("Reset to Default").clicked()
                {
                    self.controls = VehicleControls::default();
                }
            });
        }

        // ********** sound **********
        if !trailer
        {
            egui::CollapsingHeader::new("Sound").id_salt("vehicle_sound").show(ui, |ui|
            {
                use super::vehicle::engine_sound::EngineSoundLayer;

                // the sound components of the vehicle node - volume, spatial and distance are set on them
                let node = self.node.as_ref().cloned();
                let components = match node.as_ref()
                {
                    Some(node) =>
                    {
                        self.release_detached_sounds(node);
                        node.read().unwrap().find_components::<Sound>()
                    },
                    None => vec![],
                };
                let sources = &context.sound_sources;

                ui.checkbox(&mut self.sound.enabled, "Enabled");

                if sources.is_empty() && components.is_empty()
                {
                    ui.label(RichText::new("no sound resources - drop sound files into the scene, or add them under Resources > Sound Sources (right click)").color(Color32::GRAY));
                }
                else if components.is_empty()
                {
                    ui.label(RichText::new("pick a sound resource below - it becomes a Sound component of the vehicle node, with its volume and spatial settings").color(Color32::GRAY));
                }

                slider(ui, "Pitch Variation", "random rpm wobble as a share of the rpm - keeps full throttle from sounding like one flat tone", &mut self.sound.pitch_variation, 0.0..=0.1, 3);

                ui.horizontal(|ui|
                {
                    ui.label("Engine Layers");
                    ui.label("ℹ").on_hover_text("looped sound components of the vehicle node, each with the rpm it was recorded at - they are crossfaded and pitched by the engine rpm. Layers with 'on throttle' off are the overrun sound, blended in when letting go of the throttle");
                });

                let mut remove = None;
                for (index, layer) in self.sound.engine_layers.iter_mut().enumerate()
                {
                    ui.horizontal(|ui|
                    {
                        sound_component_combo(ui, format!("vehicle_sound_layer_{}", index), &mut layer.sound, &components, sources, node.as_ref(), "Engine ");
                        ui.label("rpm:");
                        ui.add(egui::DragValue::new(&mut layer.rpm).speed(10.0).range(1.0..=30000.0));

                        let mut on_throttle = layer.load >= 0.5;
                        if ui.checkbox(&mut on_throttle, "on throttle").changed()
                        {
                            layer.load = if on_throttle { 1.0 } else { 0.0 };
                        }

                        if ui.button("🗑").clicked()
                        {
                            remove = Some(index);
                        }
                    });
                }

                if let Some(index) = remove
                {
                    self.sound.engine_layers.remove(index);
                }

                if ui.button("Add Layer").clicked()
                {
                    self.sound.engine_layers.push(EngineSoundLayer { sound: OptionOrId::None, rpm: 3000.0, load: 1.0 });
                }

                for (label, hint, sound, prefix) in [("Squeal: ", "the tires sliding sideways or locked by the brakes", &mut self.sound.squeal, "Squeal "), ("Road: ", "rolling noise, rises with the speed", &mut self.sound.road, "Road ")]
                {
                    ui.horizontal(|ui|
                    {
                        ui.label(label).on_hover_text(hint);
                        sound_component_combo(ui, format!("vehicle_sound_{}", label), sound, &components, sources, node.as_ref(), prefix);
                    });
                }
            });
        }

        // ********** tire marks **********
        egui::CollapsingHeader::new("Tire Marks").id_salt("vehicle_tire_marks").show(ui, |ui|
        {
            // what the auto setup measured, across the tires
            let measured: Vec<String> = self.wheels.iter().map(|wheel| if wheel.width > 0.0 { format!("{:.2}", wheel.width) } else { "?".to_string() }).collect();
            let marks = &mut self.tire_marks;

            ui.checkbox(&mut marks.enabled, "Enabled").on_hover_text("rubber on the ground where the tires slide sideways, lock or spin - cleared when leaving play");
            slider(ui, "Opacity", "at a full slide - a light slide leaves a lighter mark", &mut marks.opacity, 0.0..=1.0, 2);

            ui.horizontal(|ui|
            {
                ui.checkbox(&mut marks.width_auto, "Auto Width").on_hover_text("the width of each tire from its bounding box, measured by the auto setup - '?' = not measured yet, then a share of the radius is used until the next auto setup");
                ui.label(RichText::new(format!("{} m", measured.join(" / "))).color(Color32::GRAY));
            });

            ui.add_enabled_ui(!marks.width_auto, |ui|
            {
                slider(ui, "Width", "m, for all tires - while Auto Width is off", &mut marks.width, 0.02..=1.5, 2);
            });
            slider(ui, "Piece Length", "m - shorter follows curves closer, but fills the pieces faster", &mut marks.segment_length, 0.05..=1.0, 2);

            ui.horizontal(|ui|
            {
                ui.label("Max Pieces:").on_hover_text("the oldest marks are reused beyond it");
                ui.add(egui::DragValue::new(&mut marks.max_pieces).speed(10.0).range(10..=20000));
            });

            ui.horizontal(|ui|
            {
                ui.label("Color:");
                let mut color = [marks.color.x, marks.color.y, marks.color.z];
                if ui.color_edit_button_rgb(&mut color).changed()
                {
                    marks.color = Vector3::new(color[0], color[1], color[2]);
                }
            });
        });

        // ********** telemetry **********
        egui::CollapsingHeader::new("Telemetry").id_salt("vehicle_telemetry").show(ui, |ui|
        {
            let t = &self.telemetry;

            ui.label(format!("{:.0} km/h, {:.0} rpm, gear {}", t.speed_kmh, t.rpm, t.gear));
            ui.label(format!("throttle {:.2}, brake {:.2}, steer {:.2}", t.throttle, t.brake, t.steer));
            ui.label(format!("drift {:.2}, wheelspin {:.2}, skid {:.2}, lean {:.1}°", t.drift, t.wheelspin, t.skid, t.lean));

            for (index, wheel) in t.wheels.iter().enumerate()
            {
                let contact = if wheel.contact { "ground" } else { "air" };
                ui.label(format!("wheel {}: {}, compression {:.3} m, grip {:.2}, drive {:.0} N, brake {:.0} N", index, contact, wheel.compression, wheel.grip, wheel.engine_force, wheel.brake));
            }

            let name = |target: &ContactTarget| match target.node_id()
            {
                Some(id) => scene.find_node_by_id(id).map(|node| node.read().unwrap().name.clone()).unwrap_or(format!("#{}", id)),
                None => "ground plane".to_string(),
            };

            let touching: Vec<String> = t.touching.iter().map(|target| name(target)).collect();
            ui.label(format!("touching: {}", if touching.is_empty() { "nothing".to_string() } else { touching.join(", ") }));

            if let Some((other, speed)) = &t.last_hit
            {
                ui.label(format!("last hit: {} at {:.1} m/s", name(other), speed));
            }

            if let Some(hitch) = &t.hitch
            {
                let other = scene.find_node_by_id(hitch.other).map(|node| node.read().unwrap().name.clone()).unwrap_or(format!("#{}", hitch.other));
                let role = if hitch.towing { "towing" } else { "towed by" };

                ui.label(format!("{} {}: {:?}, {:.1} kN, brake {:.2}", role, other, hitch.state, hitch.force, hitch.brake));
                ui.label(format!("trailer angle: yaw {:.0}°, pitch {:.0}°, roll {:.0}°", hitch.angles.x, hitch.angles.y, hitch.angles.z));
                ui.label(format!("springs carry {:.2}x the own weight", hitch.load)).on_hover_text("what rests on this vehicle through its couplings, or what its tow vehicle takes off it - the suspension is scaled by it");
            }
        });

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label(format!("Scene Colliders: {}", scene.physics.collider_amount()));
            ui.label("ℹ").on_hover_text("the ground and the solver settings live on the scene, see Physics Settings there");

            if ui.button("Rebuild").clicked()
            {
                scene.build_physics();
            }
        });

        // ********** apply **********
        if runtime_dirty && self.node.is_some() && self.frame.is_some()
        {
            // measured in the rest pose
            self.reset_visuals();
            self.remeasure_wheels(&picked_wheels);
            self.chassis_points.clear();
            self.chassis_parts.clear();
            self.setup_runtime(scene);
        }
        else if dirty
        {
            self.physics_dirty = true;
        }

        // outside play the controller does not update, so edits reach the debug view from here
        if self.physics_dirty && self.node.is_some() && self.frame.is_some()
        {
            self.build_physics(scene);
        }
    }

    // Center and radius of the wheels whose node was picked by hand.
    fn remeasure_wheels(&mut self, picked: &[usize])
    {
        let Some(node) = self.node.as_ref().cloned() else { return; };
        let Some(frame) = self.frame else { return; };

        let to_chassis = Self::chassis_inverse(&node.read().unwrap().get_full_transform());
        let children = Scene::list_all_child_nodes(&node.read().unwrap().nodes);

        for index in picked
        {
            let Some(wheel) = self.wheels.get_mut(*index) else { continue; };
            let Some(wheel_node) = Self::find_part(&children, &wheel.node_uuid, &wheel.node_name, None) else { continue; };

            if let Some((min, max)) = Self::subtree_bounds(&wheel_node, &to_chassis)
            {
                wheel.center = (min + max) * 0.5;
                wheel.radius = ((max - min).dot(&frame.up.abs()) * 0.5).max(0.01);
                wheel.width = (max - min).dot(&frame.right().abs());
            }
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::state::scene::node::Node;

    use super::*;

    const FRAME_DT: f32 = 1.0 / 60.0;

    // a car shaped like the off road test car: 4 wheels, 1.8 m track, 2.1 m wheelbase, box chassis
    fn test_car(vehicle_type: VehicleType) -> (Scene, VehicleController, u32)
    {
        let mut scene = Scene::new("vehicle test");

        let node = Node::new("car");
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("trans")))));

        let mut controller = VehicleController::default();
        controller.vehicle_type = vehicle_type;
        controller.node = OptionOrId::Some(node.clone());
        controller.frame = Some(VehicleFrame { forward: -Vector3::z(), up: Vector3::y() });

        controller.wheels = vec!
        [
            VehicleWheel::new("fl", Vector3::new(-0.9, 0.37, -1.05), 0.37),
            VehicleWheel::new("fr", Vector3::new( 0.9, 0.37, -1.05), 0.37),
            VehicleWheel::new("rl", Vector3::new(-0.9, 0.37,  1.05), 0.37),
            VehicleWheel::new("rr", Vector3::new( 0.9, 0.37,  1.05), 0.37),
        ];

        controller.chassis_bounds = Some((Vector3::new(-0.95, 0.5, -1.9), Vector3::new(0.95, 1.6, 1.9)));
        controller.chassis_points = vec![];
        for x in [-0.95, 0.95] { for y in [0.5, 1.6] { for z in [-1.9, 1.9] { controller.chassis_points.push(Vector3::new(x, y, z)); } } }

        controller.apply_preset();
        controller.engine_state.reset(&controller.engine);
        controller.build_physics(&mut scene);

        let id = node.read().unwrap().id;
        (scene, controller, id)
    }

    // runs the vehicle like the scene does: drive, step, write back - returns (speed m/s, distance m)
    fn run(scene: &mut Scene, controller: &mut VehicleController, id: u32, input: VehicleInput, seconds: f32) -> (f32, f32)
    {
        let frame = controller.frame.unwrap();
        let start = position(scene, id);
        let mut speed = 0.0;

        for _ in 0..(seconds / FRAME_DT) as usize
        {
            let state = controller.drive(&mut scene.physics, id, &frame, &input, FRAME_DT).unwrap();
            speed = state.speed;

            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);
        }

        (speed, (position(scene, id) - start).norm())
    }

    fn position(scene: &Scene, id: u32) -> Vector3<f32>
    {
        let (_, body) = scene.physics.vehicle(id).unwrap();
        let t = body.position().translation;
        Vector3::new(t.x, t.y, t.z)
    }

    fn set_speed(scene: &mut Scene, id: u32, forward: Vector3<f32>, speed: f32)
    {
        let (_, body) = scene.physics.vehicle_mut(id).unwrap();
        body.set_linvel(Vector::new(forward.x * speed, forward.y * speed, forward.z * speed), true);
    }

    fn settle(scene: &mut Scene, controller: &mut VehicleController, id: u32)
    {
        run(scene, controller, id, VehicleInput::default(), 2.0);
    }

    #[test]
    fn a_slide_stops_and_a_moderate_turn_follows_the_steering()
    {
        // sliding sideways at 3 m/s while rolling at 15 m/s, no input
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        {
            let (_, body) = scene.physics.vehicle_mut(id).unwrap();
            body.set_linvel(Vector::new(3.0, 0.0, -15.0), true);
        }
        run(&mut scene, &mut car, id, VehicleInput::default(), 0.5);
        let lateral = scene.physics.vehicle(id).unwrap().1.linvel().x;

        // 30 km/h with a quarter lock - about half a g
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        car.drift.counter_steer = 0.0;
        settle(&mut scene, &mut car, id);
        set_speed(&mut scene, id, -Vector3::z(), 8.3);
        run(&mut scene, &mut car, id, VehicleInput { throttle: 0.15, steer: 0.25, ..Default::default() }, 3.0);

        let (_, body) = scene.physics.vehicle(id).unwrap();
        let speed = body.linvel().length();
        let max_angle = car.steering.max_angle.to_radians() * (1.0 + (car.steering.high_speed_factor - 1.0) * (speed * 3.6 / car.steering.high_speed).clamp(0.0, 1.0));
        let follows = body.angvel().y / (speed * (0.25 * max_angle).tan() / 2.1);

        println!("slide 3 m/s -> {:.2} m/s after 0.5 s | 30 km/h turn: {:.0}% of the no slip yaw rate", lateral, follows * 100.0);

        assert!(lateral.abs() < 0.5, "still sliding at {} m/s", lateral);
        assert!(follows > 0.85, "the car pushes wide: {:.0}% of the steering", follows * 100.0);
    }

    // a static box the car has to get over, lying across the road at z = -6
    fn obstacle(scene: &mut Scene, height: f32, tilt_degrees: f32)
    {
        use rapier3d::prelude::ColliderBuilder;

        let rotation = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), tilt_degrees.to_radians());
        let collider = ColliderBuilder::cuboid(3.0, height * 0.5, 1.5)
            .translation(Vector::new(0.0, height * 0.5, -7.5))
            .rotation(Vector::new(rotation.scaled_axis().x, rotation.scaled_axis().y, rotation.scaled_axis().z))
            .build();

        scene.physics.colliders.insert(collider);
    }

    #[test]
    fn the_car_gets_over_steps_and_a_tilted_panel()
    {
        for (bumpers, clearance) in [(false, 0.0), (true, 0.8)]
        {
            for (height, tilt, name) in [(0.1, 0.0, "step 10 cm"), (0.2, 0.0, "step 20 cm"), (0.3, 0.0, "step 30 cm"), (0.05, 12.0, "tilted panel 12 deg")]
            {
                let (mut scene, mut car, id) = test_car(VehicleType::Car);
                car.chassis.wheel_bumpers = bumpers;
                car.chassis.min_clearance = clearance;
                car.build_physics(&mut scene);

                obstacle(&mut scene, height, tilt);
                settle(&mut scene, &mut car, id);

                // 0.36 and 0.4 can stall on the 30 cm step with bumpers, from 0.42 up all get over
                run(&mut scene, &mut car, id, VehicleInput { throttle: 0.5, ..Default::default() }, 8.0);
                let z = position(&scene, id).z;

                println!("bumpers {} clearance {:.1} | {}: z {:.1}", bumpers, clearance, name, z);
                assert!(z < -12.0, "stuck at the {} (bumpers {})", name, bumpers);
            }
        }
    }

    // the solar panel of the car test scene: 1.1 m wide under the left wheels, 30 degrees steep, 32 cm high - with the off road car body
    #[test]
    fn the_nose_does_not_catch_on_a_small_steep_ramp()
    {
        use rapier3d::prelude::ColliderBuilder;

        for sloped in [false, true]
        {
            let (mut scene, mut car, id) = test_car(VehicleType::Car);
            car.chassis_bounds = Some((Vector3::new(-1.0, 0.32, -1.9), Vector3::new(1.0, 1.86, 1.9)));
            car.chassis.shape = ChassisShape::Box;
            car.chassis.sloped_ends = sloped;
            car.chassis.min_clearance = 0.8;
            car.build_physics(&mut scene);

            let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -30f32.to_radians()).scaled_axis();
            let panel = ColliderBuilder::cuboid(0.56, 0.05, 0.35)
                .translation(Vector::new(-0.9, 0.14, -7.0))
                .rotation(Vector::new(tilt.x, tilt.y, tilt.z))
                .build();
            scene.physics.colliders.insert(panel);

            settle(&mut scene, &mut car, id);
            run(&mut scene, &mut car, id, VehicleInput { throttle: 0.3, ..Default::default() }, 8.0);

            let z = position(&scene, id).z;
            println!("clearance 0.8, sloped ends {}: z {:.1}", sloped, z);
        }

        // the default: the flat bottom at the axle height, only the wheel bumpers reach below it
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        car.chassis_bounds = Some((Vector3::new(-1.0, 0.32, -1.9), Vector3::new(1.0, 1.86, 1.9)));
        car.chassis.shape = ChassisShape::Box;
        car.build_physics(&mut scene);

        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -30f32.to_radians()).scaled_axis();
        scene.physics.colliders.insert(ColliderBuilder::cuboid(0.56, 0.05, 0.35).translation(Vector::new(-0.9, 0.14, -7.0)).rotation(Vector::new(tilt.x, tilt.y, tilt.z)).build());

        settle(&mut scene, &mut car, id);
        run(&mut scene, &mut car, id, VehicleInput { throttle: 0.3, ..Default::default() }, 8.0);

        let z = position(&scene, id).z;
        println!("default: z {:.1}", z);
        assert!(z < -12.0, "the car is stuck at the panel (z {})", z);
    }


    // ********** the real test course and the real off road car, from the local gltf files **********

    const TRACK_FILE: &str = "data/projects/vehicle_test/assets/vehicle_test_track.gltf";
    const OFFROAD_FILE: &str = "resourcesLocal/objects/TTDS_gltf/Car Stuff_Off Road Car.gltf";

    type GltfMesh = (String, Vec<Vector3<f32>>, Vec<[u32; 3]>);

    // world space triangles, with the node name
    fn gltf_meshes(path: &str) -> Option<Vec<GltfMesh>>
    {
        let (document, buffers, _) = gltf::import(path).ok()?;
        let mut result = vec![];

        fn walk(node: gltf::Node, parent: Matrix4<f32>, buffers: &[gltf::buffer::Data], result: &mut Vec<GltfMesh>)
        {
            let world = parent * Matrix4::from(node.transform().matrix());

            if let Some(mesh) = node.mesh()
            {
                for primitive in mesh.primitives()
                {
                    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                    let Some(positions) = reader.read_positions() else { continue; };

                    let vertices: Vec<Vector3<f32>> = positions.map(|p| (world * Vector3::new(p[0], p[1], p[2]).push(1.0)).xyz()).collect();
                    let indices: Vec<u32> = reader.read_indices().map(|i| i.into_u32().collect()).unwrap_or_else(|| (0..vertices.len() as u32).collect());

                    result.push((node.name().unwrap_or("").to_string(), vertices, indices.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()));
                }
            }

            for child in node.children()
            {
                walk(child, world, buffers, result);
            }
        }

        for scene in document.scenes()
        {
            for node in scene.nodes()
            {
                walk(node, Matrix4::identity(), &buffers, &mut result);
            }
        }

        Some(result)
    }

    fn add_track(scene: &mut Scene) -> bool
    {
        use rapier3d::prelude::ColliderBuilder;

        let Some(meshes) = gltf_meshes(TRACK_FILE) else { return false; };

        for (_, vertices, indices) in meshes
        {
            let vertices: Vec<Vector> = vertices.iter().map(|v| Vector::new(v.x, v.y, v.z)).collect();

            if let Ok(builder) = ColliderBuilder::trimesh(vertices, indices)
            {
                scene.physics.colliders.insert(builder.friction(0.7).build());
            }
        }

        true
    }

    // the off road car as the setup sees it: forward +z, wheels by name, the body as chassis points
    fn offroad_car(start: Vector3<f32>, shape: ChassisShape) -> Option<(Scene, VehicleController, u32)>
    {
        let meshes = gltf_meshes(OFFROAD_FILE)?;
        let mut scene = Scene::new("vehicle bench");

        // the wheels hang below the model origin - stand it on them
        let lowest = meshes.iter().filter(|(name, _, _)| name.to_lowercase().contains("wheel")).flat_map(|(_, vertices, _)| vertices.iter().map(|v| v.y)).fold(f32::MAX, f32::min);
        let start = start + Vector3::new(0.0, -lowest + 0.02, 0.0);

        let node = Node::new("off road car");
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", start, Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0))))));

        let mut controller = VehicleController::default();
        controller.node = OptionOrId::Some(node.clone());
        controller.frame = Some(VehicleFrame { forward: Vector3::z(), up: Vector3::y() });

        for (name, vertices, _) in &meshes
        {
            let (min, max) = VehicleController::bounds_of(vertices)?;

            if name.to_lowercase().contains("wheel")
            {
                controller.wheels.push(VehicleWheel::new(name, (min + max) * 0.5, (max.y - min.y) * 0.5));
            }
            else
            {
                controller.chassis_points.extend(vertices.iter().copied());
            }
        }

        controller.chassis_bounds = VehicleController::bounds_of(&controller.chassis_points);
        controller.apply_preset();
        controller.chassis.shape = shape;
        controller.engine_state.reset(&controller.engine);

        if !add_track(&mut scene)
        {
            return None;
        }

        controller.build_physics(&mut scene);

        let id = node.read().unwrap().id;
        Some((scene, controller, id))
    }

    #[derive(Default, Debug)]
    struct Ride
    {
        top_speed: f32,
        max_height: f32, // of the body above its start
        max_pitch_rate: f32,
        max_roll_rate: f32,
        flipped: bool,
        end: Vector3<f32>,
    }

    // holds a speed with the throttle and steers toward a lane - like a driver would
    fn ride(scene: &mut Scene, car: &mut VehicleController, id: u32, speed_kmh: f32, lane_x: f32, seconds: f32) -> Ride
    {
        ride_traced(scene, car, id, speed_kmh, &|_| lane_x, seconds, false)
    }

    // the loop of the test course is a helix - its lane moves sideways with the way round
    const LOOP: (f32, f32, f32, f32) = (25.0, 40.0, 7.0, 8.5); // lane x, center z, radius, shift

    fn loop_lane(at: Vector3<f32>, progress: &std::cell::Cell<f32>) -> f32
    {
        let (x, center_z, radius, shift) = LOOP;

        if at.z > center_z - radius - 1.0 && at.y > 1.5 || progress.get() > 0.0
        {
            let angle = (at.z - center_z).atan2(radius - at.y);
            let angle = if angle < 0.0 && progress.get() > 1.0 { angle + std::f32::consts::TAU } else { angle.max(0.0) };
            progress.set(progress.get().max(angle));
        }

        x + shift * (progress.get() / std::f32::consts::TAU).min(1.0)
    }

    fn ride_traced(scene: &mut Scene, car: &mut VehicleController, id: u32, speed_kmh: f32, lane: &dyn Fn(Vector3<f32>) -> f32, seconds: f32, trace: bool) -> Ride
    {
        let frame = car.frame.unwrap();
        let start_y = position(scene, id).y;
        let mut result = Ride::default();

        for step in 0..(seconds / FRAME_DT) as usize
        {
            let (rotation, at, current) =
            {
                let (_, body) = scene.physics.vehicle(id).unwrap();
                let r = body.position().rotation;
                let t = body.position().translation;
                (UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z)), Vector3::new(t.x, t.y, t.z), body.linvel().length() * 3.6)
            };

            // positive steer turns left, which is +x while driving along +z
            let forward = rotation * frame.forward;
            let wanted_x = ((lane(at) - at.x) * 0.2).clamp(-0.5, 0.5);
            let steer = ((wanted_x - forward.x) * 3.0).clamp(-1.0, 1.0);

            let input = VehicleInput { throttle: if current < speed_kmh { 1.0 } else { 0.0 }, steer, steer_analog: true, ..Default::default() };

            car.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);

            let (_, body) = scene.physics.vehicle(id).unwrap();
            let angvel = Vector3::new(body.angvel().x, body.angvel().y, body.angvel().z);
            let r = body.position().rotation;
            let rotation = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z));
            let t = body.position().translation;

            result.top_speed = result.top_speed.max(body.linvel().length() * 3.6);
            result.max_height = result.max_height.max(t.y - start_y);
            result.max_pitch_rate = result.max_pitch_rate.max(angvel.dot(&(rotation * frame.right())).abs());
            result.max_roll_rate = result.max_roll_rate.max(angvel.dot(&(rotation * frame.forward)).abs());
            result.flipped |= (rotation * frame.up).y < 0.0;
            result.end = Vector3::new(t.x, t.y, t.z);

            if trace && step % 6 == 0
            {
                let up = rotation * frame.up;
                let fwd = rotation * frame.forward;
                let wheels: String = scene.physics.vehicle(id).unwrap().0.controller.wheels().iter().map(|w| format!("{}{:.2} ", if w.raycast_info().is_in_contact { "c" } else { "-" }, w.raycast_info().suspension_length)).collect();
                println!("t {:.1} x {:5.2} y {:5.2} z {:6.2} v {:5.1} | pitch {:6.1} roll {:6.1} | steer {:5.2} | wheels {}", step as f32 * FRAME_DT, t.x, t.y, t.z, body.linvel().length() * 3.6, fwd.y.asin().to_degrees(), (rotation * frame.right()).y.asin().to_degrees(), car.steer, wheels);
                let _ = up;
            }
        }

        result
    }

    // the local test course with the off road car - skipped when the gltf files are not there
    #[test]
    fn the_off_road_car_on_the_test_course()
    {
        if gltf_meshes(TRACK_FILE).is_none() || gltf_meshes(OFFROAD_FILE).is_none()
        {
            println!("test course files missing - skipped");
            return;
        }

        // main lane: bumps, table top, curbs, jump - it has to arrive upright behind the landing ramp
        let (mut scene, mut car, id) = offroad_car(Vector3::zeros(), ChassisShape::ConvexHull).unwrap();
        settle(&mut scene, &mut car, id);
        let lane = ride(&mut scene, &mut car, id, 70.0, 0.0, 12.0);
        println!("main lane at 70 km/h: {:?}", lane);

        assert!(!lane.flipped, "rolled over on the main lane");
        assert!(lane.end.z > 100.0, "did not make it along the main lane (z {})", lane.end.z);

        // the loop, following its helix
        let (mut scene, mut car, id) = offroad_car(Vector3::new(LOOP.0, 0.1, -75.0), ChassisShape::ConvexHull).unwrap();
        settle(&mut scene, &mut car, id);
        let progress = std::cell::Cell::new(0.0);
        let looped = ride_traced(&mut scene, &mut car, id, 90.0, &|at| loop_lane(at, &progress), 13.0, false);
        let upright = (car_rotation(&scene, id) * Vector3::y()).y;
        println!("loop at 90 km/h: {:?}, upright {:.2}", looped, upright);

        assert!(looped.max_height > 10.0, "did not get over the top of the loop");
        assert!(looped.end.x > LOOP.0 + LOOP.3 * 0.7 && looped.end.z > LOOP.1 + 10.0 && upright > 0.8, "did not come out of the loop on its wheels (end {:?})", looped.end);
    }

    // into the loop of the test course, centered and off to the side: the hardest jolt per frame, over the top or not - with and without rounded body edges
    #[test]
    #[ignore]
    fn bench_loop_entry()
    {
        for rounding in [0.0, 0.2]
        {
            let (mut through, mut runs, mut jolts) = (0, 0, 0.0);

            for offset in [0.0, 0.75, 1.5]
            {
                for speed in [50.0, 60.0, 70.0, 80.0]
                {
                    let Some((mut scene, mut car, id)) = offroad_car(Vector3::new(LOOP.0 + offset, 0.1, -75.0), ChassisShape::ConvexHull) else { println!("course files missing"); return; };
                    car.chassis.rounding = rounding;
                    car.build_physics(&mut scene);
                    settle(&mut scene, &mut car, id);

                    let progress = std::cell::Cell::new(0.0);
                    let start_y = position(&scene, id).y;
                    let (mut jolt, mut height, mut flipped): (f32, f32, bool) = (0.0, 0.0, false);
                    let mut previous = Vector3::zeros();
                    let (mut touching_frames, mut jolt_touch): (usize, Option<Vector3<f32>>) = (0, None);

                    for _ in 0..(13.0 / FRAME_DT) as usize
                    {
                        ride_traced(&mut scene, &mut car, id, speed, &|at| loop_lane(at, &progress), FRAME_DT, false);
                        let (_, body) = scene.physics.vehicle(id).unwrap();
                        let angvel = Vector3::new(body.angvel().x, body.angvel().y, body.angvel().z);
                        // where the body touches the course, in body space: x right, y up, z forward
                        let touch = scene.physics.contacts_of(id).filter(|contact| !contact.stopped()).map(|contact| contact.point).next();
                        let touch_local = touch.map(|point| { let p = body.position().inverse_transform_point(Vector::new(point.x, point.y, point.z)); Vector3::new(p.x, p.y, p.z) });
                        touching_frames += touch.is_some() as usize;
                        if (angvel - previous).norm() > jolt
                        {
                            jolt = (angvel - previous).norm();
                            jolt_touch = touch_local;
                        }
                        previous = angvel;
                        height = height.max(body.translation().y - start_y);
                        flipped |= (car_rotation(&scene, id) * Vector3::y()).y < -0.5 && body.translation().y - start_y < 1.0;
                    }

                    let end = position(&scene, id);
                    runs += 1;
                    jolts += jolt;
                    through += (!flipped && height > 10.0 && end.z > LOOP.1 + 10.0) as usize;
                    println!("rounding {:.1} offset {:.1} {:>3} km/h: hardest jolt {:5.2} rad/s in a frame (body touching there: {}), top {:5.1} m, {} (end z {:.0}) | body touched the course in {} frames", rounding, offset, speed, jolt, jolt_touch.map_or("no".to_string(), |p| format!("at {:.2?}", p)), height, if flipped { "ROLLED OVER" } else if height > 10.0 && end.z > LOOP.1 + 10.0 { "through" } else { "not through" }, end.z, touching_frames);
                }
            }

            println!("rounding {:.1}: through {}/{}, average hardest jolt {:.2}", rounding, through, runs, jolts / runs as f32);
        }
    }

    fn car_rotation(scene: &Scene, id: u32) -> UnitQuaternion<f32>
    {
        let r = scene.physics.vehicle(id).unwrap().1.position().rotation;
        UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z))
    }

    #[test]
    #[ignore = "overview for tuning, prints only - cargo test bench_terrain -- --ignored --nocapture"]
    fn bench_terrain()
    {
        if gltf_meshes(TRACK_FILE).is_none() { return; }

        // name, start, lane x, speed, seconds
        for (name, start, lane, speed, seconds) in [("hills", Vector3::new(65.0, 0.1, -70.0), 65.0, 40.0, 14.0), ("waves", Vector3::new(-47.0, 0.1, -90.0), -47.0, 40.0, 8.0), ("washboard", Vector3::new(-29.0, 0.1, -28.0), -29.0, 50.0, 6.0), ("moguls", Vector3::new(-77.0, 0.1, -8.0), -77.0, 20.0, 14.0), ("rock garden", Vector3::new(-94.0, 0.1, -6.0), -94.0, 15.0, 14.0), ("stairs", Vector3::new(-37.0, 0.1, 112.0), -37.0, 20.0, 8.0), ("side ramp", Vector3::new(-30.6, 0.1, 112.0), -30.6, 30.0, 6.0)]
        {
            let (mut scene, mut car, id) = offroad_car(start, ChassisShape::ConvexHull).unwrap();
            settle(&mut scene, &mut car, id);

            let frame = car.frame.unwrap();
            let mut length: (f32, f32) = (f32::MAX, f32::MIN);
            let mut airborne = 0;
            let mut max_roll: f32 = 0.0;
            let mut flipped = false;

            for _ in 0..(seconds / FRAME_DT) as usize
            {
                let ride = ride(&mut scene, &mut car, id, speed, lane, FRAME_DT);
                flipped |= ride.flipped;

                let (vehicle, body) = scene.physics.vehicle(id).unwrap();
                let wheels = vehicle.controller.wheels();
                for wheel in wheels.iter().filter(|wheel| wheel.raycast_info().is_in_contact)
                {
                    length.0 = length.0.min(wheel.raycast_info().suspension_length);
                    length.1 = length.1.max(wheel.raycast_info().suspension_length);
                }
                if wheels.iter().all(|wheel| !wheel.raycast_info().is_in_contact) { airborne += 1; }

                let r = body.position().rotation;
                let rotation = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z));
                max_roll = max_roll.max((rotation * frame.right()).y.asin().to_degrees().abs());
            }

            let end = position(&scene, id);
            println!("{:12} {:3} km/h: end z {:6.1} flipped {} | spring length {:.2}..{:.2} (rest {:.2} +- {:.2}) | airborne {:.1} s | max roll {:.0} deg", name, speed, end.z, flipped, length.0, length.1, car.suspension.rest_length, car.suspension.travel, airborne as f32 * FRAME_DT, max_roll);
        }
    }

    // circles in the wall of death: speed up on the floor, move out onto the wall, then back down and out through the gap
    fn wall_of_death_ride(speed_kmh: f32, wall_height: f32, trace: bool) -> Option<(f32, bool, Vector3<f32>)>
    {
        const CENTER: (f32, f32) = (75.0, 110.0);

        // on the floor circle, heading +z is the counter clockwise tangent there
        let (mut scene, mut car, id) = offroad_car(Vector3::new(CENTER.0 + 10.0, 0.1, CENTER.1), ChassisShape::ConvexHull)?;
        settle(&mut scene, &mut car, id);
        let frame = car.frame.unwrap();

        let mut max_height: f32 = 0.0;
        let mut flipped = false;
        let steps = (22.0 / FRAME_DT) as usize;

        for step in 0..steps
        {
            let time = step as f32 * FRAME_DT;
            let (rotation, at, current) =
            {
                let (_, body) = scene.physics.vehicle(id).unwrap();
                let r = body.position().rotation;
                let t = body.position().translation;
                (UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z)), Vector3::new(t.x, t.y, t.z), body.linvel().length() * 3.6)
            };

            let (dx, dz) = (at.x - CENTER.0, at.z - CENTER.1);
            let radius = (dx * dx + dz * dz).sqrt().max(0.1);
            let outward = Vector3::new(dx / radius, 0.0, dz / radius);
            let tangent = Vector3::new(-outward.z, 0.0, outward.x);

            // 0-7 s speed up on the floor, 7-16 s up on the wall, then down and out towards -x
            let (target_radius, target_height, wanted_speed, leaving) = if time < 7.0 { (10.0, 0.9, speed_kmh, false) } else if time < 16.0 { (16.0, wall_height, speed_kmh, false) } else { (10.0, 0.9, 35.0, true) };

            // gently - straight outwards would be straight up the wall
            let out = ((target_radius - radius) * 0.3).clamp(-0.4, 0.4);
            let rise = ((target_height - at.y) * 0.3).clamp(-0.4, 0.4);
            let mut wanted = (tangent + outward * out + Vector3::y() * rise).normalize();

            // out through the door, straight towards -x
            if leaving && dx < -6.0 && at.y < 1.5
            {
                let door = Vector3::new(CENTER.0 - 20.0, 0.0, CENTER.1);
                let to_door = Vector3::new(door.x - at.x, 0.0, door.z - at.z);
                wanted = to_door.normalize();
            }

            // steering turns around the car's own up axis - on the wall its left is up or down
            let left = rotation * frame.up.cross(&frame.forward);
            let steer = (wanted.dot(&left) * 2.5).clamp(-1.0, 1.0);

            let input = VehicleInput { throttle: if current < wanted_speed { 1.0 } else { 0.0 }, steer, steer_analog: true, ..Default::default() };

            car.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);

            let up = car_rotation(&scene, id) * frame.up;
            flipped |= up.y < -0.2 && at.y < 1.5;
            max_height = max_height.max(at.y);

            if trace && step % 30 == 0
            {
                println!("t {:4.1} r {:5.2} y {:5.2} v {:5.1} tilt {:5.1} deg", time, radius, at.y, current, up.y.clamp(-1.0, 1.0).acos().to_degrees());
            }
        }

        Some((max_height, flipped, position(&scene, id)))
    }

    #[test]
    #[ignore = "overview for tuning, prints only - cargo test bench_wall_of_death -- --ignored --nocapture"]
    fn bench_wall_of_death()
    {
        if gltf_meshes(TRACK_FILE).is_none() { return; }

        for speed in [40.0, 55.0, 70.0]
        {
            let (height, flipped, end) = wall_of_death_ride(speed, 5.5, speed == 55.0).unwrap();
            println!("wall of death at {} km/h: highest {:.1} m, flipped {}, end {:?}, left through the door {}", speed, height, flipped, end, end.x < 59.0);
        }
    }

    // left wheels up the two wheel ramp, then on - returns (seconds on two wheels, the most tilt, how it ended)
    fn two_wheel_ride(speed_kmh: f32, balance: bool) -> Option<(f32, f32, &'static str)>
    {
        const LANE: f32 = 7.8;
        const RAMP_END: f32 = 134.0;

        let (mut scene, mut car, id) = offroad_car(Vector3::new(LANE, 0.1, 90.0), ChassisShape::ConvexHull)?;
        settle(&mut scene, &mut car, id);
        let frame = car.frame.unwrap();

        let mut two_wheels = 0;
        let mut max_tilt: f32 = 0.0;
        let mut last_tilt = 0.0;
        let mut ending = "back on four wheels";

        for _ in 0..(12.0 / FRAME_DT) as usize
        {
            let (rotation, at, current) =
            {
                let (_, body) = scene.physics.vehicle(id).unwrap();
                let r = body.position().rotation;
                let t = body.position().translation;
                (UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z)), Vector3::new(t.x, t.y, t.z), body.linvel().length() * 3.6)
            };

            // positive while the left side is up
            let left = rotation * frame.up.cross(&frame.forward);
            let tilt = left.y.clamp(-1.0, 1.0).asin();
            let tilt_rate = (tilt - last_tilt) / FRAME_DT;
            last_tilt = tilt;

            let forward = rotation * frame.forward;
            let lane_steer = (((LANE - at.x) * 0.2).clamp(-0.5, 0.5) - forward.x) * 3.0;

            // on two wheels: steering towards the wheels on the ground lifts the car further, away from them lowers it
            let steer = if balance && at.z > RAMP_END && tilt > 0.3
            {
                -(4.0 * (40f32.to_radians() - tilt) - 1.5 * tilt_rate)
            }
            else if at.z > RAMP_END { 0.0 } else { lane_steer };

            let input = VehicleInput { throttle: if current < speed_kmh { 1.0 } else { 0.0 }, steer: steer.clamp(-1.0, 1.0), steer_analog: true, ..Default::default() };

            car.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);

            if std::env::var("TRACE_TWO_WHEEL").is_ok() && (at.z > 118.0 && at.z < 142.0)
            {
                let contacts: String = scene.physics.vehicle(id).unwrap().0.controller.wheels().iter().map(|w| if w.raycast_info().is_in_contact { 'c' } else { '-' }).collect();
                println!("z {:6.2} x {:5.2} y {:5.2} tilt {:5.1} rate {:6.1} steer {:5.2} wheels {}", at.z, at.x, at.y, tilt.to_degrees(), tilt_rate.to_degrees(), steer, contacts);
            }

            if at.z > RAMP_END
            {
                max_tilt = max_tilt.max(tilt);

                if tilt > 25f32.to_radians() && tilt < 85f32.to_radians()
                {
                    two_wheels += 1;
                }

                if tilt > 85f32.to_radians()
                {
                    ending = "rolled over";
                }
            }
        }

        if last_tilt > 25f32.to_radians() && last_tilt < 85f32.to_radians()
        {
            ending = "still on two wheels";
        }

        Some((two_wheels as f32 * FRAME_DT, max_tilt.to_degrees(), ending))
    }

    #[test]
    #[ignore = "overview for tuning, prints only - cargo test bench_two_wheel_ramp -- --ignored --nocapture"]
    fn bench_two_wheel_ramp()
    {
        if gltf_meshes(TRACK_FILE).is_none() { return; }

        for balance in [false, true]
        {
            for speed in [25.0, 35.0, 45.0]
            {
                let (seconds, tilt, ending) = two_wheel_ride(speed, balance).unwrap();
                println!("balance {} at {} km/h: {:.1} s on two wheels, up to {:.0} deg, {}", balance, speed, seconds, tilt, ending);
            }
        }
    }

    const OFFROAD_COURSE_FILE: &str = "data/projects/offroad_test/assets/offroad_course.gltf";
    const OFFROAD_START_FILE: &str = "data/projects/offroad_test/assets/offroad_start.json";

    // a lap of the off road course, following its track - returns (m driven along the track, rolled over, stuck)
    fn offroad_lap(speed_kmh: f32, seconds: f32) -> Option<(f32, bool, bool)>
    {
        use rapier3d::prelude::ColliderBuilder;

        let start: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(OFFROAD_START_FILE).ok()?).ok()?;
        let path: Vec<Vector3<f32>> = start["path"].as_array()?.iter().map(|p| Vector3::new(p[0].as_f64().unwrap() as f32, 0.0, p[1].as_f64().unwrap() as f32)).collect();
        let yaw = start["yaw"].as_f64()? as f32;
        let position = Vector3::new(start["x"].as_f64()? as f32, start["y"].as_f64()? as f32, start["z"].as_f64()? as f32);

        let (mut scene, mut car, id) = offroad_car(position, ChassisShape::ConvexHull)?;

        // the course instead of the test track
        scene.physics.clear();
        for (_, vertices, indices) in gltf_meshes(OFFROAD_COURSE_FILE)?
        {
            let vertices: Vec<Vector> = vertices.iter().map(|v| Vector::new(v.x, v.y, v.z)).collect();
            if let Ok(builder) = ColliderBuilder::trimesh(vertices, indices)
            {
                scene.physics.colliders.insert(builder.friction(0.7).build());
            }
        }

        // turned like the scene object will be - the same euler y rotation
        {
            let node = car.node.as_ref().unwrap().clone();
            let transformation = node.read().unwrap().find_component::<Transformation>().unwrap();
            component_downcast_mut!(transformation, Transformation);
            let mut rotation = transformation.get_data().rotation;
            rotation.y = yaw;
            transformation.set_rotation(rotation);
        }
        car.build_physics(&mut scene);
        settle(&mut scene, &mut car, id);

        let frame = car.frame.unwrap();
        let heading = car_rotation(&scene, id) * frame.forward;
        let along = (path[1] - path[0]).normalize();
        println!("start heading {:.2} {:.2}, track {:.2} {:.2}", heading.x, heading.z, along.x, along.z);

        let mut progress = 0usize;
        let mut flipped = false;
        let mut stuck_time = 0.0;

        for _ in 0..(seconds / FRAME_DT) as usize
        {
            let (rotation, at, current) =
            {
                let (_, body) = scene.physics.vehicle(id).unwrap();
                let r = body.position().rotation;
                let t = body.position().translation;
                (UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r.w, r.x, r.y, r.z)), Vector3::new(t.x, 0.0, t.z), body.linvel().length() * 3.6)
            };

            // the nearest track point a bit ahead of the last one, then aim 8 m further on
            for k in progress..(progress + 20).min(path.len() + progress)
            {
                if (path[k % path.len()] - at).norm() < (path[progress % path.len()] - at).norm()
                {
                    progress = k;
                }
            }
            let target = path[(progress + 3) % path.len()];

            let left = rotation * frame.up.cross(&frame.forward);
            let wanted = (target - at).normalize();
            let steer = (wanted.dot(&left) * 2.0).clamp(-1.0, 1.0);

            let input = VehicleInput { throttle: if current < speed_kmh { 1.0 } else { 0.0 }, steer, steer_analog: true, ..Default::default() };
            car.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);

            flipped |= (car_rotation(&scene, id) * frame.up).y < 0.0;
            stuck_time = if current < 3.0 { stuck_time + FRAME_DT } else { 0.0 };

            if flipped || stuck_time > 4.0
            {
                break;
            }
        }

        Some((progress as f32 * 3.0, flipped, stuck_time > 4.0))
    }

    #[test]
    #[ignore = "drives the off road course, prints only - cargo test bench_offroad_course -- --ignored --nocapture"]
    fn bench_offroad_course()
    {
        for speed in [25.0, 40.0]
        {
            match offroad_lap(speed, 120.0)
            {
                Some((meters, flipped, stuck)) => println!("off road at {} km/h: {:.0} m along the track, rolled over {}, stuck {}", speed, meters, flipped, stuck),
                None => println!("off road course files missing - skipped"),
            }
        }
    }

    #[test]
    fn braking_handbrake_and_a_hard_turn()
    {
        // ********** acceleration **********
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        let rest_height = position(&scene, id).y;

        let (speed, _) = run(&mut scene, &mut car, id, VehicleInput { throttle: 1.0, ..Default::default() }, 5.0);
        println!("rest height {:.3} - 0..5 s full throttle: {:.1} km/h, gear {}", rest_height, speed * 3.6, car.engine_state.gear);

        // ********** braking from 72 km/h **********
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        set_speed(&mut scene, id, -Vector3::z(), 20.0);
        let (speed, distance) = run(&mut scene, &mut car, id, VehicleInput { brake: 1.0, ..Default::default() }, 4.0);
        println!("brake 72 km/h: {:.1} m, left {:.2} m/s (1 g would be 20.4 m)", distance, speed);

        // ********** handbrake from 72 km/h **********
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        set_speed(&mut scene, id, -Vector3::z(), 20.0);
        let (speed, distance) = run(&mut scene, &mut car, id, VehicleInput { handbrake: true, ..Default::default() }, 6.0);
        println!("handbrake 72 km/h: {:.1} m in 6 s, left {:.2} m/s", distance, speed);
        assert!(speed.abs() < 0.5, "the handbrake does not stop the car: {} m/s left", speed);

        // ********** coasting from 72 km/h **********
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        set_speed(&mut scene, id, -Vector3::z(), 20.0);
        let (speed, _) = run(&mut scene, &mut car, id, VehicleInput::default(), 5.0);
        println!("coast 5 s from 72 km/h: {:.1} km/h", speed * 3.6);

        // ********** steady turn at 40 km/h **********
        let (mut scene, mut car, id) = test_car(VehicleType::Car);
        settle(&mut scene, &mut car, id);
        set_speed(&mut scene, id, -Vector3::z(), 11.0);
        run(&mut scene, &mut car, id, VehicleInput { throttle: 0.3, steer: 0.5, ..Default::default() }, 3.0);

        let (_, body) = scene.physics.vehicle(id).unwrap();
        let rotation = body.position().rotation;
        let rotation = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z));
        let linvel = Vector3::new(body.linvel().x, body.linvel().y, body.linvel().z);
        let yaw_rate = body.angvel().y;

        let forward = rotation * -Vector3::z();
        let speed = linvel.dot(&forward);
        let lateral = linvel.dot(&(rotation * Vector3::x()));

        let max_angle = car.steering.max_angle.to_radians() * (1.0 + (car.steering.high_speed_factor - 1.0) * (speed * 3.6 / car.steering.high_speed).clamp(0.0, 1.0));
        let expected = speed * (0.5 * max_angle).tan() / 2.1;

        assert!(yaw_rate / expected > 0.6, "the car pushes wide in a hard turn");
        println!("turn at {:.1} km/h: yaw rate {:.3} rad/s (no slip would be {:.3}), side slip {:.2} m/s, slip angle {:.1} deg", speed * 3.6, yaw_rate, expected, lateral, lateral.atan2(speed).to_degrees());
    }

    // ********** kart on the city speedway **********

    const KART_FILE: &str = "resourcesLocal/objects/temp/Mario Kart Yoshi Green baked.glb";
    const SPEEDWAY_FILE: &str = "resourcesLocal/objects/scenes/city_speedway_v2.glb";

    const SPEEDWAY_PATH_FILE: &str = "resourcesLocal/objects/scenes/city_speedway_path.json";

    // the speedway is modelled about 40 times too small (the start gate) - scaled and moved so the start line sits at
    // x 0 z 0, and nothing reaches below the physics ground plane at y 0
    const SPEEDWAY_SCALE: f32 = 40.0;
    const SPEEDWAY_OFFSET: (f32, f32, f32) = (-397.6, 5.19, -354.2);
    const SPEEDWAY_ROAD_Y: f32 = 17.09;

    fn build_kart(start: Vector3<f32>, yaw: f32) -> Option<(Scene, VehicleController, u32)>
    {
        let meshes = gltf_meshes(KART_FILE)?;
        let mut scene = Scene::new("kart bench");

        let node = Node::new("kart");
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", start + Vector3::new(0.0, 0.02, 0.0), Vector3::new(0.0, yaw, 0.0), Vector3::new(1.0, 1.0, 1.0))))));

        let mut controller = VehicleController::default();
        controller.vehicle_type = VehicleType::Kart;
        controller.node = OptionOrId::Some(node.clone());
        controller.frame = Some(VehicleFrame { forward: Vector3::z(), up: Vector3::y() });

        for (name, vertices, _) in &meshes
        {
            let (min, max) = VehicleController::bounds_of(vertices)?;

            if ["back left", "back right", "front left", "front right"].contains(&name.as_str())
            {
                controller.wheels.push(VehicleWheel::new(name, (min + max) * 0.5, (max.y - min.y) * 0.5));
            }
            else
            {
                controller.chassis_points.extend(vertices.iter().copied());
            }
        }

        controller.chassis_bounds = VehicleController::bounds_of(&controller.chassis_points);
        controller.apply_preset();
        controller.engine_state.reset(&controller.engine);

        Some((scene, controller, node.read().unwrap().id)).map(|(mut scene, mut controller, id)|
        {
            controller.build_physics(&mut scene);
            (scene, controller, id)
        })
    }

    fn add_speedway(scene: &mut Scene) -> bool
    {
        use rapier3d::prelude::ColliderBuilder;

        let Some(meshes) = gltf_meshes(SPEEDWAY_FILE) else { return false; };
        let offset = Vector3::new(SPEEDWAY_OFFSET.0, SPEEDWAY_OFFSET.1, SPEEDWAY_OFFSET.2);

        for (_, vertices, indices) in meshes
        {
            let vertices: Vec<Vector> = vertices.iter().map(|v| { let v = offset + v * SPEEDWAY_SCALE; Vector::new(v.x, v.y, v.z) }).collect();

            if let Ok(builder) = ColliderBuilder::trimesh(vertices, indices)
            {
                scene.physics.colliders.insert(builder.friction(0.7).build());
            }
        }

        true
    }

    #[test]
    #[ignore = "kart on flat ground and on the speedway, prints only - cargo test bench_kart -- --ignored --nocapture"]
    fn bench_kart()
    {
        let Some((mut scene, mut kart, id)) = build_kart(Vector3::zeros(), 0.0) else { println!("kart file missing"); return; };
        let frame = kart.frame.unwrap();
        println!("wheels: {:?}", kart.wheels.iter().map(|w| (w.node_name.clone(), w.center, w.radius, w.steer, w.driven)).collect::<Vec<_>>());

        if let Ok(path) = std::env::var("KART_JSON")
        {
            std::fs::write(path, serde_json::to_string_pretty(&kart).unwrap()).unwrap();
        }
        settle(&mut scene, &mut kart, id);
        println!("settled at y {:.3}", position(&scene, id).y);

        // full throttle
        let mut to_50 = None;
        let mut speed = 0.0;
        for step in 0..(12.0 / FRAME_DT) as usize
        {
            let (s, _) = run(&mut scene, &mut kart, id, VehicleInput { throttle: 1.0, ..Default::default() }, FRAME_DT);
            speed = s;
            if to_50.is_none() && speed * 3.6 >= 50.0 { to_50 = Some(step as f32 * FRAME_DT); }
        }
        println!("full throttle: 0-50 km/h in {:?} s, after 12 s {:.1} km/h, gear {}, rpm {:.0}", to_50, speed * 3.6, kart.engine_state.gear, kart.engine_state.rpm);

        // braking
        let start = position(&scene, id);
        let mut time = 0.0;
        while run(&mut scene, &mut kart, id, VehicleInput { brake: 1.0, ..Default::default() }, FRAME_DT).0 > 0.5 && time < 10.0 { time += FRAME_DT; }
        println!("braking from {:.0} km/h: {:.1} m in {:.1} s", speed * 3.6, (position(&scene, id) - start).norm(), time);

        // full lock at 40 km/h
        for (steer, label) in [(1.0, "full lock"), (0.4, "40% lock")]
        {
            let Some((mut scene, mut kart, id)) = kart_at_speed(40.0) else { return; };
            let mut flipped = false;
            let mut min_up: f32 = 1.0;
            for _ in 0..(3.0 / FRAME_DT) as usize
            {
                run(&mut scene, &mut kart, id, VehicleInput { throttle: 0.3, steer, ..Default::default() }, FRAME_DT);
                let (_, body) = scene.physics.vehicle(id).unwrap();
                let up = body.position().rotation * Vector::Y;
                min_up = min_up.min(up.y);
                flipped |= up.y < 0.3;
            }
            let (_, body) = scene.physics.vehicle(id).unwrap();
            let v = body.linvel();
            println!("{} at 40 km/h: speed after 3 s {:.1} km/h, yaw rate {:.2} rad/s, min up {:.2}, flipped {}", label, v.length() * 3.6, body.angvel().y, min_up, flipped);
        }

        let _ = frame;

        // ********** on the speedway: from the grid over the start line **********
        let grid = Vector3::new(-2.0, SPEEDWAY_ROAD_Y + 0.5, -6.0);
        let Some((mut scene, mut kart, id)) = build_kart(grid, 0.0) else { return; };
        if !add_speedway(&mut scene) { println!("speedway file missing"); return; }
        kart.build_physics(&mut scene);
        settle(&mut scene, &mut kart, id);
        println!("speedway grid: settled at {:?}", position(&scene, id));
        for second in 0..8
        {
            let (speed, _) = run(&mut scene, &mut kart, id, VehicleInput { throttle: 0.5, ..Default::default() }, 1.0);
            let (_, body) = scene.physics.vehicle(id).unwrap();
            println!("  t {} s: {:?} {:.1} km/h, up {:.2}", second + 1, position(&scene, id), speed * 3.6, (body.position().rotation * Vector::Y).y);
        }
    }

    #[derive(Default, Debug)]
    struct Lap
    {
        time: Option<f32>,
        progress: f32, // share of the lap
        top_speed: f32,
        wall_hits: usize,
        off_road: usize, // samples beside the road
        min_up: f32,
        max_roll_rate: f32,
        max_pitch_rate: f32,
        max_slip: f32, // deg
    }

    // follows the centre line like a driver: aims at a point ahead, holds a speed that drops for the corners ahead
    fn speedway_lap(scene: &mut Scene, kart: &mut VehicleController, id: u32, path: &[Vector3<f32>], top_kmh: f32, corner_g: f32, seconds: f32) -> Lap
    {
        let frame = kart.frame.unwrap();
        let n = path.len();
        let mut lap = Lap { min_up: 1.0, ..Default::default() };
        let mut index = 0usize;
        let mut passed = 0usize;
        let mut last_speed = 0.0;
        let mut time = 0.0;

        while time < seconds
        {
            let (_, body) = scene.physics.vehicle(id).unwrap();
            let rotation = body.position().rotation;
            let t = body.position().translation;
            let position = Vector3::new(t.x, 0.0, t.z);
            let forward = rotation * Vector::new(frame.forward.x, frame.forward.y, frame.forward.z);
            let up = rotation * Vector::Y;
            let local_angvel = rotation.inverse() * body.angvel();
            let v = body.linvel();
            let speed = v.dot(forward);
            let side = (v - forward * speed).length();

            // nearest point, searched a little ahead only
            let mut best = (f32::MAX, index);
            for k in 0..20 { let i = (index + k) % n; let d = (path[i] - position).norm(); if d < best.0 { best = (d, i); } }
            if best.1 < index && index > n - 20 { passed += 1; }
            index = best.1;
            if best.0 > 9.0 { lap.off_road += 1; }

            // aim ahead, slower for the bend coming up
            let ahead = (4.0 + speed.abs() * 0.35) as usize / 2 + 1;
            let target = path[(index + ahead) % n];
            let to = target - position;
            let wanted = to.x.atan2(to.z);
            let heading = forward.x.atan2(forward.z);
            let steer = (shortest_angle_dist(heading, wanted) * 2.5).clamp(-1.0, 1.0);

            let bend = |from: usize, to: usize| { let a = path[(index + from) % n] - path[(index + from + 2) % n]; let b = path[(index + to) % n] - path[(index + to + 2) % n]; a.x.atan2(a.z) - b.x.atan2(b.z) };
            let curvature = (1..8).map(|k| shortest_angle_dist(0.0, bend(k * 2, k * 2 + 2)).abs() / 4.0).fold(0.0, f32::max);
            let corner_speed = if curvature > 1e-4 { (corner_g * EARTH_GRAVITY / curvature).sqrt() } else { f32::MAX };
            let wanted_speed = (top_kmh / 3.6).min(corner_speed);

            let input = VehicleInput { throttle: if speed < wanted_speed { 1.0 } else { 0.0 }, brake: if speed > wanted_speed + 3.0 { 1.0 } else { 0.0 }, steer, steer_analog: true, ..Default::default() };
            kart.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);
            time += FRAME_DT;

            if last_speed - speed > 12.0 / 3.6 * FRAME_DT * 60.0 * 0.25 && input.brake == 0.0
            {
                lap.wall_hits += 1;
                if std::env::var("TRACE_KART").is_ok() { println!("  hit at {:?} t {:.1} speed {:.1} -> {:.1} km/h, height {:.2}", position, time, last_speed * 3.6, speed * 3.6, t.y); }
            }
            if std::env::var("TRACE_KART").is_ok() && local_angvel.x.abs() > 2.0 { println!("  pitch {:.1} rad/s at {:?} t {:.1} speed {:.0} km/h height {:.2}", local_angvel.x, position, time, speed * 3.6, t.y); }
            last_speed = speed;
            lap.top_speed = lap.top_speed.max(speed * 3.6);
            lap.min_up = lap.min_up.min(up.y);
            lap.max_roll_rate = lap.max_roll_rate.max(local_angvel.z.abs());
            lap.max_pitch_rate = lap.max_pitch_rate.max(local_angvel.x.abs());
            if speed > 5.0 { lap.max_slip = lap.max_slip.max(side.atan2(speed).to_degrees()); }

            lap.progress = passed as f32 + index as f32 / n as f32;
            if passed >= 1 && lap.time.is_none() { lap.time = Some(time); }
            if lap.min_up < 0.3 { break; }
        }

        lap
    }

    fn speedway_path() -> Option<Vec<Vector3<f32>>>
    {
        let text = std::fs::read_to_string(SPEEDWAY_PATH_FILE).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        Some(json["path"].as_array()?.iter().map(|p| Vector3::new(p[0].as_f64().unwrap() as f32, 0.0, p[1].as_f64().unwrap() as f32)).collect())
    }

    #[test]
    #[ignore = "kart laps on the speedway, prints only - cargo test bench_kart_laps -- --ignored --nocapture"]
    fn bench_kart_laps()
    {
        let Some(path) = speedway_path() else { println!("path file missing"); return; };

        for (top, corner_g) in [(60.0, 0.8), (85.0, 1.0), (85.0, 1.3)]
        {
            let Some((mut scene, mut kart, id)) = build_kart(Vector3::new(0.0, SPEEDWAY_ROAD_Y + 0.3, -8.0), 0.0) else { return; };
            if !add_speedway(&mut scene) { println!("speedway file missing"); return; }
            kart.build_physics(&mut scene);
            settle(&mut scene, &mut kart, id);

            let lap = speedway_lap(&mut scene, &mut kart, id, &path, top, corner_g, 90.0);
            println!("top {} km/h, corners {} g: {:?}", top, corner_g, lap);
        }
    }

    // what makes a kart fun, in numbers: grip in a steady corner, how fast the steering bites, how a drift behaves
    fn kart_feel(tune: &dyn Fn(&mut VehicleController)) -> String
    {
        let mut out = String::new();
        let body_state = |scene: &Scene, id: u32| { let (_, b) = scene.physics.vehicle(id).unwrap(); (b.linvel(), b.angvel(), b.position().rotation) };

        // steady corner: full lock, speed held, lateral g
        for kmh in [40.0, 70.0]
        {
            let Some((mut scene, mut kart, id)) = build_kart(Vector3::zeros(), 0.0) else { return out; };
            tune(&mut kart);
            kart.build_physics(&mut scene);
            settle(&mut scene, &mut kart, id);
            set_speed(&mut scene, id, Vector3::z(), kmh / 3.6);
            let mut g: f32 = 0.0;
            let mut response = None;
            for step in 0..(4.0 / FRAME_DT) as usize
            {
                let (v, _, rotation) = body_state(&scene, id);
                let forward = rotation * Vector::Z;
                let speed = v.dot(forward);
                let input = VehicleInput { throttle: if speed < kmh / 3.6 { 1.0 } else { 0.0 }, steer: 1.0, ..Default::default() };
                run(&mut scene, &mut kart, id, input, FRAME_DT);
                let (v, w, _) = body_state(&scene, id);
                let lateral = v.length() * w.y.abs() / EARTH_GRAVITY;
                if step as f32 * FRAME_DT > 2.0 { g = f32::max(g, lateral); }
                if response.is_none() && w.y.abs() > 0.5 { response = Some(step as f32 * FRAME_DT); }
            }
            let (v, _, _) = body_state(&scene, id);
            out += &format!("corner {} km/h: {:.2} g, yaw 0.5 rad/s after {:?} s, speed then {:.0} km/h; ", kmh, g, response, v.length() * 3.6);
        }

        // drift: 60 km/h, full lock with the handbrake for 0.6 s, then throttle and counter steer by the driver
        {
            let Some((mut scene, mut kart, id)) = build_kart(Vector3::zeros(), 0.0) else { return out; };
            tune(&mut kart);
            kart.build_physics(&mut scene);
            settle(&mut scene, &mut kart, id);
            set_speed(&mut scene, id, Vector3::z(), 60.0 / 3.6);
            let mut max_slip: f32 = 0.0;
            let mut turned = 0.0;
            for step in 0..(3.0 / FRAME_DT) as usize
            {
                let t = step as f32 * FRAME_DT;
                let input = if t < 0.6 { VehicleInput { steer: 1.0, handbrake: true, throttle: 0.5, ..Default::default() } } else { VehicleInput { steer: 0.3, throttle: 1.0, ..Default::default() } };
                run(&mut scene, &mut kart, id, input, FRAME_DT);
                let (v, w, rotation) = body_state(&scene, id);
                let forward = rotation * Vector::Z;
                let side = (v - forward * v.dot(forward)).length();
                max_slip = max_slip.max(side.atan2(v.dot(forward).abs()).to_degrees());
                turned += w.y * FRAME_DT;
            }
            let (v, _, rotation) = body_state(&scene, id);
            out += &format!("drift: max slip {:.0} deg, turned {:.0} deg in 3 s, speed after {:.0} km/h, up {:.2}", max_slip, turned.to_degrees(), v.length() * 3.6, (rotation * Vector::Y).y);
        }

        out
    }

    #[test]
    #[ignore = "kart handling numbers, prints only - cargo test bench_kart_feel -- --ignored --nocapture"]
    fn bench_kart_feel()
    {
        println!("preset: {}", kart_feel(&|_| {}));


    }

    // a vehicle turned in the scene, wheels without front or back in their names (the re-volt car)
    #[test]
    fn the_forward_axis_follows_the_model_not_the_scene_rotation()
    {
        let mut controller = VehicleController::default();
        let wheels: Vec<(NodeItem, Vector3<f32>, f32)> = [(0.78, 1.27), (0.78, -1.09), (-0.78, 1.27), (-0.78, -1.09)].iter().map(|(x, z)| (Node::new("Wheel_L_01"), Vector3::new(*x, 0.0, *z), 0.28)).collect();

        for yaw in [0.0f32, 1.178, 2.5, -2.0]
        {
            let rotation = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), yaw);
            let forward = controller.detect_forward(&rotation, &Vector3::y(), &wheels);
            assert!((forward - Vector3::z()).norm() < 1e-4, "yaw {}: forward {:?}", yaw, forward);
        }

        controller.frame = Some(VehicleFrame { forward: Vector3::z(), up: Vector3::y() });
        controller.wheels = wheels.iter().map(|(_, center, radius)| VehicleWheel::new("wheel", *center, *radius)).collect();
        controller.assign_wheel_roles();
        assert_eq!(controller.wheels.iter().filter(|wheel| wheel.steer > 0.0).count(), 2);
        assert!(controller.wheels.iter().all(|wheel| (wheel.steer > 0.0) == (wheel.center.z > 0.0) && wheel.driven == (wheel.center.z < 0.0)));
    }

    const REVOLT_FILE: &str = "resourcesLocal/objects/temp/re-volt_3_-_sprinter_xl.glb";

    // the re-volt car at a scale, set up like the scene: car preset, wheels by name, front +z
    fn build_revolt(scale: f32) -> Option<(Scene, VehicleController, u32)>
    {
        let meshes = gltf_meshes(REVOLT_FILE)?;
        let mut scene = Scene::new("revolt bench");

        let lowest = meshes.iter().flat_map(|(_, vertices, _)| vertices.iter().map(|v| v.y * scale)).fold(f32::MAX, f32::min);
        let node = Node::new("revolt");
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", Vector3::new(0.0, -lowest + 0.02, 0.0), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0))))));

        let mut controller = VehicleController::default();
        controller.node = OptionOrId::Some(node.clone());
        controller.frame = Some(VehicleFrame { forward: Vector3::z(), up: Vector3::y() });

        for (name, vertices, _) in &meshes
        {
            let vertices: Vec<Vector3<f32>> = vertices.iter().map(|v| v * scale).collect();
            let (min, max) = VehicleController::bounds_of(&vertices)?;

            if name.starts_with("Wheel_")
            {
                controller.wheels.push(VehicleWheel::new(name, (min + max) * 0.5 / scale, (max.y - min.y) * 0.5 / scale));
            }
            else
            {
                controller.chassis_points.extend(vertices);
            }
        }

        controller.chassis_bounds = VehicleController::bounds_of(&controller.chassis_points);
        controller.apply_preset();
        controller.scale_vehicle(scale);
        controller.engine_state.reset(&controller.engine);
        controller.build_physics(&mut scene);

        let id = node.read().unwrap().id;
        Some((scene, controller, id))
    }

    #[test]
    #[ignore = "the re-volt car full size and at half size, prints only - cargo test bench_revolt -- --ignored --nocapture"]
    fn bench_revolt()
    {
        for scale in [1.0, 0.5]
        {
            let Some((mut scene, mut car, id)) = build_revolt(scale) else { println!("car file missing"); return; };
            settle(&mut scene, &mut car, id);
            let settled = position(&scene, id).y;

            let (mut to_50, mut to_100, mut speed) = (None, None, 0.0);
            for step in 0..(15.0 / FRAME_DT) as usize
            {
                speed = run(&mut scene, &mut car, id, VehicleInput { throttle: 1.0, ..Default::default() }, FRAME_DT).0;
                let t = step as f32 * FRAME_DT;
                if to_50.is_none() && speed * 3.6 >= 50.0 { to_50 = Some(t); }
                if to_100.is_none() && speed * 3.6 >= 100.0 { to_100 = Some(t); }
            }

            let start = position(&scene, id);
            let mut time = 0.0;
            while run(&mut scene, &mut car, id, VehicleInput { brake: 1.0, ..Default::default() }, FRAME_DT).0 > 0.5 && time < 10.0 { time += FRAME_DT; }
            let braking = (position(&scene, id) - start).norm();

            let Some((mut scene, mut car, id)) = build_revolt(scale) else { return; };
            settle(&mut scene, &mut car, id);
            set_speed(&mut scene, id, Vector3::z(), 50.0 / 3.6);
            let mut min_up: f32 = 1.0;
            for _ in 0..(3.0 / FRAME_DT) as usize
            {
                run(&mut scene, &mut car, id, VehicleInput { throttle: 0.3, steer: 1.0, ..Default::default() }, FRAME_DT);
                min_up = min_up.min((scene.physics.vehicle(id).unwrap().1.position().rotation * Vector::Y).y);
            }
            let yaw = scene.physics.vehicle(id).unwrap().1.angvel().y;

            println!("scale {}: mass {:.0} kg, settled {:.3} m, 0-50 {:?} s, 0-100 {:?} s, after 15 s {:.0} km/h, braking {:.0} m, full lock at 50: yaw {:.2} rad/s min up {:.2}", scale, car.chassis.mass, settled, to_50, to_100, speed * 3.6, braking, yaw, min_up);

            if scale != 1.0
            {
                if let Ok(path) = std::env::var("REVOLT_JSON") { std::fs::write(path, serde_json::to_string_pretty(&car).unwrap()).unwrap(); }
            }
        }
    }

    // a car half the usual length gets an eighth of the mass and forces, half the suspension - the wheels stay where they are
    #[test]
    fn the_preset_for_size_follows_the_measured_length()
    {
        let (_, mut car, _) = test_car(VehicleType::Car);
        let wheels_before: Vec<Vector3<f32>> = car.wheels.iter().map(|wheel| wheel.center).collect();
        car.chassis_bounds = Some((Vector3::new(-0.5, 0.2, -1.075), Vector3::new(0.5, 0.8, 1.075)));

        let factor = car.apply_preset_for_size().unwrap();
        assert!((factor - 0.5).abs() < 1e-4, "factor {}", factor);
        assert!((car.chassis.mass - 1300.0 / 8.0).abs() < 0.1, "mass {}", car.chassis.mass);
        assert!((car.suspension.rest_length - 0.15).abs() < 1e-4 && (car.suspension.stiffness - 70.0).abs() < 1e-3);
        assert_eq!(car.wheels.iter().map(|wheel| wheel.center).collect::<Vec<_>>(), wheels_before);
    }

    // tools/build_vehicle_types.py builds the controllers of its scenes on these
    #[test]
    #[ignore = "writes the preset of every vehicle type as scene json - cargo test dump_vehicle_presets -- --ignored"]
    fn dump_vehicle_presets()
    {
        let presets: serde_json::Map<String, serde_json::Value> = VehicleType::all().iter().map(|vehicle_type|
        {
            let mut controller = VehicleController::default();
            controller.vehicle_type = *vehicle_type;
            controller.apply_preset();

            let controller: Box<dyn SceneController> = Box::new(controller);
            (format!("{:?}", vehicle_type), serde_json::to_value(&controller).unwrap())
        }).collect();

        let path = std::env::var("PRESETS_JSON").unwrap_or_else(|_| "data/projects/vehicle_types/presets.json".to_string());
        std::fs::write(&path, serde_json::to_string_pretty(&presets).unwrap()).unwrap();
        println!("written {}", path);
    }

    fn kart_at_speed(speed_kmh: f32) -> Option<(Scene, VehicleController, u32)>
    {
        let (mut scene, mut kart, id) = build_kart(Vector3::zeros(), 0.0)?;
        settle(&mut scene, &mut kart, id);
        set_speed(&mut scene, id, Vector3::z(), speed_kmh / 3.6);
        Some((scene, kart, id))
    }

    // ********** the vehicles of the vehicle types project, measured like the auto setup does: wheels by name, the rider left out **********

    const VEHICLE_TYPES_DIR: &str = "data/projects/vehicle_types/assets";

    fn vehicle_type_model(file: &str, vehicle_type: VehicleType) -> Option<(Scene, VehicleController, u32)>
    {
        let meshes = gltf_meshes(&format!("{}/{}.glb", VEHICLE_TYPES_DIR, file))?;
        let mut scene = Scene::new("vehicle types bench");

        let node = Node::new(file);
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", Vector3::new(0.0, 0.02, 0.0), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0))))));

        let mut controller = VehicleController::default();
        controller.vehicle_type = vehicle_type;
        controller.node = OptionOrId::Some(node.clone());
        controller.frame = Some(VehicleFrame { forward: Vector3::z(), up: Vector3::y() });

        for (name, vertices, _) in &meshes
        {
            let (min, max) = VehicleController::bounds_of(vertices)?;

            if name.starts_with("Wheel")
            {
                controller.wheels.push(VehicleWheel::new(name, (min + max) * 0.5, (max.y - min.y) * 0.5));
            }
            else if !["Rider", "Driver", "Commander"].contains(&name.as_str())
            {
                controller.chassis_points.extend(vertices.iter().copied());
            }
        }

        controller.chassis_bounds = VehicleController::bounds_of(&controller.chassis_points);
        controller.apply_preset();
        controller.engine_state.reset(&controller.engine);
        controller.build_physics(&mut scene);

        let id = node.read().unwrap().id;
        Some((scene, controller, id))
    }

    // positive while leaning left, like the balance measures it
    fn lean_of(scene: &Scene, controller: &VehicleController, id: u32) -> f32
    {
        let right = car_rotation(scene, id) * controller.frame.unwrap().right();
        right.y.clamp(-1.0, 1.0).asin()
    }

    fn yaw_rate(scene: &Scene, id: u32) -> f32
    {
        scene.physics.vehicle(id).unwrap().1.angvel().y
    }

    fn trace_bike(scene: &Scene, bike: &VehicleController, id: u32, step: usize, phase: &str)
    {
        if std::env::var("TRACE_BIKE").map_or(true, |p| p != phase) || step % 6 != 0 { return; }
        let frame = bike.frame.unwrap();
        let (vehicle, body) = scene.physics.vehicle(id).unwrap();
        let forward = car_rotation(scene, id) * frame.forward;
        let contacts: String = vehicle.controller.wheels().iter().map(|w| if w.raycast_info().is_in_contact { 'c' } else { '-' }).collect();
        let steering: Vec<String> = vehicle.controller.wheels().iter().map(|w| format!("{:.1}", w.steering.to_degrees())).collect();
        let sides: Vec<String> = vehicle.controller.wheels().iter().map(|w| format!("{:.0}", w.side_impulse / FRAME_DT)).collect();
        println!("  t {:5.2} v {:5.1} lean {:6.1} pitch {:5.1} yaw {:6.1} deg/s roll {:6.1} deg/s steer {} side N {} wheels {} y {:.2}", step as f32 * FRAME_DT, body.linvel().length() * 3.6, lean_of(scene, bike, id).to_degrees(), forward.y.asin().to_degrees(), body.angvel().y.to_degrees(), body.angvel().dot(Vector::new(-forward.x, -forward.y, -forward.z)).to_degrees(), steering.join("/"), sides.join("/"), contacts, position(scene, id).y);
    }

    // steady turn: (lean deg, target lean deg, yaw deg/s, speed km/h, slip m/s, radius m, the most lean deg)
    fn two_wheel_turn(file: &str, vehicle_type: VehicleType, speed_kmh: f32, steer: f32) -> Option<(f32, f32, f32, f32, f32, f32, f32)>
    {
        let (mut scene, mut bike, id) = vehicle_type_model(file, vehicle_type)?;
        settle(&mut scene, &mut bike, id);
        set_speed(&mut scene, id, Vector3::z(), speed_kmh / 3.6);

        let frame = bike.frame.unwrap();
        let mut most: f32 = 0.0;
        let mut state = None;

        for _ in 0..(5.0 / FRAME_DT) as usize
        {
            let speed = scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6;
            let input = VehicleInput { throttle: if speed < speed_kmh { 1.0 } else { 0.0 }, steer, steer_analog: true, ..Default::default() };
            state = bike.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);
            most = most.max(lean_of(&scene, &bike, id).abs());

        }

        let state = state?;
        let target = state.lean_target;
        let speed = state.speed;
        let yaw = yaw_rate(&scene, id);
        let radius = if yaw.abs() > 0.01 { speed / yaw.abs() } else { f32::INFINITY };

        Some((lean_of(&scene, &bike, id).to_degrees(), target.to_degrees(), yaw.to_degrees(), speed * 3.6, state.lateral_left, radius, most.to_degrees()))
    }

    #[test]
    #[ignore = "motorcycle, bicycle and scooter of the vehicle types project, prints only - cargo test bench_two_wheelers -- --ignored --nocapture"]
    fn bench_two_wheelers()
    {
        for (file, vehicle_type, cruise) in [("motorcycle_enduro", VehicleType::Motorcycle, 60.0), ("bicycle_mtb", VehicleType::Bicycle, 25.0), ("scooter_retro", VehicleType::Scooter, 40.0)]
        {
            let Some((mut scene, mut bike, id)) = vehicle_type_model(file, vehicle_type) else { println!("{} missing", file); continue; };
            let frame = bike.frame.unwrap();
            println!("===== {:?} ({}): mass {:.0} kg, com {:?}, inertia {:?}, wheelbase {:.2} m", vehicle_type, file, bike.chassis.mass, bike.chassis.center_of_mass, bike.principal_inertia, bike.wheelbase(&frame));

            // standing
            let mut most: f32 = 0.0;
            for _ in 0..(3.0 / FRAME_DT) as usize
            {
                run(&mut scene, &mut bike, id, VehicleInput::default(), FRAME_DT);
                most = most.max(lean_of(&scene, &bike, id).abs());
            }
            println!("standing 3 s: lean {:.1} deg (most {:.1}), y {:.3}", lean_of(&scene, &bike, id).to_degrees(), most.to_degrees(), position(&scene, id).y);

            // full throttle
            let (mut to_20, mut speed, mut most) = (None, 0.0, 0.0f32);
            for step in 0..(10.0 / FRAME_DT) as usize
            {
                speed = run(&mut scene, &mut bike, id, VehicleInput { throttle: 1.0, ..Default::default() }, FRAME_DT).0;
                most = most.max(lean_of(&scene, &bike, id).abs());
                if to_20.is_none() && speed * 3.6 >= 20.0 { to_20 = Some(step as f32 * FRAME_DT); }
                trace_bike(&scene, &bike, id, step, "throttle");
            }
            println!("full throttle: 0-20 km/h {:?} s, after 10 s {:.0} km/h, most lean {:.1} deg, x drift {:.2} m", to_20, speed * 3.6, most.to_degrees(), position(&scene, id).x);

            // braking
            let start = position(&scene, id);
            let mut time = 0.0;
            let mut most: f32 = 0.0;
            while run(&mut scene, &mut bike, id, VehicleInput { brake: 1.0, ..Default::default() }, FRAME_DT).0 > 0.5 && time < 10.0 { time += FRAME_DT; most = most.max(lean_of(&scene, &bike, id).abs()); }
            println!("braking from {:.0} km/h: {:.1} m in {:.1} s, most lean {:.1} deg", speed * 3.6, (position(&scene, id) - start).norm(), time, most.to_degrees());

            // steady turns
            for speed in [cruise * 0.5, cruise, cruise * 1.5]
            {
                for steer in [0.3, 0.6, 1.0]
                {
                    if let Some((lean, target, yaw, v, slip, radius, most)) = two_wheel_turn(file, vehicle_type, speed, steer)
                    {
                        println!("  {:3.0} km/h steer {:.1}: lean {:5.1} (target {:5.1}, most {:5.1}) deg, yaw {:5.1} deg/s, radius {:6.1} m, {:5.1} km/h, slip {:5.2} m/s", speed, steer, lean, target, most, yaw, radius, v, slip);
                    }
                }
            }

            // slalom: a second left, a second right
            let (mut scene, mut bike, id) = vehicle_type_model(file, vehicle_type).unwrap();
            settle(&mut scene, &mut bike, id);
            set_speed(&mut scene, id, Vector3::z(), cruise / 3.6);
            let mut most: f32 = 0.0;
            for step in 0..(8.0 / FRAME_DT) as usize
            {
                let steer = if (step as f32 * FRAME_DT) as usize % 2 == 0 { 1.0 } else { -1.0 };
                let v = scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6;
                run(&mut scene, &mut bike, id, VehicleInput { throttle: if v < cruise { 1.0 } else { 0.0 }, steer, ..Default::default() }, FRAME_DT);
                most = most.max(lean_of(&scene, &bike, id).abs());
                trace_bike(&scene, &bike, id, step, "slalom");
            }
            let up = (car_rotation(&scene, id) * frame.up).y;
            println!("slalom at {:.0} km/h: most lean {:.1} deg, up {:.2} at the end, {:.0} km/h", cruise, most.to_degrees(), up, scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6);
        }
    }

    // full throttle from standing: (0-30 km/h s, most nose up deg, front load share over the first 1.5 s, steps with a front wheel in the air, yaw deg turned, most lean deg)
    fn launch(scene: &mut Scene, vehicle: &mut VehicleController, id: u32, steer: f32, seconds: f32) -> (Option<f32>, f32, f32, usize, f32, f32)
    {
        settle(scene, vehicle, id);
        let frame = vehicle.frame.unwrap();
        let com_along = vehicle.chassis.center_of_mass.dot(&frame.forward);
        let front: Vec<usize> = (0..vehicle.wheels.len()).filter(|i| vehicle.wheels[*i].center.dot(&frame.forward) > com_along).collect();
        let load = |scene: &Scene| -> f32 { let (v, _) = scene.physics.vehicle(id).unwrap(); front.iter().map(|i| v.controller.wheels()[*i].wheel_suspension_force).sum() };
        let mut rest = 0.0;
        for _ in 0..30
        {
            scene.physics.vehicle_mut(id).unwrap().1.wake_up(true);
            run(scene, vehicle, id, VehicleInput::default(), FRAME_DT);
            rest += load(scene) / 30.0;
        }
        let rest: f32 = rest.max(1.0);

        let (mut to_30, mut pitch, mut share, mut lifted, mut turned, mut lean) = (None, 0.0f32, 0.0, 0, 0.0, 0.0f32);
        let early = (1.5 / FRAME_DT) as usize;
        for step in 0..(seconds / FRAME_DT) as usize
        {
            let speed = run(scene, vehicle, id, VehicleInput { throttle: 1.0, steer, ..Default::default() }, FRAME_DT).0;
            let forward = car_rotation(scene, id) * frame.forward;
            pitch = pitch.max(forward.y.asin().to_degrees());
            if step < early { share += load(scene) / rest / early as f32; }
            if front.iter().any(|i| !scene.physics.vehicle(id).unwrap().0.controller.wheels()[*i].raycast_info().is_in_contact) { lifted += 1; }
            turned += yaw_rate(scene, id) * FRAME_DT;
            lean = lean.max(lean_of(scene, vehicle, id).abs());
            if to_30.is_none() && speed * 3.6 >= 30.0 { to_30 = Some(step as f32 * FRAME_DT); }
        }

        (to_30, pitch, share, lifted, turned.to_degrees(), lean.to_degrees())
    }

    #[test]
    #[ignore = "full throttle starts of the kart and the vehicle types, prints only - cargo test bench_launch -- --ignored --nocapture"]
    fn bench_launch()
    {
        let models = [("car_hot_hatch", VehicleType::Car), ("sports_wedge_gt", VehicleType::SportsCar), ("electric_volt_ev", VehicleType::ElectricCar), ("bus_school", VehicleType::Bus), ("truck_tipper", VehicleType::Truck), ("multiaxle_army_8x8", VehicleType::MultiAxle), ("motorcycle_enduro", VehicleType::Motorcycle), ("bicycle_mtb", VehicleType::Bicycle), ("scooter_retro", VehicleType::Scooter), ("trike_roadster", VehicleType::Trike), ("kart_race", VehicleType::Kart)];

        let report = |label: &str, build: &dyn Fn() -> Option<(Scene, VehicleController, u32)>|
        {
            let Some((mut scene, mut vehicle, id)) = build() else { println!("{} missing", label); return; };
            let (to_30, pitch, least, lifted, _, lean) = launch(&mut scene, &mut vehicle, id, 0.0, 4.0);
            let Some((mut scene, mut vehicle, id)) = build() else { return; };
            let (_, steer_pitch, steer_least, steer_lifted, turned, steer_lean) = launch(&mut scene, &mut vehicle, id, 1.0, 3.0);
            println!("{:20} straight: 0-30 {:?} s, nose up {:4.1} deg, front load {:4.0}%, lifted {:3} | steering: turned {:4.0} deg in 3 s, nose up {:4.1}, front load {:4.0}%, lifted {:3}, lean {:4.1}/{:4.1} deg", label, to_30.map(|t| (t * 100.0).round() / 100.0), pitch, least * 100.0, lifted, turned, steer_pitch, steer_least * 100.0, steer_lifted, lean, steer_lean);
        };

        report("yoshi kart", &|| build_kart(Vector3::zeros(), 0.0));
        for (file, vehicle_type) in models { report(file, &|| vehicle_type_model(file, vehicle_type)); }
    }

    #[test]
    #[ignore = "two wheelers: full keyboard steer at cruise speed and back, prints only - cargo test bench_lean_step -- --ignored --nocapture"]
    fn bench_lean_step()
    {
        for (file, vehicle_type, cruise) in [("motorcycle_enduro", VehicleType::Motorcycle, 60.0), ("bicycle_mtb", VehicleType::Bicycle, 25.0), ("scooter_retro", VehicleType::Scooter, 40.0)]
        {
            for tap in [0.25, 2.0]
            {
                let Some((mut scene, mut bike, id)) = vehicle_type_model(file, vehicle_type) else { println!("{} missing", file); continue; };
                settle(&mut scene, &mut bike, id);
                set_speed(&mut scene, id, Vector3::z(), cruise / 3.6);

                let mut samples = vec![];
                let (mut most, mut upright_at) = (0.0f32, None);
                for step in 0..(4.0 / FRAME_DT) as usize
                {
                    let t = step as f32 * FRAME_DT;
                    let v = scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6;
                    run(&mut scene, &mut bike, id, VehicleInput { throttle: if v < cruise { 1.0 } else { 0.0 }, steer: if t < tap { 1.0 } else { 0.0 }, ..Default::default() }, FRAME_DT);
                    let lean = lean_of(&scene, &bike, id).to_degrees();
                    most = most.max(lean.abs());
                    if [0.25, 0.5, 1.0, 2.0].iter().any(|s| (t - s).abs() < FRAME_DT * 0.5) { samples.push(format!("{:.2}s {:4.1}", t, lean)); }
                    if t > tap && upright_at.is_none() && lean.abs() < 3.0 { upright_at = Some(t - tap); }
                }
                println!("{:18} {:3.0} km/h tap {:.2} s: lean {} | most {:4.1} deg, upright again {:?} s after release", file, cruise, tap, samples.join(", "), most, upright_at.map(|t| (t * 100.0).round() / 100.0));
            }
        }
    }

    #[test]
    #[ignore = "the tank of the vehicle types project turning, prints only - cargo test bench_tank -- --ignored --nocapture"]
    fn bench_tank()
    {
        let turn = |throttle: f32, brake: f32, steer: f32, label: &str|
        {
            let Some((mut scene, mut tank, id)) = vehicle_type_model("tank_desert", VehicleType::Tank) else { println!("tank missing"); return; };
            settle(&mut scene, &mut tank, id);
            let start = position(&scene, id);

            let mut yaw = 0.0;
            for step in 0..(6.0 / FRAME_DT) as usize
            {
                run(&mut scene, &mut tank, id, VehicleInput { throttle, brake, steer, ..Default::default() }, FRAME_DT);
                yaw = yaw_rate(&scene, id);

                if std::env::var("TRACE_TANK").is_ok() && step % 30 == 0
                {
                    let at = position(&scene, id);
                    let wheels: Vec<String> = scene.physics.vehicle(id).unwrap().0.controller.wheels().iter().map(|w| format!("{:.0}/{:.0}", w.forward_impulse / FRAME_DT / 1000.0, w.side_impulse / FRAME_DT / 1000.0)).collect();
                    println!("  t {:.1} at {:.2} {:.2} yaw {:.1} deg/s | kN fwd/side {}", step as f32 * FRAME_DT, at.x, at.z, yaw.to_degrees(), wheels.join(" "));
                }
            }

            let heading = car_rotation(&scene, id) * tank.frame.unwrap().forward;
            let turned = heading.x.atan2(heading.z).to_degrees();
            let speed = scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6;
            println!("{}: yaw {:.1} deg/s, turned {:.0} deg in 6 s, moved {:.1} m, {:.1} km/h, gear {}", label, yaw.to_degrees(), turned, (position(&scene, id) - start).norm(), speed, tank.engine_state.gear);
        };

        if let Some((_, tank, _)) = vehicle_type_model("tank_desert", VehicleType::Tank) { println!("tank: mass {:.0}, com {:?}, inertia {:?}", tank.chassis.mass, tank.chassis.center_of_mass, tank.principal_inertia); }
        turn(0.0, 0.0, 1.0, "on the spot, left");
        turn(0.0, 0.0, -1.0, "on the spot, right");
        turn(0.0, 0.0, 0.5, "on the spot, half");
        turn(1.0, 0.0, 1.0, "full throttle, left");
        turn(0.4, 0.0, 1.0, "40% throttle, left");
        turn(0.0, 1.0, 1.0, "reversing, left");
    }

    // the test car with a single axle box trailer behind it, coupled at the rear bumper
    fn car_with_trailer(height: f32, break_roll: f32, break_force: f32) -> (Scene, VehicleController, u32, VehicleController, u32)
    {
        let (mut scene, mut car, car_id) = test_car(VehicleType::Car);

        let node = Node::new("trailer");
        let mut transformation = Transformation::identity("trans");
        transformation.set_local_transform(Matrix4::new_translation(&Vector3::new(0.0, 0.0, 4.6)));
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transformation))));

        let mut trailer = VehicleController::default();
        trailer.vehicle_type = VehicleType::Trailer;
        trailer.node = OptionOrId::Some(node.clone());
        trailer.frame = Some(VehicleFrame { forward: -Vector3::z(), up: Vector3::y() });
        trailer.wheels = vec![VehicleWheel::new("l", Vector3::new(-0.9, 0.37, 0.0), 0.37), VehicleWheel::new("r", Vector3::new(0.9, 0.37, 0.0), 0.37)];

        // a box on the axle and a drawbar to the ball 2.5 m ahead
        for x in [-0.8, 0.8] { for y in [0.45, height] { for z in [-1.3, 1.2] { trailer.chassis_points.push(Vector3::new(x, y, z)); } } }
        for x in [-0.05, 0.05] { for y in [0.45, 0.55] { trailer.chassis_points.push(Vector3::new(x, y, -2.5)); } }
        trailer.chassis_bounds = VehicleController::bounds_of(&trailer.chassis_points);

        trailer.apply_preset();
        trailer.build_physics(&mut scene);

        car.trailer = OptionOrId::Some(node.clone());
        car.hitch.point_auto = false;
        car.hitch.point = Vector3::new(0.0, 0.5, 2.1);
        car.hitch.break_roll = break_roll;
        car.hitch.break_force = break_force;
        car.hitch_dirty = true;
        car.sync_hitch(&mut scene, car_id);

        let trailer_id = node.read().unwrap().id;
        (scene, car, car_id, trailer, trailer_id)
    }

    fn run_train(scene: &mut Scene, car: &mut VehicleController, car_id: u32, trailer: &mut VehicleController, trailer_id: u32, input: VehicleInput, seconds: f32)
    {
        let (car_frame, trailer_frame) = (car.frame.unwrap(), trailer.frame.unwrap());

        for _ in 0..(seconds / FRAME_DT) as usize
        {
            car.drive(&mut scene.physics, car_id, &car_frame, &input, FRAME_DT);
            trailer.drive(&mut scene.physics, trailer_id, &trailer_frame, &VehicleInput::default(), FRAME_DT);

            scene.physics.step(FRAME_DT, false);
            scene.physics.apply_dynamic_bodies(false);
        }
    }

    // CCD_MODE=hard | none | soft:<prediction m> - for comparing the vehicle ccd variants, unset keeps the default
    fn set_ccd_mode(body: &mut rapier3d::prelude::RigidBody)
    {
        match std::env::var("CCD_MODE").unwrap_or_default().as_str()
        {
            "hard" => { body.set_soft_ccd_prediction(0.0); body.enable_ccd(true); }
            "none" => { body.set_soft_ccd_prediction(0.0); body.enable_ccd(false); }
            mode if mode.starts_with("soft:") => { body.enable_ccd(false); body.set_soft_ccd_prediction(mode[5..].parse().unwrap()); }
            _ => {}
        }
    }

    // driving straight over flat ground made of triangles, like a level: the biggest upward kick per step - a pop out of nothing
    #[test]
    #[ignore]
    fn bench_flat_pops()
    {
        use rapier3d::prelude::ColliderBuilder;

        let (cells, size) = (100, 2.0);
        let mut vertices = vec![];
        let mut indices = vec![];
        for i in 0..=cells { for j in 0..=cells { vertices.push(Vector::new((i as f32 - cells as f32 / 2.0) * size, 0.0, (j as f32 - cells as f32) * size + 20.0)); } }
        for i in 0..cells { for j in 0..cells
        {
            let a = (i * (cells + 1) + j) as u32;
            let (b, c, d) = (a + 1, a + cells as u32 + 1, a + cells as u32 + 2);
            indices.push([a, c, b]);
            indices.push([b, c, d]);
        } }

        for speed in [30.0, 60.0, 100.0]
        {
            let (mut scene, mut car, id) = test_car(VehicleType::Car);
            set_ccd_mode(scene.physics.vehicle_mut(id).unwrap().1);
            scene.physics.set_ground_plane(None);
            scene.physics.colliders.insert(ColliderBuilder::trimesh(vertices.clone(), indices.clone()).unwrap().build());
            settle(&mut scene, &mut car, id);
            set_speed(&mut scene, id, -Vector3::z(), speed / 3.6);

            let (mut kick, mut rise, mut lowest, mut highest): (f32, f32, f32, f32) = (0.0, 0.0, f32::MAX, f32::MIN);
            let mut previous = scene.physics.vehicle(id).unwrap().1.linvel().y;
            for _ in 0..(4.0 / FRAME_DT) as usize
            {
                run(&mut scene, &mut car, id, VehicleInput { throttle: 0.4, ..Default::default() }, FRAME_DT);
                let body = scene.physics.vehicle(id).unwrap().1;
                kick = kick.max(body.linvel().y - previous);
                rise = rise.max(body.linvel().y);
                previous = body.linvel().y;
                lowest = lowest.min(body.translation().y);
                highest = highest.max(body.translation().y);
            }
            println!("{:>3} km/h: biggest upward kick {:.3} m/s in one frame, fastest rise {:.3} m/s, height {:.3}..{:.3} m", speed, kick, rise, lowest, highest);
        }
    }

    // the off road car on the real test course, several lanes: the biggest upward kick per frame and where it happened
    #[test]
    #[ignore]
    fn bench_course_pops()
    {
        for lane in [-12.0, -6.0, 0.0, 6.0, 12.0]
        {
            for speed in [40.0, 70.0]
            {
                let Some((mut scene, mut car, id)) = offroad_car(Vector3::new(lane, 0.0, -20.0), ChassisShape::ConvexHull) else { println!("course files missing"); return; };
                set_ccd_mode(scene.physics.vehicle_mut(id).unwrap().1);
                settle(&mut scene, &mut car, id);

                let frame = car.frame.unwrap();
                let (mut kick, mut at): (f32, Vector3<f32>) = (0.0, Vector3::zeros());
                let mut previous = 0.0;
                for _ in 0..(8.0 / FRAME_DT) as usize
                {
                    let current = scene.physics.vehicle(id).unwrap().1.linvel().length() * 3.6;
                    let input = VehicleInput { throttle: if current < speed { 1.0 } else { 0.0 }, ..Default::default() };
                    car.drive(&mut scene.physics, id, &frame, &input, FRAME_DT);
                    scene.physics.step(FRAME_DT, false);
                    scene.physics.apply_dynamic_bodies(false);

                    let body = scene.physics.vehicle(id).unwrap().1;
                    if body.linvel().y - previous > kick
                    {
                        kick = body.linvel().y - previous;
                        at = Vector3::new(body.translation().x, body.translation().y, body.translation().z);
                    }
                    previous = body.linvel().y;
                }
                println!("lane {:>5.1} {:>3} km/h: biggest upward kick {:.2} m/s in one frame at {:.1?}", lane, speed, kick, at);
            }
        }
    }

    // a car at speed against a thin static wall: does it get through?
    #[test]
    #[ignore]
    fn bench_thin_wall()
    {
        use rapier3d::prelude::ColliderBuilder;

        for speed in [100.0, 180.0, 260.0]
        {
            for thickness in [0.05, 0.2]
            {
                let (mut scene, mut car, id) = test_car(VehicleType::Car);
                set_ccd_mode(scene.physics.vehicle_mut(id).unwrap().1);
                settle(&mut scene, &mut car, id);
                scene.physics.colliders.insert(ColliderBuilder::cuboid(4.0, 1.5, thickness * 0.5).translation(Vector::new(0.0, 1.5, -30.0)).build());
                set_speed(&mut scene, id, -Vector3::z(), speed / 3.6);
                run(&mut scene, &mut car, id, VehicleInput { throttle: 1.0, ..Default::default() }, 1.5);
                let z = position(&scene, id).z;
                println!("{:>3} km/h, wall {:.2} m: car ends at z {:.1} -> {}", speed, thickness, z, if z < -30.0 { "THROUGH" } else { "stopped" });
            }
        }
    }

    // a train from a trailer test scene, geometry from its glbs: per wheel contact, spring length and how far it is drawn off its modelled spot
    #[test]
    #[ignore]
    fn bench_trailer_scene()
    {
        for file in std::env::var("TRAIN_SCENE").map(|f| vec![f]).unwrap_or(vec!["tt_05_drawbar".to_string(), "tt_06_road_train".to_string()])
        {
            let text = std::fs::read_to_string(format!("data/projects/trailer_test/{}.scene", file)).unwrap();
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            let mut scene = Scene::new("train bench");
            let mut vehicles: Vec<(VehicleController, u32, Vector3<f32>, Vec<(String, Vector3<f32>)>)> = vec![];
            let mut nodes: HashMap<String, NodeItem> = HashMap::new();

            for value in json["controller"].as_array().unwrap()
            {
                let mut controller: VehicleController = serde_json::from_value(value.clone()).unwrap();
                let uuid = value["node"].as_str().unwrap().to_string();
                let object = json["objects"].as_array().unwrap().iter().find(|o| o["uuid"] == uuid).unwrap();
                let at: Vec<f32> = object["position"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
                let at = Vector3::new(at[0], at[1], at[2]);
                let meshes = gltf_meshes(&format!("data/projects/trailer_test/{}", object["source"].as_str().unwrap())).unwrap();

                let node = Node::new(object["name"].as_str().unwrap());
                node.write().unwrap().uuid = uuid.clone();
                node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", at, Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0))))));

                let mut parts = vec![];
                for (name, vertices, _) in &meshes
                {
                    if name.starts_with("Wheel") { continue; }
                    controller.chassis_points.extend(vertices.iter().copied());
                    controller.chassis_parts.push(vertices.clone());
                    let (min, max) = VehicleController::bounds_of(vertices).unwrap();
                    parts.push((name.clone(), (min + max) * 0.5));
                }
                controller.chassis_bounds = VehicleController::bounds_of(&controller.chassis_points);
                controller.node = OptionOrId::Some(node.clone());
                controller.engine_state.reset(&controller.engine);
                controller.build_physics(&mut scene);

                let id = node.read().unwrap().id;
                nodes.insert(uuid, node);
                vehicles.push((controller, id, at, parts));
            }

            // the couplings where the generator put them: the trailer's kingpin or eye, in the tow vehicle's space
            for index in 0..vehicles.len()
            {
                let Some(trailer_uuid) = vehicles[index].0.trailer.id().map(|id| id.to_string()) else { continue; };
                let trailer = vehicles.iter().find(|(c, ..)| c.node.as_ref().is_some_and(|n| n.read().unwrap().uuid == trailer_uuid)).unwrap();
                let (_, coupling) = trailer.3.iter().find(|(name, _)| name == "Kingpin" || name == "Hitch").unwrap().clone();
                let point = trailer.2 + coupling - vehicles[index].2;
                let node = nodes[&trailer_uuid].clone();
                let (controller, id, ..) = &mut vehicles[index];
                controller.trailer = OptionOrId::Some(node);
                controller.hitch.point_auto = false;
                controller.hitch.point = point;
                controller.hitch_dirty = true;
                controller.sync_hitch(&mut scene, *id);
            }

            // the loose load (crate, flatbed load, truck truck truck): a body per object when combined, else per mesh - a convex hull per mesh like the engine
            let mut loads: Vec<(String, rapier3d::prelude::RigidBodyHandle, Vector)> = vec![];
            for object in json["objects"].as_array().unwrap()
            {
                let physics = &object["options"]["settings"]["physics"];
                let source = object["source"].as_str().unwrap_or("");
                if physics["body_type"] != "Dynamic" || source.starts_with("assets/yard") { continue; }

                let at: Vec<f32> = object["position"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
                let density = physics["density"].as_f64().unwrap() as f32;
                let friction = physics["friction"].as_f64().unwrap() as f32;
                let meshes = gltf_meshes(&format!("data/projects/trailer_test/{}", source)).unwrap();
                let groups: Vec<Vec<&GltfMesh>> = if physics["combine_children"].as_bool().unwrap_or(false) { vec![meshes.iter().collect()] } else { meshes.iter().map(|mesh| vec![mesh]).collect() };

                for group in groups
                {
                    let body = scene.physics.bodies.insert(rapier3d::prelude::RigidBodyBuilder::dynamic().translation(Vector::new(at[0], at[1], at[2])).build());
                    for (_, vertices, _) in &group
                    {
                        let points: Vec<Vector> = vertices.iter().map(|v| Vector::new(v.x, v.y, v.z)).collect();
                        let Some(builder) = rapier3d::prelude::ColliderBuilder::convex_hull(&points) else { continue; };
                        scene.physics.colliders.insert_with_parent(builder.density(density).friction(friction).build(), body, &mut scene.physics.bodies);
                    }
                    let name = if group.len() > 1 { object["name"].as_str().unwrap().to_string() } else { group[0].0.clone() };
                    loads.push((name, body, scene.physics.bodies.get(body).unwrap().center_of_mass()));
                }
            }

            let report_loads = |scene: &Scene, vehicles: &Vec<(VehicleController, u32, Vector3<f32>, Vec<(String, Vector3<f32>)>)>|
            {
                let Some((_, trailer_id, trailer_start, _)) = vehicles.get(1) else { return; };
                let moved = scene.physics.vehicle(*trailer_id).unwrap().1.translation().z - trailer_start.z;
                for (name, body, start) in &loads
                {
                    let body = scene.physics.bodies.get(*body).unwrap();
                    let center = body.center_of_mass();
                    let tilt = (body.rotation() * Vector::Y).y.clamp(-1.0, 1.0).acos().to_degrees();
                    println!("    {:<20} dy {:+.3} dz vs trailer {:+.3} tilt {:.1}", name, center.y - start.y, center.z - start.z - moved, tilt);
                }
            };

            let report = |scene: &Scene, vehicles: &Vec<(VehicleController, u32, Vector3<f32>, Vec<(String, Vector3<f32>)>)>, label: &str|
            {
                println!("== {} {} - head at {:.1} km/h", file, label, scene.physics.vehicle(vehicles[0].1).unwrap().1.linvel().length() * 3.6);
                for (controller, id, start, _) in vehicles
                {
                    let (vehicle, body) = scene.physics.vehicle(*id).unwrap();
                    let rotation = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(body.rotation().w, body.rotation().x, body.rotation().y, body.rotation().z));
                    let pitch = (rotation * Vector3::z()).y.asin().to_degrees();
                    let t = body.translation();
                    let wheels: Vec<String> = vehicle.controller.wheels().iter().zip(controller.wheels.iter()).map(|(wheel, w)|
                    {
                        let info = wheel.raycast_info();
                        let compression = if info.is_in_contact { controller.suspension.rest_length - controller.sag - info.suspension_length } else { -controller.sag };
                        format!("{}{}:{:+.2}", w.node_name.replace("Wheel ", "").replace("left", "L").replace("right", "R").replace("inner", "i").replace(' ', ""), if info.is_in_contact { "" } else { "(AIR)" }, compression)
                    }).collect();
                    println!("  {:<16} {:>7.0} kg y {:+.3} (start {:+.3}) pitch {:+.1} | sag {:.3} | {}", controller.node_name, controller.chassis.mass, t.y, start.y, pitch, controller.sag, wheels.join(" "));
                }
            };

            let frames = |scene: &mut Scene, vehicles: &mut Vec<(VehicleController, u32, Vector3<f32>, Vec<(String, Vector3<f32>)>)>, input: VehicleInput, seconds: f32|
            {
                for _ in 0..(seconds / FRAME_DT) as usize
                {
                    for (index, (controller, id, ..)) in vehicles.iter_mut().enumerate()
                    {
                        let frame = controller.frame.unwrap();
                        let own = if index == 0 { input } else { VehicleInput::default() };
                        controller.drive(&mut scene.physics, *id, &frame, &own, FRAME_DT);
                    }
                    scene.physics.step(FRAME_DT, false);
                    scene.physics.apply_dynamic_bodies(false);
                }
            };

            frames(&mut scene, &mut vehicles, VehicleInput::default(), 3.0);
            report(&scene, &vehicles, "standing 3 s");
            report_loads(&scene, &vehicles);
            frames(&mut scene, &mut vehicles, VehicleInput { throttle: 0.5, ..Default::default() }, 6.0);
            report(&scene, &vehicles, "half throttle 6 s");
            report_loads(&scene, &vehicles);
            frames(&mut scene, &mut vehicles, VehicleInput { throttle: 1.0, ..Default::default() }, 6.0);
            report(&scene, &vehicles, "full throttle 6 s");
            frames(&mut scene, &mut vehicles, VehicleInput { brake: 1.0, ..Default::default() }, 4.0);
            report(&scene, &vehicles, "full brake 4 s");
            report_loads(&scene, &vehicles);
        }
    }

    // a loose crate on the trailer at speed: how much it moves against the trailer per step
    #[test]
    #[ignore]
    fn bench_trailer_load()
    {
        use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};

        for speed in [30.0, 60.0, 90.0]
        {
            let (mut scene, mut car, car_id, mut trailer, trailer_id) = car_with_trailer(1.3, 0.0, 0.0);
            for id in [car_id, trailer_id] { set_ccd_mode(scene.physics.vehicle_mut(id).unwrap().1); }
            run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput::default(), 1.0);

            let top = scene.physics.vehicle(trailer_id).unwrap().1.position().transform_point(Vector::new(0.0, 1.3 + 0.41, -0.3));
            let crate_body = scene.physics.bodies.insert(RigidBodyBuilder::dynamic().translation(top).build());
            scene.physics.colliders.insert_with_parent(ColliderBuilder::cuboid(0.4, 0.4, 0.4).density(150.0).friction(0.7).build(), crate_body, &mut scene.physics.bodies);
            run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput::default(), 1.0);

            for id in [car_id, trailer_id] { scene.physics.vehicle_mut(id).unwrap().1.set_linvel(Vector::new(0.0, 0.0, -speed / 3.6), true); }
            scene.physics.bodies.get_mut(crate_body).unwrap().set_linvel(Vector::new(0.0, 0.0, -speed / 3.6), true);

            let local = |scene: &Scene| { let t = scene.physics.vehicle(trailer_id).unwrap().1.position(); t.inverse_transform_point(scene.physics.bodies.get(crate_body).unwrap().translation()) };
            let mut samples = vec![];
            for _ in 0..120
            {
                run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { throttle: 0.6, ..Default::default() }, FRAME_DT);
                samples.push(local(&scene));
            }

            let mean = samples.iter().copied().sum::<Vector>() / samples.len() as f32;
            let jitter: Vec<f32> = samples.windows(3).map(|w| (w[1] - (w[0] + w[2]) * 0.5).length()).collect();
            let worst = jitter.iter().copied().fold(0.0, f32::max);
            let average = jitter.iter().sum::<f32>() / jitter.len() as f32;
            let drift = samples.last().unwrap().z - samples[0].z;
            println!("{:>3} km/h: crate on the trailer at {:.3?}, drifted {:.3} m along, frame to frame wobble avg {:.4} m, worst {:.4} m, end speed {:.0} km/h", speed, mean, drift, average, worst, scene.physics.vehicle(car_id).unwrap().1.linvel().length() * 3.6);
        }
    }

    // hitch force, trailer angles and the gap at the ball - straight, accelerating, braking, cornering and tipping
    #[test]
    #[ignore]
    fn bench_trailer()
    {
        let report = |scene: &Scene, car: &VehicleController, car_id: u32, trailer_id: u32, label: &str|
        {
            let hitch = scene.physics.hitch_of(trailer_id).unwrap();
            let (_, car_body) = scene.physics.vehicle(car_id).unwrap();
            let (_, trailer_body) = scene.physics.vehicle(trailer_id).unwrap();
            let ball = car_body.position().transform_point(Vector::new(car.hitch.point.x, car.hitch.point.y, car.hitch.point.z));
            let eye = trailer_body.position().transform_point(Vector::new(0.0, 0.5, -2.5));
            println!("{:<34} {:>5.1} km/h | {:?} {:>6.2} kN | yaw {:>5.1} pitch {:>5.1} roll {:>5.1} deg | gap {:.3} m", label, car_body.linvel().length() * 3.6, hitch.state, hitch.force / 1000.0, hitch.angles.x.to_degrees(), hitch.angles.y.to_degrees(), hitch.angles.z.to_degrees(), (ball - eye).length());
        };

        let (mut scene, mut car, car_id, mut trailer, trailer_id) = car_with_trailer(1.3, 0.0, 0.0);
        println!("trailer: mass {:.0} kg, com {:.2?} - tongue load by the lever {:.2} kN", trailer.chassis.mass, trailer.chassis.center_of_mass, trailer.chassis.mass * EARTH_GRAVITY * (-trailer.chassis.center_of_mass.z) / 2.5 / 1000.0);

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput::default(), 2.0);
        report(&scene, &car, car_id, trailer_id, "standing");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { throttle: 1.0, ..Default::default() }, 1.0);
        report(&scene, &car, car_id, trailer_id, "full throttle 1 s");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { throttle: 1.0, ..Default::default() }, 5.0);
        report(&scene, &car, car_id, trailer_id, "full throttle 6 s");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { throttle: 0.3, steer: 0.5, ..Default::default() }, 3.0);
        report(&scene, &car, car_id, trailer_id, "half lock 3 s");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { brake: 1.0, ..Default::default() }, 0.5);
        report(&scene, &car, car_id, trailer_id, "full brake 0.5 s");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { brake: 1.0, ..Default::default() }, 4.0);
        report(&scene, &car, car_id, trailer_id, "full brake + reverse 4.5 s");

        run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { brake: 1.0, steer: 1.0, ..Default::default() }, 3.0);
        report(&scene, &car, car_id, trailer_id, "reversing full lock 3 s");

        // fast and a hard swerve: the trailer may roll - tear off at 30 deg
        for (speed, height, label) in [(60.0, 1.3, "swerve at 60"), (90.0, 1.3, "swerve at 90"), (60.0, 2.8, "tall box, swerve at 60"), (80.0, 2.8, "tall box, swerve at 80")]
        {
            let (mut scene, mut car, car_id, mut trailer, trailer_id) = car_with_trailer(height, 30.0, 40.0);
            trailer.chassis.center_of_mass_height = 0.6;
            trailer.build_physics(&mut scene);
            car.drift.counter_steer = 0.0;
            run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput::default(), 1.0);

            for id in [car_id, trailer_id]
            {
                let (_, body) = scene.physics.vehicle_mut(id).unwrap();
                body.set_linvel(Vector::new(0.0, 0.0, -speed / 3.6), true);
            }

            let mut most_roll: f32 = 0.0;
            let mut most_force: f32 = 0.0;
            for step in 0..(3.0 / FRAME_DT) as usize
            {
                let steer = if step < 40 { 1.0 } else if step < 80 { -1.0 } else { 0.0 };
                run_train(&mut scene, &mut car, car_id, &mut trailer, trailer_id, VehicleInput { throttle: 0.5, steer, ..Default::default() }, FRAME_DT);

                let hitch = scene.physics.hitch_of(trailer_id).unwrap();
                most_roll = most_roll.max(hitch.angles.z.abs().to_degrees());
                most_force = most_force.max(hitch.force / 1000.0);
            }

            report(&scene, &car, car_id, trailer_id, label);
            println!("    most roll {:.1} deg, most force {:.1} kN", most_roll, most_force);
        }
    }
}

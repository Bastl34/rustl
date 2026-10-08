#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, Weak};

use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use serde::{Deserialize, Serialize};
use parry3d::bounding_volume::{Aabb, BoundingVolume};
use parry3d::mass_properties::MassProperties;
use parry3d::query::DefaultQueryDispatcher;
use parry3d::shape::{Shape, TypedShape};
use rapier3d::prelude::*;
use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};

use crate::{component_downcast, component_downcast_mut, console_warning, helper::math::{extract_rotation_quat_from_transform, extract_scale_from_transform, extract_translation_from_transform}, state::{scene::{components::{component::ComponentItem, mesh::Mesh, transformation::Transformation}, node::{InstanceItemArc, Node, NodeItem, PhysicsBodyType, PhysicsSettings, PhysicsShape}, scene::Scene}}};

use super::contacts::{CharacterTouch, ContactEvent, ContactTarget, ContactTracker, Measure, CHARACTER_TOUCH_KEEP};

// transform deltas below this are treated as float noise and do not trigger a bvh update
const TRANSFORM_EPSILON: f32 = 0.00001;

// a scale change needs a shape rebuild, so it uses a slightly more forgiving threshold
const SCALE_EPSILON: f32 = 0.0001;

// Above this ratio between the largest and smallest scale above a body, a rotating rigid
// body stretches enough to be obvious.
const NON_UNIFORM_SCALE_LIMIT: f32 = 1.5;

// Smallest half extent a primitive collider is built with, and the share of the object's
// own size used when it is flat. A flat mesh would otherwise produce a volume-less shape,
// and a dynamic body without volume has no mass.
const MIN_SHAPE_HALF_EXTENT: f32 = 0.001;
const MIN_SHAPE_THICKNESS_RATIO: f32 = 0.02;

// Separating an author move from float noise in the solver round trip. Far above that
// noise, far below anything a gizmo drag produces.
const AUTHOR_MOVE_EPSILON: f32 = 0.001;

// re-deriving the mass properties integrates the shape, so small edits are not worth it
const DENSITY_EPSILON: f32 = 0.0001;
const CENTER_OF_MASS_EPSILON: f32 = 0.0001;

// Half size of the ground plane quad. A flat cuboid jittered the character over 6 cm.
const GROUND_PLANE_HALF_SIZE: f32 = 500.0;

// Edge length of a single ground plane tile. Measured: a bowling pin standing on its own
// base tips over on a plane made of two 1000 unit triangles and stands on one made of small
// ones. Contact generation between a shape a few centimetres wide and a triangle that large
// loses too much precision, and the leftover torque topples anything narrow.
const GROUND_PLANE_TILE_SIZE: f32 = 25.0;

// How far below the ground plane an object may reach before it is lifted back onto it.
// Above the contact slack, so a resting object never triggers it.
const GROUND_RECOVERY_MARGIN: f32 = 0.05;

// Largest safe snap distance - 0.08 already buries a slim capsule 7 cm in the floor.
pub const SNAP_TO_GROUND_LIMIT: f32 = 0.03;

// Rescan interval for new or removed mesh instances - every frame would be wasteful.
const NODE_SCAN_INTERVAL_FRAMES: u32 = 10;

// How close two waiting objects have to be to count as touching when one of them is
// released. Cell fracture pieces share their faces, a settled pile rests within the
// contact skin.
const RELEASE_TOUCH_DISTANCE: f32 = 0.01;

// Steps after a run starts during which nothing counts as a hit: objects placed slightly
// into each other are pushed apart in the first steps, and that push is as hard as a hit.
const HIT_GRACE_STEPS: u32 = 10;

// A body that stays within this distance and angle of where it was for WOBBLE_TIME is
// wobbling in place and gets frozen too. Measured: thin glass shards lying on each other
// rock at up to 3.6 rad/s for minutes without going anywhere, too fast to ever count as
// resting, and a 2 m shard leaning on others swings ±1.5° at 3 Hz while creeping 5 mm/s.
const WOBBLE_DRIFT: f32 = 0.1;
const WOBBLE_ANGLE: f32 = 0.2;
const WOBBLE_TIME: f32 = 3.0;

// How long a body has to stay in place before the settle damping takes hold. Short enough to
// catch the rocking early, long enough that a body only just hit is already on its way.
const SETTLE_DELAY: f32 = 0.3;

// Anything faster than the wake speed covers the wobble distance well within this, so a
// body that has not for this long no longer counts as heading anywhere.
const WOBBLE_MOVER_TIME: f32 = 0.25;

// past the settle time: moved less than this within SETTLE_WINDOW, however it turned, and it is frozen
const SETTLE_DRIFT: f32 = 0.2;
const SETTLE_WINDOW: f32 = 0.5;

// a support that moved or turned further than this is checked for whether it still touches the frozen body on it
const SUPPORT_DRIFT: f32 = 0.01;
const SUPPORT_ANGLE: f32 = 0.02;

// how many loads deep on a vehicle are still kept from freezing, e.g. carrier - truck - pickup
const CARRY_DEPTH: usize = 4;

// Everything about a physics world the author gets to set. Kept apart from the solver
// state so it can be serialized with the scene and edited in the ui.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PhysicsWorldSettings
{
    pub gravity: Vector3<f32>,

    pub fixed_timestep: f32, // a solver needs a constant dt, the frame time is not
    pub max_substeps: u32,

    // endless floor - the editor grid cannot serve as one, it is rebuilt on every change
    pub ground_plane: bool,
    pub ground_plane_y: f32,

    // A body falls asleep once it stays below the thresholds for this long, and a sleeping
    // body costs nothing and stops wobbling. The defaults are rapier's and assume meters.
    // Rapier 0.36 only uses the angular one for bodies without a collider, and it sleeps a
    // group of touching bodies only once all of them are resting at the same moment.
    #[serde(default = "default_sleep_linear_threshold")]
    pub sleep_linear_threshold: f32,
    #[serde(default = "default_sleep_angular_threshold")]
    pub sleep_angular_threshold: f32,
    #[serde(default = "default_time_until_sleep")]
    pub time_until_sleep: f32,

    // A body resting on its own for freeze_after seconds becomes fixed until a hit, so a
    // fallen pile stops costing solver time without waiting for its slowest member.
    #[serde(default = "default_freeze_resting")]
    pub freeze_resting: bool,
    #[serde(default = "default_freeze_after")]
    pub freeze_after: f32,

    // anything faster than this wakes a frozen body, well below the hit speed so a push works
    #[serde(default = "default_wake_speed")]
    pub wake_speed: f32,

    // seconds a body may move after it was set moving, then it is frozen as soon as it gets nowhere - 0 turns it off
    #[serde(default = "default_settle_time")]
    pub settle_time: f32,

    // extra damping on a body that has stayed in place for a moment, so it stops rocking
    // instead of rocking on until it is frozen - 0 turns it off
    #[serde(default = "default_settle_damping")]
    pub settle_damping: f32,

    // How hard the solver works per step. Above rapier's own default of 4, which leaves a
    // visible wobble on tall narrow props in a real scene. It is not a cure for an unstable
    // contact though - measured, a rack of nudged bowling pins gets worse at 32, not better,
    // because the extra passes feed in more of whatever is wrong. Reach for the contact
    // softness first, this is for settling piles and stacks.
    #[serde(default = "default_solver_iterations")]
    pub solver_iterations: usize,

    // A touch slower than this does not count as a hit for an object that waits for one:
    // the speed of the touching body before the step, in units per second. Resting weight
    // never counts that way, whatever is stacked on top, and 1.0 is a drop from about 5 cm.
    #[serde(default = "default_hit_speed")]
    pub hit_speed: f32,
}

fn default_solver_iterations() -> usize { 8 }
fn default_hit_speed() -> f32 { 1.0 }

fn default_sleep_linear_threshold() -> f32 { 0.05 }
fn default_sleep_angular_threshold() -> f32 { 0.5 }
fn default_time_until_sleep() -> f32 { 0.5 }
fn default_freeze_resting() -> bool { true }
fn default_freeze_after() -> f32 { 1.0 }
fn default_wake_speed() -> f32 { 0.5 }
fn default_settle_time() -> f32 { 3.0 }
fn default_settle_damping() -> f32 { 5.0 }

impl Default for PhysicsWorldSettings
{
    fn default() -> Self
    {
        PhysicsWorldSettings
        {
            gravity: Vector3::new(0.0, -9.81, 0.0),

            fixed_timestep: 1.0 / 60.0,
            max_substeps: 4,

            // without a floor a dynamic object simply falls out of the world, which reads
            // as a broken simulation - y = 0 is where the editor grid sits
            ground_plane: true,
            ground_plane_y: 0.0,

            sleep_linear_threshold: default_sleep_linear_threshold(),
            sleep_angular_threshold: default_sleep_angular_threshold(),
            time_until_sleep: default_time_until_sleep(),
            freeze_resting: default_freeze_resting(),
            freeze_after: default_freeze_after(),
            wake_speed: default_wake_speed(),
            settle_time: default_settle_time(),
            settle_damping: default_settle_damping(),

            solver_iterations: default_solver_iterations(),
            hit_speed: default_hit_speed(),
        }
    }
}

// ********** objects **********

// Where a physics object takes its pose from, and where the solver result goes back to.
// Every object is a set of colliders around an anchor, one per mesh instance, plus a body
// when the solver may move it. The anchor is the only thing that differs between a single
// mesh and a whole node treated as one object - a ragdoll bone will be a third kind.
#[derive(Clone)]
pub enum Anchor
{
    // one mesh placement: the pose is the instance's world pose and the solver writes the
    // instance transform - the usual case, and the only one for static geometry
    Instance { node: NodeItem, instance: InstanceItemArc },

    // a node with everything below it as one object: the pose is the node's world pose,
    // the solver writes the node transform and the meshes below follow with their offsets
    Node { node: NodeItem },
}

// Identifies an anchor without touching any lock: the node id, plus the instance id.
pub type AnchorKey = (u32, Option<u32>);

impl Anchor
{
    pub fn node(&self) -> &NodeItem
    {
        match self
        {
            Anchor::Instance { node, .. } => node,
            Anchor::Node { node } => node,
        }
    }

    pub fn key(&self) -> AnchorKey
    {
        match self
        {
            Anchor::Instance { node, instance } => (node.read().unwrap().id, Some(instance.read().unwrap().id)),
            Anchor::Node { node } => (node.read().unwrap().id, None),
        }
    }

    pub fn name(&self) -> String
    {
        self.node().read().unwrap().name.clone()
    }

    // The settings that apply here. resolve_physics walks up to the first non-static
    // node, which for a node anchor is the node itself.
    pub fn physics(&self) -> PhysicsSettings
    {
        self.node().read().unwrap().resolve_physics()
    }

    // The pose the scene shows right now: from the cache the renderer also uses for an
    // instance, computed live for a node - a node usually has no instance of its own, and
    // its transform is what the gizmo edits when a whole object is selected.
    pub fn world_transform(&self) -> Matrix4<f32>
    {
        match self
        {
            Anchor::Instance { instance, .. } => instance.read().unwrap().get_cached_world_transform(),
            Anchor::Node { node } => node.read().unwrap().get_full_transform(),
        }
    }

    // Computed from the scene graph: for a brand new instance, which has no cached
    // transform yet, and after a restore that just changed the transforms above.
    pub fn world_transform_live(&self) -> Matrix4<f32>
    {
        match self
        {
            Anchor::Instance { instance, .. } => instance.read().unwrap().calculate_transform(),
            Anchor::Node { node } => node.read().unwrap().get_full_transform(),
        }
    }

    // The transform the written pose sits below: the node for an instance, the parent for
    // a node. A non-uniform scale in there stretches a rotating body, the scale of the
    // written transform itself does not - it is applied first.
    fn frame(&self) -> Matrix4<f32>
    {
        match self
        {
            Anchor::Instance { node, .. } => node.read().unwrap().get_full_transform(),
            Anchor::Node { node } =>
            {
                let node_read = node.read().unwrap();

                let inherits = node_read.find_component::<Transformation>().map(|transformation|
                {
                    component_downcast!(transformation, Transformation);
                    transformation.has_parent_inheritance()
                }).unwrap_or(true);

                if !inherits
                {
                    return Matrix4::identity();
                }

                node_read.parent.as_ref().map(|parent| parent.read().unwrap().get_full_transform()).unwrap_or_else(Matrix4::identity)
            }
        }
    }

    // the transformation the solver writes to
    fn transformation(&self) -> Option<ComponentItem>
    {
        match self
        {
            Anchor::Instance { instance, .. } => instance.read().unwrap().find_component::<Transformation>(),
            Anchor::Node { node } => node.read().unwrap().find_component::<Transformation>(),
        }
    }

    // The solver result has nowhere to go without a transformation. An identity one
    // changes nothing visually.
    fn ensure_transformation(&self)
    {
        if self.transformation().is_some()
        {
            return;
        }

        match self
        {
            Anchor::Instance { instance, .. } =>
            {
                instance.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Physics Transformation")))));
            }
            Anchor::Node { node } =>
            {
                node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Physics Transformation")))));
            }
        }
    }

    // Writes a solver pose back into the scene. Returns the transform the scene will
    // actually show afterwards, or None when there is nothing to write to.
    //
    // What the scene shows, not what was intended: the transform component stores a
    // position, a rotation and a scale, and a matrix that does not decompose into those
    // loses the rest. Comparing against the intention instead would look like an author
    // move every single frame and teleport the body onto it, which is how an object ends
    // up shooting off on first contact.
    fn write_back(&self, pose: &Pose) -> Option<Matrix4<f32>>
    {
        let transformation = self.transformation()?;

        let frame = self.frame();
        let frame_inverse = frame.try_inverse()?;

        let local =
        {
            component_downcast!(transformation, Transformation);
            let scale = extract_scale_from_transform(transformation.get_transform());

            PhysicsWorld::local_from_pose(pose, &frame, &frame_inverse, &scale)
        };

        {
            component_downcast_mut!(transformation, Transformation);
            transformation.set_local_transform(local);
        }

        match self
        {
            // same as for a node: the renderer would show it a frame late - a load on a trailer then trails it by the distance of one frame
            Anchor::Instance { instance, .. } =>
            {
                let world_matrix = instance.read().unwrap().calculate_transform();
                instance.write().unwrap().get_data_mut().get_mut().computed.world_matrix = world_matrix;

                Some(world_matrix)
            }
            Anchor::Node { node } =>
            {
                // The scene refreshes the cached world matrices in its update, which has
                // already run this frame, and the renderer consumes the node's change flag
                // right after this - before the instances below get to see it. So the
                // caches are refreshed here, or the meshes stay put while their colliders fall.
                PhysicsWorld::refresh_instance_cache_below(node);

                Some(node.read().unwrap().get_full_transform())
            }
        }
    }
}

// Where a body was when it last moved on, to notice it wobbling in place.
#[derive(Clone, Copy)]
struct RestTrack
{
    start: Pose,
    time: f32,
}

// A frozen body resting on one that moves again, see release_unsupported.
#[derive(Clone, Copy)]
struct SupportWatch
{
    held: usize, // entry index of the frozen body
    support: usize, // entry index of what it rests on
    start: Pose, // where the support was when the watch began
}

// One collider: a mesh instance at an offset from its anchor.
pub struct Part
{
    pub node: NodeItem,
    pub node_id: u32,

    pub instance: InstanceItemArc,
    pub instance_id: u32,

    pub handle: ColliderHandle,
    pub shape_kind: PhysicsShape, // what it was built as, to notice a change in the editor

    // the part relative to its anchor, the anchor's scale included - a change to it means
    // the author edited something below the anchor
    local: Matrix4<f32>,
    offset: Pose,        // the rigid part of local, the collider's offset from the anchor
    scale: Vector3<f32>, // the rest, baked into the shape
}

// A physics object: colliders around an anchor, plus a body when the solver may move it.
// A single mesh is an object with one part at offset zero; a combined object has one part
// per mesh below its root. Static objects have no body and their colliders stand alone -
// rapier propagates a body's pose to its colliders only in a step, and nothing steps while
// the scene is being edited.
pub struct BodyEntry
{
    pub anchor: Anchor,
    pub key: AnchorKey,

    pub body: Option<RigidBodyHandle>,
    pub body_type: PhysicsBodyType,

    // Waiting for a hit: a fixed body until something releases it, see release_entry.
    // reacts_on_hit is the authored flag, so every run starts waiting; waiting is the
    // live state.
    pub reacts_on_hit: bool,
    pub waiting: bool,

    // rested long enough to be made fixed until the next hit, see freeze_resting_bodies
    pub frozen: bool,
    rest: Option<RestTrack>,
    settling: bool, // carries the settle damping on top of its own
    active_time: f32, // since it was last set moving: run start, release or thaw

    // the pose before and after the last step while awake - written back in between like the vehicles, or a load on a trailer jitters against it
    step_pose: Option<(Pose, Pose)>,

    pub parts: Vec<Part>,

    // every part the scene asked for, built or not - a mesh without geometry stays on this
    // list, otherwise the object would count as changed and be rebuilt on every scan
    requested_parts: HashSet<(u32, u32)>,

    transform: Matrix4<f32>, // the anchor's world transform, as last handed to or received from the solver
    scale: Vector3<f32>,     // the anchor's world scale, baked into every part

    // what the mass properties were last built from - recomputing them means integrating
    // the shapes again, so it only happens when one of these actually changed
    applied_density: f32,
    applied_center_of_mass: Option<Vector3<f32>>, // None = left to the shapes
}

impl BodyEntry
{
    pub fn node_id(&self) -> u32
    {
        self.key.0
    }

    pub fn is_combined(&self) -> bool
    {
        matches!(self.anchor, Anchor::Node { .. })
    }

    // as its anchor or as one of its meshes
    pub fn has_node(&self, node_id: u32) -> bool
    {
        self.key.0 == node_id || self.parts.iter().any(|part| part.node_id == node_id)
    }

    pub fn has_instance(&self, node_id: u32, instance_id: u32) -> bool
    {
        self.parts.iter().any(|part| part.node_id == node_id && part.instance_id == instance_id)
    }
}

// A collider as the debug view draws it, mesh colliders (trimesh, convex hull) reduced to their oriented bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PhysicsDebugShape
{
    Box { half_extents: Vector3<f32> },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 }, // along the local y axis
    Bounds { half_extents: Vector3<f32> },
    Segment { a: Vector3<f32>, b: Vector3<f32> }, // one edge, local space
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PhysicsDebugState
{
    Static,
    Kinematic,
    Dynamic,
    Sleeping,
    Waiting, // reacts on its first hit and has not been hit yet
    Character,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsDebugVolume
{
    pub shape: PhysicsDebugShape,
    pub transform: Matrix4<f32>, // world from shape, rigid
    pub state: PhysicsDebugState,
}

// A character moves by shape casts and never becomes a collider, so only the debug view needs its capsule.
struct CharacterShape
{
    node: Weak<RwLock<Box<Node>>>, // weak, a deleted character simply drops out
    center_offset: f32,
    half_height: f32,
    radius: f32,
}

// The chassis a vehicle controller asks for, in chassis space: the rigid part of the vehicle node's world transform.
pub struct VehicleChassisDesc
{
    pub shape: SharedShape,
    pub mass: f32,
    pub center_of_mass: Vector3<f32>,
    pub principal_inertia: Vector3<f32>,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,

    // frictionless massless balls in chassis space (center, radius) - they slide the vehicle up over edges its wheel rays cannot see yet
    pub bumpers: Vec<(Vector3<f32>, f32)>,

    // the vehicle axes in chassis space - a hitch is straight when both vehicles' axes line up
    pub forward: Vector3<f32>,
    pub up: Vector3<f32>,
}

// One wheel, in chassis space.
pub struct VehicleWheelDesc
{
    pub connection: Vector3<f32>, // where the suspension is mounted
    pub direction: Vector3<f32>,  // suspension direction, usually chassis down
    pub axle: Vector3<f32>,       // forward x up, so a positive engine force always drives forward
    pub rest_length: f32,
    pub radius: f32,
    pub tuning: WheelTuning,
}

// A vehicle: its own body, driven by rapier's ray cast vehicle. The node and everything below stay out of the entries.
pub struct VehicleEntry
{
    node: Weak<RwLock<Box<Node>>>,

    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub controller: DynamicRayCastVehicleController,

    previous: Pose,         // body pose before the last step, the written pose is interpolated from it
    shown: Matrix4<f32>,    // the node world transform as last written or followed
    start: Option<Matrix4<f32>>, // node local transform when the run started

    // nodes outside the vehicle that ride along, e.g. the driver and the passengers, at a pose in chassis space
    riders: Vec<(Weak<RwLock<Box<Node>>>, Pose)>,
    rider_starts: Vec<(Weak<RwLock<Box<Node>>>, Matrix4<f32>)>, // node local transforms when the run started

    frame: (Vector, Vector), // forward and up, chassis space

    pre_velocity: Vector, // before the last step - what changed it besides the wheels came through the hitch
}

impl VehicleEntry
{
    fn place_riders(&self, pose: &Pose)
    {
        for (rider, seat) in &self.riders
        {
            let Some(rider) = rider.upgrade() else { continue; };

            let anchor = Anchor::Node { node: rider };
            anchor.ensure_transformation();
            anchor.write_back(&(*pose * *seat));
        }
    }

    // Standing still at the given pose: no motion, no pending forces, no driver input.
    fn stop_at(&mut self, bodies: &mut RigidBodySet, pose: Pose)
    {
        if let Some(body) = bodies.get_mut(self.body)
        {
            PhysicsWorld::teleport(body, pose);
            body.reset_forces(true);
            body.reset_torques(true);
        }

        for wheel in self.controller.wheels_mut()
        {
            wheel.engine_force = 0.0;
            wheel.brake = 0.0;
            wheel.steering = 0.0;
            wheel.rotation = 0.0;
        }

        self.previous = pose;
    }

    // the vehicle axes as a rotation from the joint axes: x forward, y up, z right
    fn frame_rotation(&self) -> Rotation
    {
        let (forward, up) = self.frame;
        let up = up.normalize_or(Vector::Y);
        let forward = (forward - up * forward.dot(up)).normalize_or(Vector::Z);

        Rotation::from_mat3(&Matrix::from_cols(forward, up, forward.cross(up)))
    }

    // the sum of what rapier's vehicle applied in its last update: suspension, drive and side grip - the step leaves the wheels as they are
    fn applied_wheel_impulse(&self, dt: f32) -> Vector
    {
        self.controller.wheels().iter().filter(|wheel| wheel.raycast_info().is_in_contact).map(|wheel|
        {
            let info = wheel.raycast_info();
            let normal = info.contact_normal_ws;
            let side = (wheel.axle() - normal * wheel.axle().dot(normal)).normalize_or_zero();
            let forward = normal.cross(side).normalize_or_zero();

            normal * wheel.wheel_suspension_force.min(wheel.max_suspension_force) * dt + forward * wheel.forward_impulse + side * wheel.side_impulse
        }).sum()
    }
}

// A trailer coupling the tow vehicle's controller asks for - a ball joint between the two chassis bodies.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HitchDesc
{
    pub point: Vector, // the ball, chassis space of the tow vehicle

    // rad to each side, from the straight line - at or above PI the axis is free
    pub yaw_limit: f32,
    pub pitch_limit: f32,
    pub roll_limit: f32,

    pub break_roll: f32, // rad of roll against the tow vehicle that tear the trailer off, 0 = never
    pub break_force: f32, // N at the ball that tear it off, 0 = never
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HitchState
{
    Waiting, // one of the two vehicles has no body yet
    Coupled,
    Broken, // torn off - stays apart until the run restarts or the tow vehicle recovers
}

pub struct HitchEntry
{
    pub tow: u32, // node id of the tow vehicle, the trailer is the key
    pub desc: HitchDesc,
    pub state: HitchState,

    pub brake: f32, // 0..1, the tow vehicle brakes with it and the trailer follows
    pub force: f32, // N at the ball in the last step
    pub angles: Vector3<f32>, // rad of the trailer against the tow vehicle: yaw, pitch, roll

    joint: Option<ImpulseJointHandle>,
    anchor: Option<Vector>, // the ball in trailer chassis space, measured when it was coupled
    release_in: f32, // s until a torn off pair collides again
}

// a torn off pair stays without contacts this long - the drawbar still sits in the tow vehicle's body, s
const HITCH_RELEASE_TIME: f32 = 0.5;

// s the hitch force is smoothed over - a single step spike, e.g. a landing, does not tear the trailer off
const HITCH_FORCE_SMOOTHING: f32 = 0.03;

// What the scene wants built at one anchor, collected during a scan.
struct Request
{
    anchor: Anchor,
    key: AnchorKey,
    parts: Vec<(NodeItem, InstanceItemArc)>,
}

// Static geometry is mirrored from the scene, dynamic bodies are moved by the solver.
pub struct PhysicsWorld
{
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub broad_phase_bvh: BroadPhaseBvh,
    pub integration_params: IntegrationParameters,

    islands: IslandManager,
    dispatcher: DefaultQueryDispatcher,

    // only used once a body exists - a purely static world never steps
    pipeline: PhysicsPipeline,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    soft_bodies: SoftBodySet, // not used currently
    ccd_solver: CCDSolver,

    pub settings: PhysicsWorldSettings,

    time_accumulator: f32,
    body_amount: usize,
    run_steps: u32, // since the run started - hits only count after a short grace

    // kinematic bodies the scene moved this frame - rapier reports no velocity for a
    // position driven kinematic body after the step, so this is what "moving" means here
    moved_kinematics: HashSet<RigidBodyHandle>,

    // The speed of every dynamic body before this frame's steps. A hit is judged by how
    // fast the hitter arrived, not by the impulse the solver applied: a resting contact's
    // impulse grows with everything stacked on top, and released waiting bodies under any
    // pile of props. Measured: g * dt = 0.16 per unit mass and step per stacked object.
    pre_step_speed: HashMap<RigidBodyHandle, f32>,

    // frozen bodies on top of something that moves again, thawed once it gets away
    supports: Vec<SupportWatch>,

    // nothing simulates outside a running mode, and leaving one has to put every
    // dynamic object back where the author placed it
    running: bool,

    snapshot_pending: bool,
    edit_snapshot: HashMap<(u32, u32), Matrix4<f32>>,

    // The gizmo edits the node transform when a whole object is selected, not the instance
    // one, so a snapshot of the instances alone would leave those moves behind.
    edit_snapshot_nodes: HashMap<u32, Matrix4<f32>>,

    entries: Vec<BodyEntry>,

    // the built floor collider plus the height it was built at, so that a settings
    // change is noticed and the plane rebuilt
    ground_plane: Option<ColliderHandle>,
    applied_ground_plane: Option<f32>,

    // the sleep settings the bodies were last given, they live per body in rapier
    applied_sleep: Option<(f32, f32, f32)>,

    // pick up objects loaded after the world was built, and drop disabled ones again.
    // not an authored setting: the editor always wants this, a build may turn it off to
    // save the recurring node scan
    pub auto_add_nodes: bool,
    scan_countdown: u32,

    // last sync result, so the editor can show whether anything is still being rebuilt per frame
    pub last_synced: usize,
    pub last_shape_rebuilds: usize,

    // never collidable, by node id - characters belong here, their skin re-syncs every frame
    excluded_nodes: HashSet<u32>,

    // character capsules by node id, debug view only - kept across a rebuild like excluded_nodes
    characters: HashMap<u32, CharacterShape>,

    // vehicles by node id - kept across a rebuild, only their controllers set them up or remove them
    vehicles: HashMap<u32, VehicleEntry>,

    // trailer couplings by the trailer's node id - the tow vehicle's controller sets them up
    hitches: HashMap<u32, HitchEntry>,

    // started, touching and stopped contacts of the last frame, see contact_events
    contacts: ContactTracker,
}

impl PhysicsWorld
{
    pub fn new() -> PhysicsWorld
    {
        let mut world = Self::empty();
        world.rebuild_ground_plane();

        world
    }

    fn empty() -> PhysicsWorld
    {
        PhysicsWorld
        {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            broad_phase_bvh: BroadPhaseBvh::new(),
            integration_params: IntegrationParameters::default(),

            islands: IslandManager::new(),
            dispatcher: DefaultQueryDispatcher,

            pipeline: PhysicsPipeline::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            soft_bodies: SoftBodySet::new(),
            ccd_solver: CCDSolver::new(),

            settings: PhysicsWorldSettings::default(),

            time_accumulator: 0.0,
            body_amount: 0,
            run_steps: 0,
            moved_kinematics: HashSet::new(),
            pre_step_speed: HashMap::new(),
            supports: vec![],
            running: true, // the editor turns this off, a game build just runs
            // a run that starts with the world needs its start state too - else leaving the scene does not put its vehicles back
            snapshot_pending: true,
            edit_snapshot: HashMap::new(),
            edit_snapshot_nodes: HashMap::new(),

            entries: vec![],
            ground_plane: None,
            applied_ground_plane: None,
            applied_sleep: None,
            auto_add_nodes: true,
            scan_countdown: 0,
            last_synced: 0,
            last_shape_rebuilds: 0,
            excluded_nodes: HashSet::new(),
            characters: HashMap::new(),
            vehicles: HashMap::new(),
            hitches: HashMap::new(),
            contacts: ContactTracker::default(),
        }
    }

    // Marks nodes as never collidable. Additive, and survives a rebuild.
    pub fn exclude_nodes(&mut self, node_ids: &HashSet<u32>)
    {
        self.excluded_nodes.extend(node_ids.iter());

        for node_id in node_ids
        {
            self.remove_node(*node_id);
        }
    }

    pub fn is_excluded(&self, node_id: u32) -> bool
    {
        self.excluded_nodes.contains(&node_id)
    }

    pub fn clear(&mut self)
    {
        let old_bodies = std::mem::replace(&mut self.bodies, RigidBodySet::new());
        let old_colliders = std::mem::replace(&mut self.colliders, ColliderSet::new());
        self.broad_phase_bvh = BroadPhaseBvh::new();
        self.islands = IslandManager::new();
        self.narrow_phase = NarrowPhase::new();
        self.impulse_joints = ImpulseJointSet::new();
        self.multibody_joints = MultibodyJointSet::new();
        self.soft_bodies = SoftBodySet::new();
        self.time_accumulator = 0.0;
        self.body_amount = 0;

        self.entries.clear();
        self.supports.clear();
        self.ground_plane = None;
        self.applied_ground_plane = None;
        self.applied_sleep = None;
        // excluded_nodes is kept on purpose - a rebuild must not resurrect character colliders

        // the new collider set hands out the old handles again
        self.contacts.reset();

        // vehicles belong to their controllers, not to the scene colliders - they keep their motion and run start
        self.carry_vehicles(&old_bodies, &old_colliders);

        // the new joint set hands out the old handles again - the couplings are made again at the next step
        for hitch in self.hitches.values_mut()
        {
            hitch.joint = None;
        }


        // the ground plane is configuration, not scene content, so it survives a rebuild
        self.rebuild_ground_plane();
    }

    // Places an endless floor at the given height, or removes it with None.
    pub fn set_ground_plane(&mut self, y: Option<f32>)
    {
        self.settings.ground_plane = y.is_some();

        if let Some(y) = y
        {
            self.settings.ground_plane_y = y;
        }

        self.rebuild_ground_plane();
    }

    // Picks up a settings change made elsewhere, e.g. in the ui. Cheap enough to call
    // every frame: only a changed ground plane costs anything.
    pub fn apply_settings(&mut self)
    {
        if self.configured_ground_plane() != self.applied_ground_plane
        {
            self.rebuild_ground_plane();
        }

        let sleep = self.configured_sleep();

        if Some(sleep) != self.applied_sleep
        {
            self.applied_sleep = Some(sleep);
            self.apply_sleep_settings();
        }
    }

    fn configured_sleep(&self) -> (f32, f32, f32)
    {
        (
            self.settings.sleep_linear_threshold.max(0.0),
            self.settings.sleep_angular_threshold.max(0.0),
            self.settings.time_until_sleep.max(0.0)
        )
    }

    // rapier keeps the thresholds per body, so a world level change has to be handed out
    fn apply_sleep_settings(&mut self)
    {
        let (linear, angular, time_until_sleep) = self.configured_sleep();

        for (_handle, body) in self.bodies.iter_mut()
        {
            let activation = body.activation_mut();

            activation.normalized_linear_threshold = linear;
            activation.angular_threshold = angular;
            activation.time_until_sleep = time_until_sleep;
        }
    }

    fn configured_ground_plane(&self) -> Option<f32>
    {
        if self.settings.ground_plane
        {
            Some(self.settings.ground_plane_y)
        }
        else
        {
            None
        }
    }

    fn rebuild_ground_plane(&mut self)
    {
        if let Some(handle) = self.ground_plane.take()
        {
            self.colliders.remove(handle, &mut self.islands, &mut self.bodies, &mut self.soft_bodies, false);
            self.rebuild_bvh();
        }

        self.applied_ground_plane = self.configured_ground_plane();

        let Some(y) = self.applied_ground_plane else { return; };

        let half = GROUND_PLANE_HALF_SIZE;

        // Measured, do not "improve" this into a solid shape: a halfspace is never found by
        // the bvh backed queries and the character drops straight through it, and a deep
        // cuboid buries the character 1.4 cm while walking and snags it on walls. A surface
        // has no inside to sink into, which is exactly why this one works.
        //
        // Tiled rather than two huge triangles, see GROUND_PLANE_TILE_SIZE.
        let tiles = ((half * 2.0) / GROUND_PLANE_TILE_SIZE).ceil().max(1.0) as u32;
        let step = (half * 2.0) / tiles as f32;
        let row = tiles + 1;

        let mut vertices = vec![];

        for z in 0..row
        {
            for x in 0..row
            {
                vertices.push(Vector::new(-half + x as f32 * step, 0.0, -half + z as f32 * step));
            }
        }

        let mut indices = vec![];

        for z in 0..tiles
        {
            for x in 0..tiles
            {
                // same winding a single quad had: (x0,z0) (x1,z0) (x1,z1) (x0,z1)
                let v00 = z * row + x;
                let v10 = v00 + 1;
                let v01 = v00 + row;
                let v11 = v01 + 1;

                indices.push([v00, v10, v11]);
                indices.push([v00, v11, v01]);
            }
        }

        let Ok(shape) = SharedShape::trimesh(vertices, indices) else { return; };

        let pose = Pose::from_translation(Vector::new(0.0, y, 0.0));
        // the friction every scene object gets by default - rapier's own 0.5 made the floor the slipperiest surface
        let collider = ColliderBuilder::new(shape).position(pose).friction(PhysicsSettings::default().friction).build();
        let handle = self.colliders.insert(collider);

        self.ground_plane = Some(handle);
        self.refresh_leaf(handle);
    }

    // An object pushed through the floor is lifted back onto it. Measured by its lowest
    // part, not its origin - a pivot away from the geometry would read as below the floor
    // while the object is sitting on it.
    fn recover_escaped_bodies(&mut self)
    {
        let Some(ground_y) = self.applied_ground_plane else { return; };

        for index in 0..self.entries.len()
        {
            if self.entries[index].body_type != PhysicsBodyType::Dynamic
            {
                continue;
            }

            let Some(body) = self.entries[index].body else { continue; };

            let lowest = self.entries[index].parts.iter()
                .filter_map(|part| self.colliders.get(part.handle))
                .map(|collider| collider.compute_aabb().mins.y)
                .fold(f32::INFINITY, f32::min);

            if !lowest.is_finite()
            {
                continue;
            }

            let penetration = ground_y - lowest;

            if penetration <= GROUND_RECOVERY_MARGIN
            {
                continue;
            }

            if let Some(body) = self.bodies.get_mut(body)
            {
                let mut pose = *body.position();
                pose.translation.y += penetration;

                Self::teleport(body, pose);
            }
        }
    }

    pub fn ground_plane_y(&self) -> Option<f32>
    {
        self.applied_ground_plane
    }

    // ********** lookups **********

    pub fn collider_amount(&self) -> usize
    {
        self.entries.iter().map(|entry| entry.parts.len()).sum()
    }

    // objects whose meshes share one body
    pub fn combined_amount(&self) -> usize
    {
        self.entries.iter().filter(|entry| entry.is_combined()).count()
    }

    pub fn body_amount(&self) -> usize
    {
        self.body_amount
    }

    pub fn has_dynamics(&self) -> bool
    {
        self.body_amount > 0 || !self.vehicles.is_empty()
    }

    pub fn is_empty(&self) -> bool
    {
        self.entries.is_empty() && self.ground_plane.is_none() && self.vehicles.is_empty()
    }

    pub fn entries(&self) -> &Vec<BodyEntry>
    {
        &self.entries
    }

    pub fn has_node(&self, node_id: u32) -> bool
    {
        self.entries.iter().any(|entry| entry.has_node(node_id))
    }

    pub fn has_instance(&self, node_id: u32, instance_id: u32) -> bool
    {
        self.entries.iter().any(|entry| entry.has_instance(node_id, instance_id))
    }

    // The body of the first object a node is part of, as its anchor or as one of its
    // meshes. None for static geometry. This is where gameplay code gets hold of a body,
    // to push it or to lock it.
    pub fn body_of(&self, node_id: u32) -> Option<RigidBodyHandle>
    {
        self.entries.iter().find(|entry| entry.has_node(node_id)).and_then(|entry| entry.body)
    }

    // ********** transform helpers **********

    // puts a body somewhere else at a standstill
    fn teleport(body: &mut RigidBody, pose: Pose)
    {
        body.set_position(pose, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
    }

    // splits a transform into a rigid pose (for the body or collider) and a scale (baked into the shape)
    fn split_transform(transform: &Matrix4<f32>) -> (Pose, Vector3<f32>)
    {
        let translation = extract_translation_from_transform(transform);
        let rotation = extract_rotation_quat_from_transform(transform);
        let scale = extract_scale_from_transform(transform);

        let pose = Pose::from_parts
        (
            Vector::new(translation.x, translation.y, translation.z),
            Rotation::from_xyzw(rotation.i, rotation.j, rotation.k, rotation.w)
        );

        (pose, scale)
    }

    fn transform_differs(a: &Matrix4<f32>, b: &Matrix4<f32>) -> bool
    {
        a.iter().zip(b.iter()).any(|(a, b)| (a - b).abs() > TRANSFORM_EPSILON)
    }

    // Relative: at large coordinates a float step is already bigger than a fixed
    // threshold, and every frame would then look like an author move.
    fn differs_beyond_noise(a: &Matrix4<f32>, b: &Matrix4<f32>) -> bool
    {
        a.iter().zip(b.iter()).any(|(a, b)|
        {
            (a - b).abs() > AUTHOR_MOVE_EPSILON * (1.0 + a.abs().max(b.abs()))
        })
    }

    fn scale_differs(a: &Vector3<f32>, b: &Vector3<f32>) -> bool
    {
        (a.x - b.x).abs() > SCALE_EPSILON || (a.y - b.y).abs() > SCALE_EPSILON || (a.z - b.z).abs() > SCALE_EPSILON
    }

    // A solver pose as a local transform below the given frame, carrying the scale the
    // transform component already has.
    //
    // A plain frame_inverse * world would be right in principle, but a parent with a
    // non-uniform scale turns any rotation below it into shear, and the transform component
    // holds a position, a rotation and a scale - nothing else. The shear would land in the
    // scale and visibly stretch the object. So the local transform is assembled from parts
    // that always decompose cleanly: the exact position, a pure rotation, and the scale
    // that was there before. The solver never changes scale anyway.
    fn local_from_pose(pose: &Pose, frame: &Matrix4<f32>, frame_inverse: &Matrix4<f32>, scale: &Vector3<f32>) -> Matrix4<f32>
    {
        let world = pose.to_mat4();
        let world = Matrix4::new
        (
            world.x_axis.x, world.y_axis.x, world.z_axis.x, world.w_axis.x,
            world.x_axis.y, world.y_axis.y, world.z_axis.y, world.w_axis.y,
            world.x_axis.z, world.y_axis.z, world.z_axis.z, world.w_axis.z,
            world.x_axis.w, world.y_axis.w, world.z_axis.w, world.w_axis.w
        );

        let position = extract_translation_from_transform(&world);
        let position = frame_inverse * Point3::from(position).to_homogeneous();
        let position = Vector3::new(position.x, position.y, position.z);

        let rotation = extract_rotation_quat_from_transform(frame).inverse() * extract_rotation_quat_from_transform(&world);

        Matrix4::new_translation(&position) * rotation.to_homogeneous() * Matrix4::new_nonuniform_scaling(scale)
    }

    // Recomputes the cached world matrix of every instance below a node, the node's own
    // included, and marks them for the renderer. Only for a node the solver moved after
    // the scene update ran - everything else is refreshed by Node::update.
    fn refresh_instance_cache_below(node: &NodeItem)
    {
        let (instances, children) =
        {
            let node_read = node.read().unwrap();
            (node_read.instances.get_ref().clone(), node_read.nodes.clone())
        };

        for instance in instances
        {
            let world_matrix = instance.read().unwrap().calculate_transform();
            instance.write().unwrap().get_data_mut().get_mut().computed.world_matrix = world_matrix;
        }

        for child in &children
        {
            Self::refresh_instance_cache_below(child);
        }
    }

    // ********** shapes **********

    // A dynamic body needs volume for mass and inertia, which a trimesh does not have.
    // Auto therefore means trimesh for static and a convex hull for everything else.
    fn effective_shape(body_type: PhysicsBodyType, shape: PhysicsShape) -> PhysicsShape
    {
        if shape != PhysicsShape::Auto
        {
            return shape;
        }

        match body_type
        {
            PhysicsBodyType::Static => PhysicsShape::TriMesh,
            _ => PhysicsShape::ConvexHull
        }
    }

    // A shape is rebuilt whenever the scale changes, so a plain warning would repeat for
    // the same object frame after frame and bury everything else in the console.
    fn warn_once(node_id: u32, message: String)
    {
        static WARNED: std::sync::OnceLock<std::sync::Mutex<HashSet<u32>>> = std::sync::OnceLock::new();

        let warned = WARNED.get_or_init(|| std::sync::Mutex::new(HashSet::new()));

        if let Ok(mut warned) = warned.lock()
        {
            if !warned.insert(node_id)
            {
                return;
            }
        }

        console_warning!("{}", message);
    }

    // The geometry of one mesh, with a transform applied to it. Colliders cannot be scaled,
    // so anything the scene graph does to the vertices has to be baked in here.
    fn collect_geometry(node: &NodeItem, transform: &Matrix4<f32>) -> Option<(Vec<Vector>, Vec<[u32; 3]>)>
    {
        let node_read = node.read().unwrap();
        let mesh = node_read.find_component::<Mesh>()?;

        component_downcast!(mesh, Mesh);

        let mesh_resource = mesh.mesh_resource.as_ref()?;
        let mesh_resource = mesh_resource.read().unwrap();
        let data = mesh_resource.get_data();

        if data.vertices.is_empty() || data.indices.is_empty()
        {
            return None;
        }

        // a skinned mesh is also placed by its joints - rest pose, so an animation never rebuilds the shape
        let joint_matrices = node_read.get_joint_transform_vec(false).filter(|_| data.joints.len() >= data.vertices.len() && data.weights.len() >= data.vertices.len());

        let vertices: Vec<Vector> = data.vertices.iter().enumerate().map(|(v_i, v)|
        {
            let mut position = v.to_homogeneous();

            if let Some(joint_matrices) = joint_matrices.as_ref()
            {
                position = Self::skin_position(&position, &data.joints[v_i], &data.weights[v_i], joint_matrices).unwrap_or(position);
            }

            let moved = transform * position;
            Vector::new(moved.x, moved.y, moved.z)
        }).collect();

        Some((vertices, data.indices.clone()))
    }

    // Weighted joint transform of one vertex, None for a vertex no joint moves.
    fn skin_position(position: &Vector4<f32>, joints: &[u32], weights: &[f32], joint_matrices: &Vec<Matrix4<f32>>) -> Option<Vector4<f32>>
    {
        let mut skinned = Vector4::<f32>::zeros();

        for (joint, weight) in joints.iter().zip(weights.iter())
        {
            let Some(joint_matrix) = joint_matrices.get(*joint as usize) else { continue; };

            if *weight > 0.0
            {
                skinned += joint_matrix * position * *weight;
            }
        }

        if skinned.w <= 0.0
        {
            return None;
        }

        Some(skinned / skinned.w)
    }

    fn resolved_shape_kind(node: &NodeItem) -> PhysicsShape
    {
        let node = node.read().unwrap();
        let physics = node.resolve_physics();

        Self::effective_shape(physics.body_type, physics.shape)
    }

    // reads the node mesh and bakes the scale into the shape
    fn build_shape(node: &NodeItem, scale: &Vector3<f32>) -> Option<SharedShape>
    {
        let shape_kind = Self::resolved_shape_kind(node);

        let scaling = Matrix4::new_nonuniform_scaling(scale);
        let (vertices, indices) = Self::collect_geometry(node, &scaling)?;

        let (name, node_id) =
        {
            let node = node.read().unwrap();
            (node.name.clone(), node.id)
        };

        Self::shape_from_geometry(shape_kind, vertices, indices, &name, node_id)
    }

    // One shape from one set of vertices. Everything that turns geometry into a collider goes
    // through here.
    fn shape_from_geometry(shape_kind: PhysicsShape, vertices: Vec<Vector>, indices: Vec<[u32; 3]>, name: &str, node_id: u32) -> Option<SharedShape>
    {
        if vertices.is_empty() || indices.is_empty()
        {
            return None;
        }

        let aabb = Aabb::from_points(vertices.iter().copied());
        let centre = aabb.center();

        // A flat mesh has a zero extent on one axis, and a primitive built from that has no
        // volume at all. A dynamic body without volume has no mass either, and the solver
        // answers a near zero mass with enormous accelerations - the object shoots off and
        // its transform ends up as NaN. So the thickness is filled in relative to the size
        // of the object, which keeps the mass in a believable range.
        let half = aabb.half_extents();
        let minimum = (half.x.max(half.y).max(half.z) * MIN_SHAPE_THICKNESS_RATIO).max(MIN_SHAPE_HALF_EXTENT);
        let half = Vector::new(half.x.max(minimum), half.y.max(minimum), half.z.max(minimum));

        // the primitives are centred on the mesh aabb, not on the node origin
        let centred = |shape: SharedShape| -> SharedShape
        {
            SharedShape::compound(vec![(Pose::from_translation(centre), shape)])
        };

        let shape = match shape_kind
        {
            PhysicsShape::TriMesh | PhysicsShape::Auto =>
            {
                SharedShape::trimesh(vertices, indices).ok()?
            }
            PhysicsShape::ConvexHull =>
            {
                match SharedShape::convex_hull(&vertices)
                {
                    Some(shape) => shape,
                    None =>
                    {
                        Self::warn_once(node_id, format!("physics: convex hull failed for '{}', falling back to a box", name));
                        centred(SharedShape::cuboid(half.x, half.y, half.z))
                    }
                }
            }
            PhysicsShape::ConvexDecomposition =>
            {
                SharedShape::convex_decomposition(&vertices, &indices)
            }
            PhysicsShape::Box => centred(SharedShape::cuboid(half.x.max(0.001), half.y.max(0.001), half.z.max(0.001))),
            PhysicsShape::Sphere => centred(SharedShape::ball(half.max_element().max(0.001))),
            PhysicsShape::Capsule =>
            {
                let radius = half.x.max(half.z).max(0.001);
                let half_height = (half.y - radius).max(0.001);

                centred(SharedShape::capsule_y(half_height, radius))
            }
        };

        Some(shape)
    }

    // pushes the collider aabb into the bvh - this is what makes it visible to queries
    fn refresh_leaf(&mut self, handle: ColliderHandle)
    {
        if let Some(collider) = self.colliders.get(handle)
        {
            let aabb = collider.compute_aabb();
            self.broad_phase_bvh.set_aabb(&self.integration_params, handle, aabb);
        }
    }

    // ********** building **********

    // The node whose object a mesh belongs to: the first non-static node from the mesh
    // upwards, the mesh itself included, provided that node combines its children. None
    // means the mesh is an object of its own. A mesh that sets its own non-static body
    // type therefore breaks out of a combined parent, like it already overrides the
    // parent's settings.
    fn compound_root(node: &NodeItem) -> Option<NodeItem>
    {
        let mut current = Some(node.clone());

        while let Some(candidate) = current
        {
            let candidate_read = candidate.read().unwrap();
            let physics = &candidate_read.settings.physics;

            if physics.body_type != PhysicsBodyType::Static
            {
                return if physics.combine_children { Some(candidate.clone()) } else { None };
            }

            current = candidate_read.parent.as_ref().cloned();
        }

        None
    }

    // Where a part sits relative to its anchor, the anchor's world scale included: the
    // body carries the rigid pose only, so the scale has to go into the parts. For an
    // instance anchor the part is the anchor, so only the scale is left. Below a node the
    // offset is built from the local transforms down to the part rather than from two
    // world transforms - that is exact, and it only changes when the author edits
    // something below the node, never while the solver moves the whole object.
    fn part_local_transform(anchor: &Anchor, anchor_scale: &Vector3<f32>, node: &NodeItem, instance: &InstanceItemArc) -> Matrix4<f32>
    {
        let scaling = Matrix4::new_nonuniform_scaling(anchor_scale);

        let Anchor::Node { node: root } = anchor else { return scaling; };
        let root_id = root.read().unwrap().id;

        let mut chain = Matrix4::<f32>::identity();
        let mut current = Some(node.clone());

        while let Some(candidate) = current
        {
            let candidate_read = candidate.read().unwrap();

            if candidate_read.id == root_id
            {
                break;
            }

            let (local, _) = candidate_read.get_transform();
            chain = local * chain;

            current = candidate_read.parent.as_ref().cloned();
        }

        let instance_local = instance.read().unwrap().find_component::<Transformation>().map(|transformation|
        {
            component_downcast!(transformation, Transformation);
            *transformation.get_transform()
        }).unwrap_or_else(Matrix4::identity);

        scaling * chain * instance_local
    }

    // A dynamic or kinematic body at the given pose, with the authored start velocities
    // and the world's sleep settings. Counts towards the body amount.
    fn insert_body(&mut self, physics: &PhysicsSettings, pose: Pose) -> RigidBodyHandle
    {
        let builder = match physics.body_type
        {
            // waits for a hit: fixed until then, the release makes it dynamic
            PhysicsBodyType::Dynamic if physics.react_on_first_hit => RigidBodyBuilder::fixed(),
            PhysicsBodyType::Dynamic => RigidBodyBuilder::dynamic()
                .linvel(Vector::new(physics.linear_velocity.x, physics.linear_velocity.y, physics.linear_velocity.z))
                .angvel(Vector::new(physics.angular_velocity.x, physics.angular_velocity.y, physics.angular_velocity.z)),
            _ => RigidBodyBuilder::kinematic_position_based()
        };

        let handle = self.bodies.insert(builder.pose(pose).build());

        let (linear, angular, time_until_sleep) = self.configured_sleep();

        if let Some(body) = self.bodies.get_mut(handle)
        {
            let activation = body.activation_mut();

            activation.normalized_linear_threshold = linear;
            activation.angular_threshold = angular;
            activation.time_until_sleep = time_until_sleep;
        }

        Self::refresh_body_settings(&mut self.bodies, handle, physics);

        self.body_amount += 1;

        handle
    }

    // The body level settings, the one place they are set - at creation and on every
    // change in the inspector. Damping for now; axis locks belong here too.
    fn refresh_body_settings(bodies: &mut RigidBodySet, handle: RigidBodyHandle, physics: &PhysicsSettings)
    {
        let Some(body) = bodies.get_mut(handle) else { return; };

        let linear_damping = physics.linear_damping.max(0.0);
        let angular_damping = physics.angular_damping.max(0.0);

        if (body.linear_damping() - linear_damping).abs() > 0.0001
        {
            body.set_linear_damping(linear_damping);
        }

        if (body.angular_damping() - angular_damping).abs() > 0.0001
        {
            body.set_angular_damping(angular_damping);
        }
    }

    // Mass and inertia from the shape, the centre moved to the anchor-space point the
    // author set, expressed in the part's own frame. Every part gets that same point, so
    // the mass weighted average rapier takes over the parts lands exactly there. Asking
    // the user for an inertia tensor instead would help nobody.
    fn part_mass_properties(shape: &dyn Shape, density: f32, center_of_mass: &Vector3<f32>, offset: &Pose) -> MassProperties
    {
        let mut mass_properties = shape.mass_properties(density);
        mass_properties.local_com = offset.inverse_transform_point(Vector::new(center_of_mass.x, center_of_mass.y, center_of_mass.z));

        mass_properties
    }

    // The collider for one part. Attached to a body it is placed by its offset, standing
    // alone it needs the world pose.
    fn part_collider(shape: SharedShape, offset: &Pose, anchor_pose: &Pose, physics: &PhysicsSettings, node_id: u32, instance_id: u32, attached: bool, report_contacts: bool) -> Collider
    {
        let density = physics.density.max(0.001);

        let mass_properties = if physics.center_of_mass_auto
        {
            None
        }
        else
        {
            Some(Self::part_mass_properties(&*shape, density, &physics.center_of_mass, offset))
        };

        let position = if attached { *offset } else { *anchor_pose * *offset };

        // A waiting object is a fixed body, and a moving kinematic one has to be able to
        // release it - rapier computes no contacts between kinematic and fixed bodies
        // unless asked to. Free while the body is dynamic, the flag only matters by type.
        let collision_types = if physics.body_type == PhysicsBodyType::Dynamic
        {
            ActiveCollisionTypes::default() | ActiveCollisionTypes::KINEMATIC_FIXED
        }
        else
        {
            ActiveCollisionTypes::default()
        };

        let collider = ColliderBuilder::new(shape)
            .user_data(Self::pack_user_data(node_id, instance_id))
            .density(density)
            .friction(physics.friction.max(0.0))
            .restitution(physics.restitution.clamp(0.0, 1.0))
            .active_collision_types(collision_types)
            .active_events(Self::contact_events_flag(report_contacts))
            .position(position);

        match mass_properties
        {
            Some(mass_properties) => collider.mass_properties(mass_properties).build(),
            None => collider.build()
        }
    }

    fn pack_user_data(node_id: u32, instance_id: u32) -> u128
    {
        (node_id as u128) | ((instance_id as u128) << 32)
    }

    fn contact_events_flag(report_contacts: bool) -> ActiveEvents
    {
        if report_contacts { ActiveEvents::COLLISION_EVENTS } else { ActiveEvents::empty() }
    }

    // set on the node or anywhere above it
    fn reports_contacts(node: &NodeItem) -> bool
    {
        let mut current = Some(node.clone());

        while let Some(item) = current
        {
            let item = item.read().unwrap();

            if item.settings.physics.report_contacts
            {
                return true;
            }

            current = item.parent.as_ref().cloned();
        }

        false
    }

    // Builds the colliders and, unless static, the body for one object. Returns false when
    // not a single part had usable geometry - there is nothing to simulate then.
    fn build_entry(&mut self, anchor: Anchor, parts: Vec<(NodeItem, InstanceItemArc)>) -> bool
    {
        let key = anchor.key();
        let physics = anchor.physics();

        let anchor_world = anchor.world_transform_live();
        let (anchor_pose, anchor_scale) = Self::split_transform(&anchor_world);

        let body = match physics.body_type
        {
            PhysicsBodyType::Static => None,
            _ => Some(self.insert_body(&physics, anchor_pose))
        };

        let mut built: Vec<Part> = vec![];
        let mut requested_parts: HashSet<(u32, u32)> = HashSet::new();

        for (node, instance) in parts
        {
            let node_id = node.read().unwrap().id;
            let instance_id = instance.read().unwrap().id;

            requested_parts.insert((node_id, instance_id));

            let local = Self::part_local_transform(&anchor, &anchor_scale, &node, &instance);
            let (offset, scale) = Self::split_transform(&local);

            let Some(shape) = Self::build_shape(&node, &scale) else { continue; };

            let collider = Self::part_collider(shape, &offset, &anchor_pose, &physics, node_id, instance_id, body.is_some(), Self::reports_contacts(&node));

            let handle = match body
            {
                Some(body) => self.colliders.insert_with_parent(collider, body, &mut self.bodies),
                None => self.colliders.insert(collider)
            };

            self.refresh_leaf(handle);

            built.push(Part
            {
                shape_kind: Self::resolved_shape_kind(&node),
                node,
                node_id,
                instance,
                instance_id,
                handle,
                local,
                offset,
                scale,
            });
        }

        if built.is_empty()
        {
            if let Some(body) = body
            {
                self.remove_bodies(&vec![body]);
            }

            return false;
        }

        if physics.body_type == PhysicsBodyType::Dynamic
        {
            // A non-uniform scale above the written transform stretches whatever sits below
            // it, and by a different amount for every orientation. A rigid body rotating
            // under one therefore changes shape as it turns, which no write back can undo.
            let frame_scale = extract_scale_from_transform(&anchor.frame());
            let largest = frame_scale.x.max(frame_scale.y).max(frame_scale.z);
            let smallest = frame_scale.x.min(frame_scale.y).min(frame_scale.z);

            if smallest > 0.0 && largest / smallest > NON_UNIFORM_SCALE_LIMIT
            {
                Self::warn_once(key.0, format!("physics: '{}' is dynamic under a non-uniform scale of {:.2}/{:.2}/{:.2} - it will visibly stretch as it rotates. Bake the scale into the mesh to fix it", anchor.name(), frame_scale.x, frame_scale.y, frame_scale.z));
            }

            anchor.ensure_transformation();

            // created while running, so it has to be remembered too
            if self.running && !self.snapshot_pending
            {
                for part in &built
                {
                    self.snapshot_instance(part.node_id, part.instance_id, &part.instance);
                    self.snapshot_node_chain(&part.node);
                }
            }
        }

        self.entries.push(BodyEntry
        {
            anchor,
            key,
            body,
            body_type: physics.body_type,
            reacts_on_hit: physics.body_type == PhysicsBodyType::Dynamic && physics.react_on_first_hit,
            waiting: physics.body_type == PhysicsBodyType::Dynamic && physics.react_on_first_hit,
            frozen: false,
            rest: None,
            settling: false,
            active_time: 0.0,
            step_pose: None,
            parts: built,
            requested_parts,
            transform: anchor_world,
            scale: anchor_scale,
            applied_density: physics.density.max(0.001),
            applied_center_of_mass: if physics.center_of_mass_auto { None } else { Some(physics.center_of_mass) },
        });

        true
    }

    // Adds one mesh instance as an object of its own, unless it is already part of
    // something. Returns its collider handle.
    pub fn add_instance(&mut self, node: NodeItem, instance: InstanceItemArc) -> Option<ColliderHandle>
    {
        let node_id = node.read().unwrap().id;
        let instance_id = instance.read().unwrap().id;

        if self.has_instance(node_id, instance_id) || self.is_excluded(node_id)
        {
            return None;
        }

        let anchor = Anchor::Instance { node: node.clone(), instance: instance.clone() };

        if !self.build_entry(anchor, vec![(node, instance)])
        {
            return None;
        }

        self.entries.last().and_then(|entry| entry.parts.first()).map(|part| part.handle)
    }

    // Adds every collidable instance of a node, each as an object of its own. Returns how
    // many colliders were created.
    pub fn add_node(&mut self, node: NodeItem) -> usize
    {
        let instances: Vec<InstanceItemArc> = node.read().unwrap().instances.get_ref().clone();

        let mut added = 0;

        for instance in instances
        {
            if !Self::is_collidable_instance(&instance)
            {
                continue;
            }

            if self.add_instance(node.clone(), instance).is_some()
            {
                added += 1;
            }
        }

        added
    }

    // Removes every object the node is part of, as its anchor or as one of its meshes. A
    // combined object goes as a whole - the next scan builds it again without the node.
    pub fn remove_node(&mut self, node_id: u32) -> bool
    {
        let mut removed = false;

        for index in (0..self.entries.len()).rev()
        {
            if self.entries[index].has_node(node_id)
            {
                self.remove_entry_at(index);
                removed = true;
            }
        }

        // the arena slots can be reused, so rebuild rather than patch single leaves
        if removed
        {
            self.rebuild_bvh();
        }

        removed
    }

    // Drops an object. A body takes its colliders with it, standalone ones go one by one.
    // The bvh is left to the caller, which usually has more to remove.
    fn remove_entry_at(&mut self, index: usize)
    {
        let entry = self.entries.remove(index);

        // the watches point at entries by index
        self.supports.retain(|watch| watch.held != index && watch.support != index);

        for watch in &mut self.supports
        {
            if watch.held > index { watch.held -= 1; }
            if watch.support > index { watch.support -= 1; }
        }

        match entry.body
        {
            Some(body) => self.remove_bodies(&vec![body]),
            None =>
            {
                for part in &entry.parts
                {
                    self.remove_collider(part.handle);
                }
            }
        }
    }

    fn remove_collider(&mut self, handle: ColliderHandle)
    {
        self.colliders.remove(handle, &mut self.islands, &mut self.bodies, &mut self.soft_bodies, false);
    }

    fn remove_bodies(&mut self, bodies: &Vec<RigidBodyHandle>)
    {
        for body in bodies
        {
            self.bodies.remove(*body, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, &mut self.soft_bodies, true);
            self.body_amount = self.body_amount.saturating_sub(1);
        }
    }

    // Drops every collider and rebuilds from the given nodes. Returns the collider count.
    pub fn build_from_nodes(&mut self, nodes: &Vec<NodeItem>) -> usize
    {
        self.clear();
        self.scan_nodes(nodes);

        self.collider_amount()
    }

    // both checks walk up the parent chain, so an object root disables everything below
    fn is_collidable(node: &NodeItem) -> bool
    {
        let node = node.read().unwrap();

        node.has_collision() && !node.is_engine_internal()
    }

    // The per instance collision flag the editor already exposes.
    fn is_collidable_instance(instance: &InstanceItemArc) -> bool
    {
        instance.read().unwrap().get_data().collision
    }

    // ********** scanning **********

    // Reconciles the objects with the scene. Returns (added, removed) colliders.
    pub fn scan_nodes(&mut self, nodes: &Vec<NodeItem>) -> (usize, usize)
    {
        let all_nodes = Scene::list_all_child_nodes_with_mesh(nodes);

        // everything that should exist right now, by anchor: a mesh below a combining node
        // joins that node's object, everything else is an object of its own
        let mut requests: Vec<Request> = vec![];

        for node in &all_nodes
        {
            if !Self::is_collidable(node)
            {
                continue;
            }

            let node_id = node.read().unwrap().id;

            // a character is never part of anything
            if self.is_excluded(node_id)
            {
                continue;
            }

            let root = Self::compound_root(node);
            let instances: Vec<InstanceItemArc> = node.read().unwrap().instances.get_ref().clone();

            for instance in instances
            {
                if !Self::is_collidable_instance(&instance)
                {
                    continue;
                }

                match &root
                {
                    Some(root) =>
                    {
                        let key = (root.read().unwrap().id, None);

                        match requests.iter_mut().find(|request| request.key == key)
                        {
                            Some(request) => request.parts.push((node.clone(), instance)),
                            None => requests.push(Request { anchor: Anchor::Node { node: root.clone() }, key, parts: vec![(node.clone(), instance)] }),
                        }
                    }
                    None =>
                    {
                        let key = (node_id, Some(instance.read().unwrap().id));
                        let anchor = Anchor::Instance { node: node.clone(), instance: instance.clone() };

                        requests.push(Request { anchor, key, parts: vec![(node.clone(), instance)] });
                    }
                }
            }
        }

        self.reconcile(requests)
    }

    // True on the first call, then every NODE_SCAN_INTERVAL_FRAMES calls.
    pub fn scan_due(&mut self) -> bool
    {
        if self.scan_countdown > 0
        {
            self.scan_countdown -= 1;
            return false;
        }

        // this call is the due one, so only the remaining frames of the interval are counted
        self.scan_countdown = NODE_SCAN_INTERVAL_FRAMES.saturating_sub(1);

        true
    }

    // Drops what is gone or built differently from what the scene asks for now, adjusts
    // the rest in place, and builds what is missing. An object is rebuilt when its parts,
    // its body type or its shape kind changed - switching a crate to dynamic in the editor
    // has to actually rebuild it, and a body that lost a collider would otherwise keep the
    // mass of the missing part. Returns (added, removed) colliders.
    fn reconcile(&mut self, requests: Vec<Request>) -> (usize, usize)
    {
        let mut removed = 0;
        let mut kept: HashSet<AnchorKey> = HashSet::new();

        // Back to front, a removal shifts the indices. And removal before building: a mesh
        // that just joined a combined object must not exist twice.
        for index in (0..self.entries.len()).rev()
        {
            let key = self.entries[index].key;

            let outdated = match requests.iter().find(|request| request.key == key)
            {
                None => true,
                Some(request) => self.entry_outdated(index, request),
            };

            if outdated
            {
                removed += self.entries[index].parts.len();
                self.remove_entry_at(index);
            }
            else
            {
                kept.insert(key);
                self.refresh_entry_settings(index);
            }
        }

        let mut added = 0;

        for request in requests
        {
            if kept.contains(&request.key)
            {
                continue;
            }

            if self.build_entry(request.anchor, request.parts)
            {
                added += self.entries.last().map(|entry| entry.parts.len()).unwrap_or(0);
            }
        }

        if removed > 0 || added > 0
        {
            self.rebuild_bvh();
        }

        (added, removed)
    }

    fn entry_outdated(&self, index: usize, request: &Request) -> bool
    {
        let entry = &self.entries[index];

        if entry.anchor.physics().body_type != entry.body_type
        {
            return true;
        }

        let requested: HashSet<(u32, u32)> = request.parts.iter()
            .map(|(node, instance)| (node.read().unwrap().id, instance.read().unwrap().id))
            .collect();

        if requested != entry.requested_parts
        {
            return true;
        }

        // the shape kind is resolved per part, like everything else
        entry.parts.iter().any(|part| part.shape_kind != Self::resolved_shape_kind(&part.node))
    }

    // Everything the inspector can change on an object that does not need a new shape:
    // friction, restitution, mass and the body settings. Body type and shape are handled
    // by the reconcile instead, those do.
    fn refresh_entry_settings(&mut self, index: usize)
    {
        let physics = self.entries[index].anchor.physics();

        let density = physics.density.max(0.001);
        let center_of_mass = if physics.center_of_mass_auto { None } else { Some(physics.center_of_mass) };

        let mass_changed =
            (self.entries[index].applied_density - density).abs() > DENSITY_EPSILON
            || !Self::center_of_mass_matches(&self.entries[index].applied_center_of_mass, &center_of_mass);

        for part_index in 0..self.entries[index].parts.len()
        {
            let handle = self.entries[index].parts[part_index].handle;
            let offset = self.entries[index].parts[part_index].offset;
            let active_events = Self::contact_events_flag(Self::reports_contacts(&self.entries[index].parts[part_index].node));

            let Some(collider) = self.colliders.get_mut(handle) else { continue; };

            if collider.active_events() != active_events
            {
                collider.set_active_events(active_events);
            }

            if (collider.friction() - physics.friction).abs() > 0.0001
            {
                collider.set_friction(physics.friction.max(0.0));
            }

            if (collider.restitution() - physics.restitution).abs() > 0.0001
            {
                collider.set_restitution(physics.restitution.clamp(0.0, 1.0));
            }

            if mass_changed
            {
                match center_of_mass
                {
                    // the shape works out the centre itself, plain density is enough
                    None => collider.set_density(density),

                    // keep the mass and inertia the shape computes, only move the centre
                    Some(center_of_mass) =>
                    {
                        let mass_properties = Self::part_mass_properties(collider.shape(), density, &center_of_mass, &offset);
                        collider.set_mass_properties(mass_properties);
                    }
                }
            }
        }

        if mass_changed
        {
            self.entries[index].applied_density = density;
            self.entries[index].applied_center_of_mass = center_of_mass;
        }

        if let Some(body) = self.entries[index].body
        {
            Self::refresh_body_settings(&mut self.bodies, body, &physics);
            self.entries[index].settling = false;
        }

        let reacts_on_hit = physics.body_type == PhysicsBodyType::Dynamic && physics.react_on_first_hit;

        if reacts_on_hit != self.entries[index].reacts_on_hit
        {
            self.entries[index].reacts_on_hit = reacts_on_hit;
            self.set_waiting(index, reacts_on_hit);
        }
    }

    fn center_of_mass_matches(a: &Option<Vector3<f32>>, b: &Option<Vector3<f32>>) -> bool
    {
        match (a, b)
        {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).norm() <= CENTER_OF_MASS_EPSILON,
            _ => false
        }
    }

    // Every run starts from the authored values, even when the recurring node scan that
    // would otherwise pick them up is turned off.
    fn refresh_all_entry_settings(&mut self)
    {
        for index in 0..self.entries.len()
        {
            self.refresh_entry_settings(index);
        }
    }

    // ********** waiting for a hit **********

    // An object that reacts on its first hit is built as a fixed body and stays one until
    // something releases it: a dynamic body that arrives faster than the hit speed, a
    // kinematic body the scene moves into it, the character walking into it, or a waiting
    // neighbour it touches being released. What it rests on from the start therefore does
    // not count - a fixed body gets no contacts with static geometry or with other waiting
    // objects, and a dynamic object resting on it has no speed to count as a hit.

    fn set_waiting(&mut self, index: usize, waiting: bool)
    {
        if self.entries[index].body_type != PhysicsBodyType::Dynamic
        {
            return;
        }

        self.entries[index].waiting = waiting;
        self.entries[index].active_time = 0.0;
        self.apply_hold(index);
    }

    fn set_frozen(&mut self, index: usize, frozen: bool)
    {
        if self.entries[index].body_type != PhysicsBodyType::Dynamic
        {
            return;
        }

        self.entries[index].frozen = frozen;
        self.entries[index].rest = None;
        self.entries[index].active_time = 0.0;
        self.set_settling(index, false);
        self.apply_hold(index);
    }

    // a waiting or frozen object is a fixed body, anything else dynamic
    fn apply_hold(&mut self, index: usize)
    {
        let held = self.entries[index].waiting || self.entries[index].frozen;

        let Some(handle) = self.entries[index].body else { return; };
        let Some(body) = self.bodies.get_mut(handle) else { return; };

        let body_type = if held { RigidBodyType::Fixed } else { RigidBodyType::Dynamic };

        if body.body_type() == body_type
        {
            return;
        }

        body.set_body_type(body_type, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);

        if !held
        {
            body.recompute_mass_properties_from_colliders(&self.colliders);
        }
    }

    // Releases an object and everything waiting that touches it, and everything touching
    // those. A hit on one shard of a fractured object brings the whole thing down that way,
    // instead of leaving the rest hanging in the air on a piece that is no longer there.
    fn release_entry(&mut self, index: usize)
    {
        if !self.entries[index].waiting
        {
            return;
        }

        // the colliders of everything waiting, with a cheap bound each
        let waiting: Vec<Vec<(ColliderHandle, Aabb)>> = self.entries.iter().map(|entry|
        {
            if !entry.waiting
            {
                return vec![];
            }

            entry.parts.iter()
                .filter_map(|part| self.colliders.get(part.handle).map(|collider| (part.handle, collider.compute_aabb())))
                .collect()
        }).collect();

        let mut pending = vec![index];

        while let Some(index) = pending.pop()
        {
            if !self.entries[index].waiting
            {
                continue;
            }

            self.set_waiting(index, false);

            for other in 0..self.entries.len()
            {
                if other == index || !self.entries[other].waiting || pending.contains(&other)
                {
                    continue;
                }

                if self.colliders_touch(&waiting[index], &waiting[other])
                {
                    pending.push(other);
                }
            }
        }
    }

    // Exact where it matters: dynamic objects are convex hulls or primitives, which parry
    // measures the distance between directly. Anything it cannot measure counts as touching
    // once the bounds overlap.
    fn colliders_touch(&self, a: &Vec<(ColliderHandle, Aabb)>, b: &Vec<(ColliderHandle, Aabb)>) -> bool
    {
        for (handle_a, aabb_a) in a
        {
            let aabb_a = aabb_a.loosened(RELEASE_TOUCH_DISTANCE);

            for (handle_b, aabb_b) in b
            {
                if !aabb_a.intersects(aabb_b)
                {
                    continue;
                }

                let (Some(collider_a), Some(collider_b)) = (self.colliders.get(*handle_a), self.colliders.get(*handle_b)) else { continue; };

                match parry3d::query::distance(collider_a.position(), collider_a.shape(), collider_b.position(), collider_b.shape())
                {
                    Ok(distance) => if distance.distance <= RELEASE_TOUCH_DISTANCE { return true; },
                    Err(_) => return true,
                }
            }
        }

        false
    }

    fn record_speeds(&mut self)
    {
        self.pre_step_speed.clear();

        for entry in &self.entries
        {
            if entry.body_type != PhysicsBodyType::Dynamic || entry.waiting || entry.frozen
            {
                continue;
            }

            let Some(handle) = entry.body else { continue; };
            let Some(body) = self.bodies.get(handle) else { continue; };

            self.pre_step_speed.insert(handle, body.linvel().length());
        }

        // a vehicle ramming a waiting object is a hit like any other
        for vehicle in self.vehicles.values()
        {
            let Some(body) = self.bodies.get(vehicle.body) else { continue; };

            self.pre_step_speed.insert(vehicle.body, body.linvel().length());
        }
    }

    // Hits the solver saw: a contact with a dynamic body that arrived faster than the hit
    // speed, or with a kinematic body the scene moved this frame - a fixed body does not
    // get out of a platform's way by itself.
    fn release_hit_bodies(&mut self)
    {
        if self.run_steps < HIT_GRACE_STEPS
        {
            return;
        }

        let hit_speed = self.settings.hit_speed.max(0.0);
        let wake_speed = self.settings.wake_speed.max(0.0);
        let mut hit: Vec<usize> = vec![];

        // they still release a waiting object, but no longer thaw a frozen one
        let overdue: HashSet<RigidBodyHandle> = self.entries.iter()
            .filter(|entry| self.is_overdue(entry))
            .filter_map(|entry| entry.body)
            .collect();

        for index in 0..self.entries.len()
        {
            if !self.entries[index].waiting && !self.entries[index].frozen
            {
                continue;
            }

            let waiting = self.entries[index].waiting;
            let threshold = if waiting { hit_speed } else { wake_speed };

            let was_hit = self.entries[index].parts.iter().any(|part|
            {
                self.narrow_phase.contact_pairs_with(part.handle).any(|pair|
                {
                    if !pair.has_any_active_contact()
                    {
                        return false;
                    }

                    let other = if pair.collider1 == part.handle { pair.collider2 } else { pair.collider1 };

                    let Some(other_body) = self.colliders.get(other).and_then(|collider| collider.parent()) else { return false; };
                    let Some(body) = self.bodies.get(other_body) else { return false; };

                    if body.is_dynamic()
                    {
                        (waiting || !overdue.contains(&other_body)) && self.pre_step_speed.get(&other_body).copied().unwrap_or(0.0) > threshold
                    }
                    else if body.is_kinematic()
                    {
                        self.moved_kinematics.contains(&other_body)
                    }
                    else
                    {
                        false
                    }
                })
            });

            if was_hit
            {
                hit.push(index);
            }
        }

        for index in hit
        {
            self.release(index);
        }
    }

    fn release(&mut self, index: usize)
    {
        if self.entries[index].waiting
        {
            self.release_entry(index);
        }
        else if self.entries[index].frozen
        {
            self.thaw_entry(index);
        }
    }

    // A hit the narrow phase never sees: the character is a shape cast, not a body.
    // approach_speed: how fast the character moves into it. Anything waiting counts at any
    // speed, a frozen piece only if walked into - not the one under its feet.
    pub fn hit_collider(&mut self, handle: ColliderHandle, approach_speed: f32)
    {
        let Some(index) = self.entries.iter().position(|entry| (entry.waiting || entry.frozen) && entry.parts.iter().any(|part| part.handle == handle)) else { return; };

        if self.entries[index].frozen && approach_speed <= self.settings.wake_speed
        {
            return;
        }

        self.release(index);
    }

    // Every run starts as authored: what reacts on a hit waits, everything else runs.
    fn reset_waiting(&mut self)
    {
        self.supports.clear();

        for index in 0..self.entries.len()
        {
            self.entries[index].frozen = false;
            self.entries[index].rest = None;
            self.set_settling(index, false);

            let waiting = self.entries[index].reacts_on_hit;
            self.set_waiting(index, waiting);
        }
    }

    pub fn frozen_amount(&self) -> usize
    {
        self.entries.iter().filter(|entry| entry.frozen).count()
    }

    // dynamic objects the solver actually simulates right now
    pub fn awake_amount(&self) -> usize
    {
        self.entries.iter().filter(|entry|
        {
            entry.body_type == PhysicsBodyType::Dynamic && !entry.waiting && !entry.frozen
                && entry.body.and_then(|handle| self.bodies.get(handle)).is_some_and(|body| !body.is_sleeping())
        }).count()
    }

    // ********** freezing resting bodies **********

    // Rapier sleeps a group of touching bodies only once every one of them rests at the same
    // moment, and a fallen pile of dominoes or rubble is one such group that rarely gets
    // there. Rapier still tracks per body how long it has been resting, so a body that has
    // rested long enough is made fixed here, and released again like a waiting object.
    // Tracks where every awake body has been, damps the ones that stay in place and freezes
    // the ones that rest or keep wobbling there.
    fn settle_bodies(&mut self, elapsed: f32)
    {
        // switched off mid run: everything frozen goes back to the solver
        if !self.settings.freeze_resting
        {
            for index in 0..self.entries.len()
            {
                if self.entries[index].frozen
                {
                    self.set_frozen(index, false);
                }
            }
        }

        if self.run_steps < HIT_GRACE_STEPS
        {
            return;
        }

        let freeze_after = self.settings.freeze_after.max(0.0);
        let wake_speed = self.settings.wake_speed.max(0.0);
        let damp = self.settings.settle_damping > 0.0;
        let carried = self.carried_by_vehicles();

        for index in 0..self.entries.len()
        {
            let entry = &self.entries[index];

            if entry.body_type != PhysicsBodyType::Dynamic || entry.waiting || entry.frozen
            {
                continue;
            }

            let Some(body) = entry.body.and_then(|handle| self.bodies.get(handle)) else { continue; };

            // a sleeping body costs nothing already, and a load must stay free - frozen it would hold its vehicle like a wall
            if body.is_sleeping() || carried.contains(&index)
            {
                self.entries[index].rest = None;
                self.entries[index].active_time = 0.0;
                self.set_settling(index, false);
                continue;
            }

            self.entries[index].active_time += elapsed;
            let overdue = self.is_overdue(&self.entries[index]);

            let pose = *body.position();
            let resting = body.activation().time_since_can_sleep >= freeze_after;

            // really on its way somewhere, e.g. just hit, whatever it did before - once overdue only the distance counts
            let fast = !overdue && body.linvel().length() > wake_speed;
            let (drift, angle) = if overdue { (SETTLE_DRIFT, f32::INFINITY) } else { (WOBBLE_DRIFT, WOBBLE_ANGLE) };

            let rest_time = match self.entries[index].rest
            {
                Some(mut rest) if !fast && Self::stays_put(&rest.start, &pose, drift, angle) =>
                {
                    rest.time += elapsed;
                    self.entries[index].rest = Some(rest);
                    rest.time
                }
                _ =>
                {
                    self.entries[index].rest = Some(RestTrack { start: pose, time: 0.0 });
                    0.0
                }
            };

            // not in mid air, a thrown object keeps flying
            let calmed = overdue && rest_time >= SETTLE_WINDOW && self.touches_anything(index);

            if self.settings.freeze_resting && (resting || rest_time >= WOBBLE_TIME || calmed)
            {
                self.set_frozen(index, true);
                continue;
            }

            self.set_settling(index, damp && rest_time >= SETTLE_DELAY);
        }
    }

    // the settle damping comes on top of the object's own, which is read back when it ends
    fn set_settling(&mut self, index: usize, settling: bool)
    {
        if self.entries[index].settling == settling
        {
            return;
        }

        self.entries[index].settling = settling;

        let Some(handle) = self.entries[index].body else { return; };
        let physics = self.entries[index].anchor.physics();
        let extra = if settling { self.settings.settle_damping.max(0.0) } else { 0.0 };

        if let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_linear_damping(physics.linear_damping.max(0.0) + extra);
            body.set_angular_damping(physics.angular_damping.max(0.0) + extra);
        }
    }

    // Position and angle apart: at the far edge of a 2 m slab a rocking of 1.5° is already 3.5 cm.
    fn stays_put(start: &Pose, pose: &Pose, max_drift: f32, max_angle: f32) -> bool
    {
        let delta = pose.rotation * start.rotation.inverse();
        let angle = 2.0 * Vector::new(delta.x, delta.y, delta.z).length().min(1.0).asin();

        (pose.translation - start.translation).length() <= max_drift && angle <= max_angle
    }

    // moving longer than the settle time since it was last set moving - it only gets frozen, it no longer wakes others
    fn is_overdue(&self, entry: &BodyEntry) -> bool
    {
        self.settings.settle_time > 0.0 && entry.active_time >= self.settings.settle_time
    }

    // the entries lying on a vehicle, directly or on another load (a car on a truck on a car carrier)
    fn carried_by_vehicles(&self) -> HashSet<usize>
    {
        let mut carried = HashSet::new();

        if self.vehicles.is_empty()
        {
            return carried;
        }

        // built on the first touch of a dynamic body only - a car on its own never pays for it
        let mut entry_of: Option<HashMap<ColliderHandle, usize>> = None;

        let mut layer: Vec<ColliderHandle> = self.vehicles.values().map(|vehicle| vehicle.collider).collect();

        for _ in 0..CARRY_DEPTH
        {
            let mut next = vec![];

            for handle in layer
            {
                for pair in self.narrow_phase.contact_pairs_with(handle)
                {
                    if !pair.has_any_active_contact()
                    {
                        continue;
                    }

                    let other = if pair.collider1 == handle { pair.collider2 } else { pair.collider1 };

                    if !self.colliders.get(other).and_then(|collider| collider.parent()).and_then(|body| self.bodies.get(body)).is_some_and(|body| body.is_dynamic())
                    {
                        continue;
                    }

                    let entry_of = entry_of.get_or_insert_with(|| self.entries.iter().enumerate()
                        .filter(|(_, entry)| entry.body_type == PhysicsBodyType::Dynamic)
                        .flat_map(|(index, entry)| entry.parts.iter().map(move |part| (part.handle, index)))
                        .collect());

                    if let Some(&index) = entry_of.get(&other) && carried.insert(index)
                    {
                        next.extend(self.entries[index].parts.iter().map(|part| part.handle));
                    }
                }
            }

            if next.is_empty()
            {
                break;
            }

            layer = next;
        }

        carried
    }

    fn touches_anything(&self, index: usize) -> bool
    {
        self.entries[index].parts.iter().any(|part| self.narrow_phase.contact_pairs_with(part.handle).any(|pair| pair.has_any_active_contact()))
    }

    // Frozen objects in the way of something moving are thawed before the step, not after
    // it: a fixed body is infinitely heavy, and a thaw after the contact comes too late -
    // the mover has already bounced off it like off a wall.
    fn thaw_ahead_of_movers(&mut self, lookahead: f32)
    {
        // a waiting object lets go before a fast mover arrives too, it would stop it dead like a wall otherwise
        let release_waiting = self.run_steps >= HIT_GRACE_STEPS;

        if !self.entries.iter().any(|entry| entry.frozen || (release_waiting && entry.waiting))
        {
            return;
        }

        let wake_speed = self.settings.wake_speed.max(0.0);
        let hit_speed = self.settings.hit_speed.max(0.0);

        // a collider moving faster than the wake speed, with how far it can get this frame
        let reach = |body: &RigidBody, collider: &Collider| -> Option<f32>
        {
            let radius = collider.compute_aabb().half_extents().length();
            let speed = body.linvel().length() + body.angvel().length() * radius;

            (speed > wake_speed).then_some(speed * lookahead + RELEASE_TOUCH_DISTANCE)
        };

        let mut movers: Vec<(ColliderHandle, RigidBodyHandle, f32)> = vec![];

        for entry in &self.entries
        {
            if entry.body_type != PhysicsBodyType::Dynamic || entry.waiting || entry.frozen
            {
                continue;
            }

            // fast but going nowhere, it would only pass its wobble on to what it touches
            if entry.rest.is_some_and(|rest| rest.time >= WOBBLE_MOVER_TIME) || self.is_overdue(entry)
            {
                continue;
            }

            let Some(body_handle) = entry.body else { continue; };
            let Some(body) = self.bodies.get(body_handle) else { continue; };

            if body.is_sleeping()
            {
                continue;
            }

            for part in &entry.parts
            {
                let Some(collider) = self.colliders.get(part.handle) else { continue; };

                if let Some(distance) = reach(body, collider)
                {
                    movers.push((part.handle, body_handle, distance));
                }
            }
        }

        for vehicle in self.vehicles.values()
        {
            let (Some(body), Some(collider)) = (self.bodies.get(vehicle.body), self.colliders.get(vehicle.collider)) else { continue; };

            if let Some(distance) = reach(body, collider)
            {
                movers.push((vehicle.collider, vehicle.body, distance));
            }
        }

        if movers.is_empty()
        {
            return;
        }

        let frozen_parts: HashMap<ColliderHandle, usize> = self.entries.iter().enumerate()
            .filter(|(_, entry)| entry.frozen || (release_waiting && entry.waiting))
            .flat_map(|(index, entry)| entry.parts.iter().map(move |part| (part.handle, index)))
            .collect();

        let mut thaw: HashSet<usize> = HashSet::new();

        {
            let query = self.query_pipeline(QueryFilter::only_fixed());

            for (handle, body, distance) in &movers
            {
                let (Some(collider), Some(body)) = (self.colliders.get(*handle), self.bodies.get(*body)) else { continue; };
                let aabb = collider.compute_aabb().loosened(*distance);

                for (other, other_collider) in query.intersect_aabb_conservative(aabb)
                {
                    let Some(&index) = frozen_parts.get(&other) else { continue; };

                    if thaw.contains(&index)
                    {
                        continue;
                    }

                    // only a mover heading into it counts - one leaning on it and rocking does not
                    let threshold = if self.entries[index].waiting { hit_speed } else { wake_speed };
                    let approaching = match parry3d::query::contact(collider.position(), collider.shape(), other_collider.position(), other_collider.shape(), *distance)
                    {
                        Ok(Some(contact)) => body.velocity_at_point(contact.point1).dot(contact.normal1) > threshold,
                        Ok(None) => false,
                        Err(_) => true,
                    };

                    if approaching
                    {
                        thaw.insert(index);
                    }
                }
            }
        }

        for index in thaw
        {
            self.release(index);
        }
    }

    // Thaws a frozen object and the layer on it, the layers above follow once their support moves - flooding a whole heap woke hundreds of pieces, measured.
    fn thaw_entry(&mut self, index: usize)
    {
        if !self.entries[index].frozen
        {
            return;
        }

        let above = self.frozen_above(index);

        self.set_frozen(index, false);

        for other in above
        {
            self.thaw_alone(other);
        }
    }

    // Thaws one object and watches the frozen ones resting on it, see release_unsupported.
    fn thaw_alone(&mut self, index: usize)
    {
        if !self.entries[index].frozen
        {
            return;
        }

        let above = self.frozen_above(index);

        self.set_frozen(index, false);

        let Some(start) = self.entries[index].body.and_then(|handle| self.bodies.get(handle)).map(|body| *body.position()) else { return; };

        self.supports.extend(above.into_iter().map(|held| SupportWatch { held, support: index, start }));
    }

    // A frozen body is fixed in place and would hang in the air once what it rests on is gone, so it goes along once the two come apart - rocking in contact does not count.
    fn release_unsupported(&mut self)
    {
        if self.supports.is_empty()
        {
            return;
        }

        let mut lost = vec![];

        for watch in std::mem::take(&mut self.supports)
        {
            if !self.entries.get(watch.held).is_some_and(|entry| entry.frozen)
            {
                continue;
            }

            let Some(support) = self.entries.get(watch.support) else { continue; };

            // held again, or moving so long it no longer wakes anything - a pile would thaw itself forever otherwise
            if support.frozen || support.waiting || self.is_overdue(support)
            {
                continue;
            }

            let Some(body) = support.body.and_then(|handle| self.bodies.get(handle)) else { continue; };

            // the distance is only measured once the support has moved at all
            if Self::stays_put(&watch.start, body.position(), SUPPORT_DRIFT, SUPPORT_ANGLE) || self.colliders_touch(&self.entry_bounds(watch.held), &self.entry_bounds(watch.support))
            {
                self.supports.push(watch);
            }
            else
            {
                lost.push(watch.held);
            }
        }

        for index in lost
        {
            self.thaw_alone(index);
        }
    }

    fn entry_bounds(&self, index: usize) -> Vec<(ColliderHandle, Aabb)>
    {
        self.entries[index].parts.iter()
            .filter_map(|part| self.colliders.get(part.handle).map(|collider| (part.handle, collider.compute_aabb())))
            .collect()
    }

    // the frozen objects touching this one with their center above its center
    fn frozen_above(&self, index: usize) -> Vec<usize>
    {
        let up = -Vector3::new(self.settings.gravity.x, self.settings.gravity.y, self.settings.gravity.z);

        let center = |entry: &BodyEntry| entry.body.and_then(|handle| self.bodies.get(handle)).map(|body|
        {
            let center = body.center_of_mass();
            Vector3::new(center.x, center.y, center.z)
        });

        let own = self.entry_bounds(index);
        let own_center = center(&self.entries[index]);

        let mut above = vec![];

        for other in 0..self.entries.len()
        {
            if other == index || !self.entries[other].frozen
            {
                continue;
            }

            let (Some(own_center), Some(other_center)) = (own_center, center(&self.entries[other])) else { continue; };

            if (other_center - own_center).dot(&up) <= 0.0
            {
                continue;
            }

            if self.colliders_touch(&own, &self.entry_bounds(other))
            {
                above.push(other);
            }
        }

        above
    }

    pub fn waiting_amount(&self) -> usize
    {
        self.entries.iter().filter(|entry| entry.waiting).count()
    }

    // ********** running **********

    pub fn is_running(&self) -> bool
    {
        self.running
    }

    // Entering run mode records where every dynamic object started, leaving it puts them
    // back. Without that a single play press would permanently rearrange the scene.
    pub fn set_running(&mut self, running: bool)
    {
        if running == self.running
        {
            return;
        }

        self.running = running;
        self.time_accumulator = 0.0;
        self.run_steps = 0;
        self.contacts.reset();

        if running
        {
            // the world is built during the update, which has not run yet at the moment
            // play is pressed - so the colliders usually do not exist by then
            self.snapshot_pending = true;
        }
        else
        {
            self.snapshot_pending = false;
            self.restore_edit_snapshot();
        }
    }

    // Every run starts with the authored velocity, so shooting an object in is repeatable.
    fn apply_start_velocities(&mut self)
    {
        for index in 0..self.entries.len()
        {
            if self.entries[index].body_type != PhysicsBodyType::Dynamic
            {
                continue;
            }

            let Some(body) = self.entries[index].body else { continue; };
            let physics = self.entries[index].anchor.physics();

            if let Some(body) = self.bodies.get_mut(body)
            {
                body.set_linvel(Vector::new(physics.linear_velocity.x, physics.linear_velocity.y, physics.linear_velocity.z), true);
                body.set_angvel(Vector::new(physics.angular_velocity.x, physics.angular_velocity.y, physics.angular_velocity.z), true);
            }
        }
    }

    fn node_local_transform(node: &NodeItem) -> Option<Matrix4<f32>>
    {
        let node = node.read().unwrap();
        let transformation = node.find_component::<Transformation>()?;

        component_downcast!(transformation, Transformation);

        Some(*transformation.get_transform())
    }

    fn snapshot_instance(&mut self, node_id: u32, instance_id: u32, instance: &InstanceItemArc)
    {
        let local = instance.read().unwrap().find_component::<Transformation>().map(|transformation|
        {
            component_downcast!(transformation, Transformation);
            *transformation.get_transform()
        });

        if let Some(local) = local
        {
            self.edit_snapshot.insert((node_id, instance_id), local);
        }
    }

    // The whole chain up to the scene root: the gizmo may have been on any of them, an
    // object root with the mesh on a child is the usual shape of a loaded asset.
    fn snapshot_node_chain(&mut self, node: &NodeItem)
    {
        let mut current = Some(node.clone());

        while let Some(node) = current
        {
            let id = node.read().unwrap().id;

            if let Some(transform) = Self::node_local_transform(&node)
            {
                self.edit_snapshot_nodes.insert(id, transform);
            }

            current = node.read().unwrap().parent.as_ref().cloned();
        }
    }

    fn take_edit_snapshot(&mut self)
    {
        self.edit_snapshot.clear();
        self.edit_snapshot_nodes.clear();

        for index in 0..self.entries.len()
        {
            if self.entries[index].body_type != PhysicsBodyType::Dynamic
            {
                continue;
            }

            let parts: Vec<(NodeItem, u32, InstanceItemArc, u32)> = self.entries[index].parts.iter()
                .map(|part| (part.node.clone(), part.node_id, part.instance.clone(), part.instance_id))
                .collect();

            for (node, node_id, instance, instance_id) in parts
            {
                self.snapshot_instance(node_id, instance_id, &instance);
                self.snapshot_node_chain(&node);
            }
        }

        for vehicle in self.vehicles.values_mut()
        {
            vehicle.start = vehicle.node.upgrade().and_then(|node| Self::node_local_transform(&node));
            vehicle.rider_starts = vehicle.riders.iter().filter_map(|(rider, _)| rider.upgrade().and_then(|node| Self::node_local_transform(&node)).map(|start| (rider.clone(), start))).collect();
        }

        // coupled again where the trailers stand in the editor
        self.reset_hitches();
    }

    fn restore_node_chain(&self, node: &NodeItem)
    {
        let mut current = Some(node.clone());

        while let Some(node) = current
        {
            let id = node.read().unwrap().id;

            if let Some(transform) = self.edit_snapshot_nodes.get(&id).copied()
            {
                let node_read = node.read().unwrap();

                if let Some(transformation) = node_read.find_component::<Transformation>()
                {
                    component_downcast_mut!(transformation, Transformation);
                    transformation.set_local_transform(transform);
                }
            }

            current = node.read().unwrap().parent.as_ref().cloned();
        }
    }

    fn restore_instance(&self, node_id: u32, instance_id: u32, instance: &InstanceItemArc)
    {
        let Some(transform) = self.edit_snapshot.get(&(node_id, instance_id)).copied() else { return; };

        let instance = instance.read().unwrap();

        if let Some(transformation) = instance.find_component::<Transformation>()
        {
            component_downcast_mut!(transformation, Transformation);
            transformation.set_local_transform(transform);
        }
    }

    fn restore_edit_snapshot(&mut self)
    {
        // the node chains first, so the instance transforms below them land in the right place
        for index in 0..self.entries.len()
        {
            let nodes: Vec<NodeItem> = self.entries[index].parts.iter().map(|part| part.node.clone()).collect();

            for node in &nodes
            {
                self.restore_node_chain(node);
            }
        }

        for index in 0..self.entries.len()
        {
            if self.entries[index].body_type != PhysicsBodyType::Dynamic
            {
                continue;
            }

            let parts: Vec<(u32, InstanceItemArc, u32)> = self.entries[index].parts.iter()
                .map(|part| (part.node_id, part.instance.clone(), part.instance_id))
                .collect();

            for (node_id, instance, instance_id) in &parts
            {
                self.restore_instance(*node_id, *instance_id, instance);
            }

            // Put the body back too, otherwise it keeps its velocity and pose. The stored
            // transform is left alone on purpose: the next sync compares the scene against
            // it and moves the body onto whatever the scene shows by then.
            let anchor = self.entries[index].anchor.clone();
            let (pose, _) = Self::split_transform(&anchor.world_transform_live());

            if let Some(body) = self.entries[index].body
            {
                if let Some(body) = self.bodies.get_mut(body)
                {
                    Self::teleport(body, pose);
                }
            }

            // the render caches too, in case this runs after the scene update of the frame
            if let Anchor::Node { node } = &anchor
            {
                Self::refresh_instance_cache_below(node);
            }
        }

        self.reset_waiting();

        self.edit_snapshot.clear();
        self.edit_snapshot_nodes.clear();

        self.restore_vehicles();
    }

    // Vehicles go back to where the run started, standing still.
    fn restore_vehicles(&mut self)
    {
        for vehicle in self.vehicles.values_mut()
        {
            let Some(node) = vehicle.node.upgrade() else { continue; };

            if let (Some(start), Some(transformation)) = (vehicle.start.take(), node.read().unwrap().find_component::<Transformation>())
            {
                component_downcast_mut!(transformation, Transformation);
                transformation.set_local_transform(start);
            }

            Self::refresh_instance_cache_below(&node);

            for (rider, start) in std::mem::take(&mut vehicle.rider_starts)
            {
                let Some(rider) = rider.upgrade() else { continue; };

                if let Some(transformation) = rider.read().unwrap().find_component::<Transformation>()
                {
                    component_downcast_mut!(transformation, Transformation);
                    transformation.set_local_transform(start);
                }

                Self::refresh_instance_cache_below(&rider);
            }

            let world = node.read().unwrap().get_full_transform();
            let (pose, _) = Self::split_transform(&world);

            vehicle.stop_at(&mut self.bodies, pose);
            vehicle.shown = world;
        }

        self.reset_hitches();
    }

    // Advances the solver in fixed steps. The frame time is not constant, and feeding a
    // varying dt into a solver makes it behave differently at different frame rates.
    // `frozen` stops the stepping without restoring anything, unlike leaving the run mode.
    pub fn step(&mut self, delta_t: f32, frozen: bool) -> u32
    {
        self.contacts.begin_frame();

        if !self.has_dynamics() || !self.running
        {
            // a character walking through a purely static scene still touches things
            if self.running && !frozen
            {
                self.report_contacts();
            }

            return 0;
        }

        if self.snapshot_pending
        {
            self.snapshot_pending = false;
            self.take_edit_snapshot();
            self.refresh_all_entry_settings();
            self.apply_start_velocities();
            self.reset_waiting();
        }

        // frozen, but everything stays exactly where it is
        if frozen
        {
            self.time_accumulator = 0.0;
            return 0;
        }

        self.time_accumulator += delta_t.max(0.0);

        // a long hitch must not turn into a burst of catch up steps
        let max_time = self.settings.fixed_timestep * self.settings.max_substeps as f32;
        self.time_accumulator = self.time_accumulator.min(max_time);

        self.integration_params.dt = self.settings.fixed_timestep;
        self.integration_params.num_solver_iterations = self.settings.solver_iterations.max(1);

        // Rapier stiffens contacts against fixed bodies to twice the normal frequency, which
        // lands at 60 Hz - exactly the rate the solver runs at. A contact spring sampled once
        // per oscillation feeds energy in rather than taking it out, and anything tall and
        // narrow standing on the floor slowly rocks itself over. Measured: ten bowling pins
        // nudged at 0.3 rad/s, three fall at 60 Hz and none at 30. More solver iterations or
        // a finer timestep make it worse, which is what gives the cause away.
        self.integration_params.static_contact_softness.natural_frequency = self.integration_params.contact_softness.natural_frequency;

        let mut steps = 0;

        let gravity = self.gravity();

        if self.time_accumulator >= self.settings.fixed_timestep
        {
            self.record_speeds();

            let pending_steps = (self.time_accumulator / self.settings.fixed_timestep).floor();
            self.thaw_ahead_of_movers(pending_steps * self.settings.fixed_timestep);
        }

        while self.time_accumulator >= self.settings.fixed_timestep
        {
            self.time_accumulator -= self.settings.fixed_timestep;
            steps += 1;

            // only the last step of the frame is shown in between
            if self.time_accumulator < self.settings.fixed_timestep
            {
                for entry in &mut self.entries
                {
                    entry.step_pose = entry.body.and_then(|handle| self.bodies.get(handle)).filter(|body| body.is_dynamic() && !body.is_sleeping()).map(|body| (*body.position(), *body.position()));
                }
            }

            self.ensure_hitches();
            self.update_vehicles(self.settings.fixed_timestep);

            self.pipeline.step
            (
                gravity,
                &self.integration_params,
                &mut self.islands,
                &mut self.broad_phase_bvh,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.soft_bodies,
                &mut self.ccd_solver,
                &(),
                &self.contacts.collector
            );

            self.check_hitches(self.settings.fixed_timestep);
        }

        self.report_contacts();

        if steps > 0
        {
            for entry in &mut self.entries
            {
                if let (Some((_, after)), Some(body)) = (entry.step_pose.as_mut(), entry.body.and_then(|handle| self.bodies.get(handle)))
                {
                    *after = *body.position();
                }
            }

            self.run_steps = self.run_steps.saturating_add(steps);
            self.release_hit_bodies();
            self.release_unsupported();
            self.settle_bodies(steps as f32 * self.settings.fixed_timestep);
        }

        self.recover_escaped_bodies();

        steps
    }

    // ********** contacts **********

    // Turns rapier's start and stop events of this frame's steps, and what the characters touch, into contact events per object.
    fn report_contacts(&mut self)
    {
        if self.contacts.has_pending()
        {
            let vehicles: HashMap<RigidBodyHandle, u32> = self.vehicles.iter().map(|(node_id, vehicle)| (vehicle.body, *node_id)).collect();
            let combined: HashMap<RigidBodyHandle, u32> = self.entries.iter().filter(|entry| entry.is_combined()).filter_map(|entry| entry.body.map(|body| (body, entry.key.0))).collect();
            let ground_plane = self.ground_plane;
            let colliders = &self.colliders;

            self.contacts.process(|handle|
            {
                if Some(handle) == ground_plane
                {
                    return Some(ContactTarget::GroundPlane);
                }

                let collider = colliders.get(handle)?;

                if let Some(body) = collider.parent()
                {
                    if let Some(node_id) = vehicles.get(&body)
                    {
                        return Some(ContactTarget::Vehicle { node_id: *node_id });
                    }

                    if let Some(node_id) = combined.get(&body)
                    {
                        return Some(ContactTarget::Object { node_id: *node_id, instance_id: None });
                    }
                }

                // a single mesh placement, its anchor is exactly what the user data holds
                Some(ContactTarget::Object { node_id: collider.user_data as u32, instance_id: Some((collider.user_data >> 32) as u32) })
            });
        }

        self.contacts.refresh(&self.bodies, &self.colliders, &self.narrow_phase);
    }

    // Every contact change and ongoing touch of the last physics frame. Each contact is in here once, seen from either side.
    pub fn contact_events(&self) -> &[ContactEvent]
    {
        self.contacts.events()
    }

    // The contacts of one node, turned so that target is that node.
    pub fn contacts_of(&self, node_id: u32) -> impl Iterator<Item = ContactEvent> + '_
    {
        self.contacts.events().iter().filter_map(move |event|
        {
            if event.target.node_id() == Some(node_id)
            {
                Some(*event)
            }
            else if event.other.node_id() == Some(node_id)
            {
                Some(event.flipped())
            }
            else
            {
                None
            }
        })
    }

    pub fn contact_amount(&self) -> usize
    {
        self.contacts.active_amount()
    }

    // Everything within reach of a character capsule, whoever moved - pose: capsule center after the move, velocity: the one it tried to move with.
    pub fn report_character_contacts(&mut self, node_id: u32, pose: Pose, capsule: &Capsule, velocity: Vector3<f32>, filter: QueryFilter)
    {
        let velocity = Vector::new(velocity.x, velocity.y, velocity.z);

        let mut reach = *capsule;
        reach.radius += CHARACTER_TOUCH_KEEP;

        let mut touches = vec![];

        {
            let queries = self.query_pipeline(filter);

            for (handle, collider) in queries.intersect_shape(pose, &reach)
            {
                let Ok(Some(contact)) = parry3d::query::contact(&pose, capsule, collider.position(), collider.shape(), CHARACTER_TOUCH_KEEP) else { continue; };

                let other_velocity = collider.parent()
                    .and_then(|body| self.bodies.get(body))
                    .map(|body| body.velocity_at_point(contact.point2))
                    .unwrap_or(Vector::ZERO);

                touches.push(CharacterTouch
                {
                    collider: handle,
                    distance: contact.dist,
                    measure: Measure { point: contact.point2, normal: contact.normal1, relative_velocity: other_velocity - velocity },
                });
            }
        }

        self.contacts.report_character(node_id, touches);
    }

    fn rebuild_bvh(&mut self)
    {
        self.broad_phase_bvh = BroadPhaseBvh::new();

        let mut handles: Vec<ColliderHandle> = self.entries.iter().flat_map(|entry| entry.parts.iter().map(|part| part.handle)).collect();
        handles.extend(self.ground_plane);

        for handle in handles
        {
            self.refresh_leaf(handle);
        }
    }

    // ********** syncing **********

    // Mirrors the scene onto the physics world. Static geometry and kinematic bodies always
    // follow the scene. While the solver advances, a dynamic body owns its pose and the
    // scene follows it; outside that - not running, or frozen - it is the other way round:
    // the author moves the object, so the body has to follow, otherwise the write back
    // would snap it right back. The exception is the author reaching in mid run - their
    // move has to win for that frame, otherwise the solver writes its own pose straight
    // back over it. apply_dynamic_bodies stores what it wrote, and the scene rebuilds
    // exactly that again, so any larger difference means somebody else moved the object.
    //
    // The parts only ever follow an edit below the anchor, which does not fight the
    // solver, so that is applied whatever the mode.
    pub fn sync_transformations(&mut self, frozen: bool) -> usize
    {
        let scene_owns_dynamics = !self.running || frozen;

        let mut updated = 0;
        let mut rebuilds = 0;

        self.moved_kinematics.clear();

        for index in 0..self.entries.len()
        {
            let dynamic = self.entries[index].body_type == PhysicsBodyType::Dynamic;
            let anchor = self.entries[index].anchor.clone();

            let anchor_world = anchor.world_transform();
            let moved = Self::transform_differs(&anchor_world, &self.entries[index].transform);

            let author_moved = moved && Self::differs_beyond_noise(&anchor_world, &self.entries[index].transform);
            let anchor_follows = moved && (!dynamic || scene_owns_dynamics || author_moved);

            // the author picked up a frozen object mid run, it has to fall from where it lands
            if author_moved && !scene_owns_dynamics && self.entries[index].frozen
            {
                self.thaw_alone(index);
            }

            let (anchor_pose, fresh_scale) = Self::split_transform(&anchor_world);

            // The scale only replaces the stored one when it really changed. It is read
            // back out of a rotating matrix, and the float noise in it would otherwise
            // reach every part and look like an edit on each of them.
            let scale_changed = anchor_follows && Self::scale_differs(&fresh_scale, &self.entries[index].scale);
            let anchor_scale = if scale_changed { fresh_scale } else { self.entries[index].scale };

            let body = self.entries[index].body;
            let mut moved_parts: Vec<ColliderHandle> = vec![];

            for part_index in 0..self.entries[index].parts.len()
            {
                let (node, instance) =
                {
                    let part = &self.entries[index].parts[part_index];
                    (part.node.clone(), part.instance.clone())
                };

                let local = Self::part_local_transform(&anchor, &anchor_scale, &node, &instance);

                if !Self::transform_differs(&local, &self.entries[index].parts[part_index].local)
                {
                    continue;
                }

                let (offset, scale) = Self::split_transform(&local);
                let handle = self.entries[index].parts[part_index].handle;

                let shape = if Self::scale_differs(&scale, &self.entries[index].parts[part_index].scale)
                {
                    rebuilds += 1;
                    Self::build_shape(&node, &scale)
                }
                else
                {
                    None
                };

                if let Some(collider) = self.colliders.get_mut(handle)
                {
                    match body
                    {
                        Some(_) => collider.set_position_wrt_parent(offset),
                        None => collider.set_position(anchor_pose * offset)
                    }

                    if let Some(shape) = shape
                    {
                        collider.set_shape(shape);
                    }
                }

                let part = &mut self.entries[index].parts[part_index];
                part.local = local;
                part.offset = offset;
                part.scale = scale;

                moved_parts.push(handle);
                updated += 1;
            }

            if scale_changed
            {
                self.entries[index].scale = fresh_scale;
            }

            // a kinematic mesh moved below its anchor moves its collider, so it is on the
            // move as far as anything it touches is concerned
            if !dynamic && !moved_parts.is_empty()
            {
                if let Some(handle) = body
                {
                    self.moved_kinematics.insert(handle);
                }
            }

            if anchor_follows
            {
                match body
                {
                    // a dynamic body is teleported to where the author put it, a kinematic
                    // one is handed to the solver so it moves things on its way
                    Some(handle) =>
                    {
                        if let Some(body) = self.bodies.get_mut(handle)
                        {
                            if dynamic
                            {
                                Self::teleport(body, anchor_pose);
                            }
                            else
                            {
                                body.set_next_kinematic_position(anchor_pose);
                                self.moved_kinematics.insert(handle);
                            }
                        }
                    }
                    // standalone colliders are placed one by one
                    None =>
                    {
                        for part_index in 0..self.entries[index].parts.len()
                        {
                            let handle = self.entries[index].parts[part_index].handle;
                            let offset = self.entries[index].parts[part_index].offset;

                            if let Some(collider) = self.colliders.get_mut(handle)
                            {
                                collider.set_position(anchor_pose * offset);
                                moved_parts.push(handle);
                            }
                        }
                    }
                }

                self.entries[index].transform = anchor_world;
                updated += 1;
            }

            // a world without a body never steps, so the bvh is kept up by hand
            if !self.has_dynamics()
            {
                for handle in moved_parts
                {
                    self.refresh_leaf(handle);
                }
            }
        }

        updated += self.sync_vehicles(scene_owns_dynamics);

        self.last_synced = updated;
        self.last_shape_rebuilds = rebuilds;

        updated
    }

    // Same rule as for a dynamic entry: the scene owns the pose outside a run, and an author move wins inside one.
    fn sync_vehicles(&mut self, scene_owns_dynamics: bool) -> usize
    {
        // a deleted vehicle node must not leave an invisible body behind
        let dead: Vec<u32> = self.vehicles.iter().filter(|(_, vehicle)| vehicle.node.upgrade().is_none()).map(|(id, _)| *id).collect();
        for id in dead
        {
            self.remove_vehicle(id);
        }

        let mut updated = 0;

        for vehicle in self.vehicles.values_mut()
        {
            if Self::follow_vehicle_node(vehicle, &mut self.bodies, scene_owns_dynamics)
            {
                updated += 1;
            }
        }

        updated
    }

    // Puts the body where the node is, if somebody else moved the node since it was last written.
    fn follow_vehicle_node(vehicle: &mut VehicleEntry, bodies: &mut RigidBodySet, scene_owns_dynamics: bool) -> bool
    {
        let Some(node) = vehicle.node.upgrade() else { return false; };
        let world = node.read().unwrap().get_full_transform();

        if !Self::transform_differs(&world, &vehicle.shown)
        {
            return false;
        }

        if !scene_owns_dynamics && !Self::differs_beyond_noise(&world, &vehicle.shown)
        {
            return false;
        }

        let (pose, _) = Self::split_transform(&world);

        if let Some(body) = bodies.get_mut(vehicle.body)
        {
            Self::teleport(body, pose);
        }

        vehicle.place_riders(&pose);

        vehicle.previous = pose;
        vehicle.shown = world;

        true
    }

    // Suspension, drive and tire forces - right before each solver step, like any other force.
    fn update_vehicles(&mut self, dt: f32)
    {
        let coupled: Vec<(RigidBodyHandle, RigidBodyHandle)> = self.hitches.iter().filter(|(_, hitch)| hitch.joint.is_some())
            .filter_map(|(trailer, hitch)| Some((self.vehicles.get(&hitch.tow)?.body, self.vehicles.get(trailer)?.body)))
            .collect();

        // a sleeping vehicle is skipped - rapier would pile the spring impulses onto its velocity and turn the wheels by it
        let mut awake: HashSet<RigidBodyHandle> = self.vehicles.values().map(|vehicle| vehicle.body).filter(|handle| self.bodies.get(*handle).is_some_and(|body| !body.is_sleeping())).collect();

        // what is coupled to a vehicle woken this frame wakes with it, before its springs would miss a step
        loop
        {
            let before = awake.len();
            for (tow, trailer) in &coupled
            {
                if awake.contains(tow) || awake.contains(trailer)
                {
                    awake.extend([*tow, *trailer]);
                }
            }

            if awake.len() == before { break; }
        }

        for vehicle in self.vehicles.values_mut()
        {
            let Some(body) = self.bodies.get(vehicle.body) else { continue; };
            vehicle.previous = *body.position();
            vehicle.pre_velocity = body.linvel();

            if !awake.contains(&vehicle.body)
            {
                continue;
            }

            if body.is_sleeping()
            {
                self.bodies.get_mut(vehicle.body).unwrap().wake_up(true);
            }

            // the wheels never stand on the vehicle they are coupled to
            let partners: Vec<RigidBodyHandle> = coupled.iter().filter_map(|(tow, trailer)| if *tow == vehicle.body { Some(*trailer) } else if *trailer == vehicle.body { Some(*tow) } else { None }).collect();
            let not_partner = |_: ColliderHandle, collider: &Collider| !collider.parent().is_some_and(|parent| partners.contains(&parent));

            let mut filter = QueryFilter::default().exclude_rigid_body(vehicle.body).exclude_sensors();
            if !partners.is_empty()
            {
                filter.predicate = Some(&not_partner);
            }

            let queries = self.broad_phase_bvh.as_query_pipeline_mut(&self.dispatcher, &mut self.bodies, &mut self.colliders, filter);

            vehicle.controller.update_vehicle(dt, queries);
        }
    }

    // Writes the vehicle poses back, interpolated between the last two steps - the steps do not line up with the frames.
    // how far the frame is into the step after the last one - the scene shows the bodies that far between their last two poses
    fn interpolation_alpha(&self) -> f32
    {
        if self.settings.fixed_timestep > 0.0 { (self.time_accumulator / self.settings.fixed_timestep).clamp(0.0, 1.0) } else { 1.0 }
    }

    fn interpolate(before: &Pose, after: &Pose, alpha: f32) -> Pose
    {
        Pose::from_parts(before.translation.lerp(after.translation, alpha), before.rotation.slerp(after.rotation, alpha))
    }

    // between the last two steps, like the vehicles - unless something moved the body since
    fn step_interpolated(step_pose: &Option<(Pose, Pose)>, current: Pose, alpha: f32) -> Pose
    {
        match step_pose
        {
            Some((before, after)) if after.translation == current.translation && after.rotation == current.rotation => Self::interpolate(before, &current, alpha),
            _ => current,
        }
    }

    fn apply_vehicles(&mut self) -> usize
    {
        let alpha = self.interpolation_alpha();
        let mut applied = 0;

        for vehicle in self.vehicles.values_mut()
        {
            let Some(node) = vehicle.node.upgrade() else { continue; };
            let Some(body) = self.bodies.get(vehicle.body) else { continue; };

            let current = *body.position();

            if !current.translation.is_finite() || !current.rotation.is_finite()
            {
                continue;
            }

            let pose = Self::interpolate(&vehicle.previous, &current, alpha);

            let anchor = Anchor::Node { node };
            anchor.ensure_transformation();

            let Some(shown) = anchor.write_back(&pose) else { continue; };

            vehicle.place_riders(&pose);

            vehicle.shown = shown;
            applied += 1;
        }

        applied
    }

    // Writes the solver result back into the scene. This is the one place where the
    // physics world is the authority and the scene graph follows. Outside a run, and while
    // frozen, the authored transform wins instead, so nothing is written back there.
    pub fn apply_dynamic_bodies(&mut self, frozen: bool) -> usize
    {
        if !self.has_dynamics() || !self.running || frozen
        {
            return 0;
        }

        let mut applied = 0;
        let alpha = self.interpolation_alpha();

        for index in 0..self.entries.len()
        {
            // a waiting body sits exactly where the scene put it, a frozen one where it was last written
            if self.entries[index].body_type != PhysicsBodyType::Dynamic || self.entries[index].waiting || self.entries[index].frozen
            {
                continue;
            }

            let Some(handle) = self.entries[index].body else { continue; };
            let Some(body) = self.bodies.get(handle) else { continue; };

            if body.is_sleeping()
            {
                continue;
            }

            let pose = Self::step_interpolated(&self.entries[index].step_pose, *body.position(), alpha);

            // A degenerate shape or a zero mass can still make the solver produce NaN. Once
            // that reaches a transform it spreads through every derived value and takes the
            // renderer down with it, so it stops here.
            if !pose.translation.is_finite() || !pose.rotation.is_finite()
            {
                continue;
            }

            let anchor = self.entries[index].anchor.clone();

            let Some(shown) = anchor.write_back(&pose) else { continue; };

            self.entries[index].transform = shown;
            applied += 1;
        }

        applied += self.apply_vehicles();

        applied
    }

    // ********** debug view **********

    // Registers the capsule a character casts through the world, for the debug view only.
    pub fn set_character_shape(&mut self, node: &NodeItem, center_offset: f32, half_height: f32, radius: f32)
    {
        let node_id = node.read().unwrap().id;

        self.characters.insert(node_id, CharacterShape { node: Arc::downgrade(node), center_offset, half_height, radius });
    }

    pub fn remove_character_shape(&mut self, node_id: u32)
    {
        self.characters.remove(&node_id);
        self.contacts.remove_character(node_id);
    }

    // Every collider and character capsule in world space, for the debug view.
    pub fn debug_volumes(&self) -> Vec<PhysicsDebugVolume>
    {
        let mut volumes = vec![];

        // the same in-between pose the scene shows, otherwise the volumes run up to a step ahead
        let alpha = self.interpolation_alpha();

        for entry in &self.entries
        {
            let body = entry.body.and_then(|handle| self.bodies.get(handle));

            let state = match entry.body_type
            {
                PhysicsBodyType::Static => PhysicsDebugState::Static,
                PhysicsBodyType::Kinematic => PhysicsDebugState::Kinematic,
                PhysicsBodyType::Dynamic if entry.waiting => PhysicsDebugState::Waiting,
                PhysicsBodyType::Dynamic if entry.frozen || body.is_some_and(|body| body.is_sleeping()) => PhysicsDebugState::Sleeping,
                PhysicsBodyType::Dynamic => PhysicsDebugState::Dynamic,
            };

            for part in &entry.parts
            {
                let Some(collider) = self.colliders.get(part.handle) else { continue; };

                // an attached collider only catches up with its body in a step, and nothing steps while editing
                let pose = match (body, collider.position_wrt_parent())
                {
                    (Some(body), Some(offset)) if self.running => Self::step_interpolated(&entry.step_pose, *body.position(), alpha) * *offset,
                    (Some(body), Some(offset)) => *body.position() * *offset,
                    _ => *collider.position()
                };

                Self::push_debug_shape(collider.shape(), &pose, state, &mut volumes);
            }
        }

        if let Some(collider) = self.ground_plane.and_then(|handle| self.colliders.get(handle))
        {
            Self::push_debug_shape(collider.shape(), collider.position(), PhysicsDebugState::Static, &mut volumes);
        }

        for character in self.characters.values()
        {
            let Some(node) = character.node.upgrade() else { continue; };

            let position = extract_translation_from_transform(&node.read().unwrap().get_full_transform());
            let pose = Pose::from_translation(Vector::new(position.x, position.y + character.center_offset, position.z));

            volumes.push(Self::debug_volume(PhysicsDebugShape::Capsule { half_height: character.half_height, radius: character.radius }, &pose, PhysicsDebugState::Character));
        }

        for vehicle in self.vehicles.values()
        {
            let Some(body) = self.bodies.get(vehicle.body) else { continue; };
            let state = if body.is_sleeping() { PhysicsDebugState::Sleeping } else { PhysicsDebugState::Dynamic };

            let pose = if self.running { Self::interpolate(&vehicle.previous, body.position(), alpha) } else { *body.position() };

            if let Some(collider) = self.colliders.get(vehicle.collider)
            {
                let offset = collider.position_wrt_parent().copied().unwrap_or(Pose::IDENTITY);
                Self::push_debug_hull(collider.shape(), &(pose * offset), state, &mut volumes);
            }

            // wheels as balls where the suspension currently holds them
            for wheel in vehicle.controller.wheels()
            {
                let hard_point = pose * wheel.chassis_connection_point_cs;
                let direction = pose.rotation * wheel.direction_cs;

                let length = if self.running { wheel.raycast_info().suspension_length } else { wheel.suspension_rest_length };

                volumes.push(Self::debug_volume(PhysicsDebugShape::Sphere { radius: wheel.radius }, &Pose::from_translation(hard_point + direction * length), PhysicsDebugState::Character));
            }
        }

        volumes
    }

    fn debug_volume(shape: PhysicsDebugShape, pose: &Pose, state: PhysicsDebugState) -> PhysicsDebugVolume
    {
        let matrix = pose.to_mat4();

        PhysicsDebugVolume
        {
            shape,
            transform: Matrix4::from_column_slice(&matrix.to_cols_array()),
            state,
        }
    }

    // Exact for primitives, compounds are walked, everything mesh based is reduced to its oriented bounds.
    fn push_debug_shape(shape: &dyn Shape, pose: &Pose, state: PhysicsDebugState, volumes: &mut Vec<PhysicsDebugVolume>)
    {
        match shape.as_typed_shape()
        {
            TypedShape::Ball(ball) =>
            {
                volumes.push(Self::debug_volume(PhysicsDebugShape::Sphere { radius: ball.radius }, pose, state));
            }
            TypedShape::Cuboid(cuboid) =>
            {
                let half = cuboid.half_extents;
                volumes.push(Self::debug_volume(PhysicsDebugShape::Box { half_extents: Vector3::new(half.x, half.y, half.z) }, pose, state));
            }
            TypedShape::Capsule(capsule) =>
            {
                let axis = capsule.segment.b - capsule.segment.a;
                let height = axis.length();

                // the debug capsule stands along y, so a tilted segment becomes a rotation
                let rotation = if height > f32::EPSILON { Rotation::from_rotation_arc(Vector::Y, axis / height) } else { Rotation::IDENTITY };
                let local = Pose::from_parts(capsule.center(), rotation);

                volumes.push(Self::debug_volume(PhysicsDebugShape::Capsule { half_height: height * 0.5, radius: capsule.radius }, &(*pose * local), state));
            }
            TypedShape::Compound(compound) =>
            {
                for (sub_pose, sub_shape) in compound.shapes()
                {
                    Self::push_debug_shape(&**sub_shape, &(*pose * *sub_pose), state, volumes);
                }
            }
            TypedShape::HalfSpace(_) => {}
            _ =>
            {
                let aabb = shape.compute_local_aabb();
                let half = aabb.half_extents();
                let local = Pose::from_translation(aabb.center());

                volumes.push(Self::debug_volume(PhysicsDebugShape::Bounds { half_extents: Vector3::new(half.x, half.y, half.z) }, &(*pose * local), state));
            }
        }
    }

    // A vehicle body edge by edge - its bounds would hide the slopes and the rounding. The rounding is drawn as the inner hull grown to the same bounds.
    fn push_debug_hull(shape: &dyn Shape, pose: &Pose, state: PhysicsDebugState, volumes: &mut Vec<PhysicsDebugVolume>)
    {
        let (hull, radius) = match shape.as_typed_shape()
        {
            TypedShape::ConvexPolyhedron(hull) => (hull, 0.0),
            TypedShape::RoundConvexPolyhedron(round) => (&round.inner_shape, round.border_radius),
            _ => return Self::push_debug_shape(shape, pose, state, volumes),
        };

        let aabb = hull.local_aabb();
        let (center, half) = (aabb.center(), aabb.half_extents());
        let grow = (half + Vector::splat(radius)) / half.max(Vector::splat(0.0001));
        let point = |index: u32| { let p = center + (hull.points()[index as usize] - center) * grow; Vector3::new(p.x, p.y, p.z) };

        // only the edges around the faces - the ones inside a face are left over from its triangles
        let mut edges = hull.edges_adj_to_face().to_vec();
        edges.sort_unstable();
        edges.dedup();

        for edge in edges.into_iter().filter_map(|index| hull.edges().get(index as usize))
        {
            volumes.push(Self::debug_volume(PhysicsDebugShape::Segment { a: point(edge.vertices[0]), b: point(edge.vertices[1]) }, pose, state));
        }
    }

    // ********** queries **********

    pub fn query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a>
    {
        self.broad_phase_bvh.as_query_pipeline(&self.dispatcher, &self.bodies, &self.colliders, filter)
    }

    // The writing variant, for queries that push something around rather than only look.
    pub fn query_pipeline_mut<'a>(&'a mut self, filter: QueryFilter<'a>) -> QueryPipelineMut<'a>
    {
        self.broad_phase_bvh.as_query_pipeline_mut(&self.dispatcher, &mut self.bodies, &mut self.colliders, filter)
    }

    pub fn collider_translation(&self, handle: ColliderHandle) -> Option<Vector3<f32>>
    {
        let collider = self.colliders.get(handle)?;
        let translation = collider.translation();

        Some(Vector3::new(translation.x, translation.y, translation.z))
    }

    // Collider directly below a point plus its translation, for moving platforms.
    pub fn ground_collider_below(&self, from: Vector3<f32>, max_distance: f32, filter: QueryFilter) -> Option<(ColliderHandle, Vector3<f32>)>
    {
        let ray = Ray::new(Vector::new(from.x, from.y, from.z), -Vector::Y);
        let queries = self.query_pipeline(filter);

        let (handle, _) = queries.cast_ray(&ray, max_distance, true)?;
        let translation = self.collider_translation(handle)?;

        Some((handle, translation))
    }

    // The scene node id a collider belongs to (stored in the collider user_data).
    pub fn node_id_of(&self, handle: ColliderHandle) -> Option<u32>
    {
        self.colliders.get(handle).map(|collider| collider.user_data as u32)
    }

    pub fn instance_id_of(&self, handle: ColliderHandle) -> Option<u32>
    {
        self.colliders.get(handle).map(|collider| (collider.user_data >> 32) as u32)
    }

    pub fn collider_friction(&self, handle: ColliderHandle) -> Option<f32>
    {
        self.colliders.get(handle).map(|collider| collider.friction())
    }

    // ********** vehicles **********

    // Builds the chassis and the wheels. A new vehicle starts at the node's world pose, an existing one keeps its body - its motion and where its run started.
    pub fn set_vehicle(&mut self, node: &NodeItem, chassis: VehicleChassisDesc, wheels: &[VehicleWheelDesc])
    {
        let node_id = node.read().unwrap().id;

        if !self.vehicles.get(&node_id).is_some_and(|vehicle| self.bodies.get(vehicle.body).is_some())
        {
            self.remove_vehicle(node_id);
            self.insert_vehicle_body(node);
        }

        let Some(vehicle) = self.vehicles.get_mut(&node_id) else { return; };
        let body = vehicle.body;

        // a node moved by hand wins over the kept body, like in the sync
        Self::follow_vehicle_node(vehicle, &mut self.bodies, !self.running);

        if let Some(body) = self.bodies.get_mut(body)
        {
            body.set_linear_damping(chassis.linear_damping.max(0.0));
            body.set_angular_damping(chassis.angular_damping.max(0.0));
        }

        let old_colliders: Vec<ColliderHandle> = self.bodies.get(body).map(|body| body.colliders().to_vec()).unwrap_or_default();
        for collider in old_colliders
        {
            self.colliders.remove(collider, &mut self.islands, &mut self.bodies, &mut self.soft_bodies, true);
        }

        let com = Vector::new(chassis.center_of_mass.x, chassis.center_of_mass.y, chassis.center_of_mass.z);
        let inertia = Vector::new(chassis.principal_inertia.x.max(0.001), chassis.principal_inertia.y.max(0.001), chassis.principal_inertia.z.max(0.001));

        let collider = ColliderBuilder::new(chassis.shape)
            .user_data(Self::pack_user_data(node_id, 0))
            .friction(chassis.friction.max(0.0))
            .restitution(chassis.restitution.clamp(0.0, 1.0))
            .active_collision_types(ActiveCollisionTypes::default() | ActiveCollisionTypes::KINEMATIC_FIXED)
            .active_events(ActiveEvents::COLLISION_EVENTS) // a vehicle always reports, running into something is its main event
            .mass_properties(MassProperties::new(com, chassis.mass.max(1.0), inertia))
            .build();

        let collider = self.colliders.insert_with_parent(collider, body, &mut self.bodies);

        for (center, radius) in &chassis.bumpers
        {
            let bumper = ColliderBuilder::ball(radius.max(0.01))
                .user_data(Self::pack_user_data(node_id, 0))
                .position(Pose::from_translation(Vector::new(center.x, center.y, center.z)))
                .density(0.0)
                .friction(0.0)
                .friction_combine_rule(CoefficientCombineRule::Min)
                .restitution(0.0)
                .active_collision_types(ActiveCollisionTypes::default() | ActiveCollisionTypes::KINEMATIC_FIXED)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build();

            self.colliders.insert_with_parent(bumper, body, &mut self.bodies);
        }

        let mut controller = DynamicRayCastVehicleController::new(body);
        controller.index_up_axis = 1;

        for wheel in wheels
        {
            let v = |v: &Vector3<f32>| Vector::new(v.x, v.y, v.z);
            controller.add_wheel(v(&wheel.connection), v(&wheel.direction), v(&wheel.axle), wheel.rest_length, wheel.radius, &wheel.tuning);
        }

        // the wheels keep their spin, everything else is measured again in the next step
        for (new, old) in controller.wheels_mut().iter_mut().zip(vehicle.controller.wheels())
        {
            new.rotation = old.rotation;
        }

        vehicle.collider = collider;
        vehicle.controller = controller;
        vehicle.frame = (Vector::new(chassis.forward.x, chassis.forward.y, chassis.forward.z), Vector::new(chassis.up.x, chassis.up.y, chassis.up.z));
    }

    // A dynamic body at the node's world pose, registered as a vehicle without colliders and wheels yet.
    fn insert_vehicle_body(&mut self, node: &NodeItem)
    {
        let node_id = node.read().unwrap().id;
        let world = node.read().unwrap().get_full_transform();
        let (pose, _) = Self::split_transform(&world);

        // no ccd, measured: hard ccd shook a load riding on it, soft ccd kicked the car up at edges - without, a car still stops at a 5 cm wall at 260 km/h
        let body = RigidBodyBuilder::dynamic()
            .pose(pose)
            .build();

        let body = self.bodies.insert(body);

        let (linear, angular, time_until_sleep) = self.configured_sleep();
        if let Some(body) = self.bodies.get_mut(body)
        {
            let activation = body.activation_mut();

            activation.normalized_linear_threshold = linear;
            activation.angular_threshold = angular;
            activation.time_until_sleep = time_until_sleep;
        }

        let controller = DynamicRayCastVehicleController::new(body);
        self.vehicles.insert(node_id, VehicleEntry { node: Arc::downgrade(node), body, collider: ColliderHandle::invalid(), controller, previous: pose, shown: world, start: None, riders: vec![], rider_starts: vec![], frame: (Vector::Z, Vector::Y), pre_velocity: Vector::ZERO });
    }

    // Moves the vehicles from the replaced sets into the new ones - a rebuild of the scene colliders must not reset them.
    fn carry_vehicles(&mut self, old_bodies: &RigidBodySet, old_colliders: &ColliderSet)
    {
        let mut lost = vec![];

        for (node_id, vehicle) in self.vehicles.iter_mut()
        {
            let Some(old_body) = old_bodies.get(vehicle.body) else
            {
                lost.push(*node_id);
                continue;
            };

            let mut body = old_body.clone();
            body.wake_up(true);
            let body = self.bodies.insert(body);

            for handle in old_body.colliders()
            {
                let Some(collider) = old_colliders.get(*handle) else { continue; };
                let new_handle = self.colliders.insert_with_parent(collider.clone(), body, &mut self.bodies);

                if *handle == vehicle.collider
                {
                    vehicle.collider = new_handle;
                }
            }

            vehicle.body = body;
            vehicle.controller.chassis = body;
        }

        for node_id in lost
        {
            self.vehicles.remove(&node_id);
        }
    }

    // Sets the nodes riding along. They are put on their seats right away.
    pub fn set_vehicle_riders(&mut self, node_id: u32, riders: Vec<(NodeItem, Pose)>)
    {
        let Some(vehicle) = self.vehicles.get_mut(&node_id) else { return; };

        vehicle.riders = riders.into_iter().map(|(rider, seat)| (Arc::downgrade(&rider), seat)).collect();

        if let Some(body) = self.bodies.get(vehicle.body)
        {
            vehicle.place_riders(body.position());
        }
    }

    pub fn remove_vehicle(&mut self, node_id: u32)
    {
        if let Some(vehicle) = self.vehicles.remove(&node_id)
        {
            self.bodies.remove(vehicle.body, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, &mut self.soft_bodies, true);
        }
    }

    pub fn has_vehicle(&self, node_id: u32) -> bool
    {
        self.vehicles.contains_key(&node_id)
    }

    pub fn vehicle(&self, node_id: u32) -> Option<(&VehicleEntry, &RigidBody)>
    {
        let vehicle = self.vehicles.get(&node_id)?;
        let body = self.bodies.get(vehicle.body)?;

        Some((vehicle, body))
    }

    pub fn vehicle_mut(&mut self, node_id: u32) -> Option<(&mut VehicleEntry, &mut RigidBody)>
    {
        let vehicle = self.vehicles.get_mut(&node_id)?;
        let body = self.bodies.get_mut(vehicle.body)?;

        Some((vehicle, body))
    }

    // Puts a vehicle somewhere else at a standstill, without an interpolated slide there. Its trailers follow, coupled again and straight behind.
    pub fn place_vehicle(&mut self, node_id: u32, pose: Pose)
    {
        let Some((vehicle, body)) = self.vehicle_mut(node_id) else { return; };

        Self::teleport(body, pose);
        vehicle.previous = pose;

        self.place_trailers(node_id, pose);
    }

    fn place_trailers(&mut self, tow_id: u32, tow_pose: Pose)
    {
        let trailers: Vec<u32> = self.hitches.iter().filter(|(_, hitch)| hitch.tow == tow_id).map(|(trailer, _)| *trailer).collect();

        for trailer_id in trailers
        {
            let Some(tow) = self.vehicles.get(&tow_id) else { return; };
            let Some(trailer) = self.vehicles.get(&trailer_id) else { continue; };
            let Some(hitch) = self.hitches.get_mut(&trailer_id) else { continue; };
            let Some(anchor) = hitch.anchor else { continue; };

            // the two balls on each other, the axes lined up
            let ball = Pose::from_parts(hitch.desc.point, tow.frame_rotation());
            let eye = Pose::from_parts(anchor, trailer.frame_rotation());
            let pose = tow_pose * ball * eye.inverse();

            if let Some(joint) = hitch.joint.take()
            {
                self.impulse_joints.remove(joint, true);
            }

            hitch.state = HitchState::Waiting;

            if let Some((trailer, body)) = self.vehicle_mut(trailer_id)
            {
                Self::teleport(body, pose);
                trailer.previous = pose;
            }

            self.place_trailers(trailer_id, pose);
        }
    }

    // ********** trailer hitches **********

    // Couples the trailer to the tow vehicle - one trailer per vehicle. The joint is made at the next step, once both vehicles have a body.
    pub fn set_hitch(&mut self, tow_id: u32, trailer_id: u32, desc: HitchDesc)
    {
        let others: Vec<u32> = self.hitches.iter().filter(|(trailer, hitch)| hitch.tow == tow_id && **trailer != trailer_id).map(|(trailer, _)| *trailer).collect();
        for other in others
        {
            self.remove_hitch(other);
        }

        if tow_id == trailer_id
        {
            return;
        }

        if self.hitches.get(&trailer_id).is_some_and(|hitch| hitch.tow == tow_id && hitch.desc == desc)
        {
            return;
        }

        // another tow vehicle, or new limits - coupled again where the two stand right now
        self.remove_hitch(trailer_id);
        self.hitches.insert(trailer_id, HitchEntry { tow: tow_id, desc, state: HitchState::Waiting, brake: 0.0, force: 0.0, angles: Vector3::zeros(), joint: None, anchor: None, release_in: 0.0 });
    }

    pub fn remove_hitch(&mut self, trailer_id: u32)
    {
        if let Some(joint) = self.hitches.remove(&trailer_id).and_then(|hitch| hitch.joint)
        {
            self.impulse_joints.remove(joint, true);
        }
    }

    pub fn remove_hitches_of(&mut self, tow_id: u32)
    {
        let trailers: Vec<u32> = self.hitches.iter().filter(|(_, hitch)| hitch.tow == tow_id).map(|(trailer, _)| *trailer).collect();
        for trailer in trailers
        {
            self.remove_hitch(trailer);
        }
    }

    pub fn has_hitch(&self, tow_id: u32, trailer_id: u32) -> bool
    {
        self.hitches.get(&trailer_id).is_some_and(|hitch| hitch.tow == tow_id)
    }

    // the coupling of a trailer
    pub fn hitch_of(&self, trailer_id: u32) -> Option<&HitchEntry>
    {
        self.hitches.get(&trailer_id)
    }

    // the coupling a vehicle tows with, and its trailer
    pub fn hitch_from(&self, tow_id: u32) -> Option<(u32, &HitchEntry)>
    {
        self.hitches.iter().find(|(_, hitch)| hitch.tow == tow_id).map(|(trailer, hitch)| (*trailer, hitch))
    }

    // what the trailers of this vehicle brake with, 0..1
    pub fn set_hitch_brake(&mut self, tow_id: u32, brake: f32)
    {
        for hitch in self.hitches.values_mut().filter(|hitch| hitch.tow == tow_id)
        {
            hitch.brake = brake;
        }
    }

    // What the springs of a vehicle carry against its own weight once its couplings hold: more under a trailer, less on a trailer the tow vehicle carries part of.
    // The suspension scales with it, so a dolly under a semi trailer does not bottom out and the trailer on it keeps its rear wheels on the ground.
    pub fn vehicle_load_factor(&self, node_id: u32) -> f32
    {
        let Some(weight) = self.vehicle_weight(node_id) else { return 1.0; };
        let carried: f32 = self.hitches.iter().filter(|(_, hitch)| hitch.tow == node_id).map(|(trailer, _)| self.hitch_load(*trailer, 0)).sum();
        let carried_by_tow = self.hitch_load(node_id, 0);

        ((weight + carried - carried_by_tow) / weight).clamp(0.2, 10.0)
    }

    fn vehicle_weight(&self, node_id: u32) -> Option<f32>
    {
        let body = self.bodies.get(self.vehicles.get(&node_id)?.body)?;
        let weight = body.mass() * self.gravity().length();
        if weight > 0.0 { Some(weight) } else { None }
    }

    fn gravity(&self) -> Vector
    {
        Vector::new(self.settings.gravity.x, self.settings.gravity.y, self.settings.gravity.z)
    }

    // N a trailer rests on its tow vehicle while standing, by the lever between its coupling and the middle of its wheels - with what its own trailers rest on it
    fn hitch_load(&self, trailer_id: u32, depth: u32) -> f32
    {
        let (Some(hitch), Some(vehicle), Some(weight)) = (self.hitches.get(&trailer_id), self.vehicles.get(&trailer_id), self.vehicle_weight(trailer_id)) else { return 0.0; };
        let (Some(anchor), Some(body)) = (hitch.anchor, self.bodies.get(vehicle.body)) else { return 0.0; };
        if hitch.state == HitchState::Broken || depth > 8
        {
            return 0.0;
        }

        let forward = vehicle.frame.0.normalize_or(Vector::Z);
        let along = |point: Vector| point.dot(forward);

        let wheels = vehicle.controller.wheels();
        if wheels.is_empty()
        {
            return weight;
        }
        let axles = wheels.iter().map(|wheel| along(wheel.chassis_connection_point_cs)).sum::<f32>() / wheels.len() as f32;

        let coupling = along(anchor) - axles;
        if coupling.abs() < 0.01
        {
            return 0.0;
        }

        let mut moment = weight * (along(body.mass_properties().local_mprops.local_com) - axles);
        let mut total = weight;
        for (child, child_hitch) in self.hitches.iter().filter(|(_, child_hitch)| child_hitch.tow == trailer_id)
        {
            let load = self.hitch_load(*child, depth + 1);
            moment += load * (along(child_hitch.desc.point) - axles);
            total += load;
        }

        (moment / coupling).clamp(0.0, total)
    }

    // Couples every waiting trailer whose tow vehicle has a body - the ball where it is on the tow vehicle, the eye where that is on the trailer.
    fn ensure_hitches(&mut self)
    {
        for (trailer_id, hitch) in self.hitches.iter_mut()
        {
            if hitch.state == HitchState::Broken
            {
                continue;
            }

            let (Some(tow), Some(trailer)) = (self.vehicles.get(&hitch.tow), self.vehicles.get(trailer_id)) else
            {
                hitch.state = HitchState::Waiting;
                continue;
            };

            let alive = hitch.joint.and_then(|joint| self.impulse_joints.get(joint)).is_some_and(|joint| joint.body1() == tow.body && joint.body2() == trailer.body);
            if alive
            {
                continue;
            }

            hitch.joint = None;

            let (Some(tow_body), Some(trailer_body)) = (self.bodies.get(tow.body), self.bodies.get(trailer.body)) else
            {
                hitch.state = HitchState::Waiting;
                continue;
            };

            let ball = hitch.desc.point;
            let anchor = trailer_body.position().inverse_transform_point(tow_body.position().transform_point(ball));

            let mut joint = SphericalJointBuilder::new()
                .local_frame1(Pose::from_parts(ball, tow.frame_rotation()))
                .local_frame2(Pose::from_parts(anchor, trailer.frame_rotation()))
                .contacts_enabled(false);

            for (axis, limit) in [(JointAxis::AngX, hitch.desc.roll_limit), (JointAxis::AngY, hitch.desc.yaw_limit), (JointAxis::AngZ, hitch.desc.pitch_limit)]
            {
                if limit < std::f32::consts::PI
                {
                    let limit = limit.max(0.01);
                    joint = joint.limits(axis, [-limit, limit]);
                }
            }

            hitch.joint = Some(self.impulse_joints.insert(tow.body, trailer.body, joint.build(), true));
            hitch.anchor = Some(anchor);
            hitch.state = HitchState::Coupled;
            hitch.force = 0.0;
        }
    }

    // Reads force and angles of every coupling after a step and tears it off beyond its limits. A torn off pair collides again after a moment.
    fn check_hitches(&mut self, dt: f32)
    {
        let gravity = self.gravity();

        for (trailer_id, hitch) in self.hitches.iter_mut()
        {
            let Some(handle) = hitch.joint else { continue; };

            if hitch.state == HitchState::Broken
            {
                hitch.release_in -= dt;
                if hitch.release_in <= 0.0
                {
                    self.impulse_joints.remove(handle, true);
                    hitch.joint = None;
                }

                continue;
            }

            let Some(joint) = self.impulse_joints.get(handle) else { continue; };
            let (Some(tow), Some(trailer)) = (self.bodies.get(joint.body1()), self.bodies.get(joint.body2())) else { continue; };
            let Some(vehicle) = self.vehicles.get(trailer_id) else { continue; };

            // what moved the trailer besides its wheels, gravity and its own drag: the ball and whatever hit it - the joint impulses read back wrong
            if trailer.is_sleeping()
            {
                hitch.force = 0.0;
            }
            else
            {
                let mass = trailer.mass();
                let velocity = trailer.linvel() * (1.0 + dt * trailer.linear_damping());
                let others = vehicle.applied_wheel_impulse(dt) + (gravity * mass * trailer.gravity_scale() + trailer.user_force()) * dt;
                let force = ((velocity - vehicle.pre_velocity) * mass - others).length() / dt;

                hitch.force += (force - hitch.force) * dt / (dt + HITCH_FORCE_SMOOTHING);
            }

            // the trailer turned against the tow vehicle, measured like the joint limits: around the joint axes
            let relative = (*tow.rotation() * joint.data.local_frame1.rotation).inverse() * (*trailer.rotation() * joint.data.local_frame2.rotation);
            let relative = if relative.w < 0.0 { -relative } else { relative };
            let angle = |imaginary: f32| 2.0 * imaginary.atan2(relative.w);
            hitch.angles = Vector3::new(angle(relative.y), angle(relative.z), angle(relative.x));

            let torn = (hitch.desc.break_force > 0.0 && hitch.force > hitch.desc.break_force) || (hitch.desc.break_roll > 0.0 && hitch.angles.z.abs() > hitch.desc.break_roll);

            if torn
            {
                if let Some(joint) = self.impulse_joints.get_mut(handle, true)
                {
                    joint.data.set_enabled(false);
                }

                hitch.state = HitchState::Broken;
                hitch.release_in = HITCH_RELEASE_TIME;
            }
        }
    }

    // a new run couples everything again, where it stands
    fn reset_hitches(&mut self)
    {
        for hitch in self.hitches.values_mut()
        {
            if let Some(joint) = hitch.joint.take()
            {
                self.impulse_joints.remove(joint, true);
            }

            hitch.state = HitchState::Waiting;
            hitch.anchor = None;
            hitch.force = 0.0;
            hitch.brake = 0.0;
        }
    }

    // Mesh vertices of the given nodes, moved into a frame - e.g. the chassis space of a vehicle.
    pub fn collect_points(nodes: &[NodeItem], to_frame: &Matrix4<f32>) -> Vec<Vector3<f32>>
    {
        let mut points = vec![];

        for node in nodes
        {
            let instances = node.read().unwrap().instances.get_ref().clone();

            for instance in instances
            {
                let world = instance.read().unwrap().calculate_transform();

                if let Some((vertices, _)) = Self::collect_geometry(node, &(to_frame * world))
                {
                    points.extend(vertices.iter().map(|v| Vector3::new(v.x, v.y, v.z)));
                }
            }
        }

        points
    }

    // Undoes exclude_nodes - the next scan picks the nodes up again.
    pub fn include_nodes(&mut self, node_ids: &HashSet<u32>)
    {
        for node_id in node_ids
        {
            self.excluded_nodes.remove(node_id);
        }
    }
}

impl Default for PhysicsWorld
{
    fn default() -> Self
    {
        PhysicsWorld::new()
    }
}

#[cfg(test)]
mod tests
{
    use std::sync::{Arc, RwLock};

    use nalgebra::Point3;
    use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};

    use crate::helper::option_or_id::OptionOrId;
    use crate::state::resources::mesh_resource::MeshResource;
    use crate::state::scene::components::transformation::Transformation;
    use crate::state::scene::node::Node;

    use super::*;

    // a 20x20 ground plane at y = 0
    fn ground_node(y: f32) -> NodeItem
    {
        let resource = MeshResource::new_plane
        (
            "ground",
            Point3::new(-10.0, 0.0, -10.0),
            Point3::new( 10.0, 0.0, -10.0),
            Point3::new( 10.0, 0.0,  10.0),
            Point3::new(-10.0, 0.0,  10.0)
        );

        let mut mesh = Mesh::new("ground mesh");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let node = Node::new("ground");
        {
            let mut node_write = node.write().unwrap();
            node_write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            node_write.add_component(Arc::new(RwLock::new(Box::new(Transformation::new
            (
                "trans",
                Vector3::new(0.0, y, 0.0),
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0)
            )))));
        }

        // colliders are created per instance, so a node without one has nothing to collide
        node.write().unwrap().create_default_instance(node.clone());

        node
    }

    // large enough that a long walk never reaches the edge
    fn big_ground_node() -> NodeItem
    {
        let resource = MeshResource::new_plane
        (
            "ground",
            Point3::new(-400.0, 0.0, -400.0),
            Point3::new( 400.0, 0.0, -400.0),
            Point3::new( 400.0, 0.0,  400.0),
            Point3::new(-400.0, 0.0,  400.0)
        );

        let mut mesh = Mesh::new("ground mesh");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let node = Node::new("ground");
        {
            let mut node_write = node.write().unwrap();
            node_write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
        }

        node.write().unwrap().create_default_instance(node.clone());

        node
    }

    // Node::update refreshes the cached world transform every frame - the tests do not run
    // it, so they refresh it the same way it does.
    // Node::update does this for every node in the scene, children included, so the helper
    // has to recurse as well - a stale child cache would make the physics read a transform
    // the scene left behind long ago
    fn refresh_instance_cache(node: &NodeItem)
    {
        let instances: Vec<InstanceItemArc> = node.read().unwrap().instances.get_ref().clone();

        for instance in instances
        {
            let world_matrix = instance.read().unwrap().calculate_transform();
            instance.write().unwrap().get_data_mut().get_mut().computed.world_matrix = world_matrix;
        }

        let children: Vec<NodeItem> = node.read().unwrap().nodes.clone();

        for child in &children
        {
            refresh_instance_cache(child);
        }
    }

    // no default ground plane: the tests place their own geometry, and a second floor at
    // y = 0 would sit exactly on top of it
    fn test_world() -> PhysicsWorld
    {
        let mut world = PhysicsWorld::new();
        world.set_ground_plane(None);

        world
    }

    fn controller() -> KinematicCharacterController
    {
        let mut controller = KinematicCharacterController::default();
        controller.up = Vector::Y;
        controller.offset = CharacterLength::Absolute(0.01);
        controller.snap_to_ground = Some(CharacterLength::Absolute(0.2));

        controller
    }

    #[test]
    fn trimesh_collider_is_reachable_through_the_bvh()
    {
        let mut world = test_world();
        assert_eq!(world.add_node(ground_node(0.0)), 1);
        assert_eq!(world.collider_amount(), 1);

        // a capsule standing right above the plane must find ground when pushed down
        let capsule = Capsule::new_y(0.5, 0.3);
        let pos = Pose::from_translation(Vector::new(0.0, 0.9, 0.0));

        let queries = world.query_pipeline(QueryFilter::default());
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(res.grounded, "capsule should be grounded on the plane");
        assert!(res.translation.y > -0.1, "the shape cast should have stopped the fall, got {}", res.translation.y);
    }

    #[test]
    fn capsule_does_not_tunnel_and_settles_on_the_plane()
    {
        let mut world = test_world();
        world.add_node(ground_node(0.0));

        let capsule = Capsule::new_y(0.5, 0.3);
        let half = 0.5 + 0.3;

        // drop it from 5 units up and integrate gravity for 3 seconds
        let mut center_y = 5.0;
        let mut velocity = 0.0f32;
        let dt = 1.0 / 60.0;
        let mut grounded = false;

        for _ in 0..180
        {
            velocity -= 9.81 * dt;

            let pos = Pose::from_translation(Vector::new(0.0, center_y, 0.0));
            let queries = world.query_pipeline(QueryFilter::default());
            let res = controller().move_shape(dt, &queries, &capsule, &pos, Vector::new(0.0, velocity * dt, 0.0), |_| {});

            center_y += res.translation.y;
            grounded = res.grounded;

            if grounded
            {
                velocity = 0.0;
            }
        }

        assert!(grounded, "capsule never landed");
        assert!((center_y - half).abs() < 0.1, "capsule settled at {} instead of ~{}", center_y, half);
    }

    #[test]
    fn moving_the_node_moves_the_collider()
    {
        let mut world = test_world();
        world.set_ground_plane(None); // only the node under test may act as ground here

        let node = ground_node(0.0);
        world.add_node(node.clone());

        let capsule = Capsule::new_y(0.5, 0.3);
        let pos = Pose::from_translation(Vector::new(0.0, 0.9, 0.0));

        // lower the ground by 3 units - without a sync the capsule would still be grounded
        {
            let node_read = node.read().unwrap();
            let transformation = node_read.find_component::<Transformation>().unwrap();
            crate::component_downcast_mut!(transformation, Transformation);
            transformation.set_translation(Vector3::new(0.0, -3.0, 0.0));
        }

        refresh_instance_cache(&node);

        assert_eq!(world.sync_transformations(false), 1, "the moved node should be picked up");
        assert_eq!(world.sync_transformations(false), 0, "a second sync has nothing left to do");

        let queries = world.query_pipeline(QueryFilter::default());
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(!res.grounded, "ground moved away, the capsule must not be grounded any more");
    }

    #[test]
    fn the_query_predicate_can_exclude_a_node()
    {
        let mut world = test_world();
        let node = ground_node(0.0);
        let node_id = node.read().unwrap().id;
        world.add_node(node);

        let capsule = Capsule::new_y(0.5, 0.3);
        let pos = Pose::from_translation(Vector::new(0.0, 0.9, 0.0));

        let predicate = |_handle: ColliderHandle, collider: &Collider| -> bool
        {
            collider.user_data as u32 != node_id
        };

        let queries = world.query_pipeline(QueryFilter::default().predicate(&predicate));
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(!res.grounded, "the only collider was excluded, nothing should be hit");
    }


    // Mirrors the loop in CharacterController::update.
    fn simulate(world: &PhysicsWorld, start_y: f32, start_velocity: f32, forward: f32, frames: usize) -> (Vec<f32>, Vec<f32>, bool)
    {
        let capsule = Capsule::new_y(0.5, 0.3);
        let center_offset = 0.8;
        let dt: f32 = 1.0 / 60.0;

        let mut pos = Vector3::new(0.0f32, start_y, 0.0);
        let mut velocity = start_velocity;

        let mut heights = vec![];
        let mut forward_steps = vec![];
        let mut grounded = start_velocity <= 0.0;

        for _ in 0..frames
        {
            // gravity only in the air, exactly like CharacterController::update
            if !grounded
            {
                velocity -= 9.81 * dt;
            }

            let y_velocity_before = velocity;

            let desired = Vector::new(0.0, y_velocity_before * dt, forward);

            let mut char_controller = controller();
            if y_velocity_before > 0.0
            {
                char_controller.snap_to_ground = None;
            }

            let capsule_pos = Pose::from_translation(Vector::new(pos.x, pos.y + center_offset, pos.z));
            let queries = world.query_pipeline(QueryFilter::default());
            let res = char_controller.move_shape(dt, &queries, &capsule, &capsule_pos, desired, |_| {});

            pos.x += res.translation.x;
            pos.y += res.translation.y;
            pos.z += res.translation.z;

            // the fix under test: only a downward grounded contact counts as landed
            let landed = res.grounded && y_velocity_before <= 0.0;
            grounded = landed;

            if landed
            {
                velocity = 0.0;
            }

            heights.push(pos.y);
            forward_steps.push(res.translation.z);
        }

        (heights, forward_steps, grounded)
    }

    #[test]
    fn jump_actually_leaves_the_ground()
    {
        let mut world = test_world();
        world.add_node(big_ground_node());

        // jump_force 5.0 from a standing start - the whole arc takes about a second
        let (heights, _, grounded) = simulate(&world, 0.0, 5.0, 0.0, 90);

        let peak = heights.iter().cloned().fold(f32::MIN, f32::max);

        // 5.0^2 / (2 * 9.81) is about 1.27 - a peak near zero means the jump was cancelled
        assert!(peak > 1.0, "jump only reached {}, it was cancelled on the ground", peak);

        let last = *heights.last().unwrap();
        assert!(last < 0.1, "character never came back down: peak {} last {}", peak, last);
        assert!(grounded, "character should be grounded again after the jump");
    }

    #[test]
    fn walking_on_flat_ground_advances_evenly()
    {
        let mut world = test_world();
        world.add_node(big_ground_node());

        // running speed, long enough to expose the drift that used to drop frames
        let step = -0.12f32;
        let (_, forward_steps, grounded) = simulate(&world, 0.0, 0.0, step, 600);

        assert!(grounded, "character should stay grounded while walking");

        // gravity on a grounded character used to stall 16 of these 600 frames
        let worst = forward_steps.iter().map(|got| (got - step).abs()).fold(0.0f32, f32::max);
        assert!(worst < 0.01, "forward motion is not smooth, worst frame was off by {}", worst);
    }

    #[test]
    fn an_excluded_node_never_becomes_a_collider()
    {
        let mut world = test_world();
        let node = ground_node(0.0);
        let node_id = node.read().unwrap().id;

        world.add_node(node.clone());
        assert_eq!(world.collider_amount(), 1);

        let mut excluded = std::collections::HashSet::new();
        excluded.insert(node_id);
        world.exclude_nodes(&excluded);

        assert_eq!(world.collider_amount(), 0, "exclusion should drop the existing collider");
        assert_eq!(world.add_node(node), 0, "an excluded node must not be re-added");

        // and a full rebuild must not resurrect it either
        world.clear();
        assert!(world.is_excluded(node_id));
    }


    #[test]
    fn the_ground_plane_catches_a_character_in_an_empty_scene()
    {
        // a fresh world already carries a floor, the test helper strips it again
        assert!(!PhysicsWorld::new().is_empty(), "a ground plane is created by default");

        let mut world = test_world();
        assert!(world.is_empty());

        world.set_ground_plane(Some(0.0));
        assert!(!world.is_empty(), "a ground plane counts as content");

        // dropped from 5 units up with no scene geometry at all
        let (heights, _, grounded) = simulate(&world, 5.0, 0.0, 0.0, 180);

        assert!(grounded, "character fell through the ground plane");

        let last = *heights.last().unwrap();
        assert!(last.abs() < 0.1, "character settled at {} instead of the plane height", last);
    }

    #[test]
    fn the_ground_plane_follows_its_height_and_survives_a_rebuild()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(3.0));

        let (heights, _, grounded) = simulate(&world, 8.0, 0.0, 0.0, 180);
        assert!(grounded);
        assert!((heights.last().unwrap() - 3.0).abs() < 0.1, "settled at {} instead of 3.0", heights.last().unwrap());

        // a rebuild drops every node collider but must keep the configured floor
        world.clear();
        assert_eq!(world.ground_plane_y(), Some(3.0));

        let (heights, _, grounded) = simulate(&world, 8.0, 0.0, 0.0, 180);
        assert!(grounded, "ground plane was lost on rebuild");
        assert!((heights.last().unwrap() - 3.0).abs() < 0.1);

        world.set_ground_plane(None);
        assert!(world.is_empty());

        // without a floor the character just keeps falling
        let (heights, _, grounded) = simulate(&world, 0.0, 0.0, 0.0, 60);
        assert!(!grounded);
        assert!(*heights.last().unwrap() < -0.1, "should be falling, got {}", heights.last().unwrap());
    }

    #[test]
    fn an_object_loaded_after_the_build_becomes_solid()
    {
        let mut world = test_world();

        // a scene that only has the ground so far
        let ground = ground_node(0.0);
        let mut scene_nodes = vec![ground];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1);

        // now the user loads another object into the scene
        scene_nodes.push(ground_node(2.0));

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 1, "the newly loaded object should have become solid");
        assert_eq!(removed, 0);
        assert_eq!(world.collider_amount(), 2);

        // a second scan must not add it twice
        let (added, _) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 0);
        assert_eq!(world.collider_amount(), 2);

        // and the character now stands on the upper one instead of falling to the lower
        let (heights, _, grounded) = simulate(&world, 6.0, 0.0, 0.0, 180);
        assert!(grounded);
        assert!((heights.last().unwrap() - 2.0).abs() < 0.1, "settled at {} instead of the new object at 2.0", heights.last().unwrap());
    }

    #[test]
    fn turning_off_the_collider_flag_drops_the_collider()
    {
        let mut world = test_world();

        let node = ground_node(0.0);
        let scene_nodes = vec![node.clone()];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1);

        node.write().unwrap().settings.collision = false;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 0);
        assert_eq!(removed, 1, "the collider flag was turned off, the collider has to go");
        assert_eq!(world.collider_amount(), 0);
    }

    #[test]
    fn the_scan_only_runs_on_its_interval()
    {
        let mut world = test_world();

        assert!(world.scan_due(), "the first call should scan right away");

        // count the calls from one due scan to the next, the due call included
        let mut interval = 1;
        while !world.scan_due()
        {
            interval += 1;
            assert!(interval < 100, "scan never came due again");
        }

        assert_eq!(interval, NODE_SCAN_INTERVAL_FRAMES, "scans should be {} calls apart", NODE_SCAN_INTERVAL_FRAMES);

        // and it stays on that interval, it is not just the first one that fits
        let mut interval = 1;
        while !world.scan_due()
        {
            interval += 1;
        }

        assert_eq!(interval, NODE_SCAN_INTERVAL_FRAMES);
    }

    // Rides a platform the way CharacterController::update does.
    fn simulate_on_platform(world: &mut PhysicsWorld, platform: &NodeItem, platform_speed: f32, forward: f32, frames: usize, ride: bool) -> (Vec<f32>, bool)
    {
        let capsule = Capsule::new_y(0.5, 0.3);
        let center_offset = 0.8;
        let dt: f32 = 1.0 / 60.0;

        let mut pos = Vector3::new(0.0f32, 0.0, 0.0);
        let mut velocity = 0.0f32;
        let mut grounded = true;
        let mut ground_collider: Option<(ColliderHandle, Vector3<f32>)> = None;

        let mut gaps = vec![];
        let mut ever_fell_through = false;

        for _ in 0..frames
        {
            // the platform moves first, exactly like the animation step in Scene::update
            {
                let node = platform.read().unwrap();
                let transformation = node.find_component::<Transformation>().unwrap();
                crate::component_downcast_mut!(transformation, Transformation);
                transformation.apply_translation(Vector3::new(0.0, platform_speed, 0.0));
            }
            refresh_instance_cache(platform);
            world.sync_transformations(false);

            let mut platform_delta = Vector3::<f32>::zeros();
            if ride
            {
                if let Some((handle, last)) = ground_collider
                {
                    if let Some(current) = world.collider_translation(handle)
                    {
                        platform_delta = current - last;
                    }
                }
            }

            if !grounded { velocity -= 9.81 * dt; }
            let yv = velocity;

            let from = pos + platform_delta;
            let desired = Vector::new(0.0, yv * dt, forward);

            let mut c = controller();
            if yv > 0.0 { c.snap_to_ground = None; }

            let cpos = Pose::from_translation(Vector::new(from.x, from.y + center_offset, from.z));
            let queries = world.query_pipeline(QueryFilter::default());
            let res = c.move_shape(dt, &queries, &capsule, &cpos, desired, |_| {});

            pos = from + Vector3::new(res.translation.x, res.translation.y, res.translation.z);

            grounded = res.grounded && yv <= 0.0;
            if grounded { velocity = 0.0; }

            let platform_y = world.collider_translation(ground_collider.map(|g| g.0).unwrap_or(ColliderSet::invalid_handle())).map(|t| t.y);

            if grounded
            {
                let feet = Vector3::new(pos.x, pos.y + center_offset, pos.z);
                ground_collider = world.ground_collider_below(feet, center_offset + 0.3 + 0.2, QueryFilter::default());
            }
            else
            {
                ground_collider = None;
            }

            // how far the feet are from the platform surface
            if let Some(platform_y) = platform_y
            {
                let gap = pos.y - platform_y;
                gaps.push(gap);

                if gap < -0.2 { ever_fell_through = true; }
            }
        }

        (gaps, ever_fell_through)
    }

    #[test]
    fn walking_on_a_rising_platform_does_not_sink_into_it()
    {
        let mut world = test_world();

        // a platform the character starts on, no ground plane below it
        let platform = ground_node(0.0);
        world.add_node(platform.clone());

        // rises 3 units per second while the character walks across it
        let speed = 3.0 / 60.0;
        let (gaps, fell_through) = simulate_on_platform(&mut world, &platform, speed, -0.12, 120, true);

        assert!(!fell_through, "character fell through the rising platform");

        // the feet have to stay at a constant height above the platform surface
        let first = gaps[gaps.len() / 4];
        let worst = gaps.iter().skip(gaps.len() / 4).map(|g| (g - first).abs()).fold(0.0f32, f32::max);

        assert!(worst < 0.05, "character drifted {} relative to the platform surface", worst);
    }

    #[test]
    fn without_riding_the_character_sinks_into_a_rising_platform()
    {
        let mut world = test_world();
        let platform = ground_node(0.0);
        world.add_node(platform.clone());

        // same run with the platform delta ignored - this is what the bug looked like
        let speed = 3.0 / 60.0;
        let (gaps, _) = simulate_on_platform(&mut world, &platform, speed, -0.12, 120, false);

        let first = gaps[gaps.len() / 4];
        let worst = gaps.iter().skip(gaps.len() / 4).map(|g| (g - first).abs()).fold(0.0f32, f32::max);

        assert!(worst > 0.05, "expected the un-ridden character to drift, but it stayed put ({})", worst);
    }

    #[test]
    fn rotating_the_instance_moves_the_collider()
    {
        let mut world = test_world();
        world.set_ground_plane(None); // only the instance under test may act as ground here

        let node = ground_node(0.0);
        assert_eq!(world.add_node(node.clone()), 1, "the default instance should get a collider");

        let capsule = Capsule::new_y(0.5, 0.3);
        let pos = Pose::from_translation(Vector::new(0.0, 0.9, 0.0));

        // move the plane on the INSTANCE, not the node - this is how doors are animated
        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let mut instance = instance.write().unwrap();

            let transformation = Transformation::new
            (
                "instance trans",
                Vector3::new(0.0, -3.0, 0.0),
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0)
            );

            instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));
        }

        refresh_instance_cache(&node);

        assert_eq!(world.sync_transformations(false), 1, "an instance transform change has to be picked up");

        let queries = world.query_pipeline(QueryFilter::default());
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(!res.grounded, "the instance moved the ground away, the capsule must not be grounded");
    }

    #[test]
    fn an_instance_with_collision_off_gets_no_collider()
    {
        let mut world = test_world();

        let node = ground_node(0.0);
        let scene_nodes = vec![node.clone()];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1);

        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let mut instance = instance.write().unwrap();
            instance.get_data_mut().get_mut().collision = false;
        }

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 0);
        assert_eq!(removed, 1, "collision off on the instance has to drop its collider");
        assert_eq!(world.collider_amount(), 0);
    }


    #[test]
    fn standing_still_on_the_ground_plane_does_not_jitter()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let capsule = Capsule::new_y(0.5, 0.3);
        let dt: f32 = 1.0 / 60.0;

        // several spots, including the one the flicker was reported at
        let spots = [(0.837f32, -2.171f32), (0.0, 0.0), (13.77, 41.3), (-97.5, 6.25), (0.1, 0.1)];

        for (sx, sz) in spots
        {
            let mut pos = Vector3::new(sx, 0.0f32, sz);
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;

            for _ in 0..240
            {
                let cpos = Pose::from_translation(Vector::new(pos.x, pos.y + 0.8, pos.z));
                let queries = world.query_pipeline(QueryFilter::default());
                let res = controller().move_shape(dt, &queries, &capsule, &cpos, Vector::ZERO, |_| {});

                pos.y += res.translation.y;
                lo = lo.min(pos.y);
                hi = hi.max(pos.y);
            }

            // a flat 5000 by 1 cuboid used to swing the character over 6 cm here
            assert!(hi - lo < 0.001, "character height swung by {} at ({}, {})", hi - lo, sx, sz);
        }
    }

    #[test]
    fn walking_on_the_ground_plane_advances_evenly()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let step = -0.12f32;
        let (_, forward_steps, grounded) = simulate(&world, 0.0, 0.0, step, 600);

        assert!(grounded, "character should stay on the ground plane");

        // a few frames of 600 always deviate - this guards the two real failure modes
        let stalled = forward_steps.iter().filter(|got| (*got - step).abs() > 0.01).count();
        assert!(stalled * 100 < forward_steps.len(), "{} of {} frames stalled while walking on the ground plane", stalled, forward_steps.len());
    }



    #[test]
    fn a_slim_capsule_does_not_sink_into_the_floor_while_walking()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // real editor capsule - slimmer than the other tests, which exposed the snap bug
        let radius = 0.221f32;
        let half_height = 0.676f32;
        let center_offset = half_height + radius;

        let capsule = Capsule::new_y(half_height, radius);
        let dt: f32 = 1.0 / 60.0;

        let mut pos = Vector3::new(1.644f32, 0.0, 13.066);
        let mut lowest = f32::MAX;

        for _ in 0..300
        {
            let mut c = controller();
            c.snap_to_ground = Some(CharacterLength::Absolute(SNAP_TO_GROUND_LIMIT));
            c.autostep = Some(CharacterAutostep
            {
                max_height: CharacterLength::Absolute(0.3),
                min_width: CharacterLength::Absolute(0.15),
                include_dynamic_bodies: false
            });

            let cpos = Pose::from_translation(Vector::new(pos.x, pos.y + center_offset, pos.z));
            let queries = world.query_pipeline(QueryFilter::default());
            let res = c.move_shape(dt, &queries, &capsule, &cpos, Vector::new(0.0, 0.0, -0.12), |_| {});

            pos.x += res.translation.x; pos.y += res.translation.y; pos.z += res.translation.z;
            lowest = lowest.min(pos.y);
        }

        // a snap distance of 0.2 dragged this capsule to -0.105
        assert!(lowest > -0.005, "character sank to {} below the floor while walking", lowest);
    }

    // adds a quad collider straight into the world (no scene node needed)
    fn add_quad(world: &mut PhysicsWorld, a: Vector, b: Vector, c: Vector, d: Vector)
    {
        let shape = SharedShape::trimesh(vec![a, b, c, d], vec![[0u32, 1, 2], [0, 2, 3]]).unwrap();
        let handle = world.colliders.insert(ColliderBuilder::new(shape).build());
        world.refresh_leaf(handle);
    }

    // walks diagonally into a wall and reports how far along it the character got
    fn slide_along(world: &PhysicsWorld, start: Vector3<f32>, dir: Vector3<f32>, frames: usize) -> (f32, f32)
    {
        let capsule = Capsule::new_y(0.676, 0.221);
        let center_offset = 0.897f32;
        let dt: f32 = 1.0 / 60.0;

        let mut pos = start;
        let mut stuck_frames = 0;

        for _ in 0..frames
        {
            let mut c = controller();
            c.snap_to_ground = Some(CharacterLength::Absolute(SNAP_TO_GROUND_LIMIT));

            let cpos = Pose::from_translation(Vector::new(pos.x, pos.y + center_offset, pos.z));
            let queries = world.query_pipeline(QueryFilter::default());
            let res = c.move_shape(dt, &queries, &capsule, &cpos, Vector::new(dir.x, dir.y, dir.z), |_| {});

            let moved = (res.translation.x * res.translation.x + res.translation.z * res.translation.z).sqrt();
            if moved < 0.001 { stuck_frames += 1; }

            pos.x += res.translation.x; pos.y += res.translation.y; pos.z += res.translation.z;
        }

        (pos.x - start.x, stuck_frames as f32)
    }

    #[test]
    fn sliding_works_along_a_wall_and_along_a_flush_panel()
    {
        let wall = |w: &mut PhysicsWorld|
        {
            add_quad(w, Vector::new(-10.0, 0.0, 0.0), Vector::new(10.0, 0.0, 0.0), Vector::new(10.0, 4.0, 0.0), Vector::new(-10.0, 4.0, 0.0));
        };

        // walking mostly along the wall while pressing into it
        let dir = Vector3::new(0.10f32, 0.0, 0.04);
        let start = Vector3::new(-6.705f32, 0.0, -1.0);

        {
            let mut world = test_world();
            world.set_ground_plane(Some(0.0));
            wall(&mut world);

            let (along, stuck) = slide_along(&world, start, dir, 200);
            assert!(along > 15.0, "character only slid {} along a plain wall", along);
            assert!(stuck < 5.0, "character stuck for {} frames on a plain wall", stuck);
        }

        {
            // a panel mounted flat against the wall must not change anything
            let mut world = test_world();
            world.set_ground_plane(Some(0.0));
            wall(&mut world);
            add_quad(&mut world, Vector::new(-3.0, 0.5, -0.1), Vector::new(3.0, 0.5, -0.1), Vector::new(3.0, 3.0, -0.1), Vector::new(-3.0, 3.0, -0.1));

            let (along, stuck) = slide_along(&world, start, dir, 200);
            assert!(along > 15.0, "character only slid {} along a wall with a flush panel", along);
            assert!(stuck < 5.0, "character stuck for {} frames on a flush panel", stuck);
        }
    }

    #[test]
    fn turning_collision_off_on_a_parent_disables_the_children()
    {
        // an object root with the mesh on a child, which is how loaded assets are shaped
        let root = Node::new("object root");
        let child = ground_node(0.0);
        Node::add_node(root.clone(), child.clone());

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1, "the child mesh should start out collidable");

        // the user turns collision off on the root, not on the mesh node
        root.write().unwrap().settings.collision = false;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 0);
        assert_eq!(removed, 1, "collision off on the parent has to disable the child mesh");
        assert_eq!(world.collider_amount(), 0);

        // and back on again
        root.write().unwrap().settings.collision = true;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(added, 1, "re-enabling on the parent has to bring the collider back");
        assert_eq!(removed, 0);
    }


    // a 1x1x1 box mesh, the usual dynamic prop
    // 2000 boxes falling onto each other: ms per frame for the step and the write back
    #[test]
    #[ignore]
    fn bench_many_bodies()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.set_running(true);

        let mut nodes = vec![];
        for i in 0..2000
        {
            let node = box_node(2.0 + (i / 400) as f32 * 1.3, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
            {
                let node_read = node.read().unwrap();
                let instance = node_read.instances.get_ref().first().unwrap().clone();
                let transformation = instance.read().unwrap().find_component::<Transformation>().unwrap();
                component_downcast_mut!(transformation, Transformation);
                transformation.set_translation(Vector3::new((i % 20) as f32 * 1.3 + (i / 400) as f32 * 0.4, 2.0 + (i / 400) as f32 * 1.3, ((i / 20) % 20) as f32 * 1.3));
            }
            refresh_instance_cache(&node);
            nodes.push(node);
        }

        let (mut step, mut apply, mut frames) = (0.0, 0.0, 0);
        for index in 0..180
        {
            if world.auto_add_nodes && world.scan_due() { world.scan_nodes(&nodes); }
            for node in &nodes { refresh_instance_cache(node); }
            world.sync_transformations(false);

            let started = std::time::Instant::now();
            world.step(1.0 / 60.0, false);
            let stepped = started.elapsed().as_secs_f64();

            let started = std::time::Instant::now();
            world.apply_dynamic_bodies(false);
            let applied = started.elapsed().as_secs_f64();

            if index >= 30
            {
                step += stepped; apply += applied; frames += 1;
            }
        }

        let awake = world.islands.active_bodies().count();
        println!("2000 boxes, {} still awake at the end: step {:.2} ms, write back {:.2} ms per frame", awake, step / frames as f64 * 1000.0, apply / frames as f64 * 1000.0);
    }

    fn box_node(y: f32, body_type: PhysicsBodyType, shape: PhysicsShape) -> NodeItem
    {
        let h = 0.5f32;
        let v = vec!
        [
            Point3::new(-h, -h, -h), Point3::new(h, -h, -h), Point3::new(h, h, -h), Point3::new(-h, h, -h),
            Point3::new(-h, -h,  h), Point3::new(h, -h,  h), Point3::new(h, h,  h), Point3::new(-h, h,  h),
        ];
        let i = vec!
        [
            [0u32,2,1],[0,3,2], [4,5,6],[4,6,7], [0,1,5],[0,5,4],
            [3,7,6],[3,6,2], [0,4,7],[0,7,3], [1,2,6],[1,6,5],
        ];

        let resource = MeshResource::new_with_data("box", v, i, vec![], vec![], vec![], vec![]);

        let mut mesh = Mesh::new("box mesh");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let node = Node::new("box");
        {
            let mut node_write = node.write().unwrap();
            node_write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            node_write.settings.physics.body_type = body_type;
            node_write.settings.physics.shape = shape;
        }

        node.write().unwrap().create_default_instance(node.clone());

        // the instance carries the transform, like everything else in the scene
        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let transformation = Transformation::new("trans", Vector3::new(0.0, y, 0.0), Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
            instance.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transformation))));
        }

        refresh_instance_cache(&node);

        node
    }

    fn instance_y(node: &NodeItem) -> f32
    {
        let node_read = node.read().unwrap();
        let instance = node_read.instances.get_ref().first().unwrap().clone();
        let instance = instance.read().unwrap();

        // computed live: apply_dynamic_bodies writes the transform, Node::update would be
        // the one to refresh the cache from it
        extract_translation_from_transform(&instance.calculate_transform()).y
    }

    #[test]
    fn a_dynamic_box_falls_and_comes_to_rest_on_the_ground()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        assert_eq!(world.add_node(node.clone()), 1);
        assert_eq!(world.body_amount(), 1, "a dynamic object needs a rigid body");
        assert!(world.has_dynamics());

        let start = instance_y(&node);
        assert!((start - 4.0).abs() < 0.001, "box should start at 4.0, got {}", start);

        world.set_running(true);

        for _ in 0..300
        {
            world.step(1.0 / 60.0, false);
            world.apply_dynamic_bodies(false);
        }

        // half the box height above the floor, give or take the solver tolerance
        let resting = instance_y(&node);
        assert!((resting - 0.5).abs() < 0.06, "box came to rest at {} instead of 0.5", resting);
    }

    #[test]
    fn a_static_object_is_never_moved_by_the_solver()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // floating in the air, and it has to stay there
        let node = box_node(4.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        world.add_node(node.clone());

        assert_eq!(world.body_amount(), 0, "a static object must not get a rigid body");
        assert!(!world.has_dynamics(), "a purely static world must not step at all");

        world.set_running(true);

        for _ in 0..120
        {
            assert_eq!(world.step(1.0 / 60.0, false), 0, "nothing to simulate, so nothing should step");
            world.apply_dynamic_bodies(false);
        }

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "static box moved to {}", instance_y(&node));
    }

    #[test]
    fn auto_picks_a_trimesh_for_static_and_a_hull_for_dynamic()
    {
        assert_eq!(PhysicsWorld::effective_shape(PhysicsBodyType::Static, PhysicsShape::Auto), PhysicsShape::TriMesh);
        assert_eq!(PhysicsWorld::effective_shape(PhysicsBodyType::Dynamic, PhysicsShape::Auto), PhysicsShape::ConvexHull);
        assert_eq!(PhysicsWorld::effective_shape(PhysicsBodyType::Kinematic, PhysicsShape::Auto), PhysicsShape::ConvexHull);

        // an explicit choice is never overridden
        assert_eq!(PhysicsWorld::effective_shape(PhysicsBodyType::Dynamic, PhysicsShape::Box), PhysicsShape::Box);
        assert_eq!(PhysicsWorld::effective_shape(PhysicsBodyType::Static, PhysicsShape::Sphere), PhysicsShape::Sphere);
    }

    #[test]
    fn the_fixed_timestep_does_not_depend_on_the_frame_rate()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.add_node(box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto));

        world.set_running(true);

        // one long frame must not turn into an unbounded burst of catch up steps
        let steps = world.step(10.0, false);
        assert!(steps <= world.settings.max_substeps, "{} steps for a 10 second hitch", steps);

        // and a frame shorter than the step accumulates instead of stepping
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.add_node(box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto));

        world.set_running(true);

        assert_eq!(world.step(1.0 / 240.0, false), 0, "a quarter step should not simulate yet");
        assert_eq!(world.step(1.0 / 240.0, false), 0);
        assert_eq!(world.step(1.0 / 240.0, false), 0);
        assert_eq!(world.step(1.0 / 240.0, false), 1, "four quarter steps make one full step");
    }

    #[test]
    fn switching_a_node_to_dynamic_rebuilds_its_collider()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1);
        assert_eq!(world.body_amount(), 0, "static gets no body");

        // this is what flipping the combo box in the editor does
        node.write().unwrap().settings.physics.body_type = PhysicsBodyType::Dynamic;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!(removed, 1, "the static collider has to be dropped");
        assert_eq!(added, 1, "and rebuilt as a dynamic one in the same scan");
        assert_eq!(world.body_amount(), 1, "dynamic needs a rigid body");

        // and it actually falls now
        world.set_running(true);

        for _ in 0..300
        {
            world.step(1.0 / 60.0, false);
            world.apply_dynamic_bodies(false);
        }

        assert!((instance_y(&node) - 0.5).abs() < 0.06, "box came to rest at {}", instance_y(&node));

        // back to static: the body goes away and it stops moving
        node.write().unwrap().settings.physics.body_type = PhysicsBodyType::Static;
        world.scan_nodes(&scene_nodes);

        assert_eq!(world.body_amount(), 0, "the rigid body has to be released again");
    }

    #[test]
    fn changing_the_shape_alone_also_rebuilds()
    {
        let mut world = test_world();
        let node = box_node(0.0, PhysicsBodyType::Dynamic, PhysicsShape::ConvexHull);
        let scene_nodes = vec![node.clone()];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.collider_amount(), 1);

        node.write().unwrap().settings.physics.shape = PhysicsShape::Sphere;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!((added, removed), (1, 1), "a shape change has to rebuild the collider");

        // an unchanged scan does nothing, so a slider drag does not thrash the world
        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!((added, removed), (0, 0));
    }

    #[test]
    fn a_body_type_set_on_a_parent_reaches_the_mesh_below_it()
    {
        // objects load as a root with the mesh underneath, and the editor sets the root
        let root = Node::new("crate root");
        let mesh = box_node(4.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        Node::add_node(root.clone(), mesh.clone());

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.build_from_nodes(&scene_nodes);

        assert_eq!(world.body_amount(), 0, "static so far");

        root.write().unwrap().settings.physics.body_type = PhysicsBodyType::Dynamic;

        world.scan_nodes(&scene_nodes);
        assert_eq!(world.body_amount(), 1, "the setting on the root has to reach the mesh child");
    }

    #[test]
    fn leaving_run_mode_puts_dynamic_objects_back()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        world.add_node(node.clone());

        world.set_running(false); // the editor is not in try mode

        // not running: nothing simulates, so editing is never disturbed
        for _ in 0..60
        {
            assert_eq!(world.step(1.0 / 60.0, false), 0, "a world that is not running must not step");
            world.apply_dynamic_bodies(false);
        }

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "box moved while not running");

        world.set_running(true);

        for _ in 0..300
        {
            world.step(1.0 / 60.0, false);
            world.apply_dynamic_bodies(false);
        }

        assert!(instance_y(&node) < 1.0, "box should have fallen in run mode");

        world.set_running(false);

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "box should be back at 4.0, got {}", instance_y(&node));

        // and a second run starts from the authored position again
        world.set_running(true);
        world.step(1.0 / 60.0, false);
        world.apply_dynamic_bodies(false);

        assert!(instance_y(&node) > 3.9, "the second run must start from the top again");
    }

    // Mirrors what Scene::update does per frame, so the test exercises the real order.
    fn frame(world: &mut PhysicsWorld, scene_nodes: &Vec<NodeItem>)
    {
        frame_frozen(world, scene_nodes, false);
    }

    fn frame_frozen(world: &mut PhysicsWorld, scene_nodes: &Vec<NodeItem>, frozen: bool)
    {
        if world.auto_add_nodes && world.scan_due()
        {
            world.scan_nodes(scene_nodes);
        }

        for node in scene_nodes
        {
            refresh_instance_cache(node);
        }

        world.sync_transformations(frozen);
        world.step(1.0 / 60.0, frozen);
        world.apply_dynamic_bodies(frozen);
    }

    #[test]
    fn pressing_play_on_an_untouched_scene_makes_the_box_fall()
    {
        // exactly the editor flow: nothing built yet, the type is set on the object root,
        // then play is pressed
        let root = Node::new("cube top");
        let mesh = box_node(4.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        Node::add_node(root.clone(), mesh.clone());

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        assert!(world.is_empty() || world.collider_amount() == 0, "nothing is built up front");

        root.write().unwrap().settings.physics.body_type = PhysicsBodyType::Dynamic;

        // play - the colliders do not exist yet at this point
        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.body_amount(), 1, "the world has to build itself while running");
        assert!((instance_y(&mesh) - 0.5).abs() < 0.06, "box came to rest at {} instead of 0.5", instance_y(&mesh));

        // back to edit mode
        world.set_running(false);

        assert!((instance_y(&mesh) - 4.0).abs() < 0.001, "box should be back at 4.0, got {}", instance_y(&mesh));
    }

    #[test]
    fn a_default_instance_without_a_transformation_still_falls()
    {
        // create_default_instance adds no transformation component at all - this is what an
        // object added in the editor actually looks like, and the write back had nowhere to
        // go, so the body fell in rapier while nothing moved on screen
        let root = Node::new("cube top");

        let h = 0.5f32;
        let v = vec!
        [
            Point3::new(-h, -h, -h), Point3::new(h, -h, -h), Point3::new(h, h, -h), Point3::new(-h, h, -h),
            Point3::new(-h, -h,  h), Point3::new(h, -h,  h), Point3::new(h, h,  h), Point3::new(-h, h,  h),
        ];
        let i = vec!
        [
            [0u32,2,1],[0,3,2], [4,5,6],[4,6,7], [0,1,5],[0,5,4],
            [3,7,6],[3,6,2], [0,4,7],[0,7,3], [1,2,6],[1,6,5],
        ];

        let resource = MeshResource::new_with_data("box", v, i, vec![], vec![], vec![], vec![]);
        let mut mesh_component = Mesh::new("box mesh");
        mesh_component.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let mesh = Node::new("cube");
        {
            let mut mesh_write = mesh.write().unwrap();
            mesh_write.add_component(Arc::new(RwLock::new(Box::new(mesh_component))));
            // the height sits on the NODE, like the editor gizmo would set it
            mesh_write.add_component(Arc::new(RwLock::new(Box::new(Transformation::new("trans", Vector3::new(0.0, 4.0, 0.0), Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0))))));
        }
        mesh.write().unwrap().create_default_instance(mesh.clone());

        Node::add_node(root.clone(), mesh.clone());
        root.write().unwrap().settings.physics.body_type = PhysicsBodyType::Dynamic;

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.body_amount(), 1, "the mesh should have become a dynamic body");
        assert!((instance_y(&mesh) - 0.5).abs() < 0.06, "box came to rest at {} instead of 0.5", instance_y(&mesh));

        world.set_running(false);
        assert!((instance_y(&mesh) - 4.0).abs() < 0.001, "box should be back at 4.0, got {}", instance_y(&mesh));
    }

    #[test]
    fn an_object_can_be_shot_into_the_scene()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(2.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        node.write().unwrap().settings.physics.linear_velocity = Vector3::new(8.0, 0.0, 0.0);

        let scene_nodes = vec![node.clone()];
        world.set_running(true);

        for _ in 0..120
        {
            frame(&mut world, &scene_nodes);
        }

        let travelled = {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let instance = instance.read().unwrap();
            extract_translation_from_transform(&instance.calculate_transform()).x
        };

        assert!(travelled > 3.0, "the box should have been shot sideways, moved {}", travelled);

        // a second run has to start with the same shot, not from where it landed
        world.set_running(false);
        world.set_running(true);
        frame(&mut world, &scene_nodes);

        let restart = {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let instance = instance.read().unwrap();
            extract_translation_from_transform(&instance.calculate_transform()).x
        };

        assert!(restart.abs() < 0.5, "the second run must start at the authored spot, got {}", restart);
    }

    #[test]
    fn a_low_center_of_mass_keeps_an_object_upright()
    {
        // same box twice, once with the weight at the bottom
        let run = |low_com: bool| -> f32
        {
            let mut world = test_world();
            world.set_ground_plane(Some(0.0));

            let node = box_node(1.5, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
            {
                let mut node_write = node.write().unwrap();
                node_write.settings.physics.angular_velocity = Vector3::new(0.0, 0.0, 3.0);

                if low_com
                {
                    node_write.settings.physics.center_of_mass_auto = false;
                    node_write.settings.physics.center_of_mass = Vector3::new(0.0, -0.45, 0.0);
                }
            }

            let scene_nodes = vec![node.clone()];
            world.set_running(true);

            for _ in 0..400
            {
                frame(&mut world, &scene_nodes);
            }

            // how far the local up axis still points up
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let instance = instance.read().unwrap();
            let transform = instance.calculate_transform();

            transform[(1, 1)]
        };

        let auto_com = run(false);
        let low_com = run(true);

        assert!(low_com > auto_com, "a low centre of mass should keep it more upright ({} vs {})", low_com, auto_com);
    }

    // moves the instance the way the gizmo does: write a new local transform and refresh
    // the cache the scene would refresh during its update
    fn move_instance_to_y(node: &NodeItem, y: f32)
    {
        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let instance = instance.read().unwrap();

            let transformation = instance.find_component::<Transformation>().unwrap();
            component_downcast_mut!(transformation, Transformation);
            transformation.set_translation(Vector3::new(0.0, y, 0.0));
        }

        refresh_instance_cache(node);
    }

    #[test]
    fn a_dynamic_object_can_be_moved_while_the_world_is_not_running()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        // edit mode: the world is built, but nothing simulates
        world.set_running(false);

        for _ in 0..10
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "the box must not move at all while editing, it is at {}", instance_y(&node));

        // the author drags it up with the gizmo
        move_instance_to_y(&node, 6.0);

        for _ in 0..60
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&node) - 6.0).abs() < 0.001, "the box snapped back to {} instead of staying where it was put", instance_y(&node));

        // and pressing play starts from there, not from where the body used to be - the scene shows the bodies a step behind, so two frames
        world.set_running(true);

        frame(&mut world, &scene_nodes);
        frame(&mut world, &scene_nodes);

        assert!(instance_y(&node) < 6.0, "the box should start falling from its new spot");
        assert!(instance_y(&node) > 5.5, "the box jumped back to its old spot at {} when the run started", instance_y(&node));

        // leaving the run puts it back where the author left it, not where it was built
        world.set_running(false);

        assert!((instance_y(&node) - 6.0).abs() < 0.001, "leaving the run has to restore the authored spot, got {}", instance_y(&node));
    }

    #[test]
    fn a_dynamic_object_can_be_moved_while_the_run_is_frozen()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        world.set_running(true);

        for _ in 0..30
        {
            frame(&mut world, &scene_nodes);
        }

        let fallen_to = instance_y(&node);
        assert!(fallen_to < 3.9, "the box should have started falling, at {}", fallen_to);

        // pause, then drag it back up with the gizmo
        for _ in 0..5
        {
            frame_frozen(&mut world, &scene_nodes, true);
        }

        // the body sits on the node, which fell with it - the drag lifts the instance from 4 to 6 above that
        move_instance_to_y(&node, 6.0);
        let put = instance_y(&node);
        assert!((put - fallen_to - 2.0).abs() < 0.001, "the drag should lift the box by 2 from {}, it is at {}", fallen_to, put);

        for _ in 0..60
        {
            frame_frozen(&mut world, &scene_nodes, true);
        }

        assert!((instance_y(&node) - put).abs() < 0.001, "the box went to {} instead of staying where it was put ({}) while frozen", instance_y(&node), put);

        // resuming carries on from the new spot
        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&node) - 0.5).abs() < 0.06, "the box should have landed, at {}", instance_y(&node));
    }

    #[test]
    fn moving_while_frozen_is_undone_by_leaving_the_run()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        // play: this is the one moment the authored spot is recorded
        world.set_running(true);

        for _ in 0..30
        {
            frame(&mut world, &scene_nodes);
        }

        // pause, move, resume - twice, the way a user pokes at a falling object
        for target in [6.0f32, 8.0f32]
        {
            for _ in 0..3
            {
                frame_frozen(&mut world, &scene_nodes, true);
            }

            move_instance_to_y(&node, target);

            for _ in 0..3
            {
                frame_frozen(&mut world, &scene_nodes, true);
            }

            for _ in 0..20
            {
                frame(&mut world, &scene_nodes);
            }
        }

        // stop has to land on the authored spot, not on anything dragged in between
        world.set_running(false);

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "stop landed at {} instead of the authored 4.0", instance_y(&node));
    }

    #[test]
    fn an_object_pushed_through_the_ground_plane_is_recovered()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // dragged straight through the floor while editing, which a surface cannot stop
        let node = box_node(-2.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        world.set_running(true);

        for _ in 0..600
        {
            frame(&mut world, &scene_nodes);
        }

        assert!(instance_y(&node) > -0.1, "the box kept falling below the floor, it is at {}", instance_y(&node));
    }

    #[test]
    fn an_object_sunk_into_the_floor_settles_on_it_at_a_high_frame_rate()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // sunk into the floor, so the recovery has something to correct
        let node = box_node(-0.5, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        world.set_running(true);

        // the editor runs far faster than the solver, so most frames carry no step at all -
        // the recovery must not add anything up across those
        for _ in 0..3000
        {
            if world.auto_add_nodes && world.scan_due()
            {
                world.scan_nodes(&scene_nodes);
            }

            refresh_instance_cache(&node);
            world.sync_transformations(false);
            world.step(1.0 / 600.0, false);
            world.apply_dynamic_bodies(false);
        }

        let resting = instance_y(&node);

        assert!((resting - 0.5).abs() < 0.06, "the object ended up at {} instead of resting at 0.5", resting);
    }

    fn move_instance_to(node: &NodeItem, x: f32, y: f32, z: f32)
    {
        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let instance = instance.read().unwrap();

            let transformation = instance.find_component::<Transformation>().unwrap();
            component_downcast_mut!(transformation, Transformation);
            transformation.set_translation(Vector3::new(x, y, z));
        }

        refresh_instance_cache(node);
    }

    fn body_speed(world: &PhysicsWorld, node: &NodeItem) -> f32
    {
        let node_id = node.read().unwrap().id;

        world.body_of(node_id)
            .and_then(|handle| world.bodies.get(handle))
            .map(|body| body.linvel().length())
            .unwrap_or(0.0)
    }

    #[test]
    fn a_kinematic_pusher_does_not_launch_what_it_touches()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // the bed: resting on the floor, nothing else acting on it
        let target = box_node(0.5, PhysicsBodyType::Dynamic, PhysicsShape::Auto);

        // the car: dragged along z by the gizmo, one editor frame at a time
        let pusher = box_node(0.5, PhysicsBodyType::Kinematic, PhysicsShape::Auto);
        move_instance_to(&pusher, 0.0, 0.5, 3.0);

        let scene_nodes = vec![target.clone(), pusher.clone()];

        world.set_running(true);

        for _ in 0..60
        {
            frame(&mut world, &scene_nodes);
        }

        // a slow nudge at editor frame rate: the editor runs far faster than the solver, so
        // most frames set a new kinematic target that no step ever consumes
        let mut z = 3.0f32;
        let mut fastest: f32 = 0.0;

        for _ in 0..750
        {
            z -= 0.004;
            move_instance_to(&pusher, 0.0, 0.5, z);

            if world.auto_add_nodes && world.scan_due()
            {
                world.scan_nodes(&scene_nodes);
            }

            for node in &scene_nodes
            {
                refresh_instance_cache(node);
            }

            world.sync_transformations(false);
            world.step(1.0 / 300.0, false);
            world.apply_dynamic_bodies(false);

            fastest = fastest.max(body_speed(&world, &target));
        }

        assert!(fastest < 5.0, "a 1.2 m/s nudge accelerated the target to {} m/s", fastest);
    }

    fn instance_world_position(node: &NodeItem) -> Vector3<f32>
    {
        let node_read = node.read().unwrap();
        let instance = node_read.instances.get_ref().first().unwrap().clone();
        let instance = instance.read().unwrap();

        extract_translation_from_transform(&instance.calculate_transform())
    }

    #[test]
    fn a_rotating_body_under_a_non_uniformly_scaled_parent_does_not_jump()
    {
        // exactly the shape of a loaded asset: an object root that carries a rotation and a
        // very uneven scale, with the mesh on a child. Expressing a rotation below that needs
        // shear, which a position/rotation/scale triple cannot store.
        let root = Node::new("object root");
        {
            let mut root_write = root.write().unwrap();
            let transformation = Transformation::new("trans", Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0), Vector3::new(1.04, 0.076, 0.75));
            root_write.add_component(Arc::new(RwLock::new(Box::new(transformation))));
        }

        let mesh = box_node(6.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);

        // spinning, so the body rotation and the parent rotation never line up
        mesh.write().unwrap().settings.physics.angular_velocity = Vector3::new(0.0, 0.0, 3.0);

        Node::add_node(root.clone(), mesh.clone());

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.set_running(true);

        let mut previous = instance_world_position(&mesh);
        let mut largest_jump: f32 = 0.0;

        for _ in 0..600
        {
            frame(&mut world, &scene_nodes);

            let current = instance_world_position(&mesh);
            largest_jump = largest_jump.max((current - previous).norm());
            previous = current;
        }

        assert!(largest_jump < 0.5, "the body jumped {} in a single frame, so the scene and the solver disagree about where it is", largest_jump);
    }

    // a tall narrow prop standing on its base, the proportions of a bowling pin
    fn pin_node(shape: PhysicsShape, x: f32, z: f32) -> NodeItem
    {
        // the measured silhouette of the asset: a 4 cm base ring, widest at 6.2 cm just
        // above it, tapering to a narrow neck - about 51 cm tall
        let profile = [(0.0f32, 0.040f32), (0.115, 0.0616), (0.330, 0.028), (0.430, 0.034), (0.5142, 0.020)];
        let segments = 16u32;

        let mut v = vec![];
        for (level, radius) in profile
        {
            for i in 0..segments
            {
                let a = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
                v.push(Point3::new(radius * a.cos(), level, radius * a.sin()));
            }
        }

        let mut i = vec![];
        let rings = profile.len() as u32;

        for ring in 0..rings - 1
        {
            for k in 0..segments
            {
                let n = (k + 1) % segments;
                let a = ring * segments;
                let b = (ring + 1) * segments;

                i.push([a + k, a + n, b + n]);
                i.push([a + k, b + n, b + k]);
            }
        }

        // caps
        for k in 1..segments - 1
        {
            i.push([0, k + 1, k]);
            let top = (rings - 1) * segments;
            i.push([top, top + k, top + k + 1]);
        }

        let resource = MeshResource::new_with_data("pin", v, i, vec![], vec![], vec![], vec![]);

        let mut mesh = Mesh::new("pin mesh");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let node = Node::new("bowling pin");
        {
            let mut node_write = node.write().unwrap();
            node_write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            node_write.settings.physics.body_type = PhysicsBodyType::Dynamic;
            node_write.settings.physics.shape = shape;
        }

        node.write().unwrap().create_default_instance(node.clone());

        {
            let node_read = node.read().unwrap();
            let instance = node_read.instances.get_ref().first().unwrap().clone();
            let transformation = Transformation::new("trans", Vector3::new(x, 0.0, z), Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
            instance.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transformation))));
        }

        refresh_instance_cache(&node);

        node
    }

    fn upright_amount(node: &NodeItem) -> f32
    {
        let node_read = node.read().unwrap();
        let instance = node_read.instances.get_ref().first().unwrap().clone();
        let instance = instance.read().unwrap();

        let transform = instance.calculate_transform();
        let up = transform * nalgebra::Vector4::new(0.0, 1.0, 0.0, 0.0);

        up.y
    }

    // the up axis as the solver itself sees it, so a tip in the physics can be told apart
    // from one that only the scene transform picked up
    fn body_upright(world: &PhysicsWorld, node: &NodeItem) -> f32
    {
        let node_id = node.read().unwrap().id;

        world.body_of(node_id)
            .and_then(|handle| world.bodies.get(handle))
            .map(|body| (body.position().rotation * Vector::new(0.0, 1.0, 0.0)).y)
            .unwrap_or(1.0)
    }

    #[test]
    fn a_tall_narrow_prop_stays_standing_on_the_ground_plane()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = pin_node(PhysicsShape::Auto, 2.2, -2.2);
        let scene_nodes = vec![node.clone()];

        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert!(upright_amount(&node) > 0.98, "the pin tipped over - scene up {}, solver up {}", upright_amount(&node), body_upright(&world, &node));
    }

    #[test]
    fn a_rack_of_tall_narrow_props_stays_standing()
    {
        // the exact bowling formation from the physics test project: ten pins, closest pair
        // 15.6 cm apart, widest diameter 12.3 cm - so about 3 cm of air between them
        let positions =
        [
            (2.0066f32, -2.0068f32), (2.1628, -2.0082), (2.3255, -2.0150), (2.4846, -2.0107),
            (2.0795, -2.1669), (2.2434, -2.1606), (2.4171, -2.1703),
            (2.1407, -2.3329), (2.3271, -2.3206),
            (2.2255, -2.4716),
        ];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let pins: Vec<NodeItem> = positions.iter().map(|(x, z)| pin_node(PhysicsShape::Auto, *x, *z)).collect();
        let scene_nodes = pins.clone();

        // A gentle nudge, the kind a settling neighbour or a passing avatar produces. It
        // carries a few percent of the energy needed to tip a pin, so all ten have to rock
        // and settle - if any of them goes over, something is adding energy.
        for pin in &pins
        {
            pin.write().unwrap().settings.physics.angular_velocity = Vector3::new(0.3, 0.0, 0.0);
        }

        world.set_running(true);

        for _ in 0..600
        {
            frame(&mut world, &scene_nodes);
        }

        let toppled = pins.iter().filter(|pin| upright_amount(pin) <= 0.98).count();

        assert_eq!(toppled, 0, "{} of {} pins fell over on their own", toppled, pins.len());
    }

    #[test]
    fn pausing_freezes_the_simulation_without_resetting_it()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let node = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![node.clone()];

        world.set_running(true);

        for _ in 0..30
        {
            frame(&mut world, &scene_nodes);
        }

        let fallen_to = instance_y(&node);
        assert!(fallen_to < 3.9, "the box should have started falling, at {}", fallen_to);

        // freeze: nothing simulates, and nothing is put back either
        for _ in 0..120
        {
            assert_eq!(world.step(1.0 / 60.0, true), 0, "a frozen world must not step");
            frame_frozen(&mut world, &scene_nodes, true);
        }

        assert!((instance_y(&node) - fallen_to).abs() < 0.001, "the box moved while frozen, {} vs {}", instance_y(&node), fallen_to);

        // and it carries on from there
        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&node) - 0.5).abs() < 0.06, "the box should have landed, at {}", instance_y(&node));

        // leaving the running mode still resets
        world.set_running(false);

        assert!((instance_y(&node) - 4.0).abs() < 0.001, "the box should be back at 4.0");
    }

    // ********** combined objects **********

    // A root without a mesh that carries the height, two boxes below it side by side - the
    // shape of a loaded asset once the author marks the root as one object.
    fn combined_object(y: f32, body_type: PhysicsBodyType) -> (NodeItem, NodeItem, NodeItem)
    {
        let root = Node::new("combined root");
        {
            let transformation = Transformation::new("trans", Vector3::new(0.0, y, 0.0), Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));

            let mut root_write = root.write().unwrap();
            root_write.add_component(Arc::new(RwLock::new(Box::new(transformation))));
            root_write.settings.physics.body_type = body_type;
            root_write.settings.physics.combine_children = true;
        }

        let left = box_node(0.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        let right = box_node(0.0, PhysicsBodyType::Static, PhysicsShape::Auto);

        Node::add_node(root.clone(), left.clone());
        Node::add_node(root.clone(), right.clone());

        move_instance_to(&left, -1.0, 0.0, 0.0);
        move_instance_to(&right, 1.0, 0.0, 0.0);

        refresh_instance_cache(&root);

        (root, left, right)
    }

    fn node_world_position(node: &NodeItem) -> Vector3<f32>
    {
        extract_translation_from_transform(&node.read().unwrap().get_full_transform())
    }

    fn set_node_position(node: &NodeItem, position: Vector3<f32>)
    {
        {
            let node_read = node.read().unwrap();
            let transformation = node_read.find_component::<Transformation>().unwrap();

            component_downcast_mut!(transformation, Transformation);
            transformation.set_translation(position);
        }

        refresh_instance_cache(node);
    }

    #[test]
    fn a_combined_object_falls_as_one_body()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let (root, left, right) = combined_object(4.0, PhysicsBodyType::Dynamic);
        let scene_nodes = vec![root.clone()];

        world.build_from_nodes(&scene_nodes);

        assert_eq!(world.body_amount(), 1, "two meshes, one body");
        assert_eq!(world.collider_amount(), 2, "one collider per mesh");

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        let root_y = node_world_position(&root).y;
        assert!((root_y - 0.5).abs() < 0.06, "the root should rest with the boxes on the floor, got y = {}", root_y);

        // the root moved, the meshes below it did not - they kept their offsets
        let left_position = instance_world_position(&left);
        let right_position = instance_world_position(&right);

        assert!((left_position.x + 1.0).abs() < 0.01 && (right_position.x - 1.0).abs() < 0.01, "the halves drifted: {} / {}", left_position.x, right_position.x);
        assert!((left_position.y - right_position.y).abs() < 0.01, "the halves must not tilt against each other");
        assert!((left_position.y - root_y).abs() < 0.01, "the meshes have to follow the root, mesh at {} root at {}", left_position.y, root_y);
    }

    #[test]
    fn a_combined_object_can_be_moved_while_editing_and_falls_from_there()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.set_running(false);

        let (root, _, _) = combined_object(4.0, PhysicsBodyType::Dynamic);
        let scene_nodes = vec![root.clone()];

        frame(&mut world, &scene_nodes);
        assert_eq!(world.body_amount(), 1);

        // the gizmo moves the root while nothing simulates
        set_node_position(&root, Vector3::new(3.0, 6.0, 0.0));
        frame(&mut world, &scene_nodes);

        {
            let root_id = root.read().unwrap().id;
            let body = world.bodies.get(world.body_of(root_id).unwrap()).unwrap();
            let translation = body.translation();

            assert!((translation.x - 3.0).abs() < 0.001 && (translation.y - 6.0).abs() < 0.001, "the body has to follow the root while editing, sits at {:?}", translation);
        }

        assert!((node_world_position(&root).y - 6.0).abs() < 0.001, "nothing may write back while not running");

        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        let position = node_world_position(&root);
        assert!((position.x - 3.0).abs() < 0.01, "it should fall straight down from where it was put, got x = {}", position.x);
        assert!((position.y - 0.5).abs() < 0.06, "and come to rest on the floor, got y = {}", position.y);

        // leaving the run puts it back where the author left it
        world.set_running(false);

        let position = node_world_position(&root);
        assert!((position.x - 3.0).abs() < 0.001 && (position.y - 6.0).abs() < 0.001, "leaving the run must restore the root, got {:?}", position);
    }

    // The real frame order: the scene refreshes the caches before the solver writes, and
    // the renderer reads them right after - nothing in between refreshes them again.
    #[test]
    fn the_meshes_below_a_combined_root_follow_it_in_the_render_cache()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let (root, left, _) = combined_object(4.0, PhysicsBodyType::Dynamic);
        let scene_nodes = vec![root.clone()];

        world.build_from_nodes(&scene_nodes);

        for _ in 0..120
        {
            world.sync_transformations(false);
            world.step(1.0 / 60.0, false);
            world.apply_dynamic_bodies(false);
        }

        let root_y = node_world_position(&root).y;
        assert!(root_y < 3.0, "the root should have fallen, at {}", root_y);

        let cached = left.read().unwrap().instances.get_ref()[0].read().unwrap().get_cached_world_transform();
        let cached_y = cached[(1, 3)];

        assert!((cached_y - root_y).abs() < 0.001, "what the renderer reads has to follow the root: cached {} root {}", cached_y, root_y);
    }

    #[test]
    fn a_kinematic_combined_object_follows_its_root()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let (root, _, _) = combined_object(1.0, PhysicsBodyType::Kinematic);
        let scene_nodes = vec![root.clone()];

        frame(&mut world, &scene_nodes);

        set_node_position(&root, Vector3::new(2.0, 1.0, 0.0));

        // the next kinematic position is applied by the step
        frame(&mut world, &scene_nodes);
        frame(&mut world, &scene_nodes);

        let root_id = root.read().unwrap().id;
        let body = world.bodies.get(world.body_of(root_id).unwrap()).unwrap();
        assert!((body.translation().x - 2.0).abs() < 0.001, "a kinematic body has to follow the root, sits at x = {}", body.translation().x);
    }

    #[test]
    fn combining_and_separating_rebuilds_the_bodies()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let (root, _, _) = combined_object(4.0, PhysicsBodyType::Dynamic);
        root.write().unwrap().settings.physics.combine_children = false;

        let scene_nodes = vec![root.clone()];

        world.build_from_nodes(&scene_nodes);
        assert_eq!(world.body_amount(), 2, "without the flag every mesh is a body of its own");
        assert_eq!(world.combined_amount(), 0);

        root.write().unwrap().settings.physics.combine_children = true;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!((added, removed), (2, 2), "the two singles go, two parts come");
        assert_eq!(world.body_amount(), 1);
        assert_eq!(world.collider_amount(), 2);

        // a settings change alone does not rebuild
        root.write().unwrap().settings.physics.friction = 0.2;

        let (added, removed) = world.scan_nodes(&scene_nodes);
        assert_eq!((added, removed), (0, 0), "friction is adjusted in place");

        root.write().unwrap().settings.physics.combine_children = false;

        world.scan_nodes(&scene_nodes);
        assert_eq!(world.body_amount(), 2, "separated again");
        assert_eq!(world.collider_amount(), 2);
        assert_eq!(world.combined_amount(), 0);
    }

    // ********** reacting on a hit **********

    fn waiting_box(y: f32) -> NodeItem
    {
        let node = box_node(y, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        node.write().unwrap().settings.physics.react_on_first_hit = true;

        node
    }

    #[test]
    fn an_object_that_reacts_on_its_first_hit_holds_still_until_something_hits_it()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // in the air on purpose: a waiting object does not even fall
        let waiting = waiting_box(3.0);
        let scene_nodes = vec![waiting.clone()];

        world.set_running(true);

        for _ in 0..120
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 1);
        assert!((instance_y(&waiting) - 3.0).abs() < 0.001, "the waiting box moved to {}", instance_y(&waiting));

        // a box dropped from well above lands on it at several units per second
        let dropped = box_node(6.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![waiting.clone(), dropped.clone()];

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 0, "the hit should have released the box");
        assert!(instance_y(&waiting) < 1.0, "the released box should have fallen, it is at {}", instance_y(&waiting));
    }

    #[test]
    fn an_object_resting_on_it_from_the_start_is_not_a_hit()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let waiting = waiting_box(0.5);
        let resting = box_node(1.5, PhysicsBodyType::Dynamic, PhysicsShape::Auto); // exactly on top of it
        let scene_nodes = vec![waiting.clone(), resting.clone()];

        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 1, "resting weight must not count as a hit");
        assert!((instance_y(&resting) - 1.5).abs() < 0.06, "the resting box should stay on top, it is at {}", instance_y(&resting));
    }

    #[test]
    fn a_release_takes_every_waiting_object_it_touches_with_it()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        // two waiting boxes side by side with their faces touching, and one further away
        let left = waiting_box(0.5);
        let right = waiting_box(0.5);
        let apart = waiting_box(0.5);
        move_instance_to(&right, 1.0, 0.5, 0.0);
        move_instance_to(&apart, 3.0, 0.5, 0.0);

        let dropped = box_node(4.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);

        let scene_nodes = vec![left.clone(), right.clone(), apart.clone(), dropped.clone()];

        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 1, "the hit on the left box should release the right one with it and leave the one apart waiting");

        let apart_id = apart.read().unwrap().id;
        assert!(world.entries().iter().find(|entry| entry.node_id() == apart_id).unwrap().waiting);
    }

    #[test]
    fn leaving_the_run_puts_a_released_object_back_to_waiting()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let waiting = waiting_box(3.0);
        let dropped = box_node(6.0, PhysicsBodyType::Dynamic, PhysicsShape::Auto);
        let scene_nodes = vec![waiting.clone(), dropped.clone()];

        world.set_running(true);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 0);
        assert!(instance_y(&waiting) < 1.0);

        world.set_running(false);

        assert_eq!(world.waiting_amount(), 1, "every run starts waiting again");
        assert!((instance_y(&waiting) - 3.0).abs() < 0.001, "the box should be back where the author put it, it is at {}", instance_y(&waiting));

        // and it holds still there on the next run, until the dropped box lands again
        world.set_running(true);

        for _ in 0..30
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&waiting) - 3.0).abs() < 0.001, "the box should hold still on the second run, it is at {}", instance_y(&waiting));
    }

    #[test]
    fn the_flag_on_an_object_root_reaches_the_meshes_below_it()
    {
        // the shape of a loaded fractured object: the root carries the settings, every
        // piece is a mesh below it with a body of its own
        let root = Node::new("object root");
        {
            let mut root_write = root.write().unwrap();
            root_write.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("trans")))));
            root_write.settings.physics.body_type = PhysicsBodyType::Dynamic;
            root_write.settings.physics.combine_children = false;
            root_write.settings.physics.react_on_first_hit = true;
        }

        let piece = box_node(3.0, PhysicsBodyType::Static, PhysicsShape::Auto);
        Node::add_node(root.clone(), piece.clone());

        let scene_nodes = vec![root.clone()];

        let mut world = test_world();
        world.set_ground_plane(Some(0.0));
        world.set_running(true);

        for _ in 0..120
        {
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 1, "the piece below the root should wait");
        assert!((instance_y(&piece) - 3.0).abs() < 0.001, "the piece should hold still in the air, it is at {}", instance_y(&piece));
    }

    #[test]
    fn a_moving_kinematic_body_releases_what_it_runs_into()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let waiting = waiting_box(0.5);
        let pusher = box_node(0.5, PhysicsBodyType::Kinematic, PhysicsShape::Auto);
        move_instance_to(&pusher, -3.0, 0.5, 0.0);

        let scene_nodes = vec![waiting.clone(), pusher.clone()];

        world.set_running(true);

        let mut x = -3.0f32;

        for _ in 0..300
        {
            // driven from the scene at 2 units per second, into the box and a bit beyond
            x += 2.0 / 60.0;
            move_instance_to(&pusher, x.min(-0.9), 0.5, 0.0);
            frame(&mut world, &scene_nodes);
        }

        assert_eq!(world.waiting_amount(), 0, "the kinematic pusher should have released the box");
    }

    #[test]
    fn a_hit_reported_from_outside_releases_the_object()
    {
        let mut world = test_world();
        world.set_ground_plane(Some(0.0));

        let waiting = waiting_box(3.0);
        let scene_nodes = vec![waiting.clone()];

        world.set_running(true);
        frame(&mut world, &scene_nodes);

        // what the character controller reports
        let handle = world.entries()[0].parts[0].handle;
        world.hit_collider(handle, f32::INFINITY);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&waiting) - 0.5).abs() < 0.06, "the box should have fallen to the ground, it is at {}", instance_y(&waiting));
    }
}

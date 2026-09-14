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

use crate::{component_downcast, component_downcast_mut, console_warning, helper::math::{extract_rotation_quat_from_transform, extract_scale_from_transform, extract_translation_from_transform}, state::{scene::{components::{component::ComponentItem, mesh::Mesh, transformation::Transformation}, node::{InstanceItemArc, Node, NodeItem, PhysicsBodyType, PhysicsSettings, PhysicsShape}, scene::Scene}}};

// transform deltas below this are treated as float noise and do not trigger a bvh update
const TRANSFORM_EPSILON: f32 = 0.00001;

// a scale change needs a shape rebuild, so it uses a slightly more forgiving threshold
const SCALE_EPSILON: f32 = 0.0001;

// Above this ratio between the largest and smallest scale above a body, a rotating rigid
// body stretches enough to be obvious.
const NON_UNIFORM_SCALE_LIMIT: f32 = 1.5;

// Nothing in a normal scene moves this fast or jumps this far in a single step, so either
// is worth reporting once.
const IMPLAUSIBLE_SPEED: f32 = 50.0;
const IMPLAUSIBLE_JUMP: f32 = 2.0;

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

    // A body falls asleep once it stays below both thresholds for this long, and a sleeping
    // body costs nothing and stops wobbling. The defaults are rapier's and assume meters.
    #[serde(default = "default_sleep_linear_threshold")]
    pub sleep_linear_threshold: f32,
    #[serde(default = "default_sleep_angular_threshold")]
    pub sleep_angular_threshold: f32,
    #[serde(default = "default_time_until_sleep")]
    pub time_until_sleep: f32,

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
            Anchor::Instance { instance, .. } => Some(instance.read().unwrap().calculate_transform()),
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
            ccd_solver: CCDSolver::new(),

            settings: PhysicsWorldSettings::default(),

            time_accumulator: 0.0,
            body_amount: 0,
            run_steps: 0,
            moved_kinematics: HashSet::new(),
            pre_step_speed: HashMap::new(),
            running: true, // the editor turns this off, a game build just runs
            snapshot_pending: false,
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
        self.bodies = RigidBodySet::new();
        self.colliders = ColliderSet::new();
        self.broad_phase_bvh = BroadPhaseBvh::new();
        self.islands = IslandManager::new();
        self.narrow_phase = NarrowPhase::new();
        self.impulse_joints = ImpulseJointSet::new();
        self.multibody_joints = MultibodyJointSet::new();
        self.time_accumulator = 0.0;
        self.body_amount = 0;

        self.entries.clear();
        self.ground_plane = None;
        self.applied_ground_plane = None;
        self.applied_sleep = None;
        // excluded_nodes is kept on purpose - a rebuild must not resurrect character colliders

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
            self.colliders.remove(handle, &mut self.islands, &mut self.bodies, false);
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
        let collider = ColliderBuilder::new(shape).position(pose).build();
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

                body.set_position(pose, true);
                body.set_linvel(Vector::ZERO, true);
                body.set_angvel(Vector::ZERO, true);
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
        self.body_amount > 0
    }

    pub fn is_empty(&self) -> bool
    {
        self.entries.is_empty() && self.ground_plane.is_none()
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

    fn translation_of(transform: &Matrix4<f32>) -> Vector
    {
        Vector::new(transform[(0, 3)], transform[(1, 3)], transform[(2, 3)])
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
    fn part_collider(shape: SharedShape, offset: &Pose, anchor_pose: &Pose, physics: &PhysicsSettings, node_id: u32, instance_id: u32, attached: bool) -> Collider
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

            let collider = Self::part_collider(shape, &offset, &anchor_pose, &physics, node_id, instance_id, body.is_some());

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
        self.colliders.remove(handle, &mut self.islands, &mut self.bodies, false);
    }

    fn remove_bodies(&mut self, bodies: &Vec<RigidBodyHandle>)
    {
        for body in bodies
        {
            self.bodies.remove(*body, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, true);
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

            let Some(collider) = self.colliders.get_mut(handle) else { continue; };

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

        let Some(handle) = self.entries[index].body else { return; };
        let Some(body) = self.bodies.get_mut(handle) else { return; };

        let body_type = if waiting { RigidBodyType::Fixed } else { RigidBodyType::Dynamic };

        if body.body_type() == body_type
        {
            return;
        }

        body.set_body_type(body_type, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);

        if !waiting
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
                    Ok(distance) => if distance <= RELEASE_TOUCH_DISTANCE { return true; },
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
            if entry.body_type != PhysicsBodyType::Dynamic || entry.waiting
            {
                continue;
            }

            let Some(handle) = entry.body else { continue; };
            let Some(body) = self.bodies.get(handle) else { continue; };

            self.pre_step_speed.insert(handle, body.linvel().length());
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
        let mut hit: Vec<usize> = vec![];

        for index in 0..self.entries.len()
        {
            if !self.entries[index].waiting
            {
                continue;
            }

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
                        self.pre_step_speed.get(&other_body).copied().unwrap_or(0.0) > hit_speed
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
            self.release_entry(index);
        }
    }

    // A hit the narrow phase never sees: the character is a shape cast, not a body.
    pub fn hit_collider(&mut self, handle: ColliderHandle)
    {
        let Some(index) = self.entries.iter().position(|entry| entry.waiting && entry.parts.iter().any(|part| part.handle == handle)) else { return; };

        self.release_entry(index);
    }

    // Every run starts as authored: what reacts on a hit waits, everything else runs.
    fn reset_waiting(&mut self)
    {
        for index in 0..self.entries.len()
        {
            let waiting = self.entries[index].reacts_on_hit;
            self.set_waiting(index, waiting);
        }
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
                    body.set_position(pose, true);
                    body.set_linvel(Vector::ZERO, true);
                    body.set_angvel(Vector::ZERO, true);
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
    }

    // Advances the solver in fixed steps. The frame time is not constant, and feeding a
    // varying dt into a solver makes it behave differently at different frame rates.
    // `frozen` stops the stepping without restoring anything, unlike leaving the run mode.
    pub fn step(&mut self, delta_t: f32, frozen: bool) -> u32
    {
        if !self.has_dynamics() || !self.running
        {
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

        let gravity = Vector::new(self.settings.gravity.x, self.settings.gravity.y, self.settings.gravity.z);

        if self.time_accumulator >= self.settings.fixed_timestep
        {
            self.record_speeds();
        }

        while self.time_accumulator >= self.settings.fixed_timestep
        {
            self.time_accumulator -= self.settings.fixed_timestep;
            steps += 1;

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
                &mut self.ccd_solver,
                &(),
                &()
            );
        }

        if steps > 0
        {
            self.run_steps = self.run_steps.saturating_add(steps);
            self.release_hit_bodies();
        }

        self.recover_escaped_bodies();

        steps
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


            let (anchor_pose, fresh_scale) = Self::split_transform(&anchor_world);

            // The scale only replaces the stored one when it really changed. It is read
            // back out of a rotating matrix, and the float noise in it would otherwise
            // reach every part and look like an edit on each of them.
            let scale_changed = Self::scale_differs(&fresh_scale, &self.entries[index].scale);
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
                                body.set_position(anchor_pose, true);
                                body.set_linvel(Vector::ZERO, true);
                                body.set_angvel(Vector::ZERO, true);
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

        self.last_synced = updated;
        self.last_shape_rebuilds = rebuilds;

        updated
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

        for index in 0..self.entries.len()
        {
            // a waiting body sits exactly where the scene put it, nothing to write
            if self.entries[index].body_type != PhysicsBodyType::Dynamic || self.entries[index].waiting
            {
                continue;
            }

            let Some(body) = self.entries[index].body else { continue; };
            let Some(body) = self.bodies.get(body) else { continue; };

            if body.is_sleeping()
            {
                continue;
            }

            let pose = *body.position();

            // A degenerate shape or a zero mass can still make the solver produce NaN. Once
            // that reaches a transform it spreads through every derived value and takes the
            // renderer down with it, so it stops here.
            if !pose.translation.is_finite() || !pose.rotation.is_finite()
            {
                continue;
            }

            // An object that suddenly leaves at an absurd speed is worth naming, and the
            // two possible causes look different here: a real speed means the solver did it,
            // a big jump at a small speed means something teleported the body.
            {
                let speed = body.linvel().length();
                let jump = (pose.translation - Self::translation_of(&self.entries[index].transform)).length();

                if speed > IMPLAUSIBLE_SPEED || jump > IMPLAUSIBLE_JUMP
                {
                    let (node_id, _) = self.entries[index].key;
                    let name = self.entries[index].anchor.name();

                    Self::warn_once(node_id, format!("physics: '{}' left at {:.1} units/s after a {:.2} unit jump - a high speed points at the solver resolving a deep overlap, a big jump at a small speed points at a teleport", name, speed, jump));
                }
            }

            let anchor = self.entries[index].anchor.clone();

            let Some(shown) = anchor.write_back(&pose) else { continue; };

            self.entries[index].transform = shown;
            applied += 1;
        }

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
    }

    // Every collider and character capsule in world space, for the debug view.
    pub fn debug_volumes(&self) -> Vec<PhysicsDebugVolume>
    {
        let mut volumes = vec![];

        for entry in &self.entries
        {
            let body = entry.body.and_then(|handle| self.bodies.get(handle));

            let state = match entry.body_type
            {
                PhysicsBodyType::Static => PhysicsDebugState::Static,
                PhysicsBodyType::Kinematic => PhysicsDebugState::Kinematic,
                PhysicsBodyType::Dynamic if entry.waiting => PhysicsDebugState::Waiting,
                PhysicsBodyType::Dynamic if body.is_some_and(|body| body.is_sleeping()) => PhysicsDebugState::Sleeping,
                PhysicsBodyType::Dynamic => PhysicsDebugState::Dynamic,
            };

            for part in &entry.parts
            {
                let Some(collider) = self.colliders.get(part.handle) else { continue; };

                // an attached collider only catches up with its body in a step, and nothing steps while editing
                let pose = match (body, collider.position_wrt_parent())
                {
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

        // and pressing play starts from there, not from where the body used to be
        world.set_running(true);

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

        move_instance_to_y(&node, 6.0);

        for _ in 0..60
        {
            frame_frozen(&mut world, &scene_nodes, true);
        }

        assert!((instance_y(&node) - 6.0).abs() < 0.001, "the box snapped back to {} instead of staying where it was put while frozen", instance_y(&node));

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
        world.hit_collider(handle);

        for _ in 0..300
        {
            frame(&mut world, &scene_nodes);
        }

        assert!((instance_y(&waiting) - 0.5).abs() < 0.06, "the box should have fallen to the ground, it is at {}", instance_y(&waiting));
    }
}

#![allow(dead_code)]

use std::collections::HashSet;

use nalgebra::{Matrix4, Vector3};
use parry3d::query::DefaultQueryDispatcher;
use rapier3d::prelude::*;

use crate::{component_downcast, helper::math::{extract_rotation_quat_from_transform, extract_scale_from_transform, extract_translation_from_transform}, state::{scene::{components::mesh::Mesh, node::{InstanceItemArc, NodeItem}, scene::Scene}}};

// transform deltas below this are treated as float noise and do not trigger a bvh update
const TRANSFORM_EPSILON: f32 = 0.00001;

// a scale change needs a shape rebuild, so it uses a slightly more forgiving threshold
const SCALE_EPSILON: f32 = 0.0001;

// Half size of the ground plane quad. A flat cuboid jittered the character over 6 cm.
const GROUND_PLANE_HALF_SIZE: f32 = 500.0;

// Largest safe snap distance - 0.08 already buries a slim capsule 7 cm in the floor.
pub const SNAP_TO_GROUND_LIMIT: f32 = 0.03;

// Rescan interval for new or removed mesh instances - every frame would be wasteful.
const NODE_SCAN_INTERVAL_FRAMES: u32 = 10;

// One collider per mesh instance - doors and the like are animated on the instance.
pub struct ColliderEntry
{
    pub node: NodeItem,
    pub node_id: u32,

    pub instance: InstanceItemArc,
    pub instance_id: u32,

    pub handle: ColliderHandle,

    transform: Matrix4<f32>, // the world transform

    // the scale is baked into the shape - only a scale change forces a shape rebuild
    scale: Vector3<f32>,
}

// Query-only collision world: no physics step, static trimeshes mirrored from the scene.
pub struct PhysicsWorld
{
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub broad_phase_bvh: BroadPhaseBvh,
    pub integration_params: IntegrationParameters,

    islands: IslandManager,
    dispatcher: DefaultQueryDispatcher,

    entries: Vec<ColliderEntry>,

    // endless floor - the editor grid cannot serve as one, it is rebuilt on every change
    ground_plane: Option<ColliderHandle>,
    ground_plane_y: Option<f32>,

    // pick up objects loaded after the world was built, and drop disabled ones again
    pub auto_add_nodes: bool,
    scan_countdown: u32,

    // last sync result, so the editor can show whether anything is still being rebuilt per frame
    pub last_synced: usize,
    pub last_shape_rebuilds: usize,

    // never collidable, by node id - characters belong here, their skin re-syncs every frame
    excluded_nodes: HashSet<u32>,
}

impl PhysicsWorld
{
    pub fn new() -> PhysicsWorld
    {
        PhysicsWorld
        {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            broad_phase_bvh: BroadPhaseBvh::new(),
            integration_params: IntegrationParameters::default(),

            islands: IslandManager::new(),
            dispatcher: DefaultQueryDispatcher,

            entries: vec![],
            ground_plane: None,
            ground_plane_y: None,
            auto_add_nodes: true,
            scan_countdown: 0,
            last_synced: 0,
            last_shape_rebuilds: 0,
            excluded_nodes: HashSet::new(),
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

        self.entries.clear();
        self.ground_plane = None;
        // excluded_nodes is kept on purpose - a rebuild must not resurrect character colliders

        // the ground plane is configuration, not scene content, so it survives a rebuild
        let ground_plane_y = self.ground_plane_y;
        self.set_ground_plane(ground_plane_y);
    }

    // Places an endless floor at the given height, or removes it with None.
    pub fn set_ground_plane(&mut self, y: Option<f32>)
    {
        if let Some(handle) = self.ground_plane.take()
        {
            self.colliders.remove(handle, &mut self.islands, &mut self.bodies, false);
            self.rebuild_bvh();
        }

        self.ground_plane_y = y;

        let Some(y) = y else { return; };

        let half = GROUND_PLANE_HALF_SIZE;

        // same vertex order MeshResource::new_plane produces for a floor
        let vertices = vec!
        [
            Vector::new(-half, 0.0, -half),
            Vector::new( half, 0.0, -half),
            Vector::new( half, 0.0,  half),
            Vector::new(-half, 0.0,  half),
        ];

        let Ok(shape) = SharedShape::trimesh(vertices, vec![[0, 1, 2], [0, 2, 3]]) else { return; };

        let pose = Pose::from_translation(Vector::new(0.0, y, 0.0));
        let collider = ColliderBuilder::new(shape).position(pose).build();
        let handle = self.colliders.insert(collider);

        self.ground_plane = Some(handle);
        self.refresh_leaf(handle);
    }

    pub fn ground_plane_y(&self) -> Option<f32>
    {
        self.ground_plane_y
    }

    pub fn collider_amount(&self) -> usize
    {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool
    {
        self.entries.is_empty() && self.ground_plane.is_none()
    }

    pub fn entries(&self) -> &Vec<ColliderEntry>
    {
        &self.entries
    }

    pub fn has_node(&self, node_id: u32) -> bool
    {
        self.entries.iter().any(|entry| entry.node_id == node_id)
    }

    pub fn has_instance(&self, node_id: u32, instance_id: u32) -> bool
    {
        self.entries.iter().any(|entry| entry.node_id == node_id && entry.instance_id == instance_id)
    }

    // ********** transform helpers **********

    // splits a node transform into a rigid pose (for the collider) and a scale (baked into the shape)
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

    fn scale_differs(a: &Vector3<f32>, b: &Vector3<f32>) -> bool
    {
        (a.x - b.x).abs() > SCALE_EPSILON || (a.y - b.y).abs() > SCALE_EPSILON || (a.z - b.z).abs() > SCALE_EPSILON
    }

    // reads the node mesh and bakes the scale into the vertices
    fn build_shape(node: &NodeItem, scale: &Vector3<f32>) -> Option<SharedShape>
    {
        let node = node.read().unwrap();
        let mesh = node.find_component::<Mesh>()?;

        component_downcast!(mesh, Mesh);

        let mesh_resource = mesh.mesh_resource.as_ref()?;
        let mesh_resource = mesh_resource.read().unwrap();
        let data = mesh_resource.get_data();

        if data.vertices.is_empty() || data.indices.is_empty()
        {
            return None;
        }

        let vertices: Vec<Vector> = data.vertices.iter().map(|v|
        {
            Vector::new(v.x * scale.x, v.y * scale.y, v.z * scale.z)
        }).collect();

        SharedShape::trimesh(vertices, data.indices.clone()).ok()
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

    // Adds a static trimesh collider for one mesh instance.
    pub fn add_instance(&mut self, node: NodeItem, instance: InstanceItemArc) -> Option<ColliderHandle>
    {
        let node_id = node.read().unwrap().id;
        let instance_id = instance.read().unwrap().id;

        if self.has_instance(node_id, instance_id) || self.is_excluded(node_id)
        {
            return None;
        }

        // computed directly, a brand new instance has no cached transform yet
        let transform = instance.read().unwrap().calculate_transform();
        let (pose, scale) = Self::split_transform(&transform);

        let shape = Self::build_shape(&node, &scale)?;

        let collider = ColliderBuilder::new(shape)
            .position(pose)
            .user_data(Self::pack_user_data(node_id, instance_id))
            .build();

        let handle = self.colliders.insert(collider);
        self.refresh_leaf(handle);

        self.entries.push(ColliderEntry
        {
            node,
            node_id,
            instance,
            instance_id,
            handle,
            transform,
            scale,
        });

        Some(handle)
    }

    // Adds every collidable instance of a node. Returns how many colliders were created.
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

    fn pack_user_data(node_id: u32, instance_id: u32) -> u128
    {
        (node_id as u128) | ((instance_id as u128) << 32)
    }

    // Removes every collider belonging to a node, across all its instances.
    pub fn remove_node(&mut self, node_id: u32) -> bool
    {
        let handles: Vec<ColliderHandle> = self.entries.iter()
            .filter(|entry| entry.node_id == node_id)
            .map(|entry| entry.handle)
            .collect();

        if handles.is_empty()
        {
            return false;
        }

        self.entries.retain(|entry| entry.node_id != node_id);

        for handle in handles
        {
            self.colliders.remove(handle, &mut self.islands, &mut self.bodies, false);
        }

        // the arena slots can be reused, so rebuild rather than patch single leaves
        self.rebuild_bvh();

        true
    }

    // Drops every collider and rebuilds from the given nodes. Returns the collider count.
    pub fn build_from_nodes(&mut self, nodes: &Vec<NodeItem>) -> usize
    {
        self.clear();
        self.scan_nodes(nodes);

        self.entries.len()
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

    // Reconciles the collider set with the scene. Returns (added, removed).
    pub fn scan_nodes(&mut self, nodes: &Vec<NodeItem>) -> (usize, usize)
    {
        let all_nodes = Scene::list_all_child_nodes_with_mesh(nodes);

        // everything that should have a collider right now, as (node id, instance id)
        let mut wanted: HashSet<(u32, u32)> = HashSet::new();
        let mut added = 0;

        for node in &all_nodes
        {
            if !Self::is_collidable(node)
            {
                continue;
            }

            let node_id = node.read().unwrap().id;
            let instances: Vec<InstanceItemArc> = node.read().unwrap().instances.get_ref().clone();

            for instance in instances
            {
                if !Self::is_collidable_instance(&instance)
                {
                    continue;
                }

                wanted.insert((node_id, instance.read().unwrap().id));

                if self.add_instance(node.clone(), instance).is_some()
                {
                    added += 1;
                }
            }
        }

        // deleted instances, collision turned off, or a node that is no longer collidable
        let stale: Vec<ColliderHandle> = self.entries.iter()
            .filter(|entry| !wanted.contains(&(entry.node_id, entry.instance_id)))
            .map(|entry| entry.handle)
            .collect();

        let removed = stale.len();

        if removed > 0
        {
            self.entries.retain(|entry| wanted.contains(&(entry.node_id, entry.instance_id)));

            for handle in stale
            {
                self.colliders.remove(handle, &mut self.islands, &mut self.bodies, false);
            }

            self.rebuild_bvh();
        }

        (added, removed)
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

    fn rebuild_bvh(&mut self)
    {
        self.broad_phase_bvh = BroadPhaseBvh::new();

        let mut handles: Vec<ColliderHandle> = self.entries.iter().map(|entry| entry.handle).collect();
        handles.extend(self.ground_plane);

        for handle in handles
        {
            self.refresh_leaf(handle);
        }
    }

    // ********** syncing **********

    // Mirrors instance transforms onto the colliders, from the cache the renderer also uses.
    pub fn sync_transformations(&mut self) -> usize
    {
        let mut updated = 0;
        let mut rebuilds = 0;

        for index in 0..self.entries.len()
        {
            let transform = self.entries[index].instance.read().unwrap().get_cached_world_transform();

            if !Self::transform_differs(&transform, &self.entries[index].transform)
            {
                continue;
            }

            let (pose, scale) = Self::split_transform(&transform);
            let handle = self.entries[index].handle;
            let scale_changed = Self::scale_differs(&scale, &self.entries[index].scale);

            let shape = if scale_changed
            {
                rebuilds += 1;
                Self::build_shape(&self.entries[index].node, &scale)
            }
            else
            {
                None
            };

            if let Some(collider) = self.colliders.get_mut(handle)
            {
                collider.set_position(pose);

                if let Some(shape) = shape
                {
                    collider.set_shape(shape);
                }
            }
            else
            {
                continue;
            }

            self.refresh_leaf(handle);

            self.entries[index].transform = transform;
            self.entries[index].scale = scale;

            updated += 1;
        }

        self.last_synced = updated;
        self.last_shape_rebuilds = rebuilds;

        updated
    }

    // ********** queries **********

    pub fn query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a>
    {
        self.broad_phase_bvh.as_query_pipeline(&self.dispatcher, &self.bodies, &self.colliders, filter)
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
    fn refresh_instance_cache(node: &NodeItem)
    {
        let instances: Vec<InstanceItemArc> = node.read().unwrap().instances.get_ref().clone();

        for instance in instances
        {
            let world_matrix = instance.read().unwrap().calculate_transform();
            instance.write().unwrap().get_data_mut().get_mut().computed.world_matrix = world_matrix;
        }
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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

        assert_eq!(world.sync_transformations(), 1, "the moved node should be picked up");
        assert_eq!(world.sync_transformations(), 0, "a second sync has nothing left to do");

        let queries = world.query_pipeline(QueryFilter::default());
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(!res.grounded, "ground moved away, the capsule must not be grounded any more");
    }

    #[test]
    fn the_query_predicate_can_exclude_a_node()
    {
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();

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
        let mut world = PhysicsWorld::new();

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
        let mut world = PhysicsWorld::new();

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
            world.sync_transformations();

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
        let mut world = PhysicsWorld::new();

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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();

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

        assert_eq!(world.sync_transformations(), 1, "an instance transform change has to be picked up");

        let queries = world.query_pipeline(QueryFilter::default());
        let res = controller().move_shape(1.0 / 60.0, &queries, &capsule, &pos, Vector::new(0.0, -0.1, 0.0), |_| {});

        assert!(!res.grounded, "the instance moved the ground away, the capsule must not be grounded");
    }

    #[test]
    fn an_instance_with_collision_off_gets_no_collider()
    {
        let mut world = PhysicsWorld::new();

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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
        let mut world = PhysicsWorld::new();
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
            let mut world = PhysicsWorld::new();
            world.set_ground_plane(Some(0.0));
            wall(&mut world);

            let (along, stuck) = slide_along(&world, start, dir, 200);
            assert!(along > 15.0, "character only slid {} along a plain wall", along);
            assert!(stuck < 5.0, "character stuck for {} frames on a plain wall", stuck);
        }

        {
            // a panel mounted flat against the wall must not change anything
            let mut world = PhysicsWorld::new();
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

        let mut world = PhysicsWorld::new();
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

}

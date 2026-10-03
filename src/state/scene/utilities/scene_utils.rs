#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use nalgebra::{Matrix4, Point3, Vector3};

use std::sync::RwLock;

use crate::{component_downcast, component_downcast_mut};
use crate::helper::math;
use crate::helper::math::yaw_pitch_to_direction;
use crate::state::scene::components::mesh::Mesh;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::camera::{Camera, CameraProjectionType};
use crate::{helper::{concurrency::execution_queue::ExecutionQueueItem, option_or_id::OptionOrId}, state::{scene::{components::component::ComponentItem, node::{Node, NodeItem}, scene::Scene}, state::State}};

const DEFAULT_ALIGN_ALPHA: f32 = std::f32::consts::PI / 6.0; // 30° yaw
const DEFAULT_ALIGN_BETA: f32 = std::f32::consts::PI / 8.0;  // 22.5° pitch

pub fn clone_all_animations(from: NodeItem, to: NodeItem) -> Vec<ComponentItem>
{
    let animations = from.read().unwrap().get_all_animations();

    let mut new_animation_components = vec![];

    for animation in animations
    {
        let cloned_animation = clone_animation(animation.clone(), to.clone());

        if let Some(cloned_animation) = cloned_animation
        {
            new_animation_components.push(cloned_animation);
        }
    }

    new_animation_components
}

pub fn clone_animation(animation_component_from: ComponentItem, animation_component_to: NodeItem) -> Option<ComponentItem>
{
    let cloned_animation = animation_component_from.read().unwrap().duplicate();
    if let Some(cloned_animation) = cloned_animation
    {
        let mut target_node = animation_component_to.write().unwrap();
        target_node.add_component(cloned_animation.clone());
        target_node.re_target_animations_to_child_nodes();

        return Some(cloned_animation);
    }

    None
}

pub fn highlight_and_unhighlight_scene_meshes(scene: &mut Scene, highlight_nodes: &Vec<u32>)
{
    let all_nodes = scene.list_all_nodes();

    for node in &all_nodes
    {
        let highlight = highlight_nodes.contains(&(node.read().unwrap().id));

        let node = node.write().unwrap();

        for instance in node.instances.get_ref()
        {
            let mut instance = instance.write().unwrap();
            if instance.get_data().highlight != highlight
            {
                instance.get_data_mut().get_mut().highlight = highlight;
            }
        }
    }
}

pub fn execute_on_scene_mut_and_wait(main_queue: ExecutionQueueItem, scene_id: u32, func: Box<dyn Fn(&mut Scene) + Send + Sync>)
{
    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id)
            {
                func(scene);
            }
        }));
    }
    res.join();
}

pub fn execute_on_scene_mut(main_queue: ExecutionQueueItem, scene_id: u32, func: Box<dyn Fn(&mut Scene) + Send + Sync>)
{
    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            func(scene);
        }
    }));
}

pub fn execute_on_state_mut(main_queue: ExecutionQueueItem, func: Box<dyn Fn(&mut State) + Send + Sync>)
{
    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        func(state);
    }));
}

/*
pub fn execute_on_state_mut_and_wait(main_queue: ExecutionQueueItem, func: Box<dyn Fn(&mut State) + Send + Sync>)
{
    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            func(state);
        }));
    }
    res.join();
}
*/

//pub fn execute_on_state_mut_and_wait_fn_once(main_queue: ExecutionQueueItem, func: Box<dyn FnOnce(&mut State) + Send + Sync>)
pub fn execute_on_state_mut_and_wait(main_queue: ExecutionQueueItem, func: Box<dyn FnOnce(&mut State) + Send + Sync>)
{
    let res;
    {
        let func = Arc::new(Mutex::new(Some(func)));

        let func_clone = func.clone();
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            let opt = func_clone.lock().unwrap().take();
            if let Some(func) = opt
            {
                func(state);
            }
        }));
    }
    res.join();
}

/// Set the parent of `node`: if `Some(node)` -> set as parent, if `None` -> make root-level (scene node).
/// `keep_transform`: the world transformation of the node is kept (the local transformation is re-mapped).
pub fn set_node_parent(scene: &mut Scene, node: NodeItem, target: Option<NodeItem>, keep_transform: bool)
{
    // the new parent can not be the node itself or one of its children
    if let Some(target) = target.as_ref()
    {
        if target.read().unwrap().has_parent_or_is_equal(node.clone())
        {
            return;
        }
    }

    // world transformation (before re-parenting)
    let world_transform = node.read().unwrap().get_full_transform();

    if let Some(target) = target
    {
        // if currently root-level, remove from scene.nodes
        if node.read().unwrap().parent.is_none()
        {
            let id = node.read().unwrap().id;
            scene.nodes.retain(|n| n.read().unwrap().id != id);
        }

        Node::set_parent(node.clone(), target);
    }
    else
    {
        // already root-level - nothing to do
        if node.read().unwrap().parent.is_none()
        {
            return;
        }

        // detach from old parent
        if let Some(old_parent) = node.read().unwrap().parent.as_ref()
        {
            let id = node.read().unwrap().id;
            old_parent.write().unwrap().nodes.retain(|n| n.read().unwrap().id != id);
        }

        node.write().unwrap().parent = OptionOrId::None;
        node.write().unwrap().force_instances_update();
        scene.nodes.push(node.clone());
    }

    if keep_transform
    {
        Node::remap_world_transform(node, world_transform);
    }
}

/// Move `source_nodes` to `target`: if `Some(node)` -> set as parent, if `None` -> make root-level.
pub fn move_nodes_to(exec_queue: ExecutionQueueItem, scene_id: u32, source_ids: Vec<u32>, target: Option<NodeItem>)
{
    if source_ids.len() == 0
    {
        return;
    }

    execute_on_scene_mut(exec_queue, scene_id, Box::new(move |scene|
    {
        let source_nodes: Vec<NodeItem> = source_ids.iter()
            .filter_map(|&id| scene.find_node_by_id(id))
            .collect();

        for source_node in source_nodes
        {
            set_node_parent(scene, source_node, target.clone(), false);
        }
    }));
}

pub fn get_scene_world_bounding_info(scene: &Scene, predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>) -> Option<(Point3<f32>, Point3<f32>)>
{
    let mut min = Point3::<f32>::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Point3::<f32>::new(f32::MIN, f32::MIN, f32::MIN);
    let mut found = false;

    for node in &scene.nodes
    {
        if let Some(predicate) = &predicate
        {
            if !predicate(node.clone())
            {
                continue;
            }
        }

        let bounds = node.read().unwrap().get_world_bounding_info(None, true, predicate.clone());

        if let Some((node_min, node_max)) = bounds
        {
            min.x = min.x.min(node_min.x);
            min.y = min.y.min(node_min.y);
            min.z = min.z.min(node_min.z);

            max.x = max.x.max(node_max.x);
            max.y = max.y.max(node_max.y);
            max.z = max.z.max(node_max.z);

            found = true;
        }
    }

    if found { Some((min, max)) } else { None }
}


pub fn align_camera_to_bounds(cam: &mut Camera, min: Point3<f32>, max: Point3<f32>, alpha: Option<f32>, beta: Option<f32>) -> bool
{
    // look at the center of the bounding box; the bounding sphere radius drives the distance
    let center = Point3::<f32>::from((min.coords + max.coords) * 0.5);
    let radius = (max - min).norm() * 0.5;

    if radius <= 0.0
    {
        return false;
    }

    let alpha = alpha.unwrap_or(DEFAULT_ALIGN_ALPHA);
    let beta = beta.unwrap_or(DEFAULT_ALIGN_BETA);

    // direction from the center towards the camera (alpha = yaw, beta = pitch)
    let dir = yaw_pitch_to_direction(alpha, beta).normalize();

    let cam_data = cam.get_data_mut().get_mut();

    // viewport aspect ratio (matches what init_matrices uses to build the projection)
    let viewport = cam_data.get_viewport();
    let aspect = (viewport.width * cam_data.resolution_width as f32).max(1.0)
               / (viewport.height * cam_data.resolution_height as f32).max(1.0);

    let distance;

    if cam_data.projection_type == CameraProjectionType::Perspective
    {
        // back off far enough that the bounding sphere fits — the narrower of the two half-fovs binds
        let half_fovy = cam_data.fovy * 0.5;
        let half_fovx = (half_fovy.tan() * aspect).atan();
        let half_fov = half_fovy.min(half_fovx);

        distance = radius / (half_fov.sin());
    }
    else
    {
        // ortho: fit the sphere into the (aspect-corrected) extent
        let half = radius / (1.0_f32).max(1.0 / aspect);
        cam_data.top = half;
        cam_data.bottom = -half;
        cam_data.left = -half * aspect;
        cam_data.right = half * aspect;

        distance = radius * 2.0;
    }

    cam_data.eye_pos = center + dir * distance;
    cam_data.dir = -dir;
    cam_data.up = Vector3::<f32>::new(0.0, 1.0, 0.0);
    cam_data.clipping_far = cam_data.clipping_far.max(distance + radius * 2.0);

    cam.init_matrices();

    true
}

pub fn align_camera_to_scene(scene: &mut Scene, cam_index: usize, alpha: Option<f32>, beta: Option<f32>, predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>) -> bool
{
    let Some((min, max)) = get_scene_world_bounding_info(scene, predicate) else
    {
        crate::console_warning!("align_camera_to_scene: no bounding info found (empty scene / nothing with a mesh?)");
        return false;
    };

    let Some(cam) = scene.cameras.get_mut(cam_index) else
    {
        crate::console_warning!("align_camera_to_scene: camera index {} not found", cam_index);
        return false;
    };

    align_camera_to_bounds(cam, min, max, alpha, beta)
}
// ********** baking a node transform into its mesh **********

// A rigid body under a non-uniform scale is stretched by a different amount for every
// orientation, so it visibly changes shape as it rotates. The scale is applied by the scene
// graph after the transform of whatever sits below it, which means no physics write back can
// undo it. Baking moves the scale into the vertices, leaving the node at a scale of one.
//
// Returns the messages worth showing the user, empty when nothing had to be said.
pub fn bake_scale(node: NodeItem, state: &mut State) -> Vec<String>
{
    bake_node_transform(node, state, false)
}

// The same, for the whole node transform. Translation and rotation are rigid, so this fixes
// nothing a physics body cares about - it is for tidying an object up. The price is the
// pivot: the node ends up on identity, so rotating it afterwards turns it around the origin
// of its parent rather than around itself, and its geometry can no longer be shared with
// another placement.
pub fn bake_transform(node: NodeItem, state: &mut State) -> Vec<String>
{
    bake_node_transform(node, state, true)
}

fn bake_node_transform(node: NodeItem, state: &mut State, full: bool) -> Vec<String>
{
    let mut notes = vec![];

    let (matrix, scale) =
    {
        let node_read = node.read().unwrap();
        let Some(transformation) = node_read.find_component::<Transformation>() else
        {
            notes.push("this node has no transformation, so there is nothing to bake".to_string());
            return notes;
        };

        component_downcast!(transformation, Transformation);

        let local = *transformation.get_transform();
        let scale = math::extract_scale_from_transform(&local);

        let matrix = if full { local } else { Matrix4::new_nonuniform_scaling(&scale) };

        (matrix, scale)
    };

    if math::approx_zero(scale.x) || math::approx_zero(scale.y) || math::approx_zero(scale.z)
    {
        notes.push("a scale of zero cannot be baked, the geometry would collapse".to_string());
        return notes;
    }

    let Some(matrix_inverse) = matrix.try_inverse() else
    {
        notes.push("this transform has no inverse, so it cannot be baked".to_string());
        return notes;
    };

    if (matrix - Matrix4::identity()).abs().max() < 0.0001
    {
        notes.push("the transform is already identity, nothing to bake".to_string());
        return notes;
    }

    // ********** the geometry **********
    let mesh_component =
    {
        let node_read = node.read().unwrap();
        node_read.find_component::<Mesh>()
    };

    if let Some(mesh_component) = mesh_component
    {
        let resource =
        {
            component_downcast!(mesh_component, Mesh);
            mesh_component.mesh_resource.as_ref().cloned()
        };

        if let Some(resource) = resource
        {
            // The resource is shared by hash, so writing into it would change every other
            // object built from the same geometry. Baking always works on a copy.
            let mut baked = resource.read().unwrap().duplicate();
            baked.apply_transform(&matrix);

            let baked = state.insert_mesh_resource_or_reuse(Arc::new(RwLock::new(Box::new(baked))), "baked");

            component_downcast_mut!(mesh_component, Mesh);
            mesh_component.mesh_resource = OptionOrId::Some(baked);
        }
    }
    else
    {
        notes.push("this node has no mesh, so only the transform below it was adjusted".to_string());
    }

    // A rotation below a non-uniform scale needs shear to stay identical, and a transform
    // component stores a position, a rotation and a scale - nothing else. Those cases are
    // counted and reported rather than quietly coming out wrong.
    let uniform = math::approx_equal(scale.x, scale.y) && math::approx_equal(scale.y, scale.z);
    let mut rotated_below = 0;

    let is_rotated = |transform: &Matrix4<f32>| -> bool
    {
        !uniform && !math::approx_zero_vec3(&math::extract_rotation_as_euler_vec(transform))
    };

    {
        let node_read = node.read().unwrap();

        // The instances carry the geometry that was just baked, so their transform has to be
        // conjugated: what they did after the bake, they now have to do before it as well.
        for instance in node_read.instances.get_ref()
        {
            let instance = instance.read().unwrap();

            let Some(transformation) = instance.find_component::<Transformation>() else { continue; };

            component_downcast_mut!(transformation, Transformation);

            let local = *transformation.get_transform();

            if is_rotated(&local) { rotated_below += 1; }

            transformation.set_local_transform(matrix * local * matrix_inverse);
        }

        // The children keep their own geometry, so the transform is simply handed down.
        for child in &node_read.nodes
        {
            let child_read = child.read().unwrap();

            let Some(transformation) = child_read.find_component::<Transformation>() else { continue; };

            component_downcast_mut!(transformation, Transformation);

            let local = *transformation.get_transform();

            if is_rotated(&local) { rotated_below += 1; }

            transformation.set_local_transform(matrix * local);
        }
    }

    if rotated_below > 0
    {
        notes.push(format!("{} rotated instances or children sit below this node - a rotation under a non-uniform scale cannot be preserved exactly, check them", rotated_below));
    }

    // ********** the node itself **********
    {
        let node_read = node.read().unwrap();
        let transformation = node_read.find_component::<Transformation>().unwrap();

        component_downcast_mut!(transformation, Transformation);

        if full
        {
            transformation.set_local_transform(Matrix4::identity());
        }
        else
        {
            transformation.set_scale(Vector3::new(1.0, 1.0, 1.0));
        }
    }

    // ********** the collider was built from the old geometry **********
    let node_id = node.read().unwrap().id;

    for scene in &mut state.scenes
    {
        if scene.find_node_by_id(node_id).is_some()
        {
            scene.build_physics();
            break;
        }
    }

    notes
}

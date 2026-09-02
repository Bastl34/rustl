#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use nalgebra::{Point3, Vector3};

use crate::helper::math::yaw_pitch_to_direction;
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
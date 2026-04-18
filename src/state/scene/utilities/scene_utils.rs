#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use crate::{helper::{concurrency::execution_queue::ExecutionQueueItem, option_or_id::OptionOrId}, state::{scene::{components::component::ComponentItem, node::{Node, NodeItem}, scene::Scene}, state::State}};

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

/// Move `source_nodes` to `target`: if `Some(node)` → set as parent, if `None` → make root-level.
pub fn move_nodes_to(exec_queue: ExecutionQueueItem, scene_id: u32, source_ids: Vec<u32>, target: Option<NodeItem>)
{
    if source_ids.len() == 0
    {
        return;
    }

    execute_on_scene_mut(exec_queue, scene_id, Box::new(move |scene|
    {
        let source_nodes: Vec<NodeItem> = source_ids.iter()
            .filter(|&&id| target.as_ref().map_or(true, |t| t.read().unwrap().id != id))
            .filter_map(|&id| scene.find_node_by_id(id))
            .collect();

        if source_nodes.is_empty() { return; }

        let source_nodes = Arc::new(source_nodes);

        if let Some(target_node) = &target
        {
            for source_node in source_nodes.iter()
            {
                // if currently root-level, remove from scene.nodes
                if source_node.read().unwrap().parent.is_none()
                {
                    let id = source_node.read().unwrap().id;
                    scene.nodes.retain(|n| n.read().unwrap().id != id);
                }
                Node::set_parent(source_node.clone(), target_node.clone());
            }
        }
        else
        {
            for source_node in source_nodes.iter()
            {
                // already root-level — nothing to do
                if source_node.read().unwrap().parent.is_none() { continue; }

                // detach from old parent
                if let Some(old_parent) = source_node.read().unwrap().parent.as_ref()
                {
                    let id = source_node.read().unwrap().id;
                    old_parent.write().unwrap().nodes.retain(|n| n.read().unwrap().id != id);
                }

                source_node.write().unwrap().parent = OptionOrId::None;
                source_node.write().unwrap().force_instances_update();
                scene.nodes.push(source_node.clone());
            }
        }
    }));
}
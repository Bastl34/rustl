use std::sync::{Arc, RwLock};

use nalgebra::{Matrix4, Point2, Point3, Vector3, Vector4};

use crate::{component_downcast_mut, gui::editor::editor::EDITOR_INTERNAL_TAG, state::{scene::{camera_controller::fly_controller::FlyController, components::{component::{Component, ComponentItem}, material::Material, mesh::Mesh, sound::Sound, transformation::Transformation}, node::NodeItem, scene::{PickPredicate, Scene, ScenePickRes}, utilities::tags}, state::{ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX, State}}};

use super::editor_state::EditorState;

pub fn pick(state: &State, pos: Point2::<f32>, allow_grid_picking: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Option<(u32, ScenePickRes)>
{
    let scenes = &state.scenes;

    let mut hit: Option<ScenePickRes> = None;
    let mut scene_id: u32 = 0;

    // do not pick internal predicate
    let inner_predicate = predicate.clone();
    let do_not_pick_internal_nodes_predicate: PickPredicate = Arc::new(move |node_arc: NodeItem, check_instance_id: Option<u32>| -> bool
    {
        if node_arc.read().unwrap().tags.contains(ENGINE_INTERNAL_TAG) || node_arc.read().unwrap().tags.contains(EDITOR_INTERNAL_TAG)
        {
            return false;
        }

        if let Some(inner_predicate) = &inner_predicate
        {
            if !inner_predicate(node_arc.clone(), check_instance_id)
            {
                return false;
            }
        }
        true
    });

    let do_not_pick_internal_nodes_predicate: Option<PickPredicate> = Some(do_not_pick_internal_nodes_predicate);

    for scene in scenes
    {
        for camera in &scene.cameras
        {
            // check if click is insight
            if camera.is_point_in_viewport(&pos)
            {
                let ray = camera.get_ray_from_viewport_coordinates(&pos);

                let mut grid_hit = None;
                if allow_grid_picking
                {
                    let grid = scene.find_mesh_node_by_name("grid");
                    if let Some(grid) = grid
                    {
                        grid_hit = scene.pick_node(grid, &ray, false, true, ignore_visible, true, predicate.clone());
                    }
                }

                let scene_hit = scene.pick(&ray, false, false, ignore_visible, ignore_pickable, do_not_pick_internal_nodes_predicate.clone());

                //dbg!(scene_hit.is_some());
                //dbg!(grid_hit.is_some());

                // check if grid hit is closer or scene hit
                let mut new_hit = grid_hit;
                if let Some(scene_hit_ref) = scene_hit.as_ref()
                {
                    if let Some(new_hit_ref) = new_hit.as_ref()
                    {
                        if scene_hit_ref.time_of_impact < new_hit_ref.time_of_impact
                        {
                            new_hit = scene_hit;
                        }
                    }
                    else
                    {
                        new_hit = scene_hit;
                    }
                }

                //dbg!(new_hit.is_some());

                let mut save_hit = false;

                if let Some(new_hit) = new_hit.as_ref()
                {
                    if let Some(hit) = hit.as_ref()
                    {
                        // check if the new hit is near
                        if new_hit.time_of_impact < hit.time_of_impact
                        {
                            save_hit = true;
                        }
                    }
                    else
                    {
                        save_hit = true;
                    }
                }

                if save_hit
                {
                    hit = new_hit;
                    scene_id = scene_id;
                }
            }
        }

        /*
        if allow_grid_picking
        {
            set_grid_picking(scene, false);
        }
        */
    }

    if let Some(hit) = hit
    {
        return Some((scene_id, hit));
    }

    None
}

pub fn pick_node(state: &State, node: NodeItem, pos: Point2::<f32>, ignore_visible: bool, ignore_pickable: bool) -> Option<(u32, ScenePickRes)>
{
    let scenes = &state.scenes;

    for scene in scenes
    {
        for camera in &scene.cameras
        {
            // check if click is insight
            if camera.is_point_in_viewport(&pos) && camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
            {
                let ray = camera.get_ray_from_viewport_coordinates(&pos);
                let hit = scene.pick_node(node.clone(), &ray, false, false, ignore_visible, ignore_pickable, None);

                if let Some(hit) = hit
                {
                    return Some((scene.id, hit));
                }
            }
        }
    }

    None
}

pub fn get_object_and_pointer_world_position(state: &State) -> Option<(String, Point3<f32>)>
{
    let pointer_pos = state.io.input_manager.get_pointer_input().pos;

    if let Some(pointer_pos) = pointer_pos
    {
        if let Some((_scene, pick_res)) = pick(state, pointer_pos, true, false, false, None)
        {
            return Some((pick_res.node.read().unwrap().name.clone(), pick_res.point));
        }
    }

    None
}

pub fn apply_fly_camera_move_state(scene: &mut Scene, state: bool)
{
    for camera in &mut scene.cameras
    {
        if !camera.enabled || !camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
        {
            continue;
        }

        if let Some(controller) = &mut camera.controller
        {
            if let Some(controller) = controller.as_any_mut().downcast_mut::<FlyController>()
            {
                controller.mouse_movement = state;
            }
        }
    }
}

pub fn find_transform_component(editor_state: &mut EditorState, state: &mut State) -> ComponentItem
{
    // ********** find transform component for node/instance **********
    let (_scene, node, instance_id) = editor_state.get_selected_node(state);

    let node = node.unwrap();

    let edit_transformation: ComponentItem;

    if let Some(instance_id) = instance_id
    {
        let instance_transform;
        {
            let node = node.read().unwrap();
            let instance = node.find_instance_by_id(instance_id).unwrap();
            let instance = instance.read().unwrap();
            instance_transform = instance.find_component::<Transformation>();
        }

        if let Some(instance_transform) = instance_transform
        {
            edit_transformation = instance_transform.clone();
        }
        else
        {
            let node = node.read().unwrap();
            let instance = node.find_instance_by_id(instance_id).unwrap() ;
            let mut instance = instance.write().unwrap();

            instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Transformation")))));

            let transformation = instance.find_component::<Transformation>().unwrap();
            edit_transformation = transformation.clone();
        }
    }
    else
    {
        let node_transform;
        {
            let node = node.read().unwrap();
            node_transform = node.find_component::<Transformation>();
        }

        if let Some(node_transform) = node_transform
        {
            edit_transformation = node_transform.clone();
        }
        else
        {
            let mut node = node.write().unwrap();

            node.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Transformation")))));

            let transformation = node.find_component::<Transformation>().unwrap();
            edit_transformation = transformation.clone();
        }
    }

    edit_transformation
}

pub fn get_world_transform_from_selected_node(editor_state: &mut EditorState, state: &mut State) -> Matrix4<f32>
{
    let (_, node, instance_id) = editor_state.get_selected_node(state);

    let node = node.unwrap();

    let transform;
    if let Some(instance_id) = instance_id
    {
        let node = node.read().unwrap();
        let instance = node.find_instance_by_id(instance_id).unwrap();
        let instance = instance.read().unwrap();
        transform = instance.calculate_transform();
    }
    else
    {
        let node = node.read().unwrap();
        transform = node.get_full_transform();
    }

    transform
}

pub fn get_parent_world_transform_from_selected_node(editor_state: &mut EditorState, state: &mut State) -> Matrix4<f32>
{
    let (_, node, instance_id) = editor_state.get_selected_node(state);

    let node = node.unwrap();

    let transform;
    if instance_id.is_some()
    {
        let node = node.read().unwrap();
        transform = node.get_full_transform();
    }
    else
    {
        let node = node.read().unwrap();

        if let Some(parent) = node.parent.as_ref()
        {
            let parent = parent.read().unwrap();
            transform = parent.get_full_transform();
        }
        else
        {
            transform = Matrix4::identity();
        }
    }

    transform
}

pub fn transform_vec_to_parent_local(instance_id: Option<u32>, selected_node: NodeItem, vec: Vector3<f32>) -> Vector3<f32>
{
    let mut vec = vec;

    if instance_id.is_some()
    {
        let node = selected_node.read().unwrap();
        vec = node.transform_vec_global_to_local(&Vector4::<f32>::new(vec.x, vec.y, vec.z, 0.0)).xyz();
    }
    else
    {
        let node = selected_node.read().unwrap();

        if let Some(parent) = node.parent.as_ref()
        {
            let parent = parent.read().unwrap();
            vec = parent.transform_vec_global_to_local(&Vector4::<f32>::new(vec.x, vec.y, vec.z, 0.0)).xyz();
        }
    }

    vec
}

pub fn set_internal_tag_for_utils_nodes(scene: &mut Scene)
{
    let utils_node = scene.find_node_by_name("editor utils");

    if utils_node.is_none()
    {
        return;
    }

    let utils_node = utils_node.unwrap();
    let utils_node = utils_node.read().unwrap();
    let all_child_nodes = Scene::list_all_child_nodes(&utils_node.nodes);

    for node in all_child_nodes
    {
        let mut node = node.write().unwrap();
        node.tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

        // materials
        {
            let materials = node.find_components::<Material>();
            for material in materials
            {
                component_downcast_mut!(material, Material);
                material.get_base_mut().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

                // textures
                for tex in material.get_all_textures()
                {
                    tex.write().unwrap().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
                }
            }
        }

        // meshes
        {
            let meshes = node.find_components::<Mesh>();
            for mesh in meshes
            {
                component_downcast_mut!(mesh, Mesh);
                if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                {
                    mesh_resource.write().unwrap().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
                }
            }
        }

        // sound sources
        {
            let sounds = node.find_components::<Sound>();
            for sound in sounds
            {
                component_downcast_mut!(sound, Sound);
                if let Some(sound_source) = sound.sound_source.as_ref()
                {
                    sound_source.write().unwrap().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
                }
            }
        }
    }
}
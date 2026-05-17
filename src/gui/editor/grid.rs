use std::{f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{Point3, Vector3, Vector4};

use crate::{component_downcast_mut, console_error, gui::editor::editor::EDITOR_UTILS_NODE_NAME, helper::{concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, math::{approx_equal_vec, is_almost_integer, snap_to_grid_vec3}, option_or_id::OptionOrId}, input::keyboard::Key, state::{resources::mesh_resource::MeshResource, scene::{components::{component::Component, material::{Material, MaterialItem}, mesh::Mesh, transformation::Transformation}, instance::Instance, loader::loader::load_asset_and_add_to_scene, node::Node, utilities::scene_utils::{execute_on_scene_mut_and_wait, execute_on_state_mut_and_wait}}, state::State}};

use super::{editor_state::EditorState, helper::set_internal_tag_for_utils_nodes};

const GRID_DEFAULT_ALPHA_INDEX: i64 = -1000;
pub const GRID_ROOT_NAME: &str = "grid root";
pub const GRID_ORIGIN_ROOT_NAME: &str = "grid origin root";

pub fn create_grid(scene_id: u32, main_queue: ExecutionQueueItem, amount: u32, spacing: f32)
{
    let integer_grid_line_scale = 3.0;

    let grid_origin_line_scale = 3.5;
    let grid_origin_line_scale_line = 1_000.0;

    let amount = amount as i32;

    let size = amount as f32 * spacing;

    let editor_utils_node_id: Arc<RwLock<Option<u32>>> = Arc::new(RwLock::new(None));
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new
    ({
        let editor_utils_node_id = editor_utils_node_id.clone();
        move |scene|
        {
            if let Some(editor_utils_node) = scene.find_node_by_name(EDITOR_UTILS_NODE_NAME)
            {
                *editor_utils_node_id.write().unwrap() = Some(editor_utils_node.read().unwrap().id);
            }
        }
    }));
    let editor_utils_node_id = *editor_utils_node_id.read().unwrap();

    if editor_utils_node_id.is_none()
    {
        console_error!("Failed to find editor utils node for grid creation");
        return;
    }

    // delte already existing first ("grid root", and "grid origin root")
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
    {
        scene.delete_node_by_name(GRID_ORIGIN_ROOT_NAME, true, true, true, true);
        scene.delete_node_by_name(GRID_ROOT_NAME, true, true, true, true);
    }));

    let loaded_assets_grid = load_asset_and_add_to_scene("objects/grid/grid_line.gltf", scene_id, editor_utils_node_id, main_queue.clone(), false, true, true, true, false, 0).unwrap();
    let loaded_assets_origin = load_asset_and_add_to_scene("objects/grid/grid_line_extruded.glb", scene_id, editor_utils_node_id, main_queue.clone(), false, false, true, true, false, 0).unwrap();

    let mut grid_root = None;

    execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
    {
        if state.find_scene_by_id_mut(scene_id).is_none()
        {
            return;
        }

        // ********** renaming **********
        {
            let scene = state.find_scene_by_id_mut(scene_id).unwrap();

            // origin lines
            if let Some(root) = loaded_assets_origin.root_node_ids.get(0)
            {
                if let Some(root_node) = scene.find_node_by_id(*root)
                {
                    {
                        root_node.write().unwrap().name = GRID_ORIGIN_ROOT_NAME.to_string();
                    }

                    // move to front
                    if let Some(parent) = root_node.read().unwrap().parent.as_ref()
                    {
                        parent.write().unwrap().move_to_front(root_node.clone());
                    }
                }
            }
            for (i, id) in loaded_assets_origin.node_ids.iter().enumerate()
            {
                // 0 is already checked/renamed
                if i == 0 { continue; }

                if let Some(node) = scene.find_node_by_id(*id)
                {
                    if node.read().unwrap().name == "grid"
                    {
                        // ranem to origin -> otherwise lookups to "grid" will fail and result the wrong node
                        node.write().unwrap().name = "grid origin".to_string();
                    }
                }
            }


            // grid itself
            if let Some(root) = loaded_assets_grid.root_node_ids.get(0)
            {
                if let Some(root_node) = scene.find_node_by_id(*root)
                {
                    grid_root = Some(root_node.clone());
                    root_node.write().unwrap().name = GRID_ROOT_NAME.to_string();

                    // move to front
                    if let Some(parent) = root_node.read().unwrap().parent.as_ref()
                    {
                        parent.write().unwrap().move_to_front(root_node.clone());
                    }
                }
            }

            // ********** grid **********

            if let Some(grid_arc) = scene.find_mesh_node_by_ids(&loaded_assets_grid.node_ids)
            {
                {
                    let mut grid = grid_arc.write().unwrap();
                    grid.clear_instances();
                }

                for i in 0..amount
                {
                    let pos = i - (amount / 2);

                    // x
                    {
                        let mut instance = Instance::new
                        (
                            format!("grid_x_{}", pos),
                            grid_arc.clone()
                        );

                        let z_pos = pos as f32 * spacing;
                        let scale = if is_almost_integer(z_pos) { integer_grid_line_scale } else { 1.0 };

                        let mut transformation = Transformation::identity("Transform");
                        transformation.apply_translation(Vector3::<f32>::new(0.0, 0.0, z_pos));
                        transformation.apply_scale(Vector3::<f32>::new(amount as f32 * spacing, scale, scale), true);

                        instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));

                        let mut grid = grid_arc.write().unwrap();
                        grid.add_instance(Box::new(instance));
                    }

                    // y
                    {
                        let mut instance = Instance::new
                        (
                            format!("grid_y_{}", pos),
                            grid_arc.clone()
                        );

                        let x_pos = pos as f32 * spacing;
                        let scale = if is_almost_integer(x_pos) { integer_grid_line_scale } else { 1.0 };

                        let mut transformation = Transformation::identity("Transform");
                        transformation.apply_translation(Vector3::<f32>::new(x_pos, 0.0, 0.0));
                        transformation.apply_rotation(Vector3::<f32>::new(0.0, PI / 2.0, 0.0));
                        transformation.apply_scale(Vector3::<f32>::new(amount as f32 * spacing, scale, scale), true);

                        instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));

                        let mut grid = grid_arc.write().unwrap();
                        grid.add_instance(Box::new(instance));
                    }
                }

                {
                    let grid = grid_arc.read().unwrap();

                    if let Some(material) = grid.find_component::<Material>()
                    {
                        component_downcast_mut!(material, Material);
                        material.get_base_mut().name = "grid material".to_string();
                        material.get_data_mut().get_mut().unlit_shading = true;
                        material.get_data_mut().get_mut().allow_xray = false;
                        material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(0.0, 0.0, 0.0);
                    }
                }
            }

            // ********** merge together grid mesh **********
            for id in &loaded_assets_grid.node_ids
            {
                if let Some(node) = scene.find_node_by_id(*id)
                {
                    let mut node = node.write().unwrap();
                    node.merge_instances();

                    let instance = node.instances.get_mut().first();

                    if let Some(instance) = instance
                    {
                        instance.write().unwrap().pickable = false;
                    }

                    node.settings.alpha_index = GRID_DEFAULT_ALPHA_INDEX - 1; // render before the grid plane
                }
            }

            // ********** grid origin **********
            if let Some(grid_arc) = scene.find_mesh_node_by_ids(&loaded_assets_origin.node_ids)
            {
                {
                    let mut grid = grid_arc.write().unwrap();
                    grid.clear_instances();

                    if let Some(material) = grid.find_component::<Material>()
                    {
                        component_downcast_mut!(material, Material);
                        material.get_base_mut().name = "grid origin material".to_string();
                        material.get_data_mut().get_mut().unlit_shading = true;
                        material.get_data_mut().get_mut().allow_xray = false;
                        material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(1.0, 1.0, 1.0);
                        material.get_data_mut().get_mut().alpha = 0.7;
                    }
                }

                // x (red)
                {
                    let mut instance = Instance::new("grid_origin_x".to_string(), grid_arc.clone());

                    let scale = grid_origin_line_scale;

                    let mut transformation = Transformation::identity("Transform");
                    transformation.apply_scale(Vector3::<f32>::new(grid_origin_line_scale_line, scale, scale), true);
                    instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));
                    instance.get_data_mut().get_mut().color = Vector4::<f32>::new(1.0, 0.0, 0.0, 1.0);
                    instance.pickable = false;

                    let mut grid = grid_arc.write().unwrap();
                    grid.add_instance(Box::new(instance));
                }

                // y (green)
                {
                    let mut instance = Instance::new("grid_origin_y".to_string(), grid_arc.clone());

                    let scale = grid_origin_line_scale;

                    let mut transformation = Transformation::identity("Transform");
                    transformation.apply_rotation(Vector3::<f32>::new(0.0, 0.0, PI / 2.0));
                    transformation.apply_scale(Vector3::<f32>::new(grid_origin_line_scale_line, scale, scale), true);
                    instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));
                    instance.get_data_mut().get_mut().color = Vector4::<f32>::new(0.0, 1.0, 0.0, 1.0);
                    instance.pickable = false;

                    let mut grid = grid_arc.write().unwrap();
                    grid.add_instance(Box::new(instance));
                }

                // z (blue)
                {
                    let mut instance = Instance::new("grid_origin_z".to_string(), grid_arc.clone());

                    let scale = grid_origin_line_scale;

                    let mut transformation = Transformation::identity("Transform");
                    transformation.apply_rotation(Vector3::<f32>::new(0.0, PI / 2.0, 0.0));
                    transformation.apply_scale(Vector3::<f32>::new(grid_origin_line_scale_line, scale, scale), true);
                    instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));
                    instance.get_data_mut().get_mut().color = Vector4::<f32>::new(0.0, 0.0, 1.0, 1.0);
                    instance.pickable = false;

                    let mut grid = grid_arc.write().unwrap();
                    grid.add_instance(Box::new(instance));
                }


                {
                    let mut grid_origin = grid_arc.write().unwrap();
                    grid_origin.settings.alpha_index = GRID_DEFAULT_ALPHA_INDEX - 2; // render before the grid plane and grid lines

                    if let Some(material) = grid_origin.find_component::<Material>()
                    {
                        component_downcast_mut!(material, Material);
                        material.get_base_mut().name = "grid origin material".to_string();
                        material.get_data_mut().get_mut().unlit_shading = true;
                        material.get_data_mut().get_mut().allow_xray = false;
                        material.get_data_mut().get_mut().alpha = 0.8;
                        material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(1.0, 1.0, 1.0);
                    }
                }
            }
        }

        // ********** create plane **********
        if let Some(grid_root) = grid_root
        {
            let half_size = size / 2.0;

            let p0 = Point3::<f32>::new(-half_size, -0.01, half_size);
            let p1 = Point3::<f32>::new(half_size, -0.01, half_size);
            let p2 = Point3::<f32>::new(half_size, -0.01, -half_size);
            let p3 = Point3::<f32>::new(-half_size, -0.01, -half_size);

            let plane_mesh_resource = Arc::new(RwLock::new(Box::new(MeshResource::new_plane("grid plane mesh", p0, p1, p2, p3))));
            let plane_mesh = state.insert_mesh_resource_or_reuse(plane_mesh_resource, "grid plane mesh");
            let mut plane_mesh_component = Mesh::new("grid plane mesh");
            plane_mesh_component.mesh_resource = OptionOrId::Some(plane_mesh);

            let mut plane_material = Material::new("grid plane material");
            plane_material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(0.9, 0.9, 1.0);
            plane_material.get_data_mut().get_mut().alpha = 0.7;
            plane_material.get_data_mut().get_mut().unlit_shading = true;
            plane_material.get_data_mut().get_mut().allow_xray = false;

            let plane_material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(plane_material)));

            {
                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                scene.add_material(&plane_material_arc.clone());
            }

            let plane_node = Node::new("plane");
            {
                {
                    let mut plane_node = plane_node.write().unwrap();
                    plane_node.add_component(Arc::new(RwLock::new(Box::new(plane_mesh_component))));
                    plane_node.add_component(plane_material_arc);

                    plane_node.settings.alpha_index = GRID_DEFAULT_ALPHA_INDEX;
                }

                plane_node.write().unwrap().create_default_instance(plane_node.clone());
            }

            Node::add_node_front(grid_root, plane_node);
        }

        // run internal tagging
        {
            let scene = state.find_scene_by_id_mut(scene_id).unwrap();
            set_internal_tag_for_utils_nodes(scene);
        }
    }));
}

pub fn update_grid(editor_state: &mut EditorState , state: &mut State)
{
    let grid_size = editor_state.grid_size;

    // create instance
    let move_up = state.io.input_manager.keyboard.is_pressed(Key::Plus);
    let move_down = state.io.input_manager.keyboard.is_pressed(Key::Minus);

    let mut move_grid_y_to = None;

    // move grid to selected node bounding box top
    if state.io.input_manager.keyboard.is_pressed(Key::Numpad8)
    {
        if let (Some(_), Some(node), instance_id) = editor_state.get_selected_node(state)
        {
            let node = node.read().unwrap();
            if let Some(bbox) = node.get_world_bounding_info(instance_id, true, None)
            {
                move_grid_y_to = Some(bbox.1.y);
            }
        }
    }
    // move grid to selected node bounding box bottom
    else if state.io.input_manager.keyboard.is_pressed(Key::Numpad2)
    {
        if let (Some(_), Some(node), instance_id) = editor_state.get_selected_node(state)
        {
            let node = node.read().unwrap();
            if let Some(bbox) = node.get_world_bounding_info(instance_id, true, None)
            {
                move_grid_y_to = Some(bbox.0.y);
            }
        }
    }
    // move grid back to 0 (ground level)
    else if state.io.input_manager.keyboard.is_pressed(Key::Numpad0)
    {
        move_grid_y_to = Some(0.0);
    }

    for scene in &mut state.scenes
    {
        if !scene.active { continue; }

        let scene_id = scene.id;

        let grid = scene.find_node_by_name(GRID_ROOT_NAME);

        // recreate grid
        if grid.is_some() && editor_state.grid_recreate
        {
            // delete first
            scene.delete_node_by_name("grid origin", true, true, true, true);
            scene.delete_node_by_name("grid", true, true, true, true);

            let grid_size = editor_state.grid_size;
            let grid_amount = editor_state.grid_amount;

            let main_queue_clone = state.main_thread_execution_queue.clone();
            spawn_thread(move ||
            {
                create_grid(scene_id, main_queue_clone.clone(), grid_amount, grid_size);
            });

            editor_state.grid_recreate = false;
        }

        // update grid position
        if let Some(grid) = grid
        {
            let mut grid = grid.write().unwrap();

            let mut transformation = grid.find_component::<Transformation>();
            if transformation.is_none()
            {
                grid.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Transform")))));
                transformation = grid.find_component::<Transformation>();
            }

            let camera = scene.get_active_camera();
            if let Some(camera) = camera
            {
                let camera_data = camera.get_data();

                let pos = &camera_data.eye_pos;
                let mut pos = Vector3::<f32>::new(pos.x.round(), 0.0, pos.z.round());
                pos = snap_to_grid_vec3(pos, grid_size);

                let transformation = transformation.unwrap();
                component_downcast_mut!(transformation, Transformation);

                pos.y = transformation.get_data().position.y;

                if let Some(move_grid_y_to) = move_grid_y_to { pos.y = move_grid_y_to; }
                else if move_up { pos.y += grid_size; }
                else if move_down { pos.y -= grid_size; }

                if !approx_equal_vec(&pos, &transformation.get_data().position)
                {
                    transformation.set_translation(Vector3::<f32>::new(pos.x, pos.y, pos.z));
                }
            }
        }
    }
}
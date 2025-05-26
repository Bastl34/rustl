use std::{f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{Point3, Vector3, Vector4};

use crate::{component_downcast_mut, helper::{concurrency::execution_queue::ExecutionQueueItem, math::is_almost_integer}, state::scene::{components::{component::Component, material::{Material, MaterialItem}, mesh::Mesh, transformation::Transformation}, instance::Instance, manager::id_manager, node::Node, scene::Scene, utilities::scene_utils::{execute_on_scene_mut_and_wait, load_object}}};

use super::helper::set_internal_tag_for_utils_nodes;

pub fn create_grid(scene_id: u64, parent_node_id: Option<u64>, main_queue: ExecutionQueueItem, amount: u32, spacing: f32)
{
    let integer_grid_line_scale = 3.0;

    let grid_origin_line_scale = 3.5;
    let grid_origin_line_scale_line = 1_000.0;

    let amount = amount as i32;

    let size = amount as f32 * spacing;

    let loaded_ids_grid = load_object("objects/grid/grid_line.gltf", scene_id, parent_node_id, main_queue.clone(), true, true, false, 0).unwrap();
    let loaded_ids_origin = load_object("objects/grid/grid_line_extruded.glb", scene_id, parent_node_id, main_queue.clone(), true, true, false, 0).unwrap();

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        // ********** renaming **********
        // origin lines
        if let Some(root) = loaded_ids_origin.get(0)
        {
            if let Some(root_node) = scene.find_node_by_id(*root)
            {
                {
                    root_node.write().unwrap().name = "grid origin root".to_string();
                }

                // move to front
                if let Some(parent) = &root_node.read().unwrap().parent
                {
                    parent.write().unwrap().move_to_front(root_node.clone());
                }
            }
        }
        for (i, id) in loaded_ids_origin.iter().enumerate()
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
        if let Some(root) = loaded_ids_grid.get(0)
        {
            if let Some(root_node) = scene.find_node_by_id(*root)
            {
                root_node.write().unwrap().name = "grid root".to_string();

                // move to front
                if let Some(parent) = &root_node.read().unwrap().parent
                {
                    parent.write().unwrap().move_to_front(root_node.clone());
                }
            }
        }

        // ********** grid **********
        if let Some(grid_arc) = scene.find_mesh_node_by_ids(&loaded_ids_grid)
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
                    let id = id_manager::get_next_instance_id();
                    let uuid = uuid::Uuid::new_v4().to_string();
                    let mut instance = Instance::new
                    (
                        id,
                        uuid,
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
                    let id = id_manager::get_next_instance_id();
                    let uuid = uuid::Uuid::new_v4().to_string();
                    let mut instance = Instance::new
                    (
                        id,
                        uuid,
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
                    material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(0.28, 0.66, 0.9);
                }
            }
        }

        // ********** merge together grid mesh **********
        for id in &loaded_ids_grid
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
            }
        }

        // ********** grid origin **********
        if let Some(grid_arc) = scene.find_mesh_node_by_ids(&loaded_ids_origin)
        {
            {
                let mut grid = grid_arc.write().unwrap();
                grid.clear_instances();
            }

            // x (red)
            {
                let id = id_manager::get_next_instance_id();
                let uuid = uuid::Uuid::new_v4().to_string();
                let mut instance = Instance::new(id, uuid, "grid_origin_x".to_string(), grid_arc.clone());

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
                let id = id_manager::get_next_instance_id();
                let uuid = uuid::Uuid::new_v4().to_string();
                let mut instance = Instance::new(id, uuid, "grid_origin_y".to_string(), grid_arc.clone());

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
                let id = id_manager::get_next_instance_id();
                let uuid = uuid::Uuid::new_v4().to_string();
                let mut instance = Instance::new(id, uuid, "grid_origin_z".to_string(), grid_arc.clone());

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

            /*
            {
                let grid = grid_arc.read().unwrap();

                if let Some(material) = grid.find_component::<Material>()
                {
                    component_downcast_mut!(material, Material);
                    material.get_base_mut().name = "grid origin material".to_string();
                    material.get_data_mut().get_mut().unlit_shading = true;
                    material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(1.0, 1.0, 1.0);
                }
            }
             */
        }

        // ********** create plane **********
        if let Some(grid_arc) = scene.find_mesh_node_by_ids(&loaded_ids_grid)
        {
            let half_size = size / 2.0;

            let p0 = Point3::<f32>::new(-half_size, -0.001, half_size);
            let p1 = Point3::<f32>::new(half_size, -0.001, half_size);
            let p2 = Point3::<f32>::new(half_size, -0.001, -half_size);
            let p3 = Point3::<f32>::new(-half_size, -0.001, -half_size);

            let plane_mesh = Mesh::new_plane("grid plane mesh", p0, p1, p2, p3);

            let mut plane_material = Material::new("grid plane material");
            plane_material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(0.005, 0.005, 0.02);
            plane_material.get_data_mut().get_mut().alpha = 0.5;
            plane_material.get_data_mut().get_mut().unlit_shading = true;

            let plane_material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(plane_material)));

            scene.add_material(&plane_material_arc.clone());

            let uuid = uuid::Uuid::new_v4().to_string();
            let plane_node = Node::new(id_manager::get_next_node_id(), uuid, "plane");
            {
                {
                    let mut plane_node = plane_node.write().unwrap();
                    plane_node.add_component(Arc::new(RwLock::new(Box::new(plane_mesh))));
                    plane_node.add_component(plane_material_arc);
                }

                let instance_id = id_manager::get_next_instance_id();
                let uuid = uuid::Uuid::new_v4().to_string();

                plane_node.write().unwrap().create_default_instance(plane_node.clone(), instance_id, uuid);
                plane_node.write().unwrap().find_instance_by_id(instance_id).unwrap().write().unwrap().pickable = false;
            }

            Node::add_node(grid_arc, plane_node);
        }

        // run internal tagging
        set_internal_tag_for_utils_nodes(scene);
    }));
}
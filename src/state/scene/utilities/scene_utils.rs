use std::{sync::{RwLock, Arc}, f32::consts::PI};

use nalgebra::Vector3;

use crate::{state::scene::{scene::Scene, instance::Instance, components::{transformation::Transformation, material::Material}}, component_downcast_mut};

pub async fn create_grid(scene: &mut Scene, amount: u32, spacing: f32)
{
    let amount = amount as i32;

    let loaded_ids = scene.load("objects/grid/grid.gltf", false).await.unwrap();
    if let Some(grid_arc) = scene.find_node_by_name("grid")
    {
        {
            let mut grid = grid_arc.write().unwrap();
            grid.clear_instances();
        }

        //grid.
        for i in 0..amount
        {
            let pos = i - (amount / 2);

            // x
            {
                let mut instance = Instance::new
                (
                    scene.id_manager.get_next_instance_id(),
                    format!("grid_x_{}", pos),
                    grid_arc.clone()
                );

                let mut transformation = Transformation::identity(scene.id_manager.get_next_component_id(), "Transform");
                transformation.apply_translation(Vector3::<f32>::new(pos as f32 * spacing, 0.0, 0.0));
                transformation.apply_scale(Vector3::<f32>::new(1.0, amount as f32 * spacing, 1.0), true);

                instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));

                let mut grid = grid_arc.write().unwrap();
                grid.add_instance(Box::new(instance));
            }

            // y
            {
                let mut instance = Instance::new
                (
                    scene.id_manager.get_next_instance_id(),
                    format!("grid_y_{}", pos),
                    grid_arc.clone()
                );

                let mut transformation = Transformation::identity(scene.id_manager.get_next_component_id(), "Transform");
                transformation.apply_translation(Vector3::<f32>::new(0.0, pos as f32 * spacing, 0.0));
                transformation.apply_rotation(Vector3::<f32>::new(0.0, 0.0, PI / 2.0));
                transformation.apply_scale(Vector3::<f32>::new(1.0, amount as f32 * spacing, 1.0), true);

                instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));

                let mut grid = grid_arc.write().unwrap();
                grid.add_instance(Box::new(instance));
            }

            {
                let grid = grid_arc.read().unwrap();

                if let Some(transformation) = grid.find_component::<Transformation>()
                {
                    component_downcast_mut!(transformation, Transformation);
                    transformation.get_data_mut().get_mut().rotation = Vector3::<f32>::new(PI / 2.0, 0.0, 0.0);
                    transformation.calc_transform();
                }

                if let Some(material) = grid.find_component::<Material>()
                {
                    component_downcast_mut!(material, Material);
                    material.get_data_mut().get_mut().unlit_shading = true;
                }
            }
        }
    }

    // merge together
    for id in loaded_ids
    {
        if let Some(node) = scene.find_node_by_id(id)
        {
            let mut node = node.write().unwrap();
            node.merge_instances();

            let instance = node.instances.get_mut().first();
            instance.unwrap().borrow_mut().pickable = false;
        }
    }
}
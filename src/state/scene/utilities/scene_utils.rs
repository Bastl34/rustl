use std::{sync::{RwLock, Arc}, f32::consts::PI, path::Path};

use nalgebra::{Point3, Vector3};

use crate::{component_downcast_mut, helper::{self, concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, file::{self, get_extension, get_stem}, math::is_almost_integer}, output::audio_device::AudioDevice, resources::resources::{self, load_binary}, state::{scene::{self, components::{component::{Component, ComponentItem}, material::{Material, MaterialItem, TextureState, TextureType}, mesh::Mesh, sound::{Sound, SoundType}, transformation::Transformation}, instance::Instance, loader::wavefront, manager::id_manager::IdManagerItem, node::{Node, NodeItem}, scene::Scene, sound_source::SoundSource, texture::{Texture, TextureItem}}, state::State}};
use crate::state::scene::loader::gltf;

pub fn load_object(path: &str, scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, reuse_materials: bool, object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<Vec<u64>>
{
    let extension = Path::new(path).extension();

    if extension.is_none()
    {
        println!("can not load {}", path);
        return Ok(vec![]);
    }
    let extension = extension.unwrap();

    if extension == "obj"
    {
        return wavefront::load(path, scene_id, main_queue, id_manager, reuse_materials, object_only, create_mipmaps, max_texture_resolution);
    }
    else if extension == "gltf" || extension == "glb"
    {
        return gltf::load(path, scene_id, main_queue, id_manager, reuse_materials, object_only, create_mipmaps, max_texture_resolution);
    }

    Ok(vec![])
}

pub fn load_texture_or_reuse(scene_id: u64, main_queue: ExecutionQueueItem, max_tex_res: u32, path: &str, extension: Option<String>) -> anyhow::Result<TextureItem>
{
    let image_bytes = resources::load_binary(path)?;
    let name = file::get_stem(path);

    Ok(load_texture_byte_or_reuse(scene_id, main_queue, max_tex_res, &image_bytes, name.as_str(), extension))
}


pub fn load_texture_byte_or_reuse(scene_id: u64, main_queue: ExecutionQueueItem, max_tex_res: u32, image_bytes: &Vec<u8>, name: &str, extension: Option<String>) -> TextureItem
{
    let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);
    let hash_clone = hash.clone();
    let name_clone = name.to_string();

    let res_texture: Arc<RwLock<Option<TextureItem>>> = Arc::new(RwLock::new(None));
    let res_texture_clone = res_texture.clone();

    let texture_id: Arc<RwLock<Option<u64>>> = Arc::new(RwLock::new(None));
    let texture_id_clone = texture_id.clone();

    let scene_id_clone = scene_id.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();

        // ***** check for reuse *****
        res = main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id_clone)
            {
                if scene.textures.contains_key(&hash_clone)
                {
                    println!("reusing texture {}", name_clone);

                    *res_texture_clone.write().unwrap() = Some(scene.textures.get_mut(&hash_clone).unwrap().clone());
                }
                else
                {
                    let id = scene.id_manager.write().unwrap().get_next_texture_id();
                    *texture_id_clone.write().unwrap() = Some(id);
                }
            }
        }))
    }
    res.join();

    if let Some(texture) = res_texture.read().unwrap().as_ref()
    {
        return texture.clone();
    }

    // ***** if not found -> load *****
    let texture = Texture::new(texture_id.read().unwrap().unwrap(), name, &image_bytes, extension, max_tex_res);
    let arc = Arc::new(RwLock::new(Box::new(texture)));

    // ***** add to scene textures *****
    let scene_id_clone = scene_id.clone();
    let arc_clone = arc.clone();
    let hash_clone = hash.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id_clone)
            {
                scene.textures.insert(hash_clone.clone(), arc_clone.clone());
            }
        }));
    }
    res.join();

    arc
}

pub fn insert_texture_or_reuse(scene_id: u64, main_queue: ExecutionQueueItem, texture: Texture, name: &str) -> TextureItem
{
    let hash = texture.hash.clone();
    let hash_clone = hash.clone();
    let name_clone = name.to_string();

    let res_texture: Arc<RwLock<Option<TextureItem>>> = Arc::new(RwLock::new(None));
    let res_texture_clone = res_texture.clone();

    // ***** check for reuse *****
    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id)
            {
                if scene.textures.contains_key(&hash_clone)
                {
                    println!("reusing texture {}", name_clone);

                    *res_texture_clone.write().unwrap() = Some(scene.textures.get_mut(&hash_clone).unwrap().clone());
                }
            }
        }));
    }
    res.join();

    //if let Some(texture) = res_texture.read().unwrap().as_ref()
    if let Some(texture) = res_texture.read().unwrap().as_ref()
    {
        return texture.clone();
    }

    // ***** if not found -> "load" *****
    let arc = Arc::new(RwLock::new(Box::new(texture)));

    // ***** add to scene textures *****
    let scene_id_clone = scene_id.clone();
    let arc_clone = arc.clone();
    let hash_clone = hash.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id_clone)
            {
                scene.textures.insert(hash_clone.clone(), arc_clone.clone());
            }
        }));
    }
    res.join();

    arc

}

pub fn create_grid(scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, amount: u32, spacing: f32)
{
    let integer_grid_line_scale = 3.0;

    let amount = amount as i32;

    let size = amount as f32 * spacing;

    let loaded_ids = load_object("objects/grid/grid.gltf", scene_id, main_queue.clone(), id_manager, true, true, false, 0).unwrap();

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        if let Some(grid_arc) = scene.find_mesh_node_by_name("grid")
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
                    let id = scene.id_manager.write().unwrap().get_next_instance_id();
                    let mut instance = Instance::new
                    (
                        id,
                        format!("grid_x_{}", pos),
                        grid_arc.clone()
                    );

                    let z_pos = pos as f32 * spacing;
                    let scale = if is_almost_integer(z_pos) { integer_grid_line_scale } else { 1.0 };

                    let component_id = scene.id_manager.write().unwrap().get_next_component_id();
                    let mut transformation = Transformation::identity(component_id, "Transform");
                    transformation.apply_translation(Vector3::<f32>::new(0.0, 0.0, z_pos));
                    transformation.apply_scale(Vector3::<f32>::new(amount as f32 * spacing, scale, scale), true);

                    instance.add_component(Arc::new(RwLock::new(Box::new(transformation))));

                    let mut grid = grid_arc.write().unwrap();
                    grid.add_instance(Box::new(instance));
                }

                // y
                {
                    let id = scene.id_manager.write().unwrap().get_next_instance_id();
                    let mut instance = Instance::new
                    (
                        id,
                        format!("grid_y_{}", pos),
                        grid_arc.clone()
                    );

                    let x_pos = pos as f32 * spacing;
                    let scale = if is_almost_integer(x_pos) { integer_grid_line_scale } else { 1.0 };

                    let component_id = scene.id_manager.write().unwrap().get_next_component_id();
                    let mut transformation = Transformation::identity(component_id, "Transform");
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

        // merge together
        for id in &loaded_ids
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

        // create plane
        if let Some(grid_arc) = scene.find_mesh_node_by_name("grid")
        {
            let mesh_component_id = scene.id_manager.write().unwrap().get_next_component_id();
            let material_id = scene.id_manager.write().unwrap().get_next_component_id();

            let half_size = size / 2.0;

            let p0 = Point3::<f32>::new(-half_size, -0.001, half_size);
            let p1 = Point3::<f32>::new(half_size, -0.001, half_size);
            let p2 = Point3::<f32>::new(half_size, -0.001, -half_size);
            let p3 = Point3::<f32>::new(-half_size, -0.001, -half_size);

            let plane_mesh = Mesh::new_plane(mesh_component_id, "grid plane mesh", p0, p1, p2, p3);

            let mut plane_material = Material::new(material_id, "grid plane material");
            plane_material.get_data_mut().get_mut().base_color = Vector3::<f32>::new(0.005, 0.005, 0.02);
            plane_material.get_data_mut().get_mut().alpha = 0.5;
            plane_material.get_data_mut().get_mut().unlit_shading = true;

            let plane_material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(plane_material)));

            scene.add_material(material_id, &plane_material_arc.clone());

            let plane_node = Node::new(scene.id_manager.write().unwrap().get_next_node_id(), "plane");
            {
                {
                    let mut plane_node = plane_node.write().unwrap();
                    plane_node.add_component(Arc::new(RwLock::new(Box::new(plane_mesh))));
                    plane_node.add_component(plane_material_arc);
                }

                let instance_id = scene.id_manager.write().unwrap().get_next_instance_id();

                plane_node.write().unwrap().create_default_instance(plane_node.clone(), instance_id);
                plane_node.write().unwrap().find_instance_by_id(instance_id).unwrap().write().unwrap().pickable = false;
            }

            Node::add_node(grid_arc, plane_node);
        }
    }));
}

pub fn load_texture(path: &str, main_queue: ExecutionQueueItem, texture_type: TextureType, scene_id: u64, material_id: Option<u64>, mipmapping: bool, max_tex_res: u32)
{
    let extension = get_extension(path);
    let name = get_stem(path);

    let bytes = load_binary(path).unwrap();

    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            // material specific texture
            if let Some(material_id) = material_id
            {
                if let Some(material) = scene.get_material_by_id(material_id)
                {
                    let tex = scene.load_texture_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()), max_tex_res);
                    tex.write().unwrap().get_data_mut().get_mut().mipmapping = mipmapping;

                    component_downcast_mut!(material, Material);
                    material.set_texture(tex, texture_type);
                }
            }
            // scene specific texture
            else
            {
                if texture_type == TextureType::Environment
                {
                    let tex = scene.load_texture_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()), max_tex_res);
                    tex.write().unwrap().get_data_mut().get_mut().mipmapping = mipmapping;

                    let scene_data = scene.get_data_mut();
                    let scene_data = scene_data.get_mut();
                    scene_data.environment_texture = Some(TextureState::new(tex.clone()));

                }
            }
        }
    }));
}

pub fn load_sound(path: &str, main_queue: ExecutionQueueItem, scene_id: u64, sound_component_id: Option<u64>)
{
    let extension = get_extension(path);
    let name = get_stem(path);

    let bytes = load_binary(path).unwrap();

    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            // sound component specific file
            if let Some(sound_component_id) = sound_component_id
            {
                if let Some(sound_component) = scene.get_sound_by_id(sound_component_id)
                {
                    let sound_source = scene.load_sound_source_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()));

                    component_downcast_mut!(sound_component, Sound);
                    sound_component.set_sound_source(sound_source);
                }
            }
            // load sound source without specific sound component
            else
            {
                scene.load_sound_source_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()));
            }
        }
    }));
}

pub fn attach_sound_to_node(path: &str, node_name: &str, spund_type: SoundType,  main_queue: ExecutionQueueItem, scene_id: u64, audio_device: Arc<RwLock<Box<AudioDevice>>>)
{
    let path: String = path.to_string();
    let node_name = node_name.to_string();

    let audio_device = audio_device.clone();
    spawn_thread(move ||
    {
        let audio_device = audio_device.clone();
        let path = path.clone();
        let node_name = node_name.clone();

        execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
        {
            let sound_source_bytes = load_binary(path.as_str());
            if let Ok(sound_source_bytes) = sound_source_bytes
            {
                let sound_souce_id = scene.id_manager.write().unwrap().get_next_sound_source_id();
                let sound_source = Arc::new(RwLock::new(Box::new(SoundSource::new(sound_souce_id, "m16", audio_device.clone(), &sound_source_bytes, Some("ogg".to_string())))));
                let sound_source_clone = sound_source.clone();

                let hash = sound_source.read().unwrap().hash.clone();
                scene.sound_sources.insert(hash, sound_source);

                let cube = scene.find_node_by_name(node_name.as_str());

                if let Some(cube) = cube
                {
                    let mut cube = cube.write().unwrap();

                    let sound_id = scene.id_manager.write().unwrap().get_next_component_id();
                    let mut sound = Sound::new(sound_id, "m16", sound_source_clone, spund_type, true);
                    sound.start();

                    cube.add_component(Arc::new(RwLock::new(Box::new(sound))));
                }
            }
        }));
    });
}

pub fn load_and_retarget_animation(path: &str, scene_id: u64, target_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem) -> anyhow::Result<bool>
{
    let animations = load_object(path, scene_id, main_queue.clone(), id_manager.clone(), false, true, false, 0);

    if let Err(animations) = animations
    {
        return Err(animations);
    }

    let animation_id = animations.unwrap()[0];

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
    {
        let target_root = scene.find_node_by_id(target_id).unwrap();
        let animation_root = scene.find_node_by_id(animation_id).unwrap();

        let target_animation_node = Node::find_animation_node(target_root.clone());
        let retarget_animation = animation_root.read().unwrap().find_child_node_by_name("Armature");

        copy_all_animations(retarget_animation.clone().unwrap(), target_animation_node.unwrap(), scene);

        animation_root.write().unwrap().delete_later();
    }));

    Ok(true)
}


pub fn copy_all_animations(from: NodeItem, to: NodeItem, scene: &Scene)
{
    let animations = from.read().unwrap().get_all_animations();

    for animation in animations
    {
        copy_animation(animation.clone(), to.clone(), scene);
    }
}

pub fn copy_animation(animation_component: ComponentItem, to: NodeItem, scene: &Scene)
{
    let component_id = scene.id_manager.write().unwrap().get_next_component_id();
    let cloned_animation = animation_component.read().unwrap().duplicate(component_id);
    if let Some(cloned_animation) = cloned_animation
    {
        let mut target_node = to.write().unwrap();
        target_node.add_component(cloned_animation);
        target_node.re_target_animations_to_child_nodes();
    }
}

pub fn execute_on_scene_mut_and_wait(main_queue: ExecutionQueueItem, scene_id: u64, func: Box<dyn Fn(&mut Scene) + Send + Sync>)
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

pub fn execute_on_scene_mut(main_queue: ExecutionQueueItem, scene_id: u64, func: Box<dyn Fn(&mut Scene) + Send + Sync>)
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
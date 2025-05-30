#![allow(dead_code)]

use std::{sync::{RwLock, Arc}, path::Path};

use crate::{component_downcast_mut, helper::{self, concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, file::{self, get_extension, get_stem}}, output::audio_device::AudioDevice, resources::resources::{self, load_binary}, state::{scene::{components::{animation::Animation, component::ComponentItem, material::{Material, TextureState, TextureType}, sound::{Sound, SoundType}}, loader::wavefront, node::{Node, NodeItem}, scene::Scene, sound_source::SoundSource, texture::{Texture, TextureItem}}, state::State}};
use crate::state::scene::loader::gltf;

pub fn load_object(path: &str, scene_id: u64, parent_node_id: Option<u64>, main_queue: ExecutionQueueItem, reuse_materials: bool, object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<Vec<u64>>
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
        return wavefront::load(path, scene_id, parent_node_id, main_queue, reuse_materials, object_only, create_mipmaps, max_texture_resolution);
    }
    else if extension == "gltf" || extension == "glb"
    {
        return gltf::load(path, scene_id, parent_node_id, main_queue, reuse_materials, object_only, create_mipmaps, max_texture_resolution);
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
            }
        }))
    }
    res.join();

    if let Some(texture) = res_texture.read().unwrap().as_ref()
    {
        return texture.clone();
    }

    // ***** if not found -> load *****
    let uuid = uuid::Uuid::new_v4().to_string();
    let texture = Texture::new(uuid, name, &image_bytes, extension, max_tex_res);
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

    let filename;
    let extension;
    {
        let path = Path::new(&path);
        filename = String::from(path.file_name().unwrap().to_string_lossy());
        extension = String::from(path.extension().unwrap().to_string_lossy());
    }

    let audio_device = audio_device.clone();
    spawn_thread(move ||
    {
        let audio_device = audio_device.clone();
        let path = path.clone();
        let node_name = node_name.clone();
        let filename = filename.clone();
        let extension = extension.clone();

        execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
        {
            let sound_source_bytes = load_binary(path.as_str());
            if let Ok(sound_source_bytes) = sound_source_bytes
            {
                let uuid = uuid::Uuid::new_v4().to_string();
                let sound_source = Arc::new(RwLock::new(Box::new(SoundSource::new(uuid, filename.as_str(), audio_device.clone(), &sound_source_bytes, Some(extension.clone())))));
                let sound_source_clone = sound_source.clone();

                let hash = sound_source.read().unwrap().hash.clone();
                scene.sound_sources.insert(hash, sound_source);

                let node = scene.find_node_by_name(node_name.as_str());

                if let Some(node) = node
                {
                    let mut node = node.write().unwrap();

                    let mut sound = Sound::new(filename.as_str(), sound_source_clone, spund_type, true);
                    sound.start();

                    node.add_component(Arc::new(RwLock::new(Box::new(sound))));
                }
            }
        }));
    });
}

pub fn load_and_re_target_animation(path: &str, scene_id: u64, target_id: u64, main_queue: ExecutionQueueItem, in_place_joint: Option<&str>) -> anyhow::Result<bool>
{
    let animations = load_object(path, scene_id, None, main_queue.clone(), false, true, false, 0);

    if let Err(animations) = animations
    {
        return Err(animations);
    }

    let animation_id = animations.unwrap()[0];

    let in_place_joint = in_place_joint.map(|s| s.to_string());

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
    {
        let target_root = scene.find_node_by_id(target_id).unwrap();
        let animation_root = scene.find_node_by_id(animation_id).unwrap();

        let target_animation_node = Node::find_animation_node(target_root.clone());
        let retarget_animation = animation_root.read().unwrap().find_child_node_by_name("Armature");

        // copy animations
        let new_animations = clone_all_animations(retarget_animation.clone().unwrap(), target_animation_node.unwrap());

        // in place joint
        if let Some(in_place_joint) = &in_place_joint
        {
            let in_place_joint_node = target_root.read().unwrap().find_child_node_by_name(in_place_joint.as_str());

            if let Some(in_place_joint_node) = &in_place_joint_node
            {
                for animation in new_animations
                {
                    component_downcast_mut!(animation, Animation);
                    animation.in_place_joint_node = Some(in_place_joint_node.clone());
                }
            }
        }

        // delete old animation (not needed)
        animation_root.write().unwrap().delete_later();
    }));

    Ok(true)
}

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
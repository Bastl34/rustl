#![allow(dead_code)]

use std::{collections::HashMap, path::Path, sync::{Arc, Mutex, RwLock}};
use crate::{component_downcast, component_downcast_mut, console_error, helper::{asset_path_descriptor::AssetPathDesciptor, concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, file::{get_extension, get_stem}, option_or_id::OptionOrId}, resources::resources::load_binary, state::scene::{components::{animation::Animation, material::{Material, TextureState, TextureType}, sound::{Sound, SoundType}}, loader::{asset_container::{AssetContainer, SceneAddResult}, gltf, wavefront}, node::Node, utilities::scene_utils::{clone_all_animations, execute_on_scene_mut_and_wait, execute_on_state_mut_and_wait}}};


#[derive(Clone)]
pub struct LoaderOptions
{
    pub path: String,
    pub extension: String,
    pub parent_node_id: Option<u32>,
    pub hide_root_nodes: bool,
    pub reuse_materials: bool,
    pub clear_unused_textures: bool,
    pub object_only: bool,
    pub create_mipmaps: bool,
    pub max_texture_resolution: u32,
}

pub fn load_asset(loader_options: &LoaderOptions) -> anyhow::Result<AssetContainer>
{
    let extension = &loader_options.extension;
    let path = &loader_options.path;

    if extension.is_empty()
    {
        console_error!("can not load {}", path);
        return Ok(AssetContainer::new(loader_options.clone()));
    }

    // ********** load asset **********
    let asset_container = if extension == "obj"
    {
        wavefront::load(&loader_options)
    }
    else if extension == "gltf" || extension == "glb"
    {
        gltf::load(&loader_options)
    }
    else
    {
        console_error!("can not load {}", path);
        return Ok(AssetContainer::new(loader_options.clone()));
    };

    if let Err(e) = asset_container
    {
        return Err(e);
    }

    let mut asset_container = asset_container.unwrap();

    // ********** cleanup **********
    if loader_options.clear_unused_textures
    {
        let mut cleanup_map = HashMap::new();
        for texture in &asset_container.textures
        {
            let mut used = false;
            for material in &asset_container.materials
            {
                component_downcast!(material, Material);
                if material.has_texture_id(texture.read().unwrap().id)
                {
                    used = true;
                    break;
                }
            }

            if !used
            {
                cleanup_map.insert(texture.read().unwrap().id, texture.clone());
            }
        }

        asset_container.textures.retain(|texture|
        {
            !cleanup_map.contains_key(&texture.read().unwrap().id)
        });
    }

    Ok(asset_container)
}

pub fn load_asset_and_add_to_scene(path: &str, scene_id: u32, parent_node_id: Option<u32>, main_queue: ExecutionQueueItem, hide_root_nodes: bool, reuse_materials: bool, clear_unused_textures: bool, object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<SceneAddResult>
{
    let extension = Path::new(path).extension().unwrap_or(&std::ffi::OsStr::new(""));

    let loader_options = LoaderOptions
    {
        path: path.to_string(),
        extension: extension.to_string_lossy().to_string(),
        parent_node_id,
        hide_root_nodes,
        reuse_materials,
        clear_unused_textures,
        object_only,
        create_mipmaps,
        max_texture_resolution,
    };

    let mut asset_container = load_asset(&loader_options)?;

    let result_slot: Arc<Mutex<SceneAddResult>> = Arc::new(Mutex::new(SceneAddResult::default()));
    let result_slot_clone = result_slot.clone();

    execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
    {
        let result = asset_container.apply_to_scene(state, scene_id);
        *result_slot_clone.lock().unwrap() = result;
    }));

    Ok(Arc::try_unwrap(result_slot).unwrap().into_inner().unwrap())
}

pub fn load_texture(path: &str, main_queue: ExecutionQueueItem, texture_type: Option<TextureType>, scene_id: Option<u32>, material_id: Option<u32>, mipmapping: bool, max_tex_res: u32)
{
    let extension = get_extension(path);
    let name = get_stem(path);

    let bytes = load_binary(path).unwrap();

    let texture_path = path.to_string();

    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        let tex = state.load_texture_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()), max_tex_res);
        {
            tex.write().unwrap().get_data_mut().get_mut().mipmapping = mipmapping;

            if tex.read().unwrap().source.is_none()
            {
                tex.write().unwrap().source = Some(AssetPathDesciptor::new_from_path(texture_path.clone()));
            }
        }

        if let Some(scene_id) = scene_id
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id)
            {
                if texture_type == Some(TextureType::Environment)
                {
                    let scene_data = scene.get_data_mut();
                    let scene_data = scene_data.get_mut();
                    scene_data.environment_texture = Some(TextureState::new(tex.clone()));
                }
            }
        }
        else if let Some(material_id) = material_id
        {
            if let Some(texture_type) = texture_type
            {
                for scene in &mut state.scenes
                {
                    if let Some(material) = scene.get_material_by_id(material_id)
                    {
                        component_downcast_mut!(material, Material);
                        material.set_texture(tex.clone(), texture_type);
                    }
                }
            }
        }
    }));
}

pub fn load_sound(path: &str, main_queue: ExecutionQueueItem, sound_component_id: Option<u32>)
{
    let extension = get_extension(path);
    let name = get_stem(path);

    let bytes = load_binary(path).unwrap();

    let mut main_queue = main_queue.write().unwrap();
    main_queue.add(Box::new(move |state|
    {
        let sound_source = state.load_sound_source_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()));

        for scene in &mut state.scenes
        {
            // sound component specific file
            if let Some(sound_component_id) = sound_component_id
            {
                if let Some(sound_component) = scene.get_sound_by_id(sound_component_id)
                {
                    component_downcast_mut!(sound_component, Sound);
                    sound_component.set_sound_source(sound_source.clone());
                }
            }
        }
    }));
}

pub fn load_sound_to_node(path: &str, node_name: &str, spund_type: SoundType, main_queue: ExecutionQueueItem)
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

    spawn_thread(move ||
    {
        let path = path.clone();
        let node_name = node_name.clone();
        let filename = filename.clone();
        let extension = extension.clone();
        let name = get_stem(&path);

        execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
        {
            let sound_source_bytes = load_binary(path.as_str());
            if let Ok(sound_source_bytes) = sound_source_bytes
            {
                let sound_source = state.load_sound_source_byte_or_reuse(&sound_source_bytes, name.as_str(), Some(extension.clone()));

                for scene in &mut state.scenes
                {
                    let node = scene.find_node_by_name(node_name.as_str());

                    if let Some(node) = node
                    {
                        let mut node = node.write().unwrap();

                        let mut sound = Sound::new(filename.as_str(), sound_source.clone(), spund_type, true);
                        sound.start();

                        node.add_component(Arc::new(RwLock::new(Box::new(sound))));
                    }
                }
            }
        }));
    });
}

pub fn load_and_re_target_animation(path: &str, scene_id: u32, target_id: u32, main_queue: ExecutionQueueItem, in_place_joint: Option<&str>) -> anyhow::Result<bool>
{
    let animations = load_asset_and_add_to_scene(path, scene_id, None, main_queue.clone(), false, false, true, true, false, 0);

    if let Err(animations) = animations
    {
        return Err(animations);
    }

    let animations = animations.unwrap();

    let animation_root_id = animations.root_node_ids.get(0);

    if animation_root_id.is_none()
    {
        console_error!("no root node found in animation file {}", path);
        return Ok(false);
    }

    let animation_root_id = *animation_root_id.unwrap();

    let in_place_joint = in_place_joint.map(|s| s.to_string());

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
    {
        let target_root = scene.find_node_by_id(target_id).unwrap();
        let animation_root = scene.find_node_by_id(animation_root_id).unwrap();

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
                    animation.in_place_joint_node = OptionOrId::Some(in_place_joint_node.clone());
                }
            }
        }

        // delete old animation (not needed)
        animation_root.write().unwrap().delete_later();
    }));

    Ok(true)
}

#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};

use crate::{component_downcast_mut, console_error, console_log, console_success, console_warning};
use crate::helper::asset_path_descriptor::AssetPathDesciptor;
use crate::helper::change_tracker::ChangeTracker;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::resolve_relative_path;
use crate::helper::option_or_id::OptionOrId;
use crate::resources::resources::{RESOURCE_SCHEME, load_binary, load_string};
use crate::state::resources::sound_source::SoundSource;
use crate::helper::crypto::get_hash_from_byte_vec;
use crate::helper::file::get_extension;
use crate::state::project::project::{AssetOverride, EDITOR_VIEW_EXTRA, MaterialOverride, SceneObject, ProjectFile, SceneFile, SceneSound, SceneTexture, LoadingGuard, ProjectDoneCallback, ProjectSettings, RESUSE_MATERIALS_TAG};
use crate::state::resources::texture::TextureItem;
use crate::state::resources::utilities::resource_utils::load_texture_byte;
use crate::state::scene::utilities::{extras::Extras, origin::Origin};
use crate::state::scene::utilities::saved_state::{NodeState, add_instance_from_value, apply_component_value, apply_node_state, assign_asset_uuids, capture_asset_baseline, component_from_value, component_value, find_asset_node, merge_patch, node_state_value, set_node_materials};
use crate::state::scene::utilities::scene_utils::set_node_parent;
use crate::state::scene::camera::Camera;
use crate::state::scene::components::component::DeserializationContext;
use crate::state::scene::components::material::{MaterialItem, TextureState};
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::node::NodeItem;
use crate::state::scene::light::Light;
use crate::state::scene::manager::id_manager;
use crate::state::scene::scene_controller::scene_controller::SceneControllerBox;
use crate::state::scene::loader::asset_container::AssetContainer;
use crate::state::scene::loader::loader::{load_asset, LoaderOptions, MaterialCache, TextureCache};
use crate::state::scene::utilities::scene_utils::execute_on_state_mut_and_wait;
use crate::state::state::{Rendering, State};

// ******************** load ********************

pub fn load_editor_project(path: &str) -> Option<ProjectFile>
{
    let json = match load_string(path)
    {
        Ok(json) => json,
        Err(error) =>
        {
            console_error!("failed to load project: {}", error);
            return None;
        },
    };

    match serde_json::from_str::<ProjectFile>(&json)
    {
        Ok(project) => Some(project),
        Err(error) =>
        {
            console_error!("failed to parse project: {}", error);
            None
        },
    }
}

pub fn load_editor_scene(path: &str) -> Option<SceneFile>
{
    let json = match load_string(path)
    {
        Ok(json) => json,
        Err(error) =>
        {
            console_error!("failed to load scene '{}': {}", path, error);
            return None;
        },
    };

    match serde_json::from_str::<SceneFile>(&json)
    {
        Ok(scene) => Some(scene),
        Err(error) =>
        {
            console_error!("failed to parse scene '{}': {}", path, error);
            None
        },
    }
}

// ******************** apply (ProjectFile --> Runtime) ********************

/// A parsed editor object ready to be inserted into a scene.
/// The heavy parsing/decoding is already done; only state-mutation remains.
struct PreparedSceneObject
{
    object: SceneObject, // objects are moved to children
    container: Option<AssetContainer>,
    children: Vec<PreparedSceneObject>,
    override_children: Vec<Vec<PreparedSceneObject>>, // prepared objects of each override
}

// applied after all objects - needs the deserialization context and all nodes
#[derive(Default)]
struct Pending
{
    components: Vec<(NodeItem, Vec<serde_json::Value>)>,
    instances: Vec<(NodeItem, Vec<serde_json::Value>)>,
    materials: Vec<(NodeItem, Vec<String>)>,
    patches: Vec<(NodeItem, bool, serde_json::Value)>, // node, node fields (false for the asset root), patch
    material_patches: Vec<(MaterialItem, serde_json::Value)>,
    parents: Vec<(NodeItem, String)>,
    removed: Vec<NodeItem>,
}

struct LoadSettings
{
    base_path: String,
    create_mipmaps: bool,
    max_tex_res: u32,
}

fn load_editor_object(mut object: SceneObject, settings: &LoadSettings, tex_cache: &mut TextureCache, mat_cache: &mut MaterialCache, progress_callback: &dyn Fn()) -> PreparedSceneObject
{
    let reuse_materials = object.options.reuse_materials_by_name.unwrap_or(false);

    let container = match &object.source
    {
        Some(source) =>
        {
            // resources://... is a bundled asset; pass through so get_path() can resolve it
            // against the resources root. Otherwise resolve relative to the project file.
            let path = if let Some(stripped) = source.strip_prefix(RESOURCE_SCHEME)
            {
                stripped.to_string()
            }
            else
            {
                resolve_relative_path(&settings.base_path, source)
            };
            let extension = Path::new(&path).extension().unwrap_or(std::ffi::OsStr::new("")).to_string_lossy().to_string();
            let loader_options = LoaderOptions
            {
                path,
                extension,
                parent_node_id: None,
                hide_root_nodes: true,
                reuse_materials,
                clear_unused_textures: true,
                object_only: true,
                create_mipmaps: settings.create_mipmaps,
                max_texture_resolution: settings.max_tex_res,

                texture_cache: Some(tex_cache.clone()),
                material_cache: Some(mat_cache.clone()),
            };

            match load_asset(&loader_options)
            {
                Ok(asset_container) =>
                {
                    // populate caches from loaded assets
                    for tex in &asset_container.textures
                    {
                        let hash = tex.read().unwrap().hash.clone();
                        if !hash.is_empty()
                        {
                            tex_cache.entry(hash).or_insert_with(|| tex.clone());
                        }
                    }

                    if reuse_materials
                    {
                        for material in &asset_container.materials
                        {
                            let name = material.read().unwrap().get_base().name.clone();
                            if !name.is_empty()
                            {
                                mat_cache.entry(name).or_insert_with(|| material.clone());
                            }
                        }
                    }

                    Some(asset_container)
                },
                Err(e) =>
                {
                    console_error!("failed to load object '{}': {}", source, e);
                    None
                }
            }
        }
        None => None,
    };

    progress_callback();

    let children = std::mem::take(&mut object.objects).into_iter()
        .map(|child| load_editor_object(child, settings, tex_cache, mat_cache, progress_callback))
        .collect();

    let override_children = object.overrides.iter_mut()
        .map(|asset_override| std::mem::take(&mut asset_override.objects).into_iter().map(|child| load_editor_object(child, settings, tex_cache, mat_cache, progress_callback)).collect())
        .collect();

    PreparedSceneObject { object, container, children, override_children }
}

// the extras of the scene object onto the node - origin scene, so the editor saves them again
fn apply_scene_extras(node_extras: &mut Extras, name: &str, extras: &serde_json::Map<String, serde_json::Value>)
{
    for (key, value) in extras
    {
        if !node_extras.insert_json(key, value, Origin::Scene)
        {
            console_warning!("object '{}': extra '{}' has an unsupported type {:?}", name, key, value);
        }
    }
}

fn apply_prepared_object(state: &mut State, scene_id: u32, parent: Option<NodeItem>, prepared: PreparedSceneObject, pending: &mut Pending)
{
    let PreparedSceneObject { object, container, children, override_children } = prepared;
    let SceneObject { source, uuid, name, options, position, rotation, rotation_quat, scale, components, materials, instances, overrides, material_overrides, extras, tags, .. } = object;

    let node: Option<NodeItem> = match container
    {
        Some(mut container) =>
        {
            container.loader_options.parent_node_id = parent.as_ref().map(|p| p.read().unwrap().id);
            let result = container.apply_to_scene(state, scene_id);

            let scene = match state.find_scene_by_id_mut(scene_id) { Some(s) => s, None => return };
            result.root_node_ids.first().and_then(|id| scene.find_node_by_id(*id))
        }
        None =>
        {
            // source-less object: either editor-created empty node, or a failed asset load
            if source.is_some()
            {
                // parse failed earlier (already logged); skip this branch
                return;
            }

            let scene = match state.find_scene_by_id_mut(scene_id) { Some(scene) => scene, None => return };
            Some(scene.add_empty_node(&name, parent.clone()))
        }
    };

    let Some(node) = node else { return; };

    {
        let mut node_write = node.write().unwrap();
        if let Some(uuid) = &uuid { node_write.uuid = uuid.clone(); }
        node_write.origin = Origin::Scene;
        node_write.name = name.clone();
        node_write.settings = options.settings.clone();
        node_write.color = options.color.map(|c| Vector3::new(c[0], c[1], c[2]));

        if source.is_some()
        {
            if let Some(reuse) = options.reuse_materials_by_name
            {
                if reuse { node_write.extras.insert(RESUSE_MATERIALS_TAG, reuse); }
                else     { node_write.extras.remove(RESUSE_MATERIALS_TAG); }
            }
        }

        apply_scene_extras(&mut node_write.extras, &name, &extras);

        for tag in &tags
        {
            node_write.tags.insert_with_origin(tag, Origin::Scene);
        }
    }

    let transform = Transformation::new
    (
        "Transform",
        Vector3::new(position[0], position[1], position[2]),
        Vector3::new(rotation[0], rotation[1], rotation[2]),
        Vector3::new(scale[0], scale[1], scale[2]),
    );
    node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));

    if let Some(quat) = rotation_quat
    {
        if let Some(transform_component) = node.read().unwrap().find_component::<Transformation>()
        {
            component_downcast_mut!(transform_component, Transformation);
            transform_component.apply_rotation_quaternion(Vector4::new(quat[0], quat[1], quat[2], quat[3]), true);
        }
    }

    // asset: stable uuids, baseline (editor), then the saved overrides
    if node.read().unwrap().asset.is_some()
    {
        assign_asset_uuids(&node);

        if cfg!(feature = "editor")
        {
            capture_asset_baseline(&node);
        }

        apply_asset_overrides(state, scene_id, &node, overrides, override_children, material_overrides, pending);
    }

    if !components.is_empty() { pending.components.push((node.clone(), components)); }
    if !materials.is_empty() { pending.materials.push((node.clone(), materials)); }
    if !instances.is_empty() { pending.instances.push((node.clone(), instances)); }

    for child in children
    {
        apply_prepared_object(state, scene_id, Some(node.clone()), child, pending);
    }
}

fn apply_asset_overrides(state: &mut State, scene_id: u32, root: &NodeItem, overrides: Vec<AssetOverride>, override_children: Vec<Vec<PreparedSceneObject>>, material_overrides: Vec<MaterialOverride>, pending: &mut Pending)
{
    let materials = root.read().unwrap().asset.as_ref().map(|asset| asset.materials.clone()).unwrap_or_default();

    for material_override in material_overrides
    {
        match materials.iter().find(|material| material.read().unwrap().get_base().asset_key == material_override.key)
        {
            Some(material) => pending.material_patches.push((material.clone(), material_override.patch)),
            None => { console_warning!("material '{}' ({}) is not in the asset anymore - its changes are dropped", material_override.name, material_override.key); },
        }
    }

    for (asset_override, children) in overrides.into_iter().zip(override_children)
    {
        let Some(node) = find_asset_node(root, &asset_override.key, &asset_override.name) else
        {
            // keeps the override as orphan - its objects are added to the root
            console_warning!("'{}' ({}) is not in the asset anymore - its changes are kept", asset_override.name, asset_override.key);
            if let (Ok(value), Some(asset)) = (serde_json::to_value(&asset_override), root.write().unwrap().asset.as_mut())
            {
                asset.orphans.push(value);
            }

            for child in children
            {
                apply_prepared_object(state, scene_id, Some(root.clone()), child, pending);
            }
            continue;
        };

        if asset_override.removed
        {
            pending.removed.push(node);
            continue;
        }

        {
            let mut node_write = node.write().unwrap();
            apply_scene_extras(&mut node_write.extras, &asset_override.name, &asset_override.extras);

            for tag in &asset_override.tags
            {
                node_write.tags.insert_with_origin(tag, Origin::Scene);
            }
        }

        if let Some(patch) = asset_override.patch { pending.patches.push((node.clone(), !asset_override.key.is_empty(), patch)); }
        if let Some(parent) = asset_override.parent { pending.parents.push((node.clone(), parent)); }
        if !asset_override.components.is_empty() { pending.components.push((node.clone(), asset_override.components)); }
        if !asset_override.instances.is_empty() { pending.instances.push((node.clone(), asset_override.instances)); }

        for child in children
        {
            apply_prepared_object(state, scene_id, Some(node.clone()), child, pending);
        }
    }
}

// the files of the sound resources, read off the main thread: entry, path, bytes
fn load_editor_sounds(sounds: &[SceneSound], base_path: &str) -> Vec<(SceneSound, String, Vec<u8>)>
{
    sounds.iter().filter_map(|sound|
    {
        let path = match sound.source.strip_prefix(RESOURCE_SCHEME)
        {
            Some(resource) => resource.to_string(),
            None => resolve_relative_path(base_path, &sound.source),
        };

        match load_binary(&path)
        {
            Ok(bytes) => Some((sound.clone(), path, bytes)),
            Err(error) =>
            {
                console_error!("can not load sound '{}' ({}): {}", sound.name, sound.source, error);
                None
            }
        }
    }).collect()
}

// loads and decodes the texture files - reuses cached ones with the same bytes
fn load_editor_textures(textures: &[SceneTexture], settings: &LoadSettings, tex_cache: &mut TextureCache) -> Vec<(String, TextureItem)>
{
    textures.iter().filter_map(|texture|
    {
        let path = match texture.source.strip_prefix(RESOURCE_SCHEME)
        {
            Some(resource) => resource.to_string(),
            None => resolve_relative_path(&settings.base_path, &texture.source),
        };

        let bytes = match load_binary(&path)
        {
            Ok(bytes) => bytes,
            Err(error) => { console_error!("can not load texture '{}' ({}): {}", texture.name, texture.source, error); return None; },
        };

        let hash = get_hash_from_byte_vec(&bytes);
        let item = tex_cache.entry(hash).or_insert_with(|| load_texture_byte(settings.max_tex_res, settings.create_mipmaps, &bytes, &texture.name, &path, Some(get_extension(&path)))).clone();

        Some((texture.uuid.clone(), item))
    }).collect()
}

// cameras and controllers reference the objects -> applied after them
fn apply_scene_entries(state: &mut State, scene_id: u32, pending: Pending, materials: Vec<serde_json::Value>, cameras: Vec<serde_json::Value>, lights: Vec<serde_json::Value>, controller: Vec<serde_json::Value>)
{
    let textures = state.resources.textures.values().cloned().collect();
    let mesh_resources = state.resources.mesh_resources.values().cloned().collect();
    let sound_sources = state.resources.sound_sources.values().cloned().collect();

    let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == scene_id) else { return; };

    let nodes = scene.list_all_nodes();
    let instances = nodes.iter().flat_map(|node| node.read().unwrap().instances.get_ref().clone()).collect();
    let components = nodes.iter().flat_map(|node| node.read().unwrap().components.clone()).collect();

    let mut context = DeserializationContext
    {
        textures,
        mesh_resources,
        sound_sources,

        scene: &mut **scene,
        nodes,
        instances,
        components,

        io: &mut state.io,
    };

    // scene materials first - nodes refer to them
    for value in materials
    {
        if let Some(material) = component_from_value(value, &mut context)
        {
            context.scene.add_material(&material);
        }
    }

    // asset nodes moved in the editor - before the deletions, a moved node can come from a deleted one
    for (node, parent_uuid) in pending.parents
    {
        let target = context.nodes.iter().find(|node| node.read().unwrap().uuid == parent_uuid).cloned();
        if target.is_none() && !parent_uuid.is_empty()
        {
            console_warning!("'{}': the node it was moved to ({}) is not there anymore", node.read().unwrap().name, parent_uuid);
            continue;
        }

        set_node_parent(&mut *context.scene, node, target, false);
    }

    // asset nodes deleted in the editor
    for node in pending.removed
    {
        let id = node.read().unwrap().id;
        context.scene.delete_node_by_id(id, false, false, false, false);
    }

    for (material, patch) in pending.material_patches
    {
        if let Some(mut value) = component_value(&material)
        {
            merge_patch(&mut value, &patch);
            apply_component_value(&material, value, &mut context);
        }
    }

    for (node, node_fields, patch) in pending.patches
    {
        let mut value = node_state_value(&node.read().unwrap(), node_fields);
        merge_patch(&mut value, &patch);

        match serde_json::from_value::<NodeState>(value)
        {
            Ok(node_state) => apply_node_state(&node, node_state, &mut context),
            Err(e) => { console_error!("failed to apply the changes of '{}': {}", node.read().unwrap().name, e); },
        }
    }

    for (node, uuids) in pending.materials
    {
        set_node_materials(&node, &uuids, &context);
    }

    // before the controllers - they refer to them by uuid
    for (node, values) in pending.components
    {
        for value in values
        {
            if let Some(component) = component_from_value(value, &mut context)
            {
                node.write().unwrap().add_component(component);
            }
        }
    }

    for (node, values) in pending.instances
    {
        for value in values
        {
            add_instance_from_value(&node, value, &mut context);
        }
    }

    for value in lights
    {
        match serde_json::from_value::<Light>(value)
        {
            Ok(mut light) =>
            {
                // ids are runtime only - the saved one may already be taken
                light.id = id_manager::get_next_light_id();
                context.scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            },
            Err(e) => { console_error!("failed to parse light: {}", e); },
        }
    }

    for value in cameras
    {
        match serde_json::from_value::<Camera>(value)
        {
            Ok(mut cam) =>
            {
                cam.id = id_manager::get_next_camera_id();

                if let Some(uuid) = cam.node.id().map(|id| id.to_string())
                {
                    cam.node = match context.nodes.iter().find(|node| node.read().unwrap().uuid == uuid)
                    {
                        Some(node) => OptionOrId::Some(node.clone()),
                        None => OptionOrId::None,
                    };
                }

                if let Some(controller) = cam.controller.as_mut()
                {
                    controller.run_after_deserialize(&mut context);
                }

                context.scene.cameras.push(Box::new(cam));
            },
            Err(e) => { console_error!("failed to parse camera: {}", e); },
        }
    }

    // after the cameras - the character controller looks its camera up by name
    for value in controller
    {
        match serde_json::from_value::<SceneControllerBox>(value)
        {
            Ok(mut controller) =>
            {
                controller.run_after_deserialize(&mut context);
                context.scene.controller.push(controller);
            },
            Err(e) => { console_error!("failed to parse scene controller: {}", e); },
        }
    }
}

// sets the environment texture - needs the loaded textures
fn apply_environment(state: &mut State, scene_id: u32, environment: Option<serde_json::Value>)
{
    let mut texture_state = match environment.map(serde_json::from_value::<TextureState>)
    {
        Some(Ok(texture_state)) => texture_state,
        Some(Err(e)) => { console_error!("failed to parse the environment: {}", e); return; },
        None => return,
    };

    let uuid = texture_state.item.id().map(str::to_string).unwrap_or_default();
    let Some(texture) = state.resources.textures.values().find(|texture| texture.read().unwrap().uuid == uuid).cloned() else
    {
        console_error!("environment texture {} not found", uuid);
        return;
    };

    texture_state.item = OptionOrId::Some(texture);

    if let Some(scene) = state.find_scene_by_id_mut(scene_id)
    {
        scene.get_data_mut().get_mut().environment_texture = Some(texture_state);
    }
}

pub struct SceneLoad
{
    pub file: SceneFile,
    pub path: String, // base for relative sources
    pub source: Option<String>, // the scene file
    pub active: bool,
    pub scene_id: Option<u32>, // replaces the content of this scene - None creates a new scene
}

fn load_editor_scenes_into_state(state: &mut State, loads: Vec<SceneLoad>, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, log_label: String, done_callback: ProjectDoneCallback) -> Vec<u32>
{
    let mut scenes: Vec<(SceneFile, u32, String, bool)> = Vec::new();
    let mut added_ids: Vec<u32> = Vec::new();

    for SceneLoad { file: editor_scene, path, source, active, scene_id } in loads
    {
        let existing = scene_id.filter(|id| state.find_scene_by_id(*id).is_some());
        let id = match existing
        {
            Some(id) => id,
            None => state.add_scene(&editor_scene.name).id,
        };

        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == id)
        {
            scene.name = editor_scene.name.clone();
            scene.active = active;
            scene.source = source.map(AssetPathDesciptor::new_from_path);

            if let Some(settings) = &editor_scene.settings
            {
                // unmarked for a new one: the render scene does not exist yet and builds from these values - a change mark would recreate pipelines it has not created yet
                let data = if existing.is_some() { scene.get_data_mut().get_mut() } else { scene.get_data_mut().get_unmarked_mut() };
                data.max_lights = settings.max_lights;
                data.gamma = settings.gamma;
                data.exposure = settings.exposure;
                data.ibl_diffuse_intensity = settings.ibl_diffuse_intensity;

                // set by apply_environment
                if existing.is_some() || settings.environment.is_some()
                {
                    data.environment_texture = None;
                }
            }

            if let Some(physics) = editor_scene.physics
            {
                scene.physics.settings = physics;
            }

            scene.tags.tags.retain(|_, tag| tag.origin == Origin::Runtime);
            for tag in &editor_scene.tags
            {
                scene.tags.insert_with_origin(tag, Origin::Scene);
            }

            // an existing scene keeps its editor view
            if existing.is_none() && !editor_scene.editor_cameras.is_empty()
            {
                match serde_json::to_string(&editor_scene.editor_cameras)
                {
                    Ok(json) => { scene.extras.insert(EDITOR_VIEW_EXTRA, json); },
                    Err(e) => { console_error!("failed to keep the editor cameras: {}", e); },
                }
            }
        }
        scenes.push((editor_scene, id, path, existing.is_some()));
        added_ids.push(id);
    }

    state.loading_scenes.extend(added_ids.iter().copied());
    let loading_ids = added_ids.clone();

    let main_queue = state.main_thread_execution_queue.clone();
    let create_mipmaps = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    // cache with the loaded textures - no decoding again
    let mut tex_cache: TextureCache = state.resources.textures.iter()
        .filter(|(_, texture)| !texture.read().unwrap().delete_later_request)
        .map(|(hash, texture)| (hash.clone(), texture.clone()))
        .collect();

    fn count_objects(objects: &[SceneObject]) -> usize
    {
        objects.iter().map(|o| 1 + count_objects(&o.objects) + o.overrides.iter().map(|ov| count_objects(&ov.objects)).sum::<usize>()).sum()
    }
    let total_objects: usize = scenes.iter().map(|(s, _, _, _)| count_objects(&s.objects)).sum();

    *loading_state.write().unwrap() = true;
    *loading_progress_state.write().unwrap() = 0.0;

    spawn_thread(move ||
    {
        let _guard = LoadingGuard(loading_state);
        console_log!("loading {} ({} scenes, {} objects)", log_label, scenes.len(), total_objects);

        let loaded_count = Arc::new(RwLock::new(0usize));
        let total = total_objects.max(1);

        for (mut editor_scene, scene_id, base_path, replace) in scenes
        {
            let settings = LoadSettings { base_path, create_mipmaps, max_tex_res };

            // parse pass: share caches across all objects in this scene
            let mut mat_cache: MaterialCache = HashMap::new();

            let loaded_count_callback = loaded_count.clone();
            let progress_callback_state = loading_progress_state.clone();
            let progress_callback = move ||
            {
                let mut count = loaded_count_callback.write().unwrap();
                *count += 1;
                *progress_callback_state.write().unwrap() = *count as f32 / total as f32;
            };

            // before the objects - they use the same cache
            let textures = load_editor_textures(&editor_scene.textures, &settings, &mut tex_cache);

            let loaded_objects: Vec<PreparedSceneObject> = std::mem::take(&mut editor_scene.objects).into_iter()
                .map(|object| load_editor_object(object, &settings, &mut tex_cache, &mut mat_cache, &progress_callback))
                .collect();

            let sounds = load_editor_sounds(&editor_scene.sounds, &settings.base_path);

            let environment = editor_scene.settings.as_ref().and_then(|settings| settings.environment.clone());
            let SceneFile { cameras, lights, controller, materials, .. } = editor_scene;

            // apply pass: single main-thread round-trip for all prepared objects of this scene
            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                // clears here and not earlier - no empty frame while loading
                if replace
                {
                    if let Some(scene) = state.find_scene_by_id_mut(scene_id)
                    {
                        scene.clear(false, false);
                    }
                }

                for (sound, path, bytes) in sounds
                {
                    let mut sound_source = SoundSource::from_file_bytes(&path, &bytes, state.io.audio_device.clone());
                    sound_source.uuid = sound.uuid;
                    sound_source.name = sound.name;
                    state.add_sound_source(sound_source);
                }

                for (uuid, texture) in textures
                {
                    let texture = state.insert_texture_or_reuse(texture, "");
                    let mut texture = texture.write().unwrap();
                    texture.uuid = uuid;
                    texture.origin = Origin::Scene;
                }

                apply_environment(state, scene_id, environment);

                let mut pending = Pending::default();
                for object in loaded_objects
                {
                    apply_prepared_object(state, scene_id, None, object, &mut pending);
                }

                apply_scene_entries(state, scene_id, pending, materials, cameras, lights, controller);
            }));
        }

        // releases the cached textures - the done callback frees unused ones
        drop(tex_cache);

        if let Some(done_callback) = done_callback
        {
            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                done_callback(state);
            }));
        }

        // after the done callback: it may still add cameras and lights
        execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
        {
            for id in loading_ids
            {
                state.finish_scene_loading(id);
            }
        }));

        *loading_progress_state.write().unwrap() = 0.0;
        console_success!("{} loaded", log_label);
    });

    added_ids
}

// loads scene files into existing scenes (replaces their content, keeps the scene ids)
pub fn reload_editor_scenes(state: &mut State, loads: Vec<SceneLoad>, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, done_callback: ProjectDoneCallback)
{
    load_editor_scenes_into_state(state, loads, loading_state, loading_progress_state, "edit state".to_string(), done_callback);
}

// missing settings fall back to the defaults, so nothing is carried over from the previous project
pub fn apply_project_settings(state: &mut State, settings: &ProjectSettings)
{
    let rendering = match settings.rendering.clone().map(serde_json::from_value::<Rendering>)
    {
        Some(Ok(rendering)) => rendering,
        Some(Err(e)) => { console_error!("failed to parse rendering settings: {}", e); Rendering::default() },
        None => Rendering::default(),
    };
    state.rendering.apply_settings(rendering, &state.rendering_adapter);

    let audio = settings.audio.clone().unwrap_or_default();
    state.io.audio_device.write().unwrap().data.get_mut().volume = audio.volume;

    state.window = settings.window.clone().unwrap_or_default();
    state.input = settings.input.clone().unwrap_or_default();
    if !cfg!(feature = "editor") && *state.rendering.fullscreen.get_ref() != state.window.fullscreen
    {
        state.rendering.fullscreen.set(state.window.fullscreen);
    }
}

pub fn apply_editor_project(state: &mut State, project: ProjectFile, path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, done_callback: ProjectDoneCallback)
{
    if project.scenes.is_empty()
    {
        console_error!("no scenes found in project '{}'", project.project.name);
        return;
    }

    state.project = project.project.clone();
    apply_project_settings(state, &project.settings);
    state.delete_all_scenes(true);

    let mut editor_scenes: Vec<SceneLoad> = Vec::new();
    for scene_ref in project.scenes
    {
        let full_path = resolve_relative_path(path, scene_ref.path.as_str());
        let editor_scene = match load_editor_scene(&full_path)
        {
            Some(scene) => scene,
            None => continue,
        };
        editor_scenes.push(SceneLoad { file: editor_scene, path: full_path.clone(), source: Some(full_path), active: scene_ref.active, scene_id: None });
    }

    let log_label = format!("editor project: {}", project.project.name);
    load_editor_scenes_into_state(state, editor_scenes, loading_state, loading_progress_state, log_label, done_callback);
}

pub fn apply_editor_scene(state: &mut State, scene_path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>,) -> Option<u32>
{
    let editor_scene = match load_editor_scene(&scene_path)
    {
        Some(scene) => scene,
        None => return None,
    };

    let log_label = format!("editor scene: {}", editor_scene.name);
    let active = editor_scene.active;
    let load = SceneLoad { file: editor_scene, path: scene_path.to_string(), source: Some(scene_path.to_string()), active, scene_id: None };
    let added_ids = load_editor_scenes_into_state(state, vec![load], loading_state, loading_progress_state, log_label, None);
    added_ids.first().copied()
}

pub fn load_and_apply_project(state: &mut State, path: &str, done_callback: ProjectDoneCallback)
{
    if let Some(project) = load_editor_project(&path)
    {
        // TODO
        let loading = Arc::new(RwLock::new(false));
        let loading_progress = Arc::new(RwLock::new(0.0));

        apply_editor_project(state, project, &path, loading, loading_progress, done_callback);
    }
}

#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};

use crate::{component_downcast_mut, console_error, console_log, console_success};
use crate::helper::asset_path_descriptor::AssetPathDesciptor;
use crate::helper::change_tracker::ChangeTracker;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::resolve_relative_path;
use crate::helper::option_or_id::OptionOrId;
use crate::resources::resources::{RESOURCE_SCHEME, load_binary, load_string};
use crate::state::resources::sound_source::SoundSource;
use crate::state::project::project::{EDITOR_VIEW_EXTRA, SceneObject, SceneObjectOptions, ProjectFile, SceneFile, SceneSound, LoadingGuard, ProjectDoneCallback, ProjectSettings, RESUSE_MATERIALS_TAG};
use crate::state::scene::camera::Camera;
use crate::state::scene::components::component::{ComponentBox, ComponentItem, DeserializationContext};
use crate::state::scene::components::transformation::Transformation;
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
    uuid: Option<String>,
    name: String,
    options: SceneObjectOptions,
    position: [f32; 3],
    rotation: [f32; 3],
    rotation_quat: Option<[f32; 4]>,
    scale: [f32; 3],
    source: Option<String>,
    container: Option<AssetContainer>,
    components: Vec<serde_json::Value>,
    children: Vec<PreparedSceneObject>,
}

// components saved with a node - they need the deserialization context, so they are applied with the controllers
type PendingComponents = Vec<(crate::state::scene::node::NodeItem, Vec<serde_json::Value>)>;

fn load_editor_object(obj: &SceneObject, base_path: &str, create_mipmaps: bool, max_tex_res: u32, tex_cache: &mut TextureCache, mat_cache: &mut MaterialCache, progress_callback: &dyn Fn()) -> PreparedSceneObject
{
    let reuse_materials = obj.options.reuse_materials_by_name.unwrap_or(false);

    let container = match &obj.source
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
                resolve_relative_path(base_path, source)
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
                create_mipmaps,
                max_texture_resolution: max_tex_res,

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

    let mut children = Vec::with_capacity(obj.objects.len());
    for child_object in &obj.objects
    {
        children.push(load_editor_object(child_object, base_path, create_mipmaps, max_tex_res, tex_cache, mat_cache, progress_callback));
    }

    PreparedSceneObject
    {
        uuid: obj.uuid.clone(),
        name: obj.name.clone(),
        options: obj.options.clone(),
        position: obj.position,
        rotation: obj.rotation,
        rotation_quat: obj.rotation_quat,
        scale: obj.scale,
        source: obj.source.clone(),
        container,
        components: obj.components.clone(),
        children,
    }
}

fn apply_prepared_object(state: &mut State, scene_id: u32, parent: Option<crate::state::scene::node::NodeItem>, object: PreparedSceneObject, pending: &mut PendingComponents)
{
    let PreparedSceneObject { uuid, name, options, position, rotation, rotation_quat, scale, source, container, components, children } = object;

    let node: Option<crate::state::scene::node::NodeItem> = match container
    {
        Some(mut container) =>
        {
            container.loader_options.parent_node_id = parent.as_ref().map(|p| p.read().unwrap().id);
            let result = container.apply_to_scene(state, scene_id);

            let scene = match state.find_scene_by_id_mut(scene_id) { Some(s) => s, None => return };

            let mut found = None;
            for id in &result.node_ids
            {
                if let Some(node) = scene.find_node_by_id(*id)
                {
                    if node.read().unwrap().root_node
                    {
                        {
                            let mut node_write = node.write().unwrap();
                            if let Some(uuid) = &uuid { node_write.uuid = uuid.clone(); }
                            node_write.name = name.clone();
                            node_write.settings = options.settings.clone();
                            node_write.settings.transient = false; // it comes from the project file, so it is saved again
                            node_write.color = options.color.map(|c| Vector3::new(c[0], c[1], c[2]));

                            if let Some(reuse) = options.reuse_materials_by_name
                            {
                                if reuse { node_write.extras.insert(RESUSE_MATERIALS_TAG, reuse); }
                                else     { node_write.extras.remove(RESUSE_MATERIALS_TAG); }
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

                        found = Some(node.clone());
                        break;
                    }
                }
            }
            found
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
            let node = scene.add_empty_node(&name, parent.clone());
            {
                let mut node_write = node.write().unwrap();
                if let Some(uuid) = &uuid { node_write.uuid = uuid.clone(); }
                node_write.settings = options.settings.clone();
                node_write.settings.transient = false;
                node_write.color = options.color.map(|c| Vector3::new(c[0], c[1], c[2]));
            }

            let transform = Transformation::new
            (
                "Transform",
                Vector3::new(position[0], position[1], position[2]),
                Vector3::new(rotation[0], rotation[1], rotation[2]),
                Vector3::new(scale[0], scale[1], scale[2]),
            );
            node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));
            Some(node)
        }
    };

    if let Some(node) = node.as_ref().filter(|_| !components.is_empty())
    {
        pending.push((node.clone(), components));
    }

    for child in children
    {
        apply_prepared_object(state, scene_id, node.clone(), child, pending);
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

// cameras and controllers reference the objects -> applied after them
fn apply_scene_entries(state: &mut State, scene_id: u32, node_components: PendingComponents, cameras: Vec<serde_json::Value>, lights: Vec<serde_json::Value>, controller: Vec<serde_json::Value>)
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

    // before the controllers - they refer to them by uuid
    for (node, values) in node_components
    {
        for value in values
        {
            match serde_json::from_value::<ComponentBox>(value)
            {
                Ok(mut component) =>
                {
                    // ids are runtime only
                    component.get_base_mut().id = id_manager::get_next_component_id();
                    component.run_after_deserialize(&mut context);

                    let component: ComponentItem = Arc::new(RwLock::new(component));
                    node.write().unwrap().add_component(component.clone());
                    context.components.push(component);
                },
                Err(e) => { console_error!("failed to parse component of '{}': {}", node.read().unwrap().name, e); },
            }
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

fn load_editor_scenes_into_state(state: &mut State, editor_scenes: Vec<(SceneFile, String, bool)>, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, log_label: String, done_callback: ProjectDoneCallback) -> Vec<u32>
{
    let mut scenes: Vec<(SceneFile, u32, String)> = Vec::new();
    let mut added_ids: Vec<u32> = Vec::new();

    for (editor_scene, full_path, active) in editor_scenes
    {
        let id = state.add_scene(&editor_scene.name).id;
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == id)
        {
            scene.active = active;
            scene.source = Some(AssetPathDesciptor::new_from_path(full_path.clone()));

            if let Some(settings) = &editor_scene.settings
            {
                // unmarked: the render scene does not exist yet and builds from these values - a change mark would recreate pipelines it has not created yet
                let data = scene.get_data_mut().get_unmarked_mut();
                data.max_lights = settings.max_lights;
                data.gamma = settings.gamma;
                data.exposure = settings.exposure;
                data.ibl_diffuse_intensity = settings.ibl_diffuse_intensity;
            }

            if let Some(physics) = editor_scene.physics
            {
                scene.physics.settings = physics;
            }

            if !editor_scene.editor_cameras.is_empty()
            {
                match serde_json::to_string(&editor_scene.editor_cameras)
                {
                    Ok(json) => { scene.extras.insert(EDITOR_VIEW_EXTRA, json); },
                    Err(e) => { console_error!("failed to keep the editor cameras: {}", e); },
                }
            }
        }
        scenes.push((editor_scene, id, full_path));
        added_ids.push(id);
    }

    let main_queue = state.main_thread_execution_queue.clone();
    let create_mipmaps = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    fn count_objects(objects: &[SceneObject]) -> usize
    {
        objects.iter().map(|o| 1 + count_objects(&o.objects)).sum()
    }
    let total_objects: usize = scenes.iter().map(|(s, _, _)| count_objects(&s.objects)).sum();

    *loading_state.write().unwrap() = true;
    *loading_progress_state.write().unwrap() = 0.0;

    spawn_thread(move ||
    {
        let _guard = LoadingGuard(loading_state);
        console_log!("loading {} ({} scenes, {} objects)", log_label, scenes.len(), total_objects);

        let loaded_count = Arc::new(RwLock::new(0usize));
        let total = total_objects.max(1);

        let mut tex_cache: TextureCache = HashMap::new();

        for (editor_scene, scene_id, base_path) in scenes
        {
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

            let mut loaded_objects: Vec<PreparedSceneObject> = Vec::with_capacity(editor_scene.objects.len());
            for object in &editor_scene.objects
            {
                loaded_objects.push(load_editor_object(object, &base_path, create_mipmaps, max_tex_res, &mut tex_cache, &mut mat_cache, &progress_callback));
            }

            let sounds = load_editor_sounds(&editor_scene.sounds, &base_path);

            let SceneFile { cameras, lights, controller, .. } = editor_scene;

            // apply pass: single main-thread round-trip for all prepared objects of this scene
            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                for (sound, path, bytes) in sounds
                {
                    let mut sound_source = SoundSource::from_file_bytes(&path, &bytes, state.io.audio_device.clone());
                    sound_source.uuid = sound.uuid;
                    sound_source.name = sound.name;
                    state.add_sound_source(sound_source);
                }

                let mut node_components = vec![];
                for object in loaded_objects
                {
                    apply_prepared_object(state, scene_id, None, object, &mut node_components);
                }

                apply_scene_entries(state, scene_id, node_components, cameras, lights, controller);
            }));
        }

        if let Some(done_callback) = done_callback
        {
            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                done_callback(state);
            }));
        }

        *loading_progress_state.write().unwrap() = 0.0;
        console_success!("{} loaded", log_label);
    });

    added_ids
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

    let mut editor_scenes: Vec<(SceneFile, String, bool)> = Vec::new();
    for scene_ref in project.scenes
    {
        let full_path = resolve_relative_path(path, scene_ref.path.as_str());
        let editor_scene = match load_editor_scene(&full_path)
        {
            Some(scene) => scene,
            None => continue,
        };
        editor_scenes.push((editor_scene, full_path, scene_ref.active));
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
    let added_ids = load_editor_scenes_into_state(state, vec![(editor_scene, scene_path.to_string(), active)], loading_state, loading_progress_state, log_label, None);
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

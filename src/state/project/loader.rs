#![allow(dead_code)]
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};

use crate::{component_downcast_mut, console_error, console_log, console_success};
use crate::helper::asset_path_descriptor::AssetPathDesciptor;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::resolve_relative_path;
use crate::resources::resources::{RESOURCE_SCHEME, load_string};
use crate::state::project::project::{EditorObject, EditorObjectOptions, EditorProject, EditorScene, LoadingGuard, ProjectDoneCallback, RESUSE_MATERIALS_TAG};
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::loader::asset_container::AssetContainer;
use crate::state::scene::loader::loader::{load_asset, LoaderOptions, MaterialCache, TextureCache};
use crate::state::scene::utilities::scene_utils::execute_on_state_mut_and_wait;
use crate::state::state::State;

// ******************** load ********************

pub fn load_editor_project(path: &str) -> Option<EditorProject>
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

    match serde_json::from_str::<EditorProject>(&json)
    {
        Ok(project) => Some(project),
        Err(error) =>
        {
            console_error!("failed to parse project: {}", error);
            None
        },
    }
}

pub fn load_editor_scene(path: &str) -> Option<EditorScene>
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

    match serde_json::from_str::<EditorScene>(&json)
    {
        Ok(scene) => Some(scene),
        Err(error) =>
        {
            console_error!("failed to parse scene '{}': {}", path, error);
            None
        },
    }
}

// ******************** apply (EditorProject --> Runtime) ********************

/// A parsed editor object ready to be inserted into a scene.
/// The heavy parsing/decoding is already done; only state-mutation remains.
struct PreparedEditorObject
{
    name: String,
    options: EditorObjectOptions,
    position: [f32; 3],
    rotation: [f32; 3],
    rotation_quat: Option<[f32; 4]>,
    scale: [f32; 3],
    source: Option<String>,
    container: Option<AssetContainer>,
    children: Vec<PreparedEditorObject>,
}

fn load_editor_object(obj: &EditorObject, base_path: &str, create_mipmaps: bool, max_tex_res: u32, tex_cache: &mut TextureCache, mat_cache: &mut MaterialCache, progress_callback: &dyn Fn()) -> PreparedEditorObject
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

    PreparedEditorObject
    {
        name: obj.name.clone(),
        options: obj.options.clone(),
        position: obj.position,
        rotation: obj.rotation,
        rotation_quat: obj.rotation_quat,
        scale: obj.scale,
        source: obj.source.clone(),
        container,
        children,
    }
}

fn apply_prepared_object(state: &mut State, scene_id: u32, parent: Option<crate::state::scene::node::NodeItem>, object: PreparedEditorObject)
{
    let PreparedEditorObject { name, options, position, rotation, rotation_quat, scale, source, container, children } = object;

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
                            node_write.name = name.clone();
                            node_write.settings.visible = options.visible;
                            node_write.settings.locked = options.locked;
                            node_write.settings.transient = false;

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
                node_write.settings.visible = options.visible;
                node_write.settings.locked = options.locked;
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

    for child in children
    {
        apply_prepared_object(state, scene_id, node.clone(), child);
    }
}

fn load_editor_scenes_into_state(state: &mut State, editor_scenes: Vec<(EditorScene, String, bool)>, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, log_label: String, done_callback: ProjectDoneCallback) -> Vec<u32>
{
    let mut scenes: Vec<(EditorScene, u32, String)> = Vec::new();
    let mut added_ids: Vec<u32> = Vec::new();

    for (editor_scene, full_path, active) in editor_scenes
    {
        let id = state.add_scene(&editor_scene.name).id;
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == id)
        {
            scene.active = active;
            scene.source = Some(AssetPathDesciptor::new_from_path(full_path.clone()));
        }
        scenes.push((editor_scene, id, full_path));
        added_ids.push(id);
    }

    let main_queue = state.main_thread_execution_queue.clone();
    let create_mipmaps = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    fn count_objects(objects: &[EditorObject]) -> usize
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

            let mut loaded_objects: Vec<PreparedEditorObject> = Vec::with_capacity(editor_scene.objects.len());
            for object in &editor_scene.objects
            {
                loaded_objects.push(load_editor_object(object, &base_path, create_mipmaps, max_tex_res, &mut tex_cache, &mut mat_cache, &progress_callback));
            }

            // apply pass: single main-thread round-trip for all prepared objects of this scene
            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                for object in loaded_objects
                {
                    apply_prepared_object(state, scene_id, None, object);
                }
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

pub fn apply_editor_project(state: &mut State, project: EditorProject, path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, done_callback: ProjectDoneCallback)
{
    if project.scenes.is_empty()
    {
        console_error!("no scenes found in project '{}'", project.project.name);
        return;
    }

    state.delete_all_scenes(true);

    let mut editor_scenes: Vec<(EditorScene, String, bool)> = Vec::new();
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

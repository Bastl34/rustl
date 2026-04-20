#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_downcast_mut, console_error, console_log, console_success};
use rfd;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::asset_path_descriptor::AssetPathDesciptor;
use crate::helper::file::{get_dirname, get_stem, make_relative_path, resolve_relative_path, sanitize_filename, write_string_to_tile};
use crate::gui::editor::editor::{EDITOR_INTERNAL_TAG, RESUSE_MATERIALS_TAG};
use crate::gui::editor::editor_state::{EditorState, LoadingGuard};
use crate::resources::resources::{self, RESOURCE_SCHEME, load_string};
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::loader::loader::{load_asset, LoaderOptions, MaterialCache, TextureCache};
use crate::state::scene::loader::asset_container::AssetContainer;
use crate::state::scene::utilities::scene_utils::execute_on_state_mut_and_wait;
use std::collections::HashMap;
use std::path::Path;
use crate::state::state::{State, ENGINE_INTERNAL_TAG_PREFX};
use crate::state::scene::exporter::serialization_helper::default_true;
use crate::state::scene::exporter::serialization_helper::is_true;
use crate::state::scene::exporter::serialization_helper::is_false;

const PROJECT_FILE_VERSION: &str = "1.0.0";

// ******************** structs ********************

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectFormat
{
    pub generator: String,
    pub version: String,
}

impl Default for EditorProjectFormat
{
    fn default() -> Self
    {
        EditorProjectFormat
        {
            generator: format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).to_string(),
            version: PROJECT_FILE_VERSION.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectData
{
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub license: String,
    pub url: String,

    pub build: u32,

    #[serde(default)]
    pub editing_time_secs: u64,
}

impl Default for EditorProjectData
{
    fn default() -> Self
    {
        EditorProjectData
        {
            name: "Untitled".to_string(),
            version: "0.0.1".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            license: "".to_string(),
            url: "".to_string(),

            build: 1,
            editing_time_secs: 0,
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectSceneRef
{
    pub path: String,

    #[serde(default, skip_serializing_if = "is_false")]
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProject
{
    pub format: EditorProjectFormat,
    pub project: EditorProjectData,

    pub scenes: Vec<EditorProjectSceneRef>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorScene
{
    pub name: String,

    /// Legacy field: read from old scene.json files but never written anymore.
    /// The active state is stored in the project.json instead.
    #[serde(default, skip_serializing)]
    pub active: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<EditorObject>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorObjectOptions
{
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub visible: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_materials_by_name: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorObject
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    pub name: String,
    pub options: EditorObjectOptions,

    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub rotation_quat: Option<[f32; 4]>,
    pub scale: [f32; 3],

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<EditorObject>,
}

// ******************** extraction (Runtime --> EditorProject) ********************

fn scene_file_name(scene_name: &str, project_name: &str) -> String
{
    format!("{}_{}.scene", sanitize_filename(project_name), sanitize_filename(&scene_name.to_lowercase()))
}

fn extract_editor_scene(scene: &crate::state::scene::scene::Scene, path: &str) -> EditorScene
{
    let objects = scene.nodes.iter()
        .filter_map(|node_item| extract_node(node_item, path))
        .collect();

    EditorScene
    {
        name: scene.name.clone(),
        active: scene.active,
        objects,
    }
}

fn extract_node(node_item: &crate::state::scene::node::NodeItem, path: &str) -> Option<EditorObject>
{
    let node = node_item.read().unwrap();

    // skip internal nodes (editor + engine)
    if node.has_tag(EDITOR_INTERNAL_TAG) || node.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
    {
        return None;
    }

    // skip transient nodes (not meant to be saved)
    if node.settings.transient
    {
        return None;
    }

    // source path (optional — None for empty/editor-created nodes)
    // - filesystem assets: stored as project-relative path (e.g. "../resourcesLocal/foo.obj")
    // - bundled `resources/` assets: stored with explicit "resources://" marker so the loader
    //   can dispatch them through `get_path()` instead of resolving relative to the project file
    let source = node.source.as_ref().map(|descriptor|
    {
        let target_path = descriptor.origin_path.clone();
        if let Some(rel) = make_relative_path(path, &target_path)
        {
            rel
        }
        else if resources::exists(&target_path)
        {
            format!("{}{}", RESOURCE_SCHEME, target_path)
        }
        else
        {
            target_path
        }
    });

    let (position, rotation, rotation_quat, scale) = extract_transform(&node);

    let options = EditorObjectOptions
    {
        visible: node.settings.visible,
        locked: node.settings.locked,
        reuse_materials_by_name: node.extras.get::<bool>(RESUSE_MATERIALS_TAG).copied(),
    };

    let objects = node.nodes.iter()
        .filter_map(|child| extract_node(child, path))
        .collect();

    Some(EditorObject
    {
        source,
        name: node.name.clone(),
        options,
        position,
        rotation,
        rotation_quat,
        scale,
        objects,
    })
}

fn extract_transform(node: &crate::state::scene::node::Node) -> ([f32; 3], [f32; 3], Option<[f32; 4]>, [f32; 3])
{
    let transform_component = node.find_component::<Transformation>();

    if let Some(transform_component) = transform_component
    {
        component_downcast!(transform_component, Transformation);
        let data = transform_component.get_data();

        let position = [data.position.x, data.position.y, data.position.z];
        let rotation = [data.rotation.x, data.rotation.y, data.rotation.z];
        let rotation_quat = data.rotation_quat.map(|q| [q.x, q.y, q.z, q.w]);
        let scale = [data.scale.x, data.scale.y, data.scale.z];

        (position, rotation, rotation_quat, scale)
    }
    else
    {
        ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], None, [1.0, 1.0, 1.0])
    }
}

// ******************** save ********************

pub fn save_editor_project(state: &State, editor_state: &mut EditorState, path: &str) -> bool
{
    editor_state.project_data.build += 1;
    editor_state.accumulate_editing_time();

    let project_name = editor_state.project_data.name.clone();
    let base_dir = get_dirname(path);

    let mut project_scenes: Vec<EditorProjectSceneRef> = Vec::new();
    let mut total_objects = 0;
    let mut used_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    for scene in &state.scenes
    {
        let editor_scene = extract_editor_scene(scene, path);
        total_objects += editor_scene.objects.len();

        // determine scene file path: reuse source if set, otherwise generate from name
        let scene_full_path = if let Some(ref descriptor) = scene.source
        {
            descriptor.origin_path.clone()
        }
        else
        {
            let base_name = scene_file_name(&editor_scene.name, &project_name);
            let stem = base_name.trim_end_matches(".scene");
            let mut candidate = if base_dir.is_empty() { base_name.clone() } else { format!("{}/{}.scene", base_dir, stem) };
            let mut suffix = 2;
            while used_paths.contains(&candidate)
            {
                let suffixed = format!("{}_{}", stem, suffix);
                candidate = if base_dir.is_empty() { format!("{}.scene", suffixed) } else { format!("{}/{}.scene", base_dir, suffixed) };
                suffix += 1;
            }
            candidate
        };
        used_paths.insert(scene_full_path.clone());

        let scene_json = match serde_json::to_string_pretty(&editor_scene)
        {
            Ok(json) => json,
            Err(e) =>
            {
                console_error!("failed to serialize scene '{}': {}", editor_scene.name, e);
                return false;
            },
        };

        if let Err(e) = write_string_to_tile(&scene_full_path, scene_json)
        {
            console_error!("failed to save scene file '{}': {}", scene_full_path, e);
            return false;
        }

        let relative = make_relative_path(path, &scene_full_path).unwrap_or(scene_full_path);
        project_scenes.push(EditorProjectSceneRef { path: relative, active: scene.active });
    }

    let project = EditorProject
    {
        project: editor_state.project_data.clone(),
        format: EditorProjectFormat::default(),
        scenes: project_scenes,
    };

    let json = match serde_json::to_string_pretty(&project)
    {
        Ok(json) => json,
        Err(e) =>
        {
            console_error!("failed to serialize project: {}", e);
            return false;
        },
    };

    let full_path = format!("{}.json", path);
    match write_string_to_tile(full_path.as_str(), json)
    {
        Ok(_) =>
        {
            console_success!("project saved: {} ({} scenes, {} objects)", full_path, project.scenes.len(), total_objects);
            true
        },
        Err(e) =>
        {
            console_error!("failed to save project: {}", e);
            false
        },
    }
}

pub fn save_editor_project_with_dialog(editor_state: &mut EditorState, state: &State, force_new_path: bool)
{
    let mut path = editor_state.project_path.clone();
    if editor_state.project_path.is_none() || force_new_path
    {
        path = rfd::FileDialog::new()
            .add_filter("Rustl Project", &["json"])
            .set_file_name(&format!("{}.json", editor_state.project_data.name))
            .save_file()
            .map(|path| path.to_string_lossy().into_owned())
    }

    if let Some(path) = path
    {
        // save_editor_project appends .json, so strip it if already present
        let base = path.strip_suffix(".json").unwrap_or(&path).to_string();

        // derive project name from filename if a new path was chosen
        let stem = get_stem(&base);
        if !stem.is_empty()
        {
            editor_state.project_data.name = stem;
        }

        if save_editor_project(state, editor_state, &base)
        {
            editor_state.project_path = Some(format!("{}.json", base));
        }
    }
}

pub fn load_editor_project_with_dialog(editor_state: &mut EditorState, state: &mut State, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>)
{
    let path = rfd::FileDialog::new()
        .add_filter("Rustl Project", &["json"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned());

    if let Some(path) = path
    {
        if let Some(project) = load_editor_project(&path)
        {
            editor_state.project_data = project.project.clone();
            editor_state.project_path = Some(path.clone());
            editor_state.project_session_start = web_time::Instant::now();
            apply_editor_project(state, project, &path, loading_state, loading_progress_state);
        }
    }
}

pub fn import_editor_scene_with_dialog(state: &mut State, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>)
{
    let path = rfd::FileDialog::new()
        .add_filter("Rustl Scene", &["scene"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned());

    if let Some(path) = path
    {
        apply_editor_scene(state, &path, loading_state, loading_progress_state);
    }
}

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

fn load_editor_scenes_into_state(state: &mut State, editor_scenes: Vec<(EditorScene, String, bool)>, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>, log_label: String)
{
    let mut scenes: Vec<(EditorScene, u32, String)> = Vec::new();

    for (editor_scene, full_path, active) in editor_scenes
    {
        let id = state.add_scene(&editor_scene.name);
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == id)
        {
            scene.active = active;
            scene.source = Some(AssetPathDesciptor::new_from_path(full_path.clone()));
        }
        scenes.push((editor_scene, id, full_path));
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

        *loading_progress_state.write().unwrap() = 0.0;
        console_success!("{} loaded", log_label);
    });
}

pub fn apply_editor_project(state: &mut State, project: EditorProject, path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>,)
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
    load_editor_scenes_into_state(state, editor_scenes, loading_state, loading_progress_state, log_label);
}

pub fn apply_editor_scene(state: &mut State, scene_path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>,)
{
    let editor_scene = match load_editor_scene(&scene_path)
    {
        Some(scene) => scene,
        None => return,
    };

    let log_label = format!("editor scene: {}", editor_scene.name);
    let active = editor_scene.active;
    load_editor_scenes_into_state(state, vec![(editor_scene, scene_path.to_string(), active)], loading_state, loading_progress_state, log_label);
}
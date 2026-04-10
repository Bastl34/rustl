#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_downcast_mut, console_error, console_log, console_success};
use rfd;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::{make_relative_path, resolve_relative_path, write_string_to_tile};
use crate::gui::editor::editor::{EDITOR_INTERNAL_TAG, RESUSE_MATERIALS_TAG};
use crate::gui::editor::editor_state::{EditorState, LoadingGuard};
use crate::resources::resources::load_string;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::utilities::scene_utils::{execute_on_scene_mut_and_wait, load_object};
use crate::state::state::{State, ENGINE_INTERNAL_TAG_PREFX};
use crate::state::scene::exporter::serialization_helper::default_true;
use crate::state::scene::exporter::serialization_helper::is_true;
use crate::state::scene::exporter::serialization_helper::is_false;

// ******************** structs ********************

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectMetadata
{
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub license: String,
    pub url: String,

    pub build: u32,
}

impl Default for EditorProjectMetadata
{
    fn default() -> Self
    {
        EditorProjectMetadata
        {
            name: "Untitled".to_string(),
            version: "0.0.1".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            license: "".to_string(),
            url: "".to_string(),

            build: 1,
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProject
{
    pub metadata: EditorProjectMetadata,
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
    pub source: String,
    pub name: String,
    pub options: EditorObjectOptions,

    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub rotation_quat: Option<[f32; 4]>,
    pub scale: [f32; 3],
}

// ******************** extraction (Runtime --> EditorProject) ********************

pub fn extract_editor_project(state: &State, project_metadata: &EditorProjectMetadata, path: &str) -> EditorProject
{
    let mut objects = Vec::new();

    for scene in &state.scenes
    {
        for node_item in &scene.nodes
        {
            let node = node_item.read().unwrap();

            if !node.root_node
            {
                continue;
            }

            // skip internal nodes (editor + engine)
            if node.has_tag(EDITOR_INTERNAL_TAG) || node.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
            {
                continue;
            }

            // skip transient nodes (not meant to be saved)
            if node.settings.transient
            {
                continue;
            }

            // source path
            let source = match &node.source
            {
                Some(descriptor) => descriptor.origin_path.clone(),
                None => continue, // skip nodes without a source asset
            };

            // make relative path if possible
            let source = make_relative_path(path, &source).unwrap_or(source);

            // transform
            let (position, rotation, rotation_quat, scale) = extract_transform(&node);

            // options
            let options = EditorObjectOptions
            {
                visible: node.settings.visible,
                locked: node.settings.locked,
                reuse_materials_by_name: node.extras.get::<bool>(RESUSE_MATERIALS_TAG).copied(),
            };

            objects.push(EditorObject
            {
                source,
                name: node.name.clone(),
                options,
                position,
                rotation,
                rotation_quat,
                scale,
            });
        }
    }

    EditorProject
    {
        metadata: project_metadata.clone(),
        objects,
    }
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
    editor_state.project_metadata.build += 1;
    let project = extract_editor_project(state, &editor_state.project_metadata, path);

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
            console_success!("project saved: {} ({} objects)", full_path, project.objects.len());
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
            .set_file_name(&format!("{}.json", editor_state.project_metadata.name))
            .save_file()
            .map(|p| p.to_string_lossy().into_owned())
    }

    if let Some(path) = path
    {
        // save_editor_project appends .json, so strip it if already present
        let base = path.strip_suffix(".json").unwrap_or(&path).to_string();
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
            editor_state.project_metadata = project.metadata.clone();
            editor_state.project_path = Some(path.clone());
            apply_editor_project(state, project, &path, loading_state, loading_progress_state);
        }
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

// ******************** apply (EditorProject --> Runtime) ********************

pub fn apply_editor_project(state: &mut State, project: EditorProject, path: &str, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>,)
{
    // get scene id + clear non-internal nodes
    let mut scene_id = None;
    for scene in &mut state.scenes
    {
        scene_id = Some(scene.id);
        scene.clear(false, true);
        break;
    }

    let scene_id = match scene_id
    {
        Some(id) => id,
        None => return,
    };

    let main_queue = state.main_thread_execution_queue.clone();
    let create_mipmaps = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();
    let base_path = path.to_string();

    *loading_state.write().unwrap() = true;
    *loading_progress_state.write().unwrap() = 0.0;

    spawn_thread(move ||
    {
        let _guard = LoadingGuard(loading_state);
        console_log!("loading editor project: {} ({} objects)", project.metadata.name, project.objects.len());

        for (i, obj) in project.objects.iter().enumerate()
        {
            let name = obj.name.clone();
            let options = obj.options.clone();
            let position = obj.position;
            let rotation = obj.rotation;
            let rotation_quat = obj.rotation_quat;
            let scale = obj.scale;

            let path = resolve_relative_path(&base_path, &obj.source);

            let loaded = load_object(&path, scene_id, None, main_queue.clone(), true, options.reuse_materials_by_name.unwrap_or(false), true, create_mipmaps, max_tex_res);

            if loaded.is_err()
            {
                console_error!("failed to load object: {}", obj.source);
                continue;
            }

            let loaded_ids = loaded.unwrap();

            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
            {
                // find the root node from loaded ids
                let mut root_node = None;
                for id in &loaded_ids
                {
                    if let Some(node) = scene.find_node_by_id(*id)
                    {
                        if node.read().unwrap().root_node
                        {
                            root_node = Some(node.clone());
                            break;
                        }
                    }
                }

                if let Some(root_node) = root_node
                {
                    // apply name + visibility + authored flag
                    {
                        let mut node = root_node.write().unwrap();
                        node.name = name.clone();
                        node.settings.visible = options.visible;
                        node.settings.locked = options.locked;
                        node.settings.transient = false;

                        if let Some(reuse_materials_by_name) = options.reuse_materials_by_name
                        {
                            if reuse_materials_by_name
                            {
                                node.extras.insert(RESUSE_MATERIALS_TAG, reuse_materials_by_name);
                            }
                            else
                            {
                                node.extras.remove(RESUSE_MATERIALS_TAG);
                            }
                        }
                    }

                    // apply transform
                    let transform = Transformation::new
                    (
                        "Transform",
                        Vector3::new(position[0], position[1], position[2]),
                        Vector3::new(rotation[0], rotation[1], rotation[2]),
                        Vector3::new(scale[0], scale[1], scale[2]),
                    );

                    root_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));

                    // apply quaternion rotation if present
                    if let Some(quat) = rotation_quat
                    {
                        if let Some(transform_component) = root_node.read().unwrap().find_component::<Transformation>()
                        {
                            component_downcast_mut!(transform_component, Transformation);
                            transform_component.apply_rotation_quaternion(Vector4::new(quat[0], quat[1], quat[2], quat[3]), true);
                        }
                    }
                }
            }));

            *loading_progress_state.write().unwrap() = (i + 1) as f32 / project.objects.len() as f32;
        }

        *loading_progress_state.write().unwrap() = 0.0;

        console_success!("editor project loaded");
    });
}


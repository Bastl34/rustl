#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_downcast_mut, console_error, console_log, console_success};
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::{make_relative_path, resolve_relative_path, write_string_to_tile};
use crate::gui::editor::editor::{EDITOR_INTERNAL_TAG, RESUSE_MATERIALS_TAG};
use crate::gui::editor::editor_state::{EditorState, LoadingGuard};
use crate::resources::resources::load_string;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::utilities::scene_utils::{execute_on_scene_mut_and_wait, load_object};
use crate::state::state::{State, ENGINE_INTERNAL_TAG_PREFX};

// ******************** structs ********************

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProject
{
    pub version: String,
    pub name: String,
    pub objects: Vec<EditorObject>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorObjectOptions
{
    pub visible: bool,
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

pub fn extract_editor_project(state: &State, project_name: &str, path: &str) -> EditorProject
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
        version: "1.0.0".to_string(),
        name: project_name.to_string(),
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

pub fn save_editor_project(state: &State, editor_state: &EditorState, path: &str) -> bool
{
    let project = extract_editor_project(state, &editor_state.project_name, path);

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

// ******************** load ********************

pub fn load_editor_project(path: &str) -> Option<EditorProject>
{
    let json = match load_string(format!("{}.json", path).as_str())
    {
        Ok(json) => json,
        Err(_) => return None,
    };

    match serde_json::from_str::<EditorProject>(&json)
    {
        Ok(project) => Some(project),
        Err(_) => None,
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
        console_log!("loading editor project: {} ({} objects)", project.name, project.objects.len());

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


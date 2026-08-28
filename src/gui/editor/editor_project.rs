#![allow(dead_code)]

//! Editor-side project handling: extracting the running scene graph into the
//! on-disk project format, saving it, and the file dialogs around it.
//!
//! The format itself and the load/apply path live in `state::project` because
//! the runtime needs them without the editor.

use std::sync::{Arc, RwLock};

use crate::{component_downcast, console_error, console_success};
use rfd;
use crate::helper::console_log::LogType;
use crate::helper::file::{get_dirname, get_stem, make_relative_path, sanitize_filename, write_string_to_tile};
use crate::gui::editor::editor::EDITOR_INTERNAL_TAG;
use crate::gui::editor::editor_state::EditorState;
use crate::resources::resources::{self, RESOURCE_SCHEME};
use crate::state::project::loader::{apply_editor_project, apply_editor_scene, load_editor_project};
use crate::state::project::project::{EditorObject, EditorObjectOptions, EditorProject, EditorProjectFormat, EditorProjectSceneRef, EditorScene, RESUSE_MATERIALS_TAG};
use crate::state::scene::components::transformation::Transformation;
use crate::state::state::{ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX, State};

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
        // skip engine/editor-internal scenes (e.g. the preview scene) — they are not part of the project
        let is_internal = scene.has_tag(ENGINE_INTERNAL_TAG) || scene.has_tag(EDITOR_INTERNAL_TAG);
        if is_internal
        {
            continue;
        }

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

    let full_path = format!("{}.project", path);
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

pub fn save_editor_project_with_dialog(editor_state: &mut EditorState, state: &State, force_new_path: bool) -> Option<String>
{
    let mut path = editor_state.project_path.clone();
    if editor_state.project_path.is_none() || force_new_path
    {
        path = rfd::FileDialog::new()
            .add_filter("Rustl Project", &["project"])
            .set_file_name(&format!("{}.project", editor_state.project_data.name))
            .save_file()
            .map(|path| path.to_string_lossy().into_owned())
    }

    if let Some(path) = &path
    {
        // save_editor_project appends .project, so strip it if already present
        let base = path.strip_suffix(".project").unwrap_or(&path).to_string();

        // derive project name from filename if a new path was chosen
        let stem = get_stem(&base);
        if !stem.is_empty()
        {
            editor_state.project_data.name = stem;
        }

        if save_editor_project(state, editor_state, &base)
        {
            editor_state.project_path = Some(format!("{}.project", base));
        }
    }

    path
}

pub fn load_editor_project_with_dialog(editor_state: &mut EditorState, state: &mut State, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>)
{
    let path = rfd::FileDialog::new()
        .add_filter("Rustl Project", &["project"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned());

    if let Some(path) = path
    {
        load_editor_project_from_path(editor_state, state, path, loading_state, loading_progress_state);
    }
}

pub fn load_editor_project_from_path(editor_state: &mut EditorState, state: &mut State, path: String, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>)
{
    if let Some(project) = load_editor_project(&path)
    {
        editor_state.project_data = project.project.clone();
        editor_state.project_path = Some(path.clone());
        editor_state.project_session_start = web_time::Instant::now();
        editor_state.recent_projects.add_and_save(path.clone());
        apply_editor_project(state, project, &path, loading_state, loading_progress_state, None);
    }
    else
    {
        // load_editor_project already logged the concrete reason (missing file or broken json)
        editor_state.alert("Project", &format!("Project can not be loaded:\n{}\n\nSee the console for details.", path), LogType::Error);
    }
}

pub fn import_editor_scene_with_dialog(state: &mut State, loading_state: Arc<RwLock<bool>>, loading_progress_state: Arc<RwLock<f32>>) -> Option<u32>
{
    let path = rfd::FileDialog::new()
        .add_filter("Rustl Scene", &["scene"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned());

    if let Some(path) = path
    {
        apply_editor_scene(state, &path, loading_state, loading_progress_state)
    }
    else
    {
        None
    }
}

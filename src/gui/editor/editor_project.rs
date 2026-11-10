#![allow(dead_code)]

//! Editor-side project handling: extracting the running scene graph into the
//! on-disk project format, saving it, and the file dialogs around it.
//!
//! The format itself and the load/apply path live in `state::project` because
//! the runtime needs them without the editor.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::{component_downcast, console_error, console_success, console_warning};
use rfd;
use crate::helper::console_log::LogType;
use crate::helper::observable::ObserverId;
use crate::helper::file::{get_dirname, get_stem, make_relative_path, sanitize_filename, write_string_to_tile};
use crate::gui::editor::editor::EDITOR_INTERNAL_TAG;
use crate::gui::editor::editor_state::EditorState;
use crate::gui::editor::project_code;
use crate::resources::resources::{self, RESOURCE_SCHEME};
use crate::state::project::loader::{SceneLoad, apply_editor_project, apply_editor_scene, apply_project_settings, load_editor_project, reload_editor_scenes};
use crate::state::project::project::ProjectDoneCallback;
use crate::state::scene::utilities::origin::Origin;
use crate::state::project::project::{AssetOverride, AudioSettings, MaterialOverride, SceneObject, SceneObjectOptions, ProjectFile, ProjectFileFormat, ProjectSceneRef, SceneFile, SceneSettings, SceneSound, SceneTexture, ProjectSettings, RESUSE_MATERIALS_TAG};
use crate::state::resources::texture::TextureItem;
use crate::state::scene::components::component::Component;
use crate::state::scene::components::material::Material;
use crate::state::scene::node::{Node, NodeItem};
use crate::state::scene::scene::Scene;
use crate::state::scene::utilities::extras::Extras;
use crate::state::scene::utilities::saved_state::{component_value, instance_value, material_uuids, merge_diff, node_state_value, scene_component_values};
use crate::state::resources::sound_source::SoundSourceItem;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::scene_controller::scene_controller::SceneControllerBox;
use crate::state::state::{ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX, RunMode, State};

// ******************** extraction (Runtime --> ProjectFile) ********************

fn scene_file_name(scene_name: &str, project_name: &str) -> String
{
    format!("{}_{}.scene", sanitize_filename(project_name), sanitize_filename(&scene_name.to_lowercase()))
}

// the sound resources with their files - the scene brings them for its controllers
fn extract_sounds(sound_sources: &[SoundSourceItem], path: &str) -> Vec<SceneSound>
{
    sound_sources.iter().filter_map(|source|
    {
        let source = source.read().unwrap();

        let Some(origin) = source.origin_path() else
        {
            console_warning!("sound '{}' has no file - not saved", source.name);
            return None;
        };

        Some(SceneSound { uuid: source.uuid.clone(), name: source.name.clone(), source: asset_source_path(origin, path) })
    }).collect()
}

// bundled resources as "resources://...", other files relative to the project file if possible
fn asset_source_path(origin: &str, path: &str) -> String
{
    if let Some(resource) = resources::to_resource_path(origin)
    {
        format!("{}{}", RESOURCE_SCHEME, resource)
    }
    else
    {
        make_relative_path(path, origin).unwrap_or_else(|| origin.to_string())
    }
}

fn extract_project_settings(state: &State) -> ProjectSettings
{
    let volume = state.io.audio_device.read().unwrap().data.get_ref().volume;

    ProjectSettings
    {
        rendering: to_json_value("rendering settings", "rendering", &state.rendering),
        audio: Some(AudioSettings { volume }),
        window: Some(state.window.clone()),
        input: Some(state.input.clone()),
    }
}

// state of one scene extraction
struct Extraction<'a>
{
    path: &'a str,
    nodes_by_id: HashMap<u32, NodeItem>, // all nodes of the scene - used to detect deleted asset nodes
    textures: Vec<TextureItem>, // textures used by the saved materials and the environment
}

// scene_textures: the scene textures of the resources - saved in every scene file, like the sounds
pub fn extract_editor_scene(scene: &Scene, sounds: Vec<SceneSound>, scene_textures: &[TextureItem], path: &str) -> SceneFile
{
    let nodes_by_id = scene.list_all_nodes().into_iter().map(|node| { let id = node.read().unwrap().id; (id, node) }).collect();
    let mut extraction = Extraction { path, nodes_by_id, textures: scene_textures.to_vec() };

    let objects = scene.nodes.iter()
        .filter_map(|node_item| extract_node(node_item, &mut extraction))
        .collect();

    // only scene materials - asset materials are loaded with their asset
    let materials = scene.materials.values()
        .filter(|material| { let material = material.read().unwrap(); material.get_base().origin == Origin::Scene && !material.get_base().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX) })
        .filter_map(|material|
        {
            component_downcast!(material, Material);
            extraction.textures.extend(material.get_all_textures());
            to_json_value("material", &material.get_base().name, material as &dyn Component)
        })
        .collect();

    // internal ones (editor cams, ...) belong to the editor, not to the scene
    let cameras = scene.cameras.iter()
        .filter(|cam| !cam.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX))
        .filter_map(|cam| to_json_value("camera", &cam.name, &**cam))
        .collect();

    let lights = scene.lights.get_ref().iter()
        .filter_map(|light|
        {
            let light = light.borrow();
            let light = light.get_ref();

            if light.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
            {
                return None;
            }

            to_json_value("light", &light.name, &**light)
        })
        .collect();

    let editor_cameras = extract_editor_cameras(scene);

    let data = scene.get_data();
    if let Some(texture) = data.environment_texture.as_ref().and_then(|state| state.get())
    {
        extraction.textures.push(texture.clone());
    }

    let settings = SceneSettings
    {
        max_lights: data.max_lights,
        environment: data.environment_texture.as_ref().and_then(|state| to_json_value("environment", "texture", state)),
        gamma: data.gamma,
        exposure: data.exposure,
        ibl_diffuse_intensity: data.ibl_diffuse_intensity,
    };

    SceneFile
    {
        name: scene.name.clone(),
        active: scene.active,
        settings: Some(settings),
        physics: Some(scene.physics.settings),
        tags: scene.tags.scene_tags(),
        editor_cameras,
        sounds,
        textures: extract_textures(&extraction.textures, path),
        materials,
        objects,
        cameras,
        lights,
        controller: extract_controllers(&scene.controller),
    }
}

fn scene_textures(state: &State) -> Vec<TextureItem>
{
    state.resources.textures.values()
        .filter(|texture| { let texture = texture.read().unwrap(); texture.origin == Origin::Scene && !texture.delete_later_request })
        .cloned()
        .collect()
}

fn extract_editor_cameras(scene: &Scene) -> Vec<serde_json::Value>
{
    scene.cameras.iter()
        .filter(|cam| cam.tags.contains(EDITOR_INTERNAL_TAG))
        .filter_map(|cam| to_json_value("editor camera", &cam.name, &**cam))
        .collect()
}

// file entries of the textures - skips textures inside an asset file
fn extract_textures(textures: &[TextureItem], path: &str) -> Vec<SceneTexture>
{
    let mut seen = HashSet::new();

    textures.iter().filter_map(|texture|
    {
        let texture = texture.read().unwrap();
        if !seen.insert(texture.uuid.clone())
        {
            return None;
        }

        let Some(source) = &texture.source else
        {
            console_warning!("texture '{}' has no file - not saved", texture.name);
            return None;
        };

        if !source.inner_path.is_empty() || !source.variation.is_empty()
        {
            return None;
        }

        Some(SceneTexture { uuid: texture.uuid.clone(), name: texture.name.clone(), source: asset_source_path(&source.origin_path, path) })
    }).collect()
}

fn extract_controllers(controllers: &[SceneControllerBox]) -> Vec<serde_json::Value>
{
    controllers.iter()
        .filter(|controller| controller.is_serializable())
        .filter_map(|controller| to_json_value("scene controller", &controller.get_base().name, controller))
        .collect()
}

fn to_json_value<T: serde::Serialize + ?Sized>(kind: &str, name: &str, item: &T) -> Option<serde_json::Value>
{
    match serde_json::to_value(item)
    {
        Ok(value) => Some(value),
        Err(e) =>
        {
            console_error!("failed to serialize {} '{}': {}", kind, name, e);
            None
        },
    }
}

fn scene_extras(extras: &Extras) -> serde_json::Map<String, serde_json::Value>
{
    extras.iter_with_origin()
        .filter(|(_, _, origin)| *origin == Origin::Scene)
        .filter_map(|(key, _, _)| extras.get_json(key).map(|value| (key.clone(), value)))
        .collect()
}

fn scene_instances(node: &Node) -> Vec<serde_json::Value>
{
    node.instances.get_ref().iter()
        .filter(|instance| instance.read().unwrap().origin == Origin::Scene)
        .filter_map(instance_value)
        .collect()
}

fn extract_node(node_item: &NodeItem, extraction: &mut Extraction) -> Option<SceneObject>
{
    let node = node_item.read().unwrap();

    // only scene nodes - asset nodes are saved as overrides, runtime nodes are skipped
    if node.origin != Origin::Scene || node.has_tag(EDITOR_INTERNAL_TAG) || node.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
    {
        return None;
    }

    let path = extraction.path;

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

    let options = SceneObjectOptions
    {
        reuse_materials_by_name: node.extras.get::<bool>(RESUSE_MATERIALS_TAG).copied(),
        color: node.color.map(|color| [color.x, color.y, color.z]),
        settings: node.settings.clone(),
    };

    let (overrides, material_overrides) = extract_asset_overrides(&node, extraction);

    let objects = node.nodes.iter()
        .filter_map(|child| extract_node(child, extraction))
        .collect();

    Some(SceneObject
    {
        source,
        uuid: Some(node.uuid.clone()),
        name: node.name.clone(),
        options,
        position,
        rotation,
        rotation_quat,
        scale,
        components: scene_component_values(&node.components),
        materials: material_uuids(&node),
        instances: scene_instances(&node),
        overrides,
        material_overrides,
        extras: scene_extras(&node.extras),
        tags: node.tags.scene_tags(),
        objects,
    })
}

// diff of the asset nodes and materials against the baseline, including the scene objects below asset nodes
fn extract_asset_overrides(root: &Node, extraction: &mut Extraction) -> (Vec<AssetOverride>, Vec<MaterialOverride>)
{
    let Some(asset) = &root.asset else { return (vec![], vec![]); };

    let mut overrides: Vec<AssetOverride> = asset.orphans.iter().filter_map(|value| serde_json::from_value(value.clone()).ok()).collect();
    let mut material_overrides = vec![];

    let Some(baseline) = &asset.baseline else { return (overrides, material_overrides); };

    for (key, entry) in &baseline.nodes
    {
        let empty = AssetOverride { key: key.clone(), name: entry.name.clone(), removed: false, patch: None, parent: None, extras: Default::default(), tags: vec![], components: vec![], instances: vec![], objects: vec![] };

        // root: only its asset components are compared, the node fields are in the scene object
        if key.is_empty()
        {
            if let Some(patch) = merge_diff(&entry.state, &node_state_value(root, false))
            {
                overrides.push(AssetOverride { patch: Some(patch), ..empty });
            }
            continue;
        }

        let Some(node_item) = extraction.nodes_by_id.get(&entry.node_id).cloned() else
        {
            overrides.push(AssetOverride { removed: true, ..empty });
            continue;
        };

        let node = node_item.read().unwrap();

        let parent_id = node.parent.as_ref().map(|parent| parent.read().unwrap().id);
        let parent = (parent_id != entry.parent_id).then(|| node.parent.as_ref().map(|parent| parent.read().unwrap().uuid.clone()).unwrap_or_default());

        let asset_override = AssetOverride
        {
            patch: merge_diff(&entry.state, &node_state_value(&node, true)),
            parent,
            extras: scene_extras(&node.extras),
            tags: node.tags.scene_tags(),
            components: scene_component_values(&node.components),
            instances: scene_instances(&node),
            objects: node.nodes.iter().filter_map(|child| extract_node(child, extraction)).collect(),
            ..empty
        };

        let changed = asset_override.patch.is_some() || asset_override.parent.is_some() || !asset_override.extras.is_empty() || !asset_override.tags.is_empty()
            || !asset_override.components.is_empty() || !asset_override.instances.is_empty() || !asset_override.objects.is_empty();

        if changed
        {
            overrides.push(asset_override);
        }
    }

    for material in &asset.materials
    {
        let (key, name) = { let material = material.read().unwrap(); (material.get_base().asset_key.clone(), material.get_base().name.clone()) };
        let Some(base) = baseline.materials.get(&key) else { continue; };

        if let Some(patch) = component_value(material).and_then(|value| merge_diff(base, &value))
        {
            component_downcast!(material, Material);
            extraction.textures.extend(material.get_all_textures());
            material_overrides.push(MaterialOverride { key, name, patch });
        }
    }

    (overrides, material_overrides)
}

fn extract_transform(node: &Node) -> ([f32; 3], [f32; 3], Option<[f32; 4]>, [f32; 3])
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

// ******************** run (Play / Simulate) ********************

// edit state of all scenes, taken when a run starts and loaded again on stop
pub struct PlaySnapshot
{
    pub path: String, // base for relative sources
    pub scenes: Vec<SceneLoad>,
    settings: ProjectSettings,
    observers: Vec<(u32, Vec<ObserverId>)>, // scene id, last observer ids
    resources: HashSet<String>, // keys of the textures, mesh resources and sound sources
}

fn resource_keys(state: &State) -> HashSet<String>
{
    state.resources.textures.keys().chain(state.resources.mesh_resources.keys()).chain(state.resources.sound_sources.keys()).cloned().collect()
}

fn is_internal_scene(scene: &Scene) -> bool
{
    scene.has_tag(ENGINE_INTERNAL_TAG) || scene.has_tag(EDITOR_INTERNAL_TAG)
}

pub fn take_play_snapshot(editor_state: &EditorState, state: &mut State) -> PlaySnapshot
{
    // project file as base for relative paths - working directory for an unsaved project
    let path = editor_state.project_path.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("unsaved.project").to_string_lossy().to_string());
    let sounds = extract_sounds(&state.list_sound_sources(), &path);
    let textures = scene_textures(state);

    let scenes = state.scenes.iter()
        .filter(|scene| !is_internal_scene(scene))
        .map(|scene|
        {
            let file = extract_editor_scene(scene, sounds.clone(), &textures, &path);
            SceneLoad { file, path: path.clone(), source: scene.source.as_ref().map(|source| source.origin_path.clone()), active: scene.active, scene_id: Some(scene.id) }
        })
        .collect();

    let observers = state.scenes.iter_mut().map(|scene| (scene.id, scene.last_observer_ids())).collect();

    PlaySnapshot { path, scenes, settings: extract_project_settings(state), observers, resources: resource_keys(state) }
}

pub fn restore_play_snapshot(editor_state: &mut EditorState, state: &mut State, snapshot: PlaySnapshot)
{
    let PlaySnapshot { scenes, settings, observers, resources, .. } = snapshot;

    // deletes scenes added during the run - deleted ones are created again by the reload
    let scene_ids: HashSet<u32> = scenes.iter().filter_map(|load| load.scene_id).collect();
    let added: Vec<u32> = state.scenes.iter().filter(|scene| !is_internal_scene(scene) && !scene_ids.contains(&scene.id)).map(|scene| scene.id).collect();
    for id in added
    {
        state.delete_scene_by_id(id, true);
    }

    // removes scene observers added during the run
    for (scene_id, last_ids) in observers
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            scene.remove_observers_added_after(&last_ids);
        }
    }

    // applies the settings only if they changed - applying rebuilds the rendering
    if serde_json::to_value(extract_project_settings(state)).ok() != serde_json::to_value(&settings).ok()
    {
        apply_project_settings(state, &settings);
    }

    state.oneshot_sounds.clear();
    editor_state.de_select_current_item(state);

    // deletes resources loaded during the run that are not used anymore
    let done: ProjectDoneCallback = Some(Box::new(move |state: &mut State|
    {
        let unused = |key: &String, count: usize| !resources.contains(key) && count == 1;

        for (key, texture) in &state.resources.textures { if unused(key, Arc::strong_count(texture)) { texture.write().unwrap().delete_later(); } }
        for (key, mesh_resource) in &state.resources.mesh_resources { if unused(key, Arc::strong_count(mesh_resource)) { mesh_resource.write().unwrap().delete_later(); } }
        for (key, sound_source) in &state.resources.sound_sources { if unused(key, Arc::strong_count(sound_source)) { sound_source.write().unwrap().delete_later(); } }
    }));

    reload_editor_scenes(state, scenes, editor_state.loading.clone(), editor_state.loading_progress.clone(), done);
}

// ******************** save ********************

pub fn save_editor_project(state: &mut State, editor_state: &mut EditorState, path: &str) -> bool
{
    // the snapshot paths are relative to the project path of the run
    if editor_state.play_snapshot.as_ref().is_some_and(|snapshot| snapshot.path != format!("{}.project", path))
    {
        editor_state.alert("Save", "Stop the run first - a new project location can only be saved in edit mode.", LogType::Warning);
        return false;
    }

    state.project.build += 1;
    editor_state.accumulate_editing_time(state);

    let project_name = state.project.name.clone();
    let base_dir = get_dirname(path);

    let mut project_scenes: Vec<ProjectSceneRef> = Vec::new();
    let mut total_objects = 0;
    let mut used_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    // the resources are shared, so every scene file lists all of them - a scene can be loaded on its own
    let sounds = extract_sounds(&state.list_sound_sources(), path);
    let textures = scene_textures(state);

    // during a run: the snapshot instead of the running state (scene file, scene file path, active)
    let scenes: Vec<(SceneFile, Option<String>, bool)> = match &editor_state.play_snapshot
    {
        Some(snapshot) => snapshot.scenes.iter().map(|load|
        {
            let editor_cameras = load.scene_id.and_then(|id| state.find_scene_by_id(id)).map(|scene| extract_editor_cameras(scene)).unwrap_or_else(|| load.file.editor_cameras.clone());
            (SceneFile { editor_cameras, ..load.file.clone() }, load.source.clone(), load.active)
        }).collect(),
        None => state.scenes.iter()
            .filter(|scene| !is_internal_scene(scene))
            .map(|scene| (extract_editor_scene(scene, sounds.clone(), &textures, path), scene.source.as_ref().map(|source| source.origin_path.clone()), scene.active))
            .collect(),
    };

    for (editor_scene, source, active) in scenes
    {
        total_objects += editor_scene.objects.len();

        // determine scene file path: reuse source if set, otherwise generate from name
        let scene_full_path = if let Some(source) = source
        {
            source
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
        project_scenes.push(ProjectSceneRef { path: relative, active });
    }

    let project = ProjectFile
    {
        project: state.project.clone(),
        format: ProjectFileFormat::default(),
        settings: editor_state.play_snapshot.as_ref().map(|snapshot| snapshot.settings.clone()).unwrap_or_else(|| extract_project_settings(state)),
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

pub fn save_editor_project_with_dialog(editor_state: &mut EditorState, state: &mut State, force_new_path: bool) -> Option<String>
{
    let mut path = editor_state.project_path.clone();
    if editor_state.project_path.is_none() || force_new_path
    {
        path = rfd::FileDialog::new()
            .add_filter("Rustl Project", &["project"])
            .set_file_name(&format!("{}.project", state.project.name))
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
            state.project.name = stem;
        }

        if save_editor_project(state, editor_state, &base)
        {
            editor_state.project_path = Some(format!("{}.project", base));
        }
        else
        {
            return None;
        }
    }

    path
}

// every new project gets a new folder with code: <location>/<name>/<name>.project + code/ (the New Project dialog)
pub fn create_editor_project(editor_state: &mut EditorState, state: &mut State, location: &std::path::Path, name: &str)
{
    let project_dir = location.join(name);
    if project_dir.exists()
    {
        editor_state.alert("New Project", &format!("{} exists already - a new project needs a new folder", project_dir.display()), LogType::Error);
        return;
    }

    if let Err(err) = std::fs::create_dir_all(&project_dir)
    {
        editor_state.alert("New Project", &format!("Can not create {}:\n{}", project_dir.display(), err), LogType::Error);
        return;
    }

    let name = name.to_string();

    editor_state.reset_project(state);
    state.delete_all_scenes(true);
    state.add_scene("main scene").add_default_lights_and_cam();
    state.run_mode = RunMode::Edit;
    state.project.name = name.clone();

    if let Err(err) = project_code::create_project_code(&project_dir, &name)
    {
        console_warning!("{}", err);
    }

    let base = project_dir.join(&name).to_string_lossy().to_string();
    if save_editor_project(state, editor_state, &base)
    {
        let path = format!("{}.project", base);
        editor_state.project_path = Some(path.clone());
        editor_state.recent_projects.add_and_save(path);
    }
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
        editor_state.play_snapshot = None;
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

    state.run_mode = RunMode::Edit;
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

#[cfg(test)]
mod tests
{
    use std::sync::{Arc, RwLock};

    use nalgebra::Matrix4;

    use crate::component_downcast_mut;
    use crate::helper::concurrency::execution_queue::ExecutionQueue;
    use crate::helper::concurrency::thread::sleep_millis;
    use crate::output::audio_device::AudioDevice;
    use crate::state::project::loader::{SceneLoad, apply_editor_scene, reload_editor_scenes};
    use crate::state::project::project::{ProjectFile, SceneFile};
    use crate::state::scene::components::mesh::Mesh;
    use crate::state::scene::instance::Instance;
    use crate::state::scene::loader::loader::{LoaderOptions, load_asset};
    use crate::state::scene::node::{Node, NodeItem};
    use crate::state::scene::utilities::origin::Origin;
    use crate::state::scene::utilities::saved_state::{asset_nodes, assign_asset_uuids, capture_asset_baseline};
    use crate::state::scene::utilities::scene_utils::set_node_parent;
    use crate::state::resources::utilities::resource_utils::load_texture_byte;
    use crate::gui::editor::editor_state::EditorState;
    use crate::helper::concurrency::thread::spawn_thread;
    use crate::state::scene::utilities::scene_utils::execute_on_state_mut_and_wait;
    use crate::state::state::{RunMode, State};

    use super::{extract_editor_scene, save_editor_project};

    const ASSET: &str = "resources/objects/monkey/monkey.glb";

    // the run session is global - tests that switch it run one after another
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn new_state() -> State
    {
        State::new(Arc::new(RwLock::new(Box::new(AudioDevice::default()))))
    }

    // loads an object like the editor does on drop
    fn load_object(state: &mut State, scene_id: u32) -> NodeItem
    {
        load_object_from(state, scene_id, &format!("{}/{}", env!("CARGO_MANIFEST_DIR"), ASSET))
    }

    fn load_object_from(state: &mut State, scene_id: u32, path: &str) -> NodeItem
    {
        let extension = crate::helper::file::get_extension(path);
        let options = LoaderOptions { path: path.to_string(), extension, parent_node_id: None, hide_root_nodes: false, reuse_materials: false, clear_unused_textures: true, object_only: true, create_mipmaps: false, max_texture_resolution: 0, texture_cache: None, material_cache: None };

        let mut container = load_asset(&options).unwrap();
        let result = container.apply_to_scene(state, scene_id);

        let root = state.find_scene_by_id(scene_id).unwrap().find_node_by_id(result.root_node_ids[0]).unwrap();
        root.write().unwrap().origin = Origin::Scene;
        assign_asset_uuids(&root);
        capture_asset_baseline(&root);

        root
    }

    // runs the main thread queue until loading is done
    fn run_until_loaded(state: &mut State, loading: &Arc<RwLock<bool>>)
    {
        let queue = state.main_thread_execution_queue.clone();
        let start = std::time::Instant::now();

        while *loading.read().unwrap() || !state.loading_scenes.is_empty()
        {
            ExecutionQueue::run_all(queue.clone(), state);
            sleep_millis(1);
            assert!(start.elapsed().as_secs() < 60, "loading takes too long");
        }

        ExecutionQueue::run_all(queue, state);
    }

    fn objects_json(file: &SceneFile) -> String
    {
        serde_json::to_string_pretty(&file.objects).unwrap()
    }

    fn find_named(root: &NodeItem, name: &str) -> NodeItem
    {
        asset_nodes(&root.read().unwrap()).into_iter().find(|node| node.read().unwrap().name == name).unwrap()
    }

    #[test]
    fn changes_in_loaded_assets_are_saved_loaded_and_restored()
    {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let dir = std::env::temp_dir().join(format!("rustl_saved_state_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project_path = dir.join("test.project").to_string_lossy().to_string();
        let scene_path = dir.join("test_main.scene").to_string_lossy().to_string();

        let mut state = new_state();
        let scene_id = state.add_scene("main").id;

        let avatar = load_object(&mut state, scene_id);
        let weapon = load_object(&mut state, scene_id);

        // nested asset nodes P > C
        let rig_path = dir.join("rig.gltf").to_string_lossy().to_string();
        std::fs::write(&rig_path, r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"P","children":[1]},{"name":"C"}]}"#).unwrap();
        let rig = load_object_from(&mut state, scene_id, &rig_path);

        // a texture added in the editor and not used anywhere
        let texture_path = format!("{}/resources/textures/test.png", env!("CARGO_MANIFEST_DIR"));
        let texture = load_texture_byte(0, false, &std::fs::read(&texture_path).unwrap(), "test", &texture_path, Some("png".to_string()));
        texture.write().unwrap().origin = Origin::Scene;
        let texture = state.insert_texture_or_reuse(texture, "");
        let texture_uuid = texture.read().unwrap().uuid.clone();

        {
            let scene = state.find_scene_by_id_mut(scene_id).unwrap();

            // asset node changed: settings, transform and an asset component
            let mesh_node = find_named(&avatar, "Suzanne");
            mesh_node.write().unwrap().settings.visible = false;
            Node::set_local_transform(mesh_node.clone(), Matrix4::new_translation(&nalgebra::Vector3::new(1.0, 2.0, 3.0)));

            let mesh = mesh_node.read().unwrap().find_component::<Mesh>().unwrap();
            {
                component_downcast_mut!(mesh, Mesh);
                mesh.update_skin_bbox_on_animation = true;
            }

            // an instance added in the editor
            let mut instance = Instance::new("added".to_string(), mesh_node.clone());
            instance.origin = Origin::Scene;
            mesh_node.write().unwrap().add_instance(Box::new(instance));

            // a folder on an asset node with the other asset in it
            let folder = scene.add_empty_node("folder", Some(find_named(&avatar, "Plane")));
            folder.write().unwrap().origin = Origin::Scene;
            set_node_parent(scene, weapon.clone(), Some(folder.clone()), false);

            // an asset node moved to the top level, then its old parent deleted
            set_node_parent(scene, find_named(&rig, "C"), None, false);
            let p = find_named(&rig, "P").read().unwrap().id;
            scene.delete_node_by_id(p, false, false, false, false);

            // an asset node of the other asset deleted
            let deleted = find_named(&weapon, "Suzanne").read().unwrap().id;
            scene.delete_node_by_id(deleted, false, false, false, false);
        }

        let saved = extract_editor_scene(state.find_scene_by_id(scene_id).unwrap(), vec![], &[texture], &project_path);
        assert_eq!(saved.textures.len(), 1);
        let saved_json = objects_json(&saved);
        println!("{}", saved_json);

        assert!(saved_json.contains("\"removed\": true"));
        assert!(saved_json.contains("\"update_skin_bbox_on_animation\": true"));
        assert!(saved_json.contains("\"parent\": \"\""));
        assert!(saved_json.contains("\"folder\""));
        assert!(saved_json.contains("\"added\""));

        std::fs::write(&scene_path, serde_json::to_string_pretty(&saved).unwrap()).unwrap();

        // load and extract again: same result
        let mut loaded = new_state();
        let loading = Arc::new(RwLock::new(false));
        let loaded_id = apply_editor_scene(&mut loaded, &scene_path, loading.clone(), Arc::new(RwLock::new(0.0))).unwrap();
        run_until_loaded(&mut loaded, &loading);

        let reloaded = extract_editor_scene(loaded.find_scene_by_id(loaded_id).unwrap(), vec![], &[], &project_path);
        assert_eq!(saved_json, objects_json(&reloaded));
        assert!(loaded.resources.textures.values().any(|texture| { let texture = texture.read().unwrap(); texture.uuid == texture_uuid && texture.origin == Origin::Scene }));
        assert!(loaded.find_scene_by_id(loaded_id).unwrap().nodes.iter().any(|node| node.read().unwrap().name == "C"));

        // changes like a run, then reload into the same scene
        {
            let scene = loaded.find_scene_by_id_mut(loaded_id).unwrap();
            let first = scene.nodes[0].read().unwrap().id;
            scene.delete_node_by_id(first, false, false, false, false);
            scene.add_empty_node("spawned by the game", None);
        }

        let load = SceneLoad { file: saved.clone(), path: project_path.clone(), source: None, active: true, scene_id: Some(loaded_id) };
        reload_editor_scenes(&mut loaded, vec![load], loading.clone(), Arc::new(RwLock::new(0.0)), None);
        run_until_loaded(&mut loaded, &loading);

        let scene = loaded.find_scene_by_id(loaded_id).unwrap();
        assert!(scene.find_node_by_name("spawned by the game").is_none());
        assert_eq!(saved_json, objects_json(&extract_editor_scene(scene, vec![], &[], &project_path)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_restores_the_edit_state()
    {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let mut state = new_state();
        state.set_run_mode(RunMode::Edit);

        let scene_id = state.add_scene("main").id;
        let avatar = load_object(&mut state, scene_id);
        find_named(&avatar, "Suzanne").write().unwrap().settings.visible = false;

        let dir = std::env::temp_dir().join(format!("rustl_run_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("test").to_string_lossy().to_string();
        let path = format!("{}.project", base);

        let mut editor_state = EditorState::new();
        editor_state.project_path = Some(path.clone());
        let before = objects_json(&extract_editor_scene(state.find_scene_by_id(scene_id).unwrap(), vec![], &[], &path));

        editor_state.set_run_mode(&mut state, RunMode::Play, false);
        assert!(editor_state.play_snapshot.is_some());

        // changes like a game: new scene, deleted and spawned nodes, observer, load that finishes after stop
        let added_scene = state.add_scene("level 2").id;
        {
            let scene = state.find_scene_by_id_mut(scene_id).unwrap();
            scene.on_before_update.add(|_| {});
            find_named(&avatar, "Suzanne").write().unwrap().settings.visible = true;
            let plane = find_named(&avatar, "Plane").read().unwrap().id;
            scene.delete_node_by_id(plane, false, false, false, false);
            scene.add_empty_node("spawned", None);
        }

        // saving during the run writes the edit state
        assert!(save_editor_project(&mut state, &mut editor_state, &base));
        let project: ProjectFile = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(project.scenes.len(), 1);
        let saved: SceneFile = serde_json::from_str(&std::fs::read_to_string(dir.join(&project.scenes[0].path)).unwrap()).unwrap();
        assert_eq!(before, objects_json(&saved));

        let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let late_load =
        {
            let gate = gate.clone();
            let queue = state.main_thread_execution_queue.clone();
            spawn_thread(move ||
            {
                while !gate.load(std::sync::atomic::Ordering::SeqCst) { sleep_millis(1); }
                execute_on_state_mut_and_wait(queue, Box::new(move |state: &mut State| { state.find_scene_by_id_mut(scene_id).unwrap().add_empty_node("late", None); }));
            })
        };

        editor_state.set_run_mode(&mut state, RunMode::Edit, false);
        let loading = editor_state.loading.clone();
        run_until_loaded(&mut state, &loading);

        gate.store(true, std::sync::atomic::Ordering::SeqCst);
        while !late_load.is_finished()
        {
            ExecutionQueue::run_all(state.main_thread_execution_queue.clone(), &mut state);
            sleep_millis(1);
        }

        assert!(editor_state.play_snapshot.is_none());
        assert!(state.find_scene_by_id(added_scene).is_none());

        let scene = state.find_scene_by_id(scene_id).unwrap();
        assert!(!scene.on_before_update.has_observers());
        assert!(scene.find_node_by_name("spawned").is_none());
        assert!(scene.find_node_by_name("late").is_none());
        assert_eq!(before, objects_json(&extract_editor_scene(scene, vec![], &[], &path)));

        let _ = std::fs::remove_dir_all(&dir);
    }

}

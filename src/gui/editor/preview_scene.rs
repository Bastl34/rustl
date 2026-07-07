#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Point3, Vector3};

use crate::component_downcast_mut;
use crate::{console_error, console_log, console_success, console_warning};
use crate::gui::editor::editor_state::EditorState;
use crate::helper::concurrency::thread::spawn_thread;
use crate::rendering::scene::render_scene_offscreen_to_image;
use crate::rendering::wgpu::WGpu;
use crate::resources::resources::{get_path, load_binary};
use crate::state::scene::camera::Camera;
use crate::state::scene::components::component::Component;
use crate::state::scene::components::material::{Material, MaterialData, MaterialItem};
use crate::state::scene::loader::loader::load_asset_and_add_to_scene;
use crate::state::scene::scene::Scene;
use crate::state::scene::utilities::scene_utils::{align_camera_to_scene, execute_on_scene_mut};
use crate::state::scene::utilities::tags;
use crate::state::state::{State, ENGINE_INTERNAL_TAG};

pub const PREVIEW_SCENE_NAME: &str = "preview scene";
pub const PREVIEW_SCENE_TAG: &str = "__internal_preview_scene";
pub const PREVIEW_SPHERE_NODE_NAME: &str = "preview sphere";
pub const PREVIEW_MATERIAL_NAME: &str = "preview material";

const PREVIEW_ENV_MAP: &str = "textures/environment/footprint_court.jpg";
const MATERIAL_PREVIEW_SPHERE_ASSET: &str = "objects/sphere/sphere.gltf";

pub fn get_preview_scene_id(state: &State) -> Option<u32>
{
    state.scenes.iter().find(|scene| scene.has_tag(PREVIEW_SCENE_TAG)).map(|scene| scene.id)
}

pub fn ensure_preview_scene(state: &mut State)
{
    if get_preview_scene_id(state).is_some()
    {
        return;
    }

    create_preview_scene(state);
}

fn create_preview_scene(state: &mut State)
{
    let scene_id =
    {
        let scene = state.add_scene(PREVIEW_SCENE_NAME);
        scene.active = false;
        scene.visible = false;

        scene.tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
        scene.tags.insert_with_color_locked(PREVIEW_SCENE_TAG, tags::DEFAULT_RED_COLOR, true);

        // key light
        let key = scene.add_light_directional("preview key light", Point3::<f32>::new(4.0, 6.0, 6.0), Vector3::<f32>::new(-0.5, -0.7, -0.6), Vector3::<f32>::new(1.0, 1.0, 1.0), 2.0);
        key.borrow_mut().get_mut().tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

        // soft ambient fill
        let hemi = scene.add_light_hemispherical("preview ambient", Vector3::<f32>::new(0.0, -1.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0), Vector3::<f32>::new(0.2, 0.2, 0.2), 0.6);
        hemi.borrow_mut().get_mut().tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

        // camera (framing is refined via align_camera_to_scene once the sphere is loaded)
        let mut cam = Camera::new("preview cam".to_string());
        cam.tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

        let cam_data = cam.get_data_mut().get_mut();
        cam_data.fovy = 45.0f32.to_radians();
        cam_data.eye_pos = Point3::<f32>::new(0.0, 0.0, 3.0);
        cam_data.dir = Vector3::<f32>::new(0.0, 0.0, -1.0);
        cam_data.up = Vector3::<f32>::new(0.0, 1.0, 0.0);
        cam_data.clipping_near = 0.01;
        cam_data.clipping_far = 1000.0;

        scene.cameras.push(Box::new(cam));

        scene.id
    };

    console_log!("creating internal preview scene (id {})", scene_id);

    state.load_scene_env_map(PREVIEW_ENV_MAP, scene_id);

    let main_queue = state.main_thread_execution_queue.clone();
    let create_mipmaps = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    // load sphere
    spawn_thread(move ||
    {
        let loaded = load_asset_and_add_to_scene(MATERIAL_PREVIEW_SPHERE_ASSET, scene_id, None, main_queue.clone(), false, false, true, true, create_mipmaps, max_tex_res);
        if let Err(err) = &loaded
        {
            console_error!("preview scene: failed to load sphere '{}': {}", MATERIAL_PREVIEW_SPHERE_ASSET, err);
        }

        // tag the loaded sphere internal and frame it with the preview camera
        execute_on_scene_mut(main_queue.clone(), scene_id, Box::new(move |scene|
        {
            for node in Scene::list_all_child_nodes(&scene.nodes)
            {
                let mut node = node.write().unwrap();
                if !node.tags.contains(ENGINE_INTERNAL_TAG)
                {
                    node.tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
                }
            }

            align_camera_to_scene(scene, 0, None, None, None);
        }));
    });
}

pub fn create_and_assign_preview_material(scene: &mut Scene) -> Option<MaterialItem>
{
    let material = if let Some(material) = scene.get_material_by_name(PREVIEW_MATERIAL_NAME)
    {
        material
    }
    else
    {
        let mut material = Material::new(PREVIEW_MATERIAL_NAME);
        material.get_base_mut().tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

        let material: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
        scene.add_material(&material);
        material
    };

    let material_id = material.read().unwrap().get_base().id;

    for node in Scene::list_all_child_nodes(&scene.nodes)
    {
        if node.read().unwrap().get_meshes_with_mesh_resource().is_empty()
        {
            continue;
        }

        let already_assigned = node.read().unwrap().find_component::<Material>().map_or(false, |m| m.read().unwrap().get_base().id == material_id);
        if !already_assigned
        {
            let mut node = node.write().unwrap();
            node.remove_components_by_type::<Material>();
            node.add_component(material.clone());
        }
    }

    Some(material)
}

pub fn apply_material_data(material: &MaterialItem, mat_path: &str) -> bool
{
    let bytes = match load_binary(mat_path)
    {
        Ok(bytes) => bytes,
        Err(err) =>
        {
            console_error!("preview material: can not load '{}': {}", mat_path, err);
            return false;
        }
    };

    let data = match serde_json::from_slice::<MaterialData>(bytes.as_slice())
    {
        Ok(data) => data,
        Err(err) =>
        {
            console_error!("preview material: can not parse '{}': {}", mat_path, err);
            return false;
        }
    };

    component_downcast_mut!(material, Material);
    material.get_data_mut().set(data);

    true
}

pub fn get_thumbnail_path(material_resolved_path: &str) -> Option<String>
{
    let path = std::path::Path::new(material_resolved_path);
    let stem = path.file_stem()?.to_string_lossy();
    let parent = path.parent()?;

    Some(parent.join(format!("{}_thumb.png", stem)).to_string_lossy().to_string())
}

pub fn generate_material_thumbnails<F>(editor_state: &EditorState, state: &mut State, wgpu: &mut WGpu, size: u32, force_regeneration: bool, on_complete: F) -> bool
    where F: FnOnce() + Send + 'static
{
    let offscreen_render_jobs: Vec<(String, String)> = editor_state.assets_materials.iter().filter_map(|asset|
    {
        let material_resolved = get_path(&asset.path);
        let thumb = get_thumbnail_path(&material_resolved)?;
        if std::path::Path::new(&thumb).exists() && !force_regeneration
        {
            None
        }
        else
        {
            Some((material_resolved, thumb))
        }
    }).collect();

    if offscreen_render_jobs.is_empty()
    {
        console_log!("material thumbnails: nothing to generate (all present)");
        return false;
    }

    console_log!("material thumbnails: generating {} ...", offscreen_render_jobs.len());

    let scene_id = match get_preview_scene_id(state)
    {
        Some(id) => id,
        None => { console_warning!("material thumbnails: no preview scene present"); return false; }
    };

    let scene_index = match state.scenes.iter().position(|scene| scene.id == scene_id)
    {
        Some(index) => index,
        None => return false,
    };
    let mut scene = state.scenes.remove(scene_index);

    let preview_sphere_loaded = Scene::list_all_child_nodes(&scene.nodes).iter().any(|node| !node.read().unwrap().get_meshes_with_mesh_resource().is_empty());
    if !preview_sphere_loaded
    {
        console_warning!("material thumbnails: preview sphere not loaded yet, try again in a moment");
        state.scenes.insert(scene_index, scene);
        return false;
    }

    let material = match create_and_assign_preview_material(&mut scene)
    {
        Some(material) => material,
        None =>
        {
            state.scenes.insert(scene_index, scene);
            return false;
        }
    };

    // render every material into an in-memory image (the gpu work has to stay on the main thread)
    let mut rendered: Vec<(String, image::DynamicImage)> = Vec::with_capacity(offscreen_render_jobs.len());
    for (material_path, thumb_path) in &offscreen_render_jobs
    {
        if !apply_material_data(&material, material_path)
        {
            continue;
        }

        let img = render_scene_offscreen_to_image(wgpu, state, &mut scene, size, size);
        rendered.push((thumb_path.clone(), img));
    }

    state.scenes.insert(scene_index, scene);

    if rendered.is_empty()
    {
        return false;
    }

    // write the pngs on a worker thread, then notify the caller (which clears the running lock + requests the reload)
    spawn_thread(move ||
    {
        let mut saved = 0;
        for (thumb_path, img) in rendered
        {
            match img.save(&thumb_path)
            {
                Ok(_) => { saved += 1; }
                Err(err) => { console_error!("material thumbnails: failed to save '{}': {}", thumb_path, err); }
            }
        }

        console_success!("material thumbnails: {} generated", saved);

        on_complete();
    });

    true
}

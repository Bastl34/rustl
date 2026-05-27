#![allow(dead_code)]

use nalgebra::{Point3, Vector3};

use crate::{console_error, console_log};
use crate::helper::concurrency::thread::spawn_thread;
use crate::state::scene::camera::Camera;
use crate::state::scene::loader::loader::load_asset_and_add_to_scene;
use crate::state::scene::scene::Scene;
use crate::state::scene::utilities::scene_utils::{align_camera_to_scene, execute_on_scene_mut};
use crate::state::scene::utilities::tags;
use crate::state::state::{State, ENGINE_INTERNAL_TAG};

pub const PREVIEW_SCENE_NAME: &str = "preview scene";
pub const PREVIEW_SCENE_TAG: &str = "__internal_preview_scene";
pub const PREVIEW_SPHERE_NODE_NAME: &str = "preview sphere";

const PREVIEW_ENV_MAP: &str = "textures/environment/footprint_court.jpg";
const PREVIEW_SPHERE_ASSET: &str = "objects/sphere/sphere.gltf";

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
        let hemi = scene.add_light_hemisperical("preview ambient", Vector3::<f32>::new(0.0, -1.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0), Vector3::<f32>::new(0.2, 0.2, 0.2), 0.6);
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
        let loaded = load_asset_and_add_to_scene(PREVIEW_SPHERE_ASSET, scene_id, None, main_queue.clone(), true, false, true, true, create_mipmaps, max_tex_res);
        if let Err(err) = &loaded
        {
            console_error!("preview scene: failed to load sphere '{}': {}", PREVIEW_SPHERE_ASSET, err);
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

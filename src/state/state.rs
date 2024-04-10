use std::{cell::RefCell, rc::Rc, sync::{RwLock, Arc}};

use instant::Instant;
use nalgebra::Vector3;

use crate::{helper::{change_tracker::ChangeTracker, concurrency::{execution_queue::{ExecutionQueue, ExecutionQueueItem}, thread::spawn_thread}}, input::input_manager::InputManager};

use super::scene::{camera_controller::camera_controller::CameraControllerBox, components::{component::ComponentItem, material::TextureType}, scene::SceneItem, scene_controller::scene_controller::{SceneControllerBase, SceneControllerBox}, utilities::scene_utils::load_texture};

pub type StateItem = Rc<RefCell<State>>;

pub const FPS_CHART_VALUES: usize = 100;
pub const DEFAULT_MAX_TEXTURE_RESOLUTION: u32 = 16384;
pub const DEFAULT_MAX_SUPPORTED_TEXTURE_RESOLUTION: u32 = 4096;

pub const REFERENCE_UPDATE_FRAMES: f32 = 60.0;

pub struct AdapterFeatures
{
    pub name: String,
    pub driver: String,
    pub driver_info: String,
    pub backend: String,

    pub storage_buffer_array_support: bool,
    pub max_msaa_samples: u32,
    pub max_texture_resolution: u32,
    pub max_supported_texture_resolution: u32
}

pub struct Rendering
{
    pub clear_color: ChangeTracker<Vector3<f32>>,
    pub v_sync: ChangeTracker<bool>,

    pub fullscreen: ChangeTracker<bool>,
    pub msaa: ChangeTracker<u32>,

    pub distance_sorting: bool,
    pub create_mipmaps: bool,
    pub max_texture_resolution: Option<u32>,
}

pub struct SupportedFileTypes
{
    pub objects: Vec<String>,
    pub textures: Vec<String>
}

pub struct Statistics
{
    pub draw_calls: u32,
    pub fps_timer: Instant,
    pub last_time: u128,
    pub fps: u32,
    pub last_fps: u32,
    pub fps_absolute: u32,
    pub fps_chart: Vec<u32>,

    pub frame_update_time: u128,
    pub frame_scale: f32,

    pub frame_time: f32,

    pub engine_update_time: f32,
    pub engine_render_time: f32,

    pub app_update_time: f32,

    pub editor_update_time: f32,

    pub egui_update_time: f32,
    pub egui_render_time: f32,

    pub frame: u64,
}

pub struct State
{
    pub adapter: AdapterFeatures,
    pub rendering: Rendering,
    pub input_manager: InputManager,

    pub main_thread_execution_queue: ExecutionQueueItem,

    pub running: bool,
    pub pause: bool,
    pub scenes: Vec<SceneItem>,

    pub registered_components: Vec<(String, fn(u64, &str) -> ComponentItem)>,
    pub registered_camera_controller: Vec<(String, fn() -> CameraControllerBox)>,
    pub registered_scene_controller: Vec<(String, fn() -> SceneControllerBox)>,

    pub supported_file_types: SupportedFileTypes,

    pub in_focus: bool,

    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,

    pub save_image: bool,
    pub save_depth_pass_image: bool,
    pub save_depth_buffer_image: bool,

    pub save_screenshot: bool,

    pub stats: Statistics,

    pub exit: bool,
}

impl State
{
    pub fn new() -> State
    {
        let mut components: Vec<(String, fn(u64, &str) -> ComponentItem)> = vec![];

        components.push(("Alpha".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::alpha::Alpha::new(id, name, 1.0)))) }));
        components.push(("Material".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::material::Material::new(id, name)))) }));
        //components.push(("Mesh".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::mesh::Mesh::new_plane(id, name, x0, x1, x2, x3)))) }));
        components.push(("Transform".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::transformation::Transformation::identity(id, name)))) }));
        components.push(("Transform Animation".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::transformation_animation::TransformationAnimation::new_empty(id, name)))) }));
        components.push(("Morph Target Animation".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::morph_target_animation::MorphTargetAnimation::new_empty(id, name)))) }));
        components.push(("Animation Blending".to_string(), |id, name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::animation_blending::AnimationBlending::new_empty(id, name)))) }));

        let mut cam_controller: Vec<(String, fn() -> CameraControllerBox)> = vec![];
        cam_controller.push(("Fly Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::fly_controller::FlyController::default()) }));
        cam_controller.push(("Target Rotation Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::target_rotation_controller::TargetRotationController::default()) }));

        let mut scene_controller: Vec<(String, fn() -> SceneControllerBox)> = vec![];
        scene_controller.push(("Character Controller".to_string(), || { Box::new(crate::state::scene::scene_controller::character_controller::CharacterController::default()) }));
        scene_controller.push(("Generic Controller".to_string(), || { Box::new(crate::state::scene::scene_controller::generic_controller::GenericController::default()) }));

        Self
        {
            adapter: AdapterFeatures
            {
                name: String::new(),
                driver: String::new(),
                driver_info: String::new(),
                backend: String::new(),
                storage_buffer_array_support: false,
                max_msaa_samples: 1,
                max_texture_resolution: DEFAULT_MAX_TEXTURE_RESOLUTION,
                max_supported_texture_resolution: DEFAULT_MAX_SUPPORTED_TEXTURE_RESOLUTION
            },

            rendering: Rendering
            {
                clear_color: ChangeTracker::new(Vector3::<f32>::new(0.0, 0.0, 0.0)),
                v_sync: ChangeTracker::new(true),

                fullscreen: ChangeTracker::new(false),
                msaa: ChangeTracker::new(8),

                distance_sorting: true,
                create_mipmaps: false,
                max_texture_resolution: None
            },

            input_manager: InputManager::new(),

            main_thread_execution_queue: Arc::new(RwLock::new(ExecutionQueue::new())),

            running: false,
            pause: false,
            scenes: vec![],

            registered_components: components,
            registered_camera_controller: cam_controller,
            registered_scene_controller: scene_controller,

            supported_file_types: SupportedFileTypes
            {
                objects: vec![String::from("obj"), String::from("gltf"), String::from("glb")],
                textures: vec![String::from("jpg"), String::from("jpeg"), String::from("png")],
            },

            in_focus: true,

            width: 0,
            height: 0,
            scale_factor: 1.0,

            save_image: false,
            save_depth_pass_image: false,
            save_depth_buffer_image: false,
            save_screenshot: false,

            stats: Statistics
            {
                draw_calls: 0,
                fps_timer: Instant::now(),
                last_time: 0,
                fps: 0,
                last_fps: 0,
                fps_absolute: 0,
                fps_chart: vec![0; 100],

                frame_update_time: 0,
                frame_scale: 0.0,

                frame_time: 0.0,

                engine_update_time: 0.0,
                engine_render_time: 0.0,

                app_update_time: 0.0,

                editor_update_time: 0.0,

                egui_update_time: 0.0,
                egui_render_time: 0.0,

                frame: 0,
            },

            exit: false
        }
    }

    pub fn load_scene_env_map(&mut self, path: &str, scene_id: u64)
    {
        let path = path.to_string().clone();

        //load default env texture
        let main_queue = self.main_thread_execution_queue.clone();
        let max_res = self.max_texture_resolution();
        spawn_thread(move ||
        {
            load_texture(path.as_str(), main_queue.clone(), TextureType::Environment, scene_id, None, true, max_res);
        });
    }

    pub fn find_scene_by_id(&self, id: u64) -> Option<&SceneItem>
    {
        for scene in &self.scenes
        {
            if scene.id == id
            {
                return Some(&scene);
            }
        }

        None
    }

    pub fn find_scene_by_id_mut(&mut self, id: u64) -> Option<&mut SceneItem>
    {
        for scene in &mut self.scenes
        {
            if scene.id == id
            {
                return Some(scene);
            }
        }

        None
    }

    pub fn max_texture_resolution(&self) -> u32
    {
        if let Some(max_tex_resolution) = self.rendering.max_texture_resolution
        {
            return max_tex_resolution;
        }

        self.adapter.max_texture_resolution
    }

    pub fn update(&mut self, time: u128, time_delta: f32, frame: u64)
    {
        // update scenes
        for scene in &mut self.scenes
        {
            scene.update(&mut self.input_manager, time, time_delta, frame);
        }
    }

    pub fn print(&self)
    {
        println!("");
        println!("ADAPTER:");
        println!(" - adapter: {}", self.adapter.name);
        println!(" - driver: {}", self.adapter.driver);
        println!(" - driver info: {}", self.adapter.driver_info);
        println!(" - backend: {}", self.adapter.backend);
        println!(" - storage_buffer_array_support: {}", self.adapter.storage_buffer_array_support);
        println!(" - max msaa_samples: {}", self.adapter.max_msaa_samples);

        println!("");

        println!("SCENES:");

        // print scenes
        for scene in &self.scenes
        {
            scene.print();
        }
    }
}
use std::{cell::RefCell, rc::Rc, sync::{RwLock, Arc}};

use instant::Instant;
use nalgebra::Vector3;

use crate::{helper::{change_tracker::ChangeTracker, concurrency::{execution_queue::ExecutionQueue, thread::spawn_thread}}, input::input_manager::InputManager};

use super::scene::{scene::SceneItem, components::{component::ComponentItem, material::{MaterialItem, TextureType}}, utilities::scene_utils::load_texture};

pub type StateItem = Rc<RefCell<State>>;

pub const FPS_CHART_VALUES: usize = 100;

pub struct AdapterFeatures
{
    pub name: String,
    pub driver: String,
    pub driver_info: String,
    pub backend: String,

    pub storage_buffer_array_support: bool,
    pub max_msaa_samples: u32
}

pub struct Rendering
{
    pub clear_color: ChangeTracker<Vector3<f32>>,
    pub v_sync: ChangeTracker<bool>,

    pub fullscreen: ChangeTracker<bool>,
    pub msaa: ChangeTracker<u32>,

    pub distance_sorting: bool,
    pub create_mipmaps: bool,
}

pub struct State
{
    pub adapter: AdapterFeatures,
    pub rendering: Rendering,
    pub input_manager: InputManager,

    pub main_thread_execution_queue: Arc<RwLock<ExecutionQueue>>,

    pub running: bool,
    pub scenes: Vec<SceneItem>,

    pub registered_components: Vec<(String, fn(u64, &str) -> ComponentItem)>,

    pub in_focus: bool,

    pub width: u32,
    pub height: u32,

    pub save_image: bool,
    pub save_depth_pass_image: bool,
    pub save_depth_buffer_image: bool,

    pub save_screenshot: bool,

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

        Self
        {
            adapter: AdapterFeatures
            {
                name: String::new(),
                driver: String::new(),
                driver_info: String::new(),
                backend: String::new(),
                storage_buffer_array_support: false,
                max_msaa_samples: 1
            },

            rendering: Rendering
            {
                clear_color: ChangeTracker::new(Vector3::<f32>::new(0.0, 0.0, 0.0)),
                v_sync: ChangeTracker::new(true),

                fullscreen: ChangeTracker::new(false),
                msaa: ChangeTracker::new(8),

                distance_sorting: true,
                create_mipmaps: false
            },

            input_manager: InputManager::new(),

            main_thread_execution_queue: Arc::new(RwLock::new(ExecutionQueue::new())),

            running: false,
            scenes: vec![],

            registered_components: components,

            in_focus: true,

            width: 0,
            height: 0,

            save_image: false,
            save_depth_pass_image: false,
            save_depth_buffer_image: false,
            save_screenshot: false,

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

            exit: false
        }
    }

    pub fn load_scene_env_map(&mut self, path: &str, scene_id: u64)
    {
        let path = path.to_string().clone();

        //load default env texture
        let main_queue = self.main_thread_execution_queue.clone();
        spawn_thread(move ||
        {
            load_texture(path.as_str(), main_queue.clone(), TextureType::Environment, scene_id, None);
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

    pub fn update(&mut self, time_delta: f32)
    {
        // update scenes
        for scene in &mut self.scenes
        {
            scene.update(&mut self.input_manager, time_delta);
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
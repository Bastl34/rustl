use std::{cell::RefCell, rc::Rc};

use instant::Instant;
use nalgebra::{Vector3};

use crate::helper::{consumable::Consumable, change_tracker::ChangeTracker};

use super::scene::scene::SceneItem;

pub type StateItem = Rc<RefCell<State>>;

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
    pub clear_color: Vector3<f32>,
    pub v_sync: ChangeTracker<bool>,

    pub fullscreen: ChangeTracker<bool>,
    pub msaa: ChangeTracker<u32>,
}

pub struct State
{
    pub adapter: AdapterFeatures,
    pub rendering: Rendering,

    pub running: bool,
    pub scenes: Vec<SceneItem>,



    /*
    pub cam_fov: f32,
    pub camera_pos: Point3<f32>,
    */



    pub instances: u32,
    pub rotation_speed: f32,

    /*
    pub light1_pos: Point3<f32>,
    pub light1_color: Vector3<f32>,

    pub light2_pos: Point3<f32>,
    pub light2_color: Vector3<f32>,
    */

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

    pub frame_update_time: u128,
    pub frame_scale: f32,

    pub frame_time: f32,
    pub update_time: f32,
    pub render_time: f32,
}

impl State
{
    pub fn new() -> State
    {
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
                clear_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
                v_sync: ChangeTracker::new(true),

                fullscreen: ChangeTracker::new(false),
                msaa: ChangeTracker::new(8),
            },

            running: false,
            scenes: vec![],

            /*
            cam_fov: 45.0,
            camera_pos: Point3::<f32>::new(0.0, 0.0, 0.0),
            */

            instances: 3,
            rotation_speed: 0.01,

            /*
            light1_color: Vector3::<f32>::new(1.0, 1.0, 1.0),
            light1_pos: Point3::<f32>::new(0.0, 0.0, 0.0),

            light2_color: Vector3::<f32>::new(1.0, 1.0, 1.0),
            light2_pos: Point3::<f32>::new(0.0, 0.0, 0.0),
            */

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

            frame_update_time: 0,
            frame_scale: 0.0,

            frame_time: 0.0,
            update_time: 0.0,
            render_time: 0.0,
        }
    }

    pub fn update(&mut self, time_delta: f32)
    {
        // update scenes
        for scene in &mut self.scenes
        {
            scene.update(time_delta);
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
        // update scnes
        for scene in &self.scenes
        {
            scene.print();
        }
    }
}
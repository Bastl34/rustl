use std::{cell::RefCell, rc::Rc};

use instant::Instant;
use nalgebra::{Vector3, Point3};

use super::scene::scene::SceneItem;

pub type StateItem = Rc<RefCell<State>>;

pub struct State
{
    pub running: bool,
    pub scenes: Vec<SceneItem>,

    pub clear_color: Vector3<f32>,
    pub light_color: Vector3<f32>,

    pub cam_fov: f32,

    pub fullscreen: bool,
    pub msaa: u8,
    pub msaa_max: u8,
    pub msaa_changed: bool,

    pub instances: u32,
    pub rotation_speed: f32,

    pub camera_pos: Point3<f32>,
    pub light_pos: Point3<f32>,

    pub save_image: bool,
    pub save_depth_pass_image: bool,
    pub save_depth_buffer_image: bool,

    pub save_screenshot: bool,

    pub draw_calls: u32,
    pub fps_timer: Instant,
    pub last_time: u128,
    pub fps: u32,
    pub last_fps: u32,

    pub frame_update_time: u128,
    pub frame_scale: f32
}

impl State
{
    pub fn new() -> State
    {
        Self
        {
            running: false,
            scenes: vec![],

            clear_color: Vector3::<f32>::new(0.0, 0.0, 0.0),
            light_color: Vector3::<f32>::new(1.0, 1.0, 1.0),

            fullscreen: false,
            msaa: 1,
            msaa_max: 1,
            msaa_changed: false,

            cam_fov: 45.0,

            instances: 3,
            rotation_speed: 0.01,

            camera_pos: Point3::<f32>::new(0.0, 0.0, 0.0),
            light_pos: Point3::<f32>::new(0.0, 0.0, 0.0),

            save_image: false,
            save_depth_pass_image: false,
            save_depth_buffer_image: false,
            save_screenshot: false,

            draw_calls: 0,
            fps_timer: Instant::now(),
            last_time: 0,
            fps: 0,
            last_fps: 0,

            frame_update_time: 0,
            frame_scale: 0.0
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
        println!("SCENES:");
        // update scnes
        for scene in &self.scenes
        {
            scene.print();
        }
    }
}
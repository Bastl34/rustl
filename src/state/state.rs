use std::{cell::RefCell, rc::Rc};

use instant::Instant;

use super::scene::scene::SceneItem;

pub type StateItem = Rc<RefCell<State>>;

pub struct State
{
    pub running: bool,
    pub scenes: Vec<SceneItem>,

    pub clear_color_r: f64,
    pub clear_color_g: f64,
    pub clear_color_b: f64,

    pub cam_fov: f32,

    pub fullscreen: bool,

    pub instances: u32,
    pub rotation_speed: f32,

    pub save_image: bool,
    pub save_depth_pass_image: bool,
    pub save_depth_buffer_image: bool,

    pub save_screenshot: bool,

    pub fps_timer: Instant,
    pub last_time: u128,
    pub fps: u32,
    pub last_fps: u32,
}

impl State
{
    pub fn new() -> State
    {
        Self
        {
            running: false,
            scenes: vec![],

            clear_color_r: 0.0,
            clear_color_g: 0.0,
            clear_color_b: 0.0,
            fullscreen: false,

            cam_fov: 45.0,

            instances: 3,
            rotation_speed: 0.01,

            save_image: false,
            save_depth_pass_image: false,
            save_depth_buffer_image: false,
            save_screenshot: false,

            fps_timer: Instant::now(),
            last_time: 0,
            fps: 0,
            last_fps: 0
        }
    }

    pub fn update(&mut self, time_delta: f32)
    {
        // update scnes
        for scene in &mut self.scenes
        {
            scene.update(time_delta);
        }
    }
}
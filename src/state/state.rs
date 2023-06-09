use std::{cell::RefCell, rc::Rc};

use instant::Instant;

pub type StateItem = Rc<RefCell<State>>;

pub struct State
{
    pub running: bool,

    pub clear_color_r: f64,
    pub clear_color_g: f64,
    pub clear_color_b: f64,

    pub fullscreen: bool,

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
            clear_color_r: 0.0,
            clear_color_g: 0.0,
            clear_color_b: 0.0,
            fullscreen: false,

            fps_timer: Instant::now(),
            last_time: 0,
            fps: 0,
            last_fps: 0
        }
    }
}
use std::sync::Arc;

use winit::window::Window;

use crate::{rendering::{egui::EGui, wgpu::WGpu}, state::state::StateItem};

pub struct Context
{
    pub state: StateItem,

    pub window_title: String,
    pub window_minimized: bool,

    pub wgpu: WGpu,
    pub window: Arc<Window>,
    pub egui: EGui,
}

impl Context
{
    pub fn get_main_scene_id(&self) -> Option<u32>
    {
        for scene in &self.state.borrow().scenes
        {
            if scene.main
            {
                return Some(scene.id);
            }
        }

        None
    }
}
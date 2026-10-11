use std::sync::Arc;

use winit::window::Window;

use crate::{helper::observable::Observable, rendering::{egui::EGui, wgpu::WGpu}, state::state::StateItem};

pub struct Context
{
    pub state: StateItem,

    pub window_title: String,
    pub window_minimized: bool,

    pub wgpu: WGpu,
    pub window: Arc<Window>,
    pub egui: EGui,

    pub on_before_render: Observable<Context>,
    pub on_after_render: Observable<Context>,
    pub on_resize: Observable<Context>,
    pub on_exit: Observable<Context>,
}

impl Context
{
    pub fn get_main_scene_id(&self) -> Option<u32>
    {
        for scene in &self.state.borrow().scenes
        {
            if scene.active
            {
                return Some(scene.id);
            }
        }

        None
    }
}
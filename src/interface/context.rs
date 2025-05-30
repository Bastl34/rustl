use std::sync::Arc;

use winit::window::Window;

use crate::{rendering::{egui::EGui, wgpu::WGpu}, state::state::StateItem};

pub struct Context
{
    pub state: StateItem,

    pub window_title: String,

    pub wgpu: WGpu,
    pub window: Arc<Window>,
    pub egui: EGui,
}
use std::sync::Arc;

use winit::window::Window;

use crate::{helper::observable::{Observable, ObserverId}, rendering::{egui::EGui, wgpu::WGpu}, state::state::StateItem};

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
    // no `..` on purpose: a new field does not compile until it is decided here whether it is an observable
    fn observables(&mut self) -> Vec<&mut Observable<Context>>
    {
        let Context { state: _, window_title: _, window_minimized: _, wgpu: _, window: _, egui: _, on_before_render, on_after_render, on_resize, on_exit } = self;
        vec![on_before_render, on_after_render, on_resize, on_exit]
    }

    pub fn last_observer_ids(&mut self) -> Vec<ObserverId>
    {
        self.observables().iter().map(|observable| observable.last_id()).collect()
    }

    // removes the observers added after last_observer_ids
    pub fn remove_observers_added_after(&mut self, last_ids: &[ObserverId])
    {
        for (observable, last_id) in self.observables().into_iter().zip(last_ids)
        {
            observable.remove_added_after(*last_id);
        }
    }

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
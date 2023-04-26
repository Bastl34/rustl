use std::cell::RefCell;
use std::rc::Rc;
use std::vec;

use winit::window::{Window, Fullscreen};

use crate::rendering::scene::Scene;
use crate::window::egui::EGui;
use crate::rendering::wgpu::{WGpu, WGpuRenderingItem};
use crate::state::state::{State, StateItem};

pub struct MainInterface
{
    state: StateItem,
    scene: Scene,

    gpu: WGpu,
    window: Window,
    gui: EGui,
}

impl MainInterface
{
    pub async fn new(window: Window, event_loop: &winit::event_loop::EventLoop<()>) -> Self
    {
        let mut gpu = WGpu::new(&window).await;
        let gui = EGui::new(event_loop, gpu.device(), gpu.surface_config(), &window);

        let state = Rc::new(RefCell::new(State::new()));
        let scene = Scene::new(state.clone(), &mut gpu);

        Self
        {
            state,
            scene,

            gpu,
            window,
            gui,
        }
    }

    pub fn window(&self) -> &Window
    {
        &self.window
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>, scale_factor: Option<f64>)
    {
        self.gpu.resize(dimensions);
        self.gui.resize(dimensions, scale_factor);
        self.scene.resize(dimensions, scale_factor);
    }

    pub fn update(&mut self)
    {
        // update states
        {
            let state = &mut *(self.state.borrow_mut());

            let mut fullscreen_new = None;
            if state.fullscreen
            {
                fullscreen_new = Some(Fullscreen::Borderless(None));
            }

            self.window.set_fullscreen(fullscreen_new);

            // fps
            let current_time = state.fps_timer.elapsed().as_millis();

            if current_time / 1000 > state.last_time / 1000
            {
                state.last_time = state.fps_timer.elapsed().as_millis();

                state.last_fps = state.fps;
                state.fps = 0;
            }
            else
            {
                state.fps += 1;
            }
        }

        // build ui
        {
            let state = &mut *(self.state.borrow_mut());
            self.gui.build(state, &self.window);
            //self.gui.request_repaint();
        }

        let mut render_passes: Vec<&mut WGpuRenderingItem> = vec![];
        render_passes.push(&mut self.scene);
        render_passes.push(&mut self.gui);

        self.gpu.render(&mut render_passes);

    }

    pub fn input(&mut self, event: &winit::event::WindowEvent)
    {
        if self.gui.on_event(event)
        {
            return;
        }
    }
}

pub fn init(window: Window, event_loop: &winit::event_loop::EventLoop<()>) -> MainInterface
{
    pollster::block_on(MainInterface::new(window, event_loop))
}

use std::cell::RefCell;
use std::rc::Rc;
use std::vec;

use winit::window::{Window, Fullscreen};

use crate::rendering::egui::EGui;
use crate::rendering::scene::Scene;

use crate::rendering::wgpu::{WGpu, WGpuRenderingItem};
use crate::state::gui::gui::build_gui;
use crate::state::state::{State, StateItem};

pub struct MainInterface
{
    state: StateItem,
    scene: Scene,

    wgpu: WGpu,
    window: Window,
    egui: EGui,
}

impl MainInterface
{
    pub async fn new(window: Window, event_loop: &winit::event_loop::EventLoop<()>) -> Self
    {
        let mut wgpu: WGpu = WGpu::new(&window).await;
        let egui = EGui::new(event_loop, wgpu.device(), wgpu.surface_config(), &window);

        let state = Rc::new(RefCell::new(State::new()));
        let scene = Scene::new(&mut wgpu);

        Self
        {
            state,
            scene,

            wgpu,
            window,
            egui,
        }
    }

    pub fn window(&self) -> &Window
    {
        &self.window
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>, scale_factor: Option<f64>)
    {
        self.wgpu.resize(dimensions);
        self.egui.resize(dimensions, scale_factor);
        self.scene.resize(&mut self.wgpu);
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

        // update scene
        {
            let state = &mut *(self.state.borrow_mut());
            self.scene.update(&mut self.wgpu, state);
        }

        // build ui
        {
            let state = &mut *(self.state.borrow_mut());

            let gui_output = build_gui(state, &self.window, &mut self.egui);
            self.egui.output = Some(gui_output);

            //self.gui.request_repaint();
        }

        let mut render_passes: Vec<&mut WGpuRenderingItem> = vec![];
        render_passes.push(&mut self.scene);
        render_passes.push(&mut self.egui);

        self.wgpu.render(&mut render_passes);

        // screenshot
        {
            let state = &mut *(self.state.borrow_mut());

            if state.save_screenshot
            {
                let img_data = self.wgpu.get_screenshot(&mut render_passes);
                img_data.save("data/screenshot.png");
                state.save_screenshot = false;
            }
        }

    }

    pub fn input(&mut self, event: &winit::event::WindowEvent)
    {
        if self.egui.on_event(event)
        {
            return;
        }
    }
}

pub fn init(window: Window, event_loop: &winit::event_loop::EventLoop<()>) -> MainInterface
{
    pollster::block_on(MainInterface::new(window, event_loop))
}

use std::cell::RefCell;
use std::rc::Rc;
use std::vec;

use nalgebra::{Point3, Vector3};
use winit::window::{Window, Fullscreen};

use crate::rendering::egui::EGui;
use crate::rendering::scene::{Scene, self};

use crate::rendering::wgpu::{WGpu, WGpuRenderingItem};
use crate::state::gui::gui::build_gui;
use crate::state::scene::camera::Camera;
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

        //init scene
        {
            let state = &mut *(state.borrow_mut());

            let mut scene = crate::state::scene::scene::Scene::new(0, "main scene");

            // load model
            scene.load("objects/cube/cube.obj").await.unwrap();


            let mut cam = Camera::new();
            cam.fovy = 45.0f32.to_radians();
            cam.eye_pos = Point3::<f32>::new(0.0, 1.0, 2.0);
            cam.dir = Vector3::<f32>::new(-cam.eye_pos.x, -cam.eye_pos.y, -cam.eye_pos.z);
            cam.clipping_near = 0.1;
            cam.clipping_far = 100.0;

            cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.init_matrices();

            scene.cameras.push(Box::new(cam));

            state.scenes.push(Box::new(scene));
        }

        let scene;
        {
            let state = &mut *(state.borrow_mut());

            let graph_scene = state.scenes.get_mut(0);

            scene = Scene::new(&mut wgpu, graph_scene.unwrap()).await;
        }

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

            state.update(1.0); //todo: delta time
        }

        // build ui
        {
            let state = &mut *(self.state.borrow_mut());

            let gui_output = build_gui(state, &self.window, &mut self.egui);
            self.egui.output = Some(gui_output);

            //self.gui.request_repaint();
        }


        // render
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
                img_data.save("data/screenshot.png").unwrap();
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
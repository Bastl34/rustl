use std::cell::RefCell;
use std::mem::take;
use std::rc::Rc;
use std::time::Instant;
use std::vec;

use nalgebra::{Point3, Vector3};
use winit::window::{Window, Fullscreen};

use crate::rendering::egui::EGui;
use crate::rendering::scene::{Scene, self};

use crate::rendering::wgpu::{WGpu};
use crate::state::gui::gui::build_gui;
use crate::state::helper::render_item::{get_render_item, get_render_item_mut};
use crate::state::scene::camera::Camera;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::light::Light;
use crate::state::scene::node::Node;
use crate::state::scene::scene::SceneItem;
use crate::state::state::{State, StateItem};

const REFERENCE_UPDATE_FRAMES: f32 = 60.0;

pub struct MainInterface
{
    state: StateItem,
    start_time: Instant,

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

            // ********** models **********
            scene.load("objects/bastl/bastl.obj").await.unwrap();
            let n0 = scene.nodes.get(0).unwrap().clone();
            let n1 = scene.nodes.get_mut(1).unwrap().clone();
            n1.write().unwrap().merge_mesh(&n0);

            scene.nodes.remove(0);


            //scene.load("objects/cube/cube.obj").await.unwrap();
            //scene.load("objects/plane/plane.obj").await.unwrap();

            {
                let node_id = 0;
                let node = scene.nodes.get_mut(node_id).unwrap();

                let mut node = node.write().unwrap();
                node.add_component(Box::new(Transformation::identity(scene.id_manager.get_next_component_id())));
                node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, 0.0, -15.0));

                //node.remove_component_by_type::<Transformation>();
            }

            let mut node1 = Node::new(scene.id_manager.get_next_node_id(), "test1");
            let mut node2 = Node::new(scene.id_manager.get_next_node_id(), "test2");

            scene.add_node(node1.clone());
            Node::add_node(node1, node2);

            // ********** cam **********
            state.camera_pos = Point3::<f32>::new(0.0, 4.0, 15.0);

            let cam_id = scene.id_manager.get_next_camera_id();
            let mut cam = Camera::new(cam_id);
            cam.fovy = 45.0f32.to_radians();
            cam.eye_pos = state.camera_pos;
            cam.dir = Vector3::<f32>::new(-cam.eye_pos.x, -cam.eye_pos.y + 5.0, -cam.eye_pos.z);
            cam.clipping_near = 0.1;
            cam.clipping_far = 100.0;

            cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.init_matrices();

            scene.cameras.push(Box::new(cam));

            // ********** light **********
            state.light_pos = Point3::<f32>::new(2.0, 5.0, 2.0);
            let light_id = scene.id_manager.get_next_light_id();
            let light = Light::new_point(light_id, state.light_pos, Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
            scene.lights.push(Box::new(light));

            // ********** scene add **********
            state.scenes.push(Box::new(scene));

            state.print();
        }

        {
            let state = &mut *(state.borrow_mut());
            for scene in state.scenes.iter_mut()
            {
                let render_item = Scene::new(&mut wgpu, scene).await;
                scene.render_item = Some(Box::new(render_item));
            }
        }


        Self
        {
            state,
            start_time: Instant::now(),

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

        {
            let state = &mut *(self.state.borrow_mut());

            for scene in &mut state.scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                render_scene.resize(&mut self.wgpu, scene);

                scene.render_item = render_item;
            }
        }
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

            // frame scale
            let elapsed = self.start_time.elapsed();
            let now = elapsed.as_micros();

            if state.frame_update_time > 0 && now - state.frame_update_time > 0
            {
                state.frame_scale = REFERENCE_UPDATE_FRAMES / (1000000.0 / (now - state.frame_update_time) as f32);
            }

            state.frame_update_time = now;
        }

        // update scene
        {
            let state = &mut *(self.state.borrow_mut());

            let mut scene_id: usize = 0;

            let mut empty_vec: Vec<SceneItem> = vec![];
            let mut scenes = take(&mut empty_vec);

            for scene in &mut scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                render_scene.update(&mut self.wgpu, state, scene_id);

                scene.render_item = render_item;

                scene_id += 1;
            }

            state.scenes = scenes;

            state.update(state.frame_scale);
        }

        // build ui
        {
            let state = &mut *(self.state.borrow_mut());

            let gui_output = build_gui(state, &self.window, &mut self.egui);
            self.egui.output = Some(gui_output);

            //self.gui.request_repaint();
        }

        // render
        let (output, view, mut encoder) = self.wgpu.start_render();
        {
            // render scenes
            let state = &mut *(self.state.borrow_mut());

            for scene in &mut state.scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                render_scene.render(&mut self.wgpu, &view, &mut encoder, scene);

                scene.render_item = render_item;
            }

            // render egui
            self.egui.render(&mut self.wgpu, &view, &mut encoder);
        }
        self.wgpu.end_render(output, encoder);

        // screenshot
        {
            let state = &mut *(self.state.borrow_mut());

            if state.save_screenshot
            {
                let (buffer_dimensions, output_buffer, texture, view, mut encoder) = self.wgpu.start_screenshot_render();
                {
                    for scene in &mut state.scenes
                    {
                        let mut render_item = scene.render_item.take();

                        let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                        render_scene.render(&mut self.wgpu, &view, &mut encoder, scene);

                        scene.render_item = render_item;
                    }

                    self.egui.render(&mut self.wgpu, &view, &mut encoder);
                }
                let img_data = self.wgpu.end_screenshot_render(buffer_dimensions, output_buffer, texture, encoder);

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
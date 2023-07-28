use std::cell::RefCell;
use std::mem::{swap};
use std::rc::Rc;
use std::time::Instant;
use std::{vec, cmp};

use nalgebra::{Point3, Vector3};
use winit::window::{Window, Fullscreen};

use crate::helper::change_tracker::ChangeTracker;
use crate::rendering::egui::EGui;
use crate::rendering::scene::{Scene};

use crate::rendering::wgpu::{WGpu};
use crate::state::gui::gui::build_gui;
use crate::state::helper::render_item::{get_render_item_mut};
use crate::state::scene::camera::Camera;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::instance::Instance;
use crate::state::scene::light::Light;
use crate::state::scene::node::Node;
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
        let state = Rc::new(RefCell::new(State::new()));

        let samlpes;
        let mut wgpu: WGpu;
        {
            let state = &mut *(state.borrow_mut());
            wgpu = WGpu::new(&window, state).await;

            dbg!(state.adapter.max_msaa_samples);
            state.rendering.msaa.set(cmp::min(state.rendering.msaa.get_ref().clone(), state.adapter.max_msaa_samples));
            samlpes = *(state.rendering.msaa.get_ref());

            wgpu.create_msaa_texture(samlpes);
        }

        let egui = EGui::new(event_loop, wgpu.device(), wgpu.surface_config(), &window);


        let mut interface = Self
        {
            state,
            start_time: Instant::now(),

            wgpu,
            window,
            egui,
        };

        interface.app_init().await;
        interface.init().await;

        interface
    }

    pub async fn init(&mut self)
    {
        let state = &mut *(self.state.borrow_mut());
        let samlpes = *(state.rendering.msaa.get_ref());

        // move out scenes from state to prevent using multiple mut borrows
        let mut scenes = vec![];
        swap(&mut state.scenes, &mut scenes);

        for scene in &mut scenes
        {
            let render_item = Scene::new(&mut self.wgpu, state, scene, samlpes).await;
            scene.render_item = Some(Box::new(render_item));
        }

        swap(&mut scenes, &mut state.scenes);
    }

    pub fn window(&self) -> &Window
    {
        &self.window
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>, scale_factor: Option<f64>)
    {
        let mut width = dimensions.width;
        let mut height = dimensions.height;

        if width == 0 { width = 1; }
        if height == 0 { height = 1; }

        self.wgpu.resize(width, height);
        self.egui.resize(width, height, scale_factor);

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

    pub async fn app_init(&mut self)
    {
        //init scene
        {
            let state = &mut *(self.state.borrow_mut());

            let mut scene = crate::state::scene::scene::Scene::new(0, "main scene");

            // ********** models **********
            scene.load("objects/bastl/bastl.obj").await.unwrap();
            let n0 = scene.nodes.get(0).unwrap().clone();
            let n1 = scene.nodes.get_mut(1).unwrap().clone();
            n1.write().unwrap().merge_mesh(&n0);

            scene.nodes.remove(0);


            scene.load("objects/cube/cube.obj").await.unwrap();
            //scene.load("objects/plane/plane.obj").await.unwrap();

            {
                let node_id = 0;
                let node = scene.nodes.get_mut(node_id).unwrap();

                let mut node = node.write().unwrap();
                node.add_component(Box::new(Transformation::identity(scene.id_manager.get_next_component_id())));
                node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, 0.0, -15.0));

                //node.remove_component_by_type::<Transformation>();
            }

            {
                let node_id = 1;
                let node = scene.nodes.get_mut(node_id).unwrap();

                let mut node = node.write().unwrap();
                node.add_component(Box::new(Transformation::identity(scene.id_manager.get_next_component_id())));
                node.find_component_mut::<Transformation>().unwrap().apply_scale(Vector3::<f32>::new(4.0, 4.0, 4.0));
                node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, 15.0, -30.0));

                //node.remove_component_by_type::<Transformation>();
            }

            let node1 = Node::new(scene.id_manager.get_next_node_id(), "test1");
            let node2 = Node::new(scene.id_manager.get_next_node_id(), "test2");

            scene.add_node(node1.clone());
            Node::add_node(node1, node2);

            // ********** cam **********
            for i in 0..4
            {
                let cam_id = scene.id_manager.get_next_camera_id();
                let mut cam = Camera::new(cam_id, format!("cam {}", i).to_string());
                cam.fovy = 45.0f32.to_radians();
                cam.eye_pos = Point3::<f32>::new(0.0, 4.0, 15.0);
                cam.dir = Vector3::<f32>::new(-cam.eye_pos.x, -cam.eye_pos.y + 5.0, -cam.eye_pos.z);
                cam.clipping_near = 0.1;
                cam.clipping_far = 1000.0;

                scene.cameras.push(RefCell::new(ChangeTracker::new(Box::new(cam))));
            }

            scene.cameras[0].borrow_mut().get_mut().init(0.0, 0.0, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[1].borrow_mut().get_mut().init(0.5, 0.0, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[2].borrow_mut().get_mut().init(0.0, 0.5, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[3].borrow_mut().get_mut().init(0.5, 0.5, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);

            // ********** light **********
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, Point3::<f32>::new(-2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }

            // ********** scene add **********
            state.scenes.push(Box::new(scene));

            state.print();
        }
    }

    pub fn app_update(&mut self)
    {
        let scene_id = 0;
        let node_id = 0;

        let state = &mut *(self.state.borrow_mut());

        // get scene
        let scene = state.scenes.get_mut(scene_id.clone());

        if scene.is_none()
        {
            return;
        }
        let scene = scene.unwrap();

        // get node
        let node_arc = scene.nodes.get_mut(node_id);

        if node_arc.is_none()
        {
            return;
        }
        let node_arc = node_arc.unwrap();
        let mut node: std::sync::RwLockWriteGuard<'_, Box<Node>> = node_arc.write().unwrap();

        {
            let instances = &mut node.instances;

            if instances.get_ref().len() != state.instances as usize
            {
                //dbg!("recreate instances");

                instances.get_mut().clear();

                for i in 0..state.instances
                {
                    let x = (i as f32 * 5.0) - ((state.instances - 1) as f32 * 5.0) / 2.0;

                    let instance = Instance::new_with_data
                    (
                        scene.id_manager.get_next_instance_id(),
                        "instance".to_string(),
                        node_arc.clone(),
                        Vector3::<f32>::new(x, 0.0, 0.0),
                        Vector3::<f32>::new(0.0, i as f32, 0.0),
                        Vector3::<f32>::new(1.0, 1.0, 1.0)
                    );

                    node.add_instance(Box::new(instance));
                }
            }
            else
            {
                if state.rotation_speed > 0.0
                {
                    for instance in instances.get_ref()
                    {
                        let mut instance = instance.borrow_mut();
                        let instance = instance.get_mut();

                        let rotation: f32 = state.rotation_speed * state.frame_scale;

                        instance.apply_rotation(Vector3::<f32>::new(0.0, rotation, 0.0));
                    }
                }
            }
        }
    }

    pub fn update(&mut self)
    {
        let frame_time = Instant::now();

        // update states
        {
            let state = &mut *(self.state.borrow_mut());

            // vsync
            let (v_sync, vsync_changed) = state.rendering.v_sync.consume_clone();
            if vsync_changed
            {
                self.wgpu.set_vsync(v_sync);
            }

            // fullscreen
            let (fullscreen, fullscreen_changed) = state.rendering.fullscreen.consume_clone();
            if fullscreen_changed
            {
                let mut fullscreen_mode = None;
                if fullscreen { fullscreen_mode = Some(Fullscreen::Borderless(None)); }
                self.window.set_fullscreen(fullscreen_mode);
            }

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

        // app update
        self.app_update();

        // update scene
        {
            let state = &mut *(self.state.borrow_mut());

            // msaa
            let (msaa_samples, msaa_changed) = state.rendering.msaa.consume_clone();

            if msaa_changed
            {
                self.wgpu.create_msaa_texture(msaa_samples);
            }

            state.update(state.frame_scale);

            // move out scenes from state to prevent using multiple mut borrows
            let mut scenes = vec![];
            swap(&mut state.scenes, &mut scenes);

            for scene in &mut scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());

                if msaa_changed
                {
                    render_scene.msaa_sample_size_update(&mut self.wgpu, scene, msaa_samples);
                }
                render_scene.update(&mut self.wgpu, state, scene);

                scene.render_item = render_item;
            }

            swap(&mut scenes, &mut state.scenes);

            state.update_time = frame_time.elapsed().as_micros() as f32 / 1000.0;
        }

        // build ui
        {
            let state = &mut *(self.state.borrow_mut());

            let gui_output = build_gui(state, &self.window, &mut self.egui);
            self.egui.output = Some(gui_output);

            //self.gui.request_repaint();
        }

        // render
        let (output, view, msaa_view, mut encoder) = self.wgpu.start_render();
        {
            let render_time = Instant::now();

            // render scenes
            let state = &mut *(self.state.borrow_mut());
            state.draw_calls = 0;

            for scene in &mut state.scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                state.draw_calls += render_scene.render(&mut self.wgpu, &view, &msaa_view, &mut encoder, scene);

                scene.render_item = render_item;
            }

            // render egui
            self.egui.render(&mut self.wgpu, &view, &mut encoder);

            state.render_time = render_time.elapsed().as_micros() as f32 / 1000.0;
        }
        self.wgpu.end_render(output, encoder);

        // screenshot
        {
            let state = &mut *(self.state.borrow_mut());

            if state.save_screenshot
            {
                let (buffer_dimensions, output_buffer, texture, view, msaa_view, mut encoder) = self.wgpu.start_screenshot_render();
                {
                    for scene in &mut state.scenes
                    {
                        let mut render_item = scene.render_item.take();

                        let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                        render_scene.render(&mut self.wgpu, &view, &msaa_view, &mut encoder, scene);

                        scene.render_item = render_item;
                    }

                    self.egui.render(&mut self.wgpu, &view, &mut encoder);
                }
                let img_data = self.wgpu.end_screenshot_render(buffer_dimensions, output_buffer, texture, encoder);

                img_data.save("data/screenshot.png").unwrap();
                state.save_screenshot = false;
            }
        }

        {
            let state = &mut *(self.state.borrow_mut());
            state.frame_time = frame_time.elapsed().as_micros() as f32 / 1000.0;

            state.fps_absolute = (1000.0 / (state.render_time + state.update_time)) as u32;
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
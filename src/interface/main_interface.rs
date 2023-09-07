use std::cell::RefCell;
use std::mem::{swap};
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use std::{vec, cmp};

use nalgebra::{Point3, Vector3, Vector2, Rotation3};
use winit::event::ElementState;
use winit::window::{Window, Fullscreen};

use crate::component_downcast_mut;
use crate::helper::change_tracker::ChangeTracker;
use crate::helper::math::yaw_pitch_from_direction;
use crate::input::keyboard::{Modifier, Key};
use crate::interface::winit::winit_map_mouse_button;
use crate::rendering::egui::EGui;
use crate::rendering::scene::{Scene};

use crate::rendering::wgpu::{WGpu};
use crate::state::gui::gui::Gui;
use crate::state::helper::render_item::{get_render_item_mut};
use crate::state::scene::camera::Camera;
use crate::state::scene::components::alpha::Alpha;
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::components::transformation_animation::TransformationAnimation;
use crate::state::scene::instance::Instance;
use crate::state::scene::light::Light;
use crate::state::scene::node::Node;
use crate::state::state::{State, StateItem};

use super::winit::winit_map_key;

const REFERENCE_UPDATE_FRAMES: f32 = 60.0;

pub struct MainInterface
{
    pub state: StateItem,
    start_time: Instant,

    editor_gui: Gui,

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
            state.width = window.inner_size().width;
            state.height = window.inner_size().height;
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

            editor_gui: Gui::new(),

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

            state.width = width;
            state.height = height;

            for scene in &mut state.scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                render_scene.resize(&mut self.wgpu, scene);

                scene.render_item = render_item;
            }

            // reset input states
            state.input_manager.reset();
        }
    }

    pub async fn app_init(&mut self)
    {
        //init scene
        {
            let state = &mut *(self.state.borrow_mut());

            let mut scene = crate::state::scene::scene::Scene::new(0, "main scene");
            scene.add_default_material();

            // ********** cam **********
            /*
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
             */

            // ********** light **********
            /*
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(-2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }
            */


            // helmet
            /*
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 100.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }

            {
                let cam_id = scene.id_manager.get_next_camera_id();
                let mut cam = Camera::new(cam_id, "cam".to_string());
                cam.fovy = 23.0f32.to_radians();
                cam.eye_pos = Point3::<f32>::new(4.2011, 2.7027438, 3.71161);
                cam.dir = Vector3::<f32>::new(-0.6515582, -0.4452714, -0.61417043);
                cam.clipping_near = 0.1;
                cam.clipping_far = 1000.0;

                scene.cameras.push(RefCell::new(ChangeTracker::new(Box::new(cam))));
            }
            */



            // lantern

            /*
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 100.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }

            {
                let cam_id = scene.id_manager.get_next_camera_id();
                let mut cam = Camera::new(cam_id, "cam".to_string());
                cam.fovy = 23.0f32.to_radians();
                cam.eye_pos = Point3::<f32>::new(4.2011, 2.7027438, 3.71161);
                cam.dir = Vector3::<f32>::new(-0.6515582, -0.4452714, -0.61417043);
                cam.up = Vector3::<f32>::new(-0.32401347, 0.8953957, -0.30542085);
                cam.clipping_near = 0.1;
                cam.clipping_far = 1000.0;

                scene.cameras.push(RefCell::new(ChangeTracker::new(Box::new(cam))));
            }
             */

            // corset
            /*
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 200.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }

            {
                let cam_id = scene.id_manager.get_next_camera_id();
                let mut cam = Camera::new(cam_id, "cam".to_string());
                cam.fovy = 23.0f32.to_radians();
                cam.eye_pos = Point3::<f32>::new(4.2011, 2.7027438, 3.71161);
                cam.up = Vector3::<f32>::new(-0.32401347, 0.8953957, -0.30542085);
                cam.dir = Vector3::<f32>::new(-0.6515582, -0.4452714, -0.61417043);
                cam.clipping_near = 0.1;
                cam.clipping_far = 1000.0;

                scene.cameras.push(RefCell::new(ChangeTracker::new(Box::new(cam))));
            }
             */

            // ********** models **********
            /*
            scene.load("objects/bastl/bastl.obj").await.unwrap();
            let n0 = scene.nodes.get(0).unwrap().clone();
            let n1 = scene.nodes.get_mut(1).unwrap().clone();
            n1.write().unwrap().merge_mesh(&n0);

            scene.nodes.remove(0);

            scene.load("objects/cube/cube.obj").await.unwrap();
            scene.load("objects/plane/plane.obj").await.unwrap();

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
            */


            //scene.load("objects/monkey/monkey.gltf").await.unwrap();
            //scene.load("objects/monkey/seperate/monkey.gltf").await.unwrap();
            //scene.load("objects/monkey/monkey.glb").await.unwrap();
            //scene.load("objects/temp/Corset.glb").await.unwrap();
            //scene.load("objects/temp/DamagedHelmet.glb").await.unwrap();
            //scene.load("objects/temp/Workbench.glb").await.unwrap();
            //scene.load("objects/temp/Lantern.glb").await.unwrap();
            scene.load("objects/temp/lotus.glb").await.unwrap();
            //scene.load("objects/temp/Sponza_fixed.glb").await.unwrap();
            //scene.load("objects/temp/scene.glb").await.unwrap();
            //scene.load("objects/temp/Toys_Railway.glb").await.unwrap();
            //scene.load("objects/temp/Toys_Railway_2.glb").await.unwrap();
            //scene.load("objects/temp/test.glb").await.unwrap();

            scene.clear_empty_nodes();

            let root_node = Node::new(scene.id_manager.get_next_node_id(), "root node");
            {
                let mut root_node = root_node.write().unwrap();
                root_node.add_component(Arc::new(RwLock::new(Box::new(Alpha::new(scene.id_manager.get_next_component_id(), "Alpha Test", 1.0)))));
            }

            for node in &scene.nodes
            {
                Node::add_node(root_node.clone(), node.clone());
            }

            scene.clear_nodes();
            scene.add_node(root_node.clone());

            if let Some(suzanne) = scene.find_node_by_name("Suzanne")
            {
                let mut node = suzanne.write().unwrap();
                {
                    let instances = node.instances.get_mut();
                    let instance = instances.get_mut(0).unwrap();

                    let mut instance = instance.borrow_mut();
                    let instance = instance.get_mut();
                    instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(scene.id_manager.get_next_component_id(), "Transform")))));

                    instance.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(scene.id_manager.get_next_component_id(), "Transform Animation", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.01, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
                }
                //node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(scene.id_manager.get_next_component_id(), Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.01, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
            }

            if let Some(train) = scene.find_node_by_name("Train")
            {
                let mut node = train.write().unwrap();
                node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(scene.id_manager.get_next_component_id(), "Left", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, -0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
                node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(scene.id_manager.get_next_component_id(), "Right", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));

                let components_len = node.components.len();
                {
                    let component = node.components.get_mut(components_len - 2).unwrap();
                    component_downcast_mut!(component, TransformationAnimation);
                    component.keyboard_key = Some(Key::Left as usize);
                }

                {
                    let component = node.components.get_mut(components_len - 1).unwrap();
                    component_downcast_mut!(component, TransformationAnimation);
                    component.keyboard_key = Some(Key::Right as usize);
                }
            }

            // add light
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }

            // add camera
            if scene.cameras.len() == 0
            {
                let mut cam = Camera::new(scene.id_manager.get_next_camera_id(), "Cam".to_string());
                cam.fovy = 45.0f32.to_radians();
                cam.eye_pos = Point3::<f32>::new(0.0, 1.0, 1.5);
                cam.dir = Vector3::<f32>::new(-cam.eye_pos.x, -cam.eye_pos.y, -cam.eye_pos.z);
                cam.clipping_near = 0.1;
                cam.clipping_far = 1000.0;
                scene.cameras.push(RefCell::new(ChangeTracker::new(Box::new(cam))));
            }


            // lantern
            /*
            {
                let node_id = 0;
                let node = scene.nodes.get_mut(node_id).unwrap();

                let mut node = node.write().unwrap();
                //node.add_component(Box::new(Transformation::identity(scene.id_manager.get_next_component_id())));
                node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, -1.25, 0.0));
                node.find_component_mut::<Transformation>().unwrap().apply_scale(Vector3::<f32>::new(0.08, 0.08, 0.08));

                //node.remove_component_by_type::<Transformation>();
            }
            */


            /*
            // corset
            {
                let node_id = 0;
                let node = scene.nodes.get_mut(node_id).unwrap();

                let mut node = node.write().unwrap();
                //node.add_component(Box::new(Transformation::identity(scene.id_manager.get_next_component_id())));
                node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.15, -0.7, -0.2));
                node.find_component_mut::<Transformation>().unwrap().apply_scale(Vector3::<f32>::new(25.0, 25.0, 25.0));

                //node.remove_component_by_type::<Transformation>();
            }
            */

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

        if state.input_manager.mouse.is_any_button_holding()
        {
            let mut cam = scene.cameras[0].borrow_mut();
            let cam = cam.get_mut();

            let up: Vector3::<f32> = cam.up;
            let dir: Vector3::<f32> = cam.dir;

            let velocity = state.input_manager.mouse.point.velocity;
            let sensitivity = state.frame_scale * 0.0025;

            let delta_x = -velocity.x * sensitivity;
            let delta_y = velocity.y * sensitivity;

            let (mut yaw, mut pitch) = yaw_pitch_from_direction(dir);

            pitch += delta_y;
            yaw -= delta_x;

            dbg!(pitch);

            let mut dir = Vector3::<f32>::zeros();

            dir.x = yaw.cos() * pitch.cos();
            dir.y = pitch.sin();
            dir.z = yaw.sin() * pitch.cos();
            dir = dir.normalize();


            cam.dir = dir;
            cam.init_matrices();

            /*
            dbg!(pitch);

            let rotation_pitch  = Rotation3::from_euler_angles(0.0, pitch, 0.0);

            //cam.dir = (rotation_pitch * Vector3::<f32>::new(1.0, 0.0, 0.0)).normalize();
            cam.dir = (rotation_pitch * Vector3::<f32>::new(1.0, 0.0, 0.0));

            cam.init_matrices();
            */

            /*
            let yaw = dir.y.atan2(dir.x) + delta_x;
            let pitch = (dir.y / dir.norm()).asin();

            let mut dir = Vector3::<f32>::zeros();

            dir.x = yaw.cos() * pitch.cos();
            dir.y = pitch.sin();
            dir.z = yaw.sin() * pitch.cos();
            dir = dir.normalize();
            */

            /*
            direction.x = cos(glm::radians(yaw)) * cos(glm::radians(pitch));
            direction.y = sin(glm::radians(pitch));
            direction.z = sin(glm::radians(yaw)) * cos(glm::radians(pitch));


            let delta_x  = Rotation3::from_euler_angles(0.0, delta_x, 0.0);
            let delta_y  = Rotation3::from_euler_angles(delta_y, 0.0, 0.0);

            cam.dir.
            */

            //cam.dir = (rotation_yaw * (rotation_pitch * dir)).normalize();
            /*
            cam.dir = delta_y * (delta_x * dir).normalize();
            cam.up = delta_y * (delta_x * up).normalize();
             */
            //cam.dir = (delta_x * cam.dir).normalize();
            //cam.dir = (delta_y * cam.dir).normalize();

            /*
            cam.up = (delta_x * cam.up).normalize();
            cam.up = (delta_y * cam.up).normalize();
            */

            /*
            let dir = Vector3::<f32>::new
            direction.x = cos(glm::radians(yaw)) * cos(glm::radians(pitch));
            direction.y = sin(glm::radians(pitch));
            direction.z = sin(glm::radians(yaw)) * cos(glm::radians(pitch));
            cameraFront = glm::normalize(direction);
            */

            //cam.dir = dir;
            //cam.init_matrices();
        }

        if state.input_manager.keyboard.is_holding_by_keys([Key::W, Key::A, Key::S, Key::D, Key::Space, Key::C].to_vec())
        {
            let mut cam = scene.cameras[0].borrow_mut();
            let cam = cam.get_mut();

            let dir = cam.dir.normalize();
            let up = cam.up.normalize();
            let right = up.cross(&dir);

            let mut vec = Vector3::<f32>::zeros();

            let mut factor = 0.1;
            if state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                factor = 0.2;
            }

            let sensitivity = state.frame_scale * factor;

            if state.input_manager.keyboard.is_holding(Key::W)
            {
                vec += dir * sensitivity;
            }
            if state.input_manager.keyboard.is_holding(Key::S)
            {
                vec -= dir * sensitivity;
            }
            if state.input_manager.keyboard.is_holding(Key::D)
            {
                vec -= right * sensitivity;
            }
            if state.input_manager.keyboard.is_holding(Key::A)
            {
                vec += right * sensitivity;
            }
            if state.input_manager.keyboard.is_holding(Key::Space)
            {
                vec += up * sensitivity;
            }
            if state.input_manager.keyboard.is_holding(Key::C)
            {
                vec -= up * sensitivity;
            }

            cam.eye_pos += vec;
            cam.init_matrices();
        }

        // get node
        /*
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
        */
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

            let gui_output = self.editor_gui.build_gui(state, &self.window, &mut self.egui);
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

            // frame update
            state.frame += 1;
        }

        // update inputs
        {
            let state = &mut *(self.state.borrow_mut());
            state.input_manager.update();
        }

        {
            let state = &mut *(self.state.borrow_mut());
            let (visible, changed) = state.input_manager.mouse.visible.consume_borrow();
            if changed
            {
                self.window.set_cursor_visible(*visible);
            }
        }
    }

    pub fn check_exit(&mut self) -> bool
    {
        self.state.borrow().exit
    }

    pub fn input(&mut self, event: &winit::event::WindowEvent)
    {
        if self.egui.on_event(event)
        {
            return;
        }
        else
        {
            let global_state = &mut *(self.state.borrow_mut());

            match event
            {
                winit::event::WindowEvent::KeyboardInput { device_id: _, input, is_synthetic: _ } =>
                {
                    if let Some(key) = input.virtual_keycode
                    {
                        let key = winit_map_key(key);
                        if input.state == ElementState::Pressed
                        {
                            global_state.input_manager.keyboard.set_key(key, true);
                        }
                        else
                        {
                            global_state.input_manager.keyboard.set_key(key, false);
                        }
                    }
                },
                winit::event::WindowEvent::ModifiersChanged(modifiers_state) =>
                {
                    global_state.input_manager.keyboard.set_modifier(Modifier::Alt, modifiers_state.alt());
                    global_state.input_manager.keyboard.set_modifier(Modifier::Ctrl, modifiers_state.ctrl());
                    global_state.input_manager.keyboard.set_modifier(Modifier::Logo, modifiers_state.logo());
                    global_state.input_manager.keyboard.set_modifier(Modifier::Shift, modifiers_state.shift());
                },
                winit::event::WindowEvent::MouseInput { device_id: _, state, button, .. } =>
                {
                    let pressed;
                    match state
                    {
                        ElementState::Pressed => pressed = true,
                        ElementState::Released => pressed = false,
                    }

                    let button = winit_map_mouse_button(button);

                    global_state.input_manager.mouse.set_button(button, pressed);
                },
                winit::event::WindowEvent::MouseWheel { device_id: _, delta, phase: _, ..} =>
                {
                    match delta
                    {
                        winit::event::MouseScrollDelta::LineDelta(x, y) =>
                        {
                            global_state.input_manager.mouse.set_wheel_delta_x(*x);
                            global_state.input_manager.mouse.set_wheel_delta_y(*y);
                        },
                        winit::event::MouseScrollDelta::PixelDelta(delta) =>
                        {
                            global_state.input_manager.mouse.set_wheel_delta_y(delta.x as f32);
                            global_state.input_manager.mouse.set_wheel_delta_y(delta.y as f32);
                        },
                    }

                },
                winit::event::WindowEvent::CursorMoved { device_id: _, position, ..} =>
                {
                    // offset maybe for later usage
                    let offset_x = 0.0;
                    let offset_y = 0.0;

                    let mut pos = Vector2::<f32>::new(position.x as f32, position.y as f32);

                    pos.x = pos.x - offset_x;
                    // invert pos (because x=0, y=0 is bottom left and "normal" window is top left)
                    pos.y = global_state.height as f32 + offset_y - pos.y;

                    global_state.input_manager.mouse.set_pos(pos, global_state.frame);
                },
                winit::event::WindowEvent::Focused(focus) =>
                {
                    global_state.in_focus = *focus;
                    global_state.input_manager.reset();
                },
                _ => {}
                /*
                winit::event::WindowEvent::Resized(_) => todo!(),
                winit::event::WindowEvent::Moved(_) => todo!(),
                winit::event::WindowEvent::CloseRequested => todo!(),
                winit::event::WindowEvent::Destroyed => todo!(),
                winit::event::WindowEvent::DroppedFile(_) => todo!(),
                winit::event::WindowEvent::HoveredFile(_) => todo!(),
                winit::event::WindowEvent::HoveredFileCancelled => todo!(),
                winit::event::WindowEvent::ReceivedCharacter(_) => todo!(),
                winit::event::WindowEvent::Focused(_) => todo!(),
                winit::event::WindowEvent::KeyboardInput { device_id, input, is_synthetic } => todo!(),
                winit::event::WindowEvent::ModifiersChanged(_) => todo!(),
                winit::event::WindowEvent::Ime(_) => todo!(),
                winit::event::WindowEvent::CursorMoved { device_id, position, modifiers } => todo!(),
                winit::event::WindowEvent::CursorEntered { device_id } => todo!(),
                winit::event::WindowEvent::CursorLeft { device_id } => todo!(),
                winit::event::WindowEvent::MouseWheel { device_id, delta, phase, modifiers } => todo!(),
                winit::event::WindowEvent::MouseInput { device_id, state, button, modifiers } => todo!(),
                winit::event::WindowEvent::TouchpadMagnify { device_id, delta, phase } => todo!(),
                winit::event::WindowEvent::SmartMagnify { device_id } => todo!(),
                winit::event::WindowEvent::TouchpadRotate { device_id, delta, phase } => todo!(),
                winit::event::WindowEvent::TouchpadPressure { device_id, pressure, stage } => todo!(),
                winit::event::WindowEvent::AxisMotion { device_id, axis, value } => todo!(),
                winit::event::WindowEvent::Touch(_) => todo!(),
                winit::event::WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => todo!(),
                winit::event::WindowEvent::ThemeChanged(_) => todo!(),
                winit::event::WindowEvent::Occluded(_) => todo!(),
                 */
            }
        }
    }
}
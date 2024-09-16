use std::cell::RefCell;
use std::mem::swap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use std::{vec, cmp};

use gilrs::Gilrs;
use gltf::scene::Transform;
use nalgebra::{Point3, Vector3, Vector2, Point2};
use winit::dpi::PhysicalPosition;
use winit::event::ElementState;
use winit::keyboard::ModifiersKeyState;
use winit::window::{Window, Fullscreen, CursorGrabMode};

use crate::component_downcast_mut;
use crate::helper::change_tracker::ChangeTracker;
use crate::helper::concurrency::execution_queue::ExecutionQueue;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::platform::is_windows;
use crate::input::keyboard::{Modifier, Key};
use crate::input::gamepad::{Gamepad, GamepadPowerInfo};
use crate::interface::winit::winit_map_mouse_button;
use crate::output::audio_device::{self, AudioDevice};
use crate::rendering::egui::EGui;
use crate::rendering::scene::Scene;
use crate::resources::resources::load_binary;
use crate::state::gui::editor::editor::Editor;
use crate::rendering::wgpu::WGpu;
use crate::state::helper::render_item::get_render_item_mut;
use crate::state::scene::camera::Camera;
use crate::state::scene::components::animation::Animation;
use crate::state::scene::components::material::Material;
use crate::state::scene::components::sound::{Sound, SoundType};
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::light::Light;
use crate::state::scene::node::Node;
use crate::state::scene::scene_controller::character_controller::CharacterController;
use crate::state::scene::sound_source::SoundSource;
use crate::state::scene::utilities::scene_utils::{self, attach_sound_to_node, execute_on_scene_mut_and_wait, execute_on_state_mut, load_object};
use crate::state::state::{State, StateItem, FPS_CHART_VALUES, REFERENCE_UPDATE_FRAMES};

use super::gilrs::{gilrs_event, gilrs_initialize};
use super::winit::winit_map_key;

const FPS_CHART_FACTOR: f32 = 25.0;

pub struct MainInterface
{
    pub state: StateItem,
    start_time: Instant,

    window_title: String,

    editor_gui: Editor,

    wgpu: WGpu,
    window: Arc<Window>,
    egui: EGui,

    gilrs: Option<Gilrs>
}

impl MainInterface
{
    //pub async fn new(window: Arc<Window>, event_loop: &winit::event_loop::EventLoop<()>) -> Self
    pub async fn new(window: Arc<Window>) -> Self
    {
        let audio_device = AudioDevice::default();
        let state = State::new(Arc::new(RwLock::new(Box::new(audio_device))));
        let state = Rc::new(RefCell::new(state));

        let samlpes;
        let mut wgpu: WGpu;
        {
            let state = &mut *(state.borrow_mut());
            state.width = window.inner_size().width;
            state.height = window.inner_size().height;
            state.scale_factor = window.scale_factor() as f32;

            wgpu = WGpu::new(window.clone(), state).await;

            dbg!(state.adapter.max_msaa_samples);
            state.rendering.msaa.set(cmp::min(state.rendering.msaa.get_ref().clone(), state.adapter.max_msaa_samples));
            samlpes = *(state.rendering.msaa.get_ref());

            wgpu.create_msaa_texture(samlpes);
        }

        //let egui = EGui::new(event_loop, wgpu.device(), wgpu.surface_config(), &window);
        let egui = EGui::new(wgpu.device(), wgpu.surface_config(), window.clone());

        let mut editor_gui = Editor::new();
        {
            let state = & *(state.borrow());
            editor_gui.init(state, &egui);
        }

        let gilrs_res = Gilrs::new();
        let mut gilrs = None;
        if let Ok(gilrs_res) = gilrs_res
        {
            gilrs = Some(gilrs_res);
        }

        let mut interface = Self
        {
            state,
            start_time: Instant::now(),

            window_title: window.title().clone(),

            editor_gui,

            wgpu,
            window,
            egui,

            gilrs
        };

        interface.app_init();
        interface.init();

        interface
    }

    pub fn init(&mut self)
    {
        let state = &mut *(self.state.borrow_mut());
        let samlpes = *(state.rendering.msaa.get_ref());

        // move out scenes from state to prevent using multiple mut borrows
        let mut scenes = vec![];
        swap(&mut state.scenes, &mut scenes);

        for scene in &mut scenes
        {
            let render_item = Scene::new(&mut self.wgpu, state, scene, samlpes);
            scene.render_item = Some(Box::new(render_item));
        }

        swap(&mut scenes, &mut state.scenes);

        // gamepad init
        if let Some(gilrs) = &mut self.gilrs
        {
            gilrs_initialize(state, gilrs);
        }
    }

    pub fn window(&self) -> &Window
    {
        &self.window
    }

    pub fn resize(&mut self, dimensions: Option<winit::dpi::PhysicalSize<u32>>, scale_factor: Option<f64>)
    {
        let mut width;
        let mut height;

        if let Some(dimensions) = dimensions
        {
            width = dimensions.width;
            height = dimensions.height;
        }
        else
        {
            let size = self.window.inner_size();
            width = size.width;
            height = size.height;
        }

        if width == 0 { width = 1; }
        if height == 0 { height = 1; }

        self.wgpu.resize(width, height);
        self.egui.resize(width, height, scale_factor);

        {
            let state = &mut *(self.state.borrow_mut());

            state.width = width;
            state.height = height;
            state.scale_factor = self.window.scale_factor() as f32;

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

    pub fn app_init(&mut self)
    {
        //init scene
        {
            let state = &mut *(self.state.borrow_mut());

            let mut scene = crate::state::scene::scene::Scene::new(0, "main scene", state.audio_device.clone());
            scene.add_defaults();

            // ********** cam **********
            /*
            for i in 0..4
            {
                let cam_id = scene.id_manager.get_next_camera_id();
                let mut cam = Camera::new(cam_id, format!("cam {}", i).to_string());
                let cam_data = cam.get_data_mut().get_mut();
                cam_data.fovy = 45.0f32.to_radians();
                cam_data.eye_pos = Point3::<f32>::new(0.0, 4.0, 15.0);
                cam_data.dir = Vector3::<f32>::new(-cam_data.eye_pos.x, -cam_data.eye_pos.y + 5.0, -cam_data.eye_pos.z);
                cam_data.clipping_near = 0.1;
                cam_data.clipping_far = 1000.0;

                scene.cameras.push(Box::new(cam));
            }

            scene.cameras[0].init(0.0, 0.0, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[1].init(0.5, 0.0, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[2].init(0.0, 0.5, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
            scene.cameras[3].init(0.5, 0.5, 0.5, 0.5, self.wgpu.surface_config().width, self.wgpu.surface_config().height);
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
            scene.load("objects/bastl/bastl.obj").unwrap();
            let n0 = scene.nodes.get(0).unwrap().clone();
            let n1 = scene.nodes.get_mut(1).unwrap().clone();
            n1.write().unwrap().merge_mesh(&n0);

            scene.nodes.remove(0);

            scene.load("objects/cube/cube.obj").unwrap();
            scene.load("objects/plane/plane.obj").unwrap();

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

            /*
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
            */

            /*
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
             */

            /*
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
            */

            /*
            // add light
            //if scene.lights.get_ref().len() == 0
            {
                let light_id = scene.id_manager.get_next_light_id();
                let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(0.0, 4.0, 4.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            }
             */

            /*
            // add camera
            if scene.cameras.len() == 0
            {
                let mut cam = Camera::new(scene.id_manager.get_next_camera_id(), "Cam".to_string());
                cam.update_resolution(state.width, state.height);
                let cam_data = cam.get_data_mut().get_mut();
                cam_data.fovy = 45.0f32.to_radians();
                cam_data.eye_pos = Point3::<f32>::new(0.0, 1.0, 1.5);
                cam_data.dir = Vector3::<f32>::new(-cam_data.eye_pos.x, -cam_data.eye_pos.y, -cam_data.eye_pos.z);
                cam_data.clipping_near = 0.001;
                cam_data.clipping_far = 1000.0;
                scene.cameras.push(Box::new(cam));
            }
             */

             /*
            // camera movement controller
            if scene.cameras.len() > 0
            {
                let cam = scene.cameras.get_mut(0).unwrap();
                //cam.add_controller_fly(true, Vector2::<f32>::new(0.0015, 0.0015), 0.1, 0.2);

                let mouse_sensivity = if platform::is_mac() { 0.1 } else { 0.01 };
                cam.add_controller_target_rotation(3.0, Vector2::<f32>::new(0.0015, 0.0015), mouse_sensivity);

                cam.controller.as_mut().unwrap().as_any_mut().downcast_mut::<TargetRotationController>().unwrap().auto_rotate = Some(0.005);
            }

            */


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
            let scene_id = scene.id.clone();
            let id_manager = scene.id_manager.clone();
            let id_manager_clone = scene.id_manager.clone();
            let id_manager_thread = scene.id_manager.clone();
            let main_queue = state.main_thread_execution_queue.clone();

            let grid_size = self.editor_gui.editor_state.grid_size;
            let grid_amount = self.editor_gui.editor_state.grid_amount;

            //scene.update(&mut state.input_manager, state.frame_scale);
            state.scenes.push(Box::new(scene));

            let main_queue_clone = main_queue.clone();
            spawn_thread(move ||
            {
                scene_utils::create_grid(scene_id, main_queue_clone.clone(), id_manager.clone(), grid_amount, grid_size);
            });
            //scene_utils::create_grid(&mut scene, 1, 1.0);

            let main_queue_clone = main_queue.clone();
            let audio_device = state.audio_device.clone();

            spawn_thread(move ||
            {
                let gizmo_nodes = scene_utils::load_object("objects/gizmo/gizmo.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
                {
                    if let Ok(gizmo_nodes) = &gizmo_nodes
                    {
                        for node_id in gizmo_nodes
                        {
                            if let Some(node) = scene.find_node_by_id(*node_id)
                            {
                                if let Some(material) = node.read().unwrap().find_component::<Material>()
                                {
                                    component_downcast_mut!(material, Material);
                                    material.get_data_mut().get_mut().unlit_shading = true;
                                }
                            }
                        }
                    }
                }));

                //let nodes = scene_utils::load_object("objects/temp/xbot@dancing.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/mech_drone.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/woman_cyber_free_model_by_oscar_creativo.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/AnimatedTriangle.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/Alien.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/Alien2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/RecursiveSkeletons.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/RiggedFigure.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/RiggedFigure.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/RiggedSimple.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/SimpleSkin.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/rpm.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/rpm2_2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/rpm2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/rpm3.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/character_with_animation.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/animated_astronaut_character_in_space_suit_loop.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/animated_astronaut_character_in_space_suit_loop_2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/ct_gsg9_hip_hop_move.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/ct_gsg9_hip_hop_move_2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/whale.CYCLES.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/thinmat_model.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/mole.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/avatar.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                //scene_utils::load_object("objects/temp/box.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                scene_utils::load_object("objects/temp/box2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                scene_utils::load_object("objects/temp/extras.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                //let nodes = scene_utils::load_object("scenes/de_dust2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                let nodes = scene_utils::load_object("scenes/simple map/simple map.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                //let nodes = scene_utils::load_object("objects/temp/avatar3.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //scene_utils::load_object("objects/temp/traffic_cone_game_ready.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //scene_utils::load_object("objects/temp/headcrab.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                //let nodes = scene_utils::load_object("objects/temp/lotus2.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/character_with_animation.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/sofa.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/sofa.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);
                //let nodes = scene_utils::load_object("objects/temp/test.gltf", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                //let nodes = scene_utils::load_object("objects/glass/glass.glb", scene_id, main_queue_clone.clone(), id_manager_clone.clone(), false, true, false, 0);

                let id_manager_clone_inner = id_manager_clone.clone();

                execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
                {
                    // start first animation
                    if let Ok(nodes) = &nodes
                    {
                        for node_id in nodes
                        {
                            if let Some(node) = scene.find_node_by_id(*node_id)
                            {
                                if let Some(animation) = node.read().unwrap().find_animation_by_name("")
                                {
                                    component_downcast_mut!(animation, Animation);
                                    animation.start();
                                }
                            }
                        }
                    }

                    // cone
                    let cone = scene.find_node_by_name("traffic_cone_game_ready");
                    //let cone = scene.find_node_by_name("headcrab");
                    if let Some(cone) = cone
                    {
                        /*
                        {
                            let mut cone = cone.write().unwrap();

                            if cone.find_component::<Transformation>().is_none()
                            {
                                let component_id = id_manager_clone_inner.clone().write().unwrap().get_next_component_id();
                                cone.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(component_id, "Transform")))));
                            }

                            if let Some(transform) = cone.find_component::<Transformation>()
                            {
                                component_downcast_mut!(transform, Transformation);
                                transform.apply_scale_all_axes(0.01, true);
                            }
                        }
                        */

                        // set cone as head
                        let head = scene.find_node_by_name("mixamorig:HeadTop_End");
                        if let Some(head) = head
                        {
                            Node::set_parent(cone.clone(), head);
                        }
                    }

                    // add camera controller and run auto setup
                    let mut controller = CharacterController::default();
                    controller.auto_setup(scene, "avatar3");
                    scene.pre_controller.push(Box::new(controller));
                }));

                let light_id = id_manager_clone.clone().write().unwrap().get_next_light_id();
                execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
                {
                    let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(2.0, 50.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                    scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                }));

                // sound
                //attach_sound_to_node("sounds/m16.ogg", "Cube", SoundType::Spatial, main_queue_clone.clone(), scene_id, audio_device.clone());
                //attach_sound_to_node("sounds/PSY - Gangnam Style.mp3", "Cube", SoundType::Spatial, main_queue_clone.clone(), scene_id, audio_device.clone());
            });

            //load default env texture
            state.load_scene_env_map("textures/environment/footprint_court.jpg", scene_id);

            {
                let main_queue = main_queue.clone();
                let editor_state = self.editor_gui.editor_state.loading.clone();
                spawn_thread(move ||
                {
                    *editor_state.write().unwrap() = true;

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(|scene|
                    {
                        //scene.clear_empty_nodes();

                        // add camera
                        if scene.cameras.len() == 0
                        {
                            let id = scene.id_manager.write().unwrap().get_next_camera_id();
                            let mut cam = Camera::new(id, "Cam".to_string());

                            cam.add_controller_fly(false, Vector2::<f32>::new(0.0015, 0.0015), 0.1, 0.2);

                            let cam_data = cam.get_data_mut().get_mut();
                            cam_data.fovy = 45.0f32.to_radians();
                            cam_data.eye_pos = Point3::<f32>::new(0.0, 5.0, 10.0);
                            cam_data.dir = Vector3::<f32>::new(-cam_data.eye_pos.x, -cam_data.eye_pos.y, -cam_data.eye_pos.z);
                            cam_data.clipping_near = 0.1;
                            cam_data.clipping_far = 1000.0;
                            scene.cameras.push(Box::new(cam));
                        }
                    }));

                    *editor_state.write().unwrap() = false;
                });
            }


            // sound debugging
            /*
            {
                let audio_device = state.audio_device.clone();
                let main_queue = main_queue.clone();
                spawn_thread(move ||
                {
                    let audio_device = audio_device.clone();
                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
                    {
                        let sound_bytes = load_binary("sounds/click.ogg").unwrap();
                        let sound_id = scene.id_manager.write().unwrap().get_next_sound_source_id();
                        let sound = SoundSource::new(sound_id, "sound", audio_device.clone(), &sound_bytes, None);

                        scene.sound_sources.insert(sound.hash.clone(),  Arc::new(RwLock::new(Box::new(sound))));
                    }));
                });
            }

            {
                let audio_device = state.audio_device.clone();
                let main_queue = main_queue.clone();
                spawn_thread(move ||
                {
                    let audio_device = audio_device.clone();
                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
                    {
                        let sound_bytes = load_binary("sounds/infoPopup.ogg").unwrap();
                        let sound_id = scene.id_manager.write().unwrap().get_next_sound_source_id();
                        let sound = SoundSource::new(sound_id, "sound", audio_device.clone(), &sound_bytes, None);

                        scene.sound_sources.insert(sound.hash.clone(),  Arc::new(RwLock::new(Box::new(sound))));
                    }));
                });
            }
             */
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
    }

    pub fn update(&mut self)
    {
        // ******************** update states ********************
        {
            let state = &mut *(self.state.borrow_mut());
            if let Some(gilrs) = &mut self.gilrs
            {
                gilrs_event(state, gilrs, state.stats.frame);
            }
        }

        let frame_time = Instant::now();

        // ******************** update states ********************
        {
            let state = &mut *(self.state.borrow_mut());

            // vsync
            let (v_sync, vsync_changed) = state.rendering.v_sync.consume_clone();
            if vsync_changed
            {
                self.wgpu.set_vsync(v_sync);
            }

            // full screen
            let (fullscreen, fullscreen_changed) = state.rendering.fullscreen.consume_clone();
            if fullscreen_changed
            {
                let mut fullscreen_mode = None;
                if fullscreen { fullscreen_mode = Some(Fullscreen::Borderless(None)); }
                self.window.set_fullscreen(fullscreen_mode);
            }

            // fps
            let current_time = state.stats.fps_timer.elapsed().as_millis();
            state.stats.fps += 1;

            if current_time / 1000 > state.stats.last_time / 1000
            {
                state.stats.last_time = state.stats.fps_timer.elapsed().as_millis();

                state.stats.last_fps = state.stats.fps;
                state.stats.fps_chart.push(state.stats.last_fps);
                if state.stats.fps_chart.len() > FPS_CHART_VALUES
                {
                    state.stats.fps_chart.remove(0);
                }

                self.window.set_title(format!("{} | FPS: {}", &self.window_title, state.stats.last_fps).as_str());
                state.stats.fps = 0;
            }

            // frame scale
            let elapsed = self.start_time.elapsed();
            let now = elapsed.as_micros();

            if state.stats.frame_update_time > 0 && now - state.stats.frame_update_time > 0
            {
                state.stats.frame_scale = REFERENCE_UPDATE_FRAMES / (1000000.0 / (now - state.stats.frame_update_time) as f32);
            }

            state.stats.frame_update_time = now;
        }

        // ******************** editor/ui update ********************
        {
            let now = Instant::now();
            let state = &mut *(self.state.borrow_mut());
            self.editor_gui.update(state);

            state.stats.editor_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** build ui ********************
        if self.editor_gui.editor_state.visible
        {
            let now = Instant::now();
            let state = &mut *(self.state.borrow_mut());

            let gui_output = self.editor_gui.build_gui(state, &self.window, &mut self.egui);
            self.egui.output = Some(gui_output);

            //self.gui.request_repaint();
            state.stats.egui_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** app update ********************

        if !self.state.borrow().pause
        {
            let now = Instant::now();
            self.app_update();

            let state = &mut *(self.state.borrow_mut());
            state.stats.app_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** update main thread queue ********************
        {
            let state = &mut *(self.state.borrow_mut());
            let main_queue = state.main_thread_execution_queue.clone();
            ExecutionQueue::run_all(main_queue, state);
        }

        // ******************** update scene ********************
        if !self.state.borrow().pause
        {
            let engine_update_time = Instant::now();

            let state = &mut *(self.state.borrow_mut());

            // msaa
            let (msaa_samples, msaa_changed) = state.rendering.msaa.consume_clone();

            if msaa_changed
            {
                self.wgpu.create_msaa_texture(msaa_samples);
            }

            state.update(state.stats.frame_update_time, state.stats.frame_scale, state.stats.frame);

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

            state.stats.engine_update_time = engine_update_time.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** render ********************
        let (output, view, msaa_view, mut encoder) = self.wgpu.start_render();
        {
            let state = &mut *(self.state.borrow_mut());

            // render scenes
            {
                let engine_render_time = Instant::now();

                state.stats.draw_calls = 0;

                for scene in &mut state.scenes
                {
                    if !scene.visible
                    {
                        continue;
                    }

                    let mut render_item = scene.render_item.take();

                    let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                    render_scene.distance_sorting = state.rendering.distance_sorting;
                    state.stats.draw_calls += render_scene.render(&mut self.wgpu, &view, &msaa_view, &mut encoder, scene);

                    scene.render_item = render_item;
                }

                state.stats.engine_render_time = engine_render_time.elapsed().as_micros() as f32 / 1000.0;
            }

            // render egui
            if self.editor_gui.editor_state.visible
            {
                let now = Instant::now();
                self.egui.render(&mut self.wgpu, &view, &mut encoder);

                state.stats.egui_render_time = now.elapsed().as_micros() as f32 / 1000.0;
            }
        }
        self.wgpu.end_render(output, encoder);

        // ******************** screenshot ********************
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
                        render_scene.distance_sorting = state.rendering.distance_sorting;
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

        // ******************** update inputs ********************
        {
            let state = &mut *(self.state.borrow_mut());
            state.input_manager.update();
        }

        // ******************** mouse visibility ********************
        {
            let state = &mut *(self.state.borrow_mut());
            let (visible, changed) = state.input_manager.mouse.visible.consume_borrow();
            if changed
            {
                self.window.set_cursor_visible(*visible);
            }
        }

        // ******************** reset global change tracker ********************
        {
            let state = &mut *(self.state.borrow_mut());
            state.audio_device.write().unwrap().data.consume_change();
        }

        // ******************** frame time ********************
        {
            let state = &mut *(self.state.borrow_mut());
            state.stats.frame_time = frame_time.elapsed().as_micros() as f32 / 1000.0;

            state.stats.fps_absolute = (1000.0 / (state.stats.engine_render_time + state.stats.engine_update_time)) as u32;

            // frame update
            state.stats.frame += 1;
        }
    }

    pub fn check_exit(&mut self) -> bool
    {
        self.state.borrow().exit
    }

    pub fn window_input(&mut self, event: &winit::event::WindowEvent)
    {
        if self.editor_gui.editor_state.visible && self.egui.on_event(event, self.window.clone())
        {
            return;
        }
        else
        {
            let global_state = &mut *(self.state.borrow_mut());
            //let main_queue = global_state.main_thread_execution_queue.clone();

            match event
            {
                winit::event::WindowEvent::KeyboardInput { device_id, event, is_synthetic } =>
                {
                    let key = winit_map_key(&event.logical_key, &event.physical_key, event.location);

                    if event.state == ElementState::Pressed
                    {
                        global_state.input_manager.keyboard.set_key(key, true, global_state.stats.frame);
                    }
                    else
                    {
                        global_state.input_manager.keyboard.set_key(key, false, global_state.stats.frame);
                    }
                },
                winit::event::WindowEvent::ModifiersChanged(modifiers_state) =>
                {
                    // TODO: Check if windows is able to catch left/right difference
                    if is_windows()
                    {
                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftAlt, modifiers_state.state().alt_key(), global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightAlt, modifiers_state.state().alt_key(), global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftCtrl, modifiers_state.state().control_key(), global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightCtrl, modifiers_state.state().control_key(), global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftLogo, modifiers_state.state().super_key(), global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightLogo, modifiers_state.state().super_key(), global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftShift, modifiers_state.state().shift_key(), global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightShift, modifiers_state.state().shift_key(), global_state.stats.frame);
                    }
                    else
                    {
                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftAlt, modifiers_state.lalt_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightAlt, modifiers_state.ralt_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftCtrl, modifiers_state.lcontrol_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightCtrl, modifiers_state.rcontrol_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftLogo, modifiers_state.lsuper_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightLogo, modifiers_state.rsuper_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.input_manager.keyboard.set_modifier(Modifier::LeftShift, modifiers_state.lshift_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.input_manager.keyboard.set_modifier(Modifier::RightShift, modifiers_state.rshift_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                    }
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

                    global_state.input_manager.mouse.set_button(button, pressed, global_state.stats.frame);
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
                    let mut pos = Point2::<f32>::new(position.x as f32, position.y as f32);

                    pos.x = pos.x;
                    // invert pos (because x=0, y=0 is bottom left and "normal" window is top left)
                    pos.y = global_state.height as f32 - pos.y;

                    global_state.input_manager.mouse.set_pos(pos, global_state.stats.frame, global_state.width, global_state.height);
                },
                winit::event::WindowEvent::Focused(focus) =>
                {
                    global_state.in_focus = *focus;
                    global_state.input_manager.reset();
                },
                winit::event::WindowEvent::DroppedFile(path) =>
                {
                    if let Some(path) = path.to_str()
                    {
                        self.editor_gui.apply_external_asset_drag(global_state, path.to_string());
                        self.window.request_redraw();
                    }
                },
                _ => {}
            }
        }
    }

    pub fn device_input(&mut self, event: &winit::event::DeviceEvent)
    {
        let global_state = &mut *(self.state.borrow_mut());

        match event
        {
            winit::event::DeviceEvent::MouseMotion { delta } =>
            {
                let velocity = Vector2::<f32>::new(delta.0 as f32, -delta.1 as f32);
                global_state.input_manager.mouse.set_raw_velocity(velocity, global_state.stats.frame);
            },
            _ => {}
        }
    }

    pub fn update_done(&mut self)
    {
        let global_state = &mut *(self.state.borrow_mut());

        let (visible, change) = global_state.input_manager.mouse.visible.consume_clone();

        if change
        {
            if visible
            {
                _ = self.window.set_cursor_grab(CursorGrabMode::None);
            }
            else
            {
                self.window.set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_e| self.window.set_cursor_grab(CursorGrabMode::Locked))
                .unwrap();
            }
        }

        /*
        if !*global_state.input_manager.mouse.visible.get_ref()
        {
            let window_size = self.window.inner_size();
            let center = PhysicalPosition::new(window_size.width as f64 / 2.0, window_size.height as f64 / 2.0);

            self.window.set_cursor_position(center).unwrap_or_else(|e|
            {
                dbg!("Failed to set mouse position: {:?}", e);
            });
        }
        */
    }
}
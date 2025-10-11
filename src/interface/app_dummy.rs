use std::{cell::RefCell, f32::consts::PI, sync::{Arc, RwLock}};

use egui::epaint::EllipseShape;
use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector2, Vector3};

use crate::{component_downcast_mut, console_debug, console_error, helper::{change_tracker::ChangeTracker, concurrency::thread::spawn_thread}, state::scene::{camera::Camera, components::{animation::{Animation, AnimationLayerType}, look_at::LookAt}, light::Light, node::Node, scene_controller::char_controller::CharacterController, utilities::scene_utils::{self, execute_on_scene_mut_and_wait}}};

use super::{app::App, context::Context};

pub struct AppDummy
{

}

impl AppDummy
{
    pub fn new() -> AppDummy
    {
        AppDummy {}
    }
}

impl App for AppDummy
{
    fn init(&mut self, context: &mut Context)
    {
        let scene_id = context.get_main_scene_id();

        let state = &mut *(context.state.borrow_mut());

        if scene_id.is_none()
        {
            return;
        }
        let scene_id = scene_id.unwrap();

        //load default env texture
        state.load_scene_env_map("textures/environment/footprint_court.jpg", scene_id);

        // ********** cam **********
        /*
        for i in 0..4
        {
            let cam_id = id_manager::get_next_camera_id();
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
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }
        {
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(-2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }
        */


        // helmet
        /*
        {
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 100.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }

        {
            let cam_id = id_manager::get_next_camera_id();
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
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 100.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }

        {
            let cam_id = id_manager::get_next_camera_id();
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
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(6.8627195, 3.287831, 1.4585655), Vector3::<f32>::new(1.0, 1.0, 1.0), 200.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }

        {
            let cam_id = id_manager::get_next_camera_id();
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
            node.add_component(Box::new(Transformation::identity(id_manager::get_next_component_id())));
            node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, 0.0, -15.0));

            //node.remove_component_by_type::<Transformation>();
        }

        {
            let node_id = 1;
            let node = scene.nodes.get_mut(node_id).unwrap();

            let mut node = node.write().unwrap();
            node.add_component(Box::new(Transformation::identity(id_manager::get_next_component_id())));
            node.find_component_mut::<Transformation>().unwrap().apply_scale(Vector3::<f32>::new(4.0, 4.0, 4.0));
            node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.0, 15.0, -30.0));

            //node.remove_component_by_type::<Transformation>();
        }

        let node1 = Node::new(id_manager::get_next_node_id(), "test1");
        let node2 = Node::new(id_manager::get_next_node_id(), "test2");

        scene.add_node(node1.clone());
        Node::add_node(node1, node2);
        */

        /*
        scene.clear_empty_nodes();

        let root_node = Node::new(id_manager::get_next_node_id(), "root node");
        {
            let mut root_node = root_node.write().unwrap();
            root_node.add_component(Arc::new(RwLock::new(Box::new(Alpha::new(id_manager::get_next_component_id(), "Alpha Test", 1.0)))));
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
                instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id_manager::get_next_component_id(), "Transform")))));

                instance.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_manager::get_next_component_id(), "Transform Animation", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.01, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
            }
            //node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_manager::get_next_component_id(), Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.01, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
        }
            */

        /*
        if let Some(train) = scene.find_node_by_name("Train")
        {
            let mut node = train.write().unwrap();
            node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_manager::get_next_component_id(), "Left", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, -0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
            node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_manager::get_next_component_id(), "Right", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));

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
            let light_id = id_manager::get_next_light_id();
            let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(0.0, 4.0, 4.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
        }
            */

        /*
        // add camera
        if scene.cameras.len() == 0
        {
            let mut cam = Camera::new(id_manager::get_next_camera_id(), "Cam".to_string());
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
            //node.add_component(Box::new(Transformation::identity(id_manager::get_next_component_id())));
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
            //node.add_component(Box::new(Transformation::identity(id_manager::get_next_component_id())));
            node.find_component_mut::<Transformation>().unwrap().apply_translation(Vector3::<f32>::new(0.15, -0.7, -0.2));
            node.find_component_mut::<Transformation>().unwrap().apply_scale(Vector3::<f32>::new(25.0, 25.0, 25.0));

            //node.remove_component_by_type::<Transformation>();
        }
        */

        // ********** scene add **********
        //let scene_id = scene.id.clone();
        let main_queue = state.main_thread_execution_queue.clone();

        //scene.update(&mut state.input_manager, state.frame_scale);
        //state.scenes.push(Box::new(scene));


        let main_queue_clone = main_queue.clone();
        let audio_device = state.io.audio_device.clone();

        spawn_thread(move ||
        {

            //let nodes = scene_utils::load_object("scenes/Sponza_fixed.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            //let nodes = scene_utils::load_object("objects/temp/xbot@dancing.glb", sscene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/mech_drone.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/woman_cyber_free_model_by_oscar_creativo.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/AnimatedTriangle.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/Alien.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/Alien2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/RecursiveSkeletons.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/RiggedFigure.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/RiggedFigure.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/RiggedSimple.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/SimpleSkin.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/rpm.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/rpm2_2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/rpm2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/rpm3.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/character_with_animation.glb", scene_id, None, main_queue_clone.clone(),false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/animated_astronaut_character_in_space_suit_loop.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/animated_astronaut_character_in_space_suit_loop_2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/ct_gsg9_hip_hop_move.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/ct_gsg9_hip_hop_move_2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/whale.CYCLES.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/thinmat_model.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/mole.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/avatar.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            //scene_utils::load_object("objects/temp/box.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //scene_utils::load_object("objects/temp/box2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //scene_utils::load_object("objects/temp/extras.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            //let nodes = scene_utils::load_object("scenes/de_dust2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);


            //let nodes = scene_utils::load_object("scenes/simple map/simple map.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            let avatar_nodes = scene_utils::load_object("resourcesLocal/objects/temp/avatar3.glb", scene_id, None, main_queue_clone.clone(), false, false, true, false, 0);

            if avatar_nodes.is_err()
            {
                console_error!("error loading avatar3.glb: {}", avatar_nodes.err().unwrap());
                return;
            }

            let avatar_root = avatar_nodes.as_ref().unwrap()[0].clone();

            // //let _ = scene_utils::load_and_retarget_animation("objects/temp/Animation Only - Happy Idle.glb", scene_id, avatar_nodes.unwrap()[0], main_queue_clone.clone(),);
            //let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/dancing.glb", scene_id, avatar_nodes.unwrap()[0], main_queue_clone.clone(), Some("mixamorig:Hips"));
            //let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/animations/shoot.glb", scene_id, avatar_nodes.unwrap()[0], main_queue_clone.clone(), None);
            //let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/animations/shoot stand.glb", scene_id, avatar_nodes.unwrap()[0], main_queue_clone.clone(), None);
            let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/animations/idle aim.glb", scene_id, avatar_root.clone(), main_queue_clone.clone(), None);
            let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/animations/idle prone.glb", scene_id, avatar_root.clone(), main_queue_clone.clone(), None);
            let _ = scene_utils::load_and_re_target_animation("resourcesLocal/objects/temp/animations/idle crouch.glb", scene_id, avatar_root.clone(), main_queue_clone.clone(), None);


            //scene_utils::load_object("objects/temp/traffic_cone_game_ready.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //scene_utils::load_object("objects/temp/headcrab.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            //let nodes = scene_utils::load_object("objects/temp/lotus2.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/character_with_animation.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/sofa.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/sofa.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);
            //let nodes = scene_utils::load_object("objects/temp/test.gltf", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            //let nodes = scene_utils::load_object("objects/glass/glass.glb", scene_id, None, main_queue_clone.clone(), false, true, false, 0);

            execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
            {
                // start first animation
                // if let Ok(nodes) = &nodes
                // {
                //     for node_id in nodes
                //     {
                //         if let Some(node) = scene.find_node_by_id(*node_id)
                //         {
                //             if let Some(animation) = node.read().unwrap().find_animation_by_name("")
                //             {
                //                 component_downcast_mut!(animation, Animation);
                //                 animation.start();
                //             }
                //         }
                //     }
                // }

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
                            let component_id = id_manager::get_next_component_id();
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

                /*
                let mut controller = CharacterController::default();
                controller.auto_setup(scene, "avatar3", "");
                scene.pre_controller.push(Box::new(controller));
                */


                // set pos for fall test
                /*
                if let Some(avatar_root) = scene.find_node_by_id(avatar_root)
                {
                    let avatar_root = avatar_root.read().unwrap();
                    let transform = avatar_root.find_component::<Transformation>().unwrap();
                    component_downcast_mut!(transform, Transformation);
                    transform.set_translation(Vector3::<f32>::new(21.980, 22.845, 6.331));
                    transform.set_rotation(Vector3::<f32>::new(0.0, -2.618, 0.0));
                }
                    */

                // add look up joint animation
                if let Some(avatar_root) = scene.find_node_by_id(avatar_root)
                {
                    let avatar_root = avatar_root.read().unwrap();

                    let spine = avatar_root.find_child_node_by_name("mixamorig:Spine1");
                    let armature = avatar_root.find_child_node_by_name("Armature");

                    if spine.is_some() && armature.is_some()
                    {
                        let armature = armature.unwrap();
                        // Target position in world space: 2 units in front of the avatar
                        let look_at = LookAt::new("Aim", spine.clone().unwrap(), Vector3::new(0.0, 1.5, -2.0));
                        armature.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(look_at))));

                        // get transform between root joint and root node (because AdditiveComponentAbsolute just takes "full" joint transform into account - and nothing inbetween root and joint root)
                        /*
                        let parent_transform = Node::get_transform_between_root_joint_and_root_node(spine.clone().unwrap());
                        let parent_inv = parent_transform.try_inverse().unwrap_or(Matrix4::<f32>::identity());

                        console_debug!(parent_inv);
                        */

                        /*
                        // get transform between root joint and root node (because AdditiveComponentAbsolute just takes "full" joint transform into account - and nothing inbetween root and joint root)
                        let parent_transform = Node::get_transform_between_root_joint_and_root_node(spine.clone().unwrap());
                        let parent_inv = parent_transform.try_inverse().unwrap_or(Matrix4::<f32>::identity());

                        let parent_axes = parent_inv.fixed_view::<3,3>(0,0);
                        let avatar_x = nalgebra::Unit::new_normalize(parent_axes * Vector3::x()); // Look Up/Down
                        let avatar_y = nalgebra::Unit::new_normalize(parent_axes * Vector3::y()); // Look Left/Right

                        let directions = vec!
                        [
                            ("look up", UnitQuaternion::from_axis_angle(&avatar_x, std::f32::consts::PI / 2.0)),
                            ("look down", UnitQuaternion::from_axis_angle(&avatar_x, -std::f32::consts::PI / 2.0)),
                            ("look left", UnitQuaternion::from_axis_angle(&avatar_y, std::f32::consts::PI / 2.0)),
                            ("look right", UnitQuaternion::from_axis_angle(&avatar_y, -std::f32::consts::PI / 2.0)),
                        ];

                        let armature = armature.unwrap();
                        let mut armature = armature.write().unwrap();

                        for (name, delta_rot) in directions
                        {
                            let mut animation = Animation::new_joint_transform_quat
                            (
                                name,
                                spine.clone().unwrap(),
                                None,
                                Some(delta_rot),
                                None,
                            );

                            animation.layer_type = AnimationLayerType::AdditiveComponentAbsolute;
                            armature.add_component(Arc::new(RwLock::new(Box::new(animation))));
                        }
                        */
                    }

                    avatar_root.start_animation("aim");
                    avatar_root.start_animation("idle aim");
                    //avatar_root.start_animation("look left");
                }
            }));

            /*
            execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
            {
                let light = Light::new_point("Point".to_string(), Point3::<f32>::new(2.0, 50.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

                scene.add_light_hemisperical("hemi", Vector3::<f32>::new(0.0, -1.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0), Vector3::<f32>::new(0.0, 0.0, 0.0), 1.0);
            }));
             */

            // sound
            //attach_sound_to_node("sounds/m16.ogg", "Cube", SoundType::Spatial, main_queue_clone.clone());
            //attach_sound_to_node("sounds/PSY - Gangnam Style.mp3", "Cube", SoundType::Spatial, main_queue_clone.clone());
        });

        /*
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
                        let mut cam = Camera::new(uuid, "Cam".to_string());

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
         */

        // TODO: remove me
        /*
        if let Some(train) = scene.find_node_by_name("Train")
        {
            let mut node = train.write().unwrap();
            let id_1 = scene.id_manager::get_next_component_id();
            let id_2 = id_manager::get_next_component_id();

            node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_1, "Left", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, -0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
            node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_2, "Right", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));

            let components_len = node.components.len();
            {
                let component = node.components.get_mut(components_len - 2).unwrap();
                component_downcast_mut!(component, TransformationAnimation);
                component.keyboard_key = Some(Key::ArrowLeft as usize);
            }

            {
                let component = node.components.get_mut(components_len - 1).unwrap();
                component_downcast_mut!(component, TransformationAnimation);
                component.keyboard_key = Some(Key::ArrowRight as usize);
            }
        }
            */



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
                    let sound_id = id_manager::get_next_sound_source_id();
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
                    let sound_id = id_manager::get_next_sound_source_id();
                    let sound = SoundSource::new(sound_id, "sound", audio_device.clone(), &sound_bytes, None);

                    scene.sound_sources.insert(sound.hash.clone(),  Arc::new(RwLock::new(Box::new(sound))));
                }));
            });
        }
            */

    }

    fn update(&mut self, context: &mut Context)
    {

    }

    fn resize(&mut self, context: &mut Context)
    {

    }

    fn exit(&mut self, context: &mut Context)
    {

    }

    fn request_exit(&mut self, context: &mut Context) -> bool
    {
        true
    }
}
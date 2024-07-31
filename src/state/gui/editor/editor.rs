use std::{cell::RefCell, f32::consts::PI, f64::consts::E, sync::{Arc, RwLock}};

use egui::FullOutput;

use nalgebra::{Vector3, Matrix4, Point2, Point3, Vector2};

use crate::{component_downcast_mut, helper::{change_tracker::ChangeTracker, concurrency::thread::spawn_thread, math::{approx_equal, approx_equal_vec, snap_to_grid, snap_to_grid_vec3}, platform}, input::{keyboard::{Key, Modifier}, mouse::MouseButton}, rendering::egui::EGui, state::{scene::{camera::Camera, camera_controller::{fly_controller::FlyController, target_rotation_controller::TargetRotationController}, components::{alpha::Alpha, component::{Component, ComponentItem}, transformation::Transformation, transformation_animation::TransformationAnimation}, light::Light, manager::id_manager, node::{Node, NodeItem}, scene::{PickPredicate, Scene, ScenePickRes}, utilities::scene_utils::{self, execute_on_scene_mut_and_wait, load_object}}, state::State}};

use super::{editor_state::{AssetType, EditMode, EditorState, PickType, SelectionType, SettingsPanel}, main_frame};

const OBJECTS_DIR: &str = "objects/";
const SCENES_DIR: &str = "scenes/";

pub struct Editor
{
    pub editor_state: EditorState,
}

impl Editor
{
    pub fn new() -> Editor
    {
        Self
        {
            editor_state: EditorState::new()
        }
    }

    pub fn init(&mut self, state: &State, egui: &EGui)
    {
        self.editor_state.load_asset_entries(SCENES_DIR, state, AssetType::Scene, egui);
        self.editor_state.load_asset_entries(OBJECTS_DIR, state, AssetType::Object, egui);
    }

    pub fn build_gui(&mut self, state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
    {
        let raw_input = egui.ui_state.take_egui_input(window);

        let full_output = egui.ctx.run(raw_input, |ctx|
        {
            main_frame::create_frame(ctx, &mut self.editor_state, state);
        });

        self.apply_asset_drag(state, &egui.ctx);

        // stop text input when the user wants to move/navigate in 3d space
        if state.input_manager.mouse.is_any_button_holding()
        {
            egui.ctx.memory_mut(|mem| { mem.stop_text_input() });
        }

        let platform_output = full_output.platform_output.clone();

        egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);

        full_output
    }

    pub fn update(&mut self, state: &mut State)
    {
        // update modes
        self.update_modes(state);

        // update grid based on camera pos
        self.update_grid(state);

        // select/pick objects
        self.select_object(state);

        // delete objects
        self.delete_objct(state);

        // edit mode
        self.move_object(state);
    }

    pub fn update_modes(&mut self, state: &mut State)
    {
        // start try out mde
        if !self.editor_state.try_out && (state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)) && state.input_manager.keyboard.is_pressed(Key::R)
        {
            self.editor_state.set_try_out(state, true);
        }

        // end try out mode
        if self.editor_state.try_out && state.input_manager.keyboard.is_pressed(Key::Escape)
        {
            self.editor_state.set_try_out(state, false);
        }

        // hide ui
        if state.input_manager.keyboard.is_pressed(Key::H)
        {
            self.editor_state.visible = !self.editor_state.visible;
        }

        // full screen
        if state.input_manager.keyboard.is_pressed(Key::F)
        {
            state.rendering.fullscreen.set(!*state.rendering.fullscreen.get_ref());
        }

        // escape
        if state.input_manager.keyboard.is_pressed(Key::Escape)
        {
            if self.editor_state.edit_mode.is_some()
            {
                self.editor_state.edit_mode = None;
            }
            else
            {
                self.editor_state.de_select_current_item(state);
            }
        }
    }

    pub fn update_grid(&mut self, state: &mut State)
    {
        let grid_size = self.editor_state.grid_size;

        for scene in &mut state.scenes
        {
            let scene_id = scene.id;

            let grid = scene.find_node_by_name("grid");

            // recreate grid
            if grid.is_some() && self.editor_state.grid_recreate
            {
                // delte first
                scene.delete_node_by_name("grid");

                let grid_size = self.editor_state.grid_size;
                let grid_amount = self.editor_state.grid_amount;

                let main_queue_clone = state.main_thread_execution_queue.clone();
                let id_manager_clone = scene.id_manager.clone();

                spawn_thread(move ||
                {
                    scene_utils::create_grid(scene_id, main_queue_clone.clone(), id_manager_clone.clone(), grid_amount, grid_size);
                });

                self.editor_state.grid_recreate = false;
            }

            // update grid position
            if let Some(grid) = grid
            {
                let mut grid = grid.write().unwrap();

                let mut transformation = grid.find_component::<Transformation>();
                if transformation.is_none()
                {
                    let id = scene.id_manager.write().unwrap().get_next_component_id();
                    grid.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id, "Transform")))));
                    transformation = grid.find_component::<Transformation>();
                }

                let camera = scene.get_active_camera();
                if let Some(camera) = camera
                {
                    let camera_data = camera.get_data();

                    let pos = &camera_data.eye_pos;
                    let mut pos = Vector3::<f32>::new(pos.x.round(), 0.0, pos.z.round());
                    pos = snap_to_grid_vec3(pos, grid_size);

                    let transformation = transformation.unwrap();
                    component_downcast_mut!(transformation, Transformation);

                    if !approx_equal_vec(&pos, &transformation.get_data().position)
                    {
                        transformation.set_translation(Vector3::<f32>::new(pos.x, pos.y, pos.z));
                    }
                }
            }
        }
    }

    pub fn select_object(&mut self, state: &mut State)
    {
        if !self.editor_state.try_out && (self.editor_state.selectable || self.editor_state.pick_mode != PickType::None) && self.editor_state.edit_mode.is_none()
        {
            let left_mouse_button = state.input_manager.mouse.clicked(MouseButton::Left);
            let right_mouse_button = state.input_manager.mouse.clicked(MouseButton::Right);

            if left_mouse_button || right_mouse_button
            {
                let pos = state.input_manager.mouse.point.pos;

                let mut hit: Option<ScenePickRes> = None;
                let mut scene_id: u64 = 0;

                if let Some(pos) = pos
                {
                    let pick_res = self.pick(state, pos, false, None);

                    if let Some(pick_res) = pick_res
                    {
                        scene_id = pick_res.0;
                        hit = Some(pick_res.1);
                    }
                }

                if let Some(hit) = hit
                {
                    // pick camera target
                    if self.editor_state.pick_mode == PickType::Camera && self.editor_state.selected_scene_id.is_some()
                    {
                        let scene_id: u64 = self.editor_state.selected_scene_id.unwrap();

                        let (camera_id, ..) = self.editor_state.get_object_ids();

                        let scene = state.find_scene_by_id_mut(scene_id);
                        if scene.is_none() { return; }

                        let scene = scene.unwrap();

                        if camera_id.is_none() { return; }
                        let camera_id = camera_id.unwrap();

                        if let Some(camera) = scene.get_camera_by_id_mut(camera_id)
                        {
                            camera.node = Some(hit.node.clone());
                        }
                    }
                    // pick parent target
                    else if self.editor_state.pick_mode == PickType::Parent && self.editor_state.selected_scene_id.is_some()
                    {
                        let scene_id: u64 = self.editor_state.selected_scene_id.unwrap();

                        let (node_id, ..) = self.editor_state.get_object_ids();

                        let scene = state.find_scene_by_id(scene_id);
                        if scene.is_none() { return; }

                        let scene = scene.unwrap();

                        if node_id.is_none() { return; }
                        let node_id = node_id.unwrap();

                        if let Some(node) = scene.find_node_by_id(node_id)
                        {
                            Node::set_parent(node, hit.node.clone());
                        }
                    }
                    // show selection
                    else
                    {
                        let mut node_arc = hit.node;
                        let mut use_root_node = false;

                        if left_mouse_button
                        {
                            if let Some(root_node) = Node::find_root_node(node_arc.clone())
                            {
                                node_arc = root_node;
                                use_root_node = true;
                            }
                        }

                        let id_string;
                        {
                            let node = node_arc.read().unwrap();

                            // select object itself if there is not instance on it
                            if node.instances.get_ref().len() == 1 || use_root_node
                            {
                                id_string = format!("objects_{}", node.id);
                            }
                            else
                            {
                                id_string = format!("objects_{}_{}", node.id, hit.instance_id);
                            }
                        }

                        let mut already_selected = false;
                        if self.editor_state.selected_object == id_string && self.editor_state.selected_scene_id == Some(scene_id)
                        {
                            already_selected = true;
                        }

                        // de-select first
                        self.editor_state.de_select_current_item(state);

                        // highlight
                        if !already_selected
                        {
                            self.editor_state.selected_object = id_string;
                            self.editor_state.selected_scene_id = Some(scene_id);
                            self.editor_state.selected_type = SelectionType::Object;

                            if self.editor_state.settings != SettingsPanel::Object && self.editor_state.settings != SettingsPanel::Components
                            {
                                self.editor_state.settings = SettingsPanel::Object;
                            }

                            if right_mouse_button
                            {
                                let node = node_arc.read().unwrap();

                                if let Some(instance) = node.find_instance_by_id(hit.instance_id)
                                {
                                    let mut instance = instance.write().unwrap();
                                    let instance_data = instance.get_data_mut().get_mut();
                                    instance_data.highlight = true;
                                }
                            }
                            else if left_mouse_button
                            {
                                let mut all_nodes = vec![];
                                all_nodes.push(node_arc.clone());
                                all_nodes.extend(Scene::list_all_child_nodes(&node_arc.read().unwrap().nodes));

                                for node in all_nodes
                                {
                                    let node = node.read().unwrap();

                                    for instance in node.instances.get_ref()
                                    {
                                        let mut instance = instance.write().unwrap();
                                        let instance_data = instance.get_data_mut().get_mut();
                                        instance_data.highlight = true;
                                    }
                                }
                            }
                        }
                    }
                }
                else
                {
                    self.editor_state.de_select_current_item(state);
                }

                self.editor_state.pick_mode = PickType::None;
            }
        }
    }

    pub fn delete_objct(&mut self, state: &mut State)
    {
        if !self.editor_state.selected_object.is_empty()
        {
            //if state.input_manager.keyboard.is_pressed(Key::X) || state.input_manager.keyboard.is_pressed(Key::Delete)
            if state.input_manager.keyboard.is_pressed(Key::Delete) || state.input_manager.keyboard.is_pressed(Key::Backspace)
            {
                // object
                if self.editor_state.selected_type == SelectionType::Object
                {
                    if let (Some(scene), Some(node), instance_id) = self.editor_state.get_selected_node(state)
                    {
                        let instances_amount = node.read().unwrap().instances.get_ref().len();

                        //scene.delete_node_by_id(id)
                        if instance_id.is_some() && instances_amount > 1
                        {
                            let instance_id = instance_id.unwrap();
                            node.write().unwrap().delete_instance_by_id(instance_id);
                        }
                        else
                        {
                            let id = node.read().unwrap().id;
                            scene.delete_node_by_id(id);
                        }

                        self.editor_state.de_select_current_item(state);
                    }
                }

                // camera
                if self.editor_state.selected_type == SelectionType::Camera
                {
                    let (camera_id, _) = self.editor_state.get_object_ids();
                    let scene = self.editor_state.get_selected_scene(state);
                    if let (Some(camera_id), Some(scene)) = (camera_id, scene)
                    {
                        scene.delete_camera_by_id(camera_id);
                    }
                }

                // light
                if self.editor_state.selected_type == SelectionType::Light
                {
                    let (light_id, _) = self.editor_state.get_object_ids();
                    let scene = self.editor_state.get_selected_scene(state);
                    if let (Some(light_id), Some(scene)) = (light_id, scene)
                    {
                        scene.delete_light_by_id(light_id);
                    }
                }

                // material
                if self.editor_state.selected_type == SelectionType::Material
                {
                    let (material_id, _) = self.editor_state.get_object_ids();
                    let scene = self.editor_state.get_selected_scene(state);
                    if let (Some(material_id), Some(scene)) = (material_id, scene)
                    {
                        scene.delete_material_by_id(material_id);
                    }
                }

                // texture
                if self.editor_state.selected_type == SelectionType::Texture
                {
                    let (texture_id, _) = self.editor_state.get_object_ids();
                    let scene = self.editor_state.get_selected_scene(state);
                    if let (Some(texture_id), Some(scene)) = (texture_id, scene)
                    {
                        scene.delete_texture_by_id(texture_id);
                    }
                }
            }
        }
    }

    pub fn pick(&self, state: &State, pos: Point2::<f32>, allow_grid_picking: bool, predicate: Option<PickPredicate>) -> Option<(u64, ScenePickRes)>
    {
        let scenes = &state.scenes;
        let width = state.width;
        let height = state.height;

        let mut hit: Option<ScenePickRes> = None;
        let mut scene_id: u64 = 0;

        for scene in scenes
        {
            let set_grid_picking = |scene: &Box<Scene>, state: bool|
            {
                // find grid
                let grid = scene.find_mesh_node_by_name("grid");

                if let Some(grid) = grid
                {
                    let mut grid = grid.write().unwrap();
                    let grid_instance = grid.instances.get_mut().first();
                    if let Some(grid_instance) = grid_instance
                    {
                        grid_instance.write().unwrap().pickable = state;
                    }
                }
            };

            /*
            if allow_grid_picking
            {
                set_grid_picking(scene, true);
            }
            */

            for camera in &scene.cameras
            {
                // check if click is insight
                if camera.is_point_in_viewport(&pos)
                {
                    let ray = camera.get_ray_from_viewport_coordinates(&pos, width, height);

                    let mut grid_hit = None;
                    if allow_grid_picking
                    {
                        let grid = scene.find_mesh_node_by_name("grid");
                        if let Some(grid) = grid
                        {
                            set_grid_picking(scene, true);
                            grid_hit = scene.pick_node(grid, &ray, false, true, predicate.clone());
                            set_grid_picking(scene, false);
                        }
                    }

                    let scene_hit = scene.pick(&ray, false, false, predicate.clone());

                    //dbg!(scene_hit.is_some());
                    //dbg!(grid_hit.is_some());

                    // check if grid hit is closer or scene hit
                    let mut new_hit = grid_hit;
                    if let Some(scene_hit_ref) = scene_hit.as_ref()
                    {
                        if let Some(new_hit_ref) = new_hit.as_ref()
                        {
                            if scene_hit_ref.time_of_impact < new_hit_ref.time_of_impact
                            {
                                new_hit = scene_hit;
                            }
                        }
                        else
                        {
                            new_hit = scene_hit;
                        }
                    }

                    //dbg!(new_hit.is_some());

                    let mut save_hit = false;

                    if let Some(new_hit) = new_hit.as_ref()
                    {
                        if let Some(hit) = hit.as_ref()
                        {
                            // check if the new hit is near
                            if new_hit.time_of_impact < hit.time_of_impact
                            {
                                save_hit = true;
                            }
                        }
                        else
                        {
                            save_hit = true;
                        }
                    }

                    if save_hit
                    {
                        hit = new_hit;
                        scene_id = scene_id;
                    }
                }
            }

            /*
            if allow_grid_picking
            {
                set_grid_picking(scene, false);
            }
            */
        }

        if let Some(hit) = hit
        {
            return Some((scene_id, hit));
        }

        None
    }

    pub fn pick_node(&self, state: &State, node: NodeItem, pos: Point2::<f32>) -> Option<(u64, ScenePickRes)>
    {
        let scenes = &state.scenes;
        let width = state.width;
        let height = state.height;

        for scene in scenes
        {
            for camera in &scene.cameras
            {
                // check if click is insight
                if camera.is_point_in_viewport(&pos)
                {
                    let ray = camera.get_ray_from_viewport_coordinates(&pos, width, height);
                    let hit = scene.pick_node(node.clone(), &ray, false, false, None);

                    if let Some(hit) = hit
                    {
                        return Some((scene.id, hit));
                    }
                }
            }
        }

        None
    }

    pub fn apply_asset_drag(&mut self, state: &mut State, ctx: &egui::Context)
    {
        if let Some(drag_id) = &self.editor_state.drag_id
        {
            let is_being_dragged = ctx.memory(|mem| { mem.is_anything_being_dragged() });

            if !is_being_dragged
            {
                if !ctx.wants_pointer_input()
                {
                    let pos = ctx.input(|i| i.pointer.latest_pos());

                    if let Some(pos) = pos
                    {
                        let pos = Vector2::<f32>::new(pos.x * state.scale_factor, pos.y * state.scale_factor);

                        if pos.x >= 0.0 && pos.y >= 0.0 && pos.x < state.width as f32 && pos.y <= state.height as f32
                        {
                            self.load_asset(state, drag_id.clone(), Point2::<f32>::new(pos.x, state.height as f32 - pos.y));
                        }
                    }
                }

                self.editor_state.drag_id = None;
            }
        }
    }

    pub fn apply_fly_camera_move_state(scene: &mut Scene, state: bool)
    {
        for camera in &mut scene.cameras
        {
            if !camera.enabled
            {
                continue;
            }

            if let Some(controller) = &mut camera.controller
            {
                if let Some(controller) = controller.as_any_mut().downcast_mut::<FlyController>()
                {
                    controller.mouse_movement = state;
                }
            }
        }
    }

    pub fn move_object(&mut self, state: &mut State)
    {
        let width = state.width;
        let height = state.height;

        let step_size = 1.0;
        let angle_steps = PI / 8.0;
        let factor = 0.01;

        let grid_size = self.editor_state.grid_size;


        // ********** enable movement if nothing is selected **********
        if self.editor_state.selected_object.is_empty() || self.editor_state.selected_type != SelectionType::Object || state.input_manager.mouse.point.pos.is_none()
        {
            self.editor_state.edit_moving = false;

            // re-allow fly camera state
            for scene in &mut state.scenes
            {
                Self::apply_fly_camera_move_state(scene, true);
            }

            return;
        }


        let pos = state.input_manager.mouse.point.pos;
        if pos.is_none()
        {
            return;
        }

        //let pos = pos.unwrap();
        //let old_pos = pos - state.input_manager.mouse.point.velocity;

        let pos_new = pos.unwrap();
        let pos = pos_new - state.input_manager.mouse.point.velocity;
        let pos_delta = state.input_manager.mouse.point.velocity;




        // ********** get selection **********
        let mut selected_scene_id = None;
        let mut selected_node = None;
        {
            let (scene, node, _) = self.editor_state.get_selected_node(state);

            if scene.is_none() || node.is_none()
            {
                return;
            }

            selected_scene_id = Some(scene.unwrap().id);
            selected_node = Some(node.unwrap().clone());
        }


        let selected_scene_id = selected_scene_id.unwrap();
        let selected_node = selected_node.unwrap();


        // ********** check locked **********
        if selected_node.read().unwrap().is_locked()
        {
            return;
        }


        // ********** check that first interaction (after selection) was on the selected object **********
        let engine_frame = state.stats.frame;

        if !self.editor_state.edit_moving && (state.input_manager.mouse.is_first_action(MouseButton::Left, engine_frame) || state.input_manager.mouse.is_first_action(MouseButton::Right, engine_frame))
        {
            let pick_res = self.pick(state, pos, false, None);

            if let Some(pick_res) = pick_res
            {
                let scene_id = pick_res.0;
                let node = &pick_res.1.node.read().unwrap();

                let has_currect_parent = node.has_parent_or_is_equal(selected_node.clone());

                if selected_scene_id == scene_id && has_currect_parent
                {
                    self.editor_state.edit_moving = true;
                }
            }
        }

        else if self.editor_state.edit_moving && !state.input_manager.mouse.is_holding(MouseButton::Left) && !state.input_manager.mouse.is_holding(MouseButton::Right)
        {
            self.editor_state.edit_moving = false;
        }

        if !self.editor_state.edit_moving
        {
            let (scene, _, _) = self.editor_state.get_selected_node(state);
            Self::apply_fly_camera_move_state(scene.unwrap(), true);

            return;
        }


        // ********** check mouse movement **********
        if !state.input_manager.mouse.has_velocity()
        {
            return;
        }


        // ********** disable camera movement **********
        let instance_id;
        {
            let (scene, node, instance) = self.editor_state.get_selected_node(state);
            let scene = scene.unwrap();

            // stop fly camera from moving
            Self::apply_fly_camera_move_state(scene, false);

            instance_id = instance;
        }


        // ********** get pick info **********
        //let mut object_pos = None;
        let mut pick_pos = None;
        let mut pick_distance = None;
        let mut bounding_min = None;
        let mut bounding_center = None;

        {
            // ***** map the mouse pos to the bottom center of the object *****
            {
                let selected_node = selected_node.read().unwrap();
                let bounding_info = selected_node.get_world_bounding_info(instance_id, true, None);
                if let Some((min, max)) = bounding_info
                {
                    bounding_min = Some(min);
                    bounding_center = Some(min + (max - min) / 2.0);
                }
            }

            // ***** pick info without node itself *****
            let selected_node_clone = selected_node.clone();
            let pick_predicate = move |node: NodeItem, check_instance_id: Option<u64>| -> bool
            {
                let node = node.read().unwrap();
                let has_currect_parent = node.has_parent_or_is_equal(selected_node_clone.clone());

                if let Some(instance_id) = instance_id
                {
                    if let Some(check_instance_id) = check_instance_id
                    {
                        return instance_id != check_instance_id;
                    }
                }
                else
                {
                    return !has_currect_parent;
                }

                true
            };

            if bounding_center.is_none() { return; }
            let bounding_center = bounding_center.unwrap();
            let bounding_min = bounding_min.unwrap();

            let bottom_center = Point3::<f32>::new(bounding_center.x, bounding_min.y, bounding_center.z);

            let mut bottom_center_screen_space = None;

            let (scene, node, instance) = self.editor_state.get_selected_node(state);
            let scene = scene.unwrap();
            for camera in &scene.cameras
            {
                // check if click is insight
                if camera.is_point_in_viewport(&pos_new)
                {
                    bottom_center_screen_space = Some(camera.get_viewport_coordinates_from_point(&bottom_center, width, height));

                    break;
                }
            }

            if bottom_center_screen_space.is_none() { return; }
            let bottom_center_screen_space = bottom_center_screen_space.unwrap();
            let new_bottom_center = bottom_center_screen_space + pos_delta;

            //dbg!(bottom_center_screen_space);
            //dbg!(new_bottom_center);

            //let pick_res: Option<(u64, ScenePickRes)> = self.pick(state, pos, true, Some(Arc::new(pick_predicate)));
            //let pick_res: Option<(u64, ScenePickRes)> = self.pick(state, pos, true, None);
            let pick_res: Option<(u64, ScenePickRes)> = self.pick(state, new_bottom_center, true, Some(Arc::new(pick_predicate)));
            //let pick_res: Option<(u64, ScenePickRes)> = self.pick(state, bottom_center_screen_space, true, Some(Arc::new(pick_predicate)));
            if let Some(pick_res) = pick_res
            {
                let node = pick_res.1.node.read().unwrap();
                //dbg!(&node.name);

                pick_pos = Some(pick_res.1.point);
                pick_distance = Some(pick_res.1.time_of_impact);
            }


            // ***** pick on node *****
            //let pick_node_res = self.pick_node(state, selected_node.clone(), pos);

            // ***** bounding info *****
            /*
            let selected_node = selected_node.read().unwrap();
            let bounding_info = selected_node.get_world_bounding_info(instance_id, true, None);

            if let Some((min, max)) = bounding_info
            {
                bounding_min = Some(min);
                bounding_center = Some(min + (max - min) / 2.0);
            }
             */
        }

        if pick_pos.is_none() || pick_distance.is_none() || bounding_min.is_none()
        {
            return;
        }

        let pick_pos = pick_pos.unwrap();
        let pick_distance = pick_distance.unwrap();
        let bounding_min = bounding_min.unwrap();
        let bounding_center = bounding_center.unwrap();

        let bottom_center = Point3::<f32>::new(bounding_center.x, bounding_min.y, bounding_center.z);

        let bottom_offset = bottom_center - bounding_center;
        //let delta = (pick_pos - bottom_center) - bottom_offset;
        let delta = pick_pos - bottom_center;

        /*
        let new_pos = pick_pos;
        let new_pos = Vector3::<f32>::new(new_pos.x, new_pos.y, new_pos.z);

        dbg!(pick_pos);
        dbg!(bottom_offset);
        dbg!(new_pos);
         */


        /*
        let p_down = Point3::<f32>::new(pick_pos.x, bounding_min.y, pick_pos.z);
        //let ray_down = p_down - pick_pos;
        let ray_down = pick_pos - p_down;


        let (scene, node, instance_id) = self.editor_state.get_selected_node(state);
        let scene = scene.unwrap();

        let mut new_cam_pick_ray = None;
        for camera in &scene.cameras
        {
            // check if click is insight
            if camera.is_point_in_viewport(&pos_new)
            {
                new_cam_pick_ray = Some(camera.get_ray_from_viewport_coordinates(&pos_new, width, height));

                break;
            }
        }

        if new_cam_pick_ray.is_none()
        {
            return;
        }

        let new_cam_pick_ray = new_cam_pick_ray.unwrap();

        let origin = new_cam_pick_ray.origin;
        let dir = new_cam_pick_ray.dir.normalize();

        let new_pos = origin + (dir * pick_distance);

        let delta = new_pos - pick_pos;

        let pick_offset = pick_pos - bounding_center;
        let result_pos = new_pos + pick_offset;
        let result_pos = Vector3::<f32>::new(result_pos.x, result_pos.y, result_pos.z);
         */


        /*
        let bottom_point = origin + (dir * pick_distance) + ray_down;
        let bottom_offset = bounding_min - bounding_center;


        let delta = bottom_point - pick_pos;

        let new_pos = bottom_point - bottom_offset;
        let new_pos = Vector3::<f32>::new(new_pos.x, new_pos.y, new_pos.z);

         */

        //dbg!(delta);

        /*
        let object_pos = object_pos.unwrap();
        let pick_pos = pick_pos.unwrap();

        let delta = pick_pos - object_pos;
        */

        //dbg!(delta);

        // TODO map to "local" object coordinates


        let (scene, node, instance_id) = self.editor_state.get_selected_node(state);
        let scene = scene.unwrap();


        // ********** find transform component for node/instance **********
        let edit_transformation: ComponentItem;
        let node_transform;
        let mut instance_transform = None;
        let instances_amount;

        {
            let node = selected_node.read().unwrap();
            instances_amount = node.instances.get_ref().len();
            node_transform = node.find_component::<Transformation>();
        }

        if let Some(instance_id) = instance_id
        {
            let node = selected_node.read().unwrap();
            let instance = node.find_instance_by_id(instance_id).unwrap();

            let instance = instance.write().unwrap();
            instance_transform = instance.find_component::<Transformation>();
        }

        // if there are multiple instances in the node -> use instance transform
        if instances_amount > 1 && instance_id.is_some()
        {
            if let Some(instance_transform) = instance_transform
            {
                edit_transformation = instance_transform.clone();
            }
            else
            {
                let node = selected_node.read().unwrap();
                let instance = node.find_instance_by_id(instance_id.unwrap()).unwrap() ;
                let mut instance = instance.write().unwrap();
                let id = scene.id_manager.write().unwrap().get_next_component_id();

                instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id, "Transformation")))));

                let transformation = instance.find_component::<Transformation>().unwrap();
                edit_transformation = transformation.clone();
            }
        }
        // if there is no node and instance transform -> use node transform
        else if instance_transform.is_none() && node_transform.is_none()
        {
            let mut node = selected_node.write().unwrap();
            let id = scene.id_manager.write().unwrap().get_next_component_id();
            node.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id, "Transformation")))));

            let transformation = node.find_component::<Transformation>().unwrap();
            edit_transformation = transformation.clone();
        }
        // if there is already a transform on the instance -> use it
        else if let Some(instance_transform) = instance_transform
        {
            edit_transformation = instance_transform.clone();
        }
        // otherwise use node transform
        else
        {
            let node_transform = node_transform.unwrap();
            edit_transformation = node_transform.clone();
        }

        {
            component_downcast_mut!(edit_transformation, Transformation);

            //let old_pos = edit_transformation.get_data().position;


            edit_transformation.apply_translation(delta);
            //edit_transformation.set_translation(new_pos);
            //edit_transformation.set_translation(result_pos);
        }

        // save not snapped position (simply in metadata/extras)
        {
            component_downcast_mut!(edit_transformation, Transformation);

            if !edit_transformation.get_extras().contains("pos_x")
            {
                let pos_x = edit_transformation.get_data().position.x;
                let pos_y = edit_transformation.get_data().position.y;
                let pos_z = edit_transformation.get_data().position.z;

                edit_transformation.get_extras_mut().insert("pos_x", pos_x);
                edit_transformation.get_extras_mut().insert("pos_y", pos_y);
                edit_transformation.get_extras_mut().insert("pos_z", pos_z);
            }
            else
            {
                let mut pos_x: f32 = *edit_transformation.get_extras().get::<f32>("pos_x").unwrap();
                let mut pos_y: f32 = *edit_transformation.get_extras().get::<f32>("pos_y").unwrap();
                let mut pos_z: f32 = *edit_transformation.get_extras().get::<f32>("pos_z").unwrap();

                pos_x += delta.x;
                pos_y += delta.y;
                pos_z += delta.z;

                edit_transformation.get_extras_mut().insert("pos_x", pos_x);
                edit_transformation.get_extras_mut().insert("pos_y", pos_y);
                edit_transformation.get_extras_mut().insert("pos_z", pos_z);
            }
        }

        // snap to grid
        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
        {
            component_downcast_mut!(edit_transformation, Transformation);

            //let mut pos = edit_transformation.get_data().position;
            let mut pos_x = *edit_transformation.get_extras().get::<f32>("pos_x").unwrap();
            let pos_y = *edit_transformation.get_extras().get::<f32>("pos_y").unwrap();
            let mut pos_z = *edit_transformation.get_extras().get::<f32>("pos_z").unwrap();

            pos_x = snap_to_grid(pos_x, grid_size);
            pos_z = snap_to_grid(pos_z, grid_size);

            let pos = Vector3::<f32>::new(pos_x, pos_y, pos_z);

            edit_transformation.set_translation(pos);
        }

        // bottom left snapping
        if state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        {
            let bounding_info = selected_node.read().unwrap().get_world_bounding_info(instance_id, true, None);
            if let Some(bounding_info) = bounding_info
            {
                component_downcast_mut!(edit_transformation, Transformation);

                let center = bounding_info.0 + (bounding_info.1 - bounding_info.0) / 2.0;
                let delta_x = center.x - bounding_info.0.x;
                let delta_z = center.z - bounding_info.0.z;

                let mut pos_x = *edit_transformation.get_extras().get::<f32>("pos_x").unwrap() - delta_x;
                let pos_y = *edit_transformation.get_extras().get::<f32>("pos_y").unwrap();
                let mut pos_z = *edit_transformation.get_extras().get::<f32>("pos_z").unwrap() - delta_z;
    
                pos_x = snap_to_grid(pos_x, grid_size);
                pos_z = snap_to_grid(pos_z, grid_size);

                let pos = Vector3::<f32>::new(pos_x + delta_x, pos_y, pos_z + delta_z);

                edit_transformation.set_translation(pos);
            }
        }

        //let delta = edit_transformation.get_data().position




        /*
        // ********** mode change **********
        if state.input_manager.keyboard.is_pressed(Key::G)
        {
            let start_pos = state.input_manager.mouse.point.pos.unwrap();
            self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, false, true));
        }
        if state.input_manager.keyboard.is_pressed(Key::R)
        {
            let start_pos = state.input_manager.mouse.point.pos.unwrap();
            self.editor_state.edit_mode = Some(EditMode::Rotate(start_pos, false, true, false));
        }

        if self.editor_state.edit_mode.is_some() && state.input_manager.mouse.is_pressed(MouseButton::Left)
        {
            self.editor_state.edit_mode = None;
        }

        if self.editor_state.edit_mode.is_some()
        {
            let moving;
            let start_pos;
            match self.editor_state.edit_mode.as_ref().unwrap()
            {
                EditMode::Movement(pos, _, _, _) => { moving = true; start_pos = pos.clone(); },
                EditMode::Rotate(pos, _, _, _) => { moving = false; start_pos = pos.clone(); },
            }

            if state.input_manager.keyboard.is_pressed(Key::X)
            {
                if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos.clone(), true, false, false)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos.clone(), true, false, false)); }
                }
                else
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, false, true, true)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos, false, true, true)); }
                }
            }

            if state.input_manager.keyboard.is_pressed(Key::Y)
            {
                if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, false, true, false)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos, false, true, false)); }
                }
                else
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, false, true)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos, true, false, true)); }
                }
            }

            if state.input_manager.keyboard.is_pressed(Key::Z)
            {
                if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, false, false, true)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos, false, false, true)); }
                }
                else
                {
                    if moving { self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, true, false)); }
                    else      { self.editor_state.edit_mode = Some(EditMode::Rotate  (start_pos, true, true, false)); }
                }
            }

            let edit_mode = self.editor_state.edit_mode.unwrap();

            dbg!(start_pos);
            */

            /*
            let mouse_pos = state.input_manager.mouse.point.pos.unwrap();
            let movement = (mouse_pos - start_pos) * factor;
            let mut movement = Vector3::<f32>::new(movement.x, 0.0, movement.y);

            dbg!(&mouse_pos);


            // get camera transform
            // TODO: if based on multiple cameras -> pick the correct one (check view port and mouse coordinates)
            let mut cam_inverse = Matrix4::<f32>::identity();
            for camera in &scene.cameras
            {
                if camera.enabled
                {
                    let cam_data = camera.get_data();
                    cam_inverse = cam_data.view_inverse.clone();
                    break;
                }
            }

            // transform by inverse camera matrix
            movement = (cam_inverse * movement.to_homogeneous()).xyz();

            let edit_transformation: ComponentItem;
            let node_transform;
            let mut instance_transform = None;
            let instances_amount;

            {
                let node = node.read().unwrap();
                instances_amount = node.instances.get_ref().len();
                node_transform = node.find_component::<Transformation>();
            }

            if let Some(instance_id) = instance_id
            {
                let node = node.read().unwrap();
                let instance = node.find_instance_by_id(instance_id).unwrap() ;

                let instance = instance.write().unwrap();
                instance_transform = instance.find_component::<Transformation>();
            }

            // if there are multiple instances in the node -> use instance transform
            if instances_amount > 1 && instance_id.is_some()
            {
                if let Some(instance_transform) = instance_transform
                {
                    edit_transformation = instance_transform.clone();
                }
                else
                {
                    let node = node.read().unwrap();
                    let instance = node.find_instance_by_id(instance_id.unwrap()).unwrap() ;
                    let mut instance = instance.write().unwrap();
                    let id = scene.id_manager.write().unwrap().get_next_component_id();

                    instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id, "Transformation")))));

                    let transformation = node.find_component::<Transformation>().unwrap();
                    edit_transformation = transformation.clone();
                }
            }
            // if there is no node and instance transform -> use node transform
            else if instance_transform.is_none() && node_transform.is_none()
            {
                let mut node = node.write().unwrap();
                let id = scene.id_manager.write().unwrap().get_next_component_id();
                node.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(id, "Transformation")))));

                let transformation = node.find_component::<Transformation>().unwrap();
                edit_transformation = transformation.clone();
            }
            // if there is already a transform on the instance -> use it
            else if let Some(instance_transform) = instance_transform
            {
                edit_transformation = instance_transform.clone();
            }
            // otherwise use node transform
            else
            {
                let node_transform = node_transform.unwrap();
                edit_transformation = node_transform.clone();
            }

            component_downcast_mut!(edit_transformation, Transformation);

            match edit_mode
            {
                EditMode::Movement(_, x, y, z) =>
                {
                    let mut applied = false;

                    let mut vec = Vector3::<f32>::zeros();
                    if x
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = movement.x.signum();
                            if movement.x.abs() >= step_size
                            {
                                vec.x = step_size * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.x = movement.x;
                            applied = true;
                        }
                    }

                    if y
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = movement.z.signum();
                            if movement.z.abs() >= step_size
                            {
                                vec.y = -step_size * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.y = -movement.z;
                            applied = true;
                        }
                    }

                    if z
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = -movement.z.signum();
                            if movement.z.abs() >= step_size
                            {
                                vec.z = -step_size * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.z = -movement.z;
                            applied = true;
                        }
                    }

                    if applied
                    {
                        edit_transformation.apply_translation(vec);
                    }

                    if applied
                    {
                        self.editor_state.edit_mode = Some(EditMode::Movement(mouse_pos, x, y, z));
                    }
                },
                EditMode::Rotate(_, x, y, z) =>
                {
                    let mut applied = false;

                    let mut vec = Vector3::<f32>::zeros();
                    if x
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = movement.z.signum();
                            if movement.z.abs() >= angle_steps
                            {
                                vec.x = angle_steps * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.x = movement.z;
                            applied = true;
                        }
                    }

                    if y
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = movement.x.signum();
                            if movement.x.abs() >= angle_steps
                            {
                                vec.y = angle_steps * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.y = movement.x;
                            applied = true;
                        }
                    }

                    if z
                    {
                        if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                        {
                            let sign = movement.x.signum();
                            if movement.x.abs() >= angle_steps
                            {
                                vec.z = -angle_steps * sign;
                                applied = true;
                            }
                        }
                        else
                        {
                            vec.z = -movement.x;
                            applied = true;
                        }
                    }

                    if applied
                    {
                        edit_transformation.apply_rotation(vec);
                    }

                    if applied
                    {
                        self.editor_state.edit_mode = Some(EditMode::Rotate(mouse_pos, x, y, z));
                    }
                },
            }
        }
        */
    }

    pub fn load_asset(&mut self, state: &mut State, path: String, pos: Point2::<f32>)
    {
        let main_queue = state.main_thread_execution_queue.clone();

        let mut scene_id = None;
        let mut id_manager = None;
        for scene in &mut state.scenes
        {
            scene_id = Some(scene.id);
            id_manager = Some(scene.id_manager.clone());

            if self.editor_state.asset_type == AssetType::Scene
            {
                scene.clear();
            }

            break;
        }

        if scene_id.is_none()
        {
            return;
        }

        let scene_id = scene_id.unwrap();
        let id_manager = id_manager.unwrap();

        let grid_size = self.editor_state.grid_size;
        let grid_amount = self.editor_state.grid_amount;

        let main_queue_clone = main_queue.clone();
        let id_manager_clone = id_manager.clone();
        if self.editor_state.asset_type == AssetType::Scene
        {
            spawn_thread(move ||
            {
                scene_utils::create_grid(scene_id, main_queue_clone.clone(), id_manager_clone.clone(), grid_amount, grid_size);
            });
        };

        // pick
        let pick_res = self.pick(state, pos, true, None);

        let mut pos = None;
        if let Some(pick_res) = pick_res
        {
            pos = Some(pick_res.1.point);
        }

        let create_mipmaps = state.rendering.create_mipmaps;
        let max_tex_res = state.max_texture_resolution();
        let object_only = if self.editor_state.asset_type == AssetType::Object { true } else { false };
        let reuse_materials = if self.editor_state.asset_type == AssetType::Object && self.editor_state.reuse_materials_by_name  { true } else { false };

        let editor_state = self.editor_state.loading.clone();
        spawn_thread(move ||
        {
            dbg!("loading ...");
            *editor_state.write().unwrap() = true;

            let loaded = load_object(path.as_str(), scene_id, main_queue.clone(), id_manager.clone(), reuse_materials, object_only, create_mipmaps, max_tex_res);

            if loaded.is_err()
            {
                dbg!("loading failed");
                dbg!(loaded.err());
                *editor_state.write().unwrap() = false;
                return;
            }

            let loaded_ids = loaded.unwrap();

            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
            {
                //scene.clear_empty_nodes();

                if let Some(pos) = pos
                {
                    let mut root_node = None;
                    for id in &loaded_ids
                    {
                        if let Some(node) = scene.find_node_by_id(*id)
                        {
                            if node.read().unwrap().root_node
                            {
                                root_node = Some(node.clone());
                                break;
                            }
                        }
                    }

                    if let Some(root_node) = root_node
                    {
                        // find offset based on bounding box
                        let mut offset = 0.0;
                        {
                            let root_node = root_node.read().unwrap();
                            let bounding_info = root_node.get_world_bounding_info(None, true, None);

                            if let Some(bounding_info) = bounding_info
                            {
                                //offset = (bounding_info.1.y - bounding_info.0.y) / 2.0;
                                offset = -bounding_info.0.y;
                            }
                        }

                        let component_id = scene.id_manager.write().unwrap().get_next_component_id();

                        let mut transform = Transformation::identity(component_id, "Transform");
                        transform.apply_translation(Vector3::<f32>::new(pos.x, pos.y + offset, pos.z));

                        root_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));
                    }
                }

                if let Some(train) = scene.find_node_by_name("Train")
                {
                    let mut node = train.write().unwrap();
                    let id_1 = scene.id_manager.write().unwrap().get_next_component_id();
                    let id_2 = scene.id_manager.write().unwrap().get_next_component_id();

                    node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_1, "Left", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, -0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));
                    node.add_component(Arc::new(RwLock::new(Box::new(TransformationAnimation::new(id_2, "Right", Vector3::<f32>::zeros(), Vector3::<f32>::new(0.0, 0.04, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0))))));

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
                if scene.lights.get_ref().len() == 0
                {
                    let light_id = scene.id_manager.write().unwrap().get_next_light_id();
                    let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(0.0, 4.0, 4.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                    scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                }

                // add camera
                if scene.cameras.len() == 0
                {
                    let id = scene.id_manager.write().unwrap().get_next_camera_id();
                    let mut cam = Camera::new(id, "Cam".to_string());
                    let cam_data = cam.get_data_mut().get_mut();
                    cam_data.fovy = 45.0f32.to_radians();
                    cam_data.eye_pos = Point3::<f32>::new(0.0, 1.0, 1.5);
                    cam_data.dir = Vector3::<f32>::new(-cam_data.eye_pos.x, -cam_data.eye_pos.y, -cam_data.eye_pos.z);
                    cam_data.clipping_near = 0.1;
                    cam_data.clipping_far = 1000.0;
                    scene.cameras.push(Box::new(cam));
                }

                // camera movement controller
                if scene.cameras.len() > 0
                {
                    let cam = scene.cameras.get_mut(0).unwrap();
                    if cam.controller.is_none()
                    {
                        cam.add_controller_fly(true, Vector2::<f32>::new(0.0015, 0.0015), 0.1, 0.2);
                    }

                    //let mouse_sensivity = if platform::is_mac() { 0.1 } else { 0.01 };
                    //cam.add_controller_target_rotation(3.0, Vector2::<f32>::new(0.0015, 0.0015), mouse_sensivity);
                    //cam.controller.as_mut().unwrap().as_any_mut().downcast_mut::<TargetRotationController>().unwrap().auto_rotate = Some(0.005);
                }
            }));

            *editor_state.write().unwrap() = false;

            dbg!("loading DONE");
        });
    }
}
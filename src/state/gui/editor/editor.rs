#![allow(dead_code)]

use std::{cell::RefCell, f32::consts::PI, sync::{Arc, RwLock}};

use egui::FullOutput;

use nalgebra::{Matrix4, Point2, Point3, Vector2, Vector3};

use crate::{component_downcast, component_downcast_mut, helper::{change_tracker::ChangeTracker, concurrency::thread::spawn_thread, math::{self, snap_to_grid}}, input::{keyboard::{Key, Modifier}, mouse::MouseButton}, rendering::egui::EGui, state::{gui::editor::helper::transform_vec_to_parent_local, scene::{camera::Camera, components::{mesh::Mesh, transformation::Transformation}, light::Light, node::{Node, NodeItem}, scene::{Scene, ScenePickRes}, utilities::{scene_utils::{self, execute_on_scene_mut_and_wait, load_object}, tags}}, state::State}};

use self::math::approx_zero;

use super::{editor_state::{AssetType, EditMode, EditorState, PickType, SelectionType, SettingsPanel}, gizmo::{create_gizmo_objects, update_gizmos}, grid::{create_grid, update_grid}, helper::{apply_fly_camera_move_state, find_transform_component, pick}};
use crate::state::gui::editor::ui::main_frame;
use crate::state::scene::exporter::json;

pub const MAX_NAME_LENGTH: usize = 24;

pub const EDITOR_INTERNAL_TAG: &str = "__internal_editor";

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

    pub fn init(&mut self, state: &mut State, egui: &EGui)
    {
        self.editor_state.load_all_asset_entries(state, &egui.ctx);

        self.create_main_entities(state);
        self.create_util_objects(state);
    }

    pub fn create_main_entities(&mut self, state: &mut State)
    {
        let scene_id = state.get_main_scene_id();

        if scene_id.is_none()
        {
            println!("No main scene found");
            return;
        }
        let scene_id = scene_id.unwrap();

        let main_queue = state.main_thread_execution_queue.clone();
        let loading_state = self.editor_state.loading.clone();
        spawn_thread(move ||
        {
            *loading_state.write().unwrap() = true;

            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(|scene|
            {
                // light
                let mut light = Light::new_point("Point".to_string(), Point3::<f32>::new(2.0, 50.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                light.tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

                let hemi = scene.add_light_hemisperical("hemi", Vector3::<f32>::new(0.0, -1.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0), Vector3::<f32>::new(0.0, 0.0, 0.0), 1.0);
                hemi.borrow_mut().get_mut().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

                // add camera
                if scene.cameras.len() == 0
                {
                    let mut cam = Camera::new("Cam".to_string());
                    cam.tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);

                    cam.add_controller_fly(false, Vector2::<f32>::new(0.0015, 0.0015), 0.1, 0.2);

                    let cam_data = cam.get_data_mut().get_mut();
                    cam_data.fovy = 45.0f32.to_radians();
                    cam_data.eye_pos = Point3::<f32>::new(0.0, 5.0, 10.0);
                    cam_data.dir = Vector3::<f32>::new(-cam_data.eye_pos.x, -cam_data.eye_pos.y, -cam_data.eye_pos.z);
                    cam_data.clipping_near = 0.1;
                    cam_data.clipping_far = 1000.0;
                    // cam_data.viewport_x = 0.25;
                    // cam_data.viewport_y = 0.25;
                    // cam_data.viewport_width = 0.5;
                    // cam_data.viewport_height = 0.5;
                    scene.cameras.push(Box::new(cam));
                }
            }));

            *loading_state.write().unwrap() = false;
        });
    }

    pub fn create_util_objects(&mut self, state: &mut State)
    {
        // use or create new scene if needed
        let scene = if state.scenes.is_empty()
        {
            let mut scene = crate::state::scene::scene::Scene::new("main scene");
            scene.add_defaults();

            state.scenes.push(Box::new(scene));
            state.scenes.last_mut().unwrap()
        }
        else
        {
            state.scenes.get_mut(0).unwrap()
        };

        let editor_utils = scene.add_empty_node("editor utils", None);
        {
            editor_utils.write().unwrap().tags.insert_with_color_locked(EDITOR_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
        }

        let editor_utils_id = editor_utils.read().unwrap().id;

        create_gizmo_objects(&mut self.editor_state, state, editor_utils_id);
    }

    pub fn build_gui(&mut self, state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
    {
        let raw_input = egui.ui_state.take_egui_input(window);

        let full_output = egui.ctx.run(raw_input, |ctx|
        {
            main_frame::create_frame(ctx, &mut self.editor_state, state);
        });

        self.apply_internal_asset_drag(state, &egui.ctx);

        // stop text input when the user wants to move/navigate in 3d space
        if state.io.input_manager.mouse.is_any_button_holding()
        {
            egui.ctx.memory_mut(|mem| { mem.stop_text_input() });
        }

        let platform_output = full_output.platform_output.clone();

        //egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);
        egui.ui_state.handle_platform_output(window, platform_output);

        full_output
    }

    pub fn update(&mut self, state: &mut State)
    {
        // update modes
        self.update_modes(state);

        // update grid based on camera pos and key inputs
        update_grid(&mut self.editor_state, state);

        if !self.editor_state.try_mode
        {
            // key bindings (copy paste, instancing, ...)
            self.key_bindings(state);

            // set edit mode
            self.set_edit_mode(state);

            // update gizmos
            update_gizmos(&mut self.editor_state, state);

            // select/pick objects
            self.select_object(state);

            // delete objects
            self.delete_objct(state);

            // edit mode
            self.move_object(state);
        }
    }

    pub fn update_modes(&mut self, state: &mut State)
    {
        // start try out mde
        if !self.editor_state.try_mode && (state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)) && state.io.input_manager.keyboard.is_pressed(Key::R)
        {
            self.editor_state.set_try_mode(state, true);
        }

        // end try out mode
        if self.editor_state.try_mode && state.io.input_manager.keyboard.is_pressed(Key::Escape)
        {
            self.editor_state.set_try_mode(state, false);
        }

        // hide ui
        if state.io.input_manager.keyboard.is_pressed(Key::H)
        {
            self.editor_state.visible = !self.editor_state.visible;
        }

        // full screen
        if state.io.input_manager.keyboard.is_pressed(Key::F)
        {
            state.rendering.fullscreen.set(!*state.rendering.fullscreen.get_ref());
        }

        // escape
        if state.io.input_manager.keyboard.is_pressed(Key::Escape)
        {
            if self.editor_state.edit_mode.is_some()
            {
                self.editor_state.edit_mode = None;
            }

            self.editor_state.de_select_current_item(state);
        }
    }

    pub fn key_bindings(&mut self, state: &mut State)
    {
        if self.editor_state.try_mode
        {
            return;
        }

        // create instance
        if state.io.input_manager.keyboard.is_pressed(Key::I)
        {
            if self.editor_state.selected_type == SelectionType::Object
            {
                if let (Some(_scene), Some(node), _) = self.editor_state.get_selected_node(state)
                {
                    node.write().unwrap().create_default_instance(node.clone());
                }
            }
        }

        // copy paste
        self.copy_paste(state);

        // save project
        if state.io.input_manager.keyboard.is_holding(Key::S) && (state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo))
        {
            if state.io.input_manager.keyboard.is_pressed_no_wait(Key::S)
            {
                json::export(state, (("data/".to_string()) + &self.editor_state.project_name).as_str());
            }
        }
    }

    pub fn copy_paste(&mut self, state: &mut State)
    {
        // select copied node (if there is one)
        let copy_node_id = *self.editor_state.copy_node_id.read().unwrap();
        if let Some(copy_node_id) = copy_node_id
        {
            if let Some(scene_id) = self.editor_state.selected_scene_id
            {
                let scene = state.find_scene_by_id_mut(scene_id);
                if let Some(scene) = scene
                {
                    self.editor_state.set_selected_object(scene, copy_node_id, None, SelectionType::Object);
                }
            }

            *self.editor_state.copy_node_id.write().unwrap() = None;
        }

        if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
        {
            // copy
            if state.io.input_manager.keyboard.is_pressed_no_wait(Key::C)
            {
                let (scene, node, _) = self.editor_state.get_selected_node(state);

                if scene.is_none() || node.is_none()
                {
                    return;
                }

                let node = node.unwrap();
                let node = node.read().unwrap();

                if let Some(source) = &node.source
                {
                    self.editor_state.copy_asset = Some(source.origin_path.clone());
                    self.editor_state.copy_asset_transform = None;

                    if let Some(transform) = node.find_component::<Transformation>()
                    {
                        component_downcast!(transform, Transformation);
                        self.editor_state.copy_asset_transform = Some(transform.get_data().clone());
                    }
                }
            }

            // paste
            if state.io.input_manager.keyboard.is_pressed_no_wait(Key::V)
            {
                // do not paste while loading
                if *self.editor_state.loading.read().unwrap() { return; }

                if let Some(copy_asset) = &self.editor_state.copy_asset
                {
                    let pos = state.io.input_manager.mouse.point.pos.unwrap();

                    let copy_node_id = self.editor_state.copy_node_id.clone();
                    let copy_asset_transform = self.editor_state.copy_asset_transform.clone();

                    self.load_asset(state, copy_asset.clone(), Point2::<f32>::new(pos.x, pos.y), true, Some(Arc::new(move |_scene: &mut Scene, root_node: NodeItem|
                    {
                        // copy over transformation
                        if let Some(transform_data) = copy_asset_transform.clone()
                        {
                            if let Some(transformation) = root_node.read().unwrap().find_component::<Transformation>()
                            {
                                component_downcast_mut!(transformation, Transformation);
                                transformation.set_rotation(transform_data.rotation.clone());
                                transformation.set_scale(transform_data.scale.clone());

                                if let Some(rotation_quat) = transform_data.rotation_quat
                                {
                                    transformation.apply_rotation_quaternion(rotation_quat,true);
                                }
                            }
                        }

                        *copy_node_id.write().unwrap() = Some(root_node.read().unwrap().id);
                    })));
                }
            }
        }
        if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) && state.io.input_manager.keyboard.is_pressed_no_wait(Key::D)
        {
            // do not paste while loading
            if *self.editor_state.loading.read().unwrap() { return; }

            // duplicate selected object
            let (scene, node, _) = self.editor_state.get_selected_node(state);

            if scene.is_none() || node.is_none()
            {
                return;
            }

            let node = node.unwrap();
            let node = node.read().unwrap();

            if let Some(source) = &node.source
            {
                let pos = state.io.input_manager.mouse.point.pos.unwrap();

                let copy_node_id = self.editor_state.copy_node_id.clone();
                let copy_asset_transform = self.editor_state.copy_asset_transform.clone();

                self.load_asset(state, source.origin_path.clone(), Point2::<f32>::new(pos.x, pos.y), true, Some(Arc::new(move |_scene: &mut Scene, root_node: NodeItem|
                {
                    // copy over transformation
                    if let Some(transform_data) = copy_asset_transform.clone()
                    {
                        if let Some(transformation) = root_node.read().unwrap().find_component::<Transformation>()
                        {
                            component_downcast_mut!(transformation, Transformation);
                            transformation.set_rotation(transform_data.rotation.clone());
                            transformation.set_scale(transform_data.scale.clone());

                            if let Some(rotation_quat) = transform_data.rotation_quat
                            {
                                transformation.apply_rotation_quaternion(rotation_quat,true);
                            }
                        }
                    }

                    *copy_node_id.write().unwrap() = Some(root_node.read().unwrap().id);
                })));
            }
        }
    }

    pub fn select_object(&mut self, state: &mut State)
    {
        if self.editor_state.selected_gizmo.is_some()
        {
            return;
        }

        //if !self.editor_state.try_out && (self.editor_state.selectable || self.editor_state.pick_mode != PickType::None) && self.editor_state.edit_mode.is_none()
        if !self.editor_state.try_mode && (self.editor_state.selectable || self.editor_state.pick_mode != PickType::None)
        {
            let left_mouse_button = state.io.input_manager.mouse.clicked(MouseButton::Left);
            let right_mouse_button = state.io.input_manager.mouse.clicked(MouseButton::Right);
            let tapped = state.io.input_manager.touch.tapped_any().is_some();

            let mut pos = state.io.input_manager.mouse.point.pos;

            if let Some(touch_id) = state.io.input_manager.touch.tapped_any()
            {
                pos = state.io.input_manager.touch.get_touch_by_id(touch_id).unwrap().pos;
            }

            if left_mouse_button || right_mouse_button || tapped
            {
                let mut hit: Option<ScenePickRes> = None;
                let mut scene_id: u64 = 0;

                if let Some(pos) = pos
                {
                    let pick_res = pick(state, pos, false, false, false, None);

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
                            camera.set_node(hit.node.clone());
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
                    // animation re-targeting (copy)
                    else if self.editor_state.pick_mode == PickType::AnimationCopy && self.editor_state.selected_scene_id.is_some()
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
                            // find root
                            let mut hit_node = hit.node;
                            if let Some(root_node) = Node::find_root_node(hit_node.clone())
                            {
                                hit_node = root_node.clone();
                            }

                            let target_animation_node = Node::find_animation_node(hit_node.clone());
                            if let Some(target_animation_node) = target_animation_node
                            {
                                if node.read().unwrap().id != target_animation_node.read().unwrap().id
                                {
                                    self.editor_state.selected_object = format!("objects_{}", target_animation_node.read().unwrap().id);
                                    self.editor_state.settings = SettingsPanel::Components;

                                    scene_utils::clone_all_animations(node, target_animation_node);
                                }
                            }
                        }
                    }
                    // show selection
                    else
                    {
                        let mut node_arc = hit.node;
                        let mut use_root_node = false;

                        if left_mouse_button || tapped
                        {
                            if let Some(root_node) = Node::find_root_node(node_arc.clone())
                            {
                                node_arc = root_node;
                                use_root_node = true;
                            }
                        }

                        let scene = state.find_scene_by_id_mut(scene_id);
                        if scene.is_none() { return; }
                        let scene = scene.unwrap();

                        let node = node_arc.read().unwrap();

                        let instande_id = if right_mouse_button && !use_root_node { Some(hit.instance_id) } else { None };

                        if right_mouse_button || left_mouse_button || tapped
                        {
                            let selected = self.editor_state.set_selected_object(scene, node.id, instande_id, SelectionType::Object);

                            if selected
                            {
                                let start_pos = pos.unwrap();
                                self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, true, true));

                                if self.editor_state.settings != SettingsPanel::Object && self.editor_state.settings != SettingsPanel::Components
                                {
                                    self.editor_state.settings = SettingsPanel::Object;
                                }
                            }
                        }
                    }
                }
                else
                {
                    //self.editor_state.de_select_current_item(state);
                    EditorState::de_select_all_items(state, None);
                }

                self.editor_state.pick_mode = PickType::None;
            }
        }
    }

    pub fn delete_objct(&mut self, state: &mut State)
    {
        if !self.editor_state.selected_object.is_empty()
        {
            //if state.io.input_manager.keyboard.is_pressed(Key::X) || state.io.input_manager.keyboard.is_pressed(Key::Delete)
            if state.io.input_manager.keyboard.is_pressed(Key::Delete) || state.io.input_manager.keyboard.is_pressed(Key::Backspace)
            {
                // object
                if self.editor_state.selected_type == SelectionType::Object
                {
                    if let (Some(scene), Some(node), instance_id) = self.editor_state.get_selected_node(state)
                    {
                        let instances_amount = node.read().unwrap().instances.get_ref().len();
                        let has_mesh = node.read().unwrap().has_component::<Mesh>();

                        //scene.delete_node_by_id(id)
                        if instance_id.is_some() && (instances_amount > 1 || !has_mesh)
                        {
                            let instance_id = instance_id.unwrap();
                            node.write().unwrap().delete_instance_by_id(instance_id);
                        }
                        else
                        {
                            let id = node.read().unwrap().id;
                            scene.delete_node_by_id(id, true, true, true, true);
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
                    if let Some(texture_id) = texture_id
                    {
                        state.delete_texture_by_id(texture_id);
                    }
                }

                // sound source
                if self.editor_state.selected_type == SelectionType::SoundSource
                {
                    let (sound_source_id, _) = self.editor_state.get_object_ids();
                    if let Some(sound_source_id) = sound_source_id
                    {
                        state.delete_sound_source_by_id(sound_source_id);
                    }
                }
            }
        }
    }

    pub fn apply_internal_asset_drag(&mut self, state: &mut State, ctx: &egui::Context)
    {
        if let Some(drag_id) = &self.editor_state.drag_id
        {
            if ctx.dragged_id().is_none()
            {
                if !ctx.wants_pointer_input()
                {
                    let pos = ctx.input(|i| i.pointer.interact_pos());

                    if let Some(pos) = pos
                    {
                        let pos = Vector2::<f32>::new(pos.x * state.scale_factor, pos.y * state.scale_factor);
                        if pos.x >= 0.0 && pos.y >= 0.0 && pos.x < state.width as f32 && pos.y <= state.height as f32
                        {
                            let reuse_materials = if self.editor_state.asset_type == AssetType::Object && self.editor_state.reuse_materials_by_name  { true } else { false };
                            self.load_asset(state, drag_id.clone(), Point2::<f32>::new(pos.x, state.height as f32 - pos.y), reuse_materials, None);
                        }
                    }
                }

                self.editor_state.drag_id = None;
            }
        }
    }

    pub fn apply_external_asset_drag(&mut self, state: &mut State, path: String)
    {
        let pos = Point2::<f32>::new(state.width as f32 / 2.0, state.height as f32 / 2.0);
        let reuse_materials = false;
        self.load_asset(state, path, pos, reuse_materials, None);
    }

    pub fn set_edit_mode(&mut self, state: &mut State)
    {
        // if its in rotation mode -> just end rotation mode on left click
        if let Some(EditMode::Rotate(start_pos, _, _, _)) = self.editor_state.edit_mode
        {
            if state.io.input_manager.mouse.is_pressed(MouseButton::Left)
            {
                self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, true, true));
                return;
            }
        }

        // ********** mode change **********
        if state.io.input_manager.keyboard.is_pressed(Key::G)
        {
            let start_pos = state.io.input_manager.mouse.point.pos.unwrap();
            self.editor_state.edit_mode = Some(EditMode::Movement(start_pos, true, true, true));
        }
        if state.io.input_manager.keyboard.is_pressed(Key::R)
        {
            let start_pos = state.io.input_manager.mouse.point.pos.unwrap();
            self.editor_state.edit_mode = Some(EditMode::Rotate(start_pos, false, true, false));
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

            if state.io.input_manager.keyboard.is_pressed(Key::X)
            {
                if !state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
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

            if state.io.input_manager.keyboard.is_pressed(Key::Y)
            {
                if !state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
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

            if state.io.input_manager.keyboard.is_pressed(Key::Z)
            {
                if !state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
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
        }
    }

    pub fn move_object(&mut self, state: &mut State)
    {
        if self.editor_state.edit_mode.is_none()
        {
            return;
        }

        if self.editor_state.selected_gizmo.is_some()
        {
            return;
        }

        {
            let (scene, node, _) = self.editor_state.get_selected_node(state);

            if scene.is_none() || node.is_none()
            {
                return;
            }
        }

        // TODO: check
        let factor = 0.01;

        let edit_mode = self.editor_state.edit_mode.unwrap();

        let start_pos;
        match edit_mode
        {
            EditMode::Movement(pos, _, _, _) => { start_pos = pos.clone(); },
            EditMode::Rotate(pos, _, _, _) => { start_pos = pos.clone(); },
        }

        let pointer_pos = state.io.input_manager.get_pointer_input().pos;
        if pointer_pos.is_none()
        {
            return;
        }

        let pointer_pos = pointer_pos.unwrap();

        let movement = (pointer_pos - start_pos) * factor;
        let mut movement = Vector3::<f32>::new(movement.x, 0.0, movement.y);

        // get camera transform
        let (scene, _, _) = self.editor_state.get_selected_node(state);
        let mut cam_inverse = Matrix4::<f32>::identity();
        for camera in &scene.unwrap().cameras
        {
            if camera.enabled && camera.is_point_in_viewport(&start_pos)
            {
                let cam_data = camera.get_data();
                cam_inverse = cam_data.view_inverse.clone();
                break;
            }
        }

        // transform by inverse camera matrix
        movement = (cam_inverse * movement.to_homogeneous()).xyz();

        match edit_mode
        {
            EditMode::Movement(_, x, y, z) =>
            {
                self.drag_and_drop_object(state, x, y, z);
            },
            EditMode::Rotate(_, x, y, z) =>
            {
                if self.rotate_object(state, movement, x, y, z, true)
                {
                    self.editor_state.edit_mode = Some(EditMode::Rotate(pointer_pos, x, y, z));
                }
            },
        }

        // rotate with mouse wheel
        if !approx_zero(state.io.input_manager.mouse.wheel_delta_y)
        {
            let delta = state.io.input_manager.mouse.wheel_delta_y.signum() * PI / 16.0;
            let movement = Vector3::<f32>::new(delta, delta, delta);
            self.rotate_object(state, movement, false, true, false, false);

            // "consume" mouse wheel
            state.io.input_manager.mouse.wheel_delta_y = 0.0;
        }

    }

    pub fn rotate_object(&mut self, state: &mut State, movement: Vector3<f32>, apply_x: bool, apply_y: bool, apply_z: bool, movement_check: bool) -> bool
    {
        let angle_steps = PI / 8.0;

        let edit_transformation = find_transform_component(&mut self.editor_state, state);

        let mut use_rotation_vec = false;
        let mut rotation_vec = Vector3::<f32>::zeros();

        let mut use_rotation_pos = false;
        let mut rotation_pos = Vector3::<f32>::zeros();

        if apply_x
        {
            if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
            {
                if movement.z.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.z.signum();
                    rotation_vec.x = angle_steps * sign;

                    use_rotation_vec = true;
                }
            }
            else if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                component_downcast!(edit_transformation, Transformation);

                if movement.z.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.z.signum();
                    rotation_pos.x = edit_transformation.get_data().rotation.x + angle_steps * sign;
                    rotation_pos.x = snap_to_grid(rotation_pos.x, angle_steps);

                    use_rotation_pos = true;
                }
            }
            else
            {
                rotation_vec.x = movement.z;
                use_rotation_vec = true;
            }
        }

        if apply_y
        {
            if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
            {
                if movement.x.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.x.signum();
                    rotation_vec.y = angle_steps * sign;

                    use_rotation_vec = true;
                }
            }
            else if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                component_downcast!(edit_transformation, Transformation);

                if movement.x.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.x.signum();
                    rotation_pos.y = edit_transformation.get_data().rotation.y + angle_steps * sign;
                    rotation_pos.y = snap_to_grid(rotation_pos.y, angle_steps);

                    use_rotation_pos = true;
                }
            }
            else
            {
                rotation_vec.y = movement.x;
                use_rotation_vec = true;
            }
        }

        if apply_z
        {
            if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
            {
                if movement.x.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.x.signum();
                    rotation_vec.z = -angle_steps * sign;
                    use_rotation_vec = true;
                }
            }
            else if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                component_downcast!(edit_transformation, Transformation);

                if movement.x.abs() >= angle_steps || !movement_check
                {
                    let sign = movement.x.signum();
                    rotation_pos.z = edit_transformation.get_data().rotation.z + angle_steps * sign;
                    rotation_pos.z = snap_to_grid(rotation_pos.z, angle_steps);

                    use_rotation_pos = true;
                }
            }
            else
            {
                rotation_vec.z = -movement.x;
                use_rotation_vec = true;
            }
        }

        if use_rotation_vec
        {
            component_downcast_mut!(edit_transformation, Transformation);
            edit_transformation.apply_rotation(rotation_vec);
        }
        else if use_rotation_pos
        {
            component_downcast_mut!(edit_transformation, Transformation);
            edit_transformation.set_rotation(rotation_pos);
        }

        use_rotation_vec || use_rotation_pos
    }

    pub fn drag_and_drop_object(&mut self, state: &mut State, apply_x: bool, apply_y: bool, apply_z: bool)
    {
        let grid_size = self.editor_state.grid_size;

        // ********** enable movement if nothing is selected **********
        if self.editor_state.selected_object.is_empty() || self.editor_state.selected_type != SelectionType::Object || state.io.input_manager.mouse.point.pos.is_none()
        {
            self.editor_state.edit_moving = false;

            // re-allow fly camera state
            for scene in &mut state.scenes
            {
                apply_fly_camera_move_state(scene, true);
            }

            return;
        }

        let mut pointer_pos = state.io.input_manager.mouse.point.pos;
        let mut pointer_velocity = state.io.input_manager.mouse.point.velocity;

        if let Some(touch) = state.io.input_manager.touch.get_first_touch()
        {
            pointer_pos = touch.pos;
            pointer_velocity = touch.velocity;
        }

        if pointer_pos.is_none()
        {
            return;
        }

        let pos_new = pointer_pos.unwrap();
        let pos = pos_new - pointer_velocity;
        let pos_delta = pointer_velocity;

        // ********** get selection **********
        let selected_scene_id;
        let selected_node;
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

        if !self.editor_state.edit_moving && (state.io.input_manager.mouse.is_first_action(MouseButton::Left, engine_frame) || state.io.input_manager.mouse.is_first_action(MouseButton::Right, engine_frame) || state.io.input_manager.touch.is_first_action(engine_frame))
        {
            let pick_res = pick(state, pos, false, false, false, None);

            if let Some(pick_res) = pick_res
            {
                let scene_id = pick_res.0;
                let node = &pick_res.1.node.read().unwrap();

                let has_currect_parent = node.has_parent_or_is_equal(selected_node.clone());

                if selected_scene_id == scene_id && has_currect_parent
                {
                    self.editor_state.edit_moving = true;
                    self.editor_state.selected_object_position = None;
                }
            }
        }

        else if self.editor_state.edit_moving && !state.io.input_manager.mouse.is_holding(MouseButton::Left) && !state.io.input_manager.mouse.is_holding(MouseButton::Right) && !state.io.input_manager.touch.has_touches()
        {
            self.editor_state.edit_moving = false;
        }

        if !self.editor_state.edit_moving
        {
            let (scene, _, _) = self.editor_state.get_selected_node(state);
            apply_fly_camera_move_state(scene.unwrap(), true);

            return;
        }

        // ********** check mouse movement **********
        if math::approx_zero_vec2(&pointer_velocity)
        {
            return;
        }

        // ********** disable camera movement **********
        let instance_id;
        {
            let (scene, _, instance) = self.editor_state.get_selected_node(state);
            let scene = scene.unwrap();

            // stop fly camera from moving
            apply_fly_camera_move_state(scene, false);

            instance_id = instance;
        }

        // ********** find transform component for node/instance **********
        let edit_transformation = find_transform_component(&mut self.editor_state, state);

        // ********** re-apply saved movement (without snapping) **********
        if let Some(selected_object_position) = self.editor_state.selected_object_position
        {
            let mut pos_x = selected_object_position.x;
            let mut pos_y = selected_object_position.y;
            let mut pos_z = selected_object_position.z;

            component_downcast_mut!(edit_transformation, Transformation);

            if !apply_x { pos_x = edit_transformation.get_data().position.x; }
            if !apply_y { pos_y = edit_transformation.get_data().position.y; }
            if !apply_z { pos_z = edit_transformation.get_data().position.z; }

            let pos = Vector3::<f32>::new(pos_x, pos_y, pos_z);

            //component_downcast_mut!(edit_transformation, Transformation);
            edit_transformation.set_translation(pos);
        }

        // ********** get pick info **********
        let mut pick_pos = None;
        let mut pick_distance = None;
        let mut bounding_min = None;
        let mut bounding_center = None;

        {
            // ***** map the pointer (mouse/touch) pos to the bottom center of the object *****
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

            let pick_predicate_grid_only = move |node: NodeItem, _check_instance_id: Option<u64>| -> bool
            {
                node.read().unwrap().name == "grid"
            };

            // ***** bounding box info *****
            if bounding_center.is_none() { return; }
            let bounding_center = bounding_center.unwrap();
            let bounding_min = bounding_min.unwrap();

            let bottom_center = Point3::<f32>::new(bounding_center.x, bounding_min.y, bounding_center.z);

            let mut bottom_center_screen_space = None;

            let (scene, _node, _instance) = self.editor_state.get_selected_node(state);
            let scene = scene.unwrap();
            for camera in &scene.cameras
            {
                // check if click is insight
                if camera.is_point_in_viewport(&pos_new)
                {
                    bottom_center_screen_space = Some(camera.get_viewport_coordinates_from_point(&bottom_center));

                    break;
                }
            }

            if bottom_center_screen_space.is_none() { return; }
            let bottom_center_screen_space = bottom_center_screen_space.unwrap();
            let new_bottom_center = bottom_center_screen_space + pos_delta;


            let pick_res: Option<(u64, ScenePickRes)>;
            if self.editor_state.drag_and_drop_grid_only
            {
                pick_res = pick(state, new_bottom_center, true, false, false, Some(Arc::new(pick_predicate_grid_only)));
            }
            else
            {
                pick_res = pick(state, new_bottom_center, true, false, false, Some(Arc::new(pick_predicate)));
            }

            if let Some(pick_res) = pick_res
            {
                pick_pos = Some(pick_res.1.point);
                pick_distance = Some(pick_res.1.time_of_impact);
            }
        }

        if pick_pos.is_none() || pick_distance.is_none() || bounding_min.is_none()
        {
            return;
        }

        let pick_pos = pick_pos.unwrap();
        let bounding_min = bounding_min.unwrap();
        let bounding_center = bounding_center.unwrap();

        let bottom_center = Point3::<f32>::new(bounding_center.x, bounding_min.y, bounding_center.z);
        let mut delta = pick_pos - bottom_center;

        // parent: because the rotation/scale of a local transform is applied otherwise to the position. which will result in movement in the wrong direction
        delta = transform_vec_to_parent_local(instance_id.clone(), selected_node.clone(), delta);

        // ********** save not snapped position **********
        if let Some(selected_object_position) = self.editor_state.selected_object_position.as_mut()
        {
            selected_object_position.x += delta.x;
            selected_object_position.y += delta.y;
            selected_object_position.z += delta.z;
        }
        else
        {
            component_downcast!(edit_transformation, Transformation);

            let pos = edit_transformation.get_data().position.clone() + delta;
            self.editor_state.selected_object_position = Some(pos);
        }

        // ********** apply movement (without snapping) **********
        // see up ^

        // ********** snap to grid center **********
        if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
        {
            let bounding_info = selected_node.read().unwrap().get_world_bounding_info(instance_id, true, None);
            if let Some((b_min, b_max)) = bounding_info
            {
                let center = b_min + (b_max - b_min) / 2.0;

                let new_x = snap_to_grid(center.x, grid_size);
                let new_z = snap_to_grid(center.z, grid_size);

                let delta = Vector3::<f32>::new(new_x - center.x, 0.0, new_z - center.z);
                let delta = transform_vec_to_parent_local(instance_id.clone(), selected_node.clone(), delta);

                component_downcast_mut!(edit_transformation, Transformation);

                edit_transformation.apply_translation(delta);
            }
        }
        // ********** bottom left snapping **********
        else if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
        {
            let bounding_info = selected_node.read().unwrap().get_world_bounding_info(instance_id, true, None);
            if let Some((b_min, b_max)) = bounding_info
            {
                let bottom_left = Vector3::<f32>::new(b_min.x, b_min.y, b_max.z);

                let new_x = snap_to_grid(bottom_left.x, grid_size);
                let new_z = snap_to_grid(bottom_left.z, grid_size);

                let delta = Vector3::<f32>::new(new_x - bottom_left.x, 0.0, new_z - bottom_left.z);
                let delta = transform_vec_to_parent_local(instance_id.clone(), selected_node.clone(), delta);

                component_downcast_mut!(edit_transformation, Transformation);

                edit_transformation.apply_translation(delta);
            }
        }
    }

    pub fn load_asset(&mut self, state: &mut State, path: String, pos: Point2::<f32>, reuse_material: bool, on_done: Option<Arc<dyn Fn(&mut Scene, NodeItem) -> () + Send + Sync>>)
    {
        if self.editor_state.loading.read().unwrap().clone()
        {
            println!("loading already in progress");
            return;
        }

        let main_queue = state.main_thread_execution_queue.clone();

        let mut scene_id = None;
        let mut editor_utils_node_id = None;
        for scene in &mut state.scenes
        {
            scene_id = Some(scene.id);

            if self.editor_state.asset_type == AssetType::Scene
            {
                scene.clear(false, true);
            }

            if let Some(editor_utils_node) = scene.find_node_by_name("editor utils")
            {
                editor_utils_node_id = Some(editor_utils_node.read().unwrap().id);
            }

            break;
        }

        if scene_id.is_none()
        {
            return;
        }

        let scene_id = scene_id.unwrap();

        let grid_size = self.editor_state.grid_size;
        let grid_amount = self.editor_state.grid_amount;

        let main_queue_clone = main_queue.clone();
        if self.editor_state.asset_type == AssetType::Scene
        {
            spawn_thread(move ||
            {
                create_grid(scene_id, editor_utils_node_id, main_queue_clone.clone(), grid_amount, grid_size);
            });
        };

        // pick
        let pick_res = pick(state, pos, true, false, false, None);

        let mut pos = None;
        if let Some(pick_res) = pick_res
        {
            pos = Some(pick_res.1.point);
        }

        let create_mipmaps = state.rendering.create_mipmaps;
        let max_tex_res = state.max_texture_resolution();
        let object_only = if self.editor_state.asset_type == AssetType::Object { true } else { false };

        let editor_state = self.editor_state.loading.clone();
        spawn_thread(move ||
        {
            dbg!("loading ...");
            *editor_state.write().unwrap() = true;

            let loaded = load_object(path.as_str(), scene_id, None, main_queue.clone(), true, reuse_material, object_only, create_mipmaps, max_tex_res);

            if loaded.is_err()
            {
                dbg!("loading failed");
                dbg!(loaded.err());
                *editor_state.write().unwrap() = false;
                return;
            }

            let loaded_ids = loaded.unwrap();

            let on_done = on_done.clone();
            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene|
            {
                //scene.clear_empty_nodes();

                let mut root_node = None;

                if let Some(pos) = pos
                {
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

                    if let Some(root_node) = &root_node
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

                        let mut transform = Transformation::identity("Transform");
                        transform.apply_translation(Vector3::<f32>::new(pos.x, pos.y + offset, pos.z));

                        root_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));

                        root_node.write().unwrap().visible = true;
                    }
                }

                if let Some(on_done) = &on_done
                {
                    if let Some(root_node) = root_node
                    {
                        on_done(scene, root_node.clone());
                    }
                }
            }));

            *editor_state.write().unwrap() = false;

            dbg!("loading DONE");
        });
    }
}
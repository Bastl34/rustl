use std::{sync::{Arc, RwLock}, f32::consts::PI};

use egui::FullOutput;

use nalgebra::{Vector3, Matrix4};

use crate::{state::{state::State, scene::{components::{transformation::Transformation, component::ComponentItem}, node::NodeItem}}, rendering::egui::EGui, input::{mouse::MouseButton, keyboard::{Key, Modifier}}, component_downcast_mut};

use super::{editor_state::{EditorState, SelectionType, SettingsPanel, EditMode}, main_frame};


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

    pub fn update(&mut self, state: &mut State)
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

        // fullscreen
        if state.input_manager.keyboard.is_pressed(Key::F)
        {
            state.rendering.fullscreen.set(!*state.rendering.fullscreen.get_ref());
        }

        // ***************** select/pick objects *****************
        if !self.editor_state.try_out && (self.editor_state.selectable || self.editor_state.pick_mode != SelectionType::None) && self.editor_state.edit_mode.is_none() && state.input_manager.mouse.clicked(MouseButton::Left)
        {
            let width = state.width;
            let height = state.height;

            let pos = state.input_manager.mouse.point.pos;

            let mut hit: Option<(f32, Vector3<f32>, NodeItem, u64, u32)> = None;
            let mut scene_id: u64 = 0;

            if let Some(pos) = pos
            {
                let scenes = &mut state.scenes;

                for scene in scenes
                {
                    for camera in &scene.cameras
                    {
                        // check if click is insight
                        if camera.is_point_in_viewport(&pos)
                        {
                            let ray = camera.get_ray_from_viewport_coordinates(&pos, width, height);

                            let new_hit = scene.pick(&ray, false);

                            let mut save_hit = false;

                            if let Some(new_hit) = new_hit.as_ref()
                            {
                                if let Some(hit) = hit.as_ref()
                                {
                                    // check if the new hit is near
                                    if new_hit.0 < hit.0
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
                }
            }

            if let Some((_t, _normal, hit_item, instance_id,_face_id)) = hit
            {
                // pick camera target
                if self.editor_state.pick_mode == SelectionType::Camera
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
                        camera.node = Some(hit_item.clone());
                    }
                }
                // show selection
                else
                {
                    let id_string;
                    {
                        let node = hit_item.read().unwrap();

                        // select object itself if there is not instance on it
                        if node.instances.get_ref().len() == 1
                        {
                            id_string = format!("objects_{}", node.id);
                        }
                        else
                        {
                            id_string = format!("objects_{}_{}", node.id, instance_id);
                        }
                    }

                    let mut already_selected = false;
                    if self.editor_state.selected_object == id_string && self.editor_state.selected_scene_id == Some(scene_id)
                    {
                        already_selected = true;
                    }

                    // deselect first
                    self.editor_state.de_select_current_item(state);

                    // highlight
                    if !already_selected
                    {
                        let node = hit_item.read().unwrap();
                        self.editor_state.selected_object = id_string;
                        self.editor_state.selected_scene_id = Some(scene_id);
                        self.editor_state.selected_type = SelectionType::Object;

                        if self.editor_state.settings != SettingsPanel::Object && self.editor_state.settings != SettingsPanel::Components
                        {
                            self.editor_state.settings = SettingsPanel::Object;
                        }

                        if let Some(instance) = node.find_instance_by_id(instance_id)
                        {
                            let mut instance = instance.write().unwrap();
                            let instance_data = instance.get_data_mut().get_mut();
                            instance_data.highlight = true;
                        }
                    }
                }
            }
            else
            {
                self.editor_state.de_select_current_item(state);
            }

            self.editor_state.pick_mode = SelectionType::None;
        }

        // ***************** DELETE OBJECT *****************
        if !self.editor_state.selected_object.is_empty()
        {
            //if state.input_manager.keyboard.is_pressed(Key::X) || state.input_manager.keyboard.is_pressed(Key::Delete)
            if state.input_manager.keyboard.is_pressed(Key::Delete)
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
                            scene.delete_node_by_id(node.read().unwrap().id);
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

        // ***************** ESCAPE *****************
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

        // ***************** EDIT MODE *****************
        let step_size = 1.0;
        let angle_steps = PI / 8.0;
        let factor = 0.01;

        if !self.editor_state.selected_object.is_empty() && self.editor_state.selected_type == SelectionType::Object && state.input_manager.mouse.point.pos.is_some()
        {
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

                let mouse_pos = state.input_manager.mouse.point.pos.unwrap();
                let movement = (mouse_pos - start_pos) * factor;
                let mut movement = Vector3::<f32>::new(movement.x, 0.0, movement.y);

                if let (Some(scene), Some(node), instance_id) = self.editor_state.get_selected_node(state)
                {
                    // get camera transform
                    // TODO: if based on multiple cameras -> pick the correct one (check viewerport and mouse coordinates)
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

                            instance.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(scene.id_manager.get_next_component_id(), "Transformation")))));

                            let transformation = node.find_component::<Transformation>().unwrap();
                            edit_transformation = transformation.clone();
                        }
                    }
                    // if there is no node and instance transform -> use node transform
                    else if instance_transform.is_none() && node_transform.is_none()
                    {
                        let mut node = node.write().unwrap();
                        node.add_component(Arc::new(RwLock::new(Box::new(Transformation::identity(scene.id_manager.get_next_component_id(), "Transformation")))));

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
            }
        }

    }

    pub fn build_gui(&mut self, state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
    {
        let raw_input = egui.ui_state.take_egui_input(window);

        let full_output = egui.ctx.run(raw_input, |ctx|
        {
            main_frame::create_frame(ctx, &mut self.editor_state, state);
        });

        let platform_output = full_output.platform_output.clone();

        egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);

        full_output
    }
}
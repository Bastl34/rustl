use std::{cell::RefCell, fmt::format, borrow::BorrowMut, mem::swap, collections::HashMap, sync::{Arc, RwLock}, f32::consts::PI};

use colored::Color;
use egui::{FullOutput, RichText, Color32, ScrollArea, Ui, RawInput, Visuals, Style, Align2};
use egui_plot::{Plot, BarChart, Bar, Legend, Corner};
use nalgebra::{Vector3, Point3, Point2, distance, ComplexField};

use crate::{state::{state::{State, FPS_CHART_VALUES}, scene::{light::{Light, LightItem}, components::{transformation::Transformation, material::{Material, MaterialItem}, mesh::Mesh, component::{Component, ComponentItem}}, node::NodeItem, scene::Scene, camera::CameraItem, instance::Instance}}, rendering::{egui::EGui, instance}, helper::change_tracker::ChangeTracker, component_downcast, input::{mouse::MouseButton, keyboard::{Key, Modifier}}, component_downcast_mut};

use super::generic_items::{self, collapse_with_title, modal_with_title};


#[derive(PartialEq, Eq)]
enum SettingsPanel
{
    Components,
    Material,
    Camera,
    Texture,
    Light,
    Scene,
    Object,
    Rendering
}

#[derive(PartialEq, Eq)]
enum SelectionType
{
    Objects,
    Cameras,
    Lights,
    Materials,
    Textures,
    None
}

#[derive(PartialEq, Eq)]
enum BottomPanel
{
    Assets,
    Debug,
    Console,
}

#[derive(Clone, Copy)]
pub enum EditMode
{
    Movement(Point2::<f32>, bool, bool, bool),
    Rotate(Point2::<f32>, bool, bool, bool)
}

pub struct Gui
{
    pub visible: bool,
    pub try_out: bool,
    pub selectable: bool,
    pub fly_camera: bool,

    pub edit_mode: Option<EditMode>,

    bottom: BottomPanel,

    settings: SettingsPanel,

    hierarchy_expand_all: bool,
    hierarchy_filter: String,

    selected_scene_id: Option<u64>,
    selected_type: SelectionType,
    selected_object: String,

    dialog_add_component: bool,
    add_component_id: usize,
    add_component_name: String,
}

impl Gui
{
    pub fn new() -> Gui
    {
        Self
        {
            visible: true,
            try_out: false,
            selectable: true,
            fly_camera: true,

            edit_mode: None,

            bottom: BottomPanel::Assets,

            settings: SettingsPanel::Rendering,

            hierarchy_expand_all: true,
            hierarchy_filter: String::new(),

            selected_scene_id: None,
            selected_type: SelectionType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID

            dialog_add_component: false,
            add_component_id: 0,
            add_component_name: "Component".to_string()
        }
    }

    pub fn de_select_current_item(&mut self, state: &mut State)
    {
        if self.selected_scene_id == None
        {
            return;
        }

        let scene_id = self.selected_scene_id.unwrap();

        for scene in &mut state.scenes
        {
            if scene_id != scene.id
            {
                continue;
            }

            let (node_id, deselect_instance_id) = self.get_object_ids();
            if let Some(node_id) = node_id
            {
                if let Some(node) = scene.find_node_by_id(node_id)
                {
                    if let Some(deselect_instance_id) = deselect_instance_id
                    {
                        if let Some(instance) = node.read().unwrap().find_instance_by_id(deselect_instance_id)
                        {
                            let mut instance = instance.borrow_mut();
                            let instance_data = instance.get_data_mut().get_mut();
                            instance_data.highlight = false;
                        }
                    }
                }
            }
        }

        self.selected_object.clear();
        self.selected_scene_id = None;
        self.selected_type = SelectionType::None;
    }

    pub fn update(&mut self, state: &mut State)
    {
        // start try out mde
        if !self.try_out && (state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)) && state.input_manager.keyboard.is_pressed(Key::R)
        {
            self.set_try_out(state, true);
        }

        // end try out mode
        if self.try_out && state.input_manager.keyboard.is_pressed(Key::Escape)
        {
            self.set_try_out(state, false);
        }

        // hide ui
        if state.input_manager.keyboard.is_pressed(Key::H)
        {
            self.visible = !self.visible;
        }

        // fullscreen
        if state.input_manager.keyboard.is_pressed(Key::F)
        {
            state.rendering.fullscreen.set(!*state.rendering.fullscreen.get_ref());
        }

        // select objects
        if !self.try_out && self.selectable && self.edit_mode.is_none() && state.input_manager.mouse.clicked(MouseButton::Left)
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
                let id_string;
                {
                    let node = hit_item.read().unwrap();
                    id_string = format!("objects_{}_{}", node.id, instance_id);
                }

                let mut already_selected = false;
                if self.selected_object == id_string && self.selected_scene_id == Some(scene_id)
                {
                    already_selected = true;
                }

                // deselect first
                self.de_select_current_item(state);

                if !already_selected
                {
                    let node = hit_item.read().unwrap();
                    self.selected_object = id_string;
                    self.selected_scene_id = Some(scene_id);
                    self.selected_type = SelectionType::Objects;

                    if self.settings != SettingsPanel::Object && self.settings != SettingsPanel::Components
                    {
                        self.settings = SettingsPanel::Object;
                    }

                    if let Some(instance) = node.find_instance_by_id(instance_id)
                    {
                        let mut instance = instance.borrow_mut();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = true;
                    }
                }
            }
            else
            {
                self.de_select_current_item(state);
            }
        }

        // ***************** DELETE OBJECT *****************
        if !self.selected_object.is_empty() && self.selected_type == SelectionType::Objects
        {
            if state.input_manager.keyboard.is_pressed(Key::X) || state.input_manager.keyboard.is_pressed(Key::Delete)
            {
                if let (Some(scene), Some(node), instance_id) = self.get_selected_node(state)
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

                    self.de_select_current_item(state);
                }
            }
        }

        // ***************** ESCAPE *****************
        if state.input_manager.keyboard.is_pressed(Key::Escape)
        {
            if self.edit_mode.is_some()
            {
                self.edit_mode = None;
            }
            else
            {
                self.de_select_current_item(state);
            }
        }

        // ***************** EDIT MODE *****************
        // TODO: move this out -> editor.rs

        let step_size = 1.0;
        let angle_steps = PI / 8.0;
        let factor = 0.01;

        if !self.selected_object.is_empty() && self.selected_type == SelectionType::Objects && state.input_manager.mouse.point.pos.is_some()
        {
            if state.input_manager.keyboard.is_pressed(Key::G)
            {
                let start_pos = state.input_manager.mouse.point.pos.unwrap();
                self.edit_mode = Some(EditMode::Movement(start_pos, true, false, true));
            }
            if state.input_manager.keyboard.is_pressed(Key::R)
            {
                let start_pos = state.input_manager.mouse.point.pos.unwrap();
                self.edit_mode = Some(EditMode::Rotate(start_pos, false, true, false));
            }

            if self.edit_mode.is_some() && state.input_manager.mouse.is_pressed(MouseButton::Left)
            {
                self.edit_mode = None;
            }

            if self.edit_mode.is_some()
            {
                let moving;
                let start_pos;
                match self.edit_mode.as_ref().unwrap()
                {
                    EditMode::Movement(pos, _, _, _) => { moving = true; start_pos = pos.clone(); },
                    EditMode::Rotate(pos, _, _, _) => { moving = false; start_pos = pos.clone(); },
                }

                if state.input_manager.keyboard.is_pressed(Key::X)
                {
                    if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos.clone(), true, false, false)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos.clone(), true, false, false)); }
                    }
                    else
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos, false, true, true)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos, false, true, true)); }
                    }
                }

                if state.input_manager.keyboard.is_pressed(Key::Y)
                {
                    if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos, false, true, false)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos, false, true, false)); }
                    }
                    else
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos, true, false, true)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos, true, false, true)); }
                    }
                }

                if state.input_manager.keyboard.is_pressed(Key::Z)
                {
                    if !state.input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos, false, false, true)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos, false, false, true)); }
                    }
                    else
                    {
                        if moving { self.edit_mode = Some(EditMode::Movement(start_pos, true, true, false)); }
                        else      { self.edit_mode = Some(EditMode::Rotate  (start_pos, true, true, false)); }
                    }
                }

                let edit_mode = self.edit_mode.unwrap();

                let mouse_pos = state.input_manager.mouse.point.pos.unwrap();
                let movement = (mouse_pos - start_pos) * factor;

                if let (Some(scene), Some(node), Some(instance_id)) = self.get_selected_node(state)
                {
                    let edit_transformation: ComponentItem;
                    let node_transform;
                    let instance_transform;
                    let instances_amount;

                    {
                        let node = node.read().unwrap();
                        instances_amount = node.instances.get_ref().len();
                        let instance = node.find_instance_by_id(instance_id).unwrap() ;

                        let instance = instance.borrow_mut();

                        node_transform = node.find_component::<Transformation>();
                        instance_transform = instance.find_component::<Transformation>();
                    }

                    // if there are multiple instances in the node -> use instance transform
                    if instances_amount > 1
                    {
                        if let Some(instance_transform) = instance_transform
                        {
                            edit_transformation = instance_transform.clone();
                        }
                        else
                        {
                            let node = node.read().unwrap();
                            let instance = node.find_instance_by_id(instance_id).unwrap() ;
                            let mut instance = instance.borrow_mut();

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
                                    let sign = movement.y.signum();
                                    if movement.y.abs() >= step_size
                                    {
                                        vec.y = step_size * sign;
                                        applied = true;
                                    }
                                }
                                else
                                {
                                    vec.y = movement.y;
                                    applied = true;
                                }
                            }

                            if z
                            {
                                if state.input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) || state.input_manager.keyboard.is_holding_modifier(Modifier::Logo)
                                {
                                    let sign = movement.y.signum();
                                    if movement.y.abs() >= step_size
                                    {
                                        vec.z = -step_size * sign;
                                        applied = true;
                                    }
                                }
                                else
                                {
                                    vec.z = -movement.y;
                                    applied = true;
                                }
                            }

                            if applied
                            {
                                edit_transformation.apply_translation(vec);
                            }

                            if applied
                            {
                                self.edit_mode = Some(EditMode::Movement(mouse_pos, x, y, z));
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
                                    let sign = movement.y.signum();
                                    if movement.y.abs() >= angle_steps
                                    {
                                        vec.x = angle_steps * sign;
                                        applied = true;
                                    }
                                }
                                else
                                {
                                    vec.x = movement.y;
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
                                self.edit_mode = Some(EditMode::Rotate(mouse_pos, x, y, z));
                            }
                        },
                    }
                }
            }
        }

    }

    pub fn set_try_out(&mut self, state: &mut State, try_out: bool)
    {
        self.try_out = try_out;
        self.visible = !try_out;
        state.rendering.fullscreen.set(try_out);
        state.input_manager.mouse.visible.set(!try_out);

        if try_out
        {
            self.de_select_current_item(state);
        }
    }

    pub fn build_gui(&mut self, state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
    {
        let raw_input = egui.ui_state.take_egui_input(window);

        let full_output = egui.ctx.run(raw_input, |ctx|
        {
            self.create_frame(ctx, state);
        });

        let platform_output = full_output.platform_output.clone();

        egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);

        full_output
    }

    fn create_frame(&mut self, ctx: &egui::Context, state: &mut State)
    {
        let mut visual = Visuals::dark();
        visual.panel_fill[3] = 253;
        //visual.override_text_color = Some(egui::Color32::WHITE);

        let style = Style
        {
            visuals: visual,
            ..Style::default()
        };

        let frame = egui::Frame::side_top_panel(&style);

        egui::TopBottomPanel::top("top_panel").frame(frame).show(ctx, |ui|
        //egui::TopBottomPanel::top("top_panel").show(ctx, |ui|
        {
            ui.horizontal(|ui|
            {
                self.create_file_menu(state, ui);
            });
        });

        //bottom
        egui::TopBottomPanel::bottom("bottom_panel").frame(frame).show(ctx, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.selectable_value(&mut self.bottom, BottomPanel::Assets, "📦 Assets");
                ui.selectable_value(&mut self.bottom, BottomPanel::Debug, "🐛 Debug");
                ui.selectable_value(&mut self.bottom, BottomPanel::Console, "📝 Console");
            });
            ui.separator();
        });

        //left
        egui::SidePanel::left("left_panel").frame(frame).show(ctx, |ui|
        {
            ui.set_min_width(300.0);

            self.create_left_sidebar(state, ui);
        });

        //right
        egui::SidePanel::right("right_panel").frame(frame).show(ctx, |ui|
        {
            ui.set_min_width(300.0);

            let mut object_settings = false;
            let mut camera_settings = false;
            let mut light_settings = false;
            let mut material_settings = false;
            let mut texture_settings = false;

            ui.horizontal(|ui|
            {
                if self.selected_type == SelectionType::Objects && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Components, " Components");
                    ui.selectable_value(&mut self.settings, SettingsPanel::Object, "◼ Object");

                    object_settings = true;
                }

                if self.selected_type == SelectionType::Cameras && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Camera, "📷 Camera");

                    camera_settings = true;
                }

                if self.selected_type == SelectionType::Lights && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Light, "💡 Light");

                    light_settings = true;
                }

                if self.selected_type == SelectionType::Materials && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Material, "🎨 Material");

                    material_settings = true;
                }

                if self.selected_type == SelectionType::Textures && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Texture, "🖼 Texture");

                    texture_settings = true;
                }

                if self.selected_scene_id.is_some()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Scene, "🎬 Scene");
                }

                ui.selectable_value(&mut self.settings, SettingsPanel::Rendering, "📷 Rendering");
            });
            ui.separator();

            ScrollArea::vertical().show(ui, |ui|
            {
                match self.settings
                {
                    SettingsPanel::Components => if object_settings
                    {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                        {
                            self.create_component_settings(state, ui);
                        });
                    },
                    SettingsPanel::Object => if object_settings { self.create_object_settings(state, ui); },
                    SettingsPanel::Material => if material_settings { self.create_material_settings(state, ui); },
                    SettingsPanel::Camera => if camera_settings { self.create_camera_settings(state, ui); },
                    SettingsPanel::Texture => if texture_settings { },
                    SettingsPanel::Light => if light_settings { self.create_light_settings(state, ui); },
                    SettingsPanel::Scene => self.create_scene_settings(state, ui),
                    SettingsPanel::Rendering => self.create_rendering_settings(state, ui),
                }
            });
        });

        //top
        egui::TopBottomPanel::top("top_panel_main").frame(frame).show(ctx, |ui|
        {
            ui.horizontal(|ui|
            {
                //ui.label("STATUS BLA BLA BLA BLA");
                self.create_tool_menu(state, ui);
            });
        });

        // create component
        self.create_component_add_modal(state, ctx);

    }

    fn create_component_add_modal(&mut self, state: &mut State, ctx: &egui::Context)
    {
        let mut dialog_add_component = self.dialog_add_component;

        modal_with_title(ctx, &mut dialog_add_component, "Add component", |ui|
        {
            ui.label("Add your component");

            ui.horizontal(|ui|
            {
                ui.label("Name: ");
                ui.text_edit_singleline(&mut self.add_component_name);
            });

            ui.horizontal(|ui|
            {
                ui.label("Component: ");

                let current_component_name = state.registered_components.get(self.add_component_id).unwrap().0.clone();

                egui::ComboBox::from_label("").selected_text(current_component_name).show_ui(ui, |ui|
                {
                    ui.style_mut().wrap = Some(false);
                    ui.set_min_width(40.0);

                    for (component_id, component) in state.registered_components.iter().enumerate()
                    {
                        ui.selectable_value(&mut self.add_component_id, component_id, component.0.clone());
                    }
                });
            });
            if ui.button("Add").clicked()
            {
                let (node_id, instance_id) = self.get_object_ids();

                if let (Some(scene_id), Some(node_id)) = (self.selected_scene_id, node_id)
                {
                    let component = state.registered_components.get(self.add_component_id).unwrap().clone();

                    let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                    let node = scene.find_node_by_id(node_id).unwrap();


                    if let Some(instance_id) = instance_id
                    {
                        let node = node.read().unwrap();
                        let instance = node.find_instance_by_id(instance_id).unwrap();
                        let mut instance = instance.borrow_mut();
                        instance.add_component(component.1(scene.id_manager.get_next_instance_id(), self.add_component_name.as_str()));
                    }
                    else
                    {
                        node.write().unwrap().add_component(component.1(scene.id_manager.get_next_instance_id(), self.add_component_name.as_str()));
                    }
                }

                self.dialog_add_component = false;
                self.add_component_name = "Component".to_string();
            }
        });

        if !dialog_add_component
        {
            self.dialog_add_component = dialog_add_component;
        }
    }

    fn create_tool_menu(&mut self, state: &mut State, ui: &mut Ui)
    {
        let icon_size = 20.0;

        ui.horizontal(|ui|
        {
            let mut fullscreen = state.rendering.fullscreen.get_ref().clone();
            let mut try_out = self.try_out;

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui|
            {
                // selectable
                if ui.toggle_value(&mut self.selectable, RichText::new("🖱").size(icon_size)).on_hover_text("select objects").changed()
                {
                    if !self.selectable
                    {
                        self.de_select_current_item(state);
                    }
                }

                ui.toggle_value(&mut self.fly_camera, RichText::new("✈").size(icon_size)).on_hover_text("fly camera");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                // fullscreen change
                if ui.toggle_value(&mut fullscreen, RichText::new("⛶").size(icon_size)).on_hover_text("fullscreen").changed()
                {
                    state.rendering.fullscreen.set(fullscreen);
                }

                // try out mode
                if ui.toggle_value(&mut try_out, RichText::new("🚀").size(icon_size)).on_hover_text("try out").changed()
                {
                    self.set_try_out(state, try_out);
                };
            });


            //🖱
        });
    }

    fn create_left_sidebar(&mut self, state: &mut State, ui: &mut Ui)
    {
        // statistics
        collapse_with_title(ui, "chart", true, "📈 Chart", |ui|
        {
            self.create_chart(state, ui);
        });

        // statistics
        collapse_with_title(ui, "statistic", true, "ℹ Statistics", |ui|
        {
            self.create_statistic(state, ui);
        });

        // hierarchy
        collapse_with_title(ui, "hierarchy", true, "🗄 Hierarchy", |ui|
        {
            ScrollArea::vertical().show(ui, |ui|
            {
                self.create_hierarchy(state, ui);
            });
        });
    }

    fn create_chart(&mut self, state: &mut State, ui: &mut Ui)
    {
        // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/plot_demo.rs#L888

        let chart = BarChart::new
        (
            state.fps_chart.iter().enumerate().map(|(i, value)|
            {
                //Bar::new((i - FPS_CHART_VALUES) as f64, *value as f64).width(0.05)
                Bar::new(i as f64, *value as f64).width(0.05)
            }).collect(),
        )
        .color(Color32::WHITE)
        .name("FPS");

        let legend = Legend::default().position(Corner::LeftTop);

        Plot::new("FPS")
            .legend(legend)
            .clamp_grid(true)
            .y_axis_width(4)
            .y_axis_position(egui_plot::HPlacement::Right)
            .allow_zoom(false)
            .height(120.0)
            .allow_drag(false)
            .show(ui, |plot_ui| plot_ui.bar_chart(chart));
    }

    fn create_statistic(&mut self, state: &mut State, ui: &mut Ui)
    {
        let mut textures = 0;
        let mut materials = 0;
        for scene in &state.scenes
        {
            textures += scene.textures.len();
            materials += scene.materials.len();
        }

        ui.label(RichText::new("ℹ Info").strong());
        ui.label(format!(" ⚫ fps: {}", state.last_fps));
        ui.label(format!(" ⚫ absolute fps: {}", state.fps_absolute));
        ui.label(format!(" ⚫ frame time: {:.3} ms", state.frame_time));

        ui.label(RichText::new("⚙ Engine").strong());
        ui.label(format!(" ⚫ update time: {:.3} ms", state.engine_update_time));
        ui.label(format!(" ⚫ render time: {:.3} ms", state.engine_render_time));
        ui.label(format!(" ⚫ draw calls: {}", state.draw_calls));
        ui.label(format!(" ⚫ textures: {}", textures));
        ui.label(format!(" ⚫ materials: {}", materials));

        ui.label(RichText::new("✏ Editor").strong());
        ui.label(format!(" ⚫ update time: {:.3} ms", state.egui_update_time));
        ui.label(format!(" ⚫ render time: {:.3} ms", state.egui_render_time));

        ui.label(RichText::new("🗖 App").strong());
        ui.label(format!(" ⚫ update time: {:.3} ms", state.app_update_time));
    }

    fn create_hierarchy(&mut self, state: &mut State, ui: &mut Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.hierarchy_filter).desired_width(120.0));

            ui.toggle_value(&mut self.hierarchy_expand_all, "⊞").on_hover_text("expand all items");
        });

        for scene in &mut state.scenes
        {
            let scene_id = scene.id;
            let id = format!("scene_{}", scene_id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() && self.selected_type == SelectionType::None { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new(format!("🎬 {}: {}", scene_id, scene.name)).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::None;
                            self.settings = SettingsPanel::Scene;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.settings = SettingsPanel::Rendering;
                        }
                    }
                });
            }).body(|ui|
            {
                //self.build_node_list(ui, &scene.nodes, scene_id, true);
                self.create_hierarchy_type_entries(scene, ui);
            });
        }
    }

    fn create_hierarchy_type_entries(&mut self, scene: &mut Box<Scene>, ui: &mut Ui)
    {
        let scene_id = scene.id;

        // objects
        {
            let id = format!("objects_{}", scene.id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == SelectionType::Objects { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("◼ Objects").color(Color32::LIGHT_GREEN).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::Objects;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = SelectionType::None;
                        }
                    }
                });
            }).body(|ui|
            {
                self.build_node_list(ui, &scene.nodes, scene.id, true);
            });
        }

        // cameras
        {
            let id = format!("cameras_{}", scene.id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == SelectionType::Cameras { selection = true; } else { selection = false; }

                    let toggle = ui.toggle_value(&mut selection, RichText::new("📷 Cameras").color(Color32::LIGHT_RED).strong());
                    let toggle = toggle.context_menu(|ui|
                    {
                        if ui.button("Add New Camera").clicked()
                        {
                            ui.close_menu();
                            scene.add_camera("Camera");
                        }
                    });

                    if toggle.clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::Cameras;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = SelectionType::None;
                        }
                    }
                });
            }).body(|ui|
            {
                self.build_camera_list(&scene.cameras, ui, scene_id);
            });
        }

        // lights
        {
            let id = format!("lights_{}", scene.id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == SelectionType::Lights { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("💡 Lights").color(Color32::YELLOW).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::Lights;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = SelectionType::None;
                        }
                    }
                });
            }).body(|ui|
            {
                self.build_light_list(&scene.lights, ui, scene_id);
            });
        }

        // materials
        {
            let id = format!("materials_{}", scene.id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == SelectionType::Materials { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("🎨 Materials").color(Color32::GOLD).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::Materials;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = SelectionType::None;
                        }
                    }
                });
            }).body(|ui|
            {
                self.build_material_list(&scene.materials, ui, scene_id);
            });
        }

        // textures
        {
            let id = format!("textures_{}", scene.id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == SelectionType::Textures { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("🖼 Textures").color(Color32::LIGHT_BLUE).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = SelectionType::Textures;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = SelectionType::None;
                        }
                    }
                });
            }).body(|ui|
            {

            });
        }
    }

    pub fn build_node_list(&mut self, ui: &mut Ui, nodes: &Vec<NodeItem>, scene_id: u64, parent_visible: bool)
    {
        for node_arc in nodes
        {
            let node = node_arc.read().unwrap();
            let child_nodes = &node.nodes.clone();

            let visible = node.visible && parent_visible;

            let name = node.name.clone();
            let node_id = node.id;

            let id = format!("objects_{}", node_id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let headline_name: String;
                    if node.is_empty()
                    {
                        headline_name = format!("👻 {}: {}", node_id, name.clone());
                    }
                    else if node.get_mesh().is_some()
                    {
                        headline_name = format!("◼ {}: {}", node_id, name.clone());
                    }
                    else
                    {
                        headline_name = format!("◻ {}: {}", node_id, name.clone());
                    }

                    let heading;
                    if visible
                    {
                        heading = RichText::new(headline_name).strong()
                    }
                    else
                    {
                        heading = RichText::new(headline_name).strikethrough();
                    }

                    let mut selection; if self.selected_object == id { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, heading).clicked()
                    {
                        if self.selected_object != id
                        {
                            self.selected_object = id;
                            self.selected_scene_id = Some(scene_id);
                            self.selected_type = SelectionType::Objects;

                            if self.settings != SettingsPanel::Components && self.settings != SettingsPanel::Object
                            {
                                self.settings = SettingsPanel::Components;
                            }
                        }
                        else
                        {
                            self.selected_object.clear();
                            self.selected_scene_id = None;
                        }
                    }
                });

            }).body(|ui|
            {
                if child_nodes.len() > 0
                {
                    self.build_node_list(ui, child_nodes, scene_id, visible);
                }

                if node.instances.get_ref().len() > 0
                {
                    self.build_instances_list(ui, node_arc.clone(), scene_id, visible);
                }
            });
        }
    }

    pub fn build_instances_list(&mut self, ui: &mut Ui, node: NodeItem, scene_id: u64, parent_visible: bool)
    {
        let node = node.read().unwrap();

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            for instance in node.instances.get_ref()
            {
                let instance = instance.borrow();
                let instance_id = instance.id;
                let instance_data = instance.get_data();

                let id = format!("objects_{}_{}", node.id, instance_id);
                let headline_name = format!("⚫ {}: {}", instance_id, instance.name);

                let mut heading = RichText::new(headline_name);

                if instance_data.visible && parent_visible
                {
                    heading = heading.strong()
                }
                else
                {
                    heading = heading.strikethrough();
                }

                if instance_data.highlight
                {
                    heading = heading.color(Color32::from_rgb(255, 175, 175));
                }

                let mut selection; if self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    if self.selected_object != id
                    {
                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = SelectionType::Objects;

                        if self.settings != SettingsPanel::Components && self.settings != SettingsPanel::Object
                        {
                            self.settings = SettingsPanel::Components;
                        }
                    }
                    else
                    {
                        self.selected_object.clear();
                        self.selected_scene_id = None;
                    }
                }
            }
        });
    }

    pub fn build_material_list(&mut self, materials: &HashMap<u64, MaterialItem>, ui: &mut Ui, scene_id: u64)
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            for (material_id, material) in materials
            {
                let material = material.read().unwrap();
                let headline_name = format!("⚫ {}: {}", material_id, material.get_base().name);

                let id = format!("material_{}", material_id);

                let heading = RichText::new(headline_name).strong();

                let mut selection; if self.selected_type == SelectionType::Materials && self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    //if self.selected_material.is_none() || (self.selected_material.is_some() && self.selected_material.unwrap() != *material_id)
                    if selection
                    {

                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = SelectionType::Materials;
                        self.settings = SettingsPanel::Material;
                    }
                    else
                    {
                        self.selected_object.clear();
                        self.selected_scene_id = None;
                    }
                }
            }
        });
    }

    pub fn build_camera_list(&mut self, cameras: &Vec<CameraItem>, ui: &mut Ui, scene_id: u64)
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            for camera in cameras
            {
                let headline_name = format!("⚫ {}: {}", camera.id, camera.name);

                let id = format!("camera_{}", camera.id);

                let mut heading = RichText::new(headline_name).strong();
                if !camera.enabled
                {
                    heading = heading.strikethrough();
                }

                let mut selection; if self.selected_type == SelectionType::Cameras && self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    if selection
                    {
                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = SelectionType::Cameras;
                        self.settings = SettingsPanel::Camera;
                    }
                    else
                    {
                        self.selected_object.clear();
                        self.selected_scene_id = None;
                    }
                }
            }
        });
    }

    pub fn build_light_list(&mut self, lights: &ChangeTracker<Vec<RefCell<ChangeTracker<LightItem>>>>, ui: &mut Ui, scene_id: u64)
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            let lights = lights.get_ref();
            for light in lights
            {
                let light = light.borrow();
                let light = light.get_ref();

                let headline_name = format!("⚫ {}: {}", light.id, light.name);

                let id = format!("light_{}", light.id);

                let mut heading = RichText::new(headline_name).strong();
                if !light.enabled
                {
                    heading = heading.strikethrough();
                }

                let mut selection; if self.selected_type == SelectionType::Lights && self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    if selection
                    {
                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = SelectionType::Lights;
                        self.settings = SettingsPanel::Light;
                    }
                    else
                    {
                        self.selected_object.clear();
                        self.selected_scene_id = None;
                    }
                }
            }
        });
    }

    fn get_object_ids(&self) -> (Option<u64>, Option<u64>)
    {
        // no scene selected
        if self.selected_scene_id == None || self.selected_object.is_empty()
        {
            return (None, None);
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();

        let mut item_id: Option<u64> = None;
        let mut subitem_id: Option<u64> = None; // like instance id

        if parts.len() >= 2
        {
            item_id = Some(parts.get(1).unwrap().parse().unwrap());
        }

        if parts.len() >= 3
        {
            subitem_id = Some(parts.get(2).unwrap().parse().unwrap());
        }

        (item_id, subitem_id)
    }

    fn get_selected_node<'a>(&'a mut self, state: &'a mut State) -> (Option<&'a mut Box<Scene>>, Option<NodeItem>, Option<u64>)
    {
        let (node_id, instance_id) = self.get_object_ids();

        if self.selected_scene_id.is_none() || node_id.is_none()
        {
            return (None, None, None);
        }

        let scene_id: u64 = self.selected_scene_id.unwrap();
        let node_id: u64 = node_id.unwrap();

        let scene = state.find_scene_by_id_mut(scene_id);

        if scene.is_none()
        {
            return (None, None, None);
        }

        let scene = scene.unwrap();

        let node = scene.find_node_by_id(node_id);

        if node.is_none()
        {
            return (None, None, None);
        }

        let node = node.unwrap();

        (Some(scene), Some(node.clone()), instance_id)
    }

    fn create_component_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (node_id, instance_id) = self.get_object_ids();

        if self.selected_scene_id.is_none() || node_id.is_none()
        {
            return;
        }

        let scene_id: u64 = self.selected_scene_id.unwrap();
        let node_id: u64 = node_id.unwrap();

        let scene = state.find_scene_by_id(scene_id);

        if scene.is_none()
        {
            return;
        }

        let scene = scene.unwrap();

        let node = scene.find_node_by_id(node_id);

        if node.is_none()
        {
            return;
        }

        let node = node.unwrap();

        // components
        if instance_id.is_none()
        {
            let mut delete_component_id = None;

            let node_read = node.read().unwrap();
            for component in &node_read.components
            {
                let component_id;
                let name;
                let component_name;
                {
                    let component = component.read().unwrap();
                    let base = component.get_base();
                    component_name = format!("{} {}", base.icon, base.component_name);
                    name = base.name.clone();
                    component_id = component.id();
                }
                generic_items::collapse(ui, component_id.to_string(), true, |ui|
                {
                    ui.label(RichText::new(component_name).heading().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                    {
                        if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                        {
                            delete_component_id = Some(component_id);
                        }

                        // enabled toggle
                        let mut enabled;
                        {
                            enabled = component.read().unwrap().get_base().is_enabled;
                        }

                        let toggle_text;
                        if enabled
                        {
                            toggle_text = RichText::new("⏺").color(Color32::GREEN);
                        }
                        else
                        {
                            toggle_text = RichText::new("⏺").color(Color32::RED);
                        }


                        if ui.toggle_value(&mut enabled, toggle_text).clicked()
                        {
                            component.write().unwrap().set_enabled(enabled);
                        }

                        if let Some(info) = &component.read().unwrap().get_base().info
                        {
                            ui.label(RichText::new("ℹ").color(Color32::WHITE)).on_hover_text(info);
                        }
                    });
                },
                |ui|
                {
                    ui.label(format!("Id: {}", component_id));
                    ui.label(format!("Name: {}", name));

                    let mut component = component.write().unwrap();
                    component.ui(ui);
                });
            }

            drop(node_read);

            if let Some(delete_component_id) = delete_component_id
            {
                node.write().unwrap().remove_component_by_id(delete_component_id);
            }
        }

        if let Some(instance_id) = instance_id
        {
            let mut delete_component_id = None;

            let node_read: std::sync::RwLockReadGuard<'_, Box<crate::state::scene::node::Node>> = node.read().unwrap();
            let instance = node_read.find_instance_by_id(instance_id);

            if let Some(instance) = instance
            {
                {
                    let instance = instance.borrow();

                    for component in &instance.components
                    {
                        let component_id;
                        let name;
                        let component_name;
                        {
                            let component = component.read().unwrap();
                            let base = component.get_base();
                            component_name = format!("{} {}", base.icon, base.component_name);
                            name = base.name.clone();
                            component_id = component.id();
                        }
                        generic_items::collapse(ui, component_id.to_string(), true, |ui|
                        {
                            ui.label(RichText::new(component_name).heading().strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                            {
                                if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                                {
                                    delete_component_id = Some(component_id);
                                }

                                // enabled toggle
                                let mut enabled;
                                {
                                    enabled = component.read().unwrap().get_base().is_enabled;
                                }

                                let toggle_text;
                                if enabled
                                {
                                    toggle_text = RichText::new("⏺").color(Color32::GREEN);
                                }
                                else
                                {
                                    toggle_text = RichText::new("⏺").color(Color32::RED);
                                }


                                if ui.toggle_value(&mut enabled, toggle_text).clicked()
                                {
                                    component.write().unwrap().set_enabled(enabled);
                                }

                                if let Some(info) = &component.read().unwrap().get_base().info
                                {
                                    ui.label(RichText::new("ℹ").color(Color32::WHITE)).on_hover_text(info);
                                }
                            });
                        },
                        |ui|
                        {
                            ui.label(format!("Id: {}", component_id));
                            ui.label(format!("Name: {}", name));

                            let mut component = component.write().unwrap();
                            component.ui(ui);
                        });
                    }
                }

                if let Some(delete_component_id) = delete_component_id
                {
                    let mut instance = instance.borrow_mut();
                    instance.remove_component_by_id(delete_component_id);
                }
            }
        }

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Add Component").heading().strong().color(Color32::WHITE)).clicked()
            {
                self.dialog_add_component = true;
            }
        });
    }

    fn create_object_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (node_id, instance_id) = self.get_object_ids();

        // no scene selected
        if self.selected_scene_id.is_none() || node_id.is_none()
        {
            return;
        }

        let scene_id: u64 = self.selected_scene_id.unwrap();
        let node_id: u64 = node_id.unwrap();

        let scene = state.find_scene_by_id(scene_id);

        if scene.is_none()
        {
            return;
        }

        let scene = scene.unwrap();

        let node = scene.find_node_by_id(node_id);

        if node.is_none()
        {
            return;
        }

        let node = node.unwrap();

        let mut direct_instances_amout = 0;
        let mut direct_meshes_amout = 0;
        let mut direct_vertices_amout = 0;
        let mut direct_indices_amout = 0;
        let mut direct_childs_amount = 0;

        let mut all_instances_amout = 0;
        let mut all_meshes_amout = 0;
        let mut all_vertices_amout = 0;
        let mut all_indices_amout = 0;
        let mut all_childs_amount = 0;

        {
            let node = node.read().unwrap();

            // direct items
            direct_instances_amout += node.instances.get_ref().len();

            {
                let mesh = node.find_component::<Mesh>();
                if let Some(mesh) = mesh
                {
                    component_downcast!(mesh, Mesh);

                    direct_meshes_amout += 1;
                    direct_vertices_amout += mesh.get_data().vertices.len();
                    direct_indices_amout += mesh.get_data().indices.len();
                }
            }

            direct_childs_amount = scene.nodes.len();

            // items of all descendants
            let all_nodes = Scene::list_all_child_nodes(&node.nodes);
            all_childs_amount = all_nodes.len();

            for node in &all_nodes
            {
                let node = node.read().unwrap();
                all_instances_amout += node.instances.get_ref().len();

                let mesh = node.find_component::<Mesh>();
                if let Some(mesh) = mesh
                {
                    component_downcast!(mesh, Mesh);

                    all_meshes_amout += 1;
                    all_vertices_amout += mesh.get_data().vertices.len();
                    all_indices_amout += mesh.get_data().indices.len();
                }
            }
        }

        // General
        collapse_with_title(ui, "object_data", true, "ℹ Object Data", |ui|
        {
            {
                let node = node.read().unwrap();

                ui.label(format!("name: {}", node.name));
                ui.label(format!("id: {}", node.id));
            }
        });


        // statistics
        collapse_with_title(ui, "object_info", true, "📈 Object Info", |ui|
        {
            ui.label(RichText::new("👤 own").strong());
            ui.label(format!(" ⚫ instances: {}", direct_instances_amout));
            ui.label(format!(" ⚫ nodes: {}", direct_childs_amount));
            ui.label(format!(" ⚫ meshes: {}", direct_meshes_amout));
            ui.label(format!(" ⚫ vertices: {}", direct_vertices_amout));
            ui.label(format!(" ⚫ indices: {}", direct_indices_amout));

            ui.label(RichText::new("👪 all descendants").strong());
            ui.label(format!(" ⚫ instances: {}", all_instances_amout));
            ui.label(format!(" ⚫ nodes: {}", all_childs_amount));
            ui.label(format!(" ⚫ meshes: {}", all_meshes_amout));
            ui.label(format!(" ⚫ vertices: {}", all_vertices_amout));
            ui.label(format!(" ⚫ indices: {}", all_indices_amout));
        });

        // Settings
        collapse_with_title(ui, "object_settings", true, "⛭ Object Settings", |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut render_children_first;
            let mut alpha_index;
            let mut name;
            {
                let node = node.read().unwrap();
                visible = node.visible;
                render_children_first = node.render_children_first;
                alpha_index = node.alpha_index;
                name = node.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });
            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut render_children_first, "render children first").changed() || changed;
            ui.horizontal(|ui|
            {
                ui.label("alpha index: ");
                changed = ui.add(egui::DragValue::new(&mut alpha_index).speed(1)).changed() || changed;
            });

            if changed
            {
                let mut node = node.write().unwrap();
                node.visible = visible;
                node.render_children_first = render_children_first;
                node.alpha_index = alpha_index;
                node.name = name;
            }

            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
            {
                if ui.button(RichText::new("Create Default Instance").heading().strong().color(Color32::LIGHT_GREEN)).clicked()
                {
                    let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                    node.write().unwrap().create_default_instance(node.clone(), scene.id_manager.get_next_instance_id());
                }

                if ui.button(RichText::new("Dispose Node").heading().strong().color(ui.visuals().error_fg_color)).clicked()
                {
                    let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                    scene.delete_node_by_id(node_id);
                }
            });
        });

        if let Some(instance_id) = instance_id
        {
            self.create_instance_settings(state, scene_id, node, instance_id, ui);
        }
    }

    fn create_instance_settings(&mut self, state: &mut State, scene_id: u64, node_arc: NodeItem, instance_id: u64 , ui: &mut Ui)
    {
        let node = node_arc.read().unwrap();
        let instance = node.find_instance_by_id(instance_id);

        if instance.is_none()
        {
            return;
        }

        ui.separator();

        let instance = instance.unwrap();

        // General
        collapse_with_title(ui, "instance_data", true, "ℹ Instance Data", |ui|
        {
            let instance = instance.borrow();

            ui.label(format!("name: {}", instance.name));
            ui.label(format!("id: {}", instance.id));
        });

        // Settings
        let mut delete_instance = false;
        collapse_with_title(ui, "instance_settings", true, "⛭ Instance Settings", |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut collision;
            let mut highlight;
            let mut name;
            let mut pickable;
            {
                let instance = instance.borrow();
                let instance_data = instance.get_data();
                visible = instance_data.visible;
                collision = instance_data.collision;
                highlight = instance_data.highlight;
                name = instance.name.clone();
                pickable = instance.pickable;
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });
            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut collision, "collision").changed() || changed;
            changed = ui.checkbox(&mut highlight, "highlight").changed() || changed;
            changed = ui.checkbox(&mut pickable, "pickable").changed() || changed;

            if changed
            {
                let mut instance = instance.borrow_mut();
                let instance_data = instance.get_data_mut().get_mut();
                instance_data.visible = visible;
                instance_data.collision = collision;
                instance_data.highlight = highlight;
                instance.name = name;
                instance.pickable = pickable;
            }

            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
            {
                if ui.button(RichText::new("Dispose Instance").heading().strong().color(ui.visuals().error_fg_color)).clicked()
                {
                    delete_instance = true;
                }
            });
        });

        drop(node);

        if delete_instance
        {
            let mut node = node_arc.write().unwrap();
            node.delete_instance_by_id(instance_id);
        }
    }

    fn create_material_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_scene_id.is_none() { return; }
        let scene_id: u64 = self.selected_scene_id.unwrap();

        let (material_id, ..) = self.get_object_ids();

        let scene = state.find_scene_by_id(scene_id);
        if scene.is_none() { return; }

        let scene = scene.unwrap();

        if material_id.is_none() { return; }
        let material_id = material_id.unwrap();

        if let Some(material) = scene.get_material_by_id(material_id)
        {
            collapse_with_title(ui, "material_settings", true, "🎨 Material Settings", |ui|
            {
                let mut material = material.write().unwrap();
                material.ui(ui);
            });
        }
    }

    fn create_camera_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_scene_id.is_none() { return; }
        let scene_id: u64 = self.selected_scene_id.unwrap();

        let (camera_id, ..) = self.get_object_ids();

        let scene = state.find_scene_by_id_mut(scene_id);
        if scene.is_none() { return; }

        let scene = scene.unwrap();

        if camera_id.is_none() { return; }
        let camera_id = camera_id.unwrap();

        if let Some(camera) = scene.get_camera_by_id_mut(camera_id)
        {
            collapse_with_title(ui, "camera_general_settings", true, "⛭ General Settings", |ui|
            {
                let mut changed = false;

                let mut enabled;
                let mut name;
                {
                    enabled = camera.enabled;
                    name = camera.name.clone();
                }

                ui.horizontal(|ui|
                {
                    ui.label("name: ");
                    changed = ui.text_edit_singleline(&mut name).changed() || changed;
                });
                changed = ui.checkbox(&mut enabled, "enabled").changed() || changed;

                if changed
                {
                    camera.enabled = enabled;
                    camera.name = name;
                }
            });

            collapse_with_title(ui, "camera_settings", true, "📷 Camera Settings", |ui|
            {
                camera.ui(ui);
            });

            if let Some(controller) = &mut camera.controller
            {
                let mut delete_controller;
                let mut enabled;
                let name;
                {
                    delete_controller = false;
                    enabled = controller.get_base().is_enabled;
                    name = format!("{} {}",controller.get_base().icon.clone(), controller.get_base().name.clone());
                }

                generic_items::collapse(ui, "camera_controller".to_string(), true, |ui|
                {
                    ui.label(RichText::new(name).heading().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                    {
                        if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                        {
                            delete_controller = true;
                        }

                        // enabled toggle

                        let toggle_text;
                        if enabled
                        {
                            toggle_text = RichText::new("⏺").color(Color32::GREEN);
                        }
                        else
                        {
                            toggle_text = RichText::new("⏺").color(Color32::RED);
                        }

                        ui.toggle_value(&mut enabled, toggle_text)
                    });
                },
                |ui|
                {
                    controller.ui(ui);
                });

                controller.get_base_mut().is_enabled = enabled;

                if delete_controller
                {
                    camera.controller = None;
                }
            }
        }

        // delete camera
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Camera").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_camera_by_id(camera_id);
            }
        });
    }

    fn create_light_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_scene_id.is_none() { return; }
        let scene_id: u64 = self.selected_scene_id.unwrap();

        let (light_id, ..) = self.get_object_ids();

        let scene = state.find_scene_by_id_mut(scene_id);
        if scene.is_none() { return; }

        let scene = scene.unwrap();

        if light_id.is_none() { return; }
        let light_id = light_id.unwrap();

        if let Some(light) = scene.get_light_by_id(light_id)
        {
            collapse_with_title(ui, "light_general_settings", true, "⛭ General Settings", |ui|
            {
                let mut changed = false;

                let mut enabled;
                let mut name;
                {
                    let mut light = light.borrow();
                    let light = light.get_ref();

                    enabled = light.enabled;
                    name = light.name.clone();
                }

                ui.horizontal(|ui|
                {
                    ui.label("name: ");
                    changed = ui.text_edit_singleline(&mut name).changed() || changed;
                });
                changed = ui.checkbox(&mut enabled, "enabled").changed() || changed;

                if changed
                {
                    let mut light = light.borrow_mut();
                    let light = light.get_mut();

                    light.enabled = enabled;
                    light.name = name;
                }
            });

            collapse_with_title(ui, "light_settings", true, "💡 Light Settings", |ui|
            {
                Light::ui(light, ui);
            });
        }

        // delete light
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Light").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_light_by_id(light_id);
            }
        });
    }

    fn create_scene_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let scene_id = self.selected_scene_id;

        // no scene selected
        if scene_id.is_none()
        {
            return;
        }

        let scene_id = scene_id.unwrap();
        let scene = state.find_scene_by_id(scene_id);

        if scene.is_none()
        {
            return;
        }

        let scene = scene.unwrap();

        let mut instances_amout = 0;
        let mut meshes_amout = 0;
        let mut vertices_amout = 0;
        let mut indices_amout = 0;

        let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

        for node in &all_nodes
        {
            let node = node.read().unwrap();
            instances_amout += node.instances.get_ref().len();

            let mesh = node.find_component::<Mesh>();
            if let Some(mesh) = mesh
            {
                component_downcast!(mesh, Mesh);

                meshes_amout += 1;
                vertices_amout += mesh.get_data().vertices.len();
                indices_amout += mesh.get_data().indices.len();
            }
        }

        let mut memory_usage = 0.0;
        let mut gpu_memory_usage = 0.0;
        for texture in &scene.textures
        {
            let texture = texture.1.as_ref().read().unwrap();
            let texture = texture.as_ref();
            memory_usage += texture.memory_usage() as f32;
            gpu_memory_usage += texture.gpu_usage() as f32;
        }

        memory_usage = memory_usage / 1024.0 / 1024.0;
        gpu_memory_usage = gpu_memory_usage / 1024.0 / 1024.0;

        // statistics
        collapse_with_title(ui, "scene_info", true, "📈 Info", |ui|
        {
            ui.label(RichText::new("🎬 scene").strong());
            ui.label(format!(" ⚫ nodes: {}", all_nodes.len()));
            ui.label(format!(" ⚫ instances: {}", instances_amout));
            ui.label(format!(" ⚫ materials: {}", scene.materials.len()));
            ui.label(format!(" ⚫ textures: {}", scene.textures.len()));
            ui.label(format!(" ⚫ cameras: {}", scene.cameras.len()));
            ui.label(format!(" ⚫ lights: {}", scene.lights.get_ref().len()));

            ui.label(RichText::new("◼ geometry").strong());
            ui.label(format!(" ⚫ meshes: {}", meshes_amout));
            ui.label(format!(" ⚫ vertices: {}", vertices_amout));
            ui.label(format!(" ⚫ indices: {}", indices_amout));

            ui.label(RichText::new("🖴 RAM memory usage").strong());
            ui.label(format!(" ⚫ textures: {:.2} MB", memory_usage));

            ui.label(RichText::new("🖵 GPU memory usage").strong());
            ui.label(format!(" ⚫ textures: {:.2} MB", gpu_memory_usage));
            ui.label(format!(" ⚫ buffers: TODO"));
        });

        collapse_with_title(ui, "scene_debugging", true, "🐛 Debugging Settings", |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                if ui.button("save image").clicked()
                {
                    state.save_image = true;
                }

                if ui.button("save depth pass image").clicked()
                {
                    state.save_depth_pass_image = true;
                }

                if ui.button("save depth buffer image").clicked()
                {
                    state.save_depth_buffer_image = true;
                }

                if ui.button("save screenshot").clicked()
                {
                    state.save_screenshot = true;
                }
            });
        });
    }

    fn create_rendering_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // general rendering settings
        collapse_with_title(ui, "render_settings", true, "General Settings", |ui|
        {
            ui.horizontal(|ui|
            {
                let clear_color = state.rendering.clear_color.get_ref();

                let r = (clear_color.x * 255.0) as u8;
                let g = (clear_color.y * 255.0) as u8;
                let b = (clear_color.z * 255.0) as u8;
                let mut color = Color32::from_rgb(r, g, b);

                ui.label("clear color:");
                let changed = ui.color_edit_button_srgba(&mut color).changed();

                if changed
                {
                    let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                    let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                    let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                    state.rendering.clear_color.set(Vector3::<f32>::new(r, g, b));
                }
            });

            {
                let mut fullscreen = state.rendering.fullscreen.get_ref().clone();
                if ui.checkbox(&mut fullscreen, "Fullscreen").changed()
                {
                    state.rendering.fullscreen.set(fullscreen);
                }
            }

            {
                let mut v_sync = state.rendering.v_sync.get_ref().clone();
                if ui.checkbox(&mut v_sync, "vSync").changed()
                {
                    state.rendering.v_sync.set(v_sync);
                }
            }

            {
                ui.checkbox(&mut state.rendering.distance_sorting, "Distance Sorting (for better alpha blending)");
            }

            ui.horizontal(|ui|
            {
                ui.label("MSAA:");

                let mut changed = false;
                let mut msaa = *state.rendering.msaa.get_ref();

                changed = ui.selectable_value(& mut msaa, 1, "1").changed() || changed;

                if state.adapter.max_msaa_samples >= 2 { changed = ui.selectable_value(& mut msaa, 2, "2").changed() || changed; }
                if state.adapter.max_msaa_samples >= 4 { changed = ui.selectable_value(& mut msaa, 4, "4").changed() || changed; }
                if state.adapter.max_msaa_samples >= 8 { changed = ui.selectable_value(& mut msaa, 8, "8").changed() || changed; }
                if state.adapter.max_msaa_samples >= 16 { changed = ui.selectable_value(& mut msaa, 16, "16").changed() || changed; }

                if changed
                {
                    state.rendering.msaa.set(msaa)
                }
            });
        });
    //});
    }

    fn create_file_menu(&mut self, state: &mut State, ui: &mut Ui)
    {
        ui.menu_button("File", |ui|
        {
            if ui.button("Exit").clicked()
            {
                state.exit = true;
            }
        });
    }
}
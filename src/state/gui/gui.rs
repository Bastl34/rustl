use std::{cell::RefCell, fmt::format, borrow::BorrowMut, mem::swap, collections::HashMap};

use colored::Color;
use egui::{FullOutput, RichText, Color32, ScrollArea, Ui, RawInput, Visuals, Style, Align2};
use egui_plot::{Plot, BarChart, Bar, Legend, Corner};
use nalgebra::{Vector3, Point3};

use crate::{state::{state::{State, FPS_CHART_VALUES}, scene::{light::Light, components::{transformation::Transformation, material::{Material, MaterialItem}, mesh::Mesh, component::Component}, node::NodeItem, scene::Scene, camera::CameraItem}}, rendering::{egui::EGui, instance}, helper::change_tracker::ChangeTracker, component_downcast};

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
enum HierarchyType
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

pub struct Gui
{
    bottom: BottomPanel,

    settings: SettingsPanel,

    hierarchy_expand_all: bool,
    hierarchy_filter: String,

    selected_scene_id: Option<u64>,
    selected_type: HierarchyType,
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
            //side_hierarchy: HierarchyPanel::Objects,
            bottom: BottomPanel::Assets,

            settings: SettingsPanel::Rendering,

            hierarchy_expand_all: true,
            hierarchy_filter: String::new(),

            selected_scene_id: None,
            selected_type: HierarchyType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID

            dialog_add_component: false,
            add_component_id: 0,
            add_component_name: "Component".to_string()
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
                if self.selected_type == HierarchyType::Objects && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Components, " Components");
                    ui.selectable_value(&mut self.settings, SettingsPanel::Object, "◼ Object");

                    object_settings = true;
                }

                if self.selected_type == HierarchyType::Cameras && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Camera, "📷 Camera");

                    camera_settings = true;
                }

                if self.selected_type == HierarchyType::Lights && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Light, "💡 Light");

                    light_settings = true;
                }

                if self.selected_type == HierarchyType::Materials && !self.selected_object.is_empty()
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Material, "🎨 Material");

                    material_settings = true;
                }

                if self.selected_type == HierarchyType::Textures && !self.selected_object.is_empty()
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
                    SettingsPanel::Light => if light_settings { },
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
                        let mut instance_borrow = instance.borrow_mut();
                        let instance = instance_borrow.get_mut();
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
        ui.horizontal(|ui|
        {
            let mut fullscreen = state.rendering.fullscreen.get_ref().clone();

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                let mut changed = false;

                changed = ui.selectable_value(& mut fullscreen, true, RichText::new("⛶").size(20.0)).on_hover_text("fullscreen").changed() || changed;
                changed = ui.selectable_value(& mut fullscreen, false, RichText::new("🗖").size(20.0)).on_hover_text("window mode").changed() || changed;

                if changed
                {
                    state.rendering.fullscreen.set(fullscreen);
                }
            });
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
        ui.label(format!(" ⚫ fps: {}", state.last_fps));
        ui.label(format!(" ⚫ absolute fps: {}", state.fps_absolute));
        ui.label(format!(" ⚫ draw calls: {}", state.draw_calls));
        ui.label(format!(" ⚫ frame time: {:.3} ms", state.frame_time));
        ui.label(format!(" ⚫ update time: {:.3} ms", state.update_time));
        ui.label(format!(" ⚫ render time: {:.3} ms", state.render_time));

        let mut textures = 0;
        let mut materials = 0;
        for scene in &state.scenes
        {
            textures += scene.textures.len();
            materials += scene.materials.len();
        }

        ui.label(format!(" ⚫ textures: {}", textures));
        ui.label(format!(" ⚫ materials: {}", materials));
    }

    fn create_hierarchy(&mut self, state: &mut State, ui: &mut Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.hierarchy_filter).desired_width(120.0));

            ui.toggle_value(&mut self.hierarchy_expand_all, "⊞").on_hover_text("expand all items");
        });

        for scene in &state.scenes
        {
            let scene_id = scene.id;
            let id = format!("scene_{}", scene_id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, self.hierarchy_expand_all).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() && self.selected_type == HierarchyType::None { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new(format!("🎬 {}: {}", scene_id, scene.name)).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::None;
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
                self.create_hierarchy_type_entries(&scene, ui);
            });
        }
    }

    fn create_hierarchy_type_entries(&mut self, scene: &Box<Scene>, ui: &mut Ui)
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
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == HierarchyType::Objects { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("◼ Objects").color(Color32::LIGHT_GREEN).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::Objects;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = HierarchyType::None;
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
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == HierarchyType::Cameras { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("📷 Cameras").color(Color32::LIGHT_RED).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::Cameras;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = HierarchyType::None;
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
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == HierarchyType::Lights { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("💡 Lights").color(Color32::YELLOW).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::Lights;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = HierarchyType::None;
                        }
                    }
                });
            }).body(|ui|
            {

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
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == HierarchyType::Materials { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("🎨 Materials").color(Color32::GOLD).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::Materials;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = HierarchyType::None;
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
                    let mut selection; if self.selected_scene_id == Some(scene_id) && self.selected_object.is_empty() &&  self.selected_type == HierarchyType::Textures { selection = true; } else { selection = false; }
                    if ui.toggle_value(&mut selection, RichText::new("🖼 Textures").color(Color32::LIGHT_BLUE).strong()).clicked()
                    {
                        if selection
                        {
                            self.selected_scene_id = Some(scene_id);
                            self.selected_object.clear();
                            self.selected_type = HierarchyType::Textures;
                        }
                        else
                        {
                            self.selected_scene_id = None;
                            self.selected_type = HierarchyType::None;
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
                            self.selected_type = HierarchyType::Objects;

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
                let instance = instance.get_ref();
                let instance_id = instance.id;

                let id = format!("objects_{}_{}", node.id, instance_id);
                let headline_name = format!("⚫ {}: {}", instance_id, instance.name);

                let mut heading = RichText::new(headline_name);

                let visible = instance.visible && parent_visible;

                if visible
                {
                    heading = heading.strong()
                }
                else
                {
                    heading = heading.strikethrough();
                }

                if instance.highlight
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
                        self.selected_type = HierarchyType::Objects;

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

                let mut selection; if self.selected_type == HierarchyType::Materials && self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    //if self.selected_material.is_none() || (self.selected_material.is_some() && self.selected_material.unwrap() != *material_id)
                    if selection
                    {

                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = HierarchyType::Materials;
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

                let heading = RichText::new(headline_name).strong();

                let mut selection; if self.selected_type == HierarchyType::Cameras && self.selected_object == id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    if selection
                    {

                        self.selected_object = id;
                        self.selected_scene_id = Some(scene_id);
                        self.selected_type = HierarchyType::Cameras;
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
                    let instance_borrow = instance.borrow();
                    let instance = instance_borrow.get_ref();

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
                    let instance = instance.get_mut();
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

        let instance = instance.unwrap();

        // General
        collapse_with_title(ui, "instance_data", true, "ℹ Instance Data", |ui|
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();

            ui.label(format!("name: {}", instance.name));
            ui.label(format!("id: {}", instance.id));
        });

        ui.separator();

        // Settings
        let mut delete_instance = false;
        collapse_with_title(ui, "instance_settings", true, "⛭ Instance Settings", |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut highlight;
            let mut name;
            {
                let instance = instance.borrow();
                let instance = instance.get_ref();
                visible = instance.visible;
                highlight = instance.highlight;
                name = instance.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });
            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut highlight, "highlight").changed() || changed;

            if changed
            {
                let mut instance = instance.borrow_mut();
                let instance = instance.get_mut();
                instance.visible = visible;
                instance.highlight = highlight;
                instance.name = name;
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
                    name = controller.get_base().name.clone();
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
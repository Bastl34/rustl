use std::{cell::RefCell, fmt::format, borrow::BorrowMut, mem::swap};

use egui::{FullOutput, RichText, Color32, ScrollArea, Ui, RawInput, Visuals, Style};
use nalgebra::{Vector3, Point3};

use crate::{state::{state::State, scene::{light::Light, components::{transformation::Transformation, material::Material, mesh::Mesh, component::Component}, node::NodeItem, scene::Scene}}, rendering::{egui::EGui, instance}, helper::change_tracker::ChangeTracker, component_downcast};


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
enum HierarchyPanel
{
    Objects,
    Cameras,
    Lights,
    Materials,
    Textures,
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
    side_hierarchy: HierarchyPanel,
    bottom: BottomPanel,

    settings: SettingsPanel,

    selected_object: String,
    selected_camera: Option<u64>,
    selected_material:Option<u64>,
    selected_texture: Option<u64>,
    selected_light: Option<u64>
}

impl Gui
{
    pub fn new() -> Gui
    {
        Self
        {
            side_hierarchy: HierarchyPanel::Objects,
            bottom: BottomPanel::Assets,

            settings: SettingsPanel::Components,

            selected_object: String::new(), // sceneID_nodeID_instanceID
            selected_camera: None,
            selected_material: None,
            selected_texture: None,
            selected_light: None
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
        egui::SidePanel::left("my_left_panel").frame(frame).show(ctx, |ui|
        {
            self.create_left_sidebar(state, ui);

            /*
            egui::Window::new("Statistics")
                .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
                .show(ui.ctx(), |ui|
            {
                self.create_statistic(state, ui);
            });
            */
        });


        //right
        egui::SidePanel::right("right_panel").frame(frame).show(ctx, |ui|
        {
            let mut object_settings = false;
            let mut camera_settings = false;
            let mut light_settings = false;
            let mut material_settings = false;
            let mut texture_settings = false;

            ui.horizontal(|ui|
            {
                if self.side_hierarchy == HierarchyPanel::Objects
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Components, " Components");
                    ui.selectable_value(&mut self.settings, SettingsPanel::Object, "◼ Object");

                    object_settings = true;
                }

                if self.side_hierarchy == HierarchyPanel::Cameras
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Camera, "📷 Camera");

                    camera_settings = true;
                }

                if self.side_hierarchy == HierarchyPanel::Lights
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Light, "💡 Light");

                    light_settings = true;
                }

                if self.side_hierarchy == HierarchyPanel::Materials
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Material, "🎨 Material");

                    material_settings = true;
                }

                if self.side_hierarchy == HierarchyPanel::Textures
                {
                    ui.selectable_value(&mut self.settings, SettingsPanel::Texture, "🖼 Texture");

                    texture_settings = true;
                }

                ui.selectable_value(&mut self.settings, SettingsPanel::Scene, "🎬 Scene");
                ui.selectable_value(&mut self.settings, SettingsPanel::Rendering, "📷 Rendering");
            });
            ui.separator();

            ScrollArea::vertical().show(ui, |ui|
            {
                match self.settings
                {
                    SettingsPanel::Components => if object_settings { self.create_component_settings(state, ui); },
                    SettingsPanel::Object => if object_settings { self.create_object_settings(state, ui); },
                    SettingsPanel::Material => if material_settings { self.create_material_settings(state, ui); },
                    SettingsPanel::Camera => if camera_settings { },
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
        ui.separator();

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Statistics").heading().strong()).default_open(true).show(ui, |ui|
        {
            self.create_statistic(state, ui);
        });

        ui.separator();

        // hierarchy
        egui::CollapsingHeader::new(RichText::new("🗄 Hierarchy").heading().strong()).default_open(true).show(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.selectable_value(&mut self.side_hierarchy, HierarchyPanel::Objects, "◼ Objects");
                ui.selectable_value(&mut self.side_hierarchy, HierarchyPanel::Cameras, "📷 Cameras");
                ui.selectable_value(&mut self.side_hierarchy, HierarchyPanel::Lights, "💡 Lights");
                ui.selectable_value(&mut self.side_hierarchy, HierarchyPanel::Materials, "🎨 Materials");
                ui.selectable_value(&mut self.side_hierarchy, HierarchyPanel::Textures, "🖼 Textures");
            });
            ui.separator();

            match self.side_hierarchy
            {
                HierarchyPanel::Objects => self.create_objects_hierarchy(state, ui),
                HierarchyPanel::Cameras => {},
                HierarchyPanel::Lights => {},
                HierarchyPanel::Materials => self.create_materials_hierarchy(state, ui),
                HierarchyPanel::Textures => {},
            }
        });
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

    fn create_objects_hierarchy(&mut self, state: &mut State, ui: &mut Ui)
    {
        let mut filter = String::new();

        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut filter).desired_width(120.0));
        });

        ScrollArea::vertical().show(ui, |ui|
        {
            for scene in &state.scenes
            {
                let scene_id = scene.id;
                let id = format!("{}", scene_id);
                let ui_id = ui.make_persistent_id(id.clone());
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
                {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                    {
                        let mut selection; if self.selected_object == id { selection = true; } else { selection = false; }
                        if ui.toggle_value(&mut selection, format!("🎬 {}: {}", scene_id, scene.name)).clicked()
                        {
                            if self.selected_object != id
                            {
                                self.selected_object = id;
                            }
                            else
                            {
                                self.selected_object.clear();
                            }
                        }
                    });
                }).body(|ui|
                {
                    self.build_node_list(ui, &scene.nodes, scene_id, true);
                });
            }

            ui.separator();
        });
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

            let id = format!("{}_{}", scene_id, node_id);
            let ui_id = ui.make_persistent_id(id.clone());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
                        }
                        else
                        {
                            self.selected_object.clear();
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

                let id = format!("{}_{}_{}", scene_id, node.id, instance_id);
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
                    }
                    else
                    {
                        self.selected_object.clear();
                    }
                }
            }
        });
    }

    fn create_materials_hierarchy(&mut self, state: &mut State, ui: &mut Ui)
    {
        let mut filter = String::new();

        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut filter).desired_width(120.0));
        });

        ScrollArea::vertical().show(ui, |ui|
        {
            for scene in &state.scenes
            {
                let scene_id = scene.id;
                let id = format!("{}", scene_id);
                let ui_id = ui.make_persistent_id(id.clone());
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
                {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                    {
                        let mut selection = false;
                        ui.toggle_value(&mut selection, format!("🎬 {}: {}", scene_id, scene.name));
                    });
                }).body(|ui|
                {
                    self.build_material_list(state, ui, scene_id);
                });
            }

            ui.separator();
        });
    }

    pub fn build_material_list(&mut self, state: &State, ui: &mut Ui, scene_id: u64)
    {
        let scene = state.find_scene_by_id(scene_id).unwrap();

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            for (material_id, material) in &scene.materials
            {
                let material = material.read().unwrap();
                let headline_name = format!("🎨 {}: {}", material_id, material.get_base().name);

                let heading = RichText::new(headline_name).strong();

                let mut selection; if self.selected_material.is_some() && self.selected_material.unwrap() == *material_id { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, heading).clicked()
                {
                    if self.selected_material.is_none() || (self.selected_material.is_some() && self.selected_material.unwrap() != *material_id)
                    {
                        self.selected_material = Some(*material_id);
                    }
                    else
                    {
                        self.selected_material = None;
                    }
                }
            }
        });
    }

    fn get_object_ids(&self) -> (Option<u64>, Option<u64>, Option<u64>)
    {
        // no scene selected
        if self.selected_object.is_empty()
        {
            return (None, None, None);
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();

        let mut scene_id: Option<u64> = None;
        let mut node_id: Option<u64> = None;
        let mut instance_id: Option<u64> = None;

        if parts.len() >= 1
        {
            scene_id = Some(parts.get(0).unwrap().parse().unwrap());
        }

        if parts.len() >= 2
        {
            node_id = Some(parts.get(1).unwrap().parse().unwrap());
        }

        if parts.len() >= 3
        {
            instance_id = Some(parts.get(2).unwrap().parse().unwrap());
        }

        (scene_id, node_id, instance_id)
    }

    fn create_component_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (scene_id, node_id, instance_id) = self.get_object_ids();

        if scene_id.is_none() || node_id.is_none()
        {
            return;
        }

        let scene_id: u64 = scene_id.unwrap();
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
        {
            let mut node = node.write().unwrap();
            for component in &mut node.components
            {
                let name;
                {
                    let component = component.read().unwrap();
                    let base = component.get_base();
                    name = format!("{} {} ({})", base.icon, base.component_name, base.name);
                }
                egui::CollapsingHeader::new(RichText::new(name).heading().strong()).default_open(true).show(ui, |ui|
                {
                    let mut component = component.write().unwrap();
                    component.ui(ui);
                });

                ui.separator();
            }
        }

        if let Some(instance_id) = instance_id
        {
            let node = node.read().unwrap();
            let instance = node.find_instance_by_id(instance_id);

            if let Some(instance) = instance
            {
                // alpha
                {
                    let name;
                    {
                        let instance = instance.borrow();
                        let instance = instance.get_ref();
                        let component = &instance.alpha;

                        let base = component.get_base();
                        name = format!("{} Instance {} ({})", base.icon, base.component_name, base.name);
                    }
                    // WARNING: if shown a buffer update is triggered -> also if there is no change
                    // thats why its not visible as default
                    egui::CollapsingHeader::new(RichText::new(name).heading().strong()).default_open(false).show(ui, |ui|
                    {
                        let mut instance = instance.borrow_mut();
                        let instance = instance.get_mut();
                        let component = &mut instance.alpha;

                        ui.label(RichText::new("Info: If this component is visble: The instance buffer update is performed on every frame.").color(ui.visuals().warn_fg_color));
                        component.ui(ui);
                    });

                    ui.separator();
                }

                // transformation
                {
                    let name;
                    {
                        let instance = instance.borrow();
                        let instance = instance.get_ref();
                        let component = &instance.transform;

                        let base = component.get_base();
                        name = format!("{} Instance {} ({})", base.icon, base.component_name, base.name);
                    }
                    // WARNING: if shown a buffer update is triggered -> also if there is no change
                    // thats why its not visible as default
                    egui::CollapsingHeader::new(RichText::new(name).heading().strong()).default_open(false).show(ui, |ui|
                    {
                        let mut instance = instance.borrow_mut();
                        let instance = instance.get_mut();
                        let component = &mut instance.transform;

                        ui.label(RichText::new("Info: If this component is visble: The instance buffer update is performed on every frame.").color(ui.visuals().warn_fg_color));
                        component.ui(ui);
                    });

                    ui.separator();
                }
            }
        }

    }

    fn create_object_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (scene_id, node_id, instance_id) = self.get_object_ids();

        // no scene selected
        if scene_id.is_none() || node_id.is_none()
        {
            return;
        }

        let scene_id: u64 = scene_id.unwrap();
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
        egui::CollapsingHeader::new(RichText::new("ℹ Object Data").heading().strong()).default_open(true).show(ui, |ui|
        {
            {
                let node = node.read().unwrap();

                ui.label(format!("name: {}", node.name));
                ui.label(format!("id: {}", node.id));
            }
        });

        ui.separator();

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Object Info").heading().strong()).default_open(true).show(ui, |ui|
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

        ui.separator();

        // Settings
        egui::CollapsingHeader::new(RichText::new("⛭ Object Settings").heading().strong()).default_open(true).show(ui, |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut render_children_first;
            let mut name;
            {
                let node = node.read().unwrap();
                visible = node.visible;
                render_children_first = node.render_children_first;
                name = node.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });
            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut render_children_first, "render children first").changed() || changed;

            if changed
            {
                let mut node = node.write().unwrap();
                node.visible = visible;
                node.render_children_first = render_children_first;
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


        ui.separator();

        if let Some(instance_id) = instance_id
        {
            self.create_instance_settings(state, scene_id, node, instance_id, ui);
        }
    }

    fn create_material_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (scene_id, ..) = self.get_object_ids();

        // no scene selected
        if scene_id.is_none() { return; }
        let scene_id: u64 = scene_id.unwrap();

        let scene = state.find_scene_by_id(scene_id);
        if scene.is_none() { return; }

        let scene = scene.unwrap();

        if self.selected_material.is_none() { return; }
        let material_id = self.selected_material.unwrap();

        if let Some(material) = scene.get_material_by_id(material_id)
        {
            egui::CollapsingHeader::new(RichText::new("🎨 Material Settings").heading().strong()).default_open(true).show(ui, |ui|
            {
                let mut material = material.write().unwrap();
                material.ui(ui);
            });
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
        egui::CollapsingHeader::new(RichText::new("ℹ Instance Data").heading().strong()).default_open(true).show(ui, |ui|
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();

            ui.label(format!("name: {}", instance.name));
            ui.label(format!("id: {}", instance.id));
        });

        ui.separator();

        // Settings
        let mut delete_instance = false;
        egui::CollapsingHeader::new(RichText::new("⛭ Instance Settings").heading().strong()).default_open(true).show(ui, |ui|
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

    fn create_scene_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        let (scene_id, ..) = self.get_object_ids();

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

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Info").heading().strong()).default_open(true).show(ui, |ui|
        {
            ui.label(format!("nodes: {}", all_nodes.len()));
            ui.label(format!("instances: {}", instances_amout));
            ui.label(format!("materials: {}", scene.materials.len()));
            ui.label(format!("textures: {}", scene.textures.len()));
            ui.label(format!("cameras: {}", scene.cameras.len()));
            ui.label(format!("lights: {}", scene.lights.get_ref().len()));

            ui.separator();

            ui.label(format!("meshes: {}", meshes_amout));
            ui.label(format!("vertices: {}", vertices_amout));
            ui.label(format!("indices: {}", indices_amout));
        });

        ui.separator();

        egui::CollapsingHeader::new(RichText::new("🐛 Debugging Settings").heading().strong()).default_open(true).show(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("instances:");
                ui.add(egui::Slider::new(&mut state.instances, 1..=10000));
            });

            ui.horizontal(|ui|
            {
                ui.label("rotation speed:");
                ui.add(egui::Slider::new(&mut state.rotation_speed, 0.0..=2.0));
            });

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
        egui::CollapsingHeader::new(RichText::new("General Settings").heading().strong()).default_open(true).show(ui, |ui|
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
use std::{cell::RefCell, fmt::format, borrow::BorrowMut};

use egui::{FullOutput, RichText, Color32, ScrollArea, Ui, RawInput, Visuals, Style};
use nalgebra::{Vector3, Point3};

use crate::{state::{state::State, scene::{light::Light, components::{transformation::Transformation, material::Material, mesh::Mesh}, node::NodeItem, scene::Scene}}, rendering::{egui::EGui, instance}, helper::change_tracker::ChangeTracker};


#[derive(PartialEq, Eq)]
enum SettingsPanel
{
    Components,
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

    selected_object: String
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

            selected_object: String::new() // sceneID_nodeID_instanceID
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
            ui.horizontal(|ui|
            {
                ui.selectable_value(&mut self.settings, SettingsPanel::Components, " Components");
                ui.selectable_value(&mut self.settings, SettingsPanel::Object, "🎬 Object");
                ui.selectable_value(&mut self.settings, SettingsPanel::Scene, "🎬 Scene");
                ui.selectable_value(&mut self.settings, SettingsPanel::Rendering, "📷 Rendering");
            });
            ui.separator();

            match self.settings
            {
                SettingsPanel::Components => self.create_component_settings(state, ui),
                SettingsPanel::Object => self.create_object_settings(state, ui),
                SettingsPanel::Scene => self.create_scene_settings(state, ui),
                SettingsPanel::Rendering => self.create_rendering_settings(state, ui),
            }
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

                changed = ui.selectable_value(& mut fullscreen, false, RichText::new("⛶").size(20.0)).on_hover_text("fullscreen").changed() || changed;
                changed = ui.selectable_value(& mut fullscreen, true, RichText::new("🗖").size(20.0)).on_hover_text("window mode").changed() || changed;

                if changed
                {
                    state.rendering.fullscreen.set(fullscreen);
                }
            });
        });
    }

    fn create_statistic(&mut self, state: &mut State, ui: &mut Ui)
    {
        ui.label(format!("fps: {}", state.last_fps));
        ui.label(format!("absolute fps: {}", state.fps_absolute));
        ui.label(format!("draw calls: {}", state.draw_calls));
        ui.label(format!("frame time: {:.3} ms", state.frame_time));
        ui.label(format!("update time: {:.3} ms", state.update_time));
        ui.label(format!("render time: {:.3} ms", state.render_time));

        let mut textures = 0;
        let mut materials = 0;
        for scene in &state.scenes
        {
            textures += scene.textures.len();
            materials += scene.materials.len();
        }

        ui.label(format!("textures: {}", textures));
        ui.label(format!("materials: {}", materials));
    }

    fn create_left_sidebar(&mut self, state: &mut State, ui: &mut Ui)
    {
        ui.separator();

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Statistics").heading()).default_open(true).show(ui, |ui|
        {
            self.create_statistic(state, ui);
        });

        ui.separator();

        // hierarchy
        egui::CollapsingHeader::new(RichText::new("🗄 Hierarchy").heading()).default_open(true).show(ui, |ui|
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
                HierarchyPanel::Materials => {},
                HierarchyPanel::Textures => {},
            }
        });


        /*
        let mut selected = false;
        let mut filter = String::new();

        //ui.set_min_width(ui.max_rect().width());
        ui.set_min_width(ui.available_width());
        //ui.toggle_value(&mut selected, "lol")
        */

        /*
        ui.button("  item 1.1").highlight();
        ui.button("  item 1.2").highlight();
        ui.button("item 2").highlight();
        */

        /*
        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut filter).desired_width(120.0));
        });
        */

        /*
        ScrollArea::vertical().show(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                ui.toggle_value(&mut selected, "⏷ ◻ 0: item 1");
                ui.toggle_value(&mut selected, "   ⏷ ◼ 1: item 1.1");
                ui.toggle_value(&mut selected, "      ⏵ ⚫ 2: item 1.1.1");
                ui.toggle_value(&mut selected, "      ⏵ ⚫ 3: item 1.1.2");
                ui.toggle_value(&mut selected, "      ⏵ ⚫ 4: item 1.1.3");
                ui.toggle_value(&mut selected, "   ⏷ ◻ 5: item 1.2");
                ui.toggle_value(&mut selected, "⏷ ◼ 6: item 2");
            });
        });
        */

        /*
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            egui::Frame::none().fill(egui::Color32::RED).show(ui, |ui|
            {
                ui.label("Label with red background");
            });
        });

        //if ui.add(egui::Label::new("hover me!").sense(egui::Sense::hover())).hovered()
        if ui.add(egui::Button::new("test").sense(egui::Sense::hover())).hovered()
        {
            //ui.ctx().set_visuals(egui::Visuals::dark());
        } else {
           // ui.ctx().set_visuals(egui::Visuals::light());
        };
        */


        /*
            let id = ui.make_persistent_id("my_collapsing_header");
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false).show_header(ui, |ui|
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    ui.toggle_value(&mut self.test_selection, "◻ 0: item 1");
                });
            })
            .body(|ui|
            {
                let id = ui.make_persistent_id("my_collapsing_header_2");
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false).show_header(ui, |ui|
                {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                    {
                        ui.toggle_value(&mut self.test_selection, "◼ 1: item 1.1");
                    });
                })
                .body(|ui| ui.label("Body"));
            });
        */

        /*
        let size = Vec2::splat(2.0 * r + 5.0);
        let (rect, _response) = ui.allocate_at_least(size, Sense::hover());
        ui.painter().rect(rect, rounding, fill_color, stroke)
        */


        /*
        let id = ui.make_persistent_id("my_collapsing_header");
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

        state.show_header(ui, |ui| {
            if ui.button("test").clicked()
            {
                state.toggle(ui);
            }
        })
        .body(|ui| ui.label("Body"));

        */

        /*
        let id = ui.make_persistent_id("my_collapsing_header");
        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);


        collapsing.show_default_button_with_size(ui, button_size);

        let test = collapsing.show_body_unindented(ui, |ui|
        {
            ui.label("lol test");
        });

        */

        /*
        state.show_header(ui, |ui| {
                ui.label("Header"); // Sie können hier auch Kontrollkästchen oder ähnliches hinzufügen
            })
            .body(|ui| ui.label("Body"));
        */
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
                    heading = heading.color(Color32::from_rgb(255, 200, 200));
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

    fn create_component_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_object.is_empty()
        {
            return;
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();
        let scene_id: usize = parts.get(0).unwrap().parse().unwrap();
    }

    fn create_object_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_object.is_empty()
        {
            return;
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();

        if parts.len() < 2
        {
            return;
        }

        let scene_id: u64 = parts.get(0).unwrap().parse().unwrap();
        let node_id: u64 = parts.get(1).unwrap().parse().unwrap();
        let instance_id = parts.get(2);

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

            let mesh = node.find_component::<Mesh>();
            if let Some(mesh) = mesh
            {
                direct_meshes_amout += 1;
                direct_vertices_amout += mesh.get_data().vertices.len();
                direct_indices_amout += mesh.get_data().indices.len();
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
                    all_meshes_amout += 1;
                    all_vertices_amout += mesh.get_data().vertices.len();
                    all_indices_amout += mesh.get_data().indices.len();
                }
            }
        }

        // General
        egui::CollapsingHeader::new(RichText::new("ℹ Object Data").heading()).default_open(true).show(ui, |ui|
        {
            {
                let node = node.read().unwrap();

                ui.label(format!("name: {}", node.name));
                ui.label(format!("id: {}", node.id));
            }
        });

        ui.separator();

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Object Info").heading()).default_open(true).show(ui, |ui|
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
        egui::CollapsingHeader::new(RichText::new("⛭ Object Settings").heading()).default_open(true).show(ui, |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut render_children_first;
            {
                let node = node.read().unwrap();
                visible = node.visible;
                render_children_first = node.render_children_first;
            }

            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut render_children_first, "render children first").changed() || changed;

            if changed
            {
                let mut node = node.write().unwrap();
                node.visible = visible;
                node.render_children_first = render_children_first;
            }
        });

        ui.separator();

        if let Some(instance_id) = instance_id
        {
            let instance_id = instance_id.parse().unwrap();
            self.create_instance_settings(state, node, instance_id, ui);
        }
    }

    fn create_instance_settings(&mut self, state: &mut State, node: NodeItem, instance_id: u64 , ui: &mut Ui)
    {
        let node = node.read().unwrap();
        let instance = node.find_instance_by_id(instance_id);

        if instance.is_none()
        {
            return;
        }

        let instance = instance.unwrap();

        // General
        egui::CollapsingHeader::new(RichText::new("ℹ Instance Data").heading()).default_open(true).show(ui, |ui|
        {
            let instance = instance.borrow();
            let instance = instance.get_ref();

            ui.label(format!("name: {}", instance.name));
            ui.label(format!("id: {}", instance.id));
        });

        ui.separator();

        // Settings
        egui::CollapsingHeader::new(RichText::new("⛭ Instance Settings").heading()).default_open(true).show(ui, |ui|
        {
            let mut changed = false;

            let mut visible;
            let mut highlight;
            {
                let instance = instance.borrow();
                let instance = instance.get_ref();
                visible = instance.visible;
                highlight = instance.highlight;
            }

            changed = ui.checkbox(&mut visible, "visible").changed() || changed;
            changed = ui.checkbox(&mut highlight, "highlight").changed() || changed;

            if changed
            {
                let mut instance = instance.borrow_mut();
                let instance = instance.get_mut();
                instance.visible = visible;
                instance.highlight = highlight;
            }
        });
    }

    fn create_scene_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // no scene selected
        if self.selected_object.is_empty()
        {
            return;
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();
        let scene_id: u64 = parts.get(0).unwrap().parse().unwrap();

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
                meshes_amout += 1;
                vertices_amout += mesh.get_data().vertices.len();
                indices_amout += mesh.get_data().indices.len();
            }
        }

        // statistics
        egui::CollapsingHeader::new(RichText::new("📈 Info").heading()).default_open(true).show(ui, |ui|
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

        egui::CollapsingHeader::new(RichText::new("🐛 Debugging Settings").heading()).default_open(true).show(ui, |ui|
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
        });
    }

    fn create_rendering_settings(&mut self, state: &mut State, ui: &mut Ui)
    {
        // general rendering settings
        //ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        //{
        //ui.collapsing(RichText::new("General").heading(), |ui|
        //{
        egui::CollapsingHeader::new(RichText::new("General Settings").heading()).default_open(true).show(ui, |ui|
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
            /*
            if ui.button("Open...").clicked()
            {
                ui.close_menu();
            }
            ui.menu_button("SubMenu", |ui|
            {
                ui.menu_button("SubMenu", |ui|
                {
                    if ui.button("Open...").clicked()
                    {
                        ui.close_menu();
                    }
                    let _ = ui.button("Item");
                });
                ui.menu_button("SubMenu", |ui|
                {
                    if ui.button("Open...").clicked()
                    {
                        ui.close_menu();
                    }
                    let _ = ui.button("Item");
                });
                let _ = ui.button("Item");
                if ui.button("Open...").clicked()
                {
                    ui.close_menu();
                }
            });
            ui.menu_button("SubMenu", |ui|
            {
                let _ = ui.button("Item1");
                let _ = ui.button("Item2");
                let _ = ui.button("Item3");
                let _ = ui.button("Item4");
                if ui.button("Open...").clicked()
                {
                    ui.close_menu();
                }
            });
             */
            if ui.button("Exit").clicked()
            {
                state.exit = true;
            }
        });
    }
}




pub fn build_gui_old(state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
{
    let raw_input = egui.ui_state.take_egui_input(window);

    let full_output = egui.ctx.run(raw_input, |ctx|
    {
        egui::Window::new("Settings").show(ctx, |ui|
        {
            ui.label(format!("fps: {}", state.last_fps));
            ui.label(format!("absolute fps: {}", state.fps_absolute));
            ui.label(format!("draw calls: {}", state.draw_calls));
            ui.label(format!("frame time: {:.3} ms", state.frame_time));
            ui.label(format!("update time: {:.3} ms", state.update_time));
            ui.label(format!("render time: {:.3} ms", state.render_time));

            let mut textures = 0;
            let mut materials = 0;
            for scene in &state.scenes
            {
                textures += scene.textures.len();
                materials += scene.materials.len();
            }

            ui.label(format!("textures: {}", textures));
            ui.label(format!("materials: {}", materials));

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

            // camera stuff
            let mut cams: Vec<(usize, usize, String, Point3<f32>, f32)> = vec![];

            for (s, scene) in state.scenes.iter().enumerate()
            {
                for (c, cam) in scene.cameras.iter().enumerate()
                {
                    let cam = cam.borrow();
                    let cam = cam.get_ref();

                    cams.push((s, c, cam.name.clone(), cam.eye_pos.clone(), cam.fovy));
                }
            }

            for cam in cams.iter_mut()
            {
                let (scene_id, cam_id, name, pos, mut fov) = cam;

                fov = fov.to_degrees();

                ui.horizontal(|ui|
                {
                    let mut changed = false;

                    ui.label(name.as_str());
                    changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                    changed = ui.add(egui::Slider::new(&mut fov, 0.0..=90.0)).changed() || changed;

                    if changed
                    {
                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        let cam = scene.cameras.get(cam_id.clone()).unwrap();
                        let mut cam = cam.borrow_mut();
                        let cam = cam.get_mut();

                        cam.eye_pos = pos.clone();
                        cam.fovy = fov.to_radians();
                        cam.init_matrices();
                    }

                    if ui.button("🗑").clicked()
                    {
                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        scene.cameras.remove(cam_id.clone());
                    }
                });
            }

            let mut lights: Vec<(usize, usize, String, Point3<f32>, Color32)> = vec![];

            for (s, scene) in state.scenes.iter().enumerate()
            {
                for (l, light) in scene.lights.get_ref().iter().enumerate()
                {
                    let light = light.borrow();
                    let light = light.get_ref();

                    let r = (light.color.x * 255.0) as u8;
                    let g = (light.color.y * 255.0) as u8;
                    let b = (light.color.z * 255.0) as u8;
                    let color = Color32::from_rgb(r, g, b);

                    lights.push((s, l, light.name.clone(), light.pos.clone(), color));
                }
            }

            for light in lights.iter_mut()
            {
                let (scene_id, light_id, name, pos, mut color) = light;

                ui.horizontal(|ui|
                {
                    let mut changed = false;

                    ui.label(name.as_str());
                    changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                    changed = ui.color_edit_button_srgba(&mut color).changed() || changed;

                    if changed
                    {
                        let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                        let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                        let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);

                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        let light = scene.lights.get_ref().get(light_id.clone()).unwrap();
                        let mut light = light.borrow_mut();
                        let light = light.get_mut();
                        light.pos = pos.clone();
                        light.color = Vector3::<f32>::new(r, g, b);
                    }

                    if ui.button("🗑").clicked()
                    {
                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        scene.lights.get_mut().remove(light_id.clone());
                    }
                });
            }

            ui.horizontal(|ui|
            {
                ui.label("add light: ");
                if ui.button("+").clicked()
                {
                    let scene = state.scenes.get_mut(0).unwrap();

                    let light_id = scene.id_manager.get_next_light_id();
                    let light = Light::new_point(light_id, "Point".to_string(), Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                    scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                }
            });

            // scene items
            ui.separator();

            let scene_id = 0;

            let scene = state.scenes.get_mut(scene_id.clone());

            if let Some(scene) = scene
            {
                let scroll_area = ScrollArea::vertical().max_height(200.0).auto_shrink([false; 2]);
                scroll_area.show(ui, |ui|
                {
                    build_node_list(ui, &scene.nodes);
                });
            }

            // just some tests
            ui.horizontal(|ui|
            {
                let mut fullscreen = state.rendering.fullscreen.get_ref().clone();

                let mut changed = ui.selectable_value(& mut fullscreen, true, RichText::new("⛶").size(20.0)).changed();
                changed = ui.selectable_value(& mut fullscreen, false, RichText::new("↕").size(20.0)).changed() || changed;

                if changed
                {
                    state.rendering.fullscreen.set(fullscreen);
                }
            });

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

    let platform_output = full_output.platform_output.clone();

    egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);

    full_output
}

pub fn build_node_list(ui: &mut Ui, nodes: &Vec<NodeItem>)
{
    for node in nodes
    {
        let mut node = node.write().unwrap();
        let child_nodes = &node.nodes.clone();

        let mut visible = node.visible;
        let mut highlight;
        {
            // use first instance for now
            if let Some(instance) = node.instances.get_ref().get(0)
            {
                highlight = instance.borrow().get_ref().highlight;
            }
            else
            {
                highlight = false;
            }
        }

        let name = node.name.clone();
        let id = node.id;
        let trans_component = node.find_component_mut::<Transformation>();

        if let Some(trans_component) = trans_component
        {
            let mut changed = false;

            let mut pos;
            let mut rot;
            let mut scale;
            {
                let data = trans_component.get_data();

                pos = data.position;
                rot = data.rotation;
                scale = data.scale;

                let coll_name = format!("{}: {}", id, name.clone());


                let heading;
                if visible
                {
                    heading = RichText::new(coll_name).strong()
                }
                else
                {
                    heading = RichText::new(coll_name).strikethrough();
                }

                ui.collapsing(heading, |ui|
                {
                    ui.vertical(|ui|
                    {
                        changed = ui.checkbox(&mut visible, "visible").changed() || changed;
                        changed = ui.checkbox(&mut highlight, "highlight").changed() || changed;

                        ui.horizontal(|ui|
                        {
                            ui.label("pos");
                            changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                        });
                        ui.horizontal(|ui|
                        {
                            ui.label("rotation");
                            changed = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed() || changed;
                        });
                        ui.horizontal(|ui|
                        {
                            ui.label("scale");
                            changed = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed() || changed;
                            changed = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed() || changed;

                            // scale = 0 is not supported / working -> otherwise a inverse transform can not be created
                            if scale.x == 0.0 { scale.x = 0.00000001; }
                            if scale.y == 0.0 { scale.y = 0.00000001; }
                            if scale.z == 0.0 { scale.z = 0.00000001; }
                        });

                    });

                    ui.separator();

                    build_node_list(ui, child_nodes);
                });
            }

            if changed
            {
                let data = trans_component.get_data_mut();
                data.get_mut().position = pos;
                data.get_mut().rotation = rot;
                data.get_mut().scale = scale;
                trans_component.calc_transform();

                node.visible = visible;

                // highlight
                if let Some(instance) = node.instances.get_ref().get(0)
                {
                    instance.borrow_mut().get_mut().highlight = highlight;
                }
            }
        }
    }
}
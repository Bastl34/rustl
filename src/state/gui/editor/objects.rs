use std::fmt::format;

use egui::{Ui, RichText, Color32};

use crate::{state::{scene::{node::NodeItem, components::{mesh::Mesh, material::Material, joint::Joint, animation::Animation}, scene::Scene}, state::State, gui::helper::generic_items::{collapse_with_title, self}}, component_downcast};

use super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_objects_list(editor_state: &mut EditorState, scene: &mut Box<Scene>, ui: &mut Ui, nodes: &Vec<NodeItem>, scene_id: u64, parent_visible: bool)
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
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let headline_name: String;
                if node.find_component::<Animation>().is_some()
                {
                    headline_name = format!("🎞 {}: {}", node_id, name.clone());
                }
                else if node.find_component::<Joint>().is_some()
                {
                    headline_name = format!("🕱 {}: {}", node_id, name.clone());
                }
                else if node.is_empty()
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

                let mut selection; if editor_state.selected_object == id { selection = true; } else { selection = false; }

                let mut toggle = ui.toggle_value(&mut selection, heading);

                if node.find_component::<Animation>().is_some()
                {
                    toggle = toggle.context_menu(|ui|
                    {
                        if ui.button("⏵ Start all Animations").clicked()
                        {
                            ui.close_menu();
                            node.start_all_animations();
                        }

                        if ui.button("⏹ Stop all Animations").clicked()
                        {
                            ui.close_menu();
                            node.stop_all_animations();
                        }
                    });
                }

                if toggle.clicked()
                {
                    if editor_state.pick_mode == SelectionType::Camera
                    {
                        if let Some(node) = scene.find_node_by_id(node_id)
                        {
                            let (camera_id, ..) = editor_state.get_object_ids();
                            if let Some(camera_id) = camera_id
                            {
                                let camera = scene.get_camera_by_id_mut(camera_id).unwrap();
                                camera.node = Some(node.clone());
                            }
                        }
                        editor_state.pick_mode = SelectionType::None;
                    }
                    else
                    {
                        if editor_state.selected_object != id
                        {
                            editor_state.selected_object = id;
                            editor_state.selected_scene_id = Some(scene_id);
                            editor_state.selected_type = SelectionType::Object;

                            if editor_state.settings != SettingsPanel::Components && editor_state.settings != SettingsPanel::Object
                            {
                                editor_state.settings = SettingsPanel::Components;
                            }
                        }
                        else
                        {
                            editor_state.selected_object.clear();
                            editor_state.selected_scene_id = None;
                        }
                    }
                }
            });

        }).body(|ui|
        {
            if child_nodes.len() > 0
            {
                build_objects_list(editor_state, scene, ui, child_nodes, scene_id, visible);
            }

            if node.instances.get_ref().len() > 0
            {
                build_instances_list(editor_state, ui, node_arc.clone(), scene_id, visible);
            }
        });
    }
}

pub fn build_instances_list(editor_state: &mut EditorState, ui: &mut Ui, node: NodeItem, scene_id: u64, parent_visible: bool)
{
    let node = node.read().unwrap();

    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for instance in node.instances.get_ref()
        {
            let instance = instance.read().unwrap();
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

            let mut selection; if editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if editor_state.selected_object != id
                {
                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Object;

                    if editor_state.settings != SettingsPanel::Components && editor_state.settings != SettingsPanel::Object
                    {
                        editor_state.settings = SettingsPanel::Components;
                    }
                }
                else
                {
                    editor_state.selected_object.clear();
                    editor_state.selected_scene_id = None;
                }
            }
        }
    });
}


pub fn create_object_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let (node_id, instance_id) = editor_state.get_object_ids();

    // no scene selected
    if editor_state.selected_scene_id.is_none() || node_id.is_none()
    {
        return;
    }

    let scene_id: u64 = editor_state.selected_scene_id.unwrap();
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
    let mut direct_faces_amout = 0;
    let direct_childs_amount;

    let mut all_instances_amout = 0;
    let mut all_meshes_amout = 0;
    let mut all_vertices_amout = 0;
    let mut all_faces_amout = 0;
    let all_childs_amount;

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
                direct_faces_amout += mesh.get_data().indices.len();
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
                all_faces_amout += mesh.get_data().indices.len();
            }
        }
    }

    let bounding_box_info = node.read().unwrap().get_bounding_info(true);

    // General
    collapse_with_title(ui, "object_data", true, "ℹ Object Data", |ui|
    {
        {
            let node = node.read().unwrap();

            ui.label(format!("Name: {}", node.name));
            ui.label(format!("Id: {}", node.id));

            if let Some(bounding_box_info) = bounding_box_info
            {
                ui.label(format!("B-Box min: x={:.3} y={:.3} z={:.3}", bounding_box_info.0.x, bounding_box_info.0.y, bounding_box_info.0.z));
                ui.label(format!("B-Box max: x={:.3} y={:.3} z={:.3}", bounding_box_info.1.x, bounding_box_info.1.y, bounding_box_info.1.z));
            }
        }
    });

    // Extras
    collapse_with_title(ui, "object_extras", true, "⊞ Extras", |ui|
    {
        ui.scope(|ui|
        {
            let node = node.read().unwrap();

            for (key, value) in &node.extras
            {
                ui.label(format!("⚫ {}: {}", key, value));
            }
        });
    });

    // Skeleton
    if let Some(skin_node) = node.read().unwrap().skin.first()
    {
        collapse_with_title(ui, "object_skeleton", true, "🕱 Skeleton", |ui|
        {
            ui.label(format!("Joints: {}", node.read().unwrap().skin.len()));
            ui.horizontal(|ui|
            {
                ui.label("Link to Skeleton: ");
                if ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to skeleton").clicked()
                {
                    editor_state.de_select_current_item(state);

                    editor_state.selected_object = format!("objects_{}", skin_node.read().unwrap().id);
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Object;
                }
            });
        });
    }

    // statistics
    collapse_with_title(ui, "object_info", true, "📈 Object Info", |ui|
    {
        ui.label(RichText::new("👤 own").strong());
        ui.label(format!(" ⚫ instances: {}", direct_instances_amout));
        ui.label(format!(" ⚫ nodes: {}", direct_childs_amount));
        ui.label(format!(" ⚫ meshes: {}", direct_meshes_amout));
        ui.label(format!(" ⚫ vertices: {}", direct_vertices_amout));
        ui.label(format!(" ⚫ faces: {}", direct_faces_amout));
        ui.label(format!(" ⚫ indices: {}", direct_faces_amout * 3));

        ui.label(RichText::new("👪 all descendants").strong());
        ui.label(format!(" ⚫ instances: {}", all_instances_amout));
        ui.label(format!(" ⚫ nodes: {}", all_childs_amount));
        ui.label(format!(" ⚫ meshes: {}", all_meshes_amout));
        ui.label(format!(" ⚫ vertices: {}", all_vertices_amout));
        ui.label(format!(" ⚫ faces: {}", all_faces_amout));
        ui.label(format!(" ⚫ indices: {}", all_faces_amout * 3));
    });

    // Settings
    collapse_with_title(ui, "object_settings", true, "⛭ Object Settings", |ui|
    {
        let mut changed = false;

        let mut visible;
        let mut root_node: bool;
        let mut render_children_first;
        let mut alpha_index;
        let mut pick_bbox_first;
        let mut name;
        {
            let node = node.read().unwrap();
            visible = node.visible;
            root_node = node.root_node;
            render_children_first = node.render_children_first;
            alpha_index = node.alpha_index;
            pick_bbox_first = node.pick_bbox_first;
            name = node.name.clone();
        }

        ui.horizontal(|ui|
        {
            ui.label("name: ");
            changed = ui.text_edit_singleline(&mut name).changed() || changed;
        });
        changed = ui.checkbox(&mut visible, "visible").changed() || changed;
        changed = ui.checkbox(&mut root_node, "root node").changed() || changed;
        changed = ui.checkbox(&mut render_children_first, "render children first").changed() || changed;
        ui.horizontal(|ui|
        {
            ui.label("alpha index: ");
            changed = ui.add(egui::DragValue::new(&mut alpha_index).speed(1)).changed() || changed;
        });
        changed = ui.checkbox(&mut pick_bbox_first, "pick bbox first").changed() || changed;

        if changed
        {
            let mut node = node.write().unwrap();
            node.visible = visible;
            node.root_node = root_node;
            node.render_children_first = render_children_first;
            node.alpha_index = alpha_index;
            node.pick_bbox_first = pick_bbox_first;
            node.name = name;
        }

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Create Default Instance").heading().strong().color(Color32::LIGHT_GREEN)).clicked()
            {
                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                let id = scene.id_manager.write().unwrap().get_next_instance_id();
                node.write().unwrap().create_default_instance(node.clone(), id);
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
        create_instance_settings(editor_state, state, scene_id, node, instance_id, ui);
    }
}

pub fn create_instance_settings(editor_state: &mut EditorState, state: &mut State, scene_id: u64, node_arc: NodeItem, instance_id: u64 , ui: &mut Ui)
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
        let instance = instance.read().unwrap();

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
            let instance = instance.read().unwrap();
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
            let mut instance = instance.write().unwrap();
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

pub fn create_component_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let (node_id, instance_id) = editor_state.get_object_ids();

    if editor_state.selected_scene_id.is_none() || node_id.is_none()
    {
        return;
    }

    let scene_id: u64 = editor_state.selected_scene_id.unwrap();
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

        let all_components;
        let all_components_clone;
        {
            let node_read = node.read().unwrap();
            all_components = node_read.components.clone();
            all_components_clone = node_read.components.clone();
        }

        for (component_i, component) in all_components.iter().enumerate()
        {
            let component_id;
            let name;
            let component_name;
            let is_material;
            {
                let component = component.read().unwrap();
                let base = component.get_base();
                component_name = format!("{} {}", base.icon, base.component_name);
                name = base.name.clone();
                component_id = component.id();

                is_material = component.as_any().downcast_ref::<Material>().is_some();
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

                    // link to the texture setting
                    if is_material && ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to material").clicked()
                    {
                        editor_state.de_select_current_item(state);

                        editor_state.selected_object = format!("material_{}", component_id);
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_type = SelectionType::Material;
                        editor_state.settings = SettingsPanel::Material;
                    }
                });
            },
            |ui|
            {
                ui.label(format!("Id: {}", component_id));
                ui.label(format!("Name: {}", name));

                // filter out current component
                {
                    let mut node: std::sync::RwLockWriteGuard<'_, Box<crate::state::scene::node::Node>> = node.write().unwrap();
                    node.components = all_components_clone.clone();
                    node.components.remove(component_i);
                }

                let mut component = component.write().unwrap();
                component.ui(ui, Some(node.clone()));

                // re-add current component
                {
                    let mut node = node.write().unwrap();
                    node.components = all_components_clone.clone();
                }
            });
        }

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
                let instance = instance.read().unwrap();

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
                        component.ui(ui, None);
                    });
                }
            }

            if let Some(delete_component_id) = delete_component_id
            {
                let mut instance = instance.write().unwrap();
                instance.remove_component_by_id(delete_component_id);
            }
        }
    }

    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
    {
        if ui.button(RichText::new("Add Component").heading().strong().color(Color32::WHITE)).clicked()
        {
            editor_state.dialog_add_component = true;
        }
    });
}
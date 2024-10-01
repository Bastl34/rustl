use std::{fmt::format, sync::{Arc, RwLock}};

use egui::{Ui, RichText, Color32};
use nalgebra::DimName;

use crate::{component_downcast, helper::concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, state::{gui::helper::generic_items::{self, collapse_with_title}, scene::{components::{animation::Animation, component::{Component, ComponentItem}, joint::Joint, material::Material, mesh::Mesh, sound::Sound}, manager::id_manager, node::{Node, NodeItem}, scene::Scene, utilities::scene_utils::{self, execute_on_scene_mut, execute_on_scene_mut_and_wait, execute_on_state_mut}}, state::State}};

use super::editor_state::{EditorState, PickType, SelectionType, SettingsPanel};

const FROM_FILE_COLOR: Color32 = Color32::from_rgb(80, 20, 20);

pub fn build_objects_list(editor_state: &mut EditorState, exec_queue: ExecutionQueueItem, scene: &mut Box<Scene>, ui: &mut Ui, nodes: &Vec<NodeItem>, scene_id: u64, parent_visible: bool, parent_locked: bool)
{
    for node_arc in nodes
    {
        let node = node_arc.read().unwrap();
        let child_nodes = &node.nodes.clone();

        let node_visible = node.visible;
        let visible = node_visible && parent_visible;

        let node_locked = node.locked;
        let locked = node_locked || parent_locked;

        let name = node.name.clone();
        let node_id = node.id;

        let filter = editor_state.hierarchy_filter.to_lowercase();

        let mut child_node_match = false;
        if !filter.is_empty()
        {
            let all_child_nodes = Scene::list_all_child_nodes(&node.nodes);
            for child_node in all_child_nodes
            {
                let child_node_name = child_node.read().unwrap().name.clone().to_lowercase();
                if child_node_name.find(filter.as_str()).is_some()
                {
                    child_node_match = true;
                    break;
                }
            }
        }

        if !filter.is_empty() && !child_node_match && name.to_lowercase().find(filter.as_str()).is_none()
        {
            continue;
        }

        let id = format!("objects_{}", node_id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut headline_name: String;
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

                if locked
                {
                    headline_name += " 🔒";
                }

                let mut heading;
                if visible
                {
                    heading = RichText::new(headline_name).strong()
                }
                else
                {
                    heading = RichText::new(headline_name).strikethrough();
                }

                if locked
                {
                    heading = heading.color(Color32::LIGHT_RED);
                }

                let mut selection; if editor_state.selected_object == id { selection = true; } else { selection = false; }

                let mut toggle = ui.toggle_value(&mut selection, heading);

                if toggle.clicked()
                {
                    if editor_state.pick_mode == PickType::Camera
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
                        editor_state.pick_mode = PickType::None;
                    }
                    else if editor_state.pick_mode == PickType::Parent
                    {
                        if let Some(node) = scene.find_node_by_id(node_id)
                        {
                            let (node_id, ..) = editor_state.get_object_ids();
                            if let Some(node_id) = node_id
                            {
                                let picking_node = scene.find_node_by_id(node_id).unwrap();
                                let node = node.clone();

                                execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |_scene|
                                {
                                    Node::set_parent(picking_node.clone(), node.clone());
                                }));
                            }
                        }
                        editor_state.pick_mode = PickType::None;
                    }
                    else if editor_state.pick_mode == PickType::AnimationCopy
                    {
                        let (node_id, ..) = editor_state.get_object_ids();
                        if let Some(node_id) = node_id
                        {
                            let from_node = scene.find_node_by_id(node_id).unwrap();

                            // find root
                            let mut picking_node = node_arc.clone();
                            if let Some(root_node) = Node::find_root_node(picking_node.clone())
                            {
                                picking_node = root_node.clone();
                            }

                            let target_animation_node = Node::find_animation_node(picking_node.clone());
                            if let Some(target_animation_node) = target_animation_node
                            {
                                if from_node.read().unwrap().id != target_animation_node.read().unwrap().id
                                {
                                    execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                                    {
                                        scene_utils::copy_all_animations(from_node.clone(), target_animation_node.clone(), scene);
                                    }));
                                }
                            }
                        }

                        editor_state.selected_object = id;
                        editor_state.settings = SettingsPanel::Components;

                        editor_state.pick_mode = PickType::None;
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

                            // highlight
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

                            // delesect all other
                            let node_id = node_arc.read().unwrap().id;

                            execute_on_state_mut(exec_queue.clone(), Box::new(move |state|
                            {
                                let predicate = move |node: NodeItem|
                                {
                                    return !node.read().unwrap().has_parent_id_or_is_equal(node_id)
                                };

                                EditorState::de_select_all_items(state, Some(Arc::new(predicate)));
                            }));
                        }
                        else
                        {
                            execute_on_state_mut(exec_queue.clone(), Box::new(move |state|
                            {
                                EditorState::de_select_all_items(state, None);
                            }));

                            editor_state.selected_object.clear();
                            editor_state.selected_scene_id = None;
                        }
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("⊞ Add empty node").clicked()
                    {
                        ui.close_menu();

                        let node_arc = node_arc.clone();
                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            scene.add_empty_node("Node", Some(node_arc.clone()));
                        }));
                    }

                    ui.separator();

                    if node.has_mesh()
                    {
                        if ui.button("🖹 Add default instance").clicked()
                        {
                            ui.close_menu();

                            let node_arc = node_arc.clone();
                            execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                            {
                                let id = scene.id_manager.write().unwrap().get_next_instance_id();
                                node_arc.write().unwrap().create_default_instance(node_arc.clone(), id);
                            }));
                        }

                        ui.separator();
                    }

                    // hide/show
                    let hide_show_text = if node_visible { "👁 Hide" } else { "👁 Show" };
                    if ui.button(hide_show_text).clicked()
                    {
                        ui.close_menu();

                        let node_arc = node_arc.clone();
                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            let mut node = node_arc.write().unwrap();
                            node.visible = !node.visible;
                        }));
                    }

                    // lock/unlock
                    let lock_unlock_text = if node_locked { "🔓 Unlock" } else { "🔒 Lock" };
                    if ui.button(lock_unlock_text).clicked()
                    {
                        ui.close_menu();

                        let node_arc = node_arc.clone();
                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            let mut node = node_arc.write().unwrap();
                            node.locked = !node.locked;
                        }));
                    }

                    if node.find_component::<Animation>().is_some()
                    {
                        ui.separator();

                        if ui.button("⏵ Start all animations").clicked()
                        {
                            ui.close_menu();
                            node.start_all_animations();
                        }

                        if ui.button("⏵ Start first animation").clicked()
                        {
                            ui.close_menu();
                            node.start_first_animation();
                        }

                        if ui.button("⏹ Stop all animations").clicked()
                        {
                            ui.close_menu();
                            node.stop_all_animations();
                        }

                        if ui.button("🗐 Copy and re-target animations").clicked()
                        {
                            ui.close_menu();

                            editor_state.de_select_current_item_from_scene(scene);
                            editor_state.selected_object = format!("objects_{}", node.id);
                            editor_state.selected_type = SelectionType::Object;
                            editor_state.selected_scene_id = Some(scene_id);
                            editor_state.pick_mode = PickType::AnimationCopy;
                        }
                    }

                    // delete
                    ui.separator();
                    if ui.button("🗑 Delete").clicked()
                    {
                        ui.close_menu();

                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            scene.delete_node_by_id(node_id);
                        }));
                    }
                });
            });

        }).body(|ui|
        {
            if child_nodes.len() > 0
            {
                build_objects_list(editor_state, exec_queue.clone(), scene, ui, child_nodes, scene_id, visible, locked);
            }

            if node.instances.get_ref().len() > 0
            {
                build_instances_list(editor_state, ui, node_arc.clone(), scene_id, visible, locked);
            }
        });
    }
}

pub fn build_instances_list(editor_state: &mut EditorState, ui: &mut Ui, node: NodeItem, scene_id: u64, parent_visible: bool, parent_locked: bool)
{
    let node_arc = node.clone();
    let node = node.read().unwrap();

    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for instance in node.instances.get_ref()
        {
            let mut toggle;
            let visible;
            let locked;
            let instance_id;
            let ui_id;

            {
                let instance = instance.read().unwrap();
                instance_id = instance.id;
                let instance_data = instance.get_data();

                visible = instance_data.visible;
                locked = instance_data.locked;

                ui_id = format!("objects_{}_{}", node.id, instance_id);
                let mut headline_name = format!("⚫ {}: {}", instance_id, instance.name);

                if parent_locked
                {
                    headline_name += " 🔒";
                }

                let mut heading = RichText::new(headline_name);

                if visible && parent_visible
                {
                    heading = heading.strong()
                }
                else
                {
                    heading = heading.strikethrough();
                }

                if locked || parent_locked
                {
                    heading = heading.color(Color32::LIGHT_RED);
                }

                if instance_data.highlight
                {
                    //heading = heading.color(Color32::from_rgb(255, 175, 175));
                    heading = heading.italics();
                }

                let mut selection; if editor_state.selected_object == ui_id { selection = true; } else { selection = false; }
                toggle = ui.toggle_value(&mut selection, heading);
            }

            if toggle.clicked()
            {
                if editor_state.selected_object != ui_id
                {
                    editor_state.selected_object = ui_id;
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

            // context menu
            let node_arc = node_arc.clone();
            toggle.context_menu(|ui|
            {
                // hide/show
                let hide_show_text = if visible { "👁 Hide" } else { "👁 Show" };
                if ui.button(hide_show_text).clicked()
                {
                    ui.close_menu();

                    let mut instance = instance.write().unwrap();
                    instance.get_data_mut().get_mut().visible = !visible;
                }

                // lock/unlock
                let lock_unlock_text = if locked { "🔓 Unlock" } else { "🔒 Lock" };
                if ui.button(lock_unlock_text).clicked()
                {
                    ui.close_menu();

                    let mut instance = instance.write().unwrap();
                    instance.get_data_mut().get_mut().locked = !locked;
                }

                // delete
                ui.separator();
                if ui.button("🗑 Delete").clicked()
                {
                    ui.close_menu();

                    spawn_thread(move ||
                    {
                        let mut node = node_arc.write().unwrap();
                        node.delete_instance_by_id(instance_id);
                    });
                }
            });
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

    let bounding_box_info = node.read().unwrap().get_world_bounding_info(None, true, None);

    // General
    collapse_with_title(ui, "object_data", true, "ℹ Object Data", None, |ui|
    {
        {
            let node = node.read().unwrap();

            ui.label(format!("Name: {}", node.name));
            ui.label(format!("Id: {}", node.id));
            ui.label(format!("Source: {:?}", node.source));

            if let Some(bounding_box_info) = bounding_box_info
            {
                ui.label(format!("B-Box min: x={:.3} y={:.3} z={:.3}", bounding_box_info.0.x, bounding_box_info.0.y, bounding_box_info.0.z));
                ui.label(format!("B-Box max: x={:.3} y={:.3} z={:.3}", bounding_box_info.1.x, bounding_box_info.1.y, bounding_box_info.1.z));
            }
        }
    });

    // Extras
    collapse_with_title(ui, "object_extras", true, "⊞ Extras", None, |ui|
    {
        ui.scope(|ui|
        {
            let node = node.read().unwrap();

            for (key, value) in node.extras.iter()
            {
                ui.label(format!("⚫ {}: {:?}", key, value));
            }
        });
    });

    // Skeleton
    if let Some(skin_node) = node.read().unwrap().skin.first()
    {
        collapse_with_title(ui, "object_skeleton", true, "🕱 Skeleton", None, |ui|
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
    collapse_with_title(ui, "object_info", true, "📈 Object Info", None, |ui|
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
    collapse_with_title(ui, "object_settings", true, "⛭ Object Settings", None, |ui|
    {
        let mut changed = false;

        let mut visible;
        let mut locked: bool;
        let mut root_node: bool;
        let mut render_children_first;
        let mut alpha_index;
        let mut pick_bbox_first;
        let mut name;
        {
            let node = node.read().unwrap();
            visible = node.visible;
            locked = node.locked;
            root_node = node.root_node;
            render_children_first = node.settings.render_children_first;
            alpha_index = node.settings.alpha_index;
            pick_bbox_first = node.settings.pick_bbox_first;
            name = node.name.clone();
        }

        ui.horizontal(|ui|
        {
            ui.label("name: ");
            ui.set_max_width(225.0);
            changed = ui.text_edit_singleline(&mut name).changed() || changed;
        });
        changed = ui.checkbox(&mut visible, "visible").changed() || changed;
        changed = ui.checkbox(&mut locked, "locked").changed() || changed;
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
            node.locked = locked;
            node.root_node = root_node;
            node.settings.render_children_first = render_children_first;
            node.settings.alpha_index = alpha_index;
            node.settings.pick_bbox_first = pick_bbox_first;
            node.name = name;
        }

        // parenting
        ui.horizontal(|ui|
        {
            let parent: Option<std::sync::Arc<std::sync::RwLock<Box<crate::state::scene::node::Node>>>> = node.read().unwrap().parent.clone();
            let mut parent_name = "".to_string();
            if let Some(parent) = parent
            {
                parent_name = parent.read().unwrap().name.clone();
            }

            ui.label("Parent:");
            ui.add_enabled_ui(false, |ui|
            {
                ui.set_max_width(225.0);
                ui.text_edit_singleline(&mut parent_name);
            });

            let mut toggle_value = if editor_state.pick_mode == PickType::Parent { true } else { false };
            if ui.toggle_value(&mut toggle_value, RichText::new("👆")).on_hover_text("pick mode").changed()
            {
                if toggle_value
                {
                    editor_state.pick_mode = PickType::Parent;
                }
                else
                {
                    editor_state.pick_mode = PickType::None;
                }
            }
        });

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Create Default Instance").heading().strong().color(Color32::LIGHT_GREEN)).clicked()
            {
                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                let id = scene.id_manager.write().unwrap().get_next_instance_id();
                node.write().unwrap().create_default_instance(node.clone(), id);
            }

            if ui.button(RichText::new("⮈ Go to parent").heading().strong()).clicked()
            {
                let (node_id, instance_id) = editor_state.get_object_ids();

                if instance_id.is_some()
                {
                    editor_state.selected_object = format!("objects_{}", node_id.unwrap());
                }
                else if let Some(node_id) = node_id
                {
                    let node = state.find_scene_by_id(scene_id).unwrap().find_node_by_id(node_id);

                    if let Some(node) = node
                    {
                        let parent = node.read().unwrap().parent.clone();

                        if let Some(parent) = parent
                        {
                            let parent = parent.read().unwrap();
                            editor_state.selected_object = format!("objects_{}", parent.id);
                        }
                    }
                }
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

    let bounding_box_info = node.get_world_bounding_info(Some(instance_id), true, None);

    // General
    collapse_with_title(ui, "instance_data", true, "ℹ Instance Data", None, |ui|
    {
        let instance = instance.read().unwrap();

        ui.label(format!("name: {}", instance.name));
        ui.label(format!("id: {}", instance.id));

        if let Some(bounding_box_info) = bounding_box_info
        {
            ui.label(format!("B-Box min: x={:.3} y={:.3} z={:.3}", bounding_box_info.0.x, bounding_box_info.0.y, bounding_box_info.0.z));
            ui.label(format!("B-Box max: x={:.3} y={:.3} z={:.3}", bounding_box_info.1.x, bounding_box_info.1.y, bounding_box_info.1.z));
        }
    });

    // Settings
    let mut delete_instance = false;
    collapse_with_title(ui, "instance_settings", true, "⛭ Instance Settings", None, |ui|
    {
        let mut changed = false;

        let mut visible;
        let mut locked;
        let mut collision;
        let mut highlight;
        let mut name;
        let mut pickable;
        {
            let instance = instance.read().unwrap();
            let instance_data = instance.get_data();
            visible = instance_data.visible;
            locked = instance_data.locked;
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
        changed = ui.checkbox(&mut locked, "locked").changed() || changed;
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

pub fn match_component_filter(component_filter: &String, component: ComponentItem) -> bool
{
    if component_filter.is_empty()
    {
        return true;
    }

    let filter = component_filter.to_lowercase();

    let component = component.read().unwrap();

    let component_name = component.get_base().component_name.to_lowercase();
    let component_id = component.id().to_string();
    let name = component.get_base().name.to_lowercase();

    if component_name.find(filter.as_str()).is_some()
    {
        return true;
    }

    if component_id.find(filter.as_str()).is_some()
    {
        return true;
    }

    if name.find(filter.as_str()).is_some()
    {
        return true;
    }

    return false;
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

    let id_manager = scene.id_manager.clone();

    let node = node.unwrap();

    // filter
    ui.horizontal(|ui|
    {
        ui.label("🔍");
        ui.add(egui::TextEdit::singleline(&mut editor_state.component_filter));

        if ui.button("⟳").clicked()
        {
            editor_state.component_filter.clear();
        }
    });

    // components
    if instance_id.is_none()
    {
        let mut delete_component_id = None;
        let mut duplicate_component: Option<ComponentItem> = None;

        let all_components;
        let all_components_clone;
        {
            let node_read = node.read().unwrap();
            all_components = node_read.components.clone();
            all_components_clone = node_read.components.clone();
        }

        for (component_i, component) in all_components.iter().enumerate()
        {
            if !match_component_filter(&editor_state.component_filter, component.clone())
            {
                continue;
            }

            let component_id;
            let name;
            let component_title;
            let component_name;
            let is_material;
            let is_sound;
            let from_file;
            let duplicatable;
            {
                let component = component.read().unwrap();
                let base = component.get_base();
                component_title = format!("{} {}", base.icon, base.name);
                component_name = base.component_name.clone();
                name = base.name.clone();
                component_id = component.id();
                from_file = base.from_file;

                duplicatable = component.duplicatable();

                is_material = component.as_any().downcast_ref::<Material>().is_some();
                is_sound = component.as_any().downcast_ref::<Sound>().is_some();
            }

            //let bg_color = if from_file { Some(FROM_FILE_COLOR) } else { None };
            let bg_color = None;

            generic_items::collapse(ui, component_id.to_string(), true, bg_color, |ui|
            {
                ui.label(RichText::new(component_title).heading().strong()).on_hover_text(component_name);
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

                    if duplicatable
                    {
                        if ui.button(RichText::new("🗐").color(Color32::WHITE)).on_hover_text("duplicate").clicked()
                        {
                            let component = component.read().unwrap();

                            let id = id_manager.write().unwrap().get_next_component_id();
                            duplicate_component = component.duplicate(id);
                        }
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

                    // link to the sound setting
                    if is_sound && ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to sound").clicked()
                    {
                        editor_state.de_select_current_item(state);

                        editor_state.selected_object = format!("sound_{}", component_id);
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_type = SelectionType::Sound;
                        editor_state.settings = SettingsPanel::Sound;
                    }

                    if let Some(info) = &component.read().unwrap().get_base().info
                    {
                        ui.label(RichText::new("ℹ").color(Color32::WHITE)).on_hover_text(info);
                    }

                    if from_file
                    {
                        ui.label(RichText::new("⚠").color(Color32::LIGHT_RED)).on_hover_text("This component was loaded from a resource. Adjustments can not be saved.");
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

        if let Some(duplicate_component) = duplicate_component
        {
            node.write().unwrap().add_component(duplicate_component);
        }
    }

    if let Some(instance_id) = instance_id
    {
        let mut delete_component_id = None;
        let mut duplicate_component: Option<ComponentItem> = None;
        let mut sound_component_id = None;

        let node_read: std::sync::RwLockReadGuard<'_, Box<crate::state::scene::node::Node>> = node.read().unwrap();
        let instance = node_read.find_instance_by_id(instance_id);

        if let Some(instance) = instance
        {
            {
                let instance = instance.read().unwrap();

                for component in &instance.components
                {
                    if !match_component_filter(&editor_state.component_filter, component.clone())
                    {
                        continue;
                    }

                    let component_id;
                    let name;
                    let component_name;
                    let component_title;
                    let is_sound;
                    let from_file;
                    let duplicatable;
                    {
                        let component = component.read().unwrap();
                        let base = component.get_base();
                        component_name = format!("{} {}", base.icon, base.name);
                        component_title = base.component_name.clone();
                        name = base.name.clone();
                        component_id = component.id();
                        from_file = base.from_file;
                        duplicatable = component.duplicatable();

                        is_sound = component.as_any().downcast_ref::<Sound>().is_some();
                    }

                    //let bg_color = if from_file { Some(FROM_FILE_COLOR) } else { None };
                    let bg_color = None;

                    generic_items::collapse(ui, component_id.to_string(), true, bg_color, |ui|
                    {
                        ui.label(RichText::new(component_title).heading().strong()).on_hover_text(component_name);
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

                            if duplicatable
                            {
                                if ui.button(RichText::new("🗐").color(Color32::WHITE)).on_hover_text("duplicate").clicked()
                                {
                                    let component = component.read().unwrap();

                                    let id = id_manager.write().unwrap().get_next_component_id();
                                    duplicate_component = component.duplicate(id);
                                }
                            }

                            if let Some(info) = &component.read().unwrap().get_base().info
                            {
                                ui.label(RichText::new("ℹ").color(Color32::WHITE)).on_hover_text(info);
                            }

                            // link to the sound setting
                            if is_sound && ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to sound").clicked()
                            {
                                sound_component_id = Some(component_id);
                            }

                            if from_file
                            {
                                ui.label(RichText::new("⚠").color(Color32::LIGHT_RED)).on_hover_text("This component was loaded from a resource. Adjustments can not be saved.");
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

            if let Some(duplicate_component) = duplicate_component
            {
                let mut instance = instance.write().unwrap();
                instance.add_component(duplicate_component);
            }

            if let Some(sound_component_id) = sound_component_id
            {
                editor_state.de_select_current_item(state);

                editor_state.selected_object = format!("sound_{}", sound_component_id);
                editor_state.selected_scene_id = Some(scene_id);
                editor_state.selected_type = SelectionType::Sound;
                editor_state.settings = SettingsPanel::Sound;
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
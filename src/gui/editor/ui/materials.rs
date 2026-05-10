use std::collections::HashMap;

use egui::{Ui, RichText, Color32};

use crate::{component_downcast_mut, gui::{editor::{editor::EDITOR_INTERNAL_TAG, editor_state::PickType, ui::{dialogs::load_texture_dialog, helper::ui_helper::{fit_hierarchy_heading, rename_hierarchy_item_or_toggle_selection}}}, helper::{generic_items::{self, collapse_with_title}, info_box::info_box}}, helper::concurrency::{execution_queue::ExecutionQueueItem, thread::spawn_thread}, state::{scene::{components::material::{ALL_TEXTURE_TYPES, Material, MaterialItem}, scene::Scene, utilities::scene_utils::execute_on_scene_mut}, state::{ENGINE_INTERNAL_TAG, State}}};

use super::super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_material_list(editor_state: &mut EditorState, exec_queue: ExecutionQueueItem, materials: &HashMap<u32, MaterialItem>, ui: &mut Ui, scene_id: u32)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (material_id, material) in materials
        {
            let material = material.read().unwrap();

            let is_internal = material.get_base().tags.contains(ENGINE_INTERNAL_TAG) || material.get_base().tags.contains(EDITOR_INTERNAL_TAG);
            let show_from_tags = !is_internal || (is_internal && editor_state.show_internal_entries);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !show_from_tags || !filter.is_empty() && material.get_base().name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let id = format!("material_{}", material_id);

            let headline_name = fit_hierarchy_heading(ui, "⚫ ", &material.get_base().name, "", 0.0);
            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::Material && editor_state.selected_object == id { selection = true; } else { selection = false; }

            let name = material.get_base().name.clone();
            let exec_queue_clone = exec_queue.clone();

            let material_id = *material_id;


            let mut toggle = rename_hierarchy_item_or_toggle_selection(ui, heading, &mut selection, editor_state, "material", material_id, name.clone(), Box::new(move |new_name|
            {
                execute_on_scene_mut(exec_queue_clone, scene_id, Box::new(move |scene|
                {
                    if let Some(material) = scene.get_material_by_id(material_id)
                    {
                        material.write().unwrap().get_base_mut().name = new_name.clone();
                    }
                }));
            }));

            toggle = toggle.on_hover_text(format!("Material ID: {}", material_id));

            if toggle.clicked()
            {
                //if self.selected_material.is_none() || (self.selected_material.is_some() && self.selected_material.unwrap() != *material_id)
                if selection
                {

                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Material;
                    editor_state.settings = SettingsPanel::Material;
                }
                else
                {
                    editor_state.selected_object.clear();
                    editor_state.selected_scene_id = None;
                }
            }

            toggle.context_menu(|ui|
            {
                if ui.button("✏ Rename").clicked()
                {
                    ui.close();
                    editor_state.hierarchy_rename_id = Some(("material".to_string(), material_id));
                    editor_state.hierarchy_rename_value = name.clone();
                }

                // delete
                ui.separator();
                if ui.button("🗑 Delete").clicked()
                {
                    ui.close();

                    execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                    {
                        scene.delete_material_by_id(material_id);
                    }));
                }
            });
        }
    });
}

pub fn create_material_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u32 = editor_state.selected_scene_id.unwrap();

    let (material_id, ..) = editor_state.get_object_ids();

    let main_queue = state.main_thread_execution_queue.clone();
    let mipmapping = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if material_id.is_none() { return; }
    let material_id = material_id.unwrap();

    if let Some(material) = scene.get_material_by_id(material_id)
    {
        collapse_with_title(ui, "material_info", true, "ℹ Material Info", None, |ui|
        {
            let material = material.read().unwrap();

            ui.label(format!("Name: {}", &material.get_base().name));
            ui.label(format!("Id: {}", material.get_base().id));
        });

        collapse_with_title(ui, "material_settings", true, "🎨 Material Settings", None, |ui|
        {
            let mut material = material.write().unwrap();
            material.ui(ui, None);
        });

        collapse_with_title(ui, "material_usage", true, "👆 Used by Objects", None, |ui|
        {
            let mut used = false;
            //Scene::list_all_child_nodes(nodes)
            let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

            for node in all_nodes
            {
                let node = node.read().unwrap();
                if node.find_component_by_id(material_id).is_some()
                {
                    ui.horizontal(|ui|
                    {
                        ui.label(format!(" ⚫ {}: {}", node.id, node.name));

                        // link to the object setting
                        if ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to node").clicked()
                        {
                            editor_state.selected_object = format!("objects_{}", node.id);
                            editor_state.selected_scene_id = Some(scene_id);
                            editor_state.selected_type = SelectionType::Object;
                            editor_state.settings = SettingsPanel::Components;
                        }
                    });

                    used = true;
                }
            }

            if !used
            {
                info_box(ui, "This material is not used by any object. Try removing it to save resources.");
            }
        });

        {
            component_downcast_mut!(material, Material);

            for texture_type in ALL_TEXTURE_TYPES
            {
                if material.has_texture(texture_type)
                {
                    let mut remove_texture = false;
                    let mut changed = false;

                    let texture = material.get_texture_by_type(texture_type);
                    let texture_state = texture.unwrap();

                    let mut enabled = texture_state.enabled;

                    if let Some(texture) = texture_state.get()
                    {
                        let mut texture = texture.write().unwrap();
                        let texture_id = texture.id;

                        let title = format!("🖼 {}", texture_type.to_string());
                        let id = format!("texture_{}", texture_type.to_string());

                        generic_items::collapse(ui, id, true, None, |ui|
                        {
                            ui.label(RichText::new(title).heading().strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                            {
                                if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).on_hover_text("delete").clicked()
                                {
                                    remove_texture = true;
                                }

                                // enabled toggle
                                let toggle_color = if enabled { Color32::GREEN } else { Color32::RED };
                                let toggle_text = RichText::new("⏺").color(toggle_color);

                                if ui.toggle_value(&mut enabled, toggle_text).on_hover_text("enable/disable").clicked()
                                {
                                    changed = true;
                                }

                                // link to the texture setting
                                if ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to texture").clicked()
                                {
                                    editor_state.selected_object = format!("texture_{}", texture_id);
                                    editor_state.selected_scene_id = Some(scene_id);
                                    editor_state.selected_type = SelectionType::Texture;
                                    editor_state.settings = SettingsPanel::Texture;
                                }
                            });
                        },
                        |ui|
                        {
                            texture.ui_info(ui);
                            drop(texture); // drop texture object - otherwise write is still open

                            ui.separator();
                            material.ui_texture_state(ui, texture_type);
                        });
                    }

                    if changed
                    {
                        material.set_texture_state(texture_type , enabled);
                    }

                    if remove_texture
                    {
                        material.remove_texture(texture_type)
                    }
                }
                else
                {
                    let title = format!("🖼 {}", texture_type.to_string());
                    let id: String = format!("texture_{}_{}", material_id, texture_type.to_string());

                    generic_items::collapse(ui, id.clone(), true, None, |ui|
                    {
                        ui.label(RichText::new(title).heading().strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                        {
                            // "enabled" toggle
                            let toggle_text = RichText::new("⏺").color(Color32::RED);

                            ui.add_enabled_ui(false, |ui|
                            {
                                let mut enabled = false;
                                ui.toggle_value(&mut enabled, toggle_text)
                            });

                            let mut toggle_value = if editor_state.pick_mode == PickType::Texture && editor_state.pick_id == id { true } else { false };
                            if ui.toggle_value(&mut toggle_value, RichText::new("👆")).on_hover_text("pick texture").changed()
                            {
                                if toggle_value
                                {
                                    editor_state.pick_id = id.clone();
                                    editor_state.pick_mode = PickType::Texture;
                                }
                                else
                                {
                                    editor_state.pick_id = "".to_string();
                                    editor_state.pick_mode = PickType::None;
                                }
                            }
                        });
                    },
                    |ui|
                    {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
                        {
                            ui.horizontal(|ui|
                            {
                                if ui.button(RichText::new("Load new texture").heading().strong()).clicked()
                                {
                                    let main_queue = main_queue.clone();
                                    spawn_thread(move ||
                                    {
                                        load_texture_dialog(main_queue.clone(), Some(texture_type), None, Some(material_id), mipmapping, max_tex_res);
                                    });
                                }
                            });
                        });
                    });
                }
            }
        }

        // delete material
        ui.spacing();
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Material").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_material_by_id(material_id);
            }
        });
    }
}
use egui::{Ui, RichText, Color32};

use crate::{gui::{editor::{editor::{EDITOR_INTERNAL_TAG, MAX_NAME_LENGTH}, ui::helper::ui_helper::rename_hierarchy_item_or_toggle_selection}, helper::generic_items::{self, collapse_with_title}}, helper::{concurrency::execution_queue::ExecutionQueueItem, generic::cut_string_to_length}, state::{scene::{camera::CameraItem, utilities::scene_utils::execute_on_scene_mut}, state::{ENGINE_INTERNAL_TAG, State}}};

use super::super::editor_state::{EditorState, PickType, SelectionType, SettingsPanel};

pub fn build_camera_list(editor_state: &mut EditorState, exec_queue: ExecutionQueueItem, cameras: &Vec<CameraItem>, ui: &mut Ui, scene_id: u32)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for camera in cameras
        {
            let headline_name = format!("⚫ {}: {}", camera.id, cut_string_to_length(&camera.name, MAX_NAME_LENGTH));

            let id = format!("camera_{}", camera.id);

            let is_internal = camera.tags.contains(ENGINE_INTERNAL_TAG) || camera.tags.contains(EDITOR_INTERNAL_TAG);
            let show_from_tags = !is_internal || (is_internal && editor_state.show_internal_entries);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !show_from_tags || !filter.is_empty() && camera.name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let mut heading = RichText::new(headline_name).strong();
            if !camera.enabled
            {
                heading = heading.strikethrough();
            }

            let mut selection; if editor_state.selected_type == SelectionType::Camera && editor_state.selected_object == id { selection = true; } else { selection = false; }

            let name = camera.name.clone();
            let camera_id = camera.id;
            let exec_queue_clone = exec_queue.clone();

            let toggle = rename_hierarchy_item_or_toggle_selection(ui, heading, &mut selection, editor_state, camera_id, name.clone(), Box::new(move |new_name|
            {
                execute_on_scene_mut(exec_queue_clone, scene_id, Box::new(move |scene|
                {
                    if let Some(camera) = scene.get_camera_by_id_mut(camera_id)
                    {
                        camera.name = new_name.clone();
                    }
                }));
            }));

            if toggle.clicked()
            {
                if selection
                {
                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Camera;
                    editor_state.settings = SettingsPanel::Camera;
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
                    editor_state.hierarchy_rename_id = Some(camera_id);
                    editor_state.hierarchy_rename_value = name.clone();
                }

                // delete
                ui.separator();
                if ui.button("🗑 Delete").clicked()
                {
                    ui.close();

                    execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                    {
                        scene.delete_camera_by_id(camera_id);
                    }));
                }
            });
        }
    });
}

pub fn create_camera_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u32 = editor_state.selected_scene_id.unwrap();

    let (camera_id, ..) = editor_state.get_object_ids();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if camera_id.is_none() { return; }
    let camera_id = camera_id.unwrap();

    if let Some(camera) = scene.get_camera_by_id_mut(camera_id)
    {
        collapse_with_title(ui, "camera_general_settings", true, "⛭ General Settings", None, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.set_max_width(225.0);

                ui.label("Name: ");
                ui.text_edit_singleline(&mut camera.name);
            });

            ui.horizontal(|ui|
            {
                let mut node_name = "".to_string();
                if let Some(node) = camera.node.as_ref()
                {
                    let node = node.read().unwrap();
                    node_name = format!("{} (id: {})", node.name, node.id);
                }

                ui.label("Target:");
                ui.add_enabled_ui(false, |ui|
                {
                    ui.set_max_width(225.0);
                    ui.text_edit_singleline(&mut node_name);
                });

                let mut toggle_value = if editor_state.pick_mode == PickType::Camera { true } else { false };
                if ui.toggle_value(&mut toggle_value, RichText::new("👆")).on_hover_text("pick mode").changed()
                {
                    if toggle_value
                    {
                        editor_state.pick_mode = PickType::Camera;
                    }
                    else
                    {
                        editor_state.pick_mode = PickType::None;
                    }
                }

                // link to the material setting
                if camera.node.is_some() && ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to object").clicked()
                {
                    let node = camera.node.as_ref().unwrap();

                    editor_state.selected_object = format!("objects_{}", node.read().unwrap().id);
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Object;
                    editor_state.settings = SettingsPanel::Object;
                }

                if camera.node.is_some() && ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).on_hover_text("remove target").clicked()
                {
                    camera.remove_node();
                }
            });

            ui.checkbox(&mut camera.enabled, "enabled");
        });

        collapse_with_title(ui, "camera_settings", true, "📷 Camera Settings", None, |ui|
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

            generic_items::collapse(ui, "camera_controller".to_string(), true, None, |ui|
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

        // add camera controller
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Add Cam Controller").heading().strong().color(Color32::WHITE)).clicked()
            {
                editor_state.dialog_add_camera_controller = true;
            }
        });

        // delete camera
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Camera").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_camera_by_id(camera_id);
            }
        });
    }
}
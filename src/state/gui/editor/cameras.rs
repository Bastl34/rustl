use egui::{Ui, RichText, Color32};

use crate::state::{scene::camera::CameraItem, state::State, gui::helper::generic_items::{collapse_with_title, self}};

use super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_camera_list(editor_state: &mut EditorState, cameras: &Vec<CameraItem>, ui: &mut Ui, scene_id: u64)
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

            let mut selection; if editor_state.selected_type == SelectionType::Cameras && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if selection
                {
                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Cameras;
                    editor_state.settings = SettingsPanel::Camera;
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

pub fn create_camera_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u64 = editor_state.selected_scene_id.unwrap();

    let (camera_id, ..) = editor_state.get_object_ids();

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
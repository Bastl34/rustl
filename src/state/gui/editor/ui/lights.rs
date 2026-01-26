use std::cell::RefCell;

use egui::{Ui, RichText};

use crate::{helper::{change_tracker::ChangeTracker, generic::cut_string_to_length}, state::{gui::{editor::editor::{EDITOR_INTERNAL_TAG, MAX_NAME_LENGTH}, helper::generic_items::collapse_with_title}, scene::light::{Light, LightItem}, state::{State, ENGINE_INTERNAL_TAG}}};

use super::super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_light_list(editor_state: &mut EditorState, lights: &ChangeTracker<Vec<RefCell<ChangeTracker<LightItem>>>>, ui: &mut Ui, scene_id: u32)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        let lights = lights.get_ref();
        for light in lights
        {
            let light = light.borrow();
            let light = light.get_ref();

            let is_internal = light.tags.contains(ENGINE_INTERNAL_TAG) || light.tags.contains(EDITOR_INTERNAL_TAG);
            let show_from_tags = !is_internal || (is_internal && editor_state.show_internal_entries);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !show_from_tags || !filter.is_empty() && light.name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let headline_name = format!("⚫ {}: {}", light.id, cut_string_to_length(&light.name, MAX_NAME_LENGTH));

            let id = format!("light_{}", light.id);

            let mut heading = RichText::new(headline_name).strong();
            if !light.enabled
            {
                heading = heading.strikethrough();
            }

            let mut selection; if editor_state.selected_type == SelectionType::Light && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if selection
                {
                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Light;
                    editor_state.settings = SettingsPanel::Light;
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

pub fn create_light_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u32 = editor_state.selected_scene_id.unwrap();

    let (light_id, ..) = editor_state.get_object_ids();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if light_id.is_none() { return; }
    let light_id = light_id.unwrap();

    if let Some(light) = scene.get_light_by_id(light_id)
    {
        collapse_with_title(ui, "light_general_settings", true, "⛭ General Settings", None, |ui|
        {
            let mut changed = false;

            let mut enabled;
            let mut name;
            {
                let light = light.borrow();
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

        collapse_with_title(ui, "light_settings", true, "💡 Light Settings", None, |ui|
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
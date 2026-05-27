use egui::{Color32, ScrollArea, Ui};

use crate::{gui::{editor::editor_state::{EditorState, SelectionType, SettingsPanel}, helper::generic_items::tab}, state::state::State};

pub fn create_scene_tabs(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // remove tabs for scenes that no longer exist
    let valid_ids: Vec<u32> = state.scenes.iter().map(|s| s.id).collect();
    editor_state.open_scene_tabs.retain(|id| valid_ids.contains(id));

    // auto-open first scene if no tabs are open yet
    if editor_state.open_scene_tabs.is_empty()
    {
        if let Some(first) = state.scenes.first()
        {
            editor_state.open_scene_tabs.push(first.id);
        }
    }

    let can_close = editor_state.open_scene_tabs.len() > 1;

    // redirect vertical mouse wheel to horizontal scrolling when hovering the tab bar
    if ui.ui_contains_pointer()
    {
        ui.input_mut(|i|
        {
            i.smooth_scroll_delta.x += i.smooth_scroll_delta.y;
            i.smooth_scroll_delta.y = 0.0;
        });
    }

    ScrollArea::horizontal().id_salt("scene_tabs_scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            ui.spacing_mut().item_spacing.x = 2.0;

            let mut tab_to_close: Option<u32> = None;
            let mut scene_to_activate: Option<u32> = None;

            for &scene_id in &editor_state.open_scene_tabs
            {
                let scene_name = state.scenes.iter()
                    .find(|s| s.id == scene_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Scene {}", scene_id));

                let is_active = state.scenes.iter()
                    .find(|s| s.id == scene_id)
                    .map(|s| s.active)
                    .unwrap_or(false);

                let is_selected = editor_state.selected_scene_id == Some(scene_id)
                    && editor_state.selected_object.is_empty()
                    && editor_state.selected_type == SelectionType::None;

                let mut label = egui::RichText::new(format!("🎬  {}", scene_name)).size(13.0);
                if is_active
                {
                    // brighter blue + bold so the active scene clearly stands out
                    label = label.color(Color32::from_rgb(170, 230, 255)).strong();
                }

                let result = tab(ui, label, is_selected, can_close);

                if result.clicked
                {
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_object.clear();
                    editor_state.selected_type = SelectionType::None;
                    editor_state.settings_panel = SettingsPanel::Scene;
                    scene_to_activate = Some(scene_id);
                }

                if result.close_clicked
                {
                    tab_to_close = Some(scene_id);
                }
            }

            // + button to add a new scene
            {
                let plus_response = tab(ui, egui::RichText::new("+").size(13.0).strong(), false, false).response;
                let plus_response = plus_response.on_hover_text("Add scene");

                egui::Popup::menu(&plus_response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui|
                {
                    ui.set_min_width(140.0);
                    if ui.button("⊞ Add Scene").clicked()
                    {
                        let new_scene_id = state.add_scene("Scene").id;
                        if !editor_state.open_scene_tabs.contains(&new_scene_id)
                        {
                            editor_state.open_scene_tabs.push(new_scene_id);
                        }
                        editor_state.selected_scene_id = Some(new_scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::None;
                        editor_state.settings_panel = SettingsPanel::Scene;
                        state.set_active_scene(new_scene_id);
                        egui::Popup::close_all(ui.ctx());
                    }

                    if ui.button("⬇ Import Scene").clicked()
                    {
                        let loading_state = editor_state.loading.clone();
                        let loading_progress_state = editor_state.loading_progress.clone();
                        if let Some(new_scene_id) = crate::gui::editor::editor_project::import_editor_scene_with_dialog(state, loading_state, loading_progress_state)
                        {
                            if !editor_state.open_scene_tabs.contains(&new_scene_id)
                            {
                                editor_state.open_scene_tabs.push(new_scene_id);
                            }
                            editor_state.selected_scene_id = Some(new_scene_id);
                            editor_state.selected_object.clear();
                            editor_state.selected_type = SelectionType::None;
                            editor_state.settings_panel = SettingsPanel::Scene;
                            state.set_active_scene(new_scene_id);
                        }
                        egui::Popup::close_all(ui.ctx());
                    }
                });
            }

            if let Some(id) = scene_to_activate
            {
                state.set_active_scene(id);
            }

            if let Some(id) = tab_to_close
            {
                editor_state.open_scene_tabs.retain(|&s| s != id);
                if editor_state.selected_scene_id == Some(id)
                {
                    editor_state.selected_scene_id = editor_state.open_scene_tabs.first().copied();
                    if editor_state.selected_scene_id.is_none()
                    {
                        editor_state.settings_panel = SettingsPanel::General;
                    }
                }
            }
        });
    });
}
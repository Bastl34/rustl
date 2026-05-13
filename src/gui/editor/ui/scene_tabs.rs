use egui::{Color32, ScrollArea, Ui};

use crate::{gui::editor::editor_state::{EditorState, SelectionType, SettingsPanel}, state::state::State};

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

                let tab_color = if is_active { Color32::LIGHT_BLUE } else { ui.visuals().text_color() };

                ui.scope(|ui|
                {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);

                    // allocate the full tab area manually so we can paint a background
                    let label_galley = ui.painter().layout_no_wrap(
                        format!("🎬  {}", scene_name),
                        egui::FontId::proportional(13.0),
                        tab_color,
                    );
                    let close_galley = ui.painter().layout_no_wrap
                    (
                        "🗙".to_string(),
                        egui::FontId::proportional(11.0),
                        ui.visuals().text_color(),
                    );

                    let h_pad = 10.0;
                    let v_pad = 5.0;
                    let gap = 6.0;
                    let tab_w = h_pad + label_galley.size().x + gap + close_galley.size().x + h_pad;
                    let tab_h = label_galley.size().y + v_pad * 2.0;

                    let (tab_rect, tab_response) = ui.allocate_exact_size(
                        egui::vec2(if can_close { tab_w } else { h_pad + label_galley.size().x + h_pad }, tab_h),
                        egui::Sense::click(),
                    );

                    // background
                    let bg_color = if is_selected
                    {
                        ui.visuals().selection.bg_fill
                    }
                    else if tab_response.hovered()
                    {
                        ui.visuals().widgets.hovered.bg_fill
                    }
                    else
                    {
                        ui.visuals().widgets.inactive.bg_fill
                    };
                    let rounding = egui::CornerRadius { nw: 4_u8, ne: 4_u8, sw: 0_u8, se: 0_u8 };
                    ui.painter().rect_filled(tab_rect, rounding, bg_color);

                    // label
                    let label_pos = egui::pos2(tab_rect.left() + h_pad, tab_rect.center().y - label_galley.size().y / 2.0);
                    ui.painter().galley(label_pos, label_galley, tab_color);

                    if tab_response.clicked()
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::None;
                        editor_state.settings = SettingsPanel::Scene;
                        scene_to_activate = Some(scene_id);
                    }

                    // close button (only when more than one tab open)
                    if can_close
                    {
                        let close_x = tab_rect.right() - h_pad - close_galley.size().x;
                        let close_y = tab_rect.center().y - close_galley.size().y / 2.0;
                        let close_rect = egui::Rect::from_min_size(
                            egui::pos2(close_x - 3.0, close_y - 2.0),
                            egui::vec2(close_galley.size().x + 6.0, close_galley.size().y + 4.0),
                        );
                        let close_response = ui.allocate_rect(close_rect, egui::Sense::click());

                        let close_color = if close_response.hovered()
                        {
                            Color32::WHITE
                        }
                        else
                        {
                            ui.visuals().weak_text_color()
                        };

                        if close_response.hovered()
                        {
                            ui.painter().rect_filled(close_rect, 3.0, egui::Color32::from_rgba_unmultiplied(180, 50, 50, 200));
                        }

                        ui.painter().galley(egui::pos2(close_x, close_y), close_galley, close_color);

                        if close_response.clicked()
                        {
                            tab_to_close = Some(scene_id);
                        }
                    }

                });
            }

            // + button to add a new scene
            ui.scope(|ui|
            {
                ui.spacing_mut().item_spacing.x = 4.0;

                let plus_galley = ui.painter().layout_no_wrap
                (
                    "+".to_string(),
                    egui::FontId::proportional(16.0),
                    ui.visuals().text_color(),
                );

                let h_pad = 10.0;
                let v_pad = 5.0;
                // match tab height (label uses size 13.0)
                let label_galley = ui.painter().layout_no_wrap
                (
                    "🎬".to_string(),
                    egui::FontId::proportional(13.0),
                    ui.visuals().text_color(),
                );
                let plus_w = h_pad * 2.0 + plus_galley.size().x;
                let plus_h = label_galley.size().y + v_pad * 2.0;

                let (plus_rect, plus_response) = ui.allocate_exact_size
                (
                    egui::vec2(plus_w, plus_h),
                    egui::Sense::click(),
                );

                let bg_color = if plus_response.hovered()
                {
                    ui.visuals().widgets.hovered.bg_fill
                }
                else
                {
                    ui.visuals().widgets.inactive.bg_fill
                };
                let rounding = egui::CornerRadius { nw: 4_u8, ne: 4_u8, sw: 0_u8, se: 0_u8 };
                ui.painter().rect_filled(plus_rect, rounding, bg_color);

                let plus_pos = egui::pos2
                (
                    plus_rect.center().x - plus_galley.size().x / 2.0,
                    plus_rect.center().y - plus_galley.size().y / 2.0,
                );
                ui.painter().galley(plus_pos, plus_galley, ui.visuals().text_color());

                let plus_response = plus_response.on_hover_text("Add scene");

                egui::Popup::menu(&plus_response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui|
                {
                    ui.set_min_width(140.0);
                    if ui.button("⊞ Add Scene").clicked()
                    {
                        let new_scene_id = state.add_scene("Scene");
                        if !editor_state.open_scene_tabs.contains(&new_scene_id)
                        {
                            editor_state.open_scene_tabs.push(new_scene_id);
                        }
                        editor_state.selected_scene_id = Some(new_scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::None;
                        editor_state.settings = SettingsPanel::Scene;
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
                            editor_state.settings = SettingsPanel::Scene;
                            state.set_active_scene(new_scene_id);
                        }
                        egui::Popup::close_all(ui.ctx());
                    }
                });
            });

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
                        editor_state.settings = SettingsPanel::General;
                    }
                }
            }
        });
    });
}
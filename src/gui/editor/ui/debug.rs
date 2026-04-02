use std::ops::Sub;

use egui::{Ui};

use crate::{gui::helper::generic_items::collapse_with_title, rendering::texture::Texture, state::{helper::render_item::get_render_item, state::State}};
use crate::gui::editor::ui::helper::ui_helper::fit_size;

use super::super::editor_state::EditorState;

pub fn create_debug_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // ********** general debugging **********
    collapse_with_title(ui, "scene_debugging", true, "🐛 Scene Debugging", None, |ui|
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            if ui.button("save image").clicked()
            {
                state.debug.save_image = true;
            }

            if ui.button("save depth pass image").clicked()
            {
                state.debug.save_depth_pass_image = true;
            }

            if ui.button("save depth buffer image").clicked()
            {
                state.debug.save_depth_buffer_image = true;
            }

            if ui.button("save hzb image").clicked()
            {
                state.debug.save_hzb_image = true;
            }

            if ui.button("save screenshot").clicked()
            {
                state.debug.save_screenshot = true;
            }

            ui.checkbox(&mut state.debug.highlight_visible_occlusions, "highlight visible occlusions");
        });
    });

    // ********** pipeline images **********
    {
        // depth pass image
        state.debug.show_depth_pass_image = None; // show and update only if the collapse is open
        collapse_with_title(ui, "debug_depth_pass_image", false, "🐛 Depth Pass Image", None, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                state.debug.show_depth_pass_image = Some(0);

                if let Some(depth_pass_image) = editor_state.debug_images.depth_pass_image.as_ref()
                {
                    let avail = ui.available_size();
                    let [texture_width, texture_height] = depth_pass_image.size();
                    let tex_size = egui::vec2(texture_width as f32, texture_height as f32);
                    let draw_size = fit_size(avail, tex_size);

                    ui.allocate_ui(draw_size, |ui|
                    {
                        let response = ui.add(egui::Image::new((depth_pass_image.id(), draw_size)).sense(egui::Sense::click()),);
                        if response.double_clicked()
                        {
                            editor_state.dialog_debug_image = true;
                            editor_state.dialog_debug_image_id = Some(depth_pass_image.clone());
                        }
                    });
                }
            });
        });

        // depth buffer image
        state.debug.show_depth_buffer_image = None; // show and update only if the collapse is open
        collapse_with_title(ui, "debug_depth_buffer_image", false, "🐛 Depth Buffer Image", None, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                state.debug.show_depth_buffer_image = Some(0);

                if let Some(depth_buffer_image) = editor_state.debug_images.depth_buffer_image.as_ref()
                {
                    let avail = ui.available_size();
                    let [texture_width, texture_height] = depth_buffer_image.size();
                    let tex_size = egui::vec2(texture_width as f32, texture_height as f32);
                    let draw_size = fit_size(avail, tex_size);

                    ui.allocate_ui(draw_size, |ui|
                    {
                        let response = ui.add(egui::Image::new((depth_buffer_image.id(), draw_size)).sense(egui::Sense::click()),);
                        if response.double_clicked()
                        {
                            editor_state.dialog_debug_image = true;
                            editor_state.dialog_debug_image_id = Some(depth_buffer_image.clone());
                        }
                    });
                }
            });
        });

        // hzb image
        let last_hzb_mip = state.debug.show_hzb_image.clone();
        state.debug.show_hzb_image = None; // show and update only if the collapse is open
        collapse_with_title(ui, "debug_hzb_image", false, "🐛 HZB Image", None, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let hzb_mip_max = state.scenes.iter()
                    .find(|scene| Some(scene.id) == editor_state.selected_scene_id)
                    .and_then(|scene| scene.get_active_camera())
                    .and_then(|cam| cam.hzb_texture_render_item.as_ref())
                    .map(|render_item_box| get_render_item::<Texture>(render_item_box).get_mip_level_count().sub(1))
                    .unwrap_or(0);

                state.debug.show_hzb_image = Some(last_hzb_mip.unwrap_or(0).min(hzb_mip_max));

                ui.add(egui::Slider::new(state.debug.show_hzb_image.as_mut().unwrap(), 0..=hzb_mip_max).text("mip level"));

                if let Some(hzb_image) = editor_state.debug_images.hzb_image.as_ref()
                {
                    let avail = ui.available_size();
                    let [texture_width, texture_height] = hzb_image.size();
                    let tex_size = egui::vec2(texture_width as f32, texture_height as f32);
                    let draw_size = fit_size(avail, tex_size);

                    ui.allocate_ui(draw_size, |ui|
                    {
                        let response = ui.add(egui::Image::new((hzb_image.id(), draw_size)).sense(egui::Sense::click()),);
                        if response.double_clicked()
                        {
                            editor_state.dialog_debug_image = true;
                            editor_state.dialog_debug_image_id = Some(hzb_image.clone());
                        }
                    });
                }
            });
        });
    }
}

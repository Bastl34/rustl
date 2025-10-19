use arboard::Clipboard;
use egui::{Ui, RichText, Color32};
use rfd::FileDialog;

use crate::{component_downcast, component_downcast_mut, helper::generic::cut_string_to_length, state::{gui::{editor::{editor::{EDITOR_INTERNAL_TAG, MAX_NAME_LENGTH}, editor_state::PickType}, helper::{generic_items::collapse_with_title, info_box::info_box}}, scene::components::{component::Component, material::Material}, state::{State, ENGINE_INTERNAL_TAG}}};

use super::super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_texture_list(editor_state: &mut EditorState, state: &State, ui: &mut Ui)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (_texture_hash, texture_arc) in &state.resources.textures
        {
            let texture = texture_arc.read().unwrap();
            let headline_name = format!("⚫ {}: {}", texture.id, cut_string_to_length(&texture.name, MAX_NAME_LENGTH));

            let is_internal = texture.tags.contains(ENGINE_INTERNAL_TAG) || texture.tags.contains(EDITOR_INTERNAL_TAG);
            let show_from_tags = !is_internal || (is_internal && editor_state.show_internal_entries);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !show_from_tags || !filter.is_empty() && texture.as_ref().name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let id = format!("texture_{}", texture.id);

            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::Texture && editor_state.selected_object == id { selection = true; } else { selection = false; }
            let toggle = ui.toggle_value(&mut selection, heading);

            if toggle.clicked()
            {
                if editor_state.pick_mode == PickType::Texture && editor_state.pick_id != ""
                {
                    if editor_state.selected_type == SelectionType::Material
                    {
                        let (material_id, ..) = editor_state.get_object_ids();

                        if let Some(material_id) = material_id
                        {
                            for scene in &state.scenes
                            {
                                if let Some(material) = scene.get_material_by_id(material_id)
                                {
                                    component_downcast_mut!(material, Material);

                                    let parts: Vec<&str> = editor_state.pick_id.split('_').collect();
                                    if let Some(tex_type) = parts.last()
                                    {
                                        material.set_texture_from_string_type(texture_arc.clone(), tex_type);
                                    }
                                }
                            }
                        }
                    }

                    editor_state.pick_mode = PickType::None;
                }
                else if selection
                {
                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = None;
                    editor_state.selected_type = SelectionType::Texture;
                    editor_state.settings = SettingsPanel::Texture;
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

pub fn create_texture_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let (texture_id, ..) = editor_state.get_object_ids();

    if texture_id.is_none() { return; }
    let texture_id = texture_id.unwrap();


    if let Some(texture) = state.get_texture_by_id(texture_id)
    {
        collapse_with_title(ui, "texture_info", true, "🖼 Texture Info", None, |ui|
        {
            {
                let mut texture = texture.write().unwrap();
                texture.ui_info(ui);
            }
        });

        collapse_with_title(ui, "texture_settings", true, "🖼 Texture Settings", None, |ui|
        {
            let mut changed = false;

            let mut name;
            {
                let texture = texture.read().unwrap();

                name = texture.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });

            if changed
            {
                let mut texture = texture.write().unwrap();

                texture.name = name;
            }

            {
                let mut texture = texture.write().unwrap();
                texture.ui(ui);
            }
        });

        collapse_with_title(ui, "texture_usage", true, "👆 Used by Materials", None, |ui|
        {
            let mut used = false;
            for scene in &state.scenes
            {
                for (material_id, material) in &scene.materials
                {
                    component_downcast!(material, Material);
                    if material.has_texture_id(texture_id)
                    {
                        ui.horizontal(|ui|
                        {
                            ui.label(format!(" ⚫ {}: {}", material_id, material.get_base().name));

                            // link to the material setting
                            if ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to material").clicked()
                            {
                                editor_state.selected_object = format!("material_{}", material_id);
                                editor_state.selected_scene_id = Some(scene.id);
                                editor_state.selected_type = SelectionType::Material;
                                editor_state.settings = SettingsPanel::Material;
                            }
                        });

                        used = true;
                    }
                }
            }

            if !used
            {
                info_box(ui, "This texture is not used by any material. Try removing it to save resources.");
            }
        });

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("💾 Save Texture").heading().strong().color(Color32::WHITE)).clicked()
            {
                let name = format!("{}.png", texture.read().unwrap().name.clone());
                if let Some(path) = FileDialog::new().add_filter("Image", &["png"]).set_directory("/").set_file_name(name).save_file()
                {
                    _ = texture.read().unwrap().get_data().image.save(path);
                }
            }

            if ui.button(RichText::new("📋 Copy to Clipboard").heading().strong().color(Color32::WHITE)).clicked()
            {
                let texture = texture.read().unwrap();
                let image = &texture.get_data().image;
                let image = image.to_rgba8();
                let image = image::DynamicImage::ImageRgba8(image);
                let bytes = image.as_bytes();

                let mut clipboard = Clipboard::new().unwrap();

                let img_data = arboard::ImageData
                {
                    width: image.width() as usize,
                    height: image.height() as usize,
                    bytes: bytes.into()
                };
                clipboard.set_image(img_data).unwrap();
            }
        });

        // delete texture
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Texture").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                state.delete_texture_by_id(texture_id);
            }
        });
    }
}
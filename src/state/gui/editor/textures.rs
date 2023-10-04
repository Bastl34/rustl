use std::{collections::HashMap, sync::Arc};

use egui::{Ui, RichText, Color32};
use rfd::FileDialog;

use crate::state::{state::State, gui::helper::generic_items::collapse_with_title, scene::texture::TextureItem};

use super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_texture_list(editor_state: &mut EditorState, textures: &HashMap<std::string::String, TextureItem>, ui: &mut Ui, scene_id: u64)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (_texture_hash, texture) in textures
        {
            let texture = texture.read().unwrap();
            let headline_name = format!("⚫ {}: {}", texture.id, texture.as_ref().name);

            let id = format!("texture_{}", texture.id);

            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::Textures && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if selection
                {

                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Textures;
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
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u64 = editor_state.selected_scene_id.unwrap();

    let (texture_id, ..) = editor_state.get_object_ids();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if texture_id.is_none() { return; }
    let texture_id = texture_id.unwrap();

    if let Some(texture) = scene.get_texture_by_id(texture_id)
    {
        collapse_with_title(ui, "texture_info", true, "🖼 Texture Info", |ui|
        {
            {
                let mut texture = texture.write().unwrap();
                texture.ui_info(ui);
            }
        });

        collapse_with_title(ui, "texture_settings", true, "🖼 Texture Settings", |ui|
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

        // load
        /*
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Load Texture").heading().strong().color(Color32::LIGHT_GREEN)).clicked()
            {
                if let Some(path) = FileDialog::new().add_filter("Image", &["jpg", "png"]).set_directory("/").pick_file()
                {
                    // TODO load
                    // TODO: replace hash in scene.textures
                }
            }
        });
        */

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Save Texture").heading().strong().color(Color32::LIGHT_GREEN)).clicked()
            {
                let name = format!("{}.png", texture.read().unwrap().name.clone());
                if let Some(path) = FileDialog::new().add_filter("Image", &["png"]).set_directory("/").set_file_name(name).save_file()
                {
                    _ = texture.read().unwrap().get_data().image.save(path);
                }
            }
        });

        // delete texture
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Texture").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_texture_by_id(texture_id);
            }
        });
    }
}
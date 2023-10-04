use std::collections::HashMap;

use egui::{Ui, RichText};

use crate::{state::{scene::components::material::{MaterialItem, ALL_TEXTURE_TYPES, Material}, state::State, gui::helper::generic_items::collapse_with_title}, component_downcast, component_downcast_mut};

use super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_material_list(editor_state: &mut EditorState, materials: &HashMap<u64, MaterialItem>, ui: &mut Ui, scene_id: u64)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (material_id, material) in materials
        {
            let material = material.read().unwrap();
            let headline_name = format!("⚫ {}: {}", material_id, material.get_base().name);

            let id = format!("material_{}", material_id);

            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::Materials && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                //if self.selected_material.is_none() || (self.selected_material.is_some() && self.selected_material.unwrap() != *material_id)
                if selection
                {

                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::Materials;
                    editor_state.settings = SettingsPanel::Material;
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

pub fn create_material_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u64 = editor_state.selected_scene_id.unwrap();

    let (material_id, ..) = editor_state.get_object_ids();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if material_id.is_none() { return; }
    let material_id = material_id.unwrap();

    if let Some(material) = scene.get_material_by_id(material_id)
    {
        collapse_with_title(ui, "material_settings", true, "🎨 Material Settings", |ui|
        {
            let mut material = material.write().unwrap();
            material.ui(ui);
        });

        // delete material
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Material").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_material_by_id(material_id);
            }
        });

        {
            component_downcast!(material, Material);

            for texture_type in ALL_TEXTURE_TYPES
            {
                if material.has_texture(texture_type)
                {
                    let texture = material.get_texture_by_type(texture_type);
                    let texture = texture.unwrap();
                    let mut texture = texture.write().unwrap();

                    let title = format!("🖼 {}", texture_type.to_string());
                    let id = format!("texture_{}", texture_type.to_string());

                    collapse_with_title(ui, id.as_str(), true, title.as_str(), |ui|
                    {
                        texture.ui_info(ui);
                    });
                }
            }
        }
    }
}
use std::{collections::HashMap, sync::{RwLock, Arc}};

use egui::{Ui, RichText, Color32};
use rfd::FileDialog;

use crate::{state::{scene::components::material::{MaterialItem, ALL_TEXTURE_TYPES, Material, TextureType}, state::State, gui::helper::generic_items::{collapse_with_title, self}}, component_downcast_mut, resources::resources::load_binary, helper::concurrency::{thread::spawn_thread, execution_queue::ExecutionQueue}};

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
            component_downcast_mut!(material, Material);

            for texture_type in ALL_TEXTURE_TYPES
            {
                if material.has_texture(texture_type)
                {
                    let texture = material.get_texture_by_type(texture_type);
                    let texture = texture.unwrap();
                    let mut enabled = texture.enabled;
                    let texture = texture.get();
                    let mut texture = texture.write().unwrap();

                    let title = format!("🖼 {}", texture_type.to_string());
                    let id = format!("texture_{}", texture_type.to_string());

                    let mut remove_texture = false;
                    let mut changed = false;

                    generic_items::collapse(ui, id, true, |ui|
                    {
                        ui.label(RichText::new(title).heading().strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                        {
                            if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                            {
                                remove_texture = true;
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


                            if ui.toggle_value(&mut enabled, toggle_text).clicked()
                            {
                                changed = true;
                            }
                        });
                    },
                    |ui|
                    {
                        texture.ui_info(ui);
                    });

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
                    let id = format!("texture_{}", texture_type.to_string());

                    generic_items::collapse(ui, id, true, |ui|
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
                        });
                    },
                    |ui|
                    {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
                        {
                            if ui.button(RichText::new("Load Texture").heading().strong()).clicked()
                            {
                                let main_queue = state.main_thread_execution_queue.clone();

                                spawn_thread(move ||
                                {
                                    load_texture(main_queue.clone(), texture_type, scene_id, material_id);
                                });
                            }
                        });
                    });
                }
            }
        }
    }
}

pub fn load_texture(main_queue: Arc<RwLock<ExecutionQueue>>, texture_type: TextureType, scene_id: u64, material_id: u64)
{
    if let Some(path) = FileDialog::new().add_filter("Image", &["jpg", "png"]).set_directory("/").pick_file()
    {
        let name: Option<&std::ffi::OsStr> = path.file_stem().clone();
        let extension = path.extension().clone();

        if name.is_none() ||  name.unwrap().to_str().is_none()
        {
            return;
        }

        if extension.is_none() ||  extension.unwrap().to_str().is_none()
        {
            return;
        }

        let name = name.unwrap().to_str().unwrap().to_string().clone();
        let extension = extension.unwrap().to_str().unwrap().to_string().clone();

        let path = &path.display().to_string();
        let bytes = load_binary(path).unwrap();

        let mut main_queue = main_queue.write().unwrap();
        main_queue.add(Box::new(move |state|
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id)
            {
                if let Some(material) = scene.get_material_by_id(material_id)
                {
                    let tex = scene.load_texture_byte_or_reuse(&bytes, name.as_str(), Some(extension.clone()));

                    component_downcast_mut!(material, Material);
                    material.set_texture(tex, texture_type);
                }
            }
        }));
    }
}